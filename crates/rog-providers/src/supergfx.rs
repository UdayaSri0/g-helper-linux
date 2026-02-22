use rog_core::{GpuMode, RogError, RogResult};
use tracing::debug;
use zbus::fdo::DBusProxy;
use zbus::zvariant::{OwnedValue, Value};
use zbus::Proxy;

use crate::traits::GpuProvider;

#[derive(Debug, Clone, Copy)]
struct SupergfxEndpoint {
    service: &'static str,
    path: &'static str,
    interface: &'static str,
}

const SUPERGFX_ENDPOINTS: &[SupergfxEndpoint] = &[SupergfxEndpoint {
    service: "org.supergfxctl.Daemon",
    path: "/org/supergfxctl/Gfx",
    interface: "org.supergfxctl.Daemon",
}];

#[derive(Debug, Clone, Default)]
pub struct SupergfxCaps {
    pub raw_supported_modes: Vec<String>,
    pub requires_reboot_hint: bool,
}

#[derive(Debug, Clone)]
pub struct SupergfxProvider {
    conn: zbus::Connection,
    endpoint: SupergfxEndpoint,
}

impl SupergfxProvider {
    pub async fn connect_system() -> RogResult<Option<Self>> {
        let conn = zbus::Connection::system()
            .await
            .map_err(|e| RogError::DependencyMissing(format!("system dbus unavailable: {e}")))?;

        let names = match DBusProxy::new(&conn).await {
            Ok(p) => p
                .list_names()
                .await
                .map(|ns| ns.into_iter().map(|n| n.to_string()).collect::<Vec<_>>())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };

        for endpoint in SUPERGFX_ENDPOINTS {
            if !names.is_empty() && !names.iter().any(|n| n == endpoint.service) {
                continue;
            }

            let proxy = match Proxy::new(&conn, endpoint.service, endpoint.path, endpoint.interface)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    debug!(
                        "supergfx probe proxy build failed for {}: {e}",
                        endpoint.service
                    );
                    continue;
                }
            };

