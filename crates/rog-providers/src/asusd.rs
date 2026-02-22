use rog_core::{BatteryLimitPercent, PerformanceProfile, RogError, RogResult};
use tracing::debug;
use zbus::fdo::DBusProxy;
use zbus::zvariant::OwnedValue;
use zbus::Proxy;

use crate::traits::{BatteryProvider, ProfileProvider};

#[derive(Debug, Clone, Copy)]
struct AsusdEndpoint {
    service: &'static str,
    path: &'static str,
    interface: &'static str,
}

const ASUSD_ENDPOINTS: &[AsusdEndpoint] = &[
    // asusctl 6.x
    AsusdEndpoint {
        service: "xyz.ljones.Asusd",
        path: "/xyz/ljones",
        interface: "xyz.ljones.Platform",
    },
    // Keep a likely future alias as a fallback.
    AsusdEndpoint {
        service: "org.asuslinux.Daemon",
        path: "/org/asuslinux",
        interface: "org.asuslinux.Platform",
    },
];

#[derive(Debug, Clone, Default)]
pub struct AsusdCaps {
    pub has_profiles: bool,
    pub has_charge_limit: bool,
    pub profile_choices: Vec<PerformanceProfile>,
}

#[derive(Debug, Clone)]
pub struct AsusdPlatformProvider {
    conn: zbus::Connection,
    endpoint: AsusdEndpoint,
}

impl AsusdPlatformProvider {
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

