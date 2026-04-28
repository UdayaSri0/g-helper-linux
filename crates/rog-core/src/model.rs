use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{RogError, RogResult, ValidationWarning};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerSource {
    Ac,
    Battery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatteryState {
    Unknown,
    Charging,
    Discharging,
    Empty,
    Full,
    PendingCharge,
    PendingDischarge,
    NotCharging,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopProcessMem {
    pub user: String,
    pub pid: u32,
    pub rss_bytes: u64,
    pub swap_bytes: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanTelemetry {
    pub hwmon_device: String,
    pub hwmon_path: String,
    pub input_path: String,
    pub raw_label: Option<String>,
    pub display_label: String,
    pub rpm: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerformanceProfile {
    Silent,
    Balanced,
    Turbo,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuMode {
    Integrated,
    Hybrid,
    Dedicated,
    Other(String),
}

pub const DEFAULT_BATTERY_LIMIT_MIN_PERCENT: u8 = 40;
pub const DEFAULT_BATTERY_LIMIT_MAX_PERCENT: u8 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatteryLimitPercent(pub u8);

impl BatteryLimitPercent {
    pub fn clamp(value: u8, min: u8, max: u8) -> Self {
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        Self(value.clamp(min, max))
    }

    pub fn clamp_default(value: u8) -> Self {
        Self::clamp(
            value,
            DEFAULT_BATTERY_LIMIT_MIN_PERCENT,
            DEFAULT_BATTERY_LIMIT_MAX_PERCENT,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FanDomain {
    Cpu,
    Gpu,
    Mid,
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanPoint {
    pub temp_c: u8,
    pub duty_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanCurve {
    pub domain: FanDomain,
    pub points: Vec<FanPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeFloor {
    pub temp_c: u8,
    pub min_duty_percent: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanCurvePolicy {
    pub temp_min_c: u8,
    pub temp_max_c: u8,
    pub duty_min_percent: u8,
    pub duty_max_percent: u8,
    pub enforce_monotonic: bool,
    pub safe_floor: Option<SafeFloor>,
}

impl Default for FanCurvePolicy {
    fn default() -> Self {
        Self {
            temp_min_c: 20,
            temp_max_c: 100,
            duty_min_percent: 0,
            duty_max_percent: 100,
            enforce_monotonic: true,
            // Safety-first: avoid 0% fan at higher temps.
            safe_floor: Some(SafeFloor {
                temp_c: 70,
                min_duty_percent: 20,
            }),
        }
    }
}

impl FanCurve {
    /// Validate + sanitize in-place (clamps, sorts, monotonic fix, safe floor).
    ///
    /// Returns warnings describing any corrections.
    pub fn sanitize(&mut self, policy: FanCurvePolicy) -> RogResult<Vec<ValidationWarning>> {
        if self.points.is_empty() {
            return Err(RogError::InvalidInput(
                "fan curve must have at least 1 point".to_string(),
            ));
        }

        let mut warnings = Vec::new();

        // Clamp.
        for p in &mut self.points {
            let orig = *p;
            p.temp_c = p.temp_c.clamp(policy.temp_min_c, policy.temp_max_c);
            p.duty_percent = p
                .duty_percent
                .clamp(policy.duty_min_percent, policy.duty_max_percent);
            if *p != orig {
                warnings.push(ValidationWarning::new(
                    "clamp",
                    format!(
                        "Clamped point from ({}C, {}%) to ({}C, {}%).",
                        orig.temp_c, orig.duty_percent, p.temp_c, p.duty_percent
                    ),
                ));
            }
        }

        // Sort by temperature.
        let mut sorted = self.points.clone();
        sorted.sort_by_key(|p| p.temp_c);
        if sorted != self.points {
            self.points = sorted;
            warnings.push(ValidationWarning::new(
                "sort",
                "Sorted fan curve points by temperature.",
            ));
        }

        // Deduplicate identical temps (keep the highest duty).
        let mut deduped: Vec<FanPoint> = Vec::with_capacity(self.points.len());
        for p in self.points.drain(..) {
            if let Some(last) = deduped.last_mut() {
                if last.temp_c == p.temp_c {
                    if p.duty_percent > last.duty_percent {
                        last.duty_percent = p.duty_percent;
                    }
                    warnings.push(ValidationWarning::new(
                        "dedup",
                        format!("Merged duplicate temperature point at {}C.", p.temp_c),
                    ));
                    continue;
                }
            }
            deduped.push(p);
        }
        self.points = deduped;

        // Safe floor.
        if let Some(floor) = policy.safe_floor {
            for p in &mut self.points {
                if p.temp_c >= floor.temp_c && p.duty_percent < floor.min_duty_percent {
                    let old = p.duty_percent;
                    p.duty_percent = floor.min_duty_percent;
                    warnings.push(ValidationWarning::new(
                        "safe_floor",
                        format!(
                            "Raised duty at {}C from {}% to safe floor {}%.",
                            p.temp_c, old, floor.min_duty_percent
                        ),
                    ));
                }
            }
        }

        // Monotonic duty.
        if policy.enforce_monotonic && self.points.len() >= 2 {
            let mut last = self.points[0].duty_percent;
            for p in &mut self.points[1..] {
                if p.duty_percent < last {
                    let old = p.duty_percent;
                    p.duty_percent = last;
                    warnings.push(ValidationWarning::new(
                        "monotonic",
                        format!(
                            "Adjusted duty at {}C from {}% to {}% to keep monotonic curve.",
                            p.temp_c, old, p.duty_percent
                        ),
                    ));
                }
                last = p.duty_percent;
            }
        }

        Ok(warnings)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightingMode {
    Off,
    Static,
    Breathe,
    Rainbow,
    Strobe,
    Pulse,
    Wave,
    Other(String),
}

impl LightingMode {
    pub fn parse_label(value: &str) -> Option<Self> {
        let key = normalize_lighting_word(value);
        match key.as_str() {
            "off" | "none" => Some(Self::Off),
            "static" | "steady" | "constant" => Some(Self::Static),
            "breathe" | "breathing" | "breath" => Some(Self::Breathe),
            "strobe" | "strobing" | "flash" | "flashing" => Some(Self::Strobe),
            "rainbow" => Some(Self::Rainbow),
            "pulse" | "pulsing" => Some(Self::Pulse),
            "wave" | "colourwave" | "colorwave" => Some(Self::Wave),
            _ => None,
        }
    }

    pub fn from_backend_label(value: &str) -> Self {
        Self::parse_label(value).unwrap_or_else(|| Self::Other(value.trim().to_string()))
    }

    pub fn label(&self) -> String {
        match self {
            Self::Off => "Off".to_string(),
            Self::Static => "Static".to_string(),
            Self::Breathe => "Breathe".to_string(),
            Self::Rainbow => "Rainbow".to_string(),
            Self::Strobe => "Strobe".to_string(),
            Self::Pulse => "Pulse".to_string(),
            Self::Wave => "Wave".to_string(),
            Self::Other(value) => value.clone(),
        }
    }

    pub fn same_user_mode(&self, other: &Self) -> bool {
        normalize_lighting_word(&self.label()) == normalize_lighting_word(&other.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn parse_hex(value: &str) -> RogResult<Self> {
        let value = value.trim();
        let value = value.strip_prefix('#').unwrap_or(value);
        if value.len() != 6 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(RogError::InvalidInput(format!(
                "RGB colour '{value}' must be in #RRGGBB format"
            )));
        }

        let r = parse_hex_byte(&value[0..2])?;
        let g = parse_hex_byte(&value[2..4])?;
        let b = parse_hex_byte(&value[4..6])?;
        Ok(Self { r, g, b })
    }

    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightingState {
    pub backend: String,
    pub device: String,
    pub brightness: Option<u32>,
    pub max_brightness: Option<u32>,
    pub mode: Option<LightingMode>,
    pub supported_modes: Vec<LightingMode>,
    pub supports_rgb: bool,
    pub rgb: Option<RgbColor>,
    pub writable: bool,
    pub status: String,
    pub last_error: Option<String>,
}

impl LightingState {
    pub fn mode_label(&self) -> Option<String> {
        self.mode.as_ref().map(LightingMode::label)
    }

    pub fn supported_mode_labels(&self) -> Vec<String> {
        self.supported_modes
            .iter()
            .map(LightingMode::label)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightingDiagnostics {
    pub keyboard_backlight_detected: bool,
    pub keyboard_backlight_backend: Option<String>,
    pub keyboard_backlight_device: Option<String>,
    pub keyboard_backlight_path: Option<String>,
    pub keyboard_backlight_brightness_path: Option<String>,
    pub keyboard_backlight_current_brightness: Option<u32>,
    pub keyboard_backlight_max_brightness: Option<u32>,
    pub keyboard_backlight_readable: bool,
    pub keyboard_backlight_writable: bool,

    pub supports_brightness: bool,
    pub supports_modes: bool,
    pub supported_modes: Vec<String>,
    pub active_mode: Option<String>,

    pub supports_rgb: bool,
    pub rgb_backend_detected: bool,
    pub rgb_backend_name: Option<String>,
    pub rgb_current_hex: Option<String>,

    pub asusd_service_detected: bool,
    pub asusd_service_name: Option<String>,
    pub asusd_services_checked: Vec<String>,
    pub asusd_object_paths_checked: Vec<String>,
    pub asusd_interfaces_detected: Vec<String>,
    pub asusd_aura_interface_detected: bool,
    pub asusd_keyboard_interface_detected: bool,
    pub asusd_potential_aura_interfaces: Vec<String>,
    pub asusd_rgb_methods_detected: Vec<String>,
    pub asusd_rgb_properties_detected: Vec<String>,

    pub active_backend: String,
    pub fallback_reason: Option<String>,
    pub unavailable_reason: Option<String>,
    pub permission_warning: Option<String>,
    pub last_probe_error: Option<String>,
    pub probe_errors: Vec<String>,
    pub recommended_action: Option<String>,
}

impl LightingDiagnostics {
    pub fn unknown() -> Self {
        Self {
            keyboard_backlight_detected: false,
            keyboard_backlight_backend: None,
            keyboard_backlight_device: None,
            keyboard_backlight_path: None,
            keyboard_backlight_brightness_path: None,
            keyboard_backlight_current_brightness: None,
            keyboard_backlight_max_brightness: None,
            keyboard_backlight_readable: false,
            keyboard_backlight_writable: false,
            supports_brightness: false,
            supports_modes: false,
            supported_modes: Vec::new(),
            active_mode: None,
            supports_rgb: false,
            rgb_backend_detected: false,
            rgb_backend_name: None,
            rgb_current_hex: None,
            asusd_service_detected: false,
            asusd_service_name: None,
            asusd_services_checked: Vec::new(),
            asusd_object_paths_checked: Vec::new(),
            asusd_interfaces_detected: Vec::new(),
            asusd_aura_interface_detected: false,
            asusd_keyboard_interface_detected: false,
            asusd_potential_aura_interfaces: Vec::new(),
            asusd_rgb_methods_detected: Vec::new(),
            asusd_rgb_properties_detected: Vec::new(),
            active_backend: "none".to_string(),
            fallback_reason: None,
            unavailable_reason: None,
            permission_warning: None,
            last_probe_error: None,
            probe_errors: Vec::new(),
            recommended_action: None,
        }
    }

    pub fn summary_line(&self) -> String {
        if self.supports_rgb {
            format!(
                "RGB lighting is available through {}.",
                self.rgb_backend_name
                    .as_deref()
                    .unwrap_or(self.active_backend.as_str())
            )
        } else if self.supports_brightness {
            self.fallback_reason.clone().unwrap_or_else(|| {
                "Keyboard brightness is available, but RGB colour control is not available."
                    .to_string()
            })
        } else {
            self.unavailable_reason
                .clone()
                .unwrap_or_else(|| "No keyboard lighting backend is available.".to_string())
        }
    }

    pub fn to_report_text(&self) -> String {
        let mut lines = Vec::new();
        lines.push("Keyboard Lighting / RGB Diagnostics".to_string());
        lines.push("===================================".to_string());
        lines.push(String::new());
        lines.push("Summary:".to_string());
        lines.push(format!(
            "- Active backend: {}",
            display_backend_name(&self.active_backend)
        ));
        lines.push(format!(
            "- Keyboard backlight detected: {}",
            yes_no(self.keyboard_backlight_detected)
        ));
        lines.push(format!(
            "- Brightness control: {}",
            availability_text(self.supports_brightness, self.keyboard_backlight_writable)
        ));
        lines.push(format!(
            "- RGB control: {}",
            if self.supports_rgb {
                "available"
            } else {
                "not available"
            }
        ));
        lines.push(format!("- Reason: {}", self.summary_line()));
        lines.push(format!(
            "- Aura support: {}",
            if self.rgb_backend_detected {
                "detected"
            } else if self.asusd_potential_aura_interfaces.is_empty() {
                "not detected through asusd"
            } else {
                "potential interface detected"
            }
        ));
        lines.push(format!(
            "- asusd status: {}",
            if self.asusd_service_detected {
                format!(
                    "service detected ({})",
                    self.asusd_service_name
                        .as_deref()
                        .unwrap_or("name not recorded")
                )
            } else {
                "service not detected".to_string()
            }
        ));
        if let Some(action) = &self.recommended_action {
            lines.push(format!("- Recommended action: {action}"));
        }

        lines.push(String::new());
        lines.push("Sysfs Keyboard Backlight:".to_string());
        lines.push(format!(
            "- Device: {}",
            opt_text(self.keyboard_backlight_device.as_deref())
        ));
        lines.push(format!(
            "- Backend: {}",
            opt_text(self.keyboard_backlight_backend.as_deref())
        ));
        lines.push(format!(
            "- Path: {}",
            opt_text(self.keyboard_backlight_path.as_deref())
        ));
        lines.push(format!(
            "- Brightness path: {}",
            opt_text(self.keyboard_backlight_brightness_path.as_deref())
        ));
        lines.push(format!(
            "- Brightness: {}",
            opt_u32(self.keyboard_backlight_current_brightness)
        ));
        lines.push(format!(
            "- Max brightness: {}",
            opt_u32(self.keyboard_backlight_max_brightness)
        ));
        lines.push(format!(
            "- Readable: {}",
            yes_no(self.keyboard_backlight_readable)
        ));
        lines.push(format!(
            "- Writable: {}",
            yes_no(self.keyboard_backlight_writable)
        ));
        lines.push(format!(
            "- Supported modes: {}",
            list_text(&self.supported_modes)
        ));
        lines.push(format!("- Supports RGB: {}", yes_no(false)));

        lines.push(String::new());
        lines.push("ASUS/Aura DBus Probe:".to_string());
        lines.push(format!(
            "- Services checked: {}",
            list_text(&self.asusd_services_checked)
        ));
        lines.push(format!(
            "- Service detected: {}",
            if self.asusd_service_detected {
                opt_text(self.asusd_service_name.as_deref()).to_string()
            } else {
                "no".to_string()
            }
        ));
        lines.push(format!(
            "- Object paths checked: {}",
            list_text(&self.asusd_object_paths_checked)
        ));
        lines.push(format!(
            "- Interfaces detected: {}",
            list_text(&self.asusd_interfaces_detected)
        ));
        lines.push(format!(
            "- Aura-like interface found: {}",
            yes_no(
                self.asusd_aura_interface_detected
                    || !self.asusd_potential_aura_interfaces.is_empty()
            )
        ));
        lines.push(format!(
            "- Keyboard-like interface found: {}",
            yes_no(self.asusd_keyboard_interface_detected)
        ));
        lines.push(format!(
            "- Potential Aura interfaces: {}",
            list_text(&self.asusd_potential_aura_interfaces)
        ));
        lines.push(format!(
            "- RGB methods found: {}",
            list_text(&self.asusd_rgb_methods_detected)
        ));
        lines.push(format!(
            "- RGB properties found: {}",
            list_text(&self.asusd_rgb_properties_detected)
        ));
        lines.push(format!("- Probe errors: {}", list_text(&self.probe_errors)));

        lines.push(String::new());
        lines.push("Decision:".to_string());
        lines.push(format!(
            "- has_kbd_backlight: {}",
            yes_no(self.keyboard_backlight_detected)
        ));
        lines.push(format!("- has_aura: {}", yes_no(self.rgb_backend_detected)));
        lines.push(format!("- supports_rgb: {}", yes_no(self.supports_rgb)));
        lines.push(format!(
            "- selected backend: {}",
            display_backend_name(&self.active_backend)
        ));
        lines.push(format!(
            "- fallback reason: {}",
            opt_text(self.fallback_reason.as_deref())
        ));
        lines.push(format!(
            "- unavailable reason: {}",
            opt_text(self.unavailable_reason.as_deref())
        ));

        let mut warnings = Vec::new();
        if let Some(warning) = &self.permission_warning {
            warnings.push(warning.clone());
        }
        if !self.supports_rgb && self.supports_brightness {
            warnings.push(
                "Keyboard brightness is available through the sysfs LED backend, but RGB colour control is not available because no Aura/RGB backend was detected."
                    .to_string(),
            );
        }
        if self.asusd_service_detected && !self.rgb_backend_detected {
            warnings.push(
                "asusd is running, but no Aura/RGB keyboard interface was found in the detected DBus interfaces. This laptop or asusd version may not expose RGB control through DBus."
                    .to_string(),
            );
        }
        if !self.asusd_potential_aura_interfaces.is_empty() && !self.rgb_backend_detected {
            warnings.push(
                "Potential Aura/RGB interface detected, but rog-helper does not yet implement this interface. Please include the introspection output in a GitHub issue."
                    .to_string(),
            );
        }
        if let Some(error) = &self.last_probe_error {
            warnings.push(format!("Last probe error: {error}"));
        }
        if !self.probe_errors.is_empty() {
            warnings.extend(
                self.probe_errors
                    .iter()
                    .map(|e| format!("Probe error: {e}")),
            );
        }

        lines.push(String::new());
        lines.push("Warnings:".to_string());
        if warnings.is_empty() {
            lines.push("- none".to_string());
        } else {
            for warning in warnings {
                lines.push(format!("- {warning}"));
            }
        }

        lines.join("\n")
    }
}

fn display_backend_name(value: &str) -> String {
    match normalize_lighting_word(value).as_str() {
        "sysfsled" => "Sysfs LED keyboard backlight".to_string(),
        "asusdaura" => "ASUS Aura DBus".to_string(),
        "none" | "" => "None".to_string(),
        _ => value.to_string(),
    }
}

fn availability_text(supported: bool, writable: bool) -> &'static str {
    match (supported, writable) {
        (true, true) => "available",
        (true, false) => "read-only",
        (false, _) => "not available",
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn opt_text(value: Option<&str>) -> &str {
    value.filter(|v| !v.trim().is_empty()).unwrap_or("(none)")
}

fn opt_u32(value: Option<u32>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "(none)".to_string())
}

fn list_text(values: &[String]) -> String {
    if values.is_empty() {
        "(none)".to_string()
    } else {
        values.join(", ")
    }
}

fn normalize_lighting_word(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn parse_hex_byte(value: &str) -> RogResult<u8> {
    u8::from_str_radix(value, 16)
        .map_err(|e| RogError::InvalidInput(format!("invalid RGB hex byte '{value}': {e}")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCaps {
    pub has_profiles: bool,
    pub has_fan_curves: bool,
    pub has_fan_reading: bool,
    pub has_charge_limit: bool,
    pub has_gpu_modes: bool,
    pub has_aura: bool,
    pub has_kbd_backlight: bool,
    pub requires_reboot_for_gpu_switch: bool,
    pub profile_access: FeatureAvailability,
    pub charge_limit_access: FeatureAvailability,
    pub gpu_mode_access: FeatureAvailability,
    pub kbd_backlight_access: FeatureAvailability,

    /// Raw discovery info for diagnostics (bus names, interfaces, sysfs hints, etc.).
    pub endpoints: Vec<String>,
    pub notes: Vec<String>,
}

impl DeviceCaps {
    pub fn unknown() -> Self {
        Self {
            has_profiles: false,
            has_fan_curves: false,
            has_fan_reading: false,
            has_charge_limit: false,
            has_gpu_modes: false,
            has_aura: false,
            has_kbd_backlight: false,
            requires_reboot_for_gpu_switch: false,
            profile_access: FeatureAvailability::unknown(),
            charge_limit_access: FeatureAvailability::unknown(),
            gpu_mode_access: FeatureAvailability::unknown(),
            kbd_backlight_access: FeatureAvailability::unknown(),
            endpoints: Vec::new(),
            notes: vec!["Capabilities not probed yet.".to_string()],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeatureAccessState {
    Unknown,
    Available,
    Unsupported,
    MissingBackend,
    PermissionDenied,
    TemporarilyUnavailable,
}

impl FeatureAccessState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Available => "available",
            Self::Unsupported => "unsupported",
            Self::MissingBackend => "missing_backend",
            Self::PermissionDenied => "permission_denied",
            Self::TemporarilyUnavailable => "temporarily_unavailable",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "available" => Self::Available,
            "unsupported" => Self::Unsupported,
            "missing_backend" => Self::MissingBackend,
            "permission_denied" => Self::PermissionDenied,
            "temporarily_unavailable" => Self::TemporarilyUnavailable,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureAvailability {
    pub status: FeatureAccessState,
    pub reason: String,
}

impl FeatureAvailability {
    pub fn new(status: FeatureAccessState, reason: impl Into<String>) -> Self {
        Self {
            status,
            reason: reason.into(),
        }
    }

    pub fn unknown() -> Self {
        Self::new(
            FeatureAccessState::Unknown,
            "Capability support has not been probed yet.",
        )
    }

    pub fn is_available(&self) -> bool {
        self.status == FeatureAccessState::Available
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
    pub timestamp_ms: u64,

    pub cpu_temp_c: Option<f32>,
    pub gpu_temp_c: Option<f32>,
    pub temps_c: BTreeMap<String, f32>,

    pub fans_rpm: BTreeMap<String, u32>,
    pub fan_rows: Vec<FanTelemetry>,

    pub power_source: Option<PowerSource>,
    pub battery_percent: Option<f32>,
    pub ac_online: Option<bool>,

    pub battery_state: Option<BatteryState>,
    pub battery_health_percent: Option<f32>,
    pub battery_cycle_count: Option<u32>,
    pub battery_charge_power_w: Option<f32>,
    pub battery_discharge_power_w: Option<f32>,
    pub battery_time_to_empty_s: Option<u64>,
    pub battery_time_to_full_s: Option<u64>,

    // Memory (RAM + swap) telemetry. Values are best-effort and may be unavailable depending on
    // kernel/features/permissions.
    pub mem_total_bytes: Option<u64>,
    pub mem_used_bytes: Option<u64>,
    pub mem_used_percent: Option<f32>,
    pub mem_available_bytes: Option<u64>,
    pub mem_free_bytes: Option<u64>,
    pub mem_cached_bytes: Option<u64>,
    pub mem_buffers_bytes: Option<u64>,
    pub mem_shared_bytes: Option<u64>,
    pub mem_anon_bytes: Option<u64>,

    pub swap_total_bytes: Option<u64>,
    pub swap_used_bytes: Option<u64>,
    pub swap_free_bytes: Option<u64>,
    pub swap_in_pages_per_s: Option<f32>,
    pub swap_out_pages_per_s: Option<f32>,
    pub zram_total_bytes: Option<u64>,
    pub zram_used_bytes: Option<u64>,
    pub zswap_enabled: Option<bool>,

    pub psi_mem_some_avg10: Option<f32>,
    pub psi_mem_some_avg60: Option<f32>,
    pub psi_mem_some_avg300: Option<f32>,
    pub psi_mem_full_avg10: Option<f32>,
    pub psi_mem_full_avg60: Option<f32>,
    pub psi_mem_full_avg300: Option<f32>,

    pub mem_active_bytes: Option<u64>,
    pub mem_inactive_bytes: Option<u64>,
    pub mem_dirty_bytes: Option<u64>,
    pub mem_writeback_bytes: Option<u64>,
    pub mem_slab_bytes: Option<u64>,
    pub mem_sreclaimable_bytes: Option<u64>,
    pub mem_sunreclaim_bytes: Option<u64>,
    pub mem_pagetables_bytes: Option<u64>,
    pub mem_kernelstack_bytes: Option<u64>,
    pub mem_mapped_bytes: Option<u64>,

    pub mem_top_processes: Option<String>,
    pub mem_top_processes_rows: Option<Vec<TopProcessMem>>,
}

impl TelemetrySnapshot {
    pub fn empty_now(timestamp_ms: u64) -> Self {
        Self {
            timestamp_ms,
            cpu_temp_c: None,
            gpu_temp_c: None,
            temps_c: BTreeMap::new(),
            fans_rpm: BTreeMap::new(),
            fan_rows: Vec::new(),
            power_source: None,
            battery_percent: None,
            ac_online: None,
            battery_state: None,
            battery_health_percent: None,
            battery_cycle_count: None,
            battery_charge_power_w: None,
            battery_discharge_power_w: None,
            battery_time_to_empty_s: None,
            battery_time_to_full_s: None,

            mem_total_bytes: None,
            mem_used_bytes: None,
            mem_used_percent: None,
            mem_available_bytes: None,
            mem_free_bytes: None,
            mem_cached_bytes: None,
            mem_buffers_bytes: None,
            mem_shared_bytes: None,
            mem_anon_bytes: None,

            swap_total_bytes: None,
            swap_used_bytes: None,
            swap_free_bytes: None,
            swap_in_pages_per_s: None,
            swap_out_pages_per_s: None,
            zram_total_bytes: None,
            zram_used_bytes: None,
            zswap_enabled: None,

            psi_mem_some_avg10: None,
            psi_mem_some_avg60: None,
            psi_mem_some_avg300: None,
            psi_mem_full_avg10: None,
            psi_mem_full_avg60: None,
            psi_mem_full_avg300: None,

            mem_active_bytes: None,
            mem_inactive_bytes: None,
            mem_dirty_bytes: None,
            mem_writeback_bytes: None,
            mem_slab_bytes: None,
            mem_sreclaimable_bytes: None,
            mem_sunreclaim_bytes: None,
            mem_pagetables_bytes: None,
            mem_kernelstack_bytes: None,
            mem_mapped_bytes: None,

            mem_top_processes: None,
            mem_top_processes_rows: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuCoreTelemetry {
    pub logical_cpu_id: u32,
    pub physical_core_index: Option<u32>,
    pub policy_id: Option<u32>,
    pub thread_index: Option<u32>,
    pub thread_count: Option<u32>,
    pub usage_percent: Option<f32>,
    pub current_freq_mhz: Option<u32>,
    pub min_freq_mhz: Option<u32>,
    pub max_freq_mhz: Option<u32>,
    pub online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuTelemetry {
    pub timestamp_ms: u64,
    pub temp_c: Option<f32>,
    pub usage_percent: Option<f32>,
    pub avg_freq_mhz: Option<f32>,
    pub package_power_w: Option<f32>,
    pub status: Option<String>,
    pub turbo_boost_enabled: Option<bool>,
    pub governor: Option<String>,
    pub epp: Option<String>,
    pub min_freq_mhz: Option<u32>,
    pub max_freq_mhz: Option<u32>,
    pub per_core: Vec<CpuCoreTelemetry>,
}

impl CpuTelemetry {
    pub fn empty_now(timestamp_ms: u64) -> Self {
        Self {
            timestamp_ms,
            temp_c: None,
            usage_percent: None,
            avg_freq_mhz: None,
            package_power_w: None,
            status: None,
            turbo_boost_enabled: None,
            governor: None,
            epp: None,
            min_freq_mhz: None,
            max_freq_mhz: None,
            per_core: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpuAccessState {
    Unknown,
    Available,
    Unsupported,
    MissingBackend,
    PermissionDenied,
    TemporarilyUnavailable,
}

impl CpuAccessState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Available => "available",
            Self::Unsupported => "unsupported",
            Self::MissingBackend => "missing_backend",
            Self::PermissionDenied => "permission_denied",
            Self::TemporarilyUnavailable => "temporarily_unavailable",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "available" => Self::Available,
            "unsupported" => Self::Unsupported,
            "missing_backend" => Self::MissingBackend,
            "permission_denied" => Self::PermissionDenied,
            "temporarily_unavailable" => Self::TemporarilyUnavailable,
            _ => Self::Unknown,
        }
    }

    pub fn is_writable(self) -> bool {
        matches!(self, Self::Available)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpuControlKind {
    Boost,
    PowerMode,
    Governor,
    Epp,
    FreqLimits,
    CoreOnline,
}

impl CpuControlKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Boost => "boost",
            Self::PowerMode => "power_mode",
            Self::Governor => "governor",
            Self::Epp => "epp",
            Self::FreqLimits => "freq_limits",
            Self::CoreOnline => "core_online",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "boost" => Some(Self::Boost),
            "power_mode" => Some(Self::PowerMode),
            "governor" => Some(Self::Governor),
            "epp" => Some(Self::Epp),
            "freq_limits" => Some(Self::FreqLimits),
            "core_online" => Some(Self::CoreOnline),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Boost => "Turbo Boost",
            Self::PowerMode => "Power Mode",
            Self::Governor => "Governor",
            Self::Epp => "Energy Performance Preference",
            Self::FreqLimits => "Frequency Limits",
            Self::CoreOnline => "Core Online/Offline",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuPathAccess {
    pub path: String,
    pub readable: bool,
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuControlAccess {
    pub kind: CpuControlKind,
    pub status: CpuAccessState,
    pub reason: String,
    pub paths: Vec<CpuPathAccess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuCaps {
    pub has_cpufreq: bool,
    pub has_epp: bool,
    pub has_boost_toggle: bool,
    pub has_package_power: bool,
    pub policy_writable: bool,
    pub has_min_freq_limit: bool,
    pub has_max_freq_limit: bool,
    pub has_governor: bool,
    pub has_core_online: bool,
    pub scaling_driver: Option<String>,
    pub cpu_count: u32,
    pub thread_count: u32,
    pub governor_choices: Vec<String>,
    pub epp_choices: Vec<String>,
    pub sysfs_paths: Vec<String>,
    pub control_access: Vec<CpuControlAccess>,
}

impl CpuCaps {
    pub fn unknown() -> Self {
        Self {
            has_cpufreq: false,
            has_epp: false,
            has_boost_toggle: false,
            has_package_power: false,
            policy_writable: false,
            has_min_freq_limit: false,
            has_max_freq_limit: false,
            has_governor: false,
            has_core_online: false,
            scaling_driver: None,
            cpu_count: 0,
            thread_count: 0,
            governor_choices: Vec::new(),
            epp_choices: Vec::new(),
            sysfs_paths: Vec::new(),
            control_access: Vec::new(),
        }
    }

    pub fn control_access(&self, kind: CpuControlKind) -> Option<&CpuControlAccess> {
        self.control_access.iter().find(|entry| entry.kind == kind)
    }

    pub fn control_state(&self, kind: CpuControlKind) -> CpuAccessState {
        self.control_access(kind)
            .map(|entry| entry.status)
            .unwrap_or(CpuAccessState::Unknown)
    }

    pub fn control_writable(&self, kind: CpuControlKind) -> bool {
        self.control_state(kind).is_writable()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub caps: DeviceCaps,
    pub telemetry: TelemetrySnapshot,
    pub cpu_caps: CpuCaps,
    pub cpu: CpuTelemetry,
    pub warnings: Vec<String>,
}

impl AppState {
    pub fn new(caps: DeviceCaps, telemetry: TelemetrySnapshot) -> Self {
        Self {
            caps,
            cpu_caps: CpuCaps::unknown(),
            cpu: CpuTelemetry::empty_now(telemetry.timestamp_ms),
            telemetry,
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fan_curve_sanitize_sorts_and_enforces_monotonic() {
        let mut curve = FanCurve {
            domain: FanDomain::Cpu,
            points: vec![
                FanPoint {
                    temp_c: 80,
                    duty_percent: 10,
                },
                FanPoint {
                    temp_c: 60,
                    duty_percent: 30,
                },
                FanPoint {
                    temp_c: 90,
                    duty_percent: 20,
                },
            ],
        };

        let warnings = curve.sanitize(FanCurvePolicy::default()).unwrap();
        assert!(!warnings.is_empty());

        assert_eq!(curve.points[0].temp_c, 60);
        assert_eq!(curve.points[1].temp_c, 80);
        assert_eq!(curve.points[2].temp_c, 90);

        // Safe floor should raise 80C/90C to >=20%.
        assert!(curve.points[1].duty_percent >= 20);
        assert!(curve.points[2].duty_percent >= curve.points[1].duty_percent);
    }

    #[test]
    fn fan_curve_sanitize_clamps() {
        let mut curve = FanCurve {
            domain: FanDomain::Cpu,
            points: vec![FanPoint {
                temp_c: 5,
                duty_percent: 250,
            }],
        };
        let _warnings = curve.sanitize(FanCurvePolicy::default()).unwrap();
        assert_eq!(curve.points[0].temp_c, 20);
        assert_eq!(curve.points[0].duty_percent, 100);
    }

    #[test]
    fn battery_limit_clamps_default() {
        assert_eq!(BatteryLimitPercent::clamp_default(10).0, 40);
        assert_eq!(BatteryLimitPercent::clamp_default(80).0, 80);
        assert_eq!(BatteryLimitPercent::clamp_default(150).0, 100);
    }

    #[test]
    fn rgb_hex_parser_accepts_hash_and_plain_rrggbb() {
        assert_eq!(
            RgbColor::parse_hex("#1A2b3C").unwrap(),
            RgbColor::new(26, 43, 60)
        );
        assert_eq!(
            RgbColor::parse_hex("00ff7F").unwrap(),
            RgbColor::new(0, 255, 127)
        );
        assert_eq!(RgbColor::new(10, 11, 12).to_hex(), "#0A0B0C");
    }

    #[test]
    fn rgb_hex_parser_rejects_invalid_values() {
        for value in ["", "#123", "12345G", "#00112233", "red"] {
            assert!(
                RgbColor::parse_hex(value).is_err(),
                "{value} should be invalid"
            );
        }
    }

    #[test]
    fn lighting_mode_parser_normalizes_user_facing_aliases() {
        assert_eq!(LightingMode::parse_label("Off"), Some(LightingMode::Off));
        assert_eq!(
            LightingMode::parse_label("Breathing"),
            Some(LightingMode::Breathe)
        );
        assert_eq!(
            LightingMode::parse_label("colour wave"),
            Some(LightingMode::Wave)
        );
        assert_eq!(
            LightingMode::parse_label("flashing"),
            Some(LightingMode::Strobe)
        );
        assert_eq!(LightingMode::parse_label("not-a-mode"), None);
    }

    #[test]
    fn lighting_diagnostics_formats_sysfs_only_brightness_report() {
        let mut diagnostics = LightingDiagnostics::unknown();
        diagnostics.keyboard_backlight_detected = true;
        diagnostics.keyboard_backlight_backend = Some("sysfs-led".to_string());
        diagnostics.keyboard_backlight_device = Some("asus::kbd_backlight".to_string());
        diagnostics.keyboard_backlight_path =
            Some("/sys/class/leds/asus::kbd_backlight".to_string());
        diagnostics.keyboard_backlight_brightness_path =
            Some("/sys/class/leds/asus::kbd_backlight/brightness".to_string());
        diagnostics.keyboard_backlight_current_brightness = Some(0);
        diagnostics.keyboard_backlight_max_brightness = Some(3);
        diagnostics.keyboard_backlight_readable = true;
        diagnostics.keyboard_backlight_writable = true;
        diagnostics.supports_brightness = true;
        diagnostics.supports_modes = true;
        diagnostics.supported_modes = vec!["Off".to_string(), "Static".to_string()];
        diagnostics.active_mode = Some("Off".to_string());
        diagnostics.active_backend = "sysfs-led".to_string();
        diagnostics.fallback_reason = Some(
            "Keyboard brightness is available through the sysfs LED backend, but RGB colour control is not available because no Aura/RGB backend was detected."
                .to_string(),
        );

        let report = diagnostics.to_report_text();
        assert!(report.contains("Active backend: Sysfs LED keyboard backlight"));
        assert!(report.contains("RGB control: not available"));
        assert!(report.contains("Supported modes: Off, Static"));
        assert!(report.contains("asus::kbd_backlight"));
    }

    #[test]
    fn lighting_diagnostics_formats_read_only_sysfs_warning() {
        let mut diagnostics = LightingDiagnostics::unknown();
        diagnostics.keyboard_backlight_detected = true;
        diagnostics.supports_brightness = true;
        diagnostics.keyboard_backlight_readable = true;
        diagnostics.keyboard_backlight_writable = false;
        diagnostics.active_backend = "sysfs-led".to_string();
        diagnostics.permission_warning = Some(
            "Keyboard backlight brightness was detected, but the brightness file is not writable by the current user/session daemon."
                .to_string(),
        );

        let report = diagnostics.to_report_text();
        assert!(report.contains("Brightness control: read-only"));
        assert!(report.contains("brightness file is not writable"));
    }

    #[test]
    fn lighting_diagnostics_explains_asusd_without_aura() {
        let mut diagnostics = LightingDiagnostics::unknown();
        diagnostics.asusd_service_detected = true;
        diagnostics.asusd_service_name = Some("xyz.ljones.Asusd".to_string());
        diagnostics.asusd_services_checked = vec!["xyz.ljones.Asusd".to_string()];
        diagnostics.asusd_interfaces_detected =
            vec!["xyz.ljones.Asusd:/xyz/ljones:xyz.ljones.Platform".to_string()];
        diagnostics.active_backend = "sysfs-led".to_string();
        diagnostics.supports_brightness = true;
        diagnostics.fallback_reason = Some(
            "Keyboard brightness is available through the sysfs LED backend, but asusd did not expose a supported Aura/RGB keyboard interface."
                .to_string(),
        );

        let report = diagnostics.to_report_text();
        assert!(report.contains("asusd status: service detected (xyz.ljones.Asusd)"));
        assert!(report.contains("no Aura/RGB keyboard interface was found"));
    }

    #[test]
    fn lighting_diagnostics_explains_potential_unimplemented_aura_interface() {
        let mut diagnostics = LightingDiagnostics::unknown();
        diagnostics.asusd_service_detected = true;
        diagnostics.asusd_service_name = Some("xyz.ljones.Asusd".to_string());
        diagnostics.asusd_potential_aura_interfaces =
            vec!["xyz.ljones.Asusd:/xyz/ljones:xyz.ljones.Aura".to_string()];
        diagnostics.asusd_aura_interface_detected = true;
        diagnostics.asusd_rgb_methods_detected =
            vec!["xyz.ljones.Asusd:/xyz/ljones:xyz.ljones.Aura.SetLedMode".to_string()];
        diagnostics.active_backend = "none".to_string();

        let report = diagnostics.to_report_text();
        assert!(report.contains("Potential Aura interfaces"));
        assert!(report.contains("does not yet implement this interface"));
    }
}