            if proxy.call::<_, _, String>("Version", &()).await.is_ok()
                || proxy.call::<_, _, u32>("Mode", &()).await.is_ok()
            {
                return Ok(Some(Self {
                    conn: conn.clone(),
                    endpoint: *endpoint,
                }));
            }
        }

        Ok(None)
    }

    pub fn endpoint_tag(&self) -> String {
        format!(
            "{}:{}:{}",
            self.endpoint.service, self.endpoint.path, self.endpoint.interface
        )
    }

    pub async fn probe_caps(&self) -> RogResult<SupergfxCaps> {
        let modes = self.read_supported_modes().await?;
        let requires_reboot_hint = self
            .read_config_always_reboot()
            .await
            .unwrap_or_else(|_| modes.iter().any(|m| m.eq_ignore_ascii_case("AsusMuxDgpu")));

        Ok(SupergfxCaps {
            raw_supported_modes: modes,
            requires_reboot_hint,
        })
    }

    async fn proxy(&self) -> RogResult<Proxy<'_>> {
        Proxy::new(
            &self.conn,
            self.endpoint.service,
            self.endpoint.path,
            self.endpoint.interface,
        )
        .await
        .map_err(|e| map_dbus_error("build supergfx proxy", e))
    }

    async fn read_mode_value(&self) -> RogResult<OwnedValue> {
        let proxy = self.proxy().await?;
        if let Ok(v) = proxy.call::<_, _, u32>("Mode", &()).await {
            return Ok(OwnedValue::from(v));
        }
        if let Ok(v) = proxy.call::<_, _, u8>("Mode", &()).await {
            return Ok(OwnedValue::from(v as u32));
        }
        if let Ok(v) = proxy.call::<_, _, i32>("Mode", &()).await {
            return Ok(OwnedValue::from(v));
        }
        if let Ok(v) = proxy.call::<_, _, String>("Mode", &()).await {
            if let Some(raw) = owned_from_string(v) {
                return Ok(raw);
            }
        }
        proxy
            .call::<_, _, OwnedValue>("Mode", &())
            .await
            .map_err(|e| map_dbus_error("read supergfx mode", e))
    }

    async fn read_supported_mode_values(&self) -> RogResult<Vec<OwnedValue>> {
        let proxy = self.proxy().await?;
        if let Ok(v) = proxy.call::<_, _, Vec<u32>>("Supported", &()).await {
            return Ok(v.into_iter().map(OwnedValue::from).collect());
        }
        if let Ok(v) = proxy.call::<_, _, Vec<u8>>("Supported", &()).await {
            return Ok(v.into_iter().map(|n| OwnedValue::from(n as u32)).collect());
        }
        if let Ok(v) = proxy.call::<_, _, Vec<i32>>("Supported", &()).await {
            return Ok(v.into_iter().map(OwnedValue::from).collect());
        }
        if let Ok(v) = proxy.call::<_, _, Vec<String>>("Supported", &()).await {
            let mut out = Vec::with_capacity(v.len());
            for item in v {
                if let Some(raw) = owned_from_string(item) {
                    out.push(raw);
                }
            }
            return Ok(out);
        }
        proxy
            .call::<_, _, Vec<OwnedValue>>("Supported", &())
            .await
            .map_err(|e| map_dbus_error("read Supported modes", e))
    }

    async fn read_supported_modes(&self) -> RogResult<Vec<String>> {
        let mut out = Vec::new();
        for v in self.read_supported_mode_values().await? {
            if let Some(name) = mode_name_from_value(&v) {
                out.push(name.to_string());
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }

    async fn read_pending_action_hint(&self) -> RogResult<Option<String>> {
        let proxy = self.proxy().await?;

        let action = if let Ok(v) = proxy.call::<_, _, u32>("PendingUserAction", &()).await {
            action_name_from_u32(v).map(str::to_string)
        } else if let Ok(v) = proxy.call::<_, _, u8>("PendingUserAction", &()).await {
            action_name_from_u32(v as u32).map(str::to_string)
        } else if let Ok(v) = proxy.call::<_, _, i32>("PendingUserAction", &()).await {
            u32::try_from(v)
                .ok()
                .and_then(action_name_from_u32)
                .map(str::to_string)
        } else if let Ok(v) = proxy.call::<_, _, String>("PendingUserAction", &()).await {
            Some(v)
        } else {
            let raw: OwnedValue = proxy
                .call("PendingUserAction", &())
                .await
                .map_err(|e| map_dbus_error("read PendingUserAction", e))?;

            if let Some(v) = value_to_u32(&raw) {
                action_name_from_u32(v).map(str::to_string)
            } else if let Ok(v) = <&str>::try_from(&raw) {
                Some(v.to_string())
            } else {
                None
            }
        };

        let Some(action) = action else {
            return Ok(None);
        };

        let key = normalize_word(&action);
        let hint = match key.as_str() {
            "nothing" => None,
            "logout" => Some("Logout required to complete pending GPU switch.".to_string()),
            "reboot" => Some("Reboot required to complete pending GPU switch.".to_string()),
            "switchtointegrated" => {
                Some("Switch to Integrated mode before this GPU change.".to_string())
            }
            "asusegpudisable" => Some(
                "Disable ASUS eGPU mode (or return to Integrated/Hybrid) before switching."
                    .to_string(),
            ),
            _ => Some(format!("GPU mode switch pending action: {action}")),
        };
        Ok(hint)
    }

    async fn read_config_always_reboot(&self) -> RogResult<bool> {
        let proxy = self.proxy().await?;
        let cfg: (u32, bool, bool, bool, bool, bool, u64, bool) =
            proxy
                .call("Config", &())
                .await
                .map_err(|e| map_dbus_error("read supergfx Config", e))?;
        Ok(cfg.4)
    }
}

impl GpuProvider for SupergfxProvider {
    async fn get_mode(&self) -> RogResult<GpuMode> {
        let raw = self.read_mode_value().await?;
        value_to_gpu_mode(&raw).ok_or_else(|| {
            RogError::Unexpected(format!(
                "unable to parse supergfx mode from {:?}",
                raw.value_signature()
            ))
        })
    }

    async fn set_mode(&self, mode: GpuMode) -> RogResult<()> {
        let supported_values = self.read_supported_mode_values().await?;
        let supported_ids = supported_values
            .iter()
            .filter_map(value_to_u32)
            .collect::<Vec<_>>();
        let supported_names = supported_values
            .iter()
            .filter_map(mode_name_from_value)
            .map(str::to_string)
            .collect::<Vec<_>>();

        let target_id = pick_target_mode(mode, &supported_ids, &supported_names)?;
        let proxy = self.proxy().await?;

        if proxy
            .call::<_, _, OwnedValue>("SetMode", &target_id)
            .await
            .is_ok()
        {
            return Ok(());
        }

        // Fallback for implementations that expose mode as a text enum.
        let target_name = mode_name_from_u32(target_id).ok_or_else(|| {
            RogError::Unexpected(format!(
                "could not map target mode id {target_id} to a name"
            ))
        })?;
        proxy
            .call::<_, _, OwnedValue>("SetMode", &target_name.to_string())
            .await
            .map_err(|e| map_dbus_error("set supergfx mode", e))?;
        Ok(())
    }

    async fn can_switch_now(&self) -> RogResult<Option<String>> {
        self.read_pending_action_hint().await
    }
}

fn pick_target_mode(mode: GpuMode, ids: &[u32], names: &[String]) -> RogResult<u32> {
    let contains = |id: u32, name: &str| ids.contains(&id) || names.iter().any(|n| n == name);

    let target = match mode {
        GpuMode::Integrated => {
            if contains(1, "Integrated") {
                Some(1)
            } else {
                None
            }
        }
        GpuMode::Hybrid => {
            if contains(0, "Hybrid") {
                Some(0)
            } else {
                None
            }
        }
        GpuMode::Dedicated => {
            if contains(5, "AsusMuxDgpu") {
                Some(5)
            } else if contains(4, "AsusEgpu") {
                Some(4)
            } else if contains(2, "NvidiaNoModeset") {
                Some(2)
            } else if contains(3, "Vfio") {
                Some(3)
            } else {
                None
            }
        }
        GpuMode::Other(name) => mode_id_from_text(&name),
    };

    target.ok_or_else(|| {
        RogError::NotSupported(format!(
            "requested GPU mode is not supported by supergfxd (supported: {:?})",
            names
        ))
    })
}

fn value_to_gpu_mode(v: &OwnedValue) -> Option<GpuMode> {
    if let Some(id) = value_to_u32(v) {
        return match id {
            0 => Some(GpuMode::Hybrid),
            1 => Some(GpuMode::Integrated),
            5 => Some(GpuMode::Dedicated),
            2 => Some(GpuMode::Other("NvidiaNoModeset".to_string())),
            3 => Some(GpuMode::Other("Vfio".to_string())),
            4 => Some(GpuMode::Other("AsusEgpu".to_string())),
            6 => Some(GpuMode::Other("Unknown".to_string())),
            _ => None,
        };
    }

    if let Ok(name) = <&str>::try_from(v) {
        return match normalize_word(name).as_str() {
            "integrated" => Some(GpuMode::Integrated),
            "hybrid" => Some(GpuMode::Hybrid),
            "asusmuxdgpu" => Some(GpuMode::Dedicated),
            "nvidianomodeset" => Some(GpuMode::Other("NvidiaNoModeset".to_string())),
            "vfio" => Some(GpuMode::Other("Vfio".to_string())),
            "asusegpu" => Some(GpuMode::Other("AsusEgpu".to_string())),
            _ => Some(GpuMode::Other(name.to_string())),
        };
    }

    None
}

fn mode_name_from_value(v: &OwnedValue) -> Option<&'static str> {
    if let Some(id) = value_to_u32(v) {
        return mode_name_from_u32(id);
    }

    if let Ok(name) = <&str>::try_from(v) {
        return mode_name_from_text(name);
    }

    None
}