        for endpoint in ASUSD_ENDPOINTS {
            if !names.is_empty() && !names.iter().any(|n| n == endpoint.service) {
                continue;
            }

            let proxy = match Proxy::new(&conn, endpoint.service, endpoint.path, endpoint.interface)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    debug!(
                        "asusd probe proxy build failed for {}: {e}",
                        endpoint.service
                    );
                    continue;
                }
            };

            // Version is present on modern asusd Platform; profile fallback keeps this resilient.
            let ok = proxy.get_property::<OwnedValue>("Version").await.is_ok()
                || proxy
                    .get_property::<OwnedValue>("PlatformProfile")
                    .await
                    .is_ok();
            if ok {
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

    pub async fn probe_caps(&self) -> RogResult<AsusdCaps> {
        let has_profiles = match self.get_profile().await {
            Ok(_) => true,
            Err(RogError::NotSupported(_)) => false,
            Err(e) => return Err(e),
        };

        let has_charge_limit = match self.get_limit().await {
            Ok(_) => true,
            Err(RogError::NotSupported(_)) => false,
            Err(e) => return Err(e),
        };

        let profile_choices = self.read_profile_choices().await.unwrap_or_default();

        Ok(AsusdCaps {
            has_profiles,
            has_charge_limit,
            profile_choices,
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
        .map_err(|e| map_dbus_error("build asusd platform proxy", e))
    }

    async fn read_profile_choices(&self) -> RogResult<Vec<PerformanceProfile>> {
        let proxy = self.proxy().await?;
        let raw: OwnedValue = proxy
            .get_property("PlatformProfileChoices")
            .await
            .map_err(|e| map_dbus_error("read PlatformProfileChoices", e))?;

        let mut out = Vec::new();

        if let Ok(v) = Vec::<u32>::try_from(raw.clone()) {
            for item in v {
                if let Some(p) = parse_profile_from_u32(item) {
                    out.push(p);
                }
            }
        } else if let Ok(v) = Vec::<i32>::try_from(raw.clone()) {
            for item in v {
                if let Ok(as_u32) = u32::try_from(item) {
                    if let Some(p) = parse_profile_from_u32(as_u32) {
                        out.push(p);
                    }
                }
            }
        } else if let Ok(v) = Vec::<String>::try_from(raw.clone()) {
            for item in v {
                if let Some(p) = parse_profile_from_text(&item) {
                    out.push(p);
                }
            }
        } else if let Ok(v) = Vec::<OwnedValue>::try_from(raw) {
            for item in v {
                if let Some(p) = parse_profile_from_value(&item) {
                    out.push(p);
                }
            }
        }

        out.sort_by_key(profile_order);
        out.dedup();
        Ok(out)
    }
}

impl ProfileProvider for AsusdPlatformProvider {
    async fn get_profile(&self) -> RogResult<PerformanceProfile> {
        let proxy = self.proxy().await?;
        let raw: OwnedValue = proxy
            .get_property("PlatformProfile")
            .await
            .map_err(|e| map_dbus_error("read PlatformProfile", e))?;

        parse_profile_from_value(&raw).ok_or_else(|| {
            RogError::Unexpected(format!(
                "unable to parse PlatformProfile from {:?}",
                raw.value_signature()
            ))
        })
    }

    async fn set_profile(&self, profile: PerformanceProfile) -> RogResult<()> {
        let proxy = self.proxy().await?;
        let wire = to_asusd_profile(profile)?;
        proxy
            .set_property("PlatformProfile", wire)
            .await
            .map_err(|e| map_dbus_fdo_error("set PlatformProfile", e))?;
        Ok(())
    }
}

impl BatteryProvider for AsusdPlatformProvider {
    async fn get_limit(&self) -> RogResult<BatteryLimitPercent> {
        let proxy = self.proxy().await?;
        let raw: OwnedValue = proxy
            .get_property("ChargeControlEndThreshold")
            .await
            .map_err(|e| map_dbus_error("read ChargeControlEndThreshold", e))?;

        let Some(limit) = value_to_u8(&raw) else {
            return Err(RogError::Unexpected(
                "ChargeControlEndThreshold had an unsupported type".to_string(),
            ));
        };

        Ok(BatteryLimitPercent(limit))
    }

    async fn set_limit(&self, limit: BatteryLimitPercent) -> RogResult<()> {
        if !(20..=100).contains(&limit.0) {
            return Err(RogError::InvalidInput(format!(
                "battery limit {} is outside supported range 20..=100 for asusd",
                limit.0
            )));
        }

        let proxy = self.proxy().await?;
        proxy
            .set_property("ChargeControlEndThreshold", limit.0)
            .await
            .map_err(|e| map_dbus_fdo_error("set ChargeControlEndThreshold", e))?;
        Ok(())
    }
}

fn parse_profile_from_value(v: &OwnedValue) -> Option<PerformanceProfile> {
    if let Some(n) = value_to_u32(v) {
        return parse_profile_from_u32(n);
    }
    if let Ok(s) = <&str>::try_from(v) {
        return parse_profile_from_text(s);
    }
    None
}

fn parse_profile_from_u32(v: u32) -> Option<PerformanceProfile> {
    match v {
        0 => Some(PerformanceProfile::Balanced),
        1 => Some(PerformanceProfile::Turbo),
        2 | 3 => Some(PerformanceProfile::Silent),
        4 => Some(PerformanceProfile::Custom("Custom".to_string())),
        _ => None,
    }
}

fn parse_profile_from_text(v: &str) -> Option<PerformanceProfile> {
    let key = normalize_word(v);
    match key.as_str() {
        "balanced" => Some(PerformanceProfile::Balanced),
        "performance" | "turbo" => Some(PerformanceProfile::Turbo),
        "quiet" | "silent" | "lowpower" => Some(PerformanceProfile::Silent),
        "custom" => Some(PerformanceProfile::Custom("Custom".to_string())),
        _ => None,
    }
}

fn to_asusd_profile(profile: PerformanceProfile) -> RogResult<u32> {
    match profile {
        PerformanceProfile::Balanced => Ok(0),
        PerformanceProfile::Turbo => Ok(1),
        PerformanceProfile::Silent => Ok(2),
        PerformanceProfile::Custom(name) => {
            let key = normalize_word(&name);
            match key.as_str() {
                "balanced" => Ok(0),
                "performance" | "turbo" => Ok(1),
                "quiet" | "silent" => Ok(2),
                "lowpower" => Ok(3),
                "custom" => Ok(4),
                _ => Err(RogError::InvalidInput(format!(
                    "unknown profile '{}', expected quiet/balanced/performance/custom",
                    name
                ))),
            }
        }
    }
}

fn profile_order(p: &PerformanceProfile) -> u8 {
    match p {
        PerformanceProfile::Silent => 0,
        PerformanceProfile::Balanced => 1,
        PerformanceProfile::Turbo => 2,
        PerformanceProfile::Custom(_) => 3,
    }
}

fn value_to_u8(v: &OwnedValue) -> Option<u8> {
    u8::try_from(v)
        .ok()
        .or_else(|| u16::try_from(v).ok().and_then(|n| u8::try_from(n).ok()))
        .or_else(|| u32::try_from(v).ok().and_then(|n| u8::try_from(n).ok()))
        .or_else(|| i32::try_from(v).ok().and_then(|n| u8::try_from(n).ok()))
}

fn value_to_u32(v: &OwnedValue) -> Option<u32> {
    u32::try_from(v)
        .ok()
        .or_else(|| u8::try_from(v).ok().map(|n| n as u32))
        .or_else(|| u16::try_from(v).ok().map(|n| n as u32))
        .or_else(|| u64::try_from(v).ok().and_then(|n| u32::try_from(n).ok()))
        .or_else(|| i32::try_from(v).ok().and_then(|n| u32::try_from(n).ok()))
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

fn map_dbus_fdo_error(ctx: &str, e: zbus::fdo::Error) -> RogError {
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
