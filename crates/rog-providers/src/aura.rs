use std::collections::{BTreeSet, VecDeque};
use std::time::Duration;

use regex::Regex;
use rog_core::{
    LightingBackendKind, LightingCaps, LightingDirection, LightingMode, LightingSpeed,
    LightingState, LightingZone, RgbColor, RogError, RogResult,
};
use tokio::time::timeout;
use tracing::debug;
use zbus::fdo::{DBusProxy, ObjectManagerProxy};
use zbus::zvariant::OwnedValue;
use zbus::Proxy;

const SERVICE_CANDIDATES: &[&str] = &["xyz.ljones.Asusd", "org.asuslinux.Daemon"];
const ASUSD_SERVICE: &str = "xyz.ljones.Asusd";
const ASUSD_AURA_INTERFACE: &str = "xyz.ljones.Aura";
const ASUSD_AURA_PATH_PREFIX: &str = "/xyz/ljones/aura/";
const PROBE_TIMEOUT_MS: u64 = 1200;
const PROBE_MAX_DEPTH: usize = 3;
const PROBE_MAX_NODES: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuraEndpoint {
    pub service: String,
    pub path: String,
    pub interface: String,
}

/// A DBus ABI which is safe for this provider to call.
///
/// Source: <https://github.com/OpenGamingCollective/asusctl>. The ABI was
/// checked against asusctl/asusd 6.3.8 (commit
/// 47bf9b134b4e702e0b5e18db7cc66839e42d9035) and the unchanged 6.4.0/current
/// interface at 1c456fa3dd8fe11287289b4bbee9dd3561afc5a3. Upstream is
/// MPL-2.0. This is an independently implemented DBus adapter; no upstream
/// implementation code or HID packet encoder is copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsusdAuraContract {
    V6_3_8ToV6_4_0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsusdAuraSpeed {
    Low,
    Med,
    High,
}

