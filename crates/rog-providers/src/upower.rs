use std::time::{SystemTime, UNIX_EPOCH};

use rog_core::{BatteryState, PowerSource, RogError, RogResult};
use tracing::debug;
use zvariant::OwnedObjectPath;

#[derive(Debug)]
pub struct UPowerProvider {
    conn: zbus::Connection,
}

#[derive(Debug, Clone, Copy)]
pub struct UPowerStatus {
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
    pub timestamp_ms: u64,
}

impl UPowerProvider {
    pub async fn connect_system() -> RogResult<Self> {
        let conn = zbus::Connection::system()
            .await
            .map_err(|e| RogError::DependencyMissing(format!("system dbus unavailable: {e}")))?;
        Ok(Self { conn })
    }

    pub async fn read_status(&self) -> RogResult<UPowerStatus> {
        let ts = now_ms();

        let proxy = zbus::Proxy::new(
            &self.conn,
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower",
            "org.freedesktop.UPower",
        )
        .await
        .map_err(|e| RogError::DependencyMissing(format!("UPower not available: {e}")))?;

        let on_battery: bool = proxy
            .get_property("OnBattery")
            .await
            .map_err(|e| RogError::Unexpected(format!("UPower OnBattery read failed: {e}")))?;
        let power_source = Some(if on_battery {
            PowerSource::Battery
        } else {
            PowerSource::Ac
        });

        // Prefer UPower's "display device" (main battery). API differs by UPower version:
        // some expose it as a method (GetDisplayDevice), older ones as a property (DisplayDevice).
        let display_device = match proxy
            .call::<_, _, OwnedObjectPath>("GetDisplayDevice", &())
            .await
        {
            Ok(path) => Some(path),
            Err(_) => proxy
                .get_property::<OwnedObjectPath>("DisplayDevice")
                .await
                .ok(),
        };

        let ac_online = read_any_line_power_online(&proxy, &self.conn)
            .await
            .ok()
            .flatten();

        let details = match display_device {
            Some(ref path) => read_battery_details(&self.conn, path)
                .await
                .ok()
                .unwrap_or_default(),
            None => BatteryDetails::default(),
        };

        Ok(UPowerStatus {
            power_source,
            battery_percent: details.battery_percent,
            ac_online,
            battery_state: details.battery_state,
            battery_health_percent: details.battery_health_percent,
            battery_cycle_count: details.battery_cycle_count,
            battery_charge_power_w: details.battery_charge_power_w,
            battery_discharge_power_w: details.battery_discharge_power_w,
            battery_time_to_empty_s: details.battery_time_to_empty_s,
            battery_time_to_full_s: details.battery_time_to_full_s,
            timestamp_ms: ts,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct BatteryDetails {
    battery_percent: Option<f32>,
    battery_state: Option<BatteryState>,
    battery_health_percent: Option<f32>,
    battery_cycle_count: Option<u32>,
    battery_charge_power_w: Option<f32>,
    battery_discharge_power_w: Option<f32>,
    battery_time_to_empty_s: Option<u64>,
    battery_time_to_full_s: Option<u64>,
}

async fn read_battery_details(
    conn: &zbus::Connection,
    device_path: &OwnedObjectPath,
) -> RogResult<BatteryDetails> {
    let dproxy = zbus::Proxy::new(
        conn,
        "org.freedesktop.UPower",
        device_path.as_str(),
        "org.freedesktop.UPower.Device",
    )
    .await
    .map_err(|e| RogError::Unexpected(format!("UPower Device proxy build failed: {e}")))?;

    let pct: f64 = dproxy
        .get_property("Percentage")
        .await
        .map_err(|e| RogError::Unexpected(format!("UPower Percentage read failed: {e}")))?;

    let battery_percent = pct.is_finite().then_some(pct as f32);

    let battery_state = dproxy
        .get_property::<u32>("State")
        .await
        .ok()
        .map(battery_state_from_upower);

    let energy_rate_w = dproxy
        .get_property::<f64>("EnergyRate")
        .await
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v as f32);

    let battery_time_to_empty_s = read_time_s(&dproxy, "TimeToEmpty").await;
    let battery_time_to_full_s = read_time_s(&dproxy, "TimeToFull").await;

    let energy_full = dproxy
        .get_property::<f64>("EnergyFull")
        .await
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0);
    let energy_full_design = dproxy
        .get_property::<f64>("EnergyFullDesign")
        .await
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0);

    let battery_health_percent = match (energy_full, energy_full_design) {
        (Some(full), Some(design)) if design > 0.0 => {
            let pct = (full / design) * 100.0;
            pct.is_finite().then_some(pct as f32)
        }
        _ => None,
    };

    // Newer UPower exposes ChargeCycles (uint32). Some implementations may use CycleCount.
    let battery_cycle_count = match dproxy.get_property::<u32>("ChargeCycles").await {
        Ok(v) => Some(v),
        Err(_) => dproxy.get_property::<u32>("CycleCount").await.ok(),
    };

    let mut battery_charge_power_w = None;
    let mut battery_discharge_power_w = None;
    if let (Some(st), Some(rate)) = (battery_state, energy_rate_w) {
        match st {
            BatteryState::Charging | BatteryState::PendingCharge => {
                battery_charge_power_w = Some(rate);
            }
            BatteryState::Discharging | BatteryState::PendingDischarge => {
                battery_discharge_power_w = Some(rate);
            }
            _ => {}
        }
    }

    Ok(BatteryDetails {
        battery_percent,
        battery_state,
        battery_health_percent,
        battery_cycle_count,
        battery_charge_power_w,
        battery_discharge_power_w,
        battery_time_to_empty_s,
        battery_time_to_full_s,
    })
}

async fn read_any_line_power_online(
    upower_proxy: &zbus::Proxy<'_>,
    conn: &zbus::Connection,
) -> RogResult<Option<bool>> {
    // EnumerateDevices -> find any Type==1 (line power) and read Online.
    let paths: Vec<OwnedObjectPath> = upower_proxy
        .call::<_, _, Vec<OwnedObjectPath>>("EnumerateDevices", &())
        .await
        .map_err(|e| RogError::Unexpected(format!("UPower EnumerateDevices failed: {e}")))?;

    debug!("UPower devices: {}", paths.len());

    let mut found_any = false;
    let mut any_online = false;

    for path in paths {
        let dproxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.UPower",
            path.as_str(),
            "org.freedesktop.UPower.Device",
        )
        .await
        .map_err(|e| RogError::Unexpected(format!("UPower Device proxy build failed: {e}")))?;

        let dtype: u32 = match dproxy.get_property("Type").await {
            Ok(v) => v,
            Err(_) => continue,
        };
        if dtype != 1 {
            continue;
        }
        found_any = true;
        let online: bool = match dproxy.get_property("Online").await {
            Ok(v) => v,
            Err(_) => continue,
        };
        if online {
            any_online = true;
            break;
        }
    }

    if found_any {
        Ok(Some(any_online))
    } else {
        Ok(None)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn battery_state_from_upower(code: u32) -> BatteryState {
    match code {
        1 => BatteryState::Charging,
        2 => BatteryState::Discharging,
        3 => BatteryState::Empty,
        4 => BatteryState::Full,
        5 => BatteryState::PendingCharge,
        6 => BatteryState::PendingDischarge,
        _ => BatteryState::Unknown,
    }
}

async fn read_time_s(dproxy: &zbus::Proxy<'_>, prop: &str) -> Option<u64> {
    // UPower typically uses int64 seconds, but some environments may report uint64.
    let v_i64 = dproxy.get_property::<i64>(prop).await.ok();
    if let Some(v) = v_i64 {
        if v > 0 {
            return Some(v as u64);
        }
        return None;
    }

    let v_u64 = dproxy.get_property::<u64>(prop).await.ok();
    v_u64.filter(|v| *v > 0)
}
