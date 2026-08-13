use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use regex::Regex;
use rog_core::{
    config_path, config_to_toml, cpu_request_readback_matches, dbus_keys, legacy_ui_config_path,
    load_or_migrate, save_config_atomic, validate_config, AppConfig, AppState, BatteryLimitPercent,
    BatteryState, CpuAccessState, CpuAuthorization, CpuCaps, CpuControlAccess, CpuControlRequest,
    CpuPathAccess, CpuPowerMode, CpuTelemetry, DependencyStatus, DeviceCaps, FanCaps,
    FanControlMode, FanCurve, FanDomain, FanInfo, FanPoint, FanState, FanTelemetry,
    FeatureAccessState, FeatureAvailability, GpuMode, LightingDiagnostics, LightingMode,
    LightingState, PerformanceProfile, PermissionStatus, PowerSource, RgbColor, SetupIssue,
    SetupStatus, TelemetrySnapshot,
};
use rog_providers::asusd::AsusdPlatformProvider;
use rog_providers::aura::{AuraProbeDiagnostics, AuraProvider};
use rog_providers::cpu::CpuTelemetryProvider;
use rog_providers::hwmon::HwmonTelemetryProvider;
use rog_providers::kbd_backlight::KbdBacklightSysfs;
use rog_providers::lighting::build_lighting_diagnostics;
use rog_providers::memory::MemoryTelemetryProvider;
use rog_providers::nvidia_smi::NvidiaSmiTelemetryProvider;
use rog_providers::power_supply::PowerSupplySysfsProvider;
use rog_providers::setup::{probe_setup_status, RogHelperdProbe};
use rog_providers::supergfx::SupergfxProvider;
use rog_providers::traits::{BatteryProvider, GpuProvider, ProfileProvider};
use rog_providers::upower::UPowerProvider;
use tokio::time::{interval, Duration};
use tracing::{info, warn};
use zbus::fdo;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{connection, interface};

mod privileged_client;

const DBUS_NAME: &str = "io.github.roghelper.Daemon";
const DBUS_PATH: &str = "/io/github/roghelper/Daemon";
const DBUS_IFACE: &str = "io.github.roghelper.Daemon1";

#[derive(Debug)]
struct SharedState {
    inner: RwLock<AppState>,
    control: RwLock<ControlState>,
    config: RwLock<AppConfig>,
    config_path: Option<PathBuf>,
    cpu_privilege: RwLock<CpuPrivilegeState>,
    fan_authorization: RwLock<String>,
}

#[derive(Debug, Clone)]
struct CpuPrivilegeState {
    status: rog_core::PrivilegedStatus,
    authorization: CpuAuthorization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CpuWriteSource {
    Direct,
    Privileged,
}

async fn with_privileged_fallback<D, P, F>(
    direct: D,
    privileged: P,
) -> rog_core::RogResult<CpuWriteSource>
where
    D: FnOnce() -> rog_core::RogResult<()>,
    P: FnOnce() -> F,
    F: Future<Output = Result<(), rog_core::PrivilegedError>>,
{
    match direct() {
        Ok(()) => Ok(CpuWriteSource::Direct),
        Err(rog_core::RogError::PermissionDenied(_)) => {
            privileged().await.map_err(map_privileged_cpu_error)?;
            Ok(CpuWriteSource::Privileged)
        }
        Err(error) => Err(error),
    }
}

fn map_privileged_cpu_error(error: rog_core::PrivilegedError) -> rog_core::RogError {
    use rog_core::PrivilegedErrorCode as Code;
    match error.code {
        Code::NotAuthorized => rog_core::RogError::PermissionDenied(
            "administrator authorization was denied or cancelled".to_string(),
        ),
        Code::NotSupported => rog_core::RogError::NotSupported(error.message),
        Code::InvalidInput => rog_core::RogError::InvalidInput(error.message),
        Code::HardwareUnavailable => rog_core::RogError::TemporarilyUnavailable(error.message),
        Code::PermissionDenied => rog_core::RogError::PermissionDenied(error.message),
        Code::BackendFailure => rog_core::RogError::TemporarilyUnavailable(
            "the privileged CPU helper is unavailable".to_string(),
        ),
        Code::Unexpected => rog_core::RogError::Unexpected(
            "the privileged CPU helper failed unexpectedly".to_string(),
        ),
    }
}

fn map_privileged_fan_error(error: rog_core::PrivilegedError) -> rog_core::RogError {
    use rog_core::PrivilegedErrorCode as Code;
    match error.code {
        Code::NotAuthorized => rog_core::RogError::PermissionDenied(
            "administrator authorization for fan control was denied or cancelled".to_string(),
        ),
        Code::NotSupported => rog_core::RogError::NotSupported(error.message),
        Code::InvalidInput => rog_core::RogError::InvalidInput(error.message),
        Code::HardwareUnavailable => rog_core::RogError::TemporarilyUnavailable(error.message),
        Code::PermissionDenied => rog_core::RogError::PermissionDenied(error.message),
        Code::BackendFailure => rog_core::RogError::TemporarilyUnavailable(
            "the privileged fan helper is unavailable; fan telemetry remains available".to_string(),
        ),
        Code::Unexpected => rog_core::RogError::Unexpected(
            "the privileged fan helper failed; Auto restore was attempted".to_string(),
        ),
    }
}

async fn with_fan_privileged_fallback<D, P, F>(
    direct: D,
    privileged: P,
) -> rog_core::RogResult<CpuWriteSource>
where
    D: FnOnce() -> rog_core::RogResult<()>,
    P: FnOnce() -> F,
    F: Future<Output = Result<(), rog_core::PrivilegedError>>,
{
    match direct() {
        Ok(()) => Ok(CpuWriteSource::Direct),
        Err(rog_core::RogError::PermissionDenied(_)) => {
            privileged().await.map_err(map_privileged_fan_error)?;
            Ok(CpuWriteSource::Privileged)
        }
        Err(error) => Err(error),
    }
}

#[derive(Debug, Clone)]
struct ControlState {
    profile: Option<PerformanceProfile>,
    gpu_mode: Option<GpuMode>,
    battery_limit: Option<BatteryLimitPercent>,
    fan_sync_enabled: bool,
    fan_mode: FanControlMode,
    fan_last_action: Option<String>,
    fan_boost_until_ms: Option<u64>,
}

impl Default for ControlState {
    fn default() -> Self {
        Self {
            profile: None,
            gpu_mode: None,
            battery_limit: None,
            fan_sync_enabled: false,
            fan_mode: FanControlMode::Auto,
            fan_last_action: None,
            fan_boost_until_ms: None,
        }
    }
}

#[derive(Debug, Clone)]
struct RogHelperDaemon {
    state: Arc<SharedState>,
    kbd_backlight: Option<KbdBacklightSysfs>,
    kbd_backlight_detected: bool,
    kbd_backlight_probe_error: Option<String>,
    aura: Option<AuraProvider>,
    aura_probe_diagnostics: AuraProbeDiagnostics,
    asusd: Option<AsusdPlatformProvider>,
    supergfx: Option<SupergfxProvider>,
    cpu: CpuTelemetryProvider,
    hwmon: HwmonTelemetryProvider,
}

impl RogHelperDaemon {
    #[allow(clippy::too_many_arguments)]
    fn new(
        state: Arc<SharedState>,
        kbd_backlight: Option<KbdBacklightSysfs>,
        kbd_backlight_detected: bool,
        kbd_backlight_probe_error: Option<String>,
        aura: Option<AuraProvider>,
        aura_probe_diagnostics: AuraProbeDiagnostics,
        asusd: Option<AsusdPlatformProvider>,
        supergfx: Option<SupergfxProvider>,
        cpu: CpuTelemetryProvider,
        hwmon: HwmonTelemetryProvider,
    ) -> Self {
        Self {
            state,
            kbd_backlight,
            kbd_backlight_detected,
            kbd_backlight_probe_error,
            aura,
            aura_probe_diagnostics,
            asusd,
            supergfx,
            cpu,
            hwmon,
        }
    }

    fn read_state(&self) -> AppState {
        self.state.inner.read().expect("rwlock poisoned").clone()
    }

    fn set_telemetry(&self, telemetry: TelemetrySnapshot, warnings: Vec<String>) {
        let mut guard = self.state.inner.write().expect("rwlock poisoned");
        guard.telemetry = telemetry;
        guard.warnings = warnings;
    }

    fn set_cpu_telemetry(&self, cpu: CpuTelemetry) {
        let mut guard = self.state.inner.write().expect("rwlock poisoned");
        guard.cpu = cpu;
    }

    fn set_cpu_caps(&self, mut caps: CpuCaps) {
        let privilege = self
            .state
            .cpu_privilege
            .read()
            .expect("rwlock poisoned")
            .clone();
        apply_cpu_privilege_to_caps(&mut caps, &privilege);
        let mut guard = self.state.inner.write().expect("rwlock poisoned");
        guard.cpu_caps = caps;
    }

    fn set_fan_state(&self, mut fan_state: FanState) {
        let privilege = self
            .state
            .cpu_privilege
            .read()
            .expect("rwlock poisoned")
            .status
            .clone();
        let authorization = self
            .state
            .fan_authorization
            .read()
            .expect("rwlock poisoned")
            .clone();
        apply_fan_privilege_to_state(&mut fan_state, &privilege, &authorization);
        let control = self.read_control_state();
        fan_state.sync_enabled = control.fan_sync_enabled;
        fan_state.mode = control.fan_mode;
        fan_state.last_action = control.fan_last_action;
        fan_state.active_boost_until_ms = control.fan_boost_until_ms;
        let mut guard = self.state.inner.write().expect("rwlock poisoned");
        guard.fan_state = fan_state;
        guard.caps.has_fan_reading = guard.fan_state.caps.has_fan_reading;
        guard.caps.has_fan_curves = guard.fan_state.caps.has_fan_curves;
        guard.caps.has_fan_manual_percent = guard.fan_state.caps.has_fan_manual_percent;
        guard.caps.has_fan_manual_rpm_target = guard.fan_state.caps.has_fan_manual_rpm_target;
        guard.caps.has_individual_fan_control = guard.fan_state.caps.has_individual_fan_control;
        guard.caps.has_fan_sync_control = guard.fan_state.caps.has_fan_sync_control;
        guard.caps.has_fan_boost = guard.fan_state.caps.has_fan_boost;
        guard.caps.fan_count = guard.fan_state.caps.fan_count;
        guard.caps.fan_backend = guard.fan_state.caps.fan_backend.clone();
    }

    fn read_control_state(&self) -> ControlState {
        self.state.control.read().expect("rwlock poisoned").clone()
    }

    fn read_config(&self) -> AppConfig {
        self.state.config.read().expect("rwlock poisoned").clone()
    }

    fn persist_config(&self, mut config: AppConfig) -> Result<AppConfig, String> {
        config.version = rog_core::CONFIG_VERSION;
        validate_config(&config)?;
        let path = self.state.config_path.as_ref().ok_or_else(|| {
            "Configuration path is unavailable because HOME and XDG_CONFIG_HOME are not set."
                .to_string()
        })?;
        save_config_atomic(path, &config)?;
        *self.state.config.write().expect("rwlock poisoned") = config.clone();
        Ok(config)
    }

    fn set_control_state(
        &self,
        profile: Option<PerformanceProfile>,
        gpu_mode: Option<GpuMode>,
        battery_limit: Option<BatteryLimitPercent>,
    ) {
        let mut guard = self.state.control.write().expect("rwlock poisoned");
        guard.profile = profile;
        guard.gpu_mode = gpu_mode;
        guard.battery_limit = battery_limit;
    }

    fn update_fan_control_state(
        &self,
        mode: FanControlMode,
        last_action: impl Into<String>,
        boost_until_ms: Option<u64>,
    ) {
        let mut guard = self.state.control.write().expect("rwlock poisoned");
        guard.fan_mode = mode;
        guard.fan_last_action = Some(last_action.into());
        guard.fan_boost_until_ms = boost_until_ms;
    }

    fn refresh_cpu_state(&self) -> rog_core::RogResult<()> {
        let (timestamp_ms, temp_c) = {
            let state = self.state.inner.read().expect("rwlock poisoned");
            (state.telemetry.timestamp_ms, state.telemetry.cpu_temp_c)
        };
        let cpu = self.cpu.read_snapshot(timestamp_ms, temp_c)?;
        self.set_cpu_telemetry(cpu);
        Ok(())
    }

    async fn apply_cpu_control(&self, request: CpuControlRequest) -> rog_core::RogResult<()> {
        let result = with_privileged_fallback(
            || self.cpu.apply_control(&request),
            || privileged_client::apply_cpu(&request),
        )
        .await;

        {
            let mut privilege = self.state.cpu_privilege.write().expect("rwlock poisoned");
            match &result {
                Ok(CpuWriteSource::Privileged) => {
                    privilege.authorization = CpuAuthorization::Authorized;
                }
                Err(rog_core::RogError::PermissionDenied(message))
                    if message.contains("authorization") =>
                {
                    privilege.authorization = CpuAuthorization::Denied;
                }
                Err(rog_core::RogError::TemporarilyUnavailable(message))
                    if message.contains("helper") =>
                {
                    privilege.status.privileged_helper_reachable = false;
                    privilege.authorization = CpuAuthorization::Unavailable;
                }
                _ => {}
            }
        }

        let refresh = self.refresh_cpu_state();
        self.set_cpu_caps(self.cpu.probe_caps());
        result?;
        refresh?;
        let state = self.read_state();
        if cpu_request_readback_matches(&request, &state.cpu) == Some(false) {
            return Err(rog_core::RogError::TransientFailure(
                "CPU control readback did not confirm the requested value".to_string(),
            ));
        }
        Ok(())
    }
}

#[interface(name = "io.github.roghelper.Daemon1")]
impl RogHelperDaemon {
    fn get_configuration(&self) -> String {
        config_to_toml(&self.read_config()).unwrap_or_else(|error| {
            warn!("failed to serialize in-memory configuration: {error}");
            config_to_toml(&AppConfig::default()).expect("default configuration serializes")
        })
    }

    fn set_configuration(&self, contents: &str) -> fdo::Result<()> {
        let config = toml::from_str::<AppConfig>(contents).map_err(|error| {
            fdo::Error::InvalidArgs(format!("configuration is invalid: {error}"))
        })?;
        self.persist_config(config)
            .map(|_| ())
            .map_err(fdo::Error::Failed)
    }

    fn reset_configuration(&self) -> fdo::Result<String> {
        let config = self
            .persist_config(AppConfig::default())
            .map_err(fdo::Error::Failed)?;
        config_to_toml(&config).map_err(fdo::Error::Failed)
    }

    fn get_caps(&self) -> HashMap<String, OwnedValue> {
        caps_to_dbus(&self.read_state().caps)
    }

    async fn get_state(&self) -> HashMap<String, OwnedValue> {
        let state = self.read_state();
        let mut m = state_to_dbus(&state);
        m.insert(
            dbus_keys::state::CPU_CAPS.to_string(),
            ov(cpu_caps_to_dbus(&state.cpu_caps)),
        );
        m.insert(
            dbus_keys::state::CPU.to_string(),
            ov(cpu_to_dbus(&state.cpu)),
        );
        m.insert(
            dbus_keys::FAN_STATE_KEY.to_string(),
            ov(fan_state_to_dbus(&state.fan_state)),
        );
        m.insert(
            dbus_keys::FAN_CAPS_KEY.to_string(),
            ov(fan_caps_to_dbus(&state.fan_state.caps)),
        );
        if let Some(l) = self.lighting_to_dbus().await {
            m.insert(dbus_keys::state::LIGHTING.to_string(), ov(l));
        }
        let lighting_diagnostics = self.lighting_diagnostics(None, None);
        m.insert(
            dbus_keys::state::LIGHTING_DIAGNOSTICS_SUMMARY.to_string(),
            ov(lighting_diagnostics.summary_line()),
        );
        m.insert(
            dbus_keys::state::LIGHTING_DIAGNOSTICS_DETAILS.to_string(),
            ov(lighting_diagnostics.to_report_text()),
        );
        let control = self.read_control_state();
        if let Some(p) = control.profile {
            m.insert(dbus_keys::state::PROFILE.to_string(), ov(profile_to_str(p)));
        }
        if let Some(g) = control.gpu_mode {
            m.insert(
                dbus_keys::state::GPU_MODE.to_string(),
                ov(gpu_mode_to_str(g)),
            );
        }
        if let Some(lim) = control.battery_limit {
            m.insert(
                dbus_keys::state::BATTERY_LIMIT.to_string(),
                OwnedValue::from(lim.0 as u64),
            );
        }
        m
    }

    fn get_telemetry(&self) -> HashMap<String, OwnedValue> {
        telemetry_to_dbus(&self.read_state().telemetry)
    }

    fn get_cpu_caps(&self) -> HashMap<String, OwnedValue> {
        cpu_caps_to_dbus(&self.read_state().cpu_caps)
    }

    fn get_cpu_telemetry(&self) -> HashMap<String, OwnedValue> {
        cpu_to_dbus(&self.read_state().cpu)
    }

    fn get_cpu_diagnostics(&self) -> String {
        let state = self.read_state();
        cpu_diagnostics_text(&state.cpu_caps, &state.cpu)
    }

    async fn get_setup_status(&self) -> HashMap<String, OwnedValue> {
        let state = self.read_state();
        let keyboard_paths = self
            .kbd_backlight
            .as_ref()
            .map(|kbd| vec![kbd.brightness_path().display().to_string()])
            .unwrap_or_default();
        let setup = probe_setup_status(
            RogHelperdProbe::AssumeConnected,
            &state.caps,
            &state.cpu_caps,
            &state.fan_state.caps,
            keyboard_paths,
        )
        .await;
        setup_status_to_dbus(&setup)
    }

