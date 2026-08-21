//! Read-only discovery and packet encoding for one verified native Aura target.
//!
//! Protocol facts were independently implemented after checking the upstream
//! ASUS Linux `asusctl` project (`rog-aura/src/builtin_modes.rs`,
//! `rog-aura/data/aura_support.ron`, and `asusd/src/aura_laptop/mod.rs`) at
//! <https://github.com/OpenGamingCollective/asusctl/tree/1c456fa3dd8fe11287289b4bbee9dd3561afc5a3>.
//! That project is MPL-2.0; no source code was copied into this
//! MIT/Apache-2.0 module.
//!
//! This module deliberately has no hidraw open/write transport. It exposes no
//! arbitrary path or byte-array write API. A privileged caller can only use the
//! typed, capability-validated encoder and must independently revalidate the
//! discovered identity before a future transport sends these fixed reports.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use rog_core::{
    LightingApplyRequest, LightingBackendKind, LightingCaps, LightingDiagnostics,
    LightingDirection, LightingHidDeviceDiagnostics, LightingMode, LightingSpeed, RgbColor,
    RogError, RogResult,
};
use sha2::{Digest, Sha256};

pub const ASUS_USB_VENDOR_ID: u16 = 0x0b05;
pub const G615JM_AURA_PRODUCT_ID: u16 = 0x19b6;
pub const G615JM_AURA_USB_INTERFACE: u8 = 0x00;
pub const AURA_REPORT_ID: u8 = 0x5d;
pub const AURA_OUTPUT_PAYLOAD_BYTES: usize = 63;
pub const AURA_OUTPUT_REPORT_BYTES: usize = 64;
pub const G615JM_REPORT_DESCRIPTOR_SHA256: &str =
    "bdcf63294f0793588d96a966c08b1e28062b36b5fdf5d54e714b0102bf1e1094";

const EFFECT_COMMAND: u8 = 0xb3;
const APPLY_COMMAND: u8 = 0xb4;
const SET_COMMAND: u8 = 0xb5;
const MAX_REPORT_DESCRIPTOR_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuraHidProtocolFamily {
    G615jmLaptopAura64,
}

