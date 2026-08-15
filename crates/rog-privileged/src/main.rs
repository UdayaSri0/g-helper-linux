use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use rog_core::{
    require_authorized, validate_polkit_action, CpuControlRequest, CpuPowerMode, FanCurve,
    FanCurvePolicy, FanDomain, FanPoint, LightingApplyRequest, LightingDirection, LightingMode,
    LightingSpeed, PrivilegedCapabilities, PrivilegedError, PrivilegedErrorCode, RgbColor,
    RogError, POLKIT_ACTION_BATTERY_CONTROL, POLKIT_ACTION_CPU_CONTROL, POLKIT_ACTION_FANS_CONTROL,
    POLKIT_ACTION_LIGHTING_CONTROL, PRIVILEGED_DBUS_INTERFACE, PRIVILEGED_DBUS_NAME,
    PRIVILEGED_DBUS_PATH,
};
use rog_providers::aura_hid::{
    encode_g615jm_effect, match_g615jm, parse_output_report_sizes, scan_native_aura_hid,
    AuraHidIdentity, AURA_OUTPUT_REPORT_BYTES, G615JM_AURA_USB_INTERFACE,
    G615JM_REPORT_DESCRIPTOR_SHA256,
};
use rog_providers::cpu::CpuTelemetryProvider;
use rog_providers::hwmon::{HwmonTelemetryProvider, ASUS_WMI_CURVE_POINTS};
use rog_providers::kbd_backlight::KbdBacklightSysfs;
use rog_providers::power_supply::{BatteryChargeLimitControl, PowerSupplySysfsProvider};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::interval;
use tracing::{info, warn};
use zbus::message::Header;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{connection, fdo, interface, Connection, Proxy};

const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const POLKIT_NAME: &str = "org.freedesktop.PolicyKit1";
const POLKIT_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
const POLKIT_INTERFACE: &str = "org.freedesktop.PolicyKit1.Authority";
const FAN_SAFETY_MARKER: &str = "/run/rog-helper/fan-control-active";
const AURA_DEVICE_ALIAS: &str = "/dev/rog-helper-aura";
const AURA_MIN_WRITE_INTERVAL: Duration = Duration::from_millis(250);
const AURA_REPORT_TIMEOUT: Duration = Duration::from_millis(250);
const AURA_OPERATION_TIMEOUT: Duration = Duration::from_secs(1);
const DBUS_DAEMON_NAME: &str = "org.freedesktop.DBus";
const DBUS_DAEMON_PATH: &str = "/org/freedesktop/DBus";
const DBUS_DAEMON_INTERFACE: &str = "org.freedesktop.DBus";
const AURA_OWNER_NAMES: &[&str] = &["xyz.ljones.Asusd", "org.asuslinux.Daemon"];

#[derive(Debug, Default)]
struct NativeAuraControlState {
    last_request: Option<LightingApplyRequest>,
    last_device_generation: Option<(u64, u64)>,
    last_attempt: Option<Instant>,
}

#[derive(Debug, Clone)]
struct PrivilegedService {
    last_activity: Arc<Mutex<Instant>>,
    cpu: CpuTelemetryProvider,
    fans: HwmonTelemetryProvider,
    keyboard_backlight: Option<KbdBacklightSysfs>,
    battery_charge_limit: Option<BatteryChargeLimitControl>,
    fan_safety_marker: PathBuf,
    native_aura: Arc<AsyncMutex<NativeAuraControlState>>,
}

impl PrivilegedService {
    fn new() -> Self {
        Self {
            last_activity: Arc::new(Mutex::new(Instant::now())),
            cpu: CpuTelemetryProvider::default(),
            fans: HwmonTelemetryProvider::default(),
            keyboard_backlight: KbdBacklightSysfs::probe_approved_asus().ok().flatten(),
            battery_charge_limit: PowerSupplySysfsProvider::default()
                .charge_limit_control()
                .ok()
                .flatten(),
            fan_safety_marker: PathBuf::from(FAN_SAFETY_MARKER),
            native_aura: Arc::new(AsyncMutex::new(NativeAuraControlState::default())),
        }
    }

    fn touch(&self) {
        *self.last_activity.lock().expect("activity mutex poisoned") = Instant::now();
    }

    fn idle_for(&self) -> Duration {
        self.last_activity
            .lock()
            .expect("activity mutex poisoned")
            .elapsed()
    }
}

#[interface(name = "io.github.roghelper.Privileged1")]
impl PrivilegedService {
    fn ping(&self) -> bool {
        self.touch();
        true
    }

    fn get_version(&self) -> &str {
        self.touch();
        env!("CARGO_PKG_VERSION")
    }

    #[zbus(out_args("api_version", "categories"))]
    fn get_capabilities(&self) -> (u32, Vec<String>) {
        self.touch();
        PrivilegedCapabilities::implemented_controls().encode()
    }