    async fn get_privileged_status(&self) -> HashMap<String, OwnedValue> {
        privileged_status_to_dbus(&privileged_client::probe().await)
    }

    fn get_fan_caps(&self) -> HashMap<String, OwnedValue> {
        fan_caps_to_dbus(&self.read_state().fan_state.caps)
    }

    fn get_fan_state(&self) -> HashMap<String, OwnedValue> {
        fan_state_to_dbus(&self.read_state().fan_state)
    }

    fn get_fan_curves(&self) -> HashMap<String, OwnedValue> {
        let mut m = HashMap::new();
        m.insert(
            dbus_keys::fan_curves::SUPPORTED.to_string(),
            OwnedValue::from(self.read_state().fan_state.caps.has_fan_curves),
        );
        m.insert(
            dbus_keys::fan_curves::REASON.to_string(),
            ov(
                "Fan curve reading is backend-dependent and not exposed by the active backend yet."
                    .to_string(),
            ),
        );
        m
    }

    async fn set_fan_auto(&self, fan_id: &str) -> fdo::Result<()> {
        let target = (!fan_id.trim().is_empty()).then_some(fan_id);
        let result = with_fan_privileged_fallback(
            || self.hwmon.set_fan_auto(target),
            || privileged_client::set_fan_auto(fan_id),
        )
        .await;
        match &result {
            Ok(CpuWriteSource::Privileged) => {
                *self
                    .state
                    .fan_authorization
                    .write()
                    .expect("rwlock poisoned") = "authorized".to_string();
            }
            Err(rog_core::RogError::PermissionDenied(_)) => {
                *self
                    .state
                    .fan_authorization
                    .write()
                    .expect("rwlock poisoned") = "denied".to_string();
            }
            _ => {}
        }
        if let Ok(state) = self.hwmon.fan_state() {
            self.set_fan_state(state);
        }
        result.map_err(map_rog_error_to_fdo)?;
        self.update_fan_control_state(
            FanControlMode::Auto,
            "Returned fan control to Auto/BIOS mode.",
            None,
        );
        Ok(())
    }

    async fn set_fan_manual_percent(&self, fan_id: &str, percent: u64) -> fdo::Result<()> {
        let percent = u8::try_from(percent)
            .map_err(|_| fdo::Error::InvalidArgs("fan percent must fit into u8".to_string()))?;
        let target = if self.read_control_state().fan_sync_enabled || fan_id.trim().is_empty() {
            ""
        } else {
            fan_id
        };
        self.hwmon
            .set_fan_manual_percent(target, percent)
            .map_err(map_rog_error_to_fdo)?;
        self.update_fan_control_state(
            FanControlMode::ManualPercent,
            format!("Applied manual fan speed: {percent}%."),
            None,
        );
        self.set_fan_state(self.hwmon.fan_state().map_err(map_rog_error_to_fdo)?);
        Ok(())
    }

    async fn set_fan_rpm_target(&self, fan_id: &str, rpm: u64) -> fdo::Result<()> {
        let rpm = u32::try_from(rpm)
            .map_err(|_| fdo::Error::InvalidArgs("fan RPM target must fit into u32".to_string()))?;
        self.hwmon
            .set_fan_rpm_target(fan_id, rpm)
            .map_err(map_rog_error_to_fdo)?;
        self.update_fan_control_state(
            FanControlMode::ManualRpmTarget,
            format!("Applied fan RPM target: {rpm} rpm."),
            None,
        );
        self.set_fan_state(self.hwmon.fan_state().map_err(map_rog_error_to_fdo)?);
        Ok(())
    }

    async fn set_fan_curve(
        &self,
        fan_id: &str,
        curve: HashMap<String, OwnedValue>,
    ) -> fdo::Result<()> {
        let curve = fan_curve_from_dbus(fan_id, &curve).map_err(map_rog_error_to_fdo)?;
        let result = with_fan_privileged_fallback(
            || self.hwmon.set_fan_curve(fan_id, curve.clone()),
            || privileged_client::set_fan_curve(fan_id, &curve),
        )
        .await;
        match &result {
            Ok(CpuWriteSource::Privileged) => {
                *self
                    .state
                    .fan_authorization
                    .write()
                    .expect("rwlock poisoned") = "authorized".to_string();
            }
            Err(rog_core::RogError::PermissionDenied(_)) => {
                *self
                    .state
                    .fan_authorization
                    .write()
                    .expect("rwlock poisoned") = "denied".to_string();
            }
            _ => {}
        }
        if let Ok(state) = self.hwmon.fan_state() {
            self.set_fan_state(state);
        }
        result.map_err(map_rog_error_to_fdo)?;
        self.update_fan_control_state(FanControlMode::Curve, "Applied fan curve.", None);
        Ok(())
    }

    fn set_fan_sync(&self, enabled: bool) {
        let mut guard = self.state.control.write().expect("rwlock poisoned");
        guard.fan_sync_enabled = enabled;
        guard.fan_last_action = Some(if enabled {
            "Fan sync enabled.".to_string()
        } else {
            "Fan sync disabled.".to_string()
        });
    }