impl AuraHidProtocolFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::G615jmLaptopAura64 => "asus-g615jm-laptop-aura-64",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuraHidIdentity {
    pub vendor_id: u16,
    pub product_id: u16,
    pub interface_number: u8,
    pub driver: Option<String>,
    pub board_name: String,
    pub report_descriptor_sha256: String,
    pub output_report_payload_bytes: BTreeMap<u8, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuraHidMatch {
    pub protocol: Option<AuraHidProtocolFamily>,
    pub rejection_reasons: Vec<String>,
}

impl AuraHidMatch {
    pub fn is_supported(&self) -> bool {
        self.protocol.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredAuraHidDevice {
    pub diagnostics: LightingHidDeviceDiagnostics,
    pub protocol: Option<AuraHidProtocolFamily>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuraHidScan {
    pub board_name: Option<String>,
    pub devices: Vec<DiscoveredAuraHidDevice>,
    pub errors: Vec<String>,
}

impl AuraHidScan {
    /// Copy read-only discovery results into the shared diagnostics model.
    /// Backend ownership/selection remains the daemon's responsibility.
    pub fn populate_diagnostics(&self, diagnostics: &mut LightingDiagnostics) {
        diagnostics.dmi_board_name = self.board_name.clone();
        diagnostics.native_aura_hid_detected = !self.devices.is_empty();
        diagnostics.native_aura_hid_supported =
            self.devices.iter().any(|device| device.protocol.is_some());
        diagnostics.native_aura_hid_devices = self
            .devices
            .iter()
            .map(|device| device.diagnostics.clone())
            .collect();
        diagnostics
            .probe_errors
            .extend(self.errors.iter().map(|error| format!("Aura HID: {error}")));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuraHidReports {
    effect: [u8; AURA_OUTPUT_REPORT_BYTES],
    set: [u8; AURA_OUTPUT_REPORT_BYTES],
    apply: [u8; AURA_OUTPUT_REPORT_BYTES],
}

impl AuraHidReports {
    pub fn effect(&self) -> &[u8; AURA_OUTPUT_REPORT_BYTES] {
        &self.effect
    }

    pub fn set(&self) -> &[u8; AURA_OUTPUT_REPORT_BYTES] {
        &self.set
    }

    pub fn apply(&self) -> &[u8; AURA_OUTPUT_REPORT_BYTES] {
        &self.apply
    }

    /// The only permitted command sequence for a high-level effect request.
    pub fn ordered(&self) -> [&[u8; AURA_OUTPUT_REPORT_BYTES]; 3] {
        [&self.effect, &self.set, &self.apply]
    }
}

/// Exact capabilities verified for the G615JM entry in upstream ASUS Linux data.
///
/// The device has single-target RGB built-in effects. The upstream entry has no
/// basic zones and explicitly disables its advanced/per-key type, so ARGB,
/// zones, and per-key RGB must remain hidden.
pub fn g615jm_lighting_caps() -> LightingCaps {
    LightingCaps {
        backend: LightingBackendKind::NativeAuraHid,
        supports_brightness: false,
        max_brightness: None,
        supports_rgb: true,
        supports_argb: false,
        supports_zones: false,
        supports_per_key: false,
        supported_modes: vec![
            LightingMode::Static,
            LightingMode::Breathe,
            LightingMode::RainbowCycle,
            LightingMode::RainbowWave,
            LightingMode::Pulse,
        ],
        supported_speeds: vec![
            LightingSpeed::Slow,
            LightingSpeed::Medium,
            LightingSpeed::Fast,
        ],
        supported_directions: vec![
            LightingDirection::Right,
            LightingDirection::Left,
            LightingDirection::Up,
            LightingDirection::Down,
        ],
        zones: Vec::new(),
        modes_requiring_primary_rgb: vec![
            LightingMode::Static,
            LightingMode::Breathe,
            LightingMode::Pulse,
        ],
        modes_supporting_secondary_rgb: vec![LightingMode::Breathe],
        modes_supporting_speed: vec![
            LightingMode::Breathe,
            LightingMode::RainbowCycle,
            LightingMode::RainbowWave,
            LightingMode::Pulse,
        ],
        // The upstream protocol documents direction as relevant only to Rainbow.
        // Of the two rainbow modes, the directional animation is RainbowWave.
        modes_supporting_direction: vec![LightingMode::RainbowWave],
        writable: true,
    }
}

/// Match only the verified target; similar-looking devices stay diagnostic-only.
pub fn match_g615jm(identity: &AuraHidIdentity) -> AuraHidMatch {
    let mut rejection_reasons = Vec::new();
    if identity.vendor_id != ASUS_USB_VENDOR_ID {
        rejection_reasons.push(format!(
            "USB vendor {:04x} is not the verified ASUS vendor {:04x}",
            identity.vendor_id, ASUS_USB_VENDOR_ID
        ));
    }
    if identity.product_id != G615JM_AURA_PRODUCT_ID {
        rejection_reasons.push(format!(
            "USB product {:04x} is not the verified product {:04x}",
            identity.product_id, G615JM_AURA_PRODUCT_ID
        ));
    }
    if identity.interface_number != G615JM_AURA_USB_INTERFACE {
        rejection_reasons.push(format!(
            "USB interface {:02x} is not the verified interface {:02x}",
            identity.interface_number, G615JM_AURA_USB_INTERFACE
        ));
    }
    if identity.driver.as_deref() != Some("asus") {
        rejection_reasons.push(format!(
            "HID driver '{}' is not the verified ASUS driver 'asus'",
            identity.driver.as_deref().unwrap_or("unbound")
        ));
    }
    if !is_g615jm_board(&identity.board_name) {
        rejection_reasons.push(format!(
            "DMI board '{}' does not match the verified G615JM family",
            identity.board_name.trim()
        ));
    }
    if !identity
        .report_descriptor_sha256
        .eq_ignore_ascii_case(G615JM_REPORT_DESCRIPTOR_SHA256)
    {
        rejection_reasons.push(format!(
            "HID report descriptor SHA-256 {} is not the verified G615JM descriptor",
            identity.report_descriptor_sha256
        ));
    }
    match identity.output_report_payload_bytes.get(&AURA_REPORT_ID) {
        Some(bytes) if *bytes == AURA_OUTPUT_PAYLOAD_BYTES => {}
        Some(bytes) => rejection_reasons.push(format!(
            "HID output report 0x{AURA_REPORT_ID:02x} has {bytes} payload bytes, expected {AURA_OUTPUT_PAYLOAD_BYTES}"
        )),
        None => rejection_reasons.push(format!(
            "HID descriptor has no output report ID 0x{AURA_REPORT_ID:02x}"
        )),
    }

    AuraHidMatch {
        protocol: rejection_reasons
            .is_empty()
            .then_some(AuraHidProtocolFamily::G615jmLaptopAura64),
        rejection_reasons,
    }
}

pub fn is_g615jm_board(board_name: &str) -> bool {
    board_name.trim().to_ascii_uppercase().starts_with("G615JM")
}

/// Parse HID short items and total the Output bits for every report ID.
///
/// This handles the global Report Size, Report Count, Report ID, Push and Pop
/// items required to establish the kernel write length. It never interprets
/// usages or trusts an Input/Feature item as evidence for an Output report.
pub fn parse_output_report_sizes(descriptor: &[u8]) -> Result<BTreeMap<u8, usize>, String> {
    #[derive(Clone, Copy, Default)]
    struct Globals {
        report_size_bits: u32,
        report_count: u32,
        report_id: u8,
    }

    let mut globals = Globals::default();
    let mut stack = Vec::new();
    let mut output_bits = BTreeMap::<u8, u64>::new();
    let mut offset = 0usize;

    while offset < descriptor.len() {
        let prefix = descriptor[offset];
        offset += 1;
        if prefix == 0xfe {
            if offset + 2 > descriptor.len() {
                return Err("truncated HID long item header".to_string());
            }
            let data_len = descriptor[offset] as usize;
            offset += 2; // length and long-item tag
            if offset + data_len > descriptor.len() {
                return Err("truncated HID long item data".to_string());
            }
            offset += data_len;
            continue;
        }

        let data_len = match prefix & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        if offset + data_len > descriptor.len() {
            return Err("truncated HID short item data".to_string());
        }
        let value = read_unsigned_le(&descriptor[offset..offset + data_len]);
        offset += data_len;

        let item_type = (prefix >> 2) & 0x03;
        let tag = (prefix >> 4) & 0x0f;
        match (item_type, tag) {
            (1, 7) => globals.report_size_bits = value,
            (1, 8) => {
                if value == 0 || value > u8::MAX as u32 {
                    return Err(format!("invalid HID report ID {value}"));
                }
                globals.report_id = value as u8;
            }
            (1, 9) => globals.report_count = value,
            (1, 10) => stack.push(globals),
            (1, 11) => {
                globals = stack
                    .pop()
                    .ok_or_else(|| "HID global Pop without matching Push".to_string())?;
            }
            // Main item tag 9 is Output.
            (0, 9) => {
                let bits = u64::from(globals.report_size_bits)
                    .checked_mul(u64::from(globals.report_count))
                    .ok_or_else(|| "HID output report size overflow".to_string())?;
                let total = output_bits.entry(globals.report_id).or_default();
                *total = total
                    .checked_add(bits)
                    .ok_or_else(|| "HID output report size overflow".to_string())?;
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err("HID global Push without matching Pop".to_string());
    }

    output_bits
        .into_iter()
        .map(|(report_id, bits)| {
            let bytes = bits
                .checked_add(7)
                .ok_or_else(|| "HID output report size overflow".to_string())?
                / 8;
            let bytes = usize::try_from(bytes)
                .map_err(|_| "HID output report is too large for this platform".to_string())?;
            Ok((report_id, bytes))
        })
        .collect()
}

fn read_unsigned_le(bytes: &[u8]) -> u32 {
    bytes.iter().enumerate().fold(0u32, |value, (shift, byte)| {
        value | (u32::from(*byte) << (shift * 8))
    })
}

/// Encode the sole fixed report sequence accepted by the verified protocol.
pub fn encode_g615jm_effect(request: &LightingApplyRequest) -> RogResult<AuraHidReports> {
    request.validate_against(&g615jm_lighting_caps())?;

    let mode = match request.mode {
        LightingMode::Static => 0x00,
        LightingMode::Breathe => 0x01,
        LightingMode::RainbowCycle => 0x02,
        LightingMode::RainbowWave => 0x03,
        LightingMode::Pulse => 0x0a,
        _ => {
            return Err(RogError::NotSupported(format!(
                "lighting mode '{}' has no G615JM Aura packet mapping",
                request.mode.label()
            )))
        }
    };
    let primary = request.primary_rgb.unwrap_or(RgbColor::new(0, 0, 0));
    let secondary = request.secondary_rgb.unwrap_or(RgbColor::new(0, 0, 0));
    let speed = match request.speed.as_ref().unwrap_or(&LightingSpeed::Medium) {
        LightingSpeed::Slow => 0xe1,
        LightingSpeed::Medium => 0xeb,
        LightingSpeed::Fast => 0xf5,
        LightingSpeed::Other(value) => {
            return Err(RogError::NotSupported(format!(
                "lighting speed '{value}' has no G615JM Aura packet mapping"
            )))
        }
    };
    let direction = match request
        .direction
        .as_ref()
        .unwrap_or(&LightingDirection::Right)
    {
        LightingDirection::Right => 0,
        LightingDirection::Left => 1,
        LightingDirection::Up => 2,
        LightingDirection::Down => 3,
        other => {
            return Err(RogError::NotSupported(format!(
                "lighting direction '{}' has no G615JM Aura packet mapping",
                other.label()
            )))
        }
    };

    let mut effect = [0u8; AURA_OUTPUT_REPORT_BYTES];
    effect[0] = AURA_REPORT_ID;
    effect[1] = EFFECT_COMMAND;
    effect[2] = 0; // verified target has no exposed basic zones
    effect[3] = mode;
    effect[4..7].copy_from_slice(&[primary.r, primary.g, primary.b]);
    effect[7] = speed;
    effect[8] = direction;
    effect[10..13].copy_from_slice(&[secondary.r, secondary.g, secondary.b]);

    let mut set = [0u8; AURA_OUTPUT_REPORT_BYTES];
    set[0] = AURA_REPORT_ID;
    set[1] = SET_COMMAND;
    let mut apply = [0u8; AURA_OUTPUT_REPORT_BYTES];
    apply[0] = AURA_REPORT_ID;
    apply[1] = APPLY_COMMAND;

    Ok(AuraHidReports { effect, set, apply })
}

/// Scan the real system using sysfs only. No `/dev/hidraw*` node is opened.
pub fn scan_native_aura_hid() -> AuraHidScan {
    scan_native_aura_hid_at(
        Path::new("/sys/class/hidraw"),
        Path::new("/sys/class/dmi/id/board_name"),
        Path::new("/dev"),
    )
}

fn scan_native_aura_hid_at(hidraw_class: &Path, board_path: &Path, dev_root: &Path) -> AuraHidScan {
    let mut scan = AuraHidScan {
        board_name: read_trimmed(board_path).ok(),
        ..AuraHidScan::default()
    };

    let entries = match fs::read_dir(hidraw_class) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return scan,
        Err(error) => {
            scan.errors.push(format!(
                "failed to enumerate {}: {error}",
                hidraw_class.display()
            ));
            return scan;
        }
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let hidraw_name = entry.file_name().to_string_lossy().to_string();
        if !hidraw_name.starts_with("hidraw") {
            continue;
        }
        match inspect_hidraw(
            &entry.path(),
            &hidraw_name,
            scan.board_name.as_deref().unwrap_or(""),
            dev_root,
        ) {
            Ok(Some(device)) => scan.devices.push(device),
            Ok(None) => {}
            Err(error) => scan.errors.push(format!("{hidraw_name}: {error}")),
        }
    }
    scan
}

fn inspect_hidraw(
    class_entry: &Path,
    hidraw_name: &str,
    board_name: &str,
    dev_root: &Path,
) -> Result<Option<DiscoveredAuraHidDevice>, String> {
    let hid_device = fs::canonicalize(class_entry.join("device"))
        .map_err(|error| format!("failed to resolve sysfs device: {error}"))?;
    let Some(usb_device) = find_ancestor_with(&hid_device, &["idVendor", "idProduct"]) else {
        // Non-USB hidraw devices are unrelated to ASUS Aura discovery and
        // should not become noisy diagnostics errors.
        return Ok(None);
    };
    let vendor_id = read_hex_u16(&usb_device.join("idVendor"))?;
    if vendor_id != ASUS_USB_VENDOR_ID {
        // Do not collect metadata about unrelated HID devices.
        return Ok(None);
    }
    let product_id = read_hex_u16(&usb_device.join("idProduct"))?;
    let interface = find_ancestor_with(&hid_device, &["bInterfaceNumber"])
        .and_then(|path| read_hex_u8(&path.join("bInterfaceNumber")).ok());

    let descriptor = fs::read(hid_device.join("report_descriptor"))
        .map_err(|error| format!("failed to read HID report descriptor: {error}"))?;
    if descriptor.len() > MAX_REPORT_DESCRIPTOR_BYTES {
        return Err(format!(
            "HID report descriptor is unexpectedly large ({} bytes)",
            descriptor.len()
        ));
    }
    let descriptor_hash = sha256_hex(&descriptor);
    let output_reports = parse_output_report_sizes(&descriptor)
        .map_err(|error| format!("invalid HID report descriptor: {error}"))?;

    let driver = find_driver_name(&hid_device);
    let identity = AuraHidIdentity {
        vendor_id,
        product_id,
        interface_number: interface.unwrap_or(u8::MAX),
        driver: driver.clone(),
        board_name: board_name.to_string(),
        report_descriptor_sha256: descriptor_hash.clone(),
        output_report_payload_bytes: output_reports.clone(),
    };
    let matched = match_g615jm(&identity);
    let output_payload = output_reports.get(&AURA_REPORT_ID).copied();
    let devnode_mode = fs::metadata(dev_root.join(hidraw_name))
        .ok()
        .map(|metadata| metadata.permissions().mode() & 0o7777);
    let protocol_family = matched
        .protocol
        .map(|protocol| protocol.as_str().to_string());
    let diagnostics = LightingHidDeviceDiagnostics {
        hidraw_name: hidraw_name.to_string(),
        vendor_id: Some(format!("{vendor_id:04x}")),
        product_id: Some(format!("{product_id:04x}")),
        interface_number: interface.map(|value| format!("{value:02x}")),
        manufacturer: read_trimmed(&usb_device.join("manufacturer"))
            .ok()
            .and_then(sanitize_metadata),
        product_name: read_trimmed(&usb_device.join("product"))
            .ok()
            .and_then(sanitize_metadata),
        driver,
        report_descriptor_sha256: Some(descriptor_hash),
        output_report_payload_bytes: output_payload.map(|bytes| bytes as u32),
        output_report_total_bytes: output_payload.map(|bytes| (bytes + 1) as u32),
        device_node_mode: devnode_mode,
        physical_path_sha256: Some(sha256_hex(hid_device.to_string_lossy().as_bytes())),
        protocol_family,
        supported: matched.is_supported(),
        rejection_reasons: matched.rejection_reasons,
    };
    Ok(Some(DiscoveredAuraHidDevice {
        diagnostics,
        protocol: matched.protocol,
    }))
}

fn find_ancestor_with(path: &Path, attributes: &[&str]) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| {
            attributes
                .iter()
                .all(|attribute| ancestor.join(attribute).is_file())
        })
        .map(Path::to_path_buf)
}

fn find_driver_name(path: &Path) -> Option<String> {
    path.ancestors().find_map(|ancestor| {
        fs::read_link(ancestor.join("driver"))
            .ok()
            .and_then(|driver| {
                driver
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
    })
}

fn read_trimmed(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .map(|value| value.trim().to_string())
        .map_err(|error| format!("failed to read {}: {error}", path.display()))
}

fn read_hex_u16(path: &Path) -> Result<u16, String> {
    let value = read_trimmed(path)?;
    u16::from_str_radix(value.trim_start_matches("0x"), 16)
        .map_err(|error| format!("invalid hexadecimal value in {}: {error}", path.display()))
}

fn read_hex_u8(path: &Path) -> Result<u8, String> {
    let value = read_trimmed(path)?;
    u8::from_str_radix(value.trim_start_matches("0x"), 16)
        .map_err(|error| format!("invalid hexadecimal value in {}: {error}", path.display()))
}

fn sanitize_metadata(value: String) -> Option<String> {
    let sanitized = value
        .chars()
        .filter(|character| !character.is_control())
        .take(128)
        .collect::<String>()
        .trim()
        .to_string();
    (!sanitized.is_empty()).then_some(sanitized)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rog_core::LightingZone;
    use std::os::unix::fs::symlink;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn valid_identity() -> AuraHidIdentity {
        AuraHidIdentity {
            vendor_id: ASUS_USB_VENDOR_ID,
            product_id: G615JM_AURA_PRODUCT_ID,
            interface_number: G615JM_AURA_USB_INTERFACE,
            driver: Some("asus".to_string()),
            board_name: "G615JM".to_string(),
            report_descriptor_sha256: G615JM_REPORT_DESCRIPTOR_SHA256.to_string(),
            output_report_payload_bytes: BTreeMap::from([(
                AURA_REPORT_ID,
                AURA_OUTPUT_PAYLOAD_BYTES,
            )]),
        }
    }

    fn request(mode: LightingMode) -> LightingApplyRequest {
        LightingApplyRequest {
            mode,
            primary_rgb: Some(RgbColor::new(0xff, 0x11, 0xdd)),
            secondary_rgb: None,
            brightness: None,
            speed: None,
            direction: None,
            zone: None,
            apply_to_all: false,
        }
    }

    #[test]
    fn descriptor_parser_finds_exact_63_byte_output_payload() {
        let descriptor = [0x85, 0x5d, 0x75, 0x08, 0x95, 0x3f, 0x91, 0x02];
        let reports = parse_output_report_sizes(&descriptor).unwrap();
        assert_eq!(reports.get(&AURA_REPORT_ID), Some(&63));
    }

    #[test]
    fn descriptor_parser_aggregates_multiple_output_items_only() {
        let descriptor = [
            0x85, 0x5d, 0x75, 0x08, 0x95, 0x1f, 0x91, 0x02, // 31 output bytes
            0x95, 0x20, 0x91, 0x02, // 32 more output bytes
            0x95, 0x40, 0x81, 0x02, // input must not count
        ];
        let reports = parse_output_report_sizes(&descriptor).unwrap();
        assert_eq!(reports.get(&AURA_REPORT_ID), Some(&63));
    }

    #[test]
    fn descriptor_parser_rejects_truncated_and_unbalanced_items() {
        assert!(parse_output_report_sizes(&[0x85]).is_err());
        assert!(parse_output_report_sizes(&[0xa4]).is_err());
        assert!(parse_output_report_sizes(&[0xb4]).is_err());
    }

    #[test]
    fn matcher_accepts_only_exact_verified_identity() {
        let matched = match_g615jm(&valid_identity());
        assert_eq!(
            matched.protocol,
            Some(AuraHidProtocolFamily::G615jmLaptopAura64)
        );
        assert!(matched.rejection_reasons.is_empty());
        assert!(is_g615jm_board("G615JM.314"));
    }

    #[test]
    fn matcher_rejects_any_identity_field_outside_verified_contract() {
        let mutations = [
            {
                let mut value = valid_identity();
                value.vendor_id = 0x1234;
                value
            },
            {
                let mut value = valid_identity();
                value.product_id = 0x19b7;
                value
            },
            {
                let mut value = valid_identity();
                value.interface_number = 1;
                value
            },
            {
                let mut value = valid_identity();
                value.driver = Some("hid-generic".to_string());
                value
            },
            {
                let mut value = valid_identity();
                value.board_name = "G614JM".to_string();
                value
            },
            {
                let mut value = valid_identity();
                value.output_report_payload_bytes.insert(AURA_REPORT_ID, 62);
                value
            },
            {
                let mut value = valid_identity();
                value.output_report_payload_bytes.clear();
                value
            },
            {
                let mut value = valid_identity();
                value.report_descriptor_sha256 = "00".repeat(32);
                value
            },
        ];
        for identity in mutations {
            let matched = match_g615jm(&identity);
            assert!(!matched.is_supported());
            assert!(!matched.rejection_reasons.is_empty());
        }
    }

    #[test]
    fn capabilities_do_not_claim_argb_zones_or_per_key() {
        let caps = g615jm_lighting_caps();
        assert!(caps.supports_rgb);
        assert!(!caps.supports_argb);
        assert!(!caps.supports_zones);
        assert!(!caps.supports_per_key);
        assert!(caps.zones.is_empty());
        assert_eq!(caps.supported_modes.len(), 5);
    }

    #[test]
    fn static_packet_sequence_matches_known_protocol_bytes() {
        let reports = encode_g615jm_effect(&request(LightingMode::Static)).unwrap();
        assert_eq!(
            &reports.effect()[..17],
            &[
                0x5d, 0xb3, 0x00, 0x00, 0xff, 0x11, 0xdd, 0xeb, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ]
        );
        assert!(reports.effect()[17..].iter().all(|byte| *byte == 0));
        assert_eq!(&reports.set()[..2], &[0x5d, 0xb5]);
        assert!(reports.set()[2..].iter().all(|byte| *byte == 0));
        assert_eq!(&reports.apply()[..2], &[0x5d, 0xb4]);
        assert!(reports.apply()[2..].iter().all(|byte| *byte == 0));
        assert_eq!(reports.ordered().len(), 3);
    }

    #[test]
    fn breathe_packet_encodes_two_colours_and_speed() {
        let mut effect = request(LightingMode::Breathe);
        effect.secondary_rgb = Some(RgbColor::new(0x10, 0x20, 0x30));
        effect.speed = Some(LightingSpeed::Fast);
        let reports = encode_g615jm_effect(&effect).unwrap();
        assert_eq!(reports.effect()[3], 0x01);
        assert_eq!(&reports.effect()[4..7], &[0xff, 0x11, 0xdd]);
        assert_eq!(reports.effect()[7], 0xf5);
        assert_eq!(&reports.effect()[10..13], &[0x10, 0x20, 0x30]);
    }

    #[test]
    fn every_verified_mode_has_the_expected_protocol_id() {
        let cases = [
            (LightingMode::Static, 0x00, true, [0xff, 0x11, 0xdd]),
            (LightingMode::Breathe, 0x01, true, [0xff, 0x11, 0xdd]),
            (LightingMode::RainbowCycle, 0x02, false, [0, 0, 0]),
            (LightingMode::RainbowWave, 0x03, false, [0, 0, 0]),
            (LightingMode::Pulse, 0x0a, true, [0xff, 0x11, 0xdd]),
        ];

        for (mode, expected_id, needs_primary, expected_primary) in cases {
            let mut effect = request(mode.clone());
            if !needs_primary {
                effect.primary_rgb = None;
            }
            if matches!(
                mode,
                LightingMode::Breathe
                    | LightingMode::RainbowCycle
                    | LightingMode::RainbowWave
                    | LightingMode::Pulse
            ) {
                effect.speed = Some(LightingSpeed::Medium);
            }
            let reports = encode_g615jm_effect(&effect)
                .unwrap_or_else(|error| panic!("{} should encode: {error}", mode.label()));
            assert_eq!(reports.effect()[3], expected_id, "{}", mode.label());
            assert_eq!(
                &reports.effect()[4..7],
                &expected_primary,
                "{}",
                mode.label()
            );
            assert_eq!(reports.effect()[7], 0xeb, "{}", mode.label());
            assert_eq!(reports.effect()[8], 0x00, "{}", mode.label());
            assert_eq!(&reports.effect()[10..13], &[0, 0, 0], "{}", mode.label());
            assert_eq!(reports.effect().len(), AURA_OUTPUT_REPORT_BYTES);
            assert_eq!(reports.set()[..2], [AURA_REPORT_ID, SET_COMMAND]);
            assert_eq!(reports.apply()[..2], [AURA_REPORT_ID, APPLY_COMMAND]);
        }
    }

    #[test]
    fn every_verified_speed_maps_to_the_expected_protocol_byte() {
        for (speed, expected) in [
            (LightingSpeed::Slow, 0xe1),
            (LightingSpeed::Medium, 0xeb),
            (LightingSpeed::Fast, 0xf5),
        ] {
            let mut effect = request(LightingMode::Pulse);
            effect.speed = Some(speed.clone());
            let reports = encode_g615jm_effect(&effect)
                .unwrap_or_else(|error| panic!("{} should encode: {error}", speed.label()));
            assert_eq!(reports.effect()[7], expected, "{}", speed.label());
        }
    }

    #[test]
    fn every_verified_wave_direction_maps_to_the_expected_protocol_byte() {
        for (direction, expected) in [
            (LightingDirection::Right, 0x00),
            (LightingDirection::Left, 0x01),
            (LightingDirection::Up, 0x02),
            (LightingDirection::Down, 0x03),
        ] {
            let mut effect = request(LightingMode::RainbowWave);
            effect.primary_rgb = None;
            effect.direction = Some(direction.clone());
            let reports = encode_g615jm_effect(&effect)
                .unwrap_or_else(|error| panic!("{} should encode: {error}", direction.label()));
            assert_eq!(reports.effect()[8], expected, "{}", direction.label());
        }
    }

    #[test]
    fn rainbow_wave_packet_encodes_verified_direction() {
        let mut effect = request(LightingMode::RainbowWave);
        effect.primary_rgb = None;
        effect.speed = Some(LightingSpeed::Slow);
        effect.direction = Some(LightingDirection::Left);
        let reports = encode_g615jm_effect(&effect).unwrap();
        assert_eq!(&reports.effect()[3..9], &[0x03, 0, 0, 0, 0xe1, 0x01]);
    }

    #[test]
    fn encoder_rejects_invalid_mode_zone_speed_and_direction() {
        let mut effect = request(LightingMode::Strobe);
        assert!(encode_g615jm_effect(&effect).is_err());

        effect = request(LightingMode::Static);
        effect.zone = Some(LightingZone::Keyboard);
        assert!(encode_g615jm_effect(&effect).is_err());

        effect = request(LightingMode::Breathe);
        effect.speed = Some(LightingSpeed::Other("turbo".to_string()));
        assert!(encode_g615jm_effect(&effect).is_err());

        effect = request(LightingMode::Static);
        effect.direction = Some(LightingDirection::Left);
        assert!(encode_g615jm_effect(&effect).is_err());

        effect = request(LightingMode::Static);
        effect.brightness = Some(3);
        assert!(encode_g615jm_effect(&effect).is_err());
    }

    #[test]
    fn sysfs_scan_resolves_usb_parent_without_opening_hidraw() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rog-helper-aura-hid-{}-{nonce}",
            std::process::id()
        ));
        let class_entry = root.join("sys/class/hidraw/hidraw7");
        let usb = root.join("sys/devices/usb1/1-2");
        let interface = usb.join("1-2:1.0");
        let hid = interface.join("0003:0B05:19B6.0001");
        let dev = root.join("dev");
        let dmi = root.join("sys/class/dmi/id");
        fs::create_dir_all(&class_entry).unwrap();
        fs::create_dir_all(&hid).unwrap();
        fs::create_dir_all(&dev).unwrap();
        fs::create_dir_all(&dmi).unwrap();
        fs::write(usb.join("idVendor"), "0b05\n").unwrap();
        fs::write(usb.join("idProduct"), "19b6\n").unwrap();
        fs::write(usb.join("manufacturer"), "ASUSTeK Computer Inc.\n").unwrap();
        fs::write(usb.join("product"), "N-KEY Device\n").unwrap();
        fs::write(interface.join("bInterfaceNumber"), "00\n").unwrap();
        fs::write(
            hid.join("report_descriptor"),
            [0x85, 0x5d, 0x75, 0x08, 0x95, 0x3f, 0x91, 0x02],
        )
        .unwrap();
        fs::write(dmi.join("board_name"), "G615JM.314\n").unwrap();
        fs::write(dev.join("hidraw7"), []).unwrap();
        symlink(&hid, class_entry.join("device")).unwrap();

        let scan = scan_native_aura_hid_at(
            &root.join("sys/class/hidraw"),
            &dmi.join("board_name"),
            &dev,
        );
        assert!(scan.errors.is_empty(), "{:?}", scan.errors);
        assert_eq!(scan.devices.len(), 1);
        let device = &scan.devices[0];
        // This compact descriptor proves report sizing but intentionally is not
        // the target's allow-listed descriptor, so discovery remains read-only.
        assert_eq!(device.protocol, None);
        assert_eq!(device.diagnostics.interface_number.as_deref(), Some("00"));
        assert_eq!(
            device.diagnostics.output_report_total_bytes,
            Some(AURA_OUTPUT_REPORT_BYTES as u32)
        );
        assert_eq!(
            device
                .diagnostics
                .report_descriptor_sha256
                .as_deref()
                .map(str::len),
            Some(64)
        );
        assert!(!device.diagnostics.supported);
        assert!(device
            .diagnostics
            .rejection_reasons
            .iter()
            .any(|reason| reason.contains("SHA-256")));

        let mut diagnostics = LightingDiagnostics::unknown();
        scan.populate_diagnostics(&mut diagnostics);
        assert!(diagnostics.native_aura_hid_detected);
        assert!(!diagnostics.native_aura_hid_supported);
        assert_eq!(diagnostics.native_aura_hid_devices.len(), 1);

        fs::remove_dir_all(&root).unwrap();
    }
}
