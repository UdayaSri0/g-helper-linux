use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use regex::Regex;
use rog_core::{AppState, BatteryState, DeviceCaps, PowerSource, TelemetrySnapshot};
use rog_providers::hwmon::HwmonTelemetryProvider;
use rog_providers::kbd_backlight::KbdBacklightSysfs;
use rog_providers::nvidia_smi::NvidiaSmiTelemetryProvider;
use rog_providers::power_supply::PowerSupplySysfsProvider;
use rog_providers::upower::UPowerProvider;
use tokio::time::{interval, Duration};
use tracing::{info, warn};
use zbus::fdo;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{connection, interface};

const DBUS_NAME: &str = "io.github.roghelper.Daemon";
const DBUS_PATH: &str = "/io/github/roghelper/Daemon";
const DBUS_IFACE: &str = "io.github.roghelper.Daemon1";

#[derive(Debug)]
struct SharedState {
    inner: RwLock<AppState>,
}

#[derive(Debug, Clone)]
struct RogHelperDaemon {
    state: Arc<SharedState>,
    kbd_backlight: Option<KbdBacklightSysfs>,
}

impl RogHelperDaemon {
    fn new(state: Arc<SharedState>, kbd_backlight: Option<KbdBacklightSysfs>) -> Self {
        Self {
            state,
            kbd_backlight,
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
}

#[interface(name = "io.github.roghelper.Daemon1")]
impl RogHelperDaemon {
    fn get_caps(&self) -> HashMap<String, OwnedValue> {
        caps_to_dbus(&self.read_state().caps)
    }

    fn get_state(&self) -> HashMap<String, OwnedValue> {
        let mut m = state_to_dbus(&self.read_state());
        if let Some(l) = self.lighting_to_dbus() {
            m.insert("lighting".to_string(), ov(l));
        }
        m
    }

    fn get_telemetry(&self) -> HashMap<String, OwnedValue> {
        telemetry_to_dbus(&self.read_state().telemetry)
    }

    fn set_lighting(&self, state: HashMap<String, OwnedValue>) -> fdo::Result<()> {
        let Some(kbd) = &self.kbd_backlight else {
            return Err(fdo::Error::NotSupported(
                "lighting not supported on this system".to_string(),
            ));
        };

        let mode = state
            .get("mode")
            .and_then(|v| <&str>::try_from(v).ok())
            .unwrap_or("Static");
        let rgb_hex = state.get("rgb_hex").and_then(|v| <&str>::try_from(v).ok());
        let mut brightness = u64_from_map(&state, "brightness").unwrap_or(0);
        if mode.eq_ignore_ascii_case("off") {
            brightness = 0;
        }

        if rgb_hex.is_some() {
            return Err(fdo::Error::NotSupported(
                "RGB color is not supported by the sysfs LED backend (requires asusd/Aura)."
                    .to_string(),
            ));
        }
        if !mode.eq_ignore_ascii_case("off") && !mode.eq_ignore_ascii_case("static") {
            return Err(fdo::Error::NotSupported(format!(
                "Lighting mode '{mode}' is not supported by the sysfs LED backend."
            )));
        }

        kbd.set_brightness(brightness as u32)
            .map_err(map_rog_error_to_fdo)?;
        Ok(())
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

    let upower = UPowerProvider::connect_system()
        .await
        .context("connect to UPower")?;
    let hwmon = HwmonTelemetryProvider::default();
    let nvidia = NvidiaSmiTelemetryProvider::default();
    let power_supply = PowerSupplySysfsProvider::default();
    let kbd_backlight = match KbdBacklightSysfs::probe() {
        Ok(v) => v,
        Err(e) => {
            warn!("kbd backlight probe failed: {e}");
            None
        }
    };

    let mut caps = DeviceCaps::unknown();
    // Minimal caps for milestone 1: only what we can infer from sysfs.
    let hw_snapshot = hwmon.read_snapshot().unwrap_or_else(|e| {
        warn!("hwmon read failed: {e}");
        TelemetrySnapshot::empty_now(now_ms())
    });
    caps.has_fan_reading = !hw_snapshot.fans_rpm.is_empty();
    caps.has_kbd_backlight = kbd_backlight.is_some() || detect_kbd_backlight();
    caps.notes
        .push("Milestone 1: asusd/supergfxd not integrated yet.".to_string());
    if let Some(kbd) = &kbd_backlight {
        caps.endpoints.push(format!("sysfs-led:{}", kbd.led_name()));
        caps.notes.push(format!(
            "Keyboard backlight via {} (max {}, writable={}).",
            kbd.led_name(),
            kbd.max_brightness(),
            kbd.can_set_brightness()
        ));
    }

    // Diagnostics: include relevant system bus names (no asusd/supergfxd hardcoding).
    let re = Regex::new("(?i)asus|rog|supergfx|power|upower").expect("valid regex");
    if let Ok(names) = rog_providers::dbus::list_system_bus_names_matching(&re).await {
        for n in names {
            caps.endpoints.push(format!("system-bus-name:{n}"));
        }
    }

    let init_state = AppState::new(caps, hw_snapshot);
    let shared = Arc::new(SharedState {
        inner: RwLock::new(init_state),
    });

    // Export DBus service on the session bus.
    let daemon = RogHelperDaemon::new(shared.clone(), kbd_backlight.clone());
    let daemon_iface = daemon.clone();
    let _conn = connection::Builder::session()?
        .name(DBUS_NAME)?
        .serve_at(DBUS_PATH, daemon_iface)?
        .build()
        .await?;

    info!("DBus service online: {DBUS_NAME} {DBUS_PATH} ({DBUS_IFACE})");

    // Telemetry loop.
    let mut ticker = interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let mut warnings = Vec::new();

                let mut telemetry = hwmon.read_snapshot().unwrap_or_else(|e| {
                    warnings.push(format!("hwmon unavailable: {e}"));
                    TelemetrySnapshot::empty_now(now_ms())
                });

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

                // Best-effort NVIDIA telemetry (read-only).
                if telemetry.gpu_temp_c.is_none() {
                    match nvidia.read_gpu_temp_c().await {
                        Ok(Some(temp)) => {
                            telemetry.gpu_temp_c = Some(temp);
                            telemetry
                                .temps_c
                                .insert("nvidia-smi:gpu_temp".to_string(), temp);
                        }
                        Ok(None) => {}
                        Err(e) => warnings.push(format!("nvidia-smi telemetry unavailable: {e}")),
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

                daemon.set_telemetry(telemetry, warnings);
            }
            _ = tokio::signal::ctrl_c() => {
                info!("ctrl-c received; exiting");
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

fn state_to_dbus(s: &AppState) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert("caps".to_string(), ov(caps_to_dbus(&s.caps)));
    m.insert("telemetry".to_string(), ov(telemetry_to_dbus(&s.telemetry)));
    m.insert("warnings".to_string(), ov(s.warnings.clone()));
    m
}

fn caps_to_dbus(c: &DeviceCaps) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert("has_profiles".to_string(), OwnedValue::from(c.has_profiles));
    m.insert(
        "has_fan_curves".to_string(),
        OwnedValue::from(c.has_fan_curves),
    );
    m.insert(
        "has_fan_reading".to_string(),
        OwnedValue::from(c.has_fan_reading),
    );
    m.insert(
        "has_charge_limit".to_string(),
        OwnedValue::from(c.has_charge_limit),
    );
    m.insert(
        "has_gpu_modes".to_string(),
        OwnedValue::from(c.has_gpu_modes),
    );
    m.insert("has_aura".to_string(), OwnedValue::from(c.has_aura));
    m.insert(
        "has_kbd_backlight".to_string(),
        OwnedValue::from(c.has_kbd_backlight),
    );
    m.insert(
        "requires_reboot_for_gpu_switch".to_string(),
        OwnedValue::from(c.requires_reboot_for_gpu_switch),
    );
    m.insert("endpoints".to_string(), ov(c.endpoints.clone()));
    m.insert("notes".to_string(), ov(c.notes.clone()));
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

fn map_rog_error_to_fdo(e: rog_core::RogError) -> fdo::Error {
    match e {
        rog_core::RogError::DependencyMissing(msg) => fdo::Error::Failed(msg),
        rog_core::RogError::NotSupported(msg) => fdo::Error::NotSupported(msg),
        rog_core::RogError::PermissionDenied(msg) => fdo::Error::AccessDenied(msg),
        rog_core::RogError::InvalidInput(msg) => fdo::Error::InvalidArgs(msg),
        rog_core::RogError::TransientFailure(msg) => fdo::Error::TimedOut(msg),
        rog_core::RogError::Unexpected(msg) => fdo::Error::Failed(msg),
    }
}

impl RogHelperDaemon {
    fn lighting_to_dbus(&self) -> Option<HashMap<String, OwnedValue>> {
        let kbd = self.kbd_backlight.as_ref()?;
        let brightness = kbd.read_brightness().ok()?;
        let max = kbd.max_brightness();
        let can_set = kbd.can_set_brightness();
        let mode = if brightness == 0 { "Off" } else { "Static" };

        let mut m = HashMap::new();
        m.insert("backend".to_string(), ov("sysfs-led"));
        m.insert("device".to_string(), ov(kbd.led_name().to_string()));
        m.insert(
            "brightness".to_string(),
            OwnedValue::from(brightness as u64),
        );
        m.insert("max_brightness".to_string(), OwnedValue::from(max as u64));
        m.insert("can_set".to_string(), OwnedValue::from(can_set));
        m.insert("mode".to_string(), ov(mode));
        m.insert("supports_rgb".to_string(), OwnedValue::from(false));
        m.insert(
            "supported_modes".to_string(),
            ov(vec!["Off".to_string(), "Static".to_string()]),
        );
        Some(m)
    }
}