    async fn set_fan_boost(
        &self,
        fan_id: &str,
        percent: u64,
        duration_seconds: u64,
    ) -> fdo::Result<()> {
        if duration_seconds == 0 || duration_seconds > 60 * 60 {
            return Err(fdo::Error::InvalidArgs(
                "fan boost duration must be 1..=3600 seconds".to_string(),
            ));
        }
        let percent = u8::try_from(percent)
            .map_err(|_| fdo::Error::InvalidArgs("fan percent must fit into u8".to_string()))?;
        let target = if fan_id.trim().is_empty() { "" } else { fan_id };
        self.hwmon
            .set_fan_manual_percent(target, percent)
            .map_err(map_rog_error_to_fdo)?;
        let boost_until = now_ms().saturating_add(duration_seconds.saturating_mul(1000));
        self.update_fan_control_state(
            FanControlMode::FullSpeedBoost,
            format!("Fan boost active at {percent}% for {duration_seconds}s."),
            Some(boost_until),
        );
        self.set_fan_state(self.hwmon.fan_state().map_err(map_rog_error_to_fdo)?);

        let daemon = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(duration_seconds)).await;
            let current_until = daemon.read_control_state().fan_boost_until_ms;
            if current_until == Some(boost_until) {
                if let Err(err) = daemon.hwmon.set_fan_auto(None) {
                    warn!("fan boost auto restore failed: {err}");
                    daemon.update_fan_control_state(
                        FanControlMode::ReadOnly,
                        format!("Fan boost restore failed: {err}"),
                        None,
                    );
                } else {
                    daemon.update_fan_control_state(
                        FanControlMode::Auto,
                        "Fan boost finished; restored Auto/BIOS mode.",
                        None,
                    );
                }
                if let Ok(state) = daemon.hwmon.fan_state() {
                    daemon.set_fan_state(state);
                }
            }
        });
        Ok(())
    }

    async fn reset_fans_to_auto(&self) -> fdo::Result<()> {
        let result = with_fan_privileged_fallback(
            || self.hwmon.set_fan_auto(None),
            privileged_client::reset_fans_to_auto,
        )
        .await;
        match &result {
            Ok(CpuWriteSource::Privileged) => {
                *self
                    .state
                    .fan_authorization
                    .write()
                    .expect("rwlock poisoned") = "authorized".to_string();
            }
            Err(rog_core::RogError::PermissionDenied(_)) => {
                *self
                    .state
                    .fan_authorization
                    .write()
                    .expect("rwlock poisoned") = "denied".to_string();
            }
            _ => {}
        }
        if let Ok(state) = self.hwmon.fan_state() {
            self.set_fan_state(state);
        }
        result.map_err(map_rog_error_to_fdo)?;
        self.update_fan_control_state(
            FanControlMode::Auto,
            "Reset all controllable fans to Auto/BIOS mode.",
            None,
        );
        Ok(())
    }

    async fn set_lighting(&self, state: HashMap<String, OwnedValue>) -> fdo::Result<()> {
        const ACCEPTED_KEYS: &[&str] = &["brightness", "mode", "rgb_hex", "speed", "zone"];
        if let Some(key) = state
            .keys()
            .find(|key| !ACCEPTED_KEYS.contains(&key.as_str()))
        {
            return Err(fdo::Error::InvalidArgs(format!(
                "unknown lighting key '{key}'"
            )));
        }

        if state.contains_key("speed") {
            return Err(fdo::Error::NotSupported(
                "Lighting speed is unavailable because no verified backend speed contract is active."
                    .to_string(),
            ));
        }
        if state.contains_key("zone") {
            return Err(fdo::Error::NotSupported(
                "Lighting zones are unavailable because no verified backend zone contract is active."
                    .to_string(),
            ));
        }

        let mode = state
            .get("mode")
            .map(|v| {
                <&str>::try_from(v)
                    .map(str::trim)
                    .map(str::to_string)
                    .map_err(|_| {
                        fdo::Error::InvalidArgs("lighting mode must be a string".to_string())
                    })
            })
            .transpose()?;
        if mode.as_deref() == Some("") {
            return Err(fdo::Error::InvalidArgs(
                "lighting mode cannot be empty".to_string(),
            ));
        }
        let rgb_hex = state
            .get("rgb_hex")
            .map(|v| {
                <&str>::try_from(v)
                    .map_err(|_| fdo::Error::InvalidArgs("rgb_hex must be a string".to_string()))
            })
            .transpose()?;
        let rgb = rgb_hex
            .map(RgbColor::parse_hex)
            .transpose()
            .map_err(map_rog_error_to_fdo)?;
        let brightness = u64_from_map(&state, "brightness");
        if state.contains_key("brightness") && brightness.is_none() {
            return Err(fdo::Error::InvalidArgs(
                "brightness must be a non-negative integer".to_string(),
            ));
        }
        let brightness = brightness
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    fdo::Error::InvalidArgs(format!(
                        "lighting brightness {value} exceeds the supported integer range"
                    ))
                })
            })
            .transpose()?;

        if mode.is_none() && rgb.is_none() && brightness.is_none() {
            return Err(fdo::Error::InvalidArgs(
                "lighting request contains no control value".to_string(),
            ));
        }

        if let Some(aura) = &self.aura {
            if let Some(mode) = mode.as_deref() {
                if !aura.can_set_mode() {
                    return Err(fdo::Error::NotSupported(format!(
                        "Aura backend does not expose writable lighting modes for '{mode}'."
                    )));
                }
            }
            if rgb.is_some() && !aura.supports_rgb() {
                return Err(fdo::Error::NotSupported(
                    "RGB colour is not exposed as a writable asusd Aura control.".to_string(),
                ));
            }
            if !aura.can_set_brightness() {
                if let (Some(brightness), Some(kbd)) = (brightness, self.kbd_backlight.as_ref()) {
                    if brightness > kbd.max_brightness() {
                        return Err(fdo::Error::InvalidArgs(format!(
                            "keyboard brightness {brightness} exceeds backend maximum {}",
                            kbd.max_brightness()
                        )));
                    }
                } else if brightness.is_some() {
                    return Err(fdo::Error::NotSupported(
                        "Aura backend does not expose brightness and no sysfs keyboard backlight fallback is available."
                            .to_string(),
                    ));
                }
            }
            if let Some(mode) = mode.as_deref() {
                let parsed_mode = LightingMode::parse_label(mode)
                    .unwrap_or_else(|| LightingMode::Other(mode.trim().to_string()));
                aura.set_mode(parsed_mode)
                    .await
                    .map_err(map_rog_error_to_fdo)?;
            }

            if let Some(rgb) = rgb {
                aura.set_rgb(rgb).await.map_err(map_rog_error_to_fdo)?;
            }

            if let Some(brightness) = brightness {
                if aura.can_set_brightness() {
                    aura.set_brightness(brightness)
                        .await
                        .map_err(map_rog_error_to_fdo)?;
                } else if let Some(kbd) = &self.kbd_backlight {
                    kbd.set_brightness(brightness)
                        .map_err(map_rog_error_to_fdo)?;
                }
            }

            return Ok(());
        }

        if rgb.is_some() {
            return Err(fdo::Error::NotSupported(
                "RGB colour requires ASUS Aura support. Current backend only supports keyboard brightness."
                    .to_string(),
            ));
        }
        let mode = mode.unwrap_or_else(|| "Static".to_string());
        if mode.eq_ignore_ascii_case("off") {
            // Off is the only synthetic mode supported by the brightness-only
            // sysfs backend, and maps exactly to brightness zero.
        } else if !mode.eq_ignore_ascii_case("static") {
            return Err(fdo::Error::NotSupported(format!(
                "Lighting mode '{mode}' is not supported by the sysfs LED backend."
            )));
        }

        let Some(kbd) = &self.kbd_backlight else {
            return Err(fdo::Error::NotSupported(
                "lighting not supported on this system".to_string(),
            ));
        };

        let brightness = if mode.eq_ignore_ascii_case("off") {
            0
        } else {
            match brightness {
                Some(brightness) => brightness,
                None => kbd.read_brightness().map_err(map_rog_error_to_fdo)?,
            }
        };
        kbd.set_brightness(brightness)
            .map_err(map_rog_error_to_fdo)?;
        Ok(())
    }

    async fn set_profile(&self, profile: &str) -> fdo::Result<()> {
        let Some(asusd) = &self.asusd else {
            return Err(fdo::Error::NotSupported(
                "profile control requires asusd platform interface".to_string(),
            ));
        };

        let profile = parse_profile_request(profile).map_err(fdo::Error::InvalidArgs)?;
        asusd
            .set_profile(profile.clone())
            .await
            .map_err(map_rog_error_to_fdo)?;

        let confirmed = asusd.get_profile().await.unwrap_or(profile);
        let mut guard = self.state.control.write().expect("rwlock poisoned");
        guard.profile = Some(confirmed);
        Ok(())
    }

    async fn set_gpu_mode(&self, mode: &str) -> fdo::Result<()> {
        let Some(supergfx) = &self.supergfx else {
            return Err(fdo::Error::NotSupported(
                "GPU mode control requires supergfxd".to_string(),
            ));
        };

        let mode = parse_gpu_mode_request(mode).map_err(fdo::Error::InvalidArgs)?;
        supergfx
            .set_mode(mode.clone())
            .await
            .map_err(map_rog_error_to_fdo)?;

        let confirmed = supergfx.get_mode().await.unwrap_or(mode);
        let mut guard = self.state.control.write().expect("rwlock poisoned");
        guard.gpu_mode = Some(confirmed);
        Ok(())
    }

    async fn set_battery_limit(&self, limit: u64) -> fdo::Result<()> {
        let Some(asusd) = &self.asusd else {
            return Err(fdo::Error::NotSupported(
                "battery limit control requires asusd platform interface".to_string(),
            ));
        };

        let limit = u8::try_from(limit)
            .map_err(|_| fdo::Error::InvalidArgs("battery limit must fit into u8".to_string()))?;
        asusd
            .set_limit(BatteryLimitPercent(limit))
            .await
            .map_err(map_rog_error_to_fdo)?;

        let confirmed = asusd
            .get_limit()
            .await
            .unwrap_or(BatteryLimitPercent(limit));
        let mut guard = self.state.control.write().expect("rwlock poisoned");
        guard.battery_limit = Some(confirmed);
        Ok(())
    }

    async fn set_cpu_turbo(&self, enabled: bool) -> fdo::Result<()> {
        self.apply_cpu_control(CpuControlRequest::Turbo(enabled))
            .await
            .map_err(map_rog_error_to_fdo)
    }

    async fn set_cpu_power_mode(&self, mode: &str) -> fdo::Result<()> {
        let mode = CpuPowerMode::parse(mode).map_err(map_rog_error_to_fdo)?;
        self.apply_cpu_control(CpuControlRequest::PowerMode(mode))
            .await
            .map_err(map_rog_error_to_fdo)
    }

    async fn set_cpu_governor(&self, governor: &str) -> fdo::Result<()> {
        self.apply_cpu_control(CpuControlRequest::Governor(governor.to_string()))
            .await
            .map_err(map_rog_error_to_fdo)
    }

    async fn set_cpu_epp(&self, epp: &str) -> fdo::Result<()> {
        self.apply_cpu_control(CpuControlRequest::Epp(epp.to_string()))
            .await
            .map_err(map_rog_error_to_fdo)
    }

    async fn set_cpu_freq_limits(&self, min_mhz: u64, max_mhz: u64) -> fdo::Result<()> {
        let min = if min_mhz == 0 {
            None
        } else {
            Some(u32::try_from(min_mhz).map_err(|_| {
                fdo::Error::InvalidArgs("min_mhz does not fit into u32".to_string())
            })?)
        };
        let max = if max_mhz == 0 {
            None
        } else {
            Some(u32::try_from(max_mhz).map_err(|_| {
                fdo::Error::InvalidArgs("max_mhz does not fit into u32".to_string())
            })?)
        };

        self.apply_cpu_control(CpuControlRequest::FrequencyLimits {
            min_mhz: min,
            max_mhz: max,
        })
        .await
        .map_err(map_rog_error_to_fdo)
    }

    async fn set_cpu_core_online(&self, core_id: u64, online: bool) -> fdo::Result<()> {
        let core_id = u32::try_from(core_id).map_err(|_| {
            fdo::Error::InvalidArgs("logical CPU id does not fit into u32".to_string())
        })?;
        self.apply_cpu_control(CpuControlRequest::CoreOnline { core_id, online })
            .await
            .map_err(map_rog_error_to_fdo)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,rog_helper=debug".into()),
        )
        .with_target(false)
        .init();

    info!("starting rog-helperd");

    let resolved_config_path = config_path();
    let (app_config, app_config_path) = match resolved_config_path {
        Ok(path) => {
            let legacy = legacy_ui_config_path().unwrap_or_else(|_| path.with_file_name("ui.toml"));
            let loaded = load_or_migrate(&path, &legacy);
            for warning in &loaded.warnings {
                warn!("{warning}");
            }
            info!("configuration path: {}", path.display());
            (loaded.config, Some(path))
        }
        Err(error) => {
            warn!("{error}; using in-memory defaults");
            (AppConfig::default(), None)
        }
    };

    let upower = UPowerProvider::connect_system()
        .await
        .context("connect to UPower")?;
    let hwmon = HwmonTelemetryProvider::default();
    let cpu = CpuTelemetryProvider::default();
    let nvidia = NvidiaSmiTelemetryProvider::default();
    let memory = MemoryTelemetryProvider::default();
    let power_supply = PowerSupplySysfsProvider::default();
    let (kbd_backlight, kbd_backlight_probe_error) = match KbdBacklightSysfs::probe() {
        Ok(v) => (v, None),
        Err(e) => {
            warn!("kbd backlight probe failed: {e}");
            (None, Some(e.to_string()))
        }
    };
    let (asusd, asusd_connect_error) = match AsusdPlatformProvider::connect_system().await {
        Ok(v) => (v, None),
        Err(e) => {
            warn!("asusd platform provider probe failed: {e}");
            (None, Some(e.to_string()))
        }
    };
    let (aura, aura_probe_diagnostics, aura_probe_notes, aura_probe_error) =
        match AuraProvider::probe_system().await {
            Ok(probe) => (probe.provider, probe.diagnostics, probe.notes, None),
            Err(e) => {
                warn!("asusd Aura provider probe failed: {e}");
                let mut diagnostics = AuraProbeDiagnostics::default();
                diagnostics.probe_errors.push(e.to_string());
                (
                    None,
                    diagnostics,
                    vec!["asusd Aura probe could not run to completion.".to_string()],
                    Some(e.to_string()),
                )
            }
        };
    let (supergfx, supergfx_connect_error) = match SupergfxProvider::connect_system().await {
        Ok(v) => (v, None),
        Err(e) => {
            warn!("supergfx provider probe failed: {e}");
            (None, Some(e.to_string()))
        }
    };

    let mut caps = DeviceCaps::unknown();
    let hw_snapshot = hwmon.read_snapshot().unwrap_or_else(|e| {
        warn!("hwmon read failed: {e}");
        TelemetrySnapshot::empty_now(now_ms())
    });
    let cpu_caps = cpu.probe_caps();
    let initial_cpu = cpu
        .read_snapshot(now_ms(), hw_snapshot.cpu_temp_c)
        .unwrap_or_else(|e| {
            warn!("cpu telemetry read failed: {e}");
            CpuTelemetry::empty_now(now_ms())
        });
    let initial_fan_state = hwmon.fan_state().unwrap_or_else(|e| {
        warn!("fan capability probe failed: {e}");
        FanState::from_fans(
            hw_snapshot
                .fan_rows
                .iter()
                .enumerate()
                .map(|(idx, fan)| FanInfo::read_only_from_telemetry((idx + 1) as u32, fan))
                .collect(),
        )
    });
    apply_fan_caps_to_device_caps(&mut caps, &initial_fan_state.caps);
    if hw_snapshot.fan_rows.is_empty() {
        caps.notes
            .push("Fan telemetry via hwmon: no fan inputs detected.".to_string());
    } else {
        caps.notes.push(format!(
            "Fan telemetry via hwmon: {} input(s) detected, {} currently reporting RPM.",
            hw_snapshot.fan_rows.len(),
            hw_snapshot.fans_rpm.len()
        ));
    }
    caps.notes.push(format!(
        "Fan control backend: {} (manual_percent={}, rpm_target={}, curves={}, sync={}, boost={}).",
        initial_fan_state.caps.fan_backend,
        initial_fan_state.caps.has_fan_manual_percent,
        initial_fan_state.caps.has_fan_manual_rpm_target,
        initial_fan_state.caps.has_fan_curves,
        initial_fan_state.caps.has_fan_sync_control,
        initial_fan_state.caps.has_fan_boost,
    ));
    for endpoint in &initial_fan_state.caps.endpoints {
        caps.endpoints.push(format!("fan:{endpoint}"));
    }
    let kbd_backlight_detected = kbd_backlight.is_some() || detect_kbd_backlight();
    caps.has_aura = aura.is_some();
    caps.has_kbd_backlight = kbd_backlight_detected;
    let sysfs_writable = kbd_backlight
        .as_ref()
        .map(KbdBacklightSysfs::can_set_brightness)
        .unwrap_or(false);
    caps.kbd_backlight_access = lighting_access_from_backend_flags(
        caps.has_aura,
        kbd_backlight.is_some(),
        sysfs_writable,
        kbd_backlight_detected,
        kbd_backlight_probe_error.as_deref(),
    );
    if let Some(err) = &kbd_backlight_probe_error {
        caps.notes
            .push(format!("keyboard backlight probing failed: {err}"));
    }
    let mut control_state = ControlState::default();

    if let Some(aura) = &aura {
        caps.endpoints.push(aura.endpoint_tag());
    }
    if let Some(err) = &aura_probe_error {
        caps.notes.push(format!("asusd Aura probe failed: {err}"));
    }
    caps.notes.extend(aura_probe_notes);
    let startup_lighting_diagnostics = build_lighting_diagnostics(
        kbd_backlight.as_ref(),
        kbd_backlight_detected,
        kbd_backlight_probe_error.as_deref(),
        aura.as_ref(),
        &aura_probe_diagnostics,
        None,
        aura_probe_error.as_deref(),
    );
    caps.notes.push(startup_lighting_diagnostics.summary_line());
    if let Some(warning) = &startup_lighting_diagnostics.permission_warning {
        caps.notes.push(warning.clone());
    }
    if let Some(reason) = &startup_lighting_diagnostics.unavailable_reason {
        caps.notes.push(reason.clone());
    }
    if let Some(action) = &startup_lighting_diagnostics.recommended_action {
        caps.notes
            .push(format!("Lighting recommended action: {action}"));
    }

    if let Some(kbd) = &kbd_backlight {
        caps.endpoints.push(format!("sysfs-led:{}", kbd.led_name()));
        caps.notes.push(format!(
            "Keyboard backlight via {} (max {}, writable={}).",
            kbd.led_name(),
            kbd.max_brightness(),
            kbd.can_set_brightness()
        ));
    }

    caps.notes.push(format!(
        "CPU controls: cpufreq={}, epp={}, boost={}, writable={}.",
        cpu_caps.has_cpufreq, cpu_caps.has_epp, cpu_caps.has_boost_toggle, cpu_caps.policy_writable
    ));
    if cpu_caps.has_package_power {
        caps.notes
            .push("CPU package power telemetry via RAPL is available.".to_string());
    }
    for path in &cpu_caps.sysfs_paths {
        caps.endpoints.push(format!("cpu-sysfs:{path}"));
    }

    match &asusd {
        Some(provider) => {
            caps.endpoints
                .push(format!("asusd-platform:{}", provider.endpoint_tag()));
            match provider.probe_caps().await {
                Ok(pcaps) => {
                    caps.has_profiles = pcaps.has_profiles;
                    caps.has_charge_limit = pcaps.has_charge_limit;
                    caps.profile_access = if pcaps.has_profiles {
                        FeatureAvailability::new(
                            FeatureAccessState::Available,
                            "Performance profiles are available through asusd.",
                        )
                    } else {
                        FeatureAvailability::new(
                            FeatureAccessState::Unsupported,
                            "asusd is available, but this machine does not expose ASUS performance profiles.",
                        )
                    };
                    caps.charge_limit_access = if pcaps.has_charge_limit {
                        FeatureAvailability::new(
                            FeatureAccessState::Available,
                            "Battery charge-limit control is available through asusd.",
                        )
                    } else {
                        FeatureAvailability::new(
                            FeatureAccessState::Unsupported,
                            "asusd is available, but this machine does not expose battery charge-limit control.",
                        )
                    };
                    if !pcaps.profile_choices.is_empty() {
                        let choices = pcaps
                            .profile_choices
                            .iter()
                            .map(|p| profile_to_str(p.clone()))
                            .collect::<Vec<_>>()
                            .join(", ");
                        caps.notes
                            .push(format!("asusd profiles available: {choices}"));
                    }
                }
                Err(e) => {
                    caps.profile_access = FeatureAvailability::new(
                        FeatureAccessState::TemporarilyUnavailable,
                        "asusd is present, but profile support could not be confirmed right now.",
                    );
                    caps.charge_limit_access = FeatureAvailability::new(
                        FeatureAccessState::TemporarilyUnavailable,
                        "asusd is present, but charge-limit support could not be confirmed right now.",
                    );
                    caps.notes
                        .push(format!("asusd platform probing failed: {e}"));
                }
            }

            if caps.has_profiles {
                match provider.get_profile().await {
                    Ok(v) => control_state.profile = Some(v),
                    Err(e) => caps
                        .notes
                        .push(format!("could not read current profile from asusd: {e}")),
                }
            }
            if caps.has_charge_limit {
                match provider.get_limit().await {
                    Ok(v) => control_state.battery_limit = Some(v),
                    Err(e) => caps
                        .notes
                        .push(format!("could not read charge limit from asusd: {e}")),
                }
            }
        }
        None => {
            if let Some(err) = &asusd_connect_error {
                caps.profile_access = FeatureAvailability::new(
                    FeatureAccessState::TemporarilyUnavailable,
                    "Performance profiles need asusd, but the system backend could not be reached right now.",
                );
                caps.charge_limit_access = FeatureAvailability::new(
                    FeatureAccessState::TemporarilyUnavailable,
                    "Charge-limit control needs asusd, but the system backend could not be reached right now.",
                );
                caps.notes.push(format!("asusd connect failed: {err}"));
            } else {
                caps.profile_access = FeatureAvailability::new(
                    FeatureAccessState::MissingBackend,
                    "Install and start asusd to enable performance profiles.",
                );
                caps.charge_limit_access = FeatureAvailability::new(
                    FeatureAccessState::MissingBackend,
                    "Install and start asusd to enable charge-limit control.",
                );
                caps.notes
                    .push("asusd not detected; profile/charge controls disabled.".to_string());
            }
        }
    }

    match &supergfx {
        Some(provider) => {
            caps.endpoints
                .push(format!("supergfxd:{}", provider.endpoint_tag()));
            match provider.probe_caps().await {
                Ok(gcaps) => {
                    caps.has_gpu_modes = !gcaps.raw_supported_modes.is_empty();
                    caps.requires_reboot_for_gpu_switch = gcaps.requires_reboot_hint;
                    caps.gpu_mode_access = if caps.has_gpu_modes {
                        FeatureAvailability::new(
                            FeatureAccessState::Available,
                            "GPU mode switching is available through supergfxd.",
                        )
                    } else {
                        FeatureAvailability::new(
                            FeatureAccessState::Unsupported,
                            "supergfxd is available, but this machine does not expose switchable GPU modes.",
                        )
                    };
                    if !gcaps.raw_supported_modes.is_empty() {
                        caps.notes.push(format!(
                            "supergfxd supported modes: {}",
                            gcaps.raw_supported_modes.join(", ")
                        ));
                    }
                }
                Err(e) => {
                    caps.gpu_mode_access = FeatureAvailability::new(
                        FeatureAccessState::TemporarilyUnavailable,
                        "supergfxd is present, but GPU mode support could not be confirmed right now.",
                    );
                    caps.notes.push(format!("supergfxd probing failed: {e}"));
                }
            }

            if caps.has_gpu_modes {
                match provider.get_mode().await {
                    Ok(v) => control_state.gpu_mode = Some(v),
                    Err(e) => caps.notes.push(format!(
                        "could not read current GPU mode from supergfxd: {e}"
                    )),
                }
            }
        }
        None => {
            if let Some(err) = &supergfx_connect_error {
                caps.gpu_mode_access = FeatureAvailability::new(
                    FeatureAccessState::TemporarilyUnavailable,
                    "GPU mode switching needs supergfxd, but the system backend could not be reached right now.",
                );
                caps.notes.push(format!("supergfxd connect failed: {err}"));
            } else {
                caps.gpu_mode_access = FeatureAvailability::new(
                    FeatureAccessState::MissingBackend,
                    "Install and start supergfxd to enable GPU mode switching.",
                );
                caps.notes
                    .push("supergfxd not detected; GPU mode controls disabled.".to_string());
            }
        }
    }

    // Diagnostics: include relevant system bus names (no asusd/supergfxd hardcoding).
    let re =
        Regex::new("(?i)asus|rog|aura|kbd|keyboard|supergfx|power|upower").expect("valid regex");
    if let Ok(names) = rog_providers::dbus::list_system_bus_names_matching(&re).await {
        for n in names {
            caps.endpoints.push(format!("system-bus-name:{n}"));
        }
    }

    let has_profiles = caps.has_profiles;
    let has_charge_limit = caps.has_charge_limit;
    let has_gpu_modes = caps.has_gpu_modes;
    let privileged_status = privileged_client::probe().await;
    let cpu_authorization = if privileged_status.privileged_helper_reachable
        && privileged_status.privileged_helper_compatible
        && privileged_status.polkit_available
        && privileged_status
            .privileged_categories_available
            .contains(&rog_core::PrivilegedCategory::Cpu)
    {
        if privileged_status.authorization_state == rog_core::AuthorizationState::Authorized {
            CpuAuthorization::Authorized
        } else {
            CpuAuthorization::Required
        }
    } else {
        CpuAuthorization::Unavailable
    };
    let mut initial_fan_state = initial_fan_state;
    apply_fan_privilege_to_state(&mut initial_fan_state, &privileged_status, "not_checked");
    let mut init_state = AppState::new(caps, hw_snapshot);
    init_state.fan_state = initial_fan_state;
    init_state.cpu_caps = cpu_caps.clone();
    init_state.cpu = initial_cpu;
    let shared = Arc::new(SharedState {
        inner: RwLock::new(init_state),
        control: RwLock::new(control_state),
        config: RwLock::new(app_config),
        config_path: app_config_path,
        cpu_privilege: RwLock::new(CpuPrivilegeState {
            status: privileged_status,
            authorization: cpu_authorization,
        }),
        fan_authorization: RwLock::new("not_checked".to_string()),
    });

    // Export DBus service on the session bus.
    let daemon = RogHelperDaemon::new(
        shared.clone(),
        kbd_backlight.clone(),
        kbd_backlight_detected,
        kbd_backlight_probe_error.clone(),
        aura.clone(),
        aura_probe_diagnostics.clone(),
        asusd.clone(),
        supergfx.clone(),
        cpu.clone(),
        hwmon.clone(),
    );
    daemon.set_cpu_caps(cpu_caps.clone());
    let daemon_iface = daemon.clone();
    let _conn = connection::Builder::session()?
        .name(DBUS_NAME)?
        .serve_at(DBUS_PATH, daemon_iface)?
        .build()
        .await?;

    info!("DBus service online: {DBUS_NAME} {DBUS_PATH} ({DBUS_IFACE})");

    // Telemetry loop.
    let mut ticker = interval(Duration::from_secs(1));
    let mut last_swap: Option<(u64, u64, u64)> = None; // (ts_ms, pswpin, pswpout)
    let mut next_top_update_ms: u64 = 0;
    let mut cached_top_rows: Option<Vec<rog_core::TopProcessMem>> = None;
    let mut cached_top_text: Option<String> = None;
    let mut next_nvidia_update_ms: u64 = 0;
    let mut cached_nvidia = None;
    let mut nvidia_status = "nvidia-smi not checked yet".to_string();
    let mut nvidia_warning: Option<String> = None;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let mut warnings = Vec::new();
                let cpu_caps = cpu.probe_caps();
                daemon.set_cpu_caps(cpu_caps.clone());

                let mut telemetry = hwmon.read_snapshot().unwrap_or_else(|e| {
                    warnings.push(format!("hwmon unavailable: {e}"));
                    TelemetrySnapshot::empty_now(now_ms())
                });

                // Best-effort memory (RAM/swap) telemetry from procfs.
                match memory.read_snapshot() {
                    Ok(mem) => {
                        telemetry.mem_total_bytes = mem.mem_total_bytes;
                        telemetry.mem_used_bytes = mem.mem_used_bytes;
                        telemetry.mem_used_percent = mem.mem_used_percent;
                        telemetry.mem_available_bytes = mem.mem_available_bytes;
                        telemetry.mem_free_bytes = mem.mem_free_bytes;
                        telemetry.mem_cached_bytes = mem.mem_cached_bytes;
                        telemetry.mem_buffers_bytes = mem.mem_buffers_bytes;
                        telemetry.mem_shared_bytes = mem.mem_shared_bytes;
                        telemetry.mem_anon_bytes = mem.mem_anon_bytes;

                        telemetry.swap_total_bytes = mem.swap_total_bytes;
                        telemetry.swap_used_bytes = mem.swap_used_bytes;
                        telemetry.swap_free_bytes = mem.swap_free_bytes;
                        telemetry.zram_total_bytes = mem.zram_total_bytes;
                        telemetry.zram_used_bytes = mem.zram_used_bytes;
                        telemetry.zswap_enabled = mem.zswap_enabled;

                        telemetry.psi_mem_some_avg10 = mem.psi_mem_some_avg10;
                        telemetry.psi_mem_some_avg60 = mem.psi_mem_some_avg60;
                        telemetry.psi_mem_some_avg300 = mem.psi_mem_some_avg300;
                        telemetry.psi_mem_full_avg10 = mem.psi_mem_full_avg10;
                        telemetry.psi_mem_full_avg60 = mem.psi_mem_full_avg60;
                        telemetry.psi_mem_full_avg300 = mem.psi_mem_full_avg300;

                        telemetry.mem_active_bytes = mem.mem_active_bytes;
                        telemetry.mem_inactive_bytes = mem.mem_inactive_bytes;
                        telemetry.mem_dirty_bytes = mem.mem_dirty_bytes;
                        telemetry.mem_writeback_bytes = mem.mem_writeback_bytes;
                        telemetry.mem_slab_bytes = mem.mem_slab_bytes;
                        telemetry.mem_sreclaimable_bytes = mem.mem_sreclaimable_bytes;
                        telemetry.mem_sunreclaim_bytes = mem.mem_sunreclaim_bytes;
                        telemetry.mem_pagetables_bytes = mem.mem_pagetables_bytes;
                        telemetry.mem_kernelstack_bytes = mem.mem_kernelstack_bytes;
                        telemetry.mem_mapped_bytes = mem.mem_mapped_bytes;
                    }
                    Err(e) => warnings.push(format!("memory telemetry unavailable: {e}")),
                }

                // Swap in/out rate (vmstat deltas).
                match memory.read_vmstat_swap_counters() {
                    Ok(Some(c)) => {
                        let now = now_ms();
                        if let Some((prev_ts, prev_in, prev_out)) = last_swap {
                            let dt_ms = now.saturating_sub(prev_ts);
                            if dt_ms > 0 {
                                let dt_s = (dt_ms as f32) / 1000.0;
                                if c.pswpin_pages >= prev_in {
                                    let din = (c.pswpin_pages - prev_in) as f32 / dt_s;
                                    telemetry.swap_in_pages_per_s = Some(din);
                                }
                                if c.pswpout_pages >= prev_out {
                                    let dout = (c.pswpout_pages - prev_out) as f32 / dt_s;
                                    telemetry.swap_out_pages_per_s = Some(dout);
                                }
                            }
                        }
                        last_swap = Some((now, c.pswpin_pages, c.pswpout_pages));
                    }
                    Ok(None) => {}
                    Err(e) => warnings.push(format!("vmstat unavailable: {e}")),
                }

                // Top memory users (update less frequently to keep proc scanning cheap).
                let now = now_ms();
                if now >= next_top_update_ms {
                    match memory.read_top_memory_processes_rows(10) {
                        Ok(rows) => {
                            cached_top_text = Some(format_top_processes_text(&rows));
                            cached_top_rows = Some(rows);
                        }
                        Err(e) => warnings.push(format!("top processes unavailable: {e}")),
                    }
                    next_top_update_ms = now.saturating_add(5_000);
                }
                telemetry.mem_top_processes = cached_top_text.clone();
                telemetry.mem_top_processes_rows = cached_top_rows.clone();

                // Best-effort battery details via sysfs (fills gaps UPower might not expose).
                match power_supply.read_battery_status() {
                    Ok(st) => {
                        telemetry.battery_health_percent =
                            telemetry.battery_health_percent.or(st.battery_health_percent);
                        telemetry.battery_cycle_count =
                            telemetry.battery_cycle_count.or(st.battery_cycle_count);
                        telemetry.battery_charge_power_w =
                            telemetry.battery_charge_power_w.or(st.battery_charge_power_w);
                        telemetry.battery_discharge_power_w =
                            telemetry.battery_discharge_power_w.or(st.battery_discharge_power_w);
                        telemetry.battery_state = telemetry.battery_state.or(st.battery_state);
                    }
                    Err(e) => warnings.push(format!("power_supply sysfs unavailable: {e}")),
                }

                // Best-effort NVIDIA telemetry (read-only). One multi-field query runs at most
                // every three seconds; its cached sample remains visible between daemon ticks.
                let poll_now_ms = now_ms();
                if poll_now_ms >= next_nvidia_update_ms {
                    next_nvidia_update_ms = poll_now_ms.saturating_add(3_000);
                    match nvidia.read_telemetry().await {
                        Ok(Some(sample)) => {
                            let missing = sample.unavailable_field_count();
                            nvidia_status = if missing == 0 {
                                format!(
                                    "nvidia-smi available · GPU {} detected ({})",
                                    sample.index, sample.name
                                )
                            } else {
                                format!(
                                    "nvidia-smi available · GPU {} detected ({}) · {missing} optional field(s) unavailable",
                                    sample.index, sample.name
                                )
                            };
                            cached_nvidia = Some(sample);
                            nvidia_warning = None;
                        }
                        Ok(None) => {
                            cached_nvidia = None;
                            nvidia_status =
                                "nvidia-smi unavailable or no NVIDIA GPU is currently queryable"
                                    .to_string();
                            nvidia_warning = None;
                        }
                        Err(error) => {
                            cached_nvidia = None;
                            nvidia_status = format!("nvidia-smi telemetry unavailable: {error}");
                            nvidia_warning = Some(nvidia_status.clone());
                        }
                    }
                }
                telemetry.gpu_telemetry_status = Some(nvidia_status.clone());
                if let Some(warning) = nvidia_warning.clone() {
                    warnings.push(warning);
                }
                if let Some(sample) = cached_nvidia.as_ref() {
                    telemetry.gpu_telemetry_provider = Some("nvidia-smi".to_string());
                    telemetry.gpu_name = Some(sample.name.clone());
                    telemetry.gpu_uuid = Some(sample.uuid.clone());
                    telemetry.gpu_pci_bus_id = Some(sample.pci_bus_id.clone());
                    telemetry.gpu_index = Some(sample.index);
                    telemetry.gpu_usage_percent = sample.usage_percent;
                    telemetry.gpu_vram_used_bytes = sample.vram_used_bytes;
                    telemetry.gpu_vram_total_bytes = sample.vram_total_bytes;
                    telemetry.gpu_core_clock_mhz = sample.core_clock_mhz;
                    telemetry.gpu_memory_clock_mhz = sample.memory_clock_mhz;
                    telemetry.gpu_power_w = sample.power_w;
                    if let Some(temp) = sample.temperature_c {
                        telemetry
                            .temps_c
                            .insert("nvidia-smi:gpu_temp".to_string(), temp);
                        // hwmon remains the primary source when it already reported a GPU temp.
                        telemetry.gpu_temp_c.get_or_insert(temp);
                    }
                }

                match upower.read_status().await {
                    Ok(st) => {
                        telemetry.power_source = st.power_source;
                        telemetry.battery_percent = st.battery_percent;
                        telemetry.ac_online = st.ac_online;
                        telemetry.battery_state = st.battery_state.or(telemetry.battery_state);
                        telemetry.battery_health_percent =
                            telemetry.battery_health_percent.or(st.battery_health_percent);
                        telemetry.battery_cycle_count =
                            telemetry.battery_cycle_count.or(st.battery_cycle_count);
                        telemetry.battery_charge_power_w =
                            st.battery_charge_power_w.or(telemetry.battery_charge_power_w);
                        telemetry.battery_discharge_power_w =
                            st.battery_discharge_power_w.or(telemetry.battery_discharge_power_w);
                        telemetry.battery_time_to_empty_s = st.battery_time_to_empty_s;
                        telemetry.battery_time_to_full_s = st.battery_time_to_full_s;
                        // Use UPower's timestamp as the authoritative timestamp if hwmon failed.
                        telemetry.timestamp_ms = telemetry.timestamp_ms.max(st.timestamp_ms);
                    }
                    Err(e) => {
                        warnings.push(format!("UPower unavailable: {e}"));
                    }
                }

                if let Some(kbd) = &kbd_backlight {
                    if !kbd.can_set_brightness() {
                        warnings.push(format!(
                            "Keyboard backlight ({}) is read-only (need asusd or a udev rule).",
                            kbd.led_name()
                        ));
                    }
                }
                if let Some(cpu_warning) = cpu_access_warning(&cpu_caps) {
                    warnings.push(cpu_warning);
                }

                match cpu.read_snapshot(now_ms(), telemetry.cpu_temp_c) {
                    Ok(cpu_snapshot) => daemon.set_cpu_telemetry(cpu_snapshot),
                    Err(e) => warnings.push(format!("cpu telemetry unavailable: {e}")),
                }

                match hwmon.fan_state() {
                    Ok(fan_state) => {
                        let current_mode = daemon.read_control_state().fan_mode;
                        let temp_missing = telemetry.cpu_temp_c.is_none() && telemetry.gpu_temp_c.is_none();
                        let critical_temp = telemetry
                            .cpu_temp_c
                            .into_iter()
                            .chain(telemetry.gpu_temp_c)
                            .any(|temp| temp >= 95.0);
                        if matches!(
                            current_mode,
                            FanControlMode::ManualPercent
                                | FanControlMode::ManualRpmTarget
                                | FanControlMode::Curve
                                | FanControlMode::FullSpeedBoost
                        ) && (temp_missing || critical_temp)
                        {
                            let reason = if critical_temp {
                                "critical temperature threshold reached"
                            } else {
                                "temperature telemetry disappeared"
                            };
                            match hwmon.set_fan_auto(None) {
                                Ok(()) => {
                                    warnings.push(format!(
                                        "fan safety restore: {reason}; returned fans to Auto/BIOS mode"
                                    ));
                                    daemon.update_fan_control_state(
                                        FanControlMode::Auto,
                                        format!("Safety restore: {reason}; Auto/BIOS mode requested."),
                                        None,
                                    );
                                }
                                Err(err) => {
                                    warnings.push(format!("fan safety restore failed after {reason}: {err}"));
                                    daemon.update_fan_control_state(
                                        FanControlMode::ReadOnly,
                                        format!("Safety restore failed after {reason}: {err}"),
                                        None,
                                    );
                                }
                            }
                        }
                        daemon.set_fan_state(fan_state);
                    }
                    Err(e) => warnings.push(format!("fan state unavailable: {e}")),
                }

                let mut current_profile = None;
                let mut current_gpu_mode = None;
                let mut current_battery_limit = None;

                if let Some(provider) = &asusd {
                    if has_profiles {
                        match provider.get_profile().await {
                            Ok(v) => current_profile = Some(v),
                            Err(e) => warnings.push(format!("profile read failed: {e}")),
                        }
                    }
                    if has_charge_limit {
                        match provider.get_limit().await {
                            Ok(v) => current_battery_limit = Some(v),
                            Err(e) => warnings.push(format!("charge limit read failed: {e}")),
                        }
                    }
                }

                if let Some(provider) = &supergfx {
                    if has_gpu_modes {
                        match provider.get_mode().await {
                            Ok(v) => current_gpu_mode = Some(v),
                            Err(e) => warnings.push(format!("GPU mode read failed: {e}")),
                        }
                        if let Ok(Some(hint)) = provider.can_switch_now().await {
                            warnings.push(format!("GPU switch hint: {hint}"));
                        }
                    }
                }

                daemon.set_control_state(current_profile, current_gpu_mode, current_battery_limit);
                daemon.set_telemetry(telemetry, warnings);
            }
            _ = tokio::signal::ctrl_c() => {
                info!("ctrl-c received; exiting");
                if matches!(
                    daemon.read_control_state().fan_mode,
                    FanControlMode::ManualPercent
                        | FanControlMode::ManualRpmTarget
                        | FanControlMode::Curve
                        | FanControlMode::FullSpeedBoost
                ) {
                    if let Err(err) = hwmon.set_fan_auto(None) {
                        warn!("fan auto restore on shutdown failed: {err}");
                    }
                }
                break;
            }
        }
    }

    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn detect_kbd_backlight() -> bool {
    // Best-effort sysfs check; capability-driven UI still needs provider support later.
    let Ok(entries) = std::fs::read_dir("/sys/class/leds") else {
        return false;
    };
    for ent in entries.flatten() {
        let Some(name) = ent.file_name().to_str().map(|s| s.to_ascii_lowercase()) else {
            continue;
        };
        if name.contains("kbd")
            || name.contains("kbd_backlight")
            || (name.contains("asus") && name.contains("keyboard"))
        {
            return true;
        }
    }
    false
}