    /// Non-interactive diagnostic probe. Real control methods added later must use
    /// the same caller-derived subject with the interactive PolicyKit flag.
    async fn can_perform(
        &self,
        action: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<bool> {
        self.touch();
        validate_polkit_action(action).map_err(|error| {
            fdo::Error::InvalidArgs(format!("{}: {}", error.code.as_str(), error.message))
        })?;
        let sender = header.sender().ok_or_else(|| {
            fdo::Error::Failed("unexpected: D-Bus caller identity is unavailable".to_string())
        })?;
        check_polkit_authorization(connection, sender.as_str(), action, false).await
    }

    async fn set_cpu_turbo(
        &self,
        enabled: bool,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        self.apply_cpu(connection, &header, CpuControlRequest::Turbo(enabled))
            .await
    }

    async fn set_cpu_power_mode(
        &self,
        mode: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        let mode = CpuPowerMode::parse(mode).map_err(map_cpu_error)?;
        self.apply_cpu(connection, &header, CpuControlRequest::PowerMode(mode))
            .await
    }

    async fn set_cpu_governor(
        &self,
        governor: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        self.apply_cpu(
            connection,
            &header,
            CpuControlRequest::Governor(governor.to_string()),
        )
        .await
    }

    async fn set_cpu_epp(
        &self,
        epp: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        self.apply_cpu(connection, &header, CpuControlRequest::Epp(epp.to_string()))
            .await
    }

    async fn set_cpu_frequency_limits(
        &self,
        min_mhz: u64,
        max_mhz: u64,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        let request = CpuControlRequest::FrequencyLimits {
            min_mhz: optional_u32(min_mhz, "min_mhz")?,
            max_mhz: optional_u32(max_mhz, "max_mhz")?,
        };
        self.apply_cpu(connection, &header, request).await
    }

    async fn set_cpu_core_online(
        &self,
        core_id: u64,
        online: bool,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        let core_id = u32::try_from(core_id).map_err(|_| {
            map_privileged_error(PrivilegedError::new(
                PrivilegedErrorCode::InvalidInput,
                "logical CPU id does not fit into u32",
            ))
        })?;
        self.apply_cpu(
            connection,
            &header,
            CpuControlRequest::CoreOnline { core_id, online },
        )
        .await
    }

    async fn set_fan_auto(
        &self,
        fan_id: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        let target = (!fan_id.trim().is_empty()).then_some(fan_id);
        if let Some(fan_id) = target {
            fan_domain(fan_id)?;
        }
        let safe_fans = self
            .fans
            .list_fans()
            .map_err(map_fan_error)?
            .into_iter()
            .filter(|fan| fan.supports_auto && fan.pwm_endpoint_verified)
            .collect::<Vec<_>>();
        if safe_fans.is_empty()
            || target.is_some_and(|fan_id| !safe_fans.iter().any(|fan| fan.id == fan_id))
        {
            return Err(map_fan_error(RogError::NotSupported(
                "fan mapping is not safe for Auto control".to_string(),
            )));
        }
        self.authorize_fans(connection, &header).await?;
        self.fans.set_fan_auto(target).map_err(map_fan_error)?;
        // The marker covers all fan channels. A targeted reset cannot prove
        // that another channel is no longer using a custom curve, so only an
        // all-channel reset may disarm crash recovery.
        if target.is_none() {
            self.clear_fan_safety_marker().map_err(map_fan_error)?;
        }
        Ok(())
    }

    async fn set_fan_curve(
        &self,
        fan_id: &str,
        points: Vec<(u8, u8)>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        let curve = FanCurve {
            domain: fan_domain(fan_id)?,
            points: points
                .into_iter()
                .map(|(temp_c, duty_percent)| FanPoint {
                    temp_c,
                    duty_percent,
                })
                .collect(),
        };
        curve
            .validate_safe(FanCurvePolicy {
                exact_point_count: Some(ASUS_WMI_CURVE_POINTS),
                ..Default::default()
            })
            .map_err(map_fan_error)?;
        if !self
            .fans
            .list_fans()
            .map_err(map_fan_error)?
            .iter()
            .any(|fan| fan.id == fan_id && fan.supports_curve && fan.pwm_endpoint_verified)
        {
            return Err(map_fan_error(RogError::NotSupported(
                "fan mapping is not safe for curve control".to_string(),
            )));
        }
        self.authorize_fans(connection, &header).await?;
        self.mark_fan_control_active().map_err(map_fan_error)?;
        if let Err(error) = self.fans.set_fan_curve(fan_id, curve) {
            if self.fans.set_fan_auto(None).is_ok() {
                let _ = self.clear_fan_safety_marker();
            }
            return Err(map_fan_error(error));
        }
        Ok(())
    }

    async fn reset_fans_to_auto(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        if !self
            .fans
            .list_fans()
            .map_err(map_fan_error)?
            .iter()
            .any(|fan| fan.supports_auto && fan.pwm_endpoint_verified)
        {
            return Err(map_fan_error(RogError::NotSupported(
                "no verified fan Auto interface is available".to_string(),
            )));
        }
        self.authorize_fans(connection, &header).await?;
        self.fans.set_fan_auto(None).map_err(map_fan_error)?;
        self.clear_fan_safety_marker().map_err(map_fan_error)
    }

    async fn set_keyboard_backlight_brightness(
        &self,
        level: u64,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        let level = u32::try_from(level).map_err(|_| {
            map_privileged_error(PrivilegedError::new(
                PrivilegedErrorCode::InvalidInput,
                "keyboard brightness does not fit into u32",
            ))
        })?;
        let keyboard = self.keyboard_backlight.as_ref().ok_or_else(|| {
            map_lighting_error(RogError::NotSupported(
                "approved ASUS keyboard backlight endpoint was not found".to_string(),
            ))
        })?;
        keyboard
            .validate_brightness(level)
            .map_err(map_lighting_error)?;
        self.authorize_lighting(connection, &header).await?;
        keyboard
            .set_approved_brightness(level)
            .map_err(map_lighting_error)
    }

    /// Apply one validated factory Aura effect to the single verified target.
    ///
    /// No device path, report ID, command ID, zone, or packet bytes cross D-Bus.
    // The five scalar effect fields are intentionally separate in the stable
    // D-Bus ABI; the remaining parameters are zbus-injected call context.
    #[allow(clippy::too_many_arguments)]
    async fn set_aura_effect(
        &self,
        mode: &str,
        primary_rgb_hex: &str,
        secondary_rgb_hex: &str,
        speed: &str,
        direction: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<bool> {
        let request =
            parse_native_aura_request(mode, primary_rgb_hex, secondary_rgb_hex, speed, direction)
                .map_err(map_lighting_error)?;
        let reports = encode_g615jm_effect(&request).map_err(map_lighting_error)?;

        reject_if_asusd_owned(connection).await?;
        require_single_supported_aura_device().map_err(map_lighting_error)?;
        self.authorize_lighting(connection, &header).await?;

        let mut state = self.native_aura.lock().await;
        reject_if_asusd_owned(connection).await?;
        let (file, generation) = open_and_validate_aura_device().map_err(map_lighting_error)?;

        if state.last_request.as_ref() == Some(&request)
            && state.last_device_generation == Some(generation)
        {
            return Ok(false);
        }
        if state
            .last_attempt
            .is_some_and(|attempt| attempt.elapsed() < AURA_MIN_WRITE_INTERVAL)
        {
            return Err(map_lighting_error(RogError::TemporarilyUnavailable(
                "Aura lighting requests are limited to one distinct update every 250 ms"
                    .to_string(),
            )));
        }
        state.last_attempt = Some(Instant::now());

        let operation_started = Instant::now();
        for report in reports.ordered() {
            reject_if_asusd_owned(connection).await?;
            write_aura_report(&file, report, operation_started).map_err(map_lighting_error)?;
        }
        state.last_request = Some(request);
        state.last_device_generation = Some(generation);
        Ok(true)
    }

    async fn set_battery_charge_limit(
        &self,
        limit: u64,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<u64> {
        let limit = u8::try_from(limit).map_err(|_| {
            map_privileged_error(PrivilegedError::new(
                PrivilegedErrorCode::InvalidInput,
                "battery charge limit does not fit into u8",
            ))
        })?;
        let limit = rog_core::BatteryLimitPercent(limit)
            .validate_control()
            .map_err(map_battery_error)?;
        let control = self.battery_charge_limit.as_ref().ok_or_else(|| {
            map_battery_error(RogError::NotSupported(
                "standard battery charge-limit endpoint was not found".to_string(),
            ))
        })?;
        control.read_limit().map_err(map_battery_error)?;
        self.authorize_battery(connection, &header).await?;
        control
            .set_limit(limit)
            .map(|actual| u64::from(actual.0))
            .map_err(map_battery_error)
    }
}

impl PrivilegedService {
    async fn authorize_battery(
        &self,
        connection: &Connection,
        header: &Header<'_>,
    ) -> fdo::Result<()> {
        self.touch();
        let sender = header.sender().ok_or_else(|| {
            map_privileged_error(PrivilegedError::new(
                PrivilegedErrorCode::Unexpected,
                "D-Bus caller identity is unavailable",
            ))
        })?;
        let authorized = check_polkit_authorization(
            connection,
            sender.as_str(),
            POLKIT_ACTION_BATTERY_CONTROL,
            true,
        )
        .await?;
        require_authorized(authorized).map_err(map_privileged_error)
    }

    async fn authorize_lighting(
        &self,
        connection: &Connection,
        header: &Header<'_>,
    ) -> fdo::Result<()> {
        self.touch();
        let sender = header.sender().ok_or_else(|| {
            map_privileged_error(PrivilegedError::new(
                PrivilegedErrorCode::Unexpected,
                "D-Bus caller identity is unavailable",
            ))
        })?;
        let authorized = check_polkit_authorization(
            connection,
            sender.as_str(),
            POLKIT_ACTION_LIGHTING_CONTROL,
            true,
        )
        .await?;
        require_authorized(authorized).map_err(map_privileged_error)
    }

    async fn authorize_fans(
        &self,
        connection: &Connection,
        header: &Header<'_>,
    ) -> fdo::Result<()> {
        self.touch();
        let sender = header.sender().ok_or_else(|| {
            map_privileged_error(PrivilegedError::new(
                PrivilegedErrorCode::Unexpected,
                "D-Bus caller identity is unavailable",
            ))
        })?;
        let authorized = check_polkit_authorization(
            connection,
            sender.as_str(),
            POLKIT_ACTION_FANS_CONTROL,
            true,
        )
        .await?;
        require_authorized(authorized).map_err(map_privileged_error)
    }

    fn mark_fan_control_active(&self) -> rog_core::RogResult<()> {
        write_fan_safety_marker(&self.fan_safety_marker)
    }

    fn clear_fan_safety_marker(&self) -> rog_core::RogResult<()> {
        match fs::remove_file(&self.fan_safety_marker) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(RogError::Unexpected(
                "could not clear the fan fail-safe marker".to_string(),
            )),
        }
    }

    fn restore_fans_if_armed(&self) -> rog_core::RogResult<()> {
        match fs::symlink_metadata(&self.fan_safety_marker) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            Ok(_) => {
                return Err(RogError::Unexpected(
                    "fan fail-safe marker has an unexpected file type".to_string(),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => {
                return Err(RogError::Unexpected(
                    "fan fail-safe marker metadata is unavailable".to_string(),
                ));
            }
        }
        self.fans.set_fan_auto(None)?;
        self.clear_fan_safety_marker()
    }

    async fn apply_cpu(
        &self,
        connection: &Connection,
        header: &Header<'_>,
        request: CpuControlRequest,
    ) -> fdo::Result<()> {
        self.touch();
        self.cpu.validate_control(&request).map_err(map_cpu_error)?;
        let sender = header.sender().ok_or_else(|| {
            map_privileged_error(PrivilegedError::new(
                PrivilegedErrorCode::Unexpected,
                "D-Bus caller identity is unavailable",
            ))
        })?;
        let authorized = check_polkit_authorization(
            connection,
            sender.as_str(),
            POLKIT_ACTION_CPU_CONTROL,
            true,
        )
        .await?;
        require_authorized(authorized).map_err(map_privileged_error)?;
        self.cpu.apply_control(&request).map_err(map_cpu_error)
    }
}

fn write_fan_safety_marker(path: &std::path::Path) -> rog_core::RogResult<()> {
    let parent = path.parent().ok_or_else(|| {
        RogError::Unexpected("fan fail-safe marker has no parent directory".to_string())
    })?;
    let parent_metadata = fs::symlink_metadata(parent).map_err(|_| {
        RogError::Unexpected("fan fail-safe runtime directory is unavailable".to_string())
    })?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.file_type().is_dir() {
        return Err(RogError::Unexpected(
            "fan fail-safe runtime directory has an unexpected file type".to_string(),
        ));
    }

    let mut options = OpenOptions::new();
    options.write(true).truncate(true);
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => {
            return Err(RogError::Unexpected(
                "fan fail-safe marker has an unexpected file type".to_string(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            options.create_new(true);
        }
        Err(_) => {
            return Err(RogError::Unexpected(
                "fan fail-safe marker metadata is unavailable".to_string(),
            ));
        }
    }
    let mut marker = options
        .open(path)
        .map_err(|_| RogError::Unexpected("could not arm the fan fail-safe marker".to_string()))?;
    marker
        .write_all(b"asus-wmi-curve\n")
        .map_err(|_| RogError::Unexpected("could not arm the fan fail-safe marker".to_string()))
}

fn fan_domain(fan_id: &str) -> fdo::Result<FanDomain> {
    match fan_id {
        "asus-wmi:cpu" => Ok(FanDomain::Cpu),
        "asus-wmi:gpu" => Ok(FanDomain::Gpu),
        "asus-wmi:mid" => Ok(FanDomain::Mid),
        _ => Err(fdo::Error::InvalidArgs(
            "invalid_input: unknown or unsafe fan id".to_string(),
        )),
    }
}

fn optional_u32(value: u64, field: &str) -> fdo::Result<Option<u32>> {
    if value == 0 {
        return Ok(None);
    }
    u32::try_from(value).map(Some).map_err(|_| {
        map_privileged_error(PrivilegedError::new(
            PrivilegedErrorCode::InvalidInput,
            format!("{field} does not fit into u32"),
        ))
    })
}

fn map_cpu_error(error: RogError) -> fdo::Error {
    let privileged = match error {
        RogError::NotSupported(_) => PrivilegedError::new(
            PrivilegedErrorCode::NotSupported,
            "the requested CPU control is not supported by this hardware",
        ),
        RogError::InvalidInput(message) => {
            PrivilegedError::new(PrivilegedErrorCode::InvalidInput, message)
        }
        RogError::TemporarilyUnavailable(_) => PrivilegedError::new(
            PrivilegedErrorCode::HardwareUnavailable,
            "the requested CPU control is temporarily unavailable",
        ),
        RogError::PermissionDenied(_) => PrivilegedError::new(
            PrivilegedErrorCode::PermissionDenied,
            "the privileged CPU endpoint could not be written",
        ),
        RogError::DependencyMissing(_) => PrivilegedError::new(
            PrivilegedErrorCode::BackendFailure,
            "a required CPU backend is unavailable",
        ),
        RogError::TransientFailure(_) | RogError::Unexpected(_) => PrivilegedError::new(
            PrivilegedErrorCode::BackendFailure,
            "the privileged CPU backend failed",
        ),
    };
    map_privileged_error(privileged)
}

fn map_fan_error(error: RogError) -> fdo::Error {
    let privileged = match error {
        RogError::NotSupported(_) => PrivilegedError::new(
            PrivilegedErrorCode::NotSupported,
            "the requested fan control is not safely supported by this hardware",
        ),
        RogError::InvalidInput(message) => {
            PrivilegedError::new(PrivilegedErrorCode::InvalidInput, message)
        }
        RogError::PermissionDenied(_) => PrivilegedError::new(
            PrivilegedErrorCode::PermissionDenied,
            "the verified fan endpoint could not be written",
        ),
        RogError::TemporarilyUnavailable(_) => PrivilegedError::new(
            PrivilegedErrorCode::HardwareUnavailable,
            "the verified fan hardware is temporarily unavailable",
        ),
        RogError::DependencyMissing(_) => PrivilegedError::new(
            PrivilegedErrorCode::BackendFailure,
            "the verified fan backend is unavailable",
        ),
        RogError::TransientFailure(_) => PrivilegedError::new(
            PrivilegedErrorCode::BackendFailure,
            "the privileged fan backend failed and Auto restore was attempted",
        ),
        RogError::Unexpected(_) => PrivilegedError::new(
            PrivilegedErrorCode::Unexpected,
            "the privileged fan write failed and Auto restore was attempted",
        ),
    };
    map_privileged_error(privileged)
}

fn parse_native_aura_request(
    mode: &str,
    primary_rgb_hex: &str,
    secondary_rgb_hex: &str,
    speed: &str,
    direction: &str,
) -> rog_core::RogResult<LightingApplyRequest> {
    let mode =
        match mode {
            "static" => LightingMode::Static,
            "breathe" => LightingMode::Breathe,
            "rainbow-cycle" => LightingMode::RainbowCycle,
            "rainbow-wave" => LightingMode::RainbowWave,
            "pulse" => LightingMode::Pulse,
            _ => return Err(RogError::InvalidInput(
                "native Aura mode must be static, breathe, rainbow-cycle, rainbow-wave, or pulse"
                    .to_string(),
            )),
        };
    let parse_colour = |value: &str, field: &str| -> rog_core::RogResult<Option<RgbColor>> {
        if value.is_empty() {
            return Ok(None);
        }
        if value.len() != 7 || !value.starts_with('#') {
            return Err(RogError::InvalidInput(format!(
                "{field} must be empty or use exact #RRGGBB form"
            )));
        }
        RgbColor::parse_hex(value).map(Some)
    };
    let speed = match speed {
        "" => None,
        "slow" => Some(LightingSpeed::Slow),
        "medium" => Some(LightingSpeed::Medium),
        "fast" => Some(LightingSpeed::Fast),
        _ => {
            return Err(RogError::InvalidInput(
                "native Aura speed must be empty, slow, medium, or fast".to_string(),
            ))
        }
    };
    let direction = match direction {
        "" => None,
        "right" => Some(LightingDirection::Right),
        "left" => Some(LightingDirection::Left),
        "up" => Some(LightingDirection::Up),
        "down" => Some(LightingDirection::Down),
        _ => {
            return Err(RogError::InvalidInput(
                "native Aura direction must be empty, right, left, up, or down".to_string(),
            ))
        }
    };
    let request = LightingApplyRequest {
        mode,
        primary_rgb: parse_colour(primary_rgb_hex, "primary_rgb_hex")?,
        secondary_rgb: parse_colour(secondary_rgb_hex, "secondary_rgb_hex")?,
        brightness: None,
        speed,
        direction,
        zone: None,
        apply_to_all: false,
    };
    request.validate_against(&rog_providers::aura_hid::g615jm_lighting_caps())?;
    Ok(request)
}

async fn reject_if_asusd_owned(connection: &Connection) -> fdo::Result<()> {
    let bus = Proxy::new(
        connection,
        DBUS_DAEMON_NAME,
        DBUS_DAEMON_PATH,
        DBUS_DAEMON_INTERFACE,
    )
    .await
    .map_err(|error| {
        map_lighting_error(RogError::TemporarilyUnavailable(format!(
            "could not check ASUS lighting ownership: {error}"
        )))
    })?;
    for name in AURA_OWNER_NAMES {
        let owned: bool = bus.call("NameHasOwner", &(*name,)).await.map_err(|error| {
            map_lighting_error(RogError::TemporarilyUnavailable(format!(
                "could not check whether {name} owns the Aura device: {error}"
            )))
        })?;
        if owned {
            return Err(map_lighting_error(RogError::TemporarilyUnavailable(
                format!(
                    "native Aura HID is suppressed because {name} is active and may own the device"
                ),
            )));
        }
    }
    Ok(())
}

fn require_single_supported_aura_device() -> rog_core::RogResult<()> {
    let scan = scan_native_aura_hid();
    let supported = scan
        .devices
        .iter()
        .filter(|device| device.protocol.is_some())
        .collect::<Vec<_>>();
    if supported.len() != 1 {
        return Err(RogError::NotSupported(format!(
            "native Aura requires exactly one verified G615JM HID interface; found {}",
            supported.len()
        )));
    }
    if supported[0].diagnostics.driver.as_deref() != Some("asus") {
        return Err(RogError::NotSupported(
            "the verified Aura HID interface is not bound to the expected ASUS HID driver"
                .to_string(),
        ));
    }
    Ok(())
}

#[repr(C)]
#[derive(Default)]
struct HidrawDevInfo {
    bustype: u32,
    vendor: i16,
    product: i16,
}

#[repr(C)]
struct HidrawReportDescriptor {
    size: u32,
    value: [u8; 4096],
}

impl Default for HidrawReportDescriptor {
    fn default() -> Self {
        Self {
            size: 0,
            value: [0; 4096],
        }
    }
}

const HIDIOCGRDESCSIZE: libc::c_ulong = 0x8004_4801;
const HIDIOCGRDESC: libc::c_ulong = 0x9004_4802;
const HIDIOCGRAWINFO: libc::c_ulong = 0x8008_4803;
const BUS_USB: u32 = 0x03;

fn open_and_validate_aura_device() -> rog_core::RogResult<(File, (u64, u64))> {
    let scan = scan_native_aura_hid();
    let supported = scan
        .devices
        .iter()
        .filter(|device| device.protocol.is_some())
        .collect::<Vec<_>>();
    if supported.len() != 1 {
        return Err(RogError::NotSupported(format!(
            "native Aura requires exactly one verified HID interface; found {}",
            supported.len()
        )));
    }
    let device = supported[0];
    if device.diagnostics.driver.as_deref() != Some("asus") {
        return Err(RogError::NotSupported(
            "Aura HID driver identity changed before write".to_string(),
        ));
    }
    let hidraw_name = &device.diagnostics.hidraw_name;
    let sysfs_entry = PathBuf::from("/sys/class/hidraw").join(hidraw_name);
    let dev_numbers = fs::read_to_string(sysfs_entry.join("dev")).map_err(|error| {
        RogError::TemporarilyUnavailable(format!("could not read Aura hidraw dev_t: {error}"))
    })?;
    let (major, minor) = dev_numbers.trim().split_once(':').ok_or_else(|| {
        RogError::Unexpected("Aura hidraw dev_t has an invalid format".to_string())
    })?;
    let major = major
        .parse::<u64>()
        .map_err(|error| RogError::Unexpected(format!("Aura hidraw major is invalid: {error}")))?;
    let minor = minor
        .parse::<u64>()
        .map_err(|error| RogError::Unexpected(format!("Aura hidraw minor is invalid: {error}")))?;
    let expected_rdev = libc::makedev(major as _, minor as _);
    let sysfs_inode = fs::metadata(sysfs_entry.join("device"))
        .map_err(|error| {
            RogError::TemporarilyUnavailable(format!(
                "could not stat the Aura HID sysfs device: {error}"
            ))
        })?
        .ino();

    let file = OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(AURA_DEVICE_ALIAS)
        .map_err(|error| {
            RogError::PermissionDenied(format!(
                "could not open the root-only Aura device alias {AURA_DEVICE_ALIAS}: {error}"
            ))
        })?;
    let metadata = file.metadata().map_err(|error| {
        RogError::TemporarilyUnavailable(format!("could not stat the opened Aura device: {error}"))
    })?;
    if metadata.rdev() != expected_rdev {
        return Err(RogError::NotSupported(
            "the Aura device alias does not reference the verified hidraw interface".to_string(),
        ));
    }

    revalidate_open_aura_fd(&file, scan.board_name.as_deref().unwrap_or(""))?;
    Ok((file, (metadata.rdev(), sysfs_inode)))
}

fn revalidate_open_aura_fd(file: &File, board_name: &str) -> rog_core::RogResult<()> {
    let fd = file.as_raw_fd();
    let mut info = HidrawDevInfo::default();
    let mut descriptor_size: libc::c_int = 0;
    // SAFETY: each ioctl is the Linux hidraw read-only query for the declared,
    // correctly sized output structure and the file descriptor remains open.
    let info_result = unsafe { libc::ioctl(fd, HIDIOCGRAWINFO, &mut info) };
    // SAFETY: see the safety note above; this writes one c_int.
    let size_result = unsafe { libc::ioctl(fd, HIDIOCGRDESCSIZE, &mut descriptor_size) };
    if info_result < 0 || size_result < 0 {
        return Err(RogError::NotSupported(
            "the opened Aura alias is not a queryable hidraw device".to_string(),
        ));
    }
    if info.bustype != BUS_USB || info.vendor as u16 != 0x0b05 || info.product as u16 != 0x19b6 {
        return Err(RogError::NotSupported(
            "the opened hidraw device identity does not match ASUS 0b05:19b6".to_string(),
        ));
    }
    if !(1..=4096).contains(&descriptor_size) {
        return Err(RogError::NotSupported(format!(
            "the opened hidraw descriptor size {descriptor_size} is invalid"
        )));
    }
    let mut descriptor = HidrawReportDescriptor {
        size: descriptor_size as u32,
        ..HidrawReportDescriptor::default()
    };
    // SAFETY: HIDIOCGRDESC fills at most descriptor.size bytes of the 4096-byte
    // inline buffer while the source file descriptor remains open.
    let descriptor_result = unsafe { libc::ioctl(fd, HIDIOCGRDESC, &mut descriptor) };
    if descriptor_result < 0 || descriptor.size as usize > descriptor.value.len() {
        return Err(RogError::NotSupported(
            "the opened Aura hidraw report descriptor could not be verified".to_string(),
        ));
    }
    let bytes = &descriptor.value[..descriptor.size as usize];
    let hash = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if hash != G615JM_REPORT_DESCRIPTOR_SHA256 {
        return Err(RogError::NotSupported(
            "the opened Aura hidraw descriptor hash is not allow-listed".to_string(),
        ));
    }
    let identity = AuraHidIdentity {
        vendor_id: info.vendor as u16,
        product_id: info.product as u16,
        interface_number: G615JM_AURA_USB_INTERFACE,
        board_name: board_name.to_string(),
        report_descriptor_sha256: hash,
        output_report_payload_bytes: parse_output_report_sizes(bytes).map_err(|error| {
            RogError::NotSupported(format!("invalid Aura HID report descriptor: {error}"))
        })?,
    };
    let matched = match_g615jm(&identity);
    if !matched.is_supported() {
        return Err(RogError::NotSupported(format!(
            "the opened Aura HID interface no longer matches: {}",
            matched.rejection_reasons.join("; ")
        )));
    }
    Ok(())
}

fn write_aura_report(
    file: &File,
    report: &[u8; AURA_OUTPUT_REPORT_BYTES],
    operation_started: Instant,
) -> rog_core::RogResult<()> {
    let operation_remaining = AURA_OPERATION_TIMEOUT
        .checked_sub(operation_started.elapsed())
        .ok_or_else(|| {
            RogError::TemporarilyUnavailable("Aura HID operation timed out".to_string())
        })?;
    let timeout = operation_remaining.min(AURA_REPORT_TIMEOUT);
    let timeout_ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
    let mut pollfd = libc::pollfd {
        fd: file.as_raw_fd(),
        events: libc::POLLOUT,
        revents: 0,
    };
    // SAFETY: pollfd references one initialized descriptor for the duration of
    // the call; the owned File keeps that descriptor alive.
    let poll_result = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };
    if poll_result <= 0 || pollfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
        return Err(RogError::TemporarilyUnavailable(
            "Aura HID device did not become writable before the deadline".to_string(),
        ));
    }
    // SAFETY: report is an initialized 64-byte buffer and the File owns a
    // writable hidraw descriptor. A single write is intentionally not retried.
    let written = unsafe {
        libc::write(
            file.as_raw_fd(),
            report.as_ptr().cast::<libc::c_void>(),
            report.len(),
        )
    };
    if written != report.len() as isize {
        return Err(RogError::TransientFailure(format!(
            "Aura HID report write returned {written}, expected {}",
            report.len()
        )));
    }
    Ok(())
}

fn map_lighting_error(error: RogError) -> fdo::Error {
    let privileged = match error {
        RogError::NotSupported(_) => PrivilegedError::new(
            PrivilegedErrorCode::NotSupported,
            "no approved ASUS lighting endpoint is available",
        ),
        RogError::InvalidInput(message) => {
            PrivilegedError::new(PrivilegedErrorCode::InvalidInput, message)
        }
        RogError::PermissionDenied(_) => PrivilegedError::new(
            PrivilegedErrorCode::PermissionDenied,
            "the approved ASUS lighting endpoint could not be written",
        ),
        RogError::TemporarilyUnavailable(_) => PrivilegedError::new(
            PrivilegedErrorCode::HardwareUnavailable,
            "the ASUS lighting hardware is temporarily unavailable",
        ),
        RogError::DependencyMissing(_) | RogError::TransientFailure(_) => PrivilegedError::new(
            PrivilegedErrorCode::BackendFailure,
            "the privileged ASUS lighting backend is unavailable",
        ),
        RogError::Unexpected(_) => PrivilegedError::new(
            PrivilegedErrorCode::Unexpected,
            "the ASUS lighting operation failed safely",
        ),
    };
    map_privileged_error(privileged)
}

fn map_battery_error(error: RogError) -> fdo::Error {
    let privileged = match error {
        RogError::NotSupported(_) => PrivilegedError::new(
            PrivilegedErrorCode::NotSupported,
            "no unambiguous standard battery charge-limit endpoint is available",
        ),
        RogError::InvalidInput(message) => {
            PrivilegedError::new(PrivilegedErrorCode::InvalidInput, message)
        }
        RogError::PermissionDenied(_) => PrivilegedError::new(
            PrivilegedErrorCode::PermissionDenied,
            "the validated battery charge-limit endpoint could not be written",
        ),
        RogError::TemporarilyUnavailable(_) => PrivilegedError::new(
            PrivilegedErrorCode::HardwareUnavailable,
            "the battery charge-limit endpoint is temporarily unavailable",
        ),
        RogError::DependencyMissing(_) | RogError::TransientFailure(_) => PrivilegedError::new(
            PrivilegedErrorCode::BackendFailure,
            "the privileged battery charge-limit backend failed",
        ),
        RogError::Unexpected(_) => PrivilegedError::new(
            PrivilegedErrorCode::Unexpected,
            "the battery charge-limit write or readback failed",
        ),
    };
    map_privileged_error(privileged)
}

fn map_privileged_error(error: PrivilegedError) -> fdo::Error {
    let message = format!("{}: {}", error.code.as_str(), error.message);
    match error.code {
        PrivilegedErrorCode::NotAuthorized | PrivilegedErrorCode::PermissionDenied => {
            fdo::Error::AccessDenied(message)
        }
        PrivilegedErrorCode::NotSupported => fdo::Error::NotSupported(message),
        PrivilegedErrorCode::InvalidInput => fdo::Error::InvalidArgs(message),
        _ => fdo::Error::Failed(message),
    }
}

async fn check_polkit_authorization(
    connection: &Connection,
    caller: &str,
    action: &str,
    allow_user_interaction: bool,
) -> fdo::Result<bool> {
    validate_polkit_action(action).map_err(|error| {
        fdo::Error::InvalidArgs(format!("{}: {}", error.code.as_str(), error.message))
    })?;

    let proxy = Proxy::new(connection, POLKIT_NAME, POLKIT_PATH, POLKIT_INTERFACE)
        .await
        .map_err(|_| fdo::Error::Failed("backend_failure: PolicyKit is unavailable".to_string()))?;
    let mut subject_details = HashMap::<String, OwnedValue>::new();
    subject_details.insert("name".to_string(), owned_value(caller.to_string()));
    let subject = ("system-bus-name", subject_details);
    let details = HashMap::<String, String>::new();
    let flags = if allow_user_interaction { 1_u32 } else { 0_u32 };
    let result: (bool, bool, HashMap<String, String>) = proxy
        .call("CheckAuthorization", &(subject, action, details, flags, ""))
        .await
        .map_err(|error| {
            let message = error.to_string().to_ascii_lowercase();
            if message.contains("cancel") {
                fdo::Error::AccessDenied("not_authorized: Authentication was cancelled".to_string())
            } else if message.contains("notauthorized") || message.contains("not authorized") {
                fdo::Error::AccessDenied(
                    "not_authorized: Administrator authorization was denied".to_string(),
                )
            } else {
                fdo::Error::Failed(
                    "backend_failure: PolicyKit authorization check failed".to_string(),
                )
            }
        })?;
    Ok(result.0)
}

fn owned_value<T>(value: T) -> OwnedValue
where
    T: Into<Value<'static>>,
{
    OwnedValue::try_from(value.into()).expect("OwnedValue conversion should succeed")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        // The root service intentionally ignores process-environment logging
        // overrides. Its packaged execution policy is fixed by systemd.
        .with_env_filter("info,rog_helper=debug")
        .with_target(false)
        .init();

    let service = PrivilegedService::new();
    if let Err(error) = service.restore_fans_if_armed() {
        // Keep the marker and bring the service online so recovery can be
        // retried if the hwmon backend was only temporarily absent.
        warn!("could not restore fan Auto state left by an interrupted helper: {error}");
    }
    let idle_service = service.clone();
    let connection = connection::Builder::system()
        .context("connect to the system D-Bus")?
        .name(PRIVILEGED_DBUS_NAME)
        .context("claim privileged D-Bus name")?
        .serve_at(PRIVILEGED_DBUS_PATH, service)
        .context("export privileged D-Bus object")?
        .build()
        .await
        .context("start privileged D-Bus service")?;

    info!(
        "privileged D-Bus service online: {PRIVILEGED_DBUS_NAME} {PRIVILEGED_DBUS_PATH} ({PRIVILEGED_DBUS_INTERFACE})"
    );
    let mut idle_check = interval(Duration::from_secs(15));
    loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                signal.context("listen for shutdown signal")?;
                break;
            }
            _ = idle_check.tick() => {
                if idle_service.idle_for() >= IDLE_TIMEOUT {
                    if let Err(error) = idle_service.restore_fans_if_armed() {
                        warn!("could not restore fans before idle exit; keeping helper online to retry: {error}");
                        idle_service.touch();
                        continue;
                    }
                    info!("privileged helper idle timeout reached; exiting");
                    break;
                }
            }
        }
    }
    if let Err(error) = idle_service.restore_fans_if_armed() {
        warn!("could not restore fans before privileged helper shutdown; recovery marker retained: {error}");
    }
    drop(connection);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rog_core::{
        require_authorized, PrivilegedErrorCode, POLKIT_ACTION_BATTERY_CONTROL,
        POLKIT_ACTION_CPU_CONTROL, POLKIT_ACTION_LIGHTING_CONTROL,
    };

    #[test]
    fn helper_advertises_only_implemented_write_categories() {
        let (_, categories) = PrivilegedCapabilities::implemented_controls().encode();
        assert_eq!(categories, vec!["cpu", "battery", "fans", "lighting"]);
    }

    #[test]
    fn authorization_failure_maps_to_not_authorized() {
        assert_eq!(
            require_authorized(false).unwrap_err().code,
            PrivilegedErrorCode::NotAuthorized
        );
    }

    #[test]
    fn intended_probe_actions_are_allow_listed() {
        assert!(validate_polkit_action(POLKIT_ACTION_CPU_CONTROL).is_ok());
        assert!(validate_polkit_action(POLKIT_ACTION_BATTERY_CONTROL).is_ok());
        assert!(validate_polkit_action(POLKIT_ACTION_FANS_CONTROL).is_ok());
        assert!(validate_polkit_action(POLKIT_ACTION_LIGHTING_CONTROL).is_ok());
        assert!(validate_polkit_action("io.github.roghelper.system.configure").is_err());
    }

    #[test]
    fn native_aura_request_accepts_only_high_level_verified_fields() {
        let request = parse_native_aura_request("breathe", "#FF0000", "#0000FF", "medium", "")
            .expect("verified request");
        assert_eq!(request.mode, LightingMode::Breathe);
        assert_eq!(request.primary_rgb, Some(RgbColor::new(255, 0, 0)));
        assert_eq!(request.secondary_rgb, Some(RgbColor::new(0, 0, 255)));

        assert!(parse_native_aura_request("strobe", "#FF0000", "", "", "").is_err());
        assert!(parse_native_aura_request("static", "FF0000", "", "", "").is_err());
        assert!(parse_native_aura_request("static", "#FF0000", "", "fast", "").is_err());
        assert!(parse_native_aura_request("rainbow-cycle", "", "#0000FF", "medium", "").is_err());
        assert!(parse_native_aura_request("rainbow-wave", "", "", "medium", "clockwise").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn fan_safety_marker_rejects_a_symlink() {
        use std::os::unix::fs::symlink;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rog-helper-marker-test-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("fixture root");
        let outside = root.join("outside");
        fs::write(&outside, "unchanged\n").expect("outside file");
        let marker = root.join("fan-control-active");
        symlink(&outside, &marker).expect("marker symlink");

        assert!(write_fan_safety_marker(&marker).is_err());
        assert_eq!(
            fs::read_to_string(&outside).expect("outside read"),
            "unchanged\n"
        );
        let _ = fs::remove_dir_all(root);
    }
}
