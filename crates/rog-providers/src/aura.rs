use std::collections::{BTreeSet, VecDeque};
use std::time::Duration;

use regex::Regex;
use rog_core::{LightingMode, LightingState, RgbColor, RogError, RogResult};
use tokio::time::timeout;
use tracing::debug;
use zbus::fdo::DBusProxy;
use zbus::zvariant::OwnedValue;
use zbus::Proxy;

const SERVICE_CANDIDATES: &[&str] = &["xyz.ljones.Asusd", "org.asuslinux.Daemon"];
const PROBE_TIMEOUT_MS: u64 = 1200;
const PROBE_MAX_DEPTH: usize = 3;
const PROBE_MAX_NODES: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuraEndpoint {
    pub service: String,
    pub path: String,
    pub interface: String,
}

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
    pub notes: Vec<String>,
}

impl AuraProbeResult {
    pub fn is_available(&self) -> bool {
        self.provider.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct AuraProvider {
    conn: zbus::Connection,
    endpoint: AuraEndpoint,
    controls: AuraControls,
}

impl AuraProvider {
    pub async fn probe_system() -> RogResult<AuraProbeResult> {
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

        let mut notes = Vec::new();
        for service in SERVICE_CANDIDATES {
            if !names.is_empty() && !names.iter().any(|n| n == service) {
                notes.push(format!(
                    "asusd Aura probe: service {service} is not present."
                ));
                continue;
            }

            let roots = service_roots(service);
            for root in roots {
                let nodes = walk_introspection(&conn, service, root).await;
                for node in nodes {
                    for iface in parse_interfaces(&node.xml) {
                        let Some(controls) = AuraControls::from_interface(&node.path, &iface)
                        else {
                            continue;
                        };

                        let endpoint = AuraEndpoint {
                            service: (*service).to_string(),
                            path: node.path.clone(),
                            interface: iface.name.clone(),
                        };
                        let provider = AuraProvider {
                            conn: conn.clone(),
                            endpoint,
                            controls,
                        };

                        notes.push(format!(
                            "ASUS Aura/RGB lighting exposed by {}.",
                            provider.endpoint.endpoint_tag()
                        ));
                        notes.push(provider.capability_note());
                        return Ok(AuraProbeResult {
                            provider: Some(provider),
                            notes,
                        });
                    }
                }
            }
        }

        if notes.is_empty() {
            notes.push("asusd Aura probe: no supported service names were detected.".to_string());
        } else {
            notes.push(
                "asusd Aura probe: no Aura/keyboard RGB interface was exposed by introspection."
                    .to_string(),
            );
        }

        Ok(AuraProbeResult {
            provider: None,
            notes,
        })
    }

    pub fn endpoint_tag(&self) -> String {
        self.endpoint.endpoint_tag()
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
            device: self.endpoint.endpoint_tag(),
            brightness,
            max_brightness: if self.controls.supports_brightness() {
                Some(3)
            } else {
                None
            },
            mode,
            supported_modes,
            supports_rgb: self.supports_rgb(),
            rgb,
            writable: self.controls.can_set_any(),
            status: status.to_string(),
            last_error,
        })
    }

    pub async fn set_mode(&self, mode: LightingMode) -> RogResult<()> {
        let proxy = self.proxy().await?;
        let supported = self.read_supported_modes(&proxy).await.unwrap_or_default();
        if !supported.is_empty() && !supported.iter().any(|m| m.same_user_mode(&mode)) {
            let supported = supported
                .iter()
                .map(LightingMode::label)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(RogError::NotSupported(format!(
                "Aura lighting mode '{}' is not reported by this backend (supported: {supported}).",
                mode.label()
            )));
        }

        if let Some(prop) = &self.controls.mode_set_property {
            return set_mode_property(&proxy, prop, &mode).await;
        }
        if let Some(method) = &self.controls.mode_set_method {
            return call_set_mode_method(&proxy, method, &mode).await;
        }

        Err(RogError::NotSupported(
            "asusd Aura backend does not expose a writable lighting mode control.".to_string(),
        ))
    }