fn lighting_access_from_backend_flags(
    aura_available: bool,
    sysfs_available: bool,
    sysfs_writable: bool,
    sysfs_detected: bool,
    sysfs_probe_error: Option<&str>,
) -> FeatureAvailability {
    if aura_available {
        return FeatureAvailability::new(
            FeatureAccessState::Available,
            "Keyboard lighting is available through ASUS Aura/RGB.",
        );
    }

    if sysfs_available {
        if sysfs_writable {
            FeatureAvailability::new(
                FeatureAccessState::Available,
                "Keyboard backlight is available through the sysfs LED backend.",
            )
        } else {
            FeatureAvailability::new(
                FeatureAccessState::PermissionDenied,
                "Keyboard backlight is detected, but writes are blocked for the current user.",
            )
        }
    } else if sysfs_probe_error.is_some() {
        FeatureAvailability::new(
            FeatureAccessState::TemporarilyUnavailable,
            "Keyboard backlight hardware was detected, but the backend could not be opened right now.",
        )
    } else if sysfs_detected {
        FeatureAvailability::new(
            FeatureAccessState::TemporarilyUnavailable,
            "Keyboard backlight hardware was detected, but the current backend is not ready yet.",
        )
    } else {
        FeatureAvailability::new(
            FeatureAccessState::Unsupported,
            "No keyboard lighting control was detected on this machine.",
        )
    }
}

fn state_to_dbus(s: &AppState) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert(
        dbus_keys::state::CAPS.to_string(),
        ov(caps_to_dbus(&s.caps)),
    );
    m.insert(
        dbus_keys::state::TELEMETRY.to_string(),
        ov(telemetry_to_dbus(&s.telemetry)),
    );
    m.insert(
        dbus_keys::FAN_STATE_KEY.to_string(),
        ov(fan_state_to_dbus(&s.fan_state)),
    );
    m.insert(
        dbus_keys::FAN_CAPS_KEY.to_string(),
        ov(fan_caps_to_dbus(&s.fan_state.caps)),
    );
    m.insert(
        dbus_keys::state::WARNINGS.to_string(),
        ov(s.warnings.clone()),
    );
    m
}

fn caps_to_dbus(c: &DeviceCaps) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert(
        dbus_keys::caps::HAS_PROFILES.to_string(),
        OwnedValue::from(c.has_profiles),
    );
    m.insert(
        dbus_keys::caps::HAS_FAN_CURVES.to_string(),
        OwnedValue::from(c.has_fan_curves),
    );
    m.insert(
        dbus_keys::caps::HAS_FAN_READING.to_string(),
        OwnedValue::from(c.has_fan_reading),
    );
    m.insert(
        dbus_keys::caps::HAS_CHARGE_LIMIT.to_string(),
        OwnedValue::from(c.has_charge_limit),
    );
    m.insert(
        dbus_keys::caps::HAS_GPU_MODES.to_string(),
        OwnedValue::from(c.has_gpu_modes),
    );
    m.insert(
        dbus_keys::caps::HAS_AURA.to_string(),
        OwnedValue::from(c.has_aura),
    );
    m.insert(
        dbus_keys::caps::HAS_KBD_BACKLIGHT.to_string(),
        OwnedValue::from(c.has_kbd_backlight),
    );
    m.insert(
        dbus_keys::caps::HAS_FAN_MANUAL_PERCENT.to_string(),
        OwnedValue::from(c.has_fan_manual_percent),
    );
    m.insert(
        dbus_keys::caps::HAS_FAN_MANUAL_RPM_TARGET.to_string(),
        OwnedValue::from(c.has_fan_manual_rpm_target),
    );
    m.insert(
        dbus_keys::caps::HAS_INDIVIDUAL_FAN_CONTROL.to_string(),
        OwnedValue::from(c.has_individual_fan_control),
    );
    m.insert(
        dbus_keys::caps::HAS_FAN_SYNC_CONTROL.to_string(),
        OwnedValue::from(c.has_fan_sync_control),
    );
    m.insert(
        dbus_keys::caps::HAS_FAN_BOOST.to_string(),
        OwnedValue::from(c.has_fan_boost),
    );
    m.insert(
        dbus_keys::caps::FAN_COUNT.to_string(),
        OwnedValue::from(c.fan_count as u64),
    );
    m.insert(
        dbus_keys::caps::FAN_BACKEND.to_string(),
        ov(c.fan_backend.clone()),
    );
    m.insert(
        dbus_keys::caps::REQUIRES_REBOOT_FOR_GPU_SWITCH.to_string(),
        OwnedValue::from(c.requires_reboot_for_gpu_switch),
    );
    m.insert(
        dbus_keys::caps::ENDPOINTS.to_string(),
        ov(c.endpoints.clone()),
    );
    m.insert(dbus_keys::caps::NOTES.to_string(), ov(c.notes.clone()));
    insert_feature_access_to_dbus(&mut m, dbus_keys::PROFILE_ACCESS_PREFIX, &c.profile_access);
    insert_feature_access_to_dbus(
        &mut m,
        dbus_keys::CHARGE_LIMIT_ACCESS_PREFIX,
        &c.charge_limit_access,
    );
    insert_feature_access_to_dbus(
        &mut m,
        dbus_keys::GPU_MODE_ACCESS_PREFIX,
        &c.gpu_mode_access,
    );
    insert_feature_access_to_dbus(
        &mut m,
        dbus_keys::KBD_BACKLIGHT_ACCESS_PREFIX,
        &c.kbd_backlight_access,
    );
    m
}