fn mode_name_from_u32(v: u32) -> Option<&'static str> {
    match v {
        0 => Some("Hybrid"),
        1 => Some("Integrated"),
        2 => Some("NvidiaNoModeset"),
        3 => Some("Vfio"),
        4 => Some("AsusEgpu"),
        5 => Some("AsusMuxDgpu"),
        6 => Some("None"),
        _ => None,
    }
}

fn mode_name_from_text(v: &str) -> Option<&'static str> {
    match normalize_word(v).as_str() {
        "hybrid" => Some("Hybrid"),
        "integrated" => Some("Integrated"),
        "nvidianomodeset" => Some("NvidiaNoModeset"),
        "vfio" => Some("Vfio"),
        "asusegpu" => Some("AsusEgpu"),
        "asusmuxdgpu" => Some("AsusMuxDgpu"),
        "none" | "unknown" => Some("None"),
        _ => None,
    }
}

fn mode_id_from_text(v: &str) -> Option<u32> {
    match normalize_word(v).as_str() {
        "hybrid" => Some(0),
        "integrated" => Some(1),
        "nvidianomodeset" => Some(2),
        "vfio" => Some(3),
        "asusegpu" => Some(4),
        "asusmuxdgpu" | "dedicated" | "discrete" => Some(5),
        _ => None,
    }
}

fn action_name_from_u32(v: u32) -> Option<&'static str> {
    match v {
        0 => Some("Logout"),
        1 => Some("Reboot"),
        2 => Some("SwitchToIntegrated"),
        3 => Some("AsusEgpuDisable"),
        4 => Some("Nothing"),
        _ => None,
    }
}

fn value_to_u32(v: &OwnedValue) -> Option<u32> {
    u32::try_from(v)
        .ok()
        .or_else(|| u8::try_from(v).ok().map(|n| n as u32))
        .or_else(|| u16::try_from(v).ok().map(|n| n as u32))
        .or_else(|| u64::try_from(v).ok().and_then(|n| u32::try_from(n).ok()))
        .or_else(|| i32::try_from(v).ok().and_then(|n| u32::try_from(n).ok()))
}

fn owned_from_string(v: String) -> Option<OwnedValue> {
    OwnedValue::try_from(Value::from(v.clone()))
        .ok()
        .or_else(|| OwnedValue::try_from(Value::from(v.as_str())).ok())
}

fn normalize_word(v: &str) -> String {
    v.to_ascii_lowercase()
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect()
}

fn map_dbus_error(ctx: &str, e: zbus::Error) -> RogError {
    classify_dbus_error(ctx, &e.to_string())
}

fn classify_dbus_error(ctx: &str, msg: &str) -> RogError {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("notsupported") || lower.contains("not supported") {
        return RogError::NotSupported(format!("{ctx}: {msg}"));
    }
    if lower.contains("access denied") || lower.contains("permission denied") {
        return RogError::PermissionDenied(format!("{ctx}: {msg}"));
    }
    if lower.contains("serviceunknown")
        || lower.contains("name has no owner")
        || lower.contains("unknown service")
    {
        return RogError::DependencyMissing(format!("{ctx}: {msg}"));
    }
    RogError::Unexpected(format!("{ctx}: {msg}"))
}