    pub async fn set_rgb(&self, rgb: RgbColor) -> RogResult<()> {
        let proxy = self.proxy().await?;
        if let Some(prop) = &self.controls.rgb_set_property {
            return set_rgb_property(&proxy, prop, rgb).await;
        }
        if let Some(method) = &self.controls.rgb_set_method {
            return call_set_rgb_method(&proxy, method, rgb).await;
        }

        Err(RogError::NotSupported(
            "RGB colour is not exposed as a writable asusd Aura control.".to_string(),
        ))
    }

    pub async fn set_brightness(&self, brightness: u32) -> RogResult<()> {
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
        let Some(prop) = &self.controls.mode_get_property else {
            return Err(RogError::NotSupported(
                "Aura lighting mode is not readable.".to_string(),
            ));
        };
        let value: OwnedValue = proxy
            .get_property(&prop.name)
            .await
            .map_err(|e| map_dbus_error("read Aura lighting mode", e))?;
        Ok(parse_mode_value(&value))
    }

    async fn read_supported_modes(&self, proxy: &Proxy<'_>) -> RogResult<Vec<LightingMode>> {
        let Some(prop) = &self.controls.supported_modes_property else {
            return Ok(self.controls.known_supported_modes.clone());
        };
        let value: OwnedValue = proxy
            .get_property(&prop.name)
            .await
            .map_err(|e| map_dbus_error("read Aura supported modes", e))?;
        Ok(parse_mode_list_value(&value))
    }