fn apply_fan_caps_to_device_caps(caps: &mut DeviceCaps, fan_caps: &FanCaps) {
    caps.has_fan_reading = fan_caps.has_fan_reading;
    caps.has_fan_curves = fan_caps.has_fan_curves;
    caps.has_fan_manual_percent = fan_caps.has_fan_manual_percent;
    caps.has_fan_manual_rpm_target = fan_caps.has_fan_manual_rpm_target;
    caps.has_individual_fan_control = fan_caps.has_individual_fan_control;
    caps.has_fan_sync_control = fan_caps.has_fan_sync_control;
    caps.has_fan_boost = fan_caps.has_fan_boost;
    caps.fan_count = fan_caps.fan_count;
    caps.fan_backend = fan_caps.fan_backend.clone();
}

fn apply_fan_privilege_to_state(
    state: &mut FanState,
    status: &rog_core::PrivilegedStatus,
    authorization: &str,
) {
    let helper_ready = status.privileged_helper_reachable
        && status.privileged_helper_compatible
        && status.polkit_available
        && status
            .privileged_categories_available
            .contains(&rog_core::PrivilegedCategory::Fans);
    for fan in &mut state.fans {
        if !fan.pwm_endpoint_verified || (!fan.supports_curve && !fan.supports_auto) {
            continue;
        }
        if fan.direct_write {
            fan.controllable = true;
            fan.privileged_write = false;
            fan.authorization = "not_required".to_string();
            fan.access_state = "direct".to_string();
        } else if helper_ready {
            fan.controllable = true;
            fan.privileged_write = true;
            fan.authorization = authorization.to_string();
            fan.access_state = match authorization {
                "denied" => "authorization_denied",
                "authorized" => "curve_control_available",
                _ => "authorization_required",
            }
            .to_string();
        } else {
            fan.controllable = false;
            fan.privileged_write = false;
            fan.authorization = "unavailable".to_string();
            fan.access_state =
                if !status.privileged_helper_installed || !status.privileged_helper_reachable {
                    "helper_missing"
                } else {
                    "unsafe_read_only"
                }
                .to_string();
        }
    }
    state.caps = FanCaps::from_fans(&state.fans);
}

fn fan_caps_to_dbus(c: &FanCaps) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert(
        "has_fan_reading".to_string(),
        OwnedValue::from(c.has_fan_reading),
    );
    m.insert(
        "has_fan_curves".to_string(),
        OwnedValue::from(c.has_fan_curves),
    );
    m.insert(
        "fan_curve_readable".to_string(),
        OwnedValue::from(c.fan_curve_readable),
    );
    m.insert(
        "fan_curve_writable".to_string(),
        OwnedValue::from(c.fan_curve_writable),
    );
    m.insert(
        "has_fan_manual_percent".to_string(),
        OwnedValue::from(c.has_fan_manual_percent),
    );
    m.insert(
        "has_fan_manual_rpm_target".to_string(),
        OwnedValue::from(c.has_fan_manual_rpm_target),
    );
    m.insert(
        "has_individual_fan_control".to_string(),
        OwnedValue::from(c.has_individual_fan_control),
    );
    m.insert(
        "has_fan_sync_control".to_string(),
        OwnedValue::from(c.has_fan_sync_control),
    );
    m.insert(
        "has_fan_boost".to_string(),
        OwnedValue::from(c.has_fan_boost),
    );
    m.insert(
        "fan_count".to_string(),
        OwnedValue::from(c.fan_count as u64),
    );
    m.insert("fan_backend".to_string(), ov(c.fan_backend.clone()));
    m.insert(
        "fan_mapping_confidence".to_string(),
        ov(c.fan_mapping_confidence.as_str().to_string()),
    );
    m.insert("endpoints".to_string(), ov(c.endpoints.clone()));
    m.insert("notes".to_string(), ov(c.notes.clone()));
    m.insert("warnings".to_string(), ov(c.warnings.clone()));
    m
}

fn fan_state_to_dbus(s: &FanState) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert(
        dbus_keys::FAN_CAPS_KEY.to_string(),
        ov(fan_caps_to_dbus(&s.caps)),
    );
    m.insert(
        dbus_keys::FAN_INFOS_KEY.to_string(),
        ov(s.fans.iter().map(fan_info_to_dbus).collect::<Vec<_>>()),
    );
    m.insert("mode".to_string(), ov(s.mode.as_str().to_string()));
    m.insert("sync_enabled".to_string(), OwnedValue::from(s.sync_enabled));
    if let Some(last_action) = &s.last_action {
        m.insert("last_action".to_string(), ov(last_action.clone()));
    }
    if let Some(fan_id) = &s.active_boost_fan_id {
        m.insert("active_boost_fan_id".to_string(), ov(fan_id.clone()));
    }
    if let Some(until) = s.active_boost_until_ms {
        m.insert("active_boost_until_ms".to_string(), OwnedValue::from(until));
    }
    if let Some(summary) = &s.active_curve_summary {
        m.insert("active_curve_summary".to_string(), ov(summary.clone()));
    }
    m.insert("warnings".to_string(), ov(s.warnings.clone()));
    m
}

fn fan_info_to_dbus(fan: &FanInfo) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert(dbus_keys::FAN_INFO_ID_KEY.to_string(), ov(fan.id.clone()));
    m.insert(
        dbus_keys::FAN_INFO_INDEX_KEY.to_string(),
        OwnedValue::from(fan.index as u64),
    );
    m.insert(
        dbus_keys::FAN_INFO_LABEL_KEY.to_string(),
        ov(fan.label.clone()),
    );
    m.insert(
        dbus_keys::FAN_INFO_MAPPING_CONFIDENCE_KEY.to_string(),
        ov(fan.mapping_confidence.as_str().to_string()),
    );
    if let Some(v) = fan.current_rpm {
        m.insert(
            dbus_keys::FAN_INFO_CURRENT_RPM_KEY.to_string(),
            OwnedValue::from(v as u64),
        );
    }
    if let Some(v) = fan.min_rpm {
        m.insert(
            dbus_keys::FAN_INFO_MIN_RPM_KEY.to_string(),
            OwnedValue::from(v as u64),
        );
    }
    if let Some(v) = fan.max_rpm {
        m.insert(
            dbus_keys::FAN_INFO_MAX_RPM_KEY.to_string(),
            OwnedValue::from(v as u64),
        );
    }
    if let Some(v) = fan.current_percent {
        m.insert(
            dbus_keys::FAN_INFO_CURRENT_PERCENT_KEY.to_string(),
            OwnedValue::from(v as u64),
        );
    }
    m.insert(
        dbus_keys::FAN_INFO_RPM_READABLE_KEY.to_string(),
        OwnedValue::from(fan.rpm_readable),
    );
    m.insert(
        dbus_keys::FAN_INFO_PWM_ENDPOINT_VERIFIED_KEY.to_string(),
        OwnedValue::from(fan.pwm_endpoint_verified),
    );
    m.insert(
        dbus_keys::FAN_INFO_DIRECT_WRITE_KEY.to_string(),
        OwnedValue::from(fan.direct_write),
    );
    m.insert(
        dbus_keys::FAN_INFO_PRIVILEGED_WRITE_KEY.to_string(),
        OwnedValue::from(fan.privileged_write),
    );
    m.insert(
        dbus_keys::FAN_INFO_AUTHORIZATION_KEY.to_string(),
        ov(fan.authorization.clone()),
    );
    m.insert(
        dbus_keys::FAN_INFO_ACCESS_STATE_KEY.to_string(),
        ov(fan.access_state.clone()),
    );
    m.insert(
        dbus_keys::FAN_INFO_CONTROLLABLE_KEY.to_string(),
        OwnedValue::from(fan.controllable),
    );
    m.insert(
        dbus_keys::FAN_INFO_SUPPORTS_MANUAL_PERCENT_KEY.to_string(),
        OwnedValue::from(fan.supports_manual_percent),
    );
    m.insert(
        dbus_keys::FAN_INFO_SUPPORTS_MANUAL_RPM_TARGET_KEY.to_string(),
        OwnedValue::from(fan.supports_manual_rpm_target),
    );
    m.insert(
        dbus_keys::FAN_INFO_SUPPORTS_CURVE_KEY.to_string(),
        OwnedValue::from(fan.supports_curve),
    );
    m.insert(
        dbus_keys::FAN_INFO_SUPPORTS_AUTO_KEY.to_string(),
        OwnedValue::from(fan.supports_auto),
    );
    m.insert(
        dbus_keys::FAN_INFO_BACKEND_KEY.to_string(),
        ov(fan.backend.clone()),
    );
    m.insert(
        dbus_keys::FAN_INFO_ENDPOINTS_KEY.to_string(),
        ov(fan.endpoints.clone()),
    );
    m.insert(
        dbus_keys::FAN_INFO_NOTES_KEY.to_string(),
        ov(fan.notes.clone()),
    );
    m.insert(
        dbus_keys::FAN_INFO_WARNINGS_KEY.to_string(),
        ov(fan.warnings.clone()),
    );
    m
}

fn insert_feature_access_to_dbus(
    map: &mut HashMap<String, OwnedValue>,
    prefix: &str,
    access: &FeatureAvailability,
) {
    map.insert(
        dbus_keys::feature_access_status_key(prefix),
        ov(access.status.as_str().to_string()),
    );
    map.insert(
        dbus_keys::feature_access_reason_key(prefix),
        ov(access.reason.clone()),
    );
}

fn cpu_caps_to_dbus(c: &CpuCaps) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert(
        dbus_keys::cpu_caps::HAS_CPUFREQ.to_string(),
        OwnedValue::from(c.has_cpufreq),
    );
    m.insert(
        dbus_keys::cpu_caps::HAS_EPP.to_string(),
        OwnedValue::from(c.has_epp),
    );
    m.insert(
        dbus_keys::cpu_caps::HAS_BOOST_TOGGLE.to_string(),
        OwnedValue::from(c.has_boost_toggle),
    );
    m.insert(
        dbus_keys::cpu_caps::HAS_PACKAGE_POWER.to_string(),
        OwnedValue::from(c.has_package_power),
    );
    m.insert(
        dbus_keys::cpu_caps::POLICY_WRITABLE.to_string(),
        OwnedValue::from(c.policy_writable),
    );
    m.insert(
        dbus_keys::cpu_caps::HAS_MIN_FREQ_LIMIT.to_string(),
        OwnedValue::from(c.has_min_freq_limit),
    );
    m.insert(
        dbus_keys::cpu_caps::HAS_MAX_FREQ_LIMIT.to_string(),
        OwnedValue::from(c.has_max_freq_limit),
    );
    m.insert(
        dbus_keys::cpu_caps::HAS_GOVERNOR.to_string(),
        OwnedValue::from(c.has_governor),
    );
    m.insert(
        dbus_keys::cpu_caps::HAS_CORE_ONLINE.to_string(),
        OwnedValue::from(c.has_core_online),
    );
    if let Some(v) = &c.scaling_driver {
        m.insert(
            dbus_keys::cpu_caps::SCALING_DRIVER.to_string(),
            ov(v.clone()),
        );
    }
    m.insert(
        dbus_keys::cpu_caps::CPU_COUNT.to_string(),
        OwnedValue::from(c.cpu_count as u64),
    );
    m.insert(
        dbus_keys::cpu_caps::THREAD_COUNT.to_string(),
        OwnedValue::from(c.thread_count as u64),
    );
    m.insert(
        dbus_keys::cpu_caps::GOVERNOR_CHOICES.to_string(),
        ov(c.governor_choices.clone()),
    );
    m.insert(
        dbus_keys::cpu_caps::EPP_CHOICES.to_string(),
        ov(c.epp_choices.clone()),
    );
    m.insert(
        dbus_keys::cpu_caps::SYSFS_PATHS.to_string(),
        ov(c.sysfs_paths.clone()),
    );
    m.insert(
        dbus_keys::CPU_CONTROL_ACCESS_KEY.to_string(),
        ov(c.control_access
            .iter()
            .map(cpu_control_access_to_dbus)
            .collect::<Vec<_>>()),
    );
    m
}

fn cpu_control_access_to_dbus(control: &CpuControlAccess) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert(
        dbus_keys::CPU_CONTROL_KIND_KEY.to_string(),
        ov(control.kind.as_str().to_string()),
    );
    m.insert(
        dbus_keys::CPU_CONTROL_STATUS_KEY.to_string(),
        ov(control.status.as_str().to_string()),
    );
    m.insert(
        dbus_keys::CPU_CONTROL_REASON_KEY.to_string(),
        ov(control.reason.clone()),
    );
    m.insert(
        dbus_keys::CPU_CONTROL_DIRECT_WRITE_KEY.to_string(),
        OwnedValue::from(control.direct_write),
    );
    m.insert(
        dbus_keys::CPU_CONTROL_PRIVILEGED_WRITE_KEY.to_string(),
        OwnedValue::from(control.privileged_write),
    );
    m.insert(
        dbus_keys::CPU_CONTROL_AUTHORIZATION_KEY.to_string(),
        ov(control.authorization.as_str().to_string()),
    );
    m.insert(
        dbus_keys::CPU_CONTROL_PATHS_KEY.to_string(),
        ov(control
            .paths
            .iter()
            .map(cpu_path_access_to_dbus)
            .collect::<Vec<_>>()),
    );
    m
}

fn setup_status_to_dbus(status: &SetupStatus) -> HashMap<String, OwnedValue> {
    let mut map = HashMap::new();
    map.insert(
        "checked_at_ms".to_string(),
        OwnedValue::from(status.checked_at_ms),
    );
    map.insert(
        dbus_keys::SETUP_DEPENDENCIES_KEY.to_string(),
        ov(status
            .dependencies
            .iter()
            .map(setup_dependency_to_dbus)
            .collect::<Vec<_>>()),
    );
    map.insert(
        dbus_keys::SETUP_PERMISSIONS_KEY.to_string(),
        ov(status
            .permissions
            .iter()
            .map(setup_permission_to_dbus)
            .collect::<Vec<_>>()),
    );
    map.insert(
        dbus_keys::SETUP_ISSUES_KEY.to_string(),
        ov(status
            .issues
            .iter()
            .map(setup_issue_to_dbus)
            .collect::<Vec<_>>()),
    );
    map
}

fn setup_dependency_to_dbus(status: &DependencyStatus) -> HashMap<String, OwnedValue> {
    let mut map = HashMap::new();
    map.insert(
        dbus_keys::SETUP_KIND_KEY.to_string(),
        ov(status.kind.as_str().to_string()),
    );
    map.insert(
        dbus_keys::SETUP_STATE_KEY.to_string(),
        ov(status.state.as_str().to_string()),
    );
    map.insert(
        dbus_keys::SETUP_SUMMARY_KEY.to_string(),
        ov(status.summary.clone()),
    );
    map.insert(
        dbus_keys::SETUP_REQUIRED_FOR_KEY.to_string(),
        ov(status.required_for.clone()),
    );
    map.insert(
        dbus_keys::SETUP_EVIDENCE_KEY.to_string(),
        ov(status.evidence.clone()),
    );
    map
}

fn setup_permission_to_dbus(status: &PermissionStatus) -> HashMap<String, OwnedValue> {
    let mut map = HashMap::new();
    map.insert(
        dbus_keys::SETUP_KIND_KEY.to_string(),
        ov(status.kind.as_str().to_string()),
    );
    map.insert(
        dbus_keys::SETUP_STATE_KEY.to_string(),
        ov(status.state.as_str().to_string()),
    );
    map.insert(
        dbus_keys::SETUP_SUMMARY_KEY.to_string(),
        ov(status.summary.clone()),
    );
    map.insert(
        dbus_keys::SETUP_PATHS_KEY.to_string(),
        ov(status.paths.clone()),
    );
    map
}

fn setup_issue_to_dbus(issue: &SetupIssue) -> HashMap<String, OwnedValue> {
    let mut map = HashMap::new();
    map.insert(
        dbus_keys::SETUP_SEVERITY_KEY.to_string(),
        ov(issue.severity.as_str().to_string()),
    );
    map.insert(
        dbus_keys::SETUP_TITLE_KEY.to_string(),
        ov(issue.title.clone()),
    );
    map.insert(
        dbus_keys::SETUP_SUMMARY_KEY.to_string(),
        ov(issue.summary.clone()),
    );
    map.insert(
        dbus_keys::SETUP_GUIDANCE_KEY.to_string(),
        ov(issue.guidance.clone()),
    );
    map
}

fn cpu_path_access_to_dbus(path: &CpuPathAccess) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert(
        dbus_keys::CPU_PATH_PATH_KEY.to_string(),
        ov(path.path.clone()),
    );
    m.insert(
        dbus_keys::CPU_PATH_READABLE_KEY.to_string(),
        OwnedValue::from(path.readable),
    );
    m.insert(
        dbus_keys::CPU_PATH_WRITABLE_KEY.to_string(),
        OwnedValue::from(path.writable),
    );
    m
}