impl AsusdAuraSpeed {
    fn as_wire(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Med => "Med",
            Self::High => "High",
        }
    }

    fn from_wire(value: &str) -> Option<Self> {
        match value {
            "Low" => Some(Self::Low),
            "Med" => Some(Self::Med),
            "High" => Some(Self::High),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsusdAuraDirection {
    Right,
    Left,
    Up,
    Down,
}

impl AsusdAuraDirection {
    fn as_wire(self) -> &'static str {
        match self {
            Self::Right => "Right",
            Self::Left => "Left",
            Self::Up => "Up",
            Self::Down => "Down",
        }
    }

    fn from_wire(value: &str) -> Option<Self> {
        match value {
            "Right" => Some(Self::Right),
            "Left" => Some(Self::Left),
            "Up" => Some(Self::Up),
            "Down" => Some(Self::Down),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsusdAuraZone {
    All,
    KeyboardZone1,
    KeyboardZone2,
    KeyboardZone3,
    KeyboardZone4,
    Logo,
    LightbarLeft,
    LightbarRight,
    Unknown(u32),
}

impl AsusdAuraZone {
    fn id(self) -> Option<u32> {
        match self {
            Self::All => Some(0),
            Self::KeyboardZone1 => Some(1),
            Self::KeyboardZone2 => Some(2),
            Self::KeyboardZone3 => Some(3),
            Self::KeyboardZone4 => Some(4),
            Self::Logo => Some(5),
            Self::LightbarLeft => Some(6),
            Self::LightbarRight => Some(7),
            Self::Unknown(_) => None,
        }
    }

    fn from_id(id: u32) -> Self {
        match id {
            0 => Self::All,
            1 => Self::KeyboardZone1,
            2 => Self::KeyboardZone2,
            3 => Self::KeyboardZone3,
            4 => Self::KeyboardZone4,
            5 => Self::Logo,
            6 => Self::LightbarLeft,
            7 => Self::LightbarRight,
            id => Self::Unknown(id),
        }
    }

    fn label(self) -> String {
        match self {
            Self::All => "All".to_string(),
            Self::KeyboardZone1 => "Keyboard Zone 1".to_string(),
            Self::KeyboardZone2 => "Keyboard Zone 2".to_string(),
            Self::KeyboardZone3 => "Keyboard Zone 3".to_string(),
            Self::KeyboardZone4 => "Keyboard Zone 4".to_string(),
            Self::Logo => "Logo".to_string(),
            Self::LightbarLeft => "Lightbar Left".to_string(),
            Self::LightbarRight => "Lightbar Right".to_string(),
            Self::Unknown(id) => format!("ASUS Aura zone {id}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsusdAuraEffect {
    pub mode: LightingMode,
    pub zone: AsusdAuraZone,
    pub primary: RgbColor,
    pub secondary: RgbColor,
    pub speed: AsusdAuraSpeed,
    pub direction: AsusdAuraDirection,
}

type AsusdAuraEffectWire = (u32, u32, (u8, u8, u8), (u8, u8, u8), String, String);

impl AuraEndpoint {
    pub fn tag(&self) -> String {
        format!("{}:{}:{}", self.service, self.path, self.interface)
    }

    pub fn endpoint_tag(&self) -> String {
        format!("aura-dbus:{}", self.tag())
    }
}

#[derive(Debug, Clone)]
pub struct AuraProbeResult {
    pub provider: Option<AuraProvider>,
    pub diagnostics: AuraProbeDiagnostics,
    pub notes: Vec<String>,
}

impl AuraProbeResult {
    pub fn is_available(&self) -> bool {
        self.provider.is_some()
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuraProbeDiagnostics {
    pub services_checked: Vec<String>,
    pub service_detected: bool,
    pub service_name: Option<String>,
    pub object_paths_checked: Vec<String>,
    pub interfaces_detected: Vec<String>,
    pub aura_interface_detected: bool,
    pub keyboard_interface_detected: bool,
    pub potential_aura_interfaces: Vec<String>,
    pub rgb_methods_detected: Vec<String>,
    pub rgb_properties_detected: Vec<String>,
    pub brightness_methods_detected: Vec<String>,
    pub brightness_properties_detected: Vec<String>,
    pub mode_methods_detected: Vec<String>,
    pub mode_properties_detected: Vec<String>,
    pub speed_methods_detected: Vec<String>,
    pub speed_properties_detected: Vec<String>,
    pub zone_methods_detected: Vec<String>,
    pub zone_properties_detected: Vec<String>,
    pub verified_interface_detected: bool,
    pub selected_endpoint: Option<String>,
    pub probe_errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AuraProvider {
    conn: zbus::Connection,
    endpoint: AuraEndpoint,
    contract: AsusdAuraContract,
    controls: AuraControls,
}

impl AuraProvider {
    pub async fn probe_system() -> RogResult<AuraProbeResult> {
        let conn = zbus::Connection::system()
            .await
            .map_err(|e| RogError::DependencyMissing(format!("system dbus unavailable: {e}")))?;

        let names = DBusProxy::new(&conn)
            .await
            .map_err(|e| RogError::Unexpected(format!("build system DBus proxy: {e}")))?
            .list_names()
            .await
            .map_err(|e| map_fdo_error("list system DBus names", e))?
            .into_iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>();

        let mut notes = Vec::new();
        let mut diagnostics = AuraProbeDiagnostics {
            services_checked: SERVICE_CANDIDATES
                .iter()
                .map(|service| (*service).to_string())
                .collect(),
            ..AuraProbeDiagnostics::default()
        };
        let services = service_candidates_from_names(&names);
        for service in &services {
            push_unique(&mut diagnostics.services_checked, service.clone());
        }

        for service in &services {
            diagnostics.service_detected = true;
            if diagnostics.service_name.is_none() {
                diagnostics.service_name = Some(service.clone());
            }

            // Current asusd publishes dynamic device paths through the DBus
            // ObjectManager at `/`. A path obtained only by guessing or by an
            // unrelated service's introspection tree can never authorize
            // writes.
            let walk = if service == ASUSD_SERVICE {
                walk_managed_objects(&conn, service).await
            } else {
                walk_introspection(&conn, service, "/").await
            };
            for path in walk.paths_checked {
                push_unique(&mut diagnostics.object_paths_checked, path);
            }
            for error in walk.errors {
                push_unique(&mut diagnostics.probe_errors, error);
            }

            for node in walk.nodes {
                for iface in parse_interfaces(&node.xml) {
                    record_interface_diagnostics(&mut diagnostics, service, &node.path, &iface);

                    let Some(candidate) =
                        AuraControls::candidate_from_interface(&node.path, &iface)
                    else {
                        continue;
                    };
                    let Some((contract, controls)) =
                        verified_controls_for_interface(service, &node.path, &iface, candidate)
                    else {
                        continue;
                    };

                    let endpoint = AuraEndpoint {
                        service: service.clone(),
                        path: node.path.clone(),
                        interface: iface.name.clone(),
                    };
                    let provider = AuraProvider {
                        conn: conn.clone(),
                        endpoint,
                        contract,
                        controls,
                    };

                    diagnostics.selected_endpoint = Some(provider.endpoint.endpoint_tag());
                    diagnostics.verified_interface_detected = true;
                    notes.push(format!(
                        "ASUS Aura/RGB lighting exposed by {}.",
                        provider.endpoint.endpoint_tag()
                    ));
                    notes.push(provider.capability_note());
                    return Ok(AuraProbeResult {
                        provider: Some(provider),
                        diagnostics,
                        notes,
                    });
                }
            }
        }

        if !diagnostics.potential_aura_interfaces.is_empty() {
            notes.push(
                "Aura-like DBus interfaces were detected, but no verified service/path/interface/signature contract matched; they remain diagnostic-only."
                    .to_string(),
            );
        } else if diagnostics.service_detected {
            notes.push(
                "An ASUS-related DBus service was detected, but it exposed no verified Aura/keyboard RGB interface."
                    .to_string(),
            );
        } else {
            notes.push("asusd Aura probe: no supported service names were detected.".to_string());
        }

        Ok(AuraProbeResult {
            provider: None,
            diagnostics,
            notes,
        })
    }

    pub fn endpoint_tag(&self) -> String {
        self.endpoint.endpoint_tag()
    }

    pub fn contract(&self) -> AsusdAuraContract {
        self.contract
    }

    pub fn supports_rgb(&self) -> bool {
        self.controls.can_set_rgb()
    }

    pub fn supports_brightness(&self) -> bool {
        self.controls.supports_brightness()
    }

    pub fn can_set_mode(&self) -> bool {
        self.controls.mode_set_property.is_some() || self.controls.mode_set_method.is_some()
    }

    pub fn can_set_brightness(&self) -> bool {
        self.controls.can_set_brightness()
    }

    pub fn supports_speed(&self) -> bool {
        matches!(self.contract, AsusdAuraContract::V6_3_8ToV6_4_0)
    }

    pub fn supported_zones(&self) -> Vec<String> {
        Vec::new()
    }

    pub fn supported_modes_hint(&self) -> Vec<LightingMode> {
        self.controls.known_supported_modes.clone()
    }

    pub fn capability_note(&self) -> String {
        let rgb = if self.supports_rgb() {
            "available"
        } else if self.controls.has_rgb_surface() {
            "read-only or not writable"
        } else {
            "not exposed by asusd"
        };
        let modes = if self.controls.supports_modes() {
            "available"
        } else {
            "not exposed by asusd"
        };
        let brightness = if self.controls.supports_brightness() {
            "available"
        } else {
            "not exposed by asusd"
        };
        format!("Aura/RGB provider: rgb={rgb}, modes={modes}, brightness={brightness}.")
    }

    pub async fn read_state(&self) -> RogResult<LightingState> {
        let proxy = self.proxy().await?;
        let mut last_error = None;

        let mode = match self.read_mode(&proxy).await {
            Ok(mode) => mode,
            Err(e) => {
                if !matches!(e, RogError::NotSupported(_)) {
                    last_error = Some(format!("mode read failed: {e}"));
                }
                None
            }
        };

        let supported_modes = match self.read_supported_modes(&proxy).await {
            Ok(modes) if !modes.is_empty() => modes,
            Ok(_) => self.controls.known_supported_modes.clone(),
            Err(e) => {
                if !matches!(e, RogError::NotSupported(_)) {
                    last_error = Some(format!("supported mode read failed: {e}"));
                }
                self.controls.known_supported_modes.clone()
            }
        };

        let rgb = match self.read_rgb(&proxy).await {
            Ok(rgb) => rgb,
            Err(e) => {
                if self.controls.has_rgb_surface() && !matches!(e, RogError::NotSupported(_)) {
                    last_error = Some(format!("RGB read failed: {e}"));
                }
                None
            }
        };

        let brightness = match self.read_brightness(&proxy).await {
            Ok(value) => value,
            Err(e) => {
                if self.controls.supports_brightness() && !matches!(e, RogError::NotSupported(_)) {
                    last_error = Some(format!("Aura brightness read failed: {e}"));
                }
                None
            }
        };

        let supported_zones = match self.read_supported_zones(&proxy).await {
            Ok(zones) => zones,
            Err(e) => {
                if !matches!(e, RogError::NotSupported(_)) {
                    last_error = Some(format!("supported Aura zones read failed: {e}"));
                }
                Vec::new()
            }
        };

        let effect = match self
            .read_effect_wire(&proxy)
            .await
            .and_then(effect_from_wire)
        {
            Ok(effect) => Some(effect),
            Err(e) => {
                last_error = Some(format!("structured Aura effect read failed: {e}"));
                None
            }
        };
        let secondary_rgb = effect.as_ref().map(|effect| effect.secondary);
        let speed = effect.as_ref().map(|effect| match effect.speed {
            AsusdAuraSpeed::Low => LightingSpeed::Slow,
            AsusdAuraSpeed::Med => LightingSpeed::Medium,
            AsusdAuraSpeed::High => LightingSpeed::Fast,
        });
        let direction = effect.as_ref().map(|effect| match effect.direction {
            AsusdAuraDirection::Right => LightingDirection::Right,
            AsusdAuraDirection::Left => LightingDirection::Left,
            AsusdAuraDirection::Up => LightingDirection::Up,
            AsusdAuraDirection::Down => LightingDirection::Down,
        });
        let active_zone = effect
            .as_ref()
            .map(|effect| LightingZone::from_backend_label(&effect.zone.label()));
        let zones = supported_zones
            .iter()
            .map(|zone| LightingZone::from_backend_label(zone))
            .collect::<Vec<_>>();
        let supported_speeds = vec![
            LightingSpeed::Slow,
            LightingSpeed::Medium,
            LightingSpeed::Fast,
        ];
        let supported_directions = vec![
            LightingDirection::Right,
            LightingDirection::Left,
            LightingDirection::Up,
            LightingDirection::Down,
        ];

        let status = if last_error.is_some() {
            "backend_error"
        } else if self.supports_rgb() {
            "available"
        } else if self.controls.has_rgb_surface() {
            "rgb_read_only"
        } else {
            "rgb_not_exposed"
        };

        Ok(LightingState {
            backend: "asusd-aura".to_string(),
            backend_kind: LightingBackendKind::AsusdAura,
            capabilities: LightingCaps {
                backend: LightingBackendKind::AsusdAura,
                supports_brightness: self.controls.supports_brightness(),
                supports_rgb: self.supports_rgb(),
                supports_argb: !zones.is_empty(),
                supports_zones: !zones.is_empty(),
                supported_modes: supported_modes.clone(),
                supported_speeds: supported_speeds.clone(),
                supported_directions,
                zones,
                writable: self.controls.can_set_any(),
                ..LightingCaps::default()
            },
            device: self.endpoint.endpoint_tag(),
            brightness,
            max_brightness: None,
            mode,
            supports_brightness: self.controls.supports_brightness(),
            supports_modes: self.controls.supports_modes(),
            supported_modes,
            supports_rgb: self.supports_rgb(),
            rgb,
            secondary_rgb,
            supports_speed: self.supports_speed(),
            supported_speeds: vec!["Low".to_string(), "Med".to_string(), "High".to_string()],
            speed,
            direction,
            supported_zones,
            active_zone,
            writable: self.controls.can_set_any(),
            direct_writable: self.controls.can_set_any(),
            privileged_writable: false,
            authorization_required: false,
            authorization: "not_required".to_string(),
            fallback_reason: None,
            status: status.to_string(),
            last_error,
            last_apply_outcome: None,
        })
    }

    pub async fn set_mode(&self, mode: LightingMode) -> RogResult<()> {
        let proxy = self.proxy().await?;
        let mode_id = mode_to_asusd_id(&mode).ok_or_else(|| {
            RogError::NotSupported(format!(
                "Aura lighting mode '{}' has no verified asusd 6.3.8/6.4.0 mode ID.",
                mode.label()
            ))
        })?;
        let supported = self.read_supported_mode_ids(&proxy).await?;
        if supported.is_empty() {
            return Err(RogError::NotSupported(
                "Aura backend did not report supported lighting modes; refusing an unverified mode write."
                    .to_string(),
            ));
        }
        if !supported.contains(&mode_id) {
            let supported = supported
                .iter()
                .map(|id| mode_from_asusd_id(*id).label())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(RogError::NotSupported(format!(
                "Aura lighting mode '{}' is not reported by this backend (supported: {supported}).",
                mode.label()
            )));
        }

        let mut effect = self.read_effect_wire(&proxy).await?;
        effect.0 = mode_id;
        self.write_effect_wire(&proxy, effect).await
    }

    pub async fn set_rgb(&self, rgb: RgbColor) -> RogResult<()> {
        let proxy = self.proxy().await?;
        let mut effect = self.read_effect_wire(&proxy).await?;
        effect.2 = (rgb.r, rgb.g, rgb.b);
        self.write_effect_wire(&proxy, effect).await
    }

    /// Apply one complete basic Aura effect through the verified structured
    /// property. Existing `set_mode` and `set_rgb` remain read-modify-write
    /// helpers for callers which only want to change one field.
    pub async fn set_effect(&self, effect: AsusdAuraEffect) -> RogResult<()> {
        let proxy = self.proxy().await?;
        let mode_id = mode_to_asusd_id(&effect.mode).ok_or_else(|| {
            RogError::NotSupported(format!(
                "Aura lighting mode '{}' has no verified asusd mode ID.",
                effect.mode.label()
            ))
        })?;
        let zone_id = effect.zone.id().ok_or_else(|| {
            RogError::NotSupported("unknown ASUS Aura zones are read-only".to_string())
        })?;

        let supported_modes = self.read_supported_mode_ids(&proxy).await?;
        if !supported_modes.contains(&mode_id) {
            return Err(RogError::NotSupported(format!(
                "Aura lighting mode '{}' is not advertised by asusd.",
                effect.mode.label()
            )));
        }
        if zone_id != 0 {
            let supported_zones = self.read_supported_zone_ids(&proxy).await?;
            if !supported_zones.contains(&zone_id) {
                return Err(RogError::NotSupported(format!(
                    "Aura zone '{}' is not advertised by asusd.",
                    effect.zone.label()
                )));
            }
        }

        self.write_effect_wire(
            &proxy,
            (
                mode_id,
                zone_id,
                (effect.primary.r, effect.primary.g, effect.primary.b),
                (effect.secondary.r, effect.secondary.g, effect.secondary.b),
                effect.speed.as_wire().to_string(),
                effect.direction.as_wire().to_string(),
            ),
        )
        .await
    }

    pub async fn set_brightness(&self, brightness: u32) -> RogResult<()> {
        if brightness > 3 {
            return Err(RogError::InvalidInput(format!(
                "asusd Aura brightness {brightness} is outside the verified 0..=3 range."
            )));
        }
        let proxy = self.proxy().await?;
        if let Some(prop) = &self.controls.brightness_set_property {
            return set_u32_property(&proxy, prop, brightness).await;
        }
        if let Some(method) = &self.controls.brightness_set_method {
            return call_set_u32_method(&proxy, method, brightness).await;
        }

        Err(RogError::NotSupported(
            "asusd Aura backend does not expose writable brightness.".to_string(),
        ))
    }

    async fn proxy(&self) -> RogResult<Proxy<'_>> {
        Proxy::new(
            &self.conn,
            self.endpoint.service.as_str(),
            self.endpoint.path.as_str(),
            self.endpoint.interface.as_str(),
        )
        .await
        .map_err(|e| map_dbus_error("build asusd Aura proxy", e))
    }

    async fn read_mode(&self, proxy: &Proxy<'_>) -> RogResult<Option<LightingMode>> {
        Ok(Some(mode_from_asusd_id(
            self.read_effect_wire(proxy).await?.0,
        )))
    }

    async fn read_supported_modes(&self, proxy: &Proxy<'_>) -> RogResult<Vec<LightingMode>> {
        Ok(self
            .read_supported_mode_ids(proxy)
            .await?
            .into_iter()
            .map(mode_from_asusd_id)
            .collect())
    }

    async fn read_rgb(&self, proxy: &Proxy<'_>) -> RogResult<Option<RgbColor>> {
        let colour = self.read_effect_wire(proxy).await?.2;
        Ok(Some(RgbColor::new(colour.0, colour.1, colour.2)))
    }

    async fn read_brightness(&self, proxy: &Proxy<'_>) -> RogResult<Option<u32>> {
        let Some(prop) = &self.controls.brightness_get_property else {
            return Err(RogError::NotSupported(
                "Aura brightness is not readable.".to_string(),
            ));
        };
        let value: OwnedValue = proxy
            .get_property(&prop.name)
            .await
            .map_err(|e| map_dbus_error("read Aura brightness", e))?;
        Ok(value_to_u32(&value))
    }

    async fn read_effect_wire(&self, proxy: &Proxy<'_>) -> RogResult<AsusdAuraEffectWire> {
        proxy
            .get_property("LedModeData")
            .await
            .map_err(|e| map_dbus_error("read structured Aura effect", e))
    }

    async fn write_effect_wire(
        &self,
        proxy: &Proxy<'_>,
        effect: AsusdAuraEffectWire,
    ) -> RogResult<()> {
        validate_effect_wire(&effect)?;
        proxy
            .set_property("LedModeData", effect)
            .await
            .map_err(|e| map_fdo_error("set structured Aura effect", e))
    }

    pub async fn read_effect(&self) -> RogResult<AsusdAuraEffect> {
        let proxy = self.proxy().await?;
        effect_from_wire(self.read_effect_wire(&proxy).await?)
    }

    async fn read_supported_mode_ids(&self, proxy: &Proxy<'_>) -> RogResult<Vec<u32>> {
        proxy
            .get_property("SupportedBasicModes")
            .await
            .map_err(|e| map_dbus_error("read Aura supported modes", e))
    }

    async fn read_supported_zone_ids(&self, proxy: &Proxy<'_>) -> RogResult<Vec<u32>> {
        proxy
            .get_property("SupportedBasicZones")
            .await
            .map_err(|e| map_dbus_error("read Aura supported zones", e))
    }

    async fn read_supported_zones(&self, proxy: &Proxy<'_>) -> RogResult<Vec<String>> {
        Ok(self
            .read_supported_zone_ids(proxy)
            .await?
            .into_iter()
            .map(|id| AsusdAuraZone::from_id(id).label())
            .collect())
    }
}

#[derive(Debug, Clone, Default)]
struct AuraControls {
    mode_get_property: Option<DbusProperty>,
    mode_set_property: Option<DbusProperty>,
    mode_set_method: Option<DbusMethod>,
    supported_modes_property: Option<DbusProperty>,
    rgb_get_property: Option<DbusProperty>,
    rgb_set_property: Option<DbusProperty>,
    rgb_set_method: Option<DbusMethod>,
    brightness_get_property: Option<DbusProperty>,
    brightness_set_property: Option<DbusProperty>,
    brightness_set_method: Option<DbusMethod>,
    known_supported_modes: Vec<LightingMode>,
}

impl AuraControls {
    fn candidate_from_interface(path: &str, iface: &InterfaceInfo) -> Option<Self> {
        let mut controls = Self::default();

        for prop in &iface.properties {
            let key = normalize_name(&prop.name);
            if is_supported_modes_name(&key) {
                controls.supported_modes_property = Some(prop.clone());
            } else if is_mode_name(&key) {
                if prop.readable {
                    controls.mode_get_property = Some(prop.clone());
                }
                if prop.writable {
                    controls.mode_set_property = Some(prop.clone());
                }
            } else if is_rgb_name(&key) {
                if prop.readable {
                    controls.rgb_get_property = Some(prop.clone());
                }
                if prop.writable {
                    controls.rgb_set_property = Some(prop.clone());
                }
            } else if is_brightness_name(&key) {
                if prop.readable {
                    controls.brightness_get_property = Some(prop.clone());
                }
                if prop.writable {
                    controls.brightness_set_property = Some(prop.clone());
                }
            }
        }

        for method in &iface.methods {
            let key = normalize_name(&method.name);
            if is_rgb_setter_name(&key) {
                controls.rgb_set_method = Some(method.clone());
            } else if is_mode_setter_name(&key) {
                controls.mode_set_method = Some(method.clone());
            } else if is_brightness_setter_name(&key) {
                controls.brightness_set_method = Some(method.clone());
            }
        }

        controls.known_supported_modes = supported_modes_from_names(iface);

        let score = aura_score(path, iface);
        let has_controls = controls.has_rgb_surface() || controls.supports_modes();

        if score >= 8 && has_controls {
            Some(controls)
        } else {
            None
        }
    }

    fn has_rgb_surface(&self) -> bool {
        self.rgb_get_property.is_some()
            || self.rgb_set_property.is_some()
            || self.rgb_set_method.is_some()
    }

    fn can_set_rgb(&self) -> bool {
        self.rgb_set_property.is_some() || self.rgb_set_method.is_some()
    }

    fn supports_modes(&self) -> bool {
        self.mode_get_property.is_some()
            || self.mode_set_property.is_some()
            || self.mode_set_method.is_some()
            || self.supported_modes_property.is_some()
            || !self.known_supported_modes.is_empty()
    }

    fn supports_brightness(&self) -> bool {
        self.brightness_get_property.is_some()
            || self.brightness_set_property.is_some()
            || self.brightness_set_method.is_some()
    }

    fn can_set_brightness(&self) -> bool {
        self.brightness_set_property.is_some() || self.brightness_set_method.is_some()
    }

    fn can_set_any(&self) -> bool {
        self.can_set_rgb()
            || self.mode_set_property.is_some()
            || self.mode_set_method.is_some()
            || self.can_set_brightness()
    }
}

fn verified_controls_for_interface(
    service: &str,
    path: &str,
    iface: &InterfaceInfo,
    _candidate: AuraControls,
) -> Option<(AsusdAuraContract, AuraControls)> {
    if service != ASUSD_SERVICE
        || iface.name != ASUSD_AURA_INTERFACE
        || !is_asusd_aura_device_path(path)
    {
        return None;
    }

    let led_mode_data = exact_property(iface, "LedModeData", "(uu(yyy)(yyy)ss)", true, true)?;
    let led_mode = exact_property(iface, "LedMode", "u", true, true)?;
    let supported_modes = exact_property(iface, "SupportedBasicModes", "au", true, false)?;
    exact_property(iface, "SupportedBasicZones", "au", true, false)?;

    // These are part of the current interface. We verify their signatures so
    // that an Aura-looking but incompatible API cannot become writable. This
    // provider deliberately has no call site for DirectAddressingRaw.
    exact_method(iface, "AllModeData", &[], &["a{u(uu(yyy)(yyy)ss)}"])?;
    exact_method(iface, "DirectAddressingRaw", &["aay"], &[])?;

    let brightness = optional_exact_property(iface, "Brightness", "u", true, true)?;
    optional_exact_property(iface, "SupportedBrightness", "au", true, false)?;

    let controls = AuraControls {
        mode_get_property: Some(led_mode_data.clone()),
        mode_set_property: Some(led_mode),
        supported_modes_property: Some(supported_modes),
        rgb_get_property: Some(led_mode_data.clone()),
        rgb_set_property: Some(led_mode_data),
        brightness_get_property: brightness.clone(),
        brightness_set_property: brightness,
        ..AuraControls::default()
    };

    Some((AsusdAuraContract::V6_3_8ToV6_4_0, controls))
}

fn is_asusd_aura_device_path(path: &str) -> bool {
    path.strip_prefix(ASUSD_AURA_PATH_PREFIX)
        .is_some_and(|device| !device.is_empty() && !device.contains('/'))
}

fn exact_property(
    iface: &InterfaceInfo,
    name: &str,
    signature: &str,
    readable: bool,
    writable: bool,
) -> Option<DbusProperty> {
    iface
        .properties
        .iter()
        .find(|prop| {
            prop.name == name
                && prop.signature == signature
                && prop.readable == readable
                && prop.writable == writable
        })
        .cloned()
}

fn optional_exact_property(
    iface: &InterfaceInfo,
    name: &str,
    signature: &str,
    readable: bool,
    writable: bool,
) -> Option<Option<DbusProperty>> {
    let properties = iface
        .properties
        .iter()
        .filter(|prop| prop.name == name)
        .collect::<Vec<_>>();
    if properties.is_empty() {
        return Some(None);
    }
    if properties.len() != 1 {
        return None;
    }
    exact_property(iface, name, signature, readable, writable).map(Some)
}

fn exact_method(
    iface: &InterfaceInfo,
    name: &str,
    input_types: &[&str],
    output_types: &[&str],
) -> Option<()> {
    let method = iface.methods.iter().find(|method| method.name == name)?;
    (method.input_types == input_types && method.output_types == output_types).then_some(())
}

#[derive(Debug, Clone)]
struct DbusNode {
    path: String,
    xml: String,
}

#[derive(Debug, Clone, Default)]
struct IntrospectionWalk {
    nodes: Vec<DbusNode>,
    paths_checked: Vec<String>,
    errors: Vec<String>,
}

#[derive(Debug, Clone)]
struct InterfaceInfo {
    name: String,
    methods: Vec<DbusMethod>,
    properties: Vec<DbusProperty>,
}

#[derive(Debug, Clone)]
struct DbusMethod {
    name: String,
    input_types: Vec<String>,
    output_types: Vec<String>,
}

#[derive(Debug, Clone)]
struct DbusProperty {
    name: String,
    signature: String,
    readable: bool,
    writable: bool,
}

fn service_candidates_from_names(names: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let re = Regex::new("(?i)asus|rog|aura|keyboard|kbd|led|rgb").unwrap();
    for name in names {
        if !name.starts_with(':') && re.is_match(name) {
            push_unique(&mut out, name.clone());
        }
    }

    out
}

async fn walk_introspection(
    conn: &zbus::Connection,
    service: &str,
    root: &str,
) -> IntrospectionWalk {
    let mut out = IntrospectionWalk::default();
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::from([(root.to_string(), 0_usize)]);

    while let Some((path, depth)) = queue.pop_front() {
        if out.nodes.len() >= PROBE_MAX_NODES
            || depth > PROBE_MAX_DEPTH
            || !visited.insert(path.clone())
        {
            continue;
        }

        push_unique(&mut out.paths_checked, format!("{service}:{path}"));

        let xml = match introspect(conn, service, &path).await {
            Ok(xml) => xml,
            Err(e) => {
                push_unique(
                    &mut out.errors,
                    format!("introspection failed for {service}:{path}: {e}"),
                );
                continue;
            }
        };
        let children = parse_children(&xml);
        out.nodes.push(DbusNode {
            path: path.clone(),
            xml,
        });

        for child in children {
            let next = if path == "/" {
                format!("/{child}")
            } else {
                format!("{path}/{child}")
            };
            queue.push_back((next, depth + 1));
        }
    }

    out
}

async fn walk_managed_objects(conn: &zbus::Connection, service: &str) -> IntrospectionWalk {
    let mut out = IntrospectionWalk::default();
    let manager = match ObjectManagerProxy::new(conn, service, "/").await {
        Ok(manager) => manager,
        Err(e) => {
            out.errors
                .push(format!("build ObjectManager proxy for {service}: {e}"));
            return out;
        }
    };
    let managed = match timeout(
        Duration::from_millis(PROBE_TIMEOUT_MS),
        manager.get_managed_objects(),
    )
    .await
    {
        Ok(Ok(managed)) => managed,
        Ok(Err(e)) => {
            out.errors
                .push(format!("read ObjectManager objects for {service}: {e}"));
            return out;
        }
        Err(_) => {
            out.errors
                .push(format!("ObjectManager query timed out for {service}"));
            return out;
        }
    };

    let mut paths = managed.keys().map(ToString::to_string).collect::<Vec<_>>();
    paths.sort();
    paths.truncate(PROBE_MAX_NODES);
    for path in paths {
        push_unique(&mut out.paths_checked, format!("{service}:{path}"));
        match introspect(conn, service, &path).await {
            Ok(xml) => out.nodes.push(DbusNode { path, xml }),
            Err(e) => push_unique(
                &mut out.errors,
                format!("introspection failed for {service}:{path}: {e}"),
            ),
        }
    }
    out
}

fn record_interface_diagnostics(
    diagnostics: &mut AuraProbeDiagnostics,
    service: &str,
    path: &str,
    iface: &InterfaceInfo,
) {
    let tag = format!("{service}:{path}:{}", iface.name);
    push_unique(&mut diagnostics.interfaces_detected, tag.clone());

    let name_key = normalize_name(&iface.name);
    if name_key.contains("aura") {
        diagnostics.aura_interface_detected = true;
    }
    if name_key.contains("keyboard") || name_key.contains("kbd") {
        diagnostics.keyboard_interface_detected = true;
    }

    let aura_like = aura_score(path, iface) >= 8;
    if aura_like {
        push_unique(&mut diagnostics.potential_aura_interfaces, tag.clone());
    }

    for method in &iface.methods {
        let key = normalize_name(&method.name);
        let method = format!("{tag}.{}({})", method.name, method.input_types.join(","));
        if is_rgb_name(&key) {
            push_unique(&mut diagnostics.rgb_methods_detected, method.clone());
        }
        if aura_like && is_brightness_name(&key) {
            push_unique(&mut diagnostics.brightness_methods_detected, method.clone());
        }
        if aura_like && is_mode_name(&key) {
            push_unique(&mut diagnostics.mode_methods_detected, method.clone());
        }
        if aura_like && is_speed_name(&key) {
            push_unique(&mut diagnostics.speed_methods_detected, method.clone());
        }
        if aura_like && is_zone_name(&key) {
            push_unique(&mut diagnostics.zone_methods_detected, method);
        }
    }

    for prop in &iface.properties {
        let key = normalize_name(&prop.name);
        let access = match (prop.readable, prop.writable) {
            (true, true) => "readwrite",
            (true, false) => "read",
            (false, true) => "write",
            (false, false) => "none",
        };
        let property = format!("{tag}.{}:{}:{access}", prop.name, prop.signature);
        if is_rgb_name(&key) {
            push_unique(&mut diagnostics.rgb_properties_detected, property.clone());
        }
        if aura_like && is_brightness_name(&key) {
            push_unique(
                &mut diagnostics.brightness_properties_detected,
                property.clone(),
            );
        }
        if aura_like && (is_mode_name(&key) || is_supported_modes_name(&key)) {
            push_unique(&mut diagnostics.mode_properties_detected, property.clone());
        }
        if aura_like && is_speed_name(&key) {
            push_unique(&mut diagnostics.speed_properties_detected, property.clone());
        }
        if aura_like && is_zone_name(&key) {
            push_unique(&mut diagnostics.zone_properties_detected, property);
        }
    }
}

async fn introspect(conn: &zbus::Connection, service: &str, path: &str) -> RogResult<String> {
    let proxy = Proxy::new(conn, service, path, "org.freedesktop.DBus.Introspectable")
        .await
        .map_err(|e| map_dbus_error("build introspection proxy", e))?;

    timeout(
        Duration::from_millis(PROBE_TIMEOUT_MS),
        proxy.call::<_, _, String>("Introspect", &()),
    )
    .await
    .map_err(|_| RogError::TransientFailure("Aura DBus introspection timed out".to_string()))?
    .map_err(|e| map_dbus_error("introspect asusd Aura endpoint", e))
}

fn parse_interfaces(xml: &str) -> Vec<InterfaceInfo> {
    let iface_re = Regex::new(r#"(?s)<interface\s+([^>]*)>(.*?)</interface>"#).unwrap();
    iface_re
        .captures_iter(xml)
        .filter_map(|caps| {
            let attrs = caps.get(1)?.as_str();
            let body = caps.get(2)?.as_str();
            let name = attr_value(attrs, "name")?;
            if name.starts_with("org.freedesktop.DBus") {
                return None;
            }

            Some(InterfaceInfo {
                name,
                methods: parse_methods(body),
                properties: parse_properties(body),
            })
        })
        .collect()
}

fn parse_methods(body: &str) -> Vec<DbusMethod> {
    let method_re = Regex::new(r#"(?s)<method\s+([^>]*)>(.*?)</method>"#).unwrap();
    let arg_re = Regex::new(r#"<arg\s+([^>]*)/?>"#).unwrap();
    method_re
        .captures_iter(body)
        .filter_map(|caps| {
            let attrs = caps.get(1)?.as_str();
            let body = caps.get(2)?.as_str();
            let name = attr_value(attrs, "name")?;
            let input_types = arg_re
                .captures_iter(body)
                .filter_map(|arg_caps| {
                    let attrs = arg_caps.get(1)?.as_str();
                    let direction =
                        attr_value(attrs, "direction").unwrap_or_else(|| "in".to_string());
                    if direction == "out" {
                        None
                    } else {
                        attr_value(attrs, "type")
                    }
                })
                .collect();
            let output_types = arg_re
                .captures_iter(body)
                .filter_map(|arg_caps| {
                    let attrs = arg_caps.get(1)?.as_str();
                    (attr_value(attrs, "direction").as_deref() == Some("out"))
                        .then(|| attr_value(attrs, "type"))
                        .flatten()
                })
                .collect();
            Some(DbusMethod {
                name,
                input_types,
                output_types,
            })
        })
        .collect()
}

fn parse_properties(body: &str) -> Vec<DbusProperty> {
    let prop_re = Regex::new(r#"<property\s+([^>]*)/?>"#).unwrap();
    prop_re
        .captures_iter(body)
        .filter_map(|caps| {
            let attrs = caps.get(1)?.as_str();
            let name = attr_value(attrs, "name")?;
            let signature = attr_value(attrs, "type").unwrap_or_default();
            let access = attr_value(attrs, "access").unwrap_or_else(|| "read".to_string());
            Some(DbusProperty {
                name,
                signature,
                readable: access.contains("read"),
                writable: access.contains("write"),
            })
        })
        .collect()
}

fn parse_children(xml: &str) -> Vec<String> {
    let node_re = Regex::new(r#"<node\s+name="([^"]+)""#).unwrap();
    node_re
        .captures_iter(xml)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(r#"{name}="([^"]*)""#)).ok()?;
    re.captures(attrs)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn aura_score(path: &str, iface: &InterfaceInfo) -> usize {
    let mut haystack = format!("{path} {}", iface.name);
    for method in &iface.methods {
        haystack.push(' ');
        haystack.push_str(&method.name);
    }
    for prop in &iface.properties {
        haystack.push(' ');
        haystack.push_str(&prop.name);
    }
    let haystack = normalize_name(&haystack);

    let mut score: usize = 0;
    if haystack.contains("aura") {
        score += 12;
    }
    if haystack.contains("keyboard") || haystack.contains("kbd") {
        score += 8;
    }
    if haystack.contains("led") || haystack.contains("lighting") {
        score += 6;
    }
    if haystack.contains("rgb") || haystack.contains("color") || haystack.contains("colour") {
        score += 5;
    }
    score
}

fn normalize_name(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn is_mode_name(key: &str) -> bool {
    (key.contains("mode") || key.contains("effect"))
        && !key.contains("supported")
        && !key.contains("available")
        && !key.contains("profile")
}

fn is_supported_modes_name(key: &str) -> bool {
    (key.contains("supported")
        || key.contains("available")
        || key.contains("list")
        || key.ends_with("modes")
        || key.contains("modechoices"))
        && (key.contains("mode") || key.contains("effect"))
        && !key.contains("profile")
}

fn is_rgb_name(key: &str) -> bool {
    key.contains("rgb")
        || key.contains("color")
        || key.contains("colour")
        || key.contains("staticcolour")
        || key.contains("staticcolor")
}

fn is_brightness_name(key: &str) -> bool {
    key.contains("brightness") && !key.contains("screen")
}

fn is_speed_name(key: &str) -> bool {
    key.contains("speed") && !key.contains("fan")
}

fn is_zone_name(key: &str) -> bool {
    key.contains("zone")
}

fn is_mode_setter_name(key: &str) -> bool {
    key.starts_with("set") && is_mode_name(key)
}

fn is_rgb_setter_name(key: &str) -> bool {
    key.starts_with("set") && is_rgb_name(key)
}

fn is_brightness_setter_name(key: &str) -> bool {
    key.starts_with("set") && is_brightness_name(key)
}

fn supported_modes_from_names(iface: &InterfaceInfo) -> Vec<LightingMode> {
    let mut modes = Vec::new();
    for value in iface
        .properties
        .iter()
        .map(|p| p.name.as_str())
        .chain(iface.methods.iter().map(|m| m.name.as_str()))
    {
        for mode in [
            LightingMode::Off,
            LightingMode::Static,
            LightingMode::Breathe,
            LightingMode::Strobe,
            LightingMode::Rainbow,
            LightingMode::Pulse,
            LightingMode::Wave,
        ] {
            if normalize_name(value).contains(&normalize_name(&mode.label()))
                && !modes.iter().any(|m: &LightingMode| m.same_user_mode(&mode))
            {
                modes.push(mode);
            }
        }
    }
    modes
}

fn mode_from_asusd_id(id: u32) -> LightingMode {
    match id {
        0 => LightingMode::Static,
        1 => LightingMode::Breathe,
        2 => LightingMode::RainbowCycle,
        3 => LightingMode::RainbowWave,
        4 => LightingMode::Other("Stars".to_string()),
        5 => LightingMode::Other("Rain".to_string()),
        6 => LightingMode::Other("Highlight".to_string()),
        7 => LightingMode::Other("Laser".to_string()),
        8 => LightingMode::Other("Ripple".to_string()),
        10 => LightingMode::Pulse,
        11 => LightingMode::Other("Comet".to_string()),
        12 => LightingMode::Strobe,
        id => LightingMode::Other(format!("ASUS Aura mode {id}")),
    }
}

fn mode_to_asusd_id(mode: &LightingMode) -> Option<u32> {
    match mode {
        LightingMode::Static => Some(0),
        LightingMode::Breathe => Some(1),
        LightingMode::RainbowCycle | LightingMode::Rainbow => Some(2),
        LightingMode::RainbowWave | LightingMode::Wave => Some(3),
        LightingMode::Pulse => Some(10),
        LightingMode::Strobe => Some(12),
        LightingMode::Other(name) => match normalize_name(name).as_str() {
            "rainbowcycle" => Some(2),
            "rainbowwave" => Some(3),
            "star" | "stars" => Some(4),
            "rain" => Some(5),
            "highlight" => Some(6),
            "laser" => Some(7),
            "ripple" => Some(8),
            "comet" => Some(11),
            "flash" => Some(12),
            _ => None,
        },
        LightingMode::Off => None,
    }
}

fn validate_effect_wire(effect: &AsusdAuraEffectWire) -> RogResult<()> {
    if mode_to_asusd_id(&mode_from_asusd_id(effect.0)) != Some(effect.0) {
        return Err(RogError::NotSupported(format!(
            "ASUS Aura mode ID {} is not part of the verified contract.",
            effect.0
        )));
    }
    if AsusdAuraZone::from_id(effect.1).id() != Some(effect.1) {
        return Err(RogError::NotSupported(format!(
            "ASUS Aura zone ID {} is not part of the verified contract.",
            effect.1
        )));
    }
    if AsusdAuraSpeed::from_wire(&effect.4).is_none() {
        return Err(RogError::NotSupported(format!(
            "ASUS Aura speed '{}' is not Low, Med, or High.",
            effect.4
        )));
    }
    if AsusdAuraDirection::from_wire(&effect.5).is_none() {
        return Err(RogError::NotSupported(format!(
            "ASUS Aura direction '{}' is not Right, Left, Up, or Down.",
            effect.5
        )));
    }
    Ok(())
}

fn effect_from_wire(effect: AsusdAuraEffectWire) -> RogResult<AsusdAuraEffect> {
    let speed = AsusdAuraSpeed::from_wire(&effect.4).ok_or_else(|| {
        RogError::NotSupported(format!("unknown ASUS Aura speed '{}'.", effect.4))
    })?;
    let direction = AsusdAuraDirection::from_wire(&effect.5).ok_or_else(|| {
        RogError::NotSupported(format!("unknown ASUS Aura direction '{}'.", effect.5))
    })?;
    Ok(AsusdAuraEffect {
        mode: mode_from_asusd_id(effect.0),
        zone: AsusdAuraZone::from_id(effect.1),
        primary: RgbColor::new(effect.2 .0, effect.2 .1, effect.2 .2),
        secondary: RgbColor::new(effect.3 .0, effect.3 .1, effect.3 .2),
        speed,
        direction,
    })
}

fn value_to_u32(value: &OwnedValue) -> Option<u32> {
    u32::try_from(value)
        .ok()
        .or_else(|| u8::try_from(value).ok().map(|v| v as u32))
        .or_else(|| u16::try_from(value).ok().map(|v| v as u32))
        .or_else(|| {
            u64::try_from(value)
                .ok()
                .and_then(|v| u32::try_from(v).ok())
        })
        .or_else(|| {
            i32::try_from(value)
                .ok()
                .and_then(|v| u32::try_from(v).ok())
        })
}

async fn set_u32_property(proxy: &Proxy<'_>, prop: &DbusProperty, value: u32) -> RogResult<()> {
    match prop.signature.as_str() {
        "y" => proxy
            .set_property(&prop.name, value.min(u8::MAX as u32) as u8)
            .await
            .map_err(|e| map_fdo_error("set Aura brightness", e)),
        "u" => proxy
            .set_property(&prop.name, value)
            .await
            .map_err(|e| map_fdo_error("set Aura brightness", e)),
        "i" => proxy
            .set_property(&prop.name, value.min(i32::MAX as u32) as i32)
            .await
            .map_err(|e| map_fdo_error("set Aura brightness", e)),
        sig => Err(RogError::NotSupported(format!(
            "Aura brightness property '{}' has unsupported DBus signature '{sig}'.",
            prop.name
        ))),
    }
}

async fn call_set_u32_method(proxy: &Proxy<'_>, method: &DbusMethod, value: u32) -> RogResult<()> {
    match method.input_types.as_slice() {
        [sig] if sig == "y" => proxy
            .call::<_, _, ()>(method.name.as_str(), &(value.min(u8::MAX as u32) as u8))
            .await
            .map_err(|e| map_dbus_error("call Aura brightness setter", e)),
        [sig] if sig == "u" => proxy
            .call::<_, _, ()>(method.name.as_str(), &value)
            .await
            .map_err(|e| map_dbus_error("call Aura brightness setter", e)),
        [sig] if sig == "i" => proxy
            .call::<_, _, ()>(method.name.as_str(), &(value.min(i32::MAX as u32) as i32))
            .await
            .map_err(|e| map_dbus_error("call Aura brightness setter", e)),
        _ => Err(RogError::NotSupported(format!(
            "Aura brightness method '{}' has unsupported input signature {:?}.",
            method.name, method.input_types
        ))),
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !value.trim().is_empty() && !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn map_dbus_error(ctx: &str, e: zbus::Error) -> RogError {
    classify_error(ctx, &e.to_string())
}

fn map_fdo_error(ctx: &str, e: zbus::fdo::Error) -> RogError {
    classify_error(ctx, &e.to_string())
}

fn classify_error(ctx: &str, msg: &str) -> RogError {
    let lower = msg.to_ascii_lowercase();
    debug!("{ctx}: {msg}");
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

#[cfg(test)]
mod tests {
    use super::*;

    const ASUSD_AURA_XML: &str = include_str!("fixtures/asusd-aura-6.3.8.xml");

    fn match_contract(
        service: &str,
        path: &str,
        xml: &str,
    ) -> Option<(AsusdAuraContract, AuraControls)> {
        let iface = parse_interfaces(xml).remove(0);
        let candidate = AuraControls::candidate_from_interface(path, &iface)?;
        verified_controls_for_interface(service, path, &iface, candidate)
    }

    #[test]
    fn captured_asusd_6_3_8_contract_is_writable() {
        let (contract, controls) =
            match_contract(ASUSD_SERVICE, "/xyz/ljones/aura/19b6_3_1", ASUSD_AURA_XML).unwrap();
        assert_eq!(contract, AsusdAuraContract::V6_3_8ToV6_4_0);
        assert!(controls.can_set_rgb());
        assert!(controls.supports_modes());
        assert!(controls.can_set_brightness());
        assert!(controls.supported_modes_property.is_some());
    }

    #[test]
    fn wrong_service_path_and_interface_are_diagnostic_only() {
        assert!(match_contract(
            "org.asuslinux.Daemon",
            "/xyz/ljones/aura/19b6_3_1",
            ASUSD_AURA_XML,
        )
        .is_none());
        assert!(match_contract(ASUSD_SERVICE, "/xyz/ljones/Aura", ASUSD_AURA_XML).is_none());
        assert!(match_contract(ASUSD_SERVICE, "/xyz/ljones/aura", ASUSD_AURA_XML).is_none());

        let wrong_interface = ASUSD_AURA_XML.replace("xyz.ljones.Aura", "xyz.ljones.Aura2");
        assert!(
            match_contract(ASUSD_SERVICE, "/xyz/ljones/aura/19b6_3_1", &wrong_interface,).is_none()
        );
    }

    #[test]
    fn wrong_property_method_access_and_signature_are_diagnostic_only() {
        let cases = [
            ASUSD_AURA_XML.replace("name=\"LedModeData\"", "name=\"ModeData\""),
            ASUSD_AURA_XML.replace(
                "name=\"DirectAddressingRaw\"",
                "name=\"DirectAddressingBytes\"",
            ),
            ASUSD_AURA_XML.replace(
                "name=\"LedModeData\" type=\"(uu(yyy)(yyy)ss)\" access=\"readwrite\"",
                "name=\"LedModeData\" type=\"(uu(yyy)(yyy)ss)\" access=\"read\"",
            ),
            ASUSD_AURA_XML.replace("type=\"(uu(yyy)(yyy)ss)\"", "type=\"(u(yyy)ss)\""),
            ASUSD_AURA_XML.replace(
                "type=\"aay\" direction=\"in\"",
                "type=\"ay\" direction=\"in\"",
            ),
        ];
        for xml in cases {
            assert!(match_contract(ASUSD_SERVICE, "/xyz/ljones/aura/19b6_3_1", &xml,).is_none());
        }
    }

    #[test]
    fn captured_contract_remains_visible_in_diagnostics() {
        let iface = parse_interfaces(ASUSD_AURA_XML).remove(0);

        let mut diagnostics = AuraProbeDiagnostics::default();
        record_interface_diagnostics(
            &mut diagnostics,
            ASUSD_SERVICE,
            "/xyz/ljones/aura/19b6_3_1",
            &iface,
        );
        assert!(diagnostics
            .zone_properties_detected
            .iter()
            .any(|property| property.contains("SupportedBasicZones:au:read")));
        assert!(diagnostics
            .brightness_properties_detected
            .iter()
            .any(|property| property.contains("Brightness:u:readwrite")));
        assert!(diagnostics
            .mode_properties_detected
            .iter()
            .any(|property| property.contains("SupportedBasicModes:au:read")));
        assert!(!diagnostics.verified_interface_detected);
    }

    #[test]
    fn platform_profile_interface_is_not_treated_as_aura() {
        let xml = r#"
            <node>
              <interface name="xyz.ljones.Platform">
                <property name="PlatformProfile" type="u" access="readwrite"/>
                <property name="PlatformProfileChoices" type="au" access="read"/>
              </interface>
            </node>
        "#;

        let iface = parse_interfaces(xml).remove(0);
        assert!(AuraControls::candidate_from_interface("/xyz/ljones", &iface).is_none());
    }

    #[test]
    fn unknown_numeric_mode_ids_are_not_writable() {
        assert_eq!(
            mode_to_asusd_id(&mode_from_asusd_id(99)),
            None,
            "unknown numeric IDs must remain diagnostic-only"
        );
    }

    #[test]
    fn current_mode_zone_speed_and_direction_maps_are_exact() {
        assert_eq!(mode_to_asusd_id(&LightingMode::Static), Some(0));
        assert_eq!(mode_to_asusd_id(&LightingMode::Breathe), Some(1));
        assert_eq!(mode_to_asusd_id(&LightingMode::RainbowCycle), Some(2));
        assert_eq!(mode_to_asusd_id(&LightingMode::RainbowWave), Some(3));
        assert_eq!(mode_to_asusd_id(&LightingMode::Pulse), Some(10));
        assert_eq!(mode_to_asusd_id(&LightingMode::Strobe), Some(12));
        assert_eq!(mode_to_asusd_id(&LightingMode::Off), None);
        assert_eq!(AsusdAuraZone::from_id(6), AsusdAuraZone::LightbarLeft);
        assert_eq!(AsusdAuraZone::from_id(7), AsusdAuraZone::LightbarRight);
        assert_eq!(AsusdAuraSpeed::from_wire("Med"), Some(AsusdAuraSpeed::Med));
        assert_eq!(
            AsusdAuraDirection::from_wire("Down"),
            Some(AsusdAuraDirection::Down)
        );
    }

    #[test]
    fn structured_effect_update_rejects_unverified_strings() {
        let bad_speed = (0, 0, (1, 2, 3), (4, 5, 6), "Medium".into(), "Right".into());
        assert!(validate_effect_wire(&bad_speed).is_err());
        let bad_direction = (0, 0, (1, 2, 3), (4, 5, 6), "Med".into(), "Clockwise".into());
        assert!(validate_effect_wire(&bad_direction).is_err());
    }
}
