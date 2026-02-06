use std::time::{SystemTime, UNIX_EPOCH};

use rog_core::{PowerSource, RogError, RogResult};
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

        let battery_percent = match display_device {
            Some(ref path) => read_battery_percentage(&self.conn, path)
                .await
                .ok()
                .flatten(),
            None => None,
        };

        let ac_online = read_any_line_power_online(&proxy, &self.conn)
            .await
            .ok()
            .flatten();

        Ok(UPowerStatus {
            power_source,
            battery_percent,
            ac_online,
            timestamp_ms: ts,
        })
    }
}

async fn read_battery_percentage(
    conn: &zbus::Connection,
    device_path: &OwnedObjectPath,
) -> RogResult<Option<f32>> {
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

    if pct.is_finite() {
        Ok(Some(pct as f32))
    } else {
        Ok(None)
    }
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
