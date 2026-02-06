use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use regex::Regex;
use rog_core::{AppState, DeviceCaps, PowerSource, TelemetrySnapshot};
use rog_providers::hwmon::HwmonTelemetryProvider;
use rog_providers::upower::UPowerProvider;
use tokio::time::{interval, Duration};
use tracing::{info, warn};
use zbus::zvariant::{OwnedValue, Value};
use zbus::{connection, interface};

const DBUS_NAME: &str = "io.github.roghelper.Daemon";
const DBUS_PATH: &str = "/io/github/roghelper/Daemon";
const DBUS_IFACE: &str = "io.github.roghelper.Daemon1";

#[derive(Debug)]
struct SharedState {
    inner: RwLock<AppState>,
}

#[derive(Debug)]
struct RogHelperDaemon {
    state: Arc<SharedState>,
}

impl RogHelperDaemon {
    fn new(state: Arc<SharedState>) -> Self {
        Self { state }
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
        state_to_dbus(&self.read_state())
    }

    fn get_telemetry(&self) -> HashMap<String, OwnedValue> {
        telemetry_to_dbus(&self.read_state().telemetry)
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

    let mut caps = DeviceCaps::unknown();
    // Minimal caps for milestone 1: only what we can infer from sysfs.
    let hw_snapshot = hwmon.read_snapshot().unwrap_or_else(|e| {
        warn!("hwmon read failed: {e}");
        TelemetrySnapshot::empty_now(now_ms())
    });
    caps.has_fan_reading = !hw_snapshot.fans_rpm.is_empty();
    caps.has_kbd_backlight = detect_kbd_backlight();
    caps.notes
        .push("Milestone 1: asusd/supergfxd not integrated yet.".to_string());

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
    let daemon_iface = RogHelperDaemon::new(shared.clone());
    let _conn = connection::Builder::session()?
        .name(DBUS_NAME)?
        .serve_at(DBUS_PATH, daemon_iface)?
        .build()
        .await?;

    info!("DBus service online: {DBUS_NAME} {DBUS_PATH} ({DBUS_IFACE})");

    // Telemetry loop.
    let daemon = RogHelperDaemon::new(shared.clone());
    let mut ticker = interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let mut warnings = Vec::new();

                let mut telemetry = hwmon.read_snapshot().unwrap_or_else(|e| {
                    warnings.push(format!("hwmon unavailable: {e}"));
                    TelemetrySnapshot::empty_now(now_ms())
                });

                match upower.read_status().await {
                    Ok(st) => {
                        telemetry.power_source = st.power_source;
                        telemetry.battery_percent = st.battery_percent;
                        telemetry.ac_online = st.ac_online;
                        // Use UPower's timestamp as the authoritative timestamp if hwmon failed.
                        telemetry.timestamp_ms = telemetry.timestamp_ms.max(st.timestamp_ms);
                    }
                    Err(e) => {
                        warnings.push(format!("UPower unavailable: {e}"));
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
        if name.contains("kbd") || (name.contains("asus") && name.contains("keyboard")) {
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
    m
}

fn power_source_to_str(p: PowerSource) -> &'static str {
    match p {
        PowerSource::Ac => "Ac",
        PowerSource::Battery => "Battery",
    }
}

fn ov<T>(v: T) -> OwnedValue
where
    T: Into<Value<'static>>,
{
    OwnedValue::try_from(v.into()).expect("OwnedValue conversion should succeed")
}