    async fn read_rgb(&self, proxy: &Proxy<'_>) -> RogResult<Option<RgbColor>> {
        let Some(prop) = &self.controls.rgb_get_property else {
            return Err(RogError::NotSupported(
                "Aura RGB colour is not readable.".to_string(),
            ));
        };
        let value: OwnedValue = proxy
            .get_property(&prop.name)
            .await
            .map_err(|e| map_dbus_error("read Aura RGB colour", e))?;
        Ok(parse_rgb_value(&value))
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
    fn from_interface(path: &str, iface: &InterfaceInfo) -> Option<Self> {
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

#[derive(Debug, Clone)]
struct DbusNode {
    path: String,
    xml: String,
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
}

#[derive(Debug, Clone)]
struct DbusProperty {
    name: String,
    signature: String,
    readable: bool,
    writable: bool,
}

fn service_roots(service: &str) -> Vec<&'static str> {
    match service {
        "xyz.ljones.Asusd" => vec!["/xyz/ljones", "/"],
        "org.asuslinux.Daemon" => vec!["/org/asuslinux", "/"],
        _ => vec!["/"],
    }
}

async fn walk_introspection(conn: &zbus::Connection, service: &str, root: &str) -> Vec<DbusNode> {
    let mut out = Vec::new();
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::from([(root.to_string(), 0_usize)]);

    while let Some((path, depth)) = queue.pop_front() {
        if out.len() >= PROBE_MAX_NODES || depth > PROBE_MAX_DEPTH || !visited.insert(path.clone())
        {
            continue;
        }

        let Ok(xml) = introspect(conn, service, &path).await else {
            continue;
        };
        let children = parse_children(&xml);
        out.push(DbusNode {
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
            Some(DbusMethod { name, input_types })
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

fn parse_mode_value(value: &OwnedValue) -> Option<LightingMode> {
    if let Ok(text) = <&str>::try_from(value) {
        return Some(LightingMode::from_backend_label(text));
    }
    value_to_u32(value).and_then(mode_from_wire)
}

fn parse_mode_list_value(value: &OwnedValue) -> Vec<LightingMode> {
    let mut out = Vec::new();

    if let Ok(values) = Vec::<String>::try_from(value.clone()) {
        for value in values {
            push_mode(&mut out, LightingMode::from_backend_label(&value));
        }
    } else if let Ok(values) = Vec::<u8>::try_from(value.clone()) {
        for value in values {
            if let Some(mode) = mode_from_wire(value as u32) {
                push_mode(&mut out, mode);
            }
        }
    } else if let Ok(values) = Vec::<u32>::try_from(value.clone()) {
        for value in values {
            if let Some(mode) = mode_from_wire(value) {
                push_mode(&mut out, mode);
            }
        }
    } else if let Ok(values) = Vec::<OwnedValue>::try_from(value.clone()) {
        for value in values {
            if let Some(mode) = parse_mode_value(&value) {
                push_mode(&mut out, mode);
            }
        }
    }

    out
}

fn push_mode(out: &mut Vec<LightingMode>, mode: LightingMode) {
    if !out.iter().any(|existing| existing.same_user_mode(&mode)) {
        out.push(mode);
    }
}

fn mode_from_wire(value: u32) -> Option<LightingMode> {
    match value {
        0 => Some(LightingMode::Static),
        1 => Some(LightingMode::Breathe),
        2 => Some(LightingMode::Strobe),
        3 => Some(LightingMode::Rainbow),
        4 => Some(LightingMode::Pulse),
        5 => Some(LightingMode::Wave),
        255 => Some(LightingMode::Off),
        _ => None,
    }
}

fn mode_to_wire_u8(mode: &LightingMode) -> RogResult<u8> {
    match mode {
        LightingMode::Static => Ok(0),
        LightingMode::Breathe => Ok(1),
        LightingMode::Strobe => Ok(2),
        LightingMode::Rainbow => Ok(3),
        LightingMode::Pulse => Ok(4),
        LightingMode::Wave => Ok(5),
        LightingMode::Off => Ok(255),
        LightingMode::Other(value) => Err(RogError::NotSupported(format!(
            "Aura backend requires a numeric mode, and '{value}' has no known mapping."
        ))),
    }
}

fn parse_rgb_value(value: &OwnedValue) -> Option<RgbColor> {
    if let Ok(text) = <&str>::try_from(value) {
        if let Ok(rgb) = RgbColor::parse_hex(text) {
            return Some(rgb);
        }
    }
    if let Ok((r, g, b)) = <(u8, u8, u8)>::try_from(value.clone()) {
        return Some(RgbColor::new(r, g, b));
    }
    if let Ok(values) = Vec::<u8>::try_from(value.clone()) {
        if values.len() >= 3 {
            return Some(RgbColor::new(values[0], values[1], values[2]));
        }
    }
    value_to_u32(value).map(|packed| {
        RgbColor::new(
            ((packed >> 16) & 0xff) as u8,
            ((packed >> 8) & 0xff) as u8,
            (packed & 0xff) as u8,
        )
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

async fn set_mode_property(
    proxy: &Proxy<'_>,
    prop: &DbusProperty,
    mode: &LightingMode,
) -> RogResult<()> {
    match prop.signature.as_str() {
        "s" => proxy
            .set_property(&prop.name, mode.label())
            .await
            .map_err(|e| map_fdo_error("set Aura lighting mode", e)),
        "y" => proxy
            .set_property(&prop.name, mode_to_wire_u8(mode)?)
            .await
            .map_err(|e| map_fdo_error("set Aura lighting mode", e)),
        "u" => proxy
            .set_property(&prop.name, mode_to_wire_u8(mode)? as u32)
            .await
            .map_err(|e| map_fdo_error("set Aura lighting mode", e)),
        "i" => proxy
            .set_property(&prop.name, mode_to_wire_u8(mode)? as i32)
            .await
            .map_err(|e| map_fdo_error("set Aura lighting mode", e)),
        sig => Err(RogError::NotSupported(format!(
            "Aura lighting mode property '{}' has unsupported DBus signature '{sig}'.",
            prop.name
        ))),
    }
}

async fn call_set_mode_method(
    proxy: &Proxy<'_>,
    method: &DbusMethod,
    mode: &LightingMode,
) -> RogResult<()> {
    match method.input_types.as_slice() {
        [sig] if sig == "s" => proxy
            .call::<_, _, ()>(method.name.as_str(), &(mode.label()))
            .await
            .map_err(|e| map_dbus_error("call Aura mode setter", e)),
        [sig] if sig == "y" => proxy
            .call::<_, _, ()>(method.name.as_str(), &(mode_to_wire_u8(mode)?))
            .await
            .map_err(|e| map_dbus_error("call Aura mode setter", e)),
        [sig] if sig == "u" => proxy
            .call::<_, _, ()>(method.name.as_str(), &(mode_to_wire_u8(mode)? as u32))
            .await
            .map_err(|e| map_dbus_error("call Aura mode setter", e)),
        [sig] if sig == "i" => proxy
            .call::<_, _, ()>(method.name.as_str(), &(mode_to_wire_u8(mode)? as i32))
            .await
            .map_err(|e| map_dbus_error("call Aura mode setter", e)),
        _ => Err(RogError::NotSupported(format!(
            "Aura mode method '{}' has unsupported input signature {:?}.",
            method.name, method.input_types
        ))),
    }
}

async fn set_rgb_property(proxy: &Proxy<'_>, prop: &DbusProperty, rgb: RgbColor) -> RogResult<()> {
    match prop.signature.as_str() {
        "s" => proxy
            .set_property(&prop.name, rgb.to_hex())
            .await
            .map_err(|e| map_fdo_error("set Aura RGB colour", e)),
        "(yyy)" => proxy
            .set_property(&prop.name, (rgb.r, rgb.g, rgb.b))
            .await
            .map_err(|e| map_fdo_error("set Aura RGB colour", e)),
        "ay" => proxy
            .set_property(&prop.name, vec![rgb.r, rgb.g, rgb.b])
            .await
            .map_err(|e| map_fdo_error("set Aura RGB colour", e)),
        "u" => proxy
            .set_property(&prop.name, rgb_to_u32(rgb))
            .await
            .map_err(|e| map_fdo_error("set Aura RGB colour", e)),
        sig => Err(RogError::NotSupported(format!(
            "Aura RGB property '{}' has unsupported DBus signature '{sig}'.",
            prop.name
        ))),
    }
}

async fn call_set_rgb_method(
    proxy: &Proxy<'_>,
    method: &DbusMethod,
    rgb: RgbColor,
) -> RogResult<()> {
    match method.input_types.as_slice() {
        [sig] if sig == "s" => proxy
            .call::<_, _, ()>(method.name.as_str(), &(rgb.to_hex()))
            .await
            .map_err(|e| map_dbus_error("call Aura RGB setter", e)),
        [a, b, c] if a == "y" && b == "y" && c == "y" => proxy
            .call::<_, _, ()>(method.name.as_str(), &(rgb.r, rgb.g, rgb.b))
            .await
            .map_err(|e| map_dbus_error("call Aura RGB setter", e)),
        [a, b, c] if a == "u" && b == "u" && c == "u" => proxy
            .call::<_, _, ()>(
                method.name.as_str(),
                &(rgb.r as u32, rgb.g as u32, rgb.b as u32),
            )
            .await
            .map_err(|e| map_dbus_error("call Aura RGB setter", e)),
        [sig] if sig == "u" => proxy
            .call::<_, _, ()>(method.name.as_str(), &(rgb_to_u32(rgb)))
            .await
            .map_err(|e| map_dbus_error("call Aura RGB setter", e)),
        _ => Err(RogError::NotSupported(format!(
            "Aura RGB method '{}' has unsupported input signature {:?}.",
            method.name, method.input_types
        ))),
    }
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

fn rgb_to_u32(rgb: RgbColor) -> u32 {
    ((rgb.r as u32) << 16) | ((rgb.g as u32) << 8) | rgb.b as u32
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

    #[test]
    fn introspection_parser_finds_aura_controls() {
        let xml = r#"
            <node>
              <interface name="xyz.ljones.Aura">
                <method name="SetLedMode">
                  <arg name="mode" type="y" direction="in"/>
                </method>
                <method name="SetLedColour">
                  <arg name="r" type="y" direction="in"/>
                  <arg name="g" type="y" direction="in"/>
                  <arg name="b" type="y" direction="in"/>
                </method>
                <property name="LedMode" type="y" access="readwrite"/>
                <property name="LedColour" type="(yyy)" access="readwrite"/>
                <property name="SupportedLedModes" type="ay" access="read"/>
              </interface>
            </node>
        "#;

        let iface = parse_interfaces(xml).remove(0);
        let controls = AuraControls::from_interface("/xyz/ljones", &iface).unwrap();

        assert!(controls.can_set_rgb());
        assert!(controls.supports_modes());
        assert!(controls.mode_set_method.is_some());
        assert!(controls.supported_modes_property.is_some());
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
        assert!(AuraControls::from_interface("/xyz/ljones", &iface).is_none());
    }
}