fn cpu_to_dbus(c: &CpuTelemetry) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert("timestamp_ms".to_string(), OwnedValue::from(c.timestamp_ms));
    if let Some(v) = c.temp_c {
        m.insert("temp_c".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = c.usage_percent {
        m.insert("usage_percent".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = c.avg_freq_mhz {
        m.insert("avg_freq_mhz".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = c.package_power_w {
        m.insert("package_power_w".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = &c.status {
        m.insert("status".to_string(), ov(v.clone()));
    }
    if let Some(v) = c.turbo_boost_enabled {
        m.insert("turbo_boost_enabled".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = &c.governor {
        m.insert("governor".to_string(), ov(v.clone()));
    }
    if let Some(v) = &c.epp {
        m.insert("epp".to_string(), ov(v.clone()));
    }
    if let Some(v) = c.min_freq_mhz {
        m.insert("min_freq_mhz".to_string(), OwnedValue::from(v as u64));
    }
    if let Some(v) = c.max_freq_mhz {
        m.insert("max_freq_mhz".to_string(), OwnedValue::from(v as u64));
    }
    if !c.per_core.is_empty() {
        let mut rows: Vec<HashMap<String, OwnedValue>> = Vec::with_capacity(c.per_core.len());
        for core in &c.per_core {
            let mut row = HashMap::new();
            row.insert(
                dbus_keys::CPU_LOGICAL_CPU_ID_KEY.to_string(),
                OwnedValue::from(core.logical_cpu_id as u64),
            );
            // Compatibility alias for older clients that still expect "core_id".
            row.insert(
                dbus_keys::CPU_CORE_ID_COMPAT_KEY.to_string(),
                OwnedValue::from(core.logical_cpu_id as u64),
            );
            if let Some(v) = core.physical_core_index {
                row.insert(
                    dbus_keys::CPU_PHYSICAL_CORE_INDEX_KEY.to_string(),
                    OwnedValue::from(v as u64),
                );
            }
            if let Some(v) = core.policy_id {
                row.insert(
                    dbus_keys::CPU_POLICY_ID_KEY.to_string(),
                    OwnedValue::from(v as u64),
                );
            }
            if let Some(v) = core.thread_index {
                row.insert(
                    dbus_keys::CPU_THREAD_INDEX_KEY.to_string(),
                    OwnedValue::from(v as u64),
                );
            }
            if let Some(v) = core.thread_count {
                row.insert(
                    dbus_keys::CPU_THREAD_COUNT_KEY.to_string(),
                    OwnedValue::from(v as u64),
                );
            }
            if let Some(v) = core.usage_percent {
                row.insert("usage_percent".to_string(), OwnedValue::from(v as f64));
            }
            if let Some(v) = core.current_freq_mhz {
                row.insert("current_freq_mhz".to_string(), OwnedValue::from(v as u64));
            }
            if let Some(v) = core.min_freq_mhz {
                row.insert("min_freq_mhz".to_string(), OwnedValue::from(v as u64));
            }
            if let Some(v) = core.max_freq_mhz {
                row.insert("max_freq_mhz".to_string(), OwnedValue::from(v as u64));
            }
            row.insert("online".to_string(), OwnedValue::from(core.online));
            rows.push(row);
        }
        m.insert(dbus_keys::CPU_TELEMETRY_PER_CORE_KEY.to_string(), ov(rows));
    }
    m
}

fn telemetry_to_dbus(t: &TelemetrySnapshot) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert("timestamp_ms".to_string(), OwnedValue::from(t.timestamp_ms));

    if let Some(v) = t.cpu_temp_c {
        m.insert("cpu_temp_c".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.gpu_temp_c {
        m.insert("gpu_temp_c".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.gpu_usage_percent {
        m.insert("gpu_usage_percent".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.gpu_vram_used_bytes {
        m.insert("gpu_vram_used_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.gpu_vram_total_bytes {
        m.insert("gpu_vram_total_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.gpu_core_clock_mhz {
        m.insert("gpu_core_clock_mhz".to_string(), OwnedValue::from(v as u64));
    }
    if let Some(v) = t.gpu_memory_clock_mhz {
        m.insert(
            "gpu_memory_clock_mhz".to_string(),
            OwnedValue::from(v as u64),
        );
    }
    if let Some(v) = t.gpu_power_w {
        m.insert("gpu_power_w".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.gpu_index {
        m.insert("gpu_index".to_string(), OwnedValue::from(v));
    }
    for (key, value) in [
        ("gpu_name", t.gpu_name.as_ref()),
        ("gpu_uuid", t.gpu_uuid.as_ref()),
        ("gpu_pci_bus_id", t.gpu_pci_bus_id.as_ref()),
        ("gpu_telemetry_provider", t.gpu_telemetry_provider.as_ref()),
        ("gpu_telemetry_status", t.gpu_telemetry_status.as_ref()),
    ] {
        if let Some(value) = value {
            m.insert(key.to_string(), ov(value.clone()));
        }
    }
    if !t.temps_c.is_empty() {
        let temps: HashMap<String, f64> = t
            .temps_c
            .iter()
            .map(|(k, v)| (k.clone(), *v as f64))
            .collect();
        m.insert("temps_c".to_string(), OwnedValue::from(temps));
    }
    if !t.fans_rpm.is_empty() {
        let fans: HashMap<String, u32> = t.fans_rpm.iter().map(|(k, v)| (k.clone(), *v)).collect();
        m.insert("fans_rpm".to_string(), OwnedValue::from(fans));
    }
    if !t.fan_rows.is_empty() {
        m.insert(
            dbus_keys::TELEMETRY_FAN_ROWS_KEY.to_string(),
            ov(t.fan_rows.iter().map(fan_row_to_dbus).collect::<Vec<_>>()),
        );
    }
    if let Some(ps) = t.power_source {
        m.insert("power_source".to_string(), ov(power_source_to_str(ps)));
    }
    if let Some(v) = t.battery_percent {
        m.insert("battery_percent".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.ac_online {
        m.insert("ac_online".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.battery_state {
        m.insert("battery_state".to_string(), ov(battery_state_to_str(v)));
    }
    if let Some(v) = t.battery_health_percent {
        m.insert(
            "battery_health_percent".to_string(),
            OwnedValue::from(v as f64),
        );
    }
    if let Some(v) = t.battery_cycle_count {
        m.insert("battery_cycle_count".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.battery_charge_power_w {
        m.insert(
            "battery_charge_power_w".to_string(),
            OwnedValue::from(v as f64),
        );
    }
    if let Some(v) = t.battery_discharge_power_w {
        m.insert(
            "battery_discharge_power_w".to_string(),
            OwnedValue::from(v as f64),
        );
    }
    if let Some(v) = t.battery_time_to_empty_s {
        m.insert("battery_time_to_empty_s".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.battery_time_to_full_s {
        m.insert("battery_time_to_full_s".to_string(), OwnedValue::from(v));
    }

    // Memory.
    if let Some(v) = t.mem_total_bytes {
        m.insert("mem_total_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_used_bytes {
        m.insert("mem_used_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_used_percent {
        m.insert("mem_used_percent".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.mem_available_bytes {
        m.insert("mem_available_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_free_bytes {
        m.insert("mem_free_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_cached_bytes {
        m.insert("mem_cached_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_buffers_bytes {
        m.insert("mem_buffers_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_shared_bytes {
        m.insert("mem_shared_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_anon_bytes {
        m.insert("mem_anon_bytes".to_string(), OwnedValue::from(v));
    }

    if let Some(v) = t.swap_total_bytes {
        m.insert("swap_total_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.swap_used_bytes {
        m.insert("swap_used_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.swap_free_bytes {
        m.insert("swap_free_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.swap_in_pages_per_s {
        m.insert(
            "swap_in_pages_per_s".to_string(),
            OwnedValue::from(v as f64),
        );
    }
    if let Some(v) = t.swap_out_pages_per_s {
        m.insert(
            "swap_out_pages_per_s".to_string(),
            OwnedValue::from(v as f64),
        );
    }
    if let Some(v) = t.zram_total_bytes {
        m.insert("zram_total_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.zram_used_bytes {
        m.insert("zram_used_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.zswap_enabled {
        m.insert("zswap_enabled".to_string(), OwnedValue::from(v));
    }

    if let Some(v) = t.psi_mem_some_avg10 {
        m.insert("psi_mem_some_avg10".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.psi_mem_some_avg60 {
        m.insert("psi_mem_some_avg60".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.psi_mem_some_avg300 {
        m.insert(
            "psi_mem_some_avg300".to_string(),
            OwnedValue::from(v as f64),
        );
    }
    if let Some(v) = t.psi_mem_full_avg10 {
        m.insert("psi_mem_full_avg10".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.psi_mem_full_avg60 {
        m.insert("psi_mem_full_avg60".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.psi_mem_full_avg300 {
        m.insert(
            "psi_mem_full_avg300".to_string(),
            OwnedValue::from(v as f64),
        );
    }

    if let Some(v) = t.mem_active_bytes {
        m.insert("mem_active_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_inactive_bytes {
        m.insert("mem_inactive_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_dirty_bytes {
        m.insert("mem_dirty_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_writeback_bytes {
        m.insert("mem_writeback_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_slab_bytes {
        m.insert("mem_slab_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_sreclaimable_bytes {
        m.insert("mem_sreclaimable_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_sunreclaim_bytes {
        m.insert("mem_sunreclaim_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_pagetables_bytes {
        m.insert("mem_pagetables_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_kernelstack_bytes {
        m.insert("mem_kernelstack_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_mapped_bytes {
        m.insert("mem_mapped_bytes".to_string(), OwnedValue::from(v));
    }

    if let Some(ref v) = t.mem_top_processes {
        m.insert("mem_top_processes".to_string(), ov(v.clone()));
    }
    if let Some(ref rows) = t.mem_top_processes_rows {
        let mut out_rows: Vec<HashMap<String, OwnedValue>> = Vec::with_capacity(rows.len());
        for p in rows {
            let mut r = HashMap::new();
            r.insert("user".to_string(), ov(p.user.clone()));
            r.insert("pid".to_string(), OwnedValue::from(p.pid));
            r.insert("rss_bytes".to_string(), OwnedValue::from(p.rss_bytes));
            r.insert("swap_bytes".to_string(), OwnedValue::from(p.swap_bytes));
            r.insert("name".to_string(), ov(p.name.clone()));
            out_rows.push(r);
        }
        m.insert("mem_top_processes_rows".to_string(), ov(out_rows));
    }
    m
}

fn fan_row_to_dbus(fan: &FanTelemetry) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert(
        dbus_keys::FAN_ROW_HWMON_DEVICE_KEY.to_string(),
        ov(fan.hwmon_device.clone()),
    );
    m.insert(
        dbus_keys::FAN_ROW_HWMON_PATH_KEY.to_string(),
        ov(fan.hwmon_path.clone()),
    );
    m.insert(
        dbus_keys::FAN_ROW_INPUT_PATH_KEY.to_string(),
        ov(fan.input_path.clone()),
    );
    if let Some(raw_label) = &fan.raw_label {
        m.insert(
            dbus_keys::FAN_ROW_RAW_LABEL_KEY.to_string(),
            ov(raw_label.clone()),
        );
    }
    m.insert(
        dbus_keys::FAN_ROW_DISPLAY_LABEL_KEY.to_string(),
        ov(fan.display_label.clone()),
    );
    if let Some(rpm) = fan.rpm {
        m.insert(
            dbus_keys::FAN_ROW_RPM_KEY.to_string(),
            OwnedValue::from(rpm),
        );
    }
    m
}

fn power_source_to_str(p: PowerSource) -> &'static str {
    match p {
        PowerSource::Ac => "Ac",
        PowerSource::Battery => "Battery",
    }
}

fn battery_state_to_str(s: BatteryState) -> &'static str {
    match s {
        BatteryState::Unknown => "Unknown",
        BatteryState::Charging => "Charging",
        BatteryState::Discharging => "Discharging",
        BatteryState::Empty => "Empty",
        BatteryState::Full => "Full",
        BatteryState::PendingCharge => "PendingCharge",
        BatteryState::PendingDischarge => "PendingDischarge",
        BatteryState::NotCharging => "NotCharging",
    }
}

fn profile_to_str(profile: PerformanceProfile) -> &'static str {
    match profile {
        PerformanceProfile::Silent => "Silent",
        PerformanceProfile::Balanced => "Balanced",
        PerformanceProfile::Turbo => "Turbo",
        PerformanceProfile::Custom(_) => "Custom",
    }
}

fn gpu_mode_to_str(mode: GpuMode) -> &'static str {
    match mode {
        GpuMode::Integrated => "Integrated",
        GpuMode::Hybrid => "Hybrid",
        GpuMode::Dedicated => "Dedicated",
        GpuMode::Other(_) => "Other",
    }
}

fn parse_profile_request(v: &str) -> Result<PerformanceProfile, String> {
    let key: String = v
        .to_ascii_lowercase()
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect();

    match key.as_str() {
        "silent" | "quiet" | "lowpower" => Ok(PerformanceProfile::Silent),
        "balanced" => Ok(PerformanceProfile::Balanced),
        "turbo" | "performance" => Ok(PerformanceProfile::Turbo),
        "custom" => Ok(PerformanceProfile::Custom("Custom".to_string())),
        _ => Err(format!(
            "unknown profile '{v}', expected Silent|Balanced|Turbo"
        )),
    }
}

fn parse_gpu_mode_request(v: &str) -> Result<GpuMode, String> {
    let key: String = v
        .to_ascii_lowercase()
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect();

    match key.as_str() {
        "integrated" => Ok(GpuMode::Integrated),
        "hybrid" | "dynamic" | "optimus" => Ok(GpuMode::Hybrid),
        "dedicated" | "discrete" | "asusmuxdgpu" | "asusegpu" | "vfio" => Ok(GpuMode::Dedicated),
        "other" => Ok(GpuMode::Other("Other".to_string())),
        _ => Err(format!(
            "unknown GPU mode '{v}', expected Integrated|Hybrid|Dedicated"
        )),
    }
}

fn cpu_access_warning(caps: &CpuCaps) -> Option<String> {
    let blocked = caps
        .control_access
        .iter()
        .filter(|control| {
            matches!(
                control.status,
                CpuAccessState::PermissionDenied
                    | CpuAccessState::AuthorizationRequired
                    | CpuAccessState::AuthorizationDenied
                    | CpuAccessState::HelperMissing
                    | CpuAccessState::ReadOnly
                    | CpuAccessState::MissingBackend
                    | CpuAccessState::TemporarilyUnavailable
            )
        })
        .collect::<Vec<_>>();

    if blocked.is_empty() {
        return None;
    }

    if blocked
        .iter()
        .any(|control| control.status == CpuAccessState::AuthorizationRequired)
    {
        return Some(
            "Administrator access is required for some supported CPU controls; authorization occurs only when Apply is used."
                .to_string(),
        );
    }

    if blocked
        .iter()
        .any(|control| control.status == CpuAccessState::AuthorizationDenied)
    {
        return Some(
            "Administrator authorization was denied or cancelled for a CPU control. Apply again to retry."
                .to_string(),
        );
    }

    if blocked
        .iter()
        .any(|control| control.status == CpuAccessState::HelperMissing)
    {
        return Some(
            "Some supported CPU controls need rog-helper-privileged, but the helper is unavailable."
                .to_string(),
        );
    }

    if blocked
        .iter()
        .all(|control| control.status == CpuAccessState::PermissionDenied)
    {
        return Some(
            "CPU write access is blocked: detected CPU sysfs control paths are readable but not writable by the current user."
                .to_string(),
        );
    }

    if blocked
        .iter()
        .any(|control| control.status == CpuAccessState::MissingBackend)
    {
        return Some(
            "Some CPU controls are unavailable because the required sysfs backend was not detected."
                .to_string(),
        );
    }

    if blocked
        .iter()
        .any(|control| control.status == CpuAccessState::TemporarilyUnavailable)
    {
        return Some(
            "Some CPU controls are temporarily unavailable; inspect CPU diagnostics for the affected sysfs paths."
                .to_string(),
        );
    }

    None
}

fn apply_cpu_privilege_to_caps(caps: &mut CpuCaps, privilege: &CpuPrivilegeState) {
    let helper_ready = privilege.status.privileged_helper_reachable
        && privilege.status.privileged_helper_compatible
        && privilege.status.polkit_available
        && privilege
            .status
            .privileged_categories_available
            .contains(&rog_core::PrivilegedCategory::Cpu);
    for control in &mut caps.control_access {
        control.direct_write = control.status == CpuAccessState::Available;
        if control.direct_write {
            control.privileged_write = false;
            control.authorization = CpuAuthorization::NotRequired;
            continue;
        }
        if control.status != CpuAccessState::PermissionDenied {
            continue;
        }
        if helper_ready {
            control.privileged_write = true;
            control.authorization = privilege.authorization;
            control.status = if privilege.authorization == CpuAuthorization::Denied {
                CpuAccessState::AuthorizationDenied
            } else {
                CpuAccessState::AuthorizationRequired
            };
            control.reason = if privilege.authorization == CpuAuthorization::Denied {
                "Administrator authorization was denied or cancelled. Apply to retry.".to_string()
            } else {
                "Administrator access is required. Apply to authenticate.".to_string()
            };
        } else if !privilege.status.privileged_helper_installed
            || !privilege.status.privileged_helper_reachable
        {
            control.status = CpuAccessState::HelperMissing;
            control.authorization = CpuAuthorization::Unavailable;
            control.reason =
                "The privileged CPU helper is not installed or cannot be reached.".to_string();
        } else {
            control.status = CpuAccessState::ReadOnly;
            control.authorization = CpuAuthorization::Unavailable;
            control.reason = "The CPU control is supported, but no compatible authorized write route is available."
                .to_string();
        }
    }
    caps.policy_writable = caps
        .control_access
        .iter()
        .any(|control| control.status.is_actionable());
}

fn cpu_diagnostics_text(caps: &CpuCaps, cpu: &CpuTelemetry) -> String {
    let mut out = String::new();
    out.push_str("CPU Diagnostics\n");
    out.push_str("================\n");
    out.push_str(&format!("has_cpufreq: {}\n", caps.has_cpufreq));
    out.push_str(&format!("has_governor: {}\n", caps.has_governor));
    out.push_str(&format!("has_epp: {}\n", caps.has_epp));
    out.push_str(&format!("has_boost_toggle: {}\n", caps.has_boost_toggle));
    out.push_str(&format!("has_package_power: {}\n", caps.has_package_power));
    out.push_str(&format!("policy_writable: {}\n", caps.policy_writable));
    out.push_str(&format!("cpu_count (physical cores): {}\n", caps.cpu_count));
    out.push_str(&format!(
        "thread_count (logical threads): {}\n",
        caps.thread_count
    ));
    if let Some(driver) = &caps.scaling_driver {
        out.push_str(&format!("scaling_driver: {driver}\n"));
    }
    if !caps.governor_choices.is_empty() {
        out.push_str(&format!(
            "governor_choices: {}\n",
            caps.governor_choices.join(", ")
        ));
    }
    if !caps.epp_choices.is_empty() {
        out.push_str(&format!("epp_choices: {}\n", caps.epp_choices.join(", ")));
    }
    if !caps.control_access.is_empty() {
        out.push_str("\nWrite Access\n");
        out.push_str("------------\n");
        for control in &caps.control_access {
            let supported = !matches!(
                control.status,
                CpuAccessState::Unknown
                    | CpuAccessState::Unsupported
                    | CpuAccessState::MissingBackend
            );
            out.push_str(&format!("{}:\n", control.kind.as_str()));
            out.push_str(&format!("  supported: {supported}\n"));
            out.push_str(&format!("  direct_write: {}\n", control.direct_write));
            out.push_str(&format!(
                "  privileged_write: {}\n",
                control.privileged_write
            ));
            out.push_str(&format!(
                "  authorization: {}\n",
                control.authorization.as_str()
            ));
            out.push_str(&format!("  status: {}", control.status.as_str()));
            if !control.reason.is_empty() {
                out.push_str(&format!(" ({})", control.reason));
            }
            out.push('\n');
            for path in &control.paths {
                out.push_str(&format!(
                    "  [{}{}] {}\n",
                    if path.readable { "r" } else { "-" },
                    if path.writable { "w" } else { "-" },
                    path.path
                ));
            }
        }
        out.push_str("\nSuggested checks\n");
        out.push_str("----------------\n");
        out.push_str("  ls -l /sys/devices/system/cpu/cpufreq/policy*/scaling_governor\n");
        out.push_str(
            "  ls -l /sys/devices/system/cpu/cpufreq/policy*/energy_performance_preference\n",
        );
        out.push_str("  ls -l /sys/devices/system/cpu/cpufreq/policy*/scaling_{min,max}_freq\n");
        out.push_str("  ls -l /sys/devices/system/cpu/intel_pstate/no_turbo\n");
        out.push_str("  ls -l /sys/devices/system/cpu/cpu*/online\n");
        out.push_str("  systemctl --user restart rog-helperd\n");
    }

    out.push_str("\nCurrent\n");
    out.push_str("-------\n");
    out.push_str(&format!("temp_c: {:?}\n", cpu.temp_c));
    out.push_str(&format!("usage_percent: {:?}\n", cpu.usage_percent));
    out.push_str(&format!("avg_freq_mhz: {:?}\n", cpu.avg_freq_mhz));
    out.push_str(&format!("package_power_w: {:?}\n", cpu.package_power_w));
    out.push_str(&format!("status: {:?}\n", cpu.status));
    out.push_str(&format!(
        "turbo_boost_enabled: {:?}\n",
        cpu.turbo_boost_enabled
    ));
    out.push_str(&format!("governor: {:?}\n", cpu.governor));
    out.push_str(&format!("epp: {:?}\n", cpu.epp));
    out.push_str(&format!("min_freq_mhz: {:?}\n", cpu.min_freq_mhz));
    out.push_str(&format!("max_freq_mhz: {:?}\n", cpu.max_freq_mhz));

    out.push_str("\nLogical CPUs / Threads\n");
    out.push_str("----------------------\n");
    out.push_str("cpu  pcore  thr   pol   online  usage%  cur_mhz  min_mhz  max_mhz\n");
    for core in &cpu.per_core {
        let thread_label = match (core.thread_index, core.thread_count) {
            (Some(index), Some(count)) => format!("{index}/{count}"),
            _ => "-".to_string(),
        };
        out.push_str(&format!(
            "{:<3}  {:<5}  {:<4}  {:<4}  {:<6}  {:<6}  {:<7}  {:<7}  {:<7}\n",
            core.logical_cpu_id,
            core.physical_core_index
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
            thread_label,
            core.policy_id
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
            if core.online { "yes" } else { "no" },
            core.usage_percent
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".to_string()),
            core.current_freq_mhz
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
            core.min_freq_mhz
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
            core.max_freq_mhz
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ));
    }

    if !caps.sysfs_paths.is_empty() {
        out.push_str("\nDetected sysfs paths\n");
        out.push_str("-------------------\n");
        for p in &caps.sysfs_paths {
            out.push_str(&format!("{p}\n"));
        }
    }

    out
}

fn format_top_processes_text(rows: &[rog_core::TopProcessMem]) -> String {
    let mut out = String::new();
    out.push_str("USER        PID      RSS     SWAP  NAME\n");
    out.push_str("----------------------------------------\n");
    for p in rows {
        out.push_str(&format!(
            "{user:<10} {pid:>6} {rss:>8} {swap:>8}  {name}\n",
            user = p.user,
            pid = p.pid,
            rss = format_bytes_short(p.rss_bytes),
            swap = format_bytes_short(p.swap_bytes),
            name = p.name
        ));
    }
    out
}

fn format_bytes_short(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

    let b = bytes as f64;
    if b >= 10.0 * GIB {
        format!("{:.0}G", b / GIB)
    } else if b >= GIB {
        format!("{:.1}G", b / GIB)
    } else if b >= 10.0 * MIB {
        format!("{:.0}M", b / MIB)
    } else if b >= MIB {
        format!("{:.1}M", b / MIB)
    } else if b >= 10.0 * KIB {
        format!("{:.0}K", b / KIB)
    } else if b >= KIB {
        format!("{:.1}K", b / KIB)
    } else {
        format!("{bytes}B")
    }
}

fn ov<T>(v: T) -> OwnedValue
where
    T: Into<Value<'static>>,
{
    OwnedValue::try_from(v.into()).expect("OwnedValue conversion should succeed")
}

fn u64_from_map(map: &HashMap<String, OwnedValue>, key: &str) -> Option<u64> {
    let v = map.get(key)?;
    u64::try_from(v)
        .ok()
        .or_else(|| u32::try_from(v).ok().map(|v| v as u64))
}

fn fan_curve_from_dbus(
    fan_id: &str,
    map: &HashMap<String, OwnedValue>,
) -> rog_core::RogResult<FanCurve> {
    let rows = map
        .get(dbus_keys::fan_curves::POINTS)
        .cloned()
        .and_then(|value| Vec::<HashMap<String, OwnedValue>>::try_from(value).ok())
        .ok_or_else(|| {
            rog_core::RogError::InvalidInput("fan curve requires a points array".to_string())
        })?;
    let mut points = Vec::with_capacity(rows.len());
    for row in rows {
        let temp_c = u64_from_map(&row, dbus_keys::fan_curves::TEMP_C)
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| {
                rog_core::RogError::InvalidInput("fan curve point missing temp_c".to_string())
            })?;
        let duty_percent = u64_from_map(&row, dbus_keys::fan_curves::SPEED_PERCENT)
            .or_else(|| u64_from_map(&row, dbus_keys::fan_curves::DUTY_PERCENT_COMPAT))
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| {
                rog_core::RogError::InvalidInput(
                    "fan curve point missing speed_percent".to_string(),
                )
            })?;
        points.push(FanPoint {
            temp_c,
            duty_percent,
        });
    }
    let curve = FanCurve {
        domain: FanDomain::Other(fan_id.to_string()),
        points,
    };
    curve.validate_safe(rog_core::FanCurvePolicy::default())?;
    Ok(curve)
}

fn map_rog_error_to_fdo(e: rog_core::RogError) -> fdo::Error {
    match e {
        rog_core::RogError::DependencyMissing(msg) => fdo::Error::Failed(msg),
        rog_core::RogError::NotSupported(msg) => fdo::Error::NotSupported(msg),
        rog_core::RogError::PermissionDenied(msg) => fdo::Error::AccessDenied(msg),
        rog_core::RogError::InvalidInput(msg) => fdo::Error::InvalidArgs(msg),
        rog_core::RogError::TransientFailure(msg) => fdo::Error::TimedOut(msg),
        rog_core::RogError::TemporarilyUnavailable(msg) => fdo::Error::Failed(msg),
        rog_core::RogError::Unexpected(msg) => fdo::Error::Failed(msg),
    }
}

fn privileged_status_to_dbus(status: &rog_core::PrivilegedStatus) -> HashMap<String, OwnedValue> {
    use dbus_keys::privileged_status as keys;

    let mut map = HashMap::new();
    map.insert(
        keys::HELPER_INSTALLED.to_string(),
        OwnedValue::from(status.privileged_helper_installed),
    );
    map.insert(
        keys::HELPER_REACHABLE.to_string(),
        OwnedValue::from(status.privileged_helper_reachable),
    );
    map.insert(
        keys::HELPER_COMPATIBLE.to_string(),
        OwnedValue::from(status.privileged_helper_compatible),
    );
    map.insert(
        keys::HELPER_VERSION.to_string(),
        ov(status.privileged_helper_version.clone().unwrap_or_default()),
    );
    map.insert(
        keys::POLKIT_AVAILABLE.to_string(),
        OwnedValue::from(status.polkit_available),
    );
    map.insert(
        keys::AUTHORIZATION_BACKEND.to_string(),
        ov(status.authorization_backend.clone()),
    );
    map.insert(
        keys::AUTHORIZATION_STATE.to_string(),
        ov(status.authorization_state.as_str().to_string()),
    );
    map.insert(
        keys::CATEGORIES_AVAILABLE.to_string(),
        ov(status
            .privileged_categories_available
            .iter()
            .map(|category| category.as_str().to_string())
            .collect::<Vec<_>>()),
    );
    map
}

impl RogHelperDaemon {
    async fn lighting_to_dbus(&self) -> Option<HashMap<String, OwnedValue>> {
        if let Some(aura) = &self.aura {
            match aura.read_state().await {
                Ok(mut state) => {
                    self.merge_sysfs_brightness(&mut state);
                    let diagnostics = self.lighting_diagnostics(Some(&state), None);
                    return Some(lighting_state_to_dbus(&state, &diagnostics));
                }
                Err(e) => {
                    let error = e.to_string();
                    let diagnostics = self.lighting_diagnostics(None, Some(error.as_str()));
                    if let Some(mut state) = self.sysfs_lighting_state() {
                        state.status = "aura_backend_error".to_string();
                        state.last_error = Some(format!("Aura backend read failed: {error}"));
                        return Some(lighting_state_to_dbus(&state, &diagnostics));
                    }
                    return Some(lighting_state_to_dbus(
                        &LightingState {
                            backend: "asusd-aura".to_string(),
                            device: aura.endpoint_tag(),
                            brightness: None,
                            max_brightness: None,
                            mode: None,
                            supports_brightness: aura.supports_brightness(),
                            supports_modes: aura.can_set_mode()
                                || !aura.supported_modes_hint().is_empty(),
                            supported_modes: aura.supported_modes_hint(),
                            supports_rgb: aura.supports_rgb(),
                            supports_speed: aura.supports_speed(),
                            supported_speeds: Vec::new(),
                            supported_zones: aura.supported_zones(),
                            writable: aura.supports_rgb()
                                || aura.can_set_mode()
                                || aura.can_set_brightness(),
                            rgb: None,
                            status: "backend_error".to_string(),
                            last_error: Some(format!("Aura backend read failed: {error}")),
                        },
                        &diagnostics,
                    ));
                }
            }
        }

        self.sysfs_lighting_state().map(|state| {
            let diagnostics = self.lighting_diagnostics(Some(&state), None);
            lighting_state_to_dbus(&state, &diagnostics)
        })
    }

    fn lighting_diagnostics(
        &self,
        state: Option<&LightingState>,
        aura_state_error: Option<&str>,
    ) -> LightingDiagnostics {
        build_lighting_diagnostics(
            self.kbd_backlight.as_ref(),
            self.kbd_backlight_detected,
            self.kbd_backlight_probe_error.as_deref(),
            self.aura.as_ref(),
            &self.aura_probe_diagnostics,
            state,
            aura_state_error,
        )
    }

    fn merge_sysfs_brightness(&self, state: &mut LightingState) {
        if state.brightness.is_some() && state.max_brightness.is_some() {
            return;
        }
        let Some(kbd) = &self.kbd_backlight else {
            return;
        };
        if state.brightness.is_none() {
            state.brightness = kbd.read_brightness().ok();
        }
        if state.max_brightness.is_none() {
            state.max_brightness = Some(kbd.max_brightness());
        }
        state.writable |= kbd.can_set_brightness();
    }

    fn sysfs_lighting_state(&self) -> Option<LightingState> {
        let kbd = self.kbd_backlight.as_ref()?;
        let brightness = kbd.read_brightness().ok()?;
        let mode = if brightness == 0 {
            LightingMode::Off
        } else {
            LightingMode::Static
        };

        Some(LightingState {
            backend: "sysfs-led".to_string(),
            device: kbd.led_name().to_string(),
            brightness: Some(brightness),
            max_brightness: Some(kbd.max_brightness()),
            mode: Some(mode),
            supports_brightness: true,
            supports_modes: true,
            supported_modes: vec![LightingMode::Off, LightingMode::Static],
            supports_rgb: false,
            rgb: None,
            supports_speed: false,
            supported_speeds: Vec::new(),
            supported_zones: Vec::new(),
            writable: kbd.can_set_brightness(),
            status: "rgb_unsupported".to_string(),
            last_error: None,
        })
    }
}

fn lighting_state_to_dbus(
    state: &LightingState,
    diagnostics: &LightingDiagnostics,
) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert(
        dbus_keys::lighting::BACKEND.to_string(),
        ov(state.backend.clone()),
    );
    m.insert(
        dbus_keys::lighting::DEVICE.to_string(),
        ov(state.device.clone()),
    );
    m.insert(
        dbus_keys::lighting::BRIGHTNESS.to_string(),
        OwnedValue::from(state.brightness.unwrap_or(0) as u64),
    );
    m.insert(
        dbus_keys::lighting::MAX_BRIGHTNESS.to_string(),
        OwnedValue::from(state.max_brightness.unwrap_or(0) as u64),
    );
    m.insert(
        dbus_keys::lighting::CAN_SET.to_string(),
        OwnedValue::from(state.writable),
    );
    m.insert(
        dbus_keys::lighting::WRITABLE.to_string(),
        OwnedValue::from(state.writable),
    );
    if let Some(mode) = state.mode_label() {
        m.insert(dbus_keys::lighting::MODE.to_string(), ov(mode));
    }
    m.insert(
        dbus_keys::lighting::SUPPORTED_MODES.to_string(),
        ov(state.supported_mode_labels()),
    );
    m.insert(
        dbus_keys::lighting::SUPPORTS_BRIGHTNESS.to_string(),
        OwnedValue::from(state.supports_brightness),
    );
    m.insert(
        dbus_keys::lighting::SUPPORTS_MODES.to_string(),
        OwnedValue::from(state.supports_modes),
    );
    m.insert(
        dbus_keys::lighting::SUPPORTS_RGB.to_string(),
        OwnedValue::from(state.supports_rgb),
    );
    if let Some(rgb) = state.rgb {
        m.insert(dbus_keys::lighting::RGB_HEX.to_string(), ov(rgb.to_hex()));
    }
    m.insert(
        dbus_keys::lighting::SUPPORTS_SPEED.to_string(),
        OwnedValue::from(state.supports_speed),
    );
    m.insert(
        dbus_keys::lighting::SUPPORTED_SPEEDS.to_string(),
        ov(state.supported_speeds.clone()),
    );
    m.insert(
        dbus_keys::lighting::SUPPORTED_ZONES.to_string(),
        ov(state.supported_zones.clone()),
    );
    m.insert(
        dbus_keys::lighting::STATUS.to_string(),
        ov(state.status.clone()),
    );
    if let Some(error) = &state.last_error {
        m.insert(
            dbus_keys::lighting::LAST_ERROR.to_string(),
            ov(error.clone()),
        );
    }
    m.insert(
        dbus_keys::lighting::DIAGNOSTICS_SUMMARY.to_string(),
        ov(diagnostics.summary_line()),
    );
    m.insert(
        dbus_keys::lighting::DIAGNOSTICS_DETAILS.to_string(),
        ov(diagnostics.to_report_text()),
    );
    if let Some(reason) = &diagnostics.fallback_reason {
        m.insert(
            dbus_keys::lighting::FALLBACK_REASON.to_string(),
            ov(reason.clone()),
        );
    }
    if let Some(reason) = &diagnostics.unavailable_reason {
        m.insert(
            dbus_keys::lighting::UNAVAILABLE_REASON.to_string(),
            ov(reason.clone()),
        );
    }
    if let Some(warning) = &diagnostics.permission_warning {
        m.insert(
            dbus_keys::lighting::PERMISSION_WARNING.to_string(),
            ov(warning.clone()),
        );
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value_as_str(map: &HashMap<String, OwnedValue>, key: &str) -> String {
        map.get(key)
            .and_then(|value| <&str>::try_from(value).ok())
            .unwrap_or_default()
            .to_string()
    }

    fn rows_from_value(value: &OwnedValue) -> Vec<HashMap<String, OwnedValue>> {
        Vec::<HashMap<String, OwnedValue>>::try_from(value.clone()).expect("rows should decode")
    }

    #[test]
    fn privileged_status_map_exposes_required_diagnostics() {
        use dbus_keys::privileged_status as keys;

        let status = rog_core::PrivilegedStatus {
            privileged_helper_installed: true,
            privileged_helper_reachable: true,
            privileged_helper_compatible: true,
            privileged_helper_version: Some("0.2.2".to_string()),
            polkit_available: true,
            authorization_backend: "polkit".to_string(),
            authorization_state: rog_core::AuthorizationState::Denied,
            privileged_categories_available: vec![rog_core::PrivilegedCategory::Cpu],
        };
        let map = privileged_status_to_dbus(&status);

        assert_eq!(
            map.get(keys::HELPER_INSTALLED)
                .and_then(|value| bool::try_from(value).ok()),
            Some(true)
        );
        assert_eq!(value_as_str(&map, keys::AUTHORIZATION_STATE), "denied");
        assert_eq!(
            map.get(keys::CATEGORIES_AVAILABLE)
                .cloned()
                .and_then(|value| Vec::<String>::try_from(value).ok()),
            Some(vec!["cpu".to_string()])
        );
    }

    #[test]
    fn caps_to_dbus_emits_feature_access_status_and_reason_keys() {
        let mut caps = DeviceCaps::unknown();
        caps.profile_access = FeatureAvailability::new(
            FeatureAccessState::MissingBackend,
            "Install and start asusd to enable performance profiles.",
        );
        caps.charge_limit_access = FeatureAvailability::new(
            FeatureAccessState::TemporarilyUnavailable,
            "Charge-limit control needs asusd, but the system backend could not be reached right now.",
        );
        caps.gpu_mode_access = FeatureAvailability::new(
            FeatureAccessState::Unsupported,
            "supergfxd is available, but this machine does not expose switchable GPU modes.",
        );
        caps.kbd_backlight_access = FeatureAvailability::new(
            FeatureAccessState::PermissionDenied,
            "Keyboard backlight is detected, but writes are blocked for the current user.",
        );

        let map = caps_to_dbus(&caps);

        for (prefix, status, reason) in [
            (
                dbus_keys::PROFILE_ACCESS_PREFIX,
                "missing_backend",
                "Install and start asusd to enable performance profiles.",
            ),
            (
                dbus_keys::CHARGE_LIMIT_ACCESS_PREFIX,
                "temporarily_unavailable",
                "Charge-limit control needs asusd, but the system backend could not be reached right now.",
            ),
            (
                dbus_keys::GPU_MODE_ACCESS_PREFIX,
                "unsupported",
                "supergfxd is available, but this machine does not expose switchable GPU modes.",
            ),
            (
                dbus_keys::KBD_BACKLIGHT_ACCESS_PREFIX,
                "permission_denied",
                "Keyboard backlight is detected, but writes are blocked for the current user.",
            ),
        ] {
            assert_eq!(
                value_as_str(&map, &dbus_keys::feature_access_status_key(prefix)),
                status
            );
            assert_eq!(
                value_as_str(&map, &dbus_keys::feature_access_reason_key(prefix)),
                reason
            );
        }
    }

    #[test]
    fn fan_caps_to_dbus_keeps_read_and_write_capabilities_separate() {
        let mut caps = FanCaps::from_fans(&[]);
        caps.fan_curve_readable = true;
        caps.fan_curve_writable = false;
        caps.fan_mapping_confidence = rog_core::FanMappingConfidence::HardwareLabel;

        let map = fan_caps_to_dbus(&caps);

        assert_eq!(
            map.get("fan_curve_readable")
                .and_then(|value| bool::try_from(value).ok()),
            Some(true)
        );
        assert_eq!(
            map.get("fan_curve_writable")
                .and_then(|value| bool::try_from(value).ok()),
            Some(false)
        );
        assert_eq!(
            value_as_str(&map, "fan_mapping_confidence"),
            "hardware_label"
        );
    }

    #[test]
    fn setup_status_to_dbus_emits_structured_rows() {
        let status = SetupStatus {
            checked_at_ms: 42,
            dependencies: vec![rog_core::DependencyStatus {
                kind: rog_core::DependencyKind::Asusd,
                state: rog_core::DependencyState::NotAvailable,
                summary: "No compatible API found.".to_string(),
                required_for: vec!["performance profiles".to_string()],
                evidence: vec!["systemd unit not found".to_string()],
            }],
            permissions: vec![rog_core::PermissionStatus {
                kind: rog_core::PermissionKind::CpuPolicyWrites,
                state: rog_core::PermissionState::ReadOnly,
                summary: "Writes blocked.".to_string(),
                paths: vec!["/sys/example".to_string()],
            }],
            issues: vec![rog_core::SetupIssue {
                severity: rog_core::SetupSeverity::Warning,
                title: "asusd needs attention".to_string(),
                summary: "Missing service.".to_string(),
                guidance: "See installation documentation.".to_string(),
            }],
        };

        let map = setup_status_to_dbus(&status);
        assert_eq!(
            map.get("checked_at_ms")
                .and_then(|value| u64::try_from(value).ok()),
            Some(42)
        );
        assert_eq!(
            rows_from_value(&map[dbus_keys::SETUP_DEPENDENCIES_KEY]).len(),
            1
        );
        assert_eq!(
            rows_from_value(&map[dbus_keys::SETUP_PERMISSIONS_KEY]).len(),
            1
        );
        assert_eq!(rows_from_value(&map[dbus_keys::SETUP_ISSUES_KEY]).len(), 1);
    }

    #[test]
    fn telemetry_to_dbus_emits_structured_fan_rows() {
        let mut telemetry = TelemetrySnapshot::empty_now(42);
        telemetry.gpu_usage_percent = Some(32.0);
        telemetry.gpu_vram_used_bytes = Some(2_147_483_648);
        telemetry.gpu_vram_total_bytes = Some(8_589_934_592);
        telemetry.gpu_core_clock_mhz = Some(2100);
        telemetry.gpu_memory_clock_mhz = Some(7000);
        telemetry.gpu_power_w = Some(42.5);
        telemetry.gpu_name = Some("NVIDIA Test GPU".to_string());
        telemetry.gpu_uuid = Some("GPU-test".to_string());
        telemetry.gpu_index = Some(0);
        telemetry.fan_rows.push(FanTelemetry {
            hwmon_device: "hwmon3".to_string(),
            hwmon_path: "/sys/class/hwmon/hwmon3".to_string(),
            input_path: "/sys/class/hwmon/hwmon3/fan1_input".to_string(),
            raw_label: Some("cpu".to_string()),
            display_label: "CPU Fan".to_string(),
            rpm: Some(3210),
        });

        let map = telemetry_to_dbus(&telemetry);
        assert_eq!(
            map.get("gpu_vram_total_bytes")
                .and_then(|value| u64::try_from(value).ok()),
            Some(8_589_934_592)
        );
        assert_eq!(value_as_str(&map, "gpu_name"), "NVIDIA Test GPU");
        let rows = rows_from_value(
            map.get(dbus_keys::TELEMETRY_FAN_ROWS_KEY)
                .expect("fan_rows key should exist"),
        );
        let row = rows.first().expect("fan row should exist");

        assert_eq!(
            value_as_str(row, dbus_keys::FAN_ROW_HWMON_DEVICE_KEY),
            "hwmon3"
        );
        assert_eq!(
            value_as_str(row, dbus_keys::FAN_ROW_HWMON_PATH_KEY),
            "/sys/class/hwmon/hwmon3"
        );
        assert_eq!(
            value_as_str(row, dbus_keys::FAN_ROW_INPUT_PATH_KEY),
            "/sys/class/hwmon/hwmon3/fan1_input"
        );
        assert_eq!(value_as_str(row, dbus_keys::FAN_ROW_RAW_LABEL_KEY), "cpu");
        assert_eq!(
            value_as_str(row, dbus_keys::FAN_ROW_DISPLAY_LABEL_KEY),
            "CPU Fan"
        );
        assert_eq!(u64_from_map(row, dbus_keys::FAN_ROW_RPM_KEY), Some(3210));
        assert_eq!(u64_from_map(&map, "gpu_core_clock_mhz"), Some(2100));
        assert_eq!(u64_from_map(&map, "gpu_memory_clock_mhz"), Some(7000));
    }

    #[test]
    fn telemetry_to_dbus_omits_unavailable_optional_fields() {
        let telemetry = TelemetrySnapshot::empty_now(42);
        let map = telemetry_to_dbus(&telemetry);

        assert!(map.contains_key(dbus_keys::telemetry::TIMESTAMP_MS));
        for key in [
            dbus_keys::telemetry::GPU_TEMP_C,
            dbus_keys::telemetry::GPU_USAGE_PERCENT,
            dbus_keys::telemetry::GPU_VRAM_USED_BYTES,
            dbus_keys::telemetry::GPU_VRAM_TOTAL_BYTES,
            dbus_keys::telemetry::GPU_POWER_W,
            dbus_keys::telemetry::POWER_SOURCE,
        ] {
            assert!(
                !map.contains_key(key),
                "optional key {key} must stay absent"
            );
        }
    }

    #[test]
    fn cpu_caps_to_dbus_emits_structured_control_access_rows() {
        let mut caps = CpuCaps::unknown();
        caps.control_access.push(CpuControlAccess {
            kind: rog_core::CpuControlKind::Governor,
            status: CpuAccessState::PermissionDenied,
            reason: "Governor paths are readable but not writable by the current user.".to_string(),
            direct_write: false,
            privileged_write: false,
            authorization: CpuAuthorization::NotApplicable,
            paths: vec![CpuPathAccess {
                path: "/sys/devices/system/cpu/cpufreq/policy0/scaling_governor".to_string(),
                readable: true,
                writable: false,
            }],
        });

        let map = cpu_caps_to_dbus(&caps);
        let rows = rows_from_value(
            map.get(dbus_keys::CPU_CONTROL_ACCESS_KEY)
                .expect("control_access key should exist"),
        );
        let row = rows.first().expect("control access row should exist");

        assert_eq!(
            value_as_str(row, dbus_keys::CPU_CONTROL_KIND_KEY),
            rog_core::CpuControlKind::Governor.as_str()
        );
        assert_eq!(
            value_as_str(row, dbus_keys::CPU_CONTROL_STATUS_KEY),
            CpuAccessState::PermissionDenied.as_str()
        );
        assert_eq!(
            value_as_str(row, dbus_keys::CPU_CONTROL_REASON_KEY),
            "Governor paths are readable but not writable by the current user."
        );

        let paths = rows_from_value(
            row.get(dbus_keys::CPU_CONTROL_PATHS_KEY)
                .expect("paths key should exist"),
        );
        let path = paths.first().expect("path row should exist");
        assert_eq!(
            value_as_str(path, dbus_keys::CPU_PATH_PATH_KEY),
            "/sys/devices/system/cpu/cpufreq/policy0/scaling_governor"
        );
        assert_eq!(
            path.get(dbus_keys::CPU_PATH_READABLE_KEY)
                .and_then(|value| bool::try_from(value).ok()),
            Some(true)
        );
        assert_eq!(
            path.get(dbus_keys::CPU_PATH_WRITABLE_KEY)
                .and_then(|value| bool::try_from(value).ok()),
            Some(false)
        );
    }

    #[test]
    fn cpu_to_dbus_includes_logical_cpu_id_and_core_id_alias() {
        let mut cpu = CpuTelemetry::empty_now(7);
        cpu.per_core.push(rog_core::CpuCoreTelemetry {
            logical_cpu_id: 11,
            physical_core_index: Some(5),
            policy_id: Some(11),
            thread_index: Some(1),
            thread_count: Some(2),
            usage_percent: Some(43.5),
            current_freq_mhz: Some(4200),
            min_freq_mhz: Some(800),
            max_freq_mhz: Some(5200),
            online: true,
        });

        let map = cpu_to_dbus(&cpu);
        let rows = rows_from_value(
            map.get(dbus_keys::CPU_TELEMETRY_PER_CORE_KEY)
                .expect("per_core key should exist"),
        );
        let row = rows.first().expect("per_core row should exist");

        assert_eq!(
            row.get(dbus_keys::CPU_LOGICAL_CPU_ID_KEY)
                .and_then(|value| u64::try_from(value).ok()),
            Some(11)
        );
        assert_eq!(
            row.get(dbus_keys::CPU_CORE_ID_COMPAT_KEY)
                .and_then(|value| u64::try_from(value).ok()),
            Some(11)
        );
        assert_eq!(
            row.get(dbus_keys::CPU_PHYSICAL_CORE_INDEX_KEY)
                .and_then(|value| u64::try_from(value).ok()),
            Some(5)
        );
        assert_eq!(
            row.get(dbus_keys::CPU_POLICY_ID_KEY)
                .and_then(|value| u64::try_from(value).ok()),
            Some(11)
        );
    }

    #[test]
    fn lighting_access_prefers_verified_aura_when_available() {
        let access = lighting_access_from_backend_flags(true, false, false, false, None);

        assert_eq!(access.status, FeatureAccessState::Available);
        assert!(access.reason.contains("ASUS Aura"));
    }

    #[test]
    fn lighting_dbus_map_exposes_explicit_optional_capabilities() {
        let state = LightingState {
            backend: "test-verified-aura".to_string(),
            device: "test-endpoint".to_string(),
            brightness: None,
            max_brightness: None,
            mode: Some(LightingMode::Static),
            supports_brightness: false,
            supports_modes: true,
            supported_modes: vec![LightingMode::Static],
            supports_rgb: true,
            rgb: Some(RgbColor::new(1, 2, 3)),
            supports_speed: true,
            supported_speeds: vec!["Medium".to_string()],
            supported_zones: vec!["Keyboard".to_string()],
            writable: true,
            status: "available".to_string(),
            last_error: None,
        };

        let map = lighting_state_to_dbus(&state, &LightingDiagnostics::unknown());
        assert_eq!(
            map.get("supports_brightness")
                .and_then(|value| bool::try_from(value).ok()),
            Some(false)
        );
        assert_eq!(
            map.get("supports_modes")
                .and_then(|value| bool::try_from(value).ok()),
            Some(true)
        );
        assert_eq!(
            map.get("supports_rgb")
                .and_then(|value| bool::try_from(value).ok()),
            Some(true)
        );
        assert_eq!(
            map.get("supports_speed")
                .and_then(|value| bool::try_from(value).ok()),
            Some(true)
        );
        assert_eq!(
            map.get("rgb_hex")
                .and_then(|value| <&str>::try_from(value).ok()),
            Some("#010203")
        );
    }

    #[test]
    fn lighting_access_keeps_sysfs_brightness_fallback() {
        let access = lighting_access_from_backend_flags(false, true, true, true, None);

        assert_eq!(access.status, FeatureAccessState::Available);
        assert!(access.reason.contains("sysfs LED backend"));
    }

    #[test]
    fn lighting_access_reports_unsupported_when_no_backend_exists() {
        let access = lighting_access_from_backend_flags(false, false, false, false, None);

        assert_eq!(access.status, FeatureAccessState::Unsupported);
        assert!(access.reason.contains("No keyboard lighting control"));
    }

    #[tokio::test]
    async fn direct_cpu_write_is_preferred() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let privileged_calls = AtomicUsize::new(0);
        let source = with_privileged_fallback(
            || Ok(()),
            || async {
                privileged_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(source, CpuWriteSource::Direct);
        assert_eq!(privileged_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn permission_failure_uses_successful_privileged_write() {
        let source = with_privileged_fallback(
            || {
                Err(rog_core::RogError::PermissionDenied(
                    "read-only".to_string(),
                ))
            },
            || async { Ok(()) },
        )
        .await
        .unwrap();
        assert_eq!(source, CpuWriteSource::Privileged);
    }

    #[tokio::test]
    async fn authorization_denial_is_preserved_as_permission_error() {
        let error = with_privileged_fallback(
            || {
                Err(rog_core::RogError::PermissionDenied(
                    "read-only".to_string(),
                ))
            },
            || async {
                Err(rog_core::PrivilegedError::new(
                    rog_core::PrivilegedErrorCode::NotAuthorized,
                    "denied",
                ))
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(error, rog_core::RogError::PermissionDenied(_)));
        assert!(error.to_string().contains("denied or cancelled"));
    }

    #[tokio::test]
    async fn helper_unavailable_is_a_transient_failure() {
        let error = with_privileged_fallback(
            || {
                Err(rog_core::RogError::PermissionDenied(
                    "read-only".to_string(),
                ))
            },
            || async {
                Err(rog_core::PrivilegedError::new(
                    rog_core::PrivilegedErrorCode::BackendFailure,
                    "unavailable",
                ))
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            rog_core::RogError::TemporarilyUnavailable(_)
        ));
    }

    #[tokio::test]
    async fn unsupported_cpu_control_never_uses_privileged_fallback() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let privileged_calls = AtomicUsize::new(0);
        let error = with_privileged_fallback(
            || Err(rog_core::RogError::NotSupported("hardware".to_string())),
            || async {
                privileged_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(error, rog_core::RogError::NotSupported(_)));
        assert_eq!(privileged_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn fan_fallback_prefers_direct_and_preserves_helper_failures() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let calls = AtomicUsize::new(0);
        let source = with_fan_privileged_fallback(
            || Ok(()),
            || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(source, CpuWriteSource::Direct);
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        let source = with_fan_privileged_fallback(
            || {
                Err(rog_core::RogError::PermissionDenied(
                    "read-only".to_string(),
                ))
            },
            || async { Ok(()) },
        )
        .await
        .unwrap();
        assert_eq!(source, CpuWriteSource::Privileged);

        for (code, permission_denied) in [
            (rog_core::PrivilegedErrorCode::NotAuthorized, true),
            (rog_core::PrivilegedErrorCode::BackendFailure, false),
        ] {
            let error = with_fan_privileged_fallback(
                || {
                    Err(rog_core::RogError::PermissionDenied(
                        "read-only".to_string(),
                    ))
                },
                || async { Err(rog_core::PrivilegedError::new(code, "failure")) },
            )
            .await
            .unwrap_err();
            assert_eq!(
                matches!(error, rog_core::RogError::PermissionDenied(_)),
                permission_denied
            );
        }
    }
}
