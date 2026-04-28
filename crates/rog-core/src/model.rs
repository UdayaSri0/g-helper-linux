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
}
