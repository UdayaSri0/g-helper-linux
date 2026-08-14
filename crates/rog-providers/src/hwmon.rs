use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rog_core::{
    validate_fan_percent, validate_fan_request, FanCaps, FanControlMode, FanControlRequest,
    FanCurve, FanDomain, FanInfo, FanMappingConfidence, FanState, FanTelemetry, RogError,
    RogResult, TelemetrySnapshot,
};
use tracing::debug;

#[derive(Debug, Clone)]
pub struct HwmonTelemetryProvider {
    root: PathBuf,
    write_lock: Arc<Mutex<()>>,
}

impl Default for HwmonTelemetryProvider {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/sys/class/hwmon"),
            write_lock: Arc::new(Mutex::new(())),
        }
    }
}

impl HwmonTelemetryProvider {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn list_hwmon(&self) -> RogResult<Vec<(String, PathBuf)>> {
        let mut out = Vec::new();
        let entries = fs::read_dir(&self.root)
            .map_err(|e| RogError::DependencyMissing(format!("hwmon unavailable: {e}")))?;
        for ent in entries.flatten() {
            let path = ent.path();
            if !path.is_dir() {
                continue;
            }
            let name_path = path.join("name");
            let name = fs::read_to_string(&name_path)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "unknown".to_string());
            out.push((name, path));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        Ok(out)
    }

    pub fn read_snapshot(&self) -> RogResult<TelemetrySnapshot> {
        let ts = now_ms();
        let mut snapshot = TelemetrySnapshot::empty_now(ts);

        let mut temps: BTreeMap<String, f32> = BTreeMap::new();
        let mut fans: BTreeMap<String, u32> = BTreeMap::new();
        let mut fan_rows = Vec::new();

        let hwmons = self.list_hwmon()?;
        debug!("found {} hwmon devices", hwmons.len());

        let mut cpu_candidates: Vec<f32> = Vec::new();
        let mut gpu_candidates: Vec<f32> = Vec::new();

        for (hwname, hwpath) in hwmons {
            // Temps
            for (key, val) in read_temp_inputs(&hwname, &hwpath) {
                if is_cpu_hwmon(&hwname) {
                    cpu_candidates.push(val);
                }
                if is_gpu_hwmon(&hwname) {
                    gpu_candidates.push(val);
                }
                temps.insert(key, val);
            }

            // Fans
            fan_rows.extend(read_fan_inputs(&hwname, &hwpath));
        }

        finalize_fan_display_labels(&mut fan_rows);
        for fan in &fan_rows {
            if let Some(rpm) = fan.rpm {
                fans.insert(fan.display_label.clone(), rpm);
            }
        }

        snapshot.cpu_temp_c = cpu_candidates.into_iter().max_by(|a, b| a.total_cmp(b));
        snapshot.gpu_temp_c = gpu_candidates.into_iter().max_by(|a, b| a.total_cmp(b));

        snapshot.temps_c = temps;
        snapshot.fans_rpm = fans;
        snapshot.fan_rows = fan_rows;
        Ok(snapshot)
    }

    pub fn list_fans(&self) -> RogResult<Vec<FanInfo>> {
        let mut out = Vec::new();
        let hwmons = self.list_hwmon()?;
        for (hwname, hwpath) in &hwmons {
            out.extend(read_fan_infos(hwname, hwpath));
        }
        enrich_verified_asus_curve_fans(&mut out, &hwmons);
        finalize_fan_info_labels(&mut out);
        Ok(out)
    }

    pub fn fan_caps(&self) -> RogResult<FanCaps> {
        let fans = self.list_fans()?;
        Ok(self.fan_caps_from_fans(&fans))
    }

    pub fn fan_state(&self) -> RogResult<FanState> {
        let fans = self.list_fans()?;
        let mut state = FanState::from_fans(fans);
        state.caps = self.fan_caps_from_fans(&state.fans);
        Ok(state)
    }

    fn fan_caps_from_fans(&self, fans: &[FanInfo]) -> FanCaps {
        let mut caps = FanCaps::from_fans(fans);
        let Ok(hwmons) = self.list_hwmon() else {
            return caps;
        };

        for (name, path) in hwmons {
            if name != "asus_custom_fan_curve" {
                continue;
            }
            let discovery = discover_asus_curve_points(&path);
            caps.endpoints.extend(discovery.endpoints);
            caps.notes.extend(discovery.notes);
            caps.warnings.extend(discovery.warnings);
            caps.fan_curve_readable |= discovery.readable;
        }
        caps.endpoints.sort();
        caps.endpoints.dedup();
        caps.notes.sort();
        caps.notes.dedup();
        caps.warnings.sort();
        caps.warnings.dedup();

        // A readable curve ABI is not write authorisation. Generic hwmon curve
        // writes remain disabled until a backend-specific implementation can
        // validate the device, ranges, restore path, and daemon permissions.
        caps
    }

    pub fn set_fan_auto(&self, fan_id: Option<&str>) -> RogResult<()> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| RogError::Unexpected("fan write lock is unavailable".to_string()))?;
        let channels = self.verified_asus_curve_channels()?;
        let selected = select_verified_channels(&channels, fan_id)?;
        for channel in selected {
            reset_asus_channel_to_auto(channel)?;
        }
        Ok(())
    }

    pub fn set_fan_manual_percent(&self, fan_id: &str, percent: u8) -> RogResult<()> {
        validate_fan_percent(percent)?;
        let fans = self.list_fans()?;
        let _ = select_fans(&fans, Some(fan_id))?;
        Err(RogError::NotSupported(
            "generic hwmon manual-percent writes are disabled until the backend is verified"
                .to_string(),
        ))
    }

    pub fn set_fan_rpm_target(&self, fan_id: &str, rpm: u32) -> RogResult<()> {
        if rpm == 0 {
            return Err(RogError::InvalidInput(
                "fan RPM target must be greater than zero".to_string(),
            ));
        }
        let fans = self.list_fans()?;
        let _ = select_fans(&fans, Some(fan_id))?;
        Err(RogError::NotSupported(
            "generic hwmon RPM-target writes are disabled until the backend is verified"
                .to_string(),
        ))
    }

    pub fn set_fan_curve(&self, fan_id: &str, curve: FanCurve) -> RogResult<()> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| RogError::Unexpected("fan write lock is unavailable".to_string()))?;
        let policy = rog_core::FanCurvePolicy {
            exact_point_count: Some(ASUS_WMI_CURVE_POINTS),
            ..Default::default()
        };
        curve.validate_safe(policy)?;
        let channels = self.verified_asus_curve_channels()?;
        let channel = select_verified_channels(&channels, Some(fan_id))?
            .into_iter()
            .next()
            .ok_or_else(|| RogError::NotSupported("no verified fan curve channel".to_string()))?;
        let domain_matches = matches!(
            (&curve.domain, fan_id),
            (FanDomain::Cpu, "asus-wmi:cpu")
                | (FanDomain::Gpu, "asus-wmi:gpu")
                | (FanDomain::Mid, "asus-wmi:mid")
        ) || matches!(&curve.domain, FanDomain::Other(value) if value == fan_id);
        if !domain_matches {
            return Err(RogError::InvalidInput(
                "fan curve domain does not match the selected fan id".to_string(),
            ));
        }
        write_asus_curve_transaction(channel, &curve)
    }

    pub fn apply_fan_request(&self, request: FanControlRequest) -> RogResult<()> {
        let fans = self.list_fans()?;
        validate_fan_request(&request, &fans)?;
        match request.mode {
            FanControlMode::Auto => self.set_fan_auto(if request.fan_id.trim().is_empty() {
                None
            } else {
                Some(request.fan_id.as_str())
            }),
            FanControlMode::ManualPercent | FanControlMode::FullSpeedBoost => {
                self.set_fan_manual_percent(&request.fan_id, request.speed_percent.unwrap_or(100))
            }
            FanControlMode::ManualRpmTarget => {
                self.set_fan_rpm_target(&request.fan_id, request.rpm_target.unwrap_or_default())
            }
            FanControlMode::Curve => self.set_fan_curve(
                &request.fan_id,
                request
                    .curve
                    .ok_or_else(|| RogError::InvalidInput("missing fan curve".to_string()))?,
            ),
            FanControlMode::Unsupported | FanControlMode::ReadOnly => Err(RogError::NotSupported(
                "selected fan mode is not writable".to_string(),
            )),
        }
    }

    fn verified_asus_curve_channels(&self) -> RogResult<Vec<AsusCurveChannel>> {
        discover_verified_asus_curve_channels(&self.list_hwmon()?)
    }
}

impl crate::traits::FanProvider for HwmonTelemetryProvider {
    async fn get_fan_rpm(&self) -> RogResult<Vec<(FanDomain, u32)>> {
        let fans = self.list_fans()?;
        Ok(fans
            .into_iter()
            .filter_map(|fan| {
                fan.current_rpm
                    .map(|rpm| (FanDomain::Other(fan.label), rpm))
            })
            .collect())
    }

    async fn supports_curves(&self) -> RogResult<bool> {
        Ok(self.fan_caps()?.has_fan_curves)
    }

    async fn get_curve(&self, _domain: FanDomain) -> RogResult<FanCurve> {
        Err(RogError::NotSupported(
            "generic hwmon fan curve reading is not supported".to_string(),
        ))
    }

    async fn set_curve(&self, domain: FanDomain, curve: FanCurve) -> RogResult<()> {
        let fan_id = match domain {
            FanDomain::Other(value) => value,
            FanDomain::Cpu => "cpu".to_string(),
            FanDomain::Gpu => "gpu".to_string(),
            FanDomain::Mid => "mid".to_string(),
        };
        self.set_fan_curve(&fan_id, curve)
    }

    async fn fan_caps(&self) -> RogResult<FanCaps> {
        HwmonTelemetryProvider::fan_caps(self)
    }

    async fn list_fans(&self) -> RogResult<Vec<FanInfo>> {
        HwmonTelemetryProvider::list_fans(self)
    }

    async fn get_fan_state(&self) -> RogResult<FanState> {
        self.fan_state()
    }

    async fn set_fan_auto(&self, fan_id: Option<&str>) -> RogResult<()> {
        HwmonTelemetryProvider::set_fan_auto(self, fan_id)
    }

    async fn set_fan_manual_percent(&self, fan_id: &str, percent: u8) -> RogResult<()> {
        HwmonTelemetryProvider::set_fan_manual_percent(self, fan_id, percent)
    }

    async fn set_fan_rpm_target(&self, fan_id: &str, rpm: u32) -> RogResult<()> {
        HwmonTelemetryProvider::set_fan_rpm_target(self, fan_id, rpm)
    }

    async fn set_fan_control(&self, request: FanControlRequest) -> RogResult<()> {
        self.apply_fan_request(request)
    }

    async fn restore_fan_defaults(&self) -> RogResult<()> {
        self.set_fan_auto(None)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn is_cpu_hwmon(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("coretemp") || n.contains("k10temp") || n.contains("zenpower") || n.contains("cpu")
}

fn is_gpu_hwmon(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("amdgpu") || n.contains("nouveau") || n.contains("nvidia") || n.contains("gpu")
}

fn read_temp_inputs(hwname: &str, hwpath: &Path) -> Vec<(String, f32)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(hwpath) else {
        return out;
    };
    for ent in entries.flatten() {
        let path = ent.path();
        let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !fname.starts_with("temp") || !fname.ends_with("_input") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(millic) = raw.trim().parse::<i64>() else {
            continue;
        };
        let c = millic as f32 / 1000.0;

        let label = temp_label(hwpath, fname)
            .or_else(|| Some(fname.replace("_input", "")))
            .unwrap_or_else(|| "temp".to_string());

        let key = format!("{hwname}:{label}");
        out.push((key, c));
    }
    out
}

fn read_fan_inputs(hwname: &str, hwpath: &Path) -> Vec<FanTelemetry> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(hwpath) else {
        return out;
    };
    for ent in entries.flatten() {
        let path = ent.path();
        let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !fname.starts_with("fan") || !fname.ends_with("_input") {
            continue;
        }
        let raw_label = fan_label(hwpath, fname);
        let rpm = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| raw.trim().parse::<u32>().ok());

        out.push(FanTelemetry {
            hwmon_device: hwname.to_string(),
            hwmon_path: hwpath.display().to_string(),
            input_path: path.display().to_string(),
            raw_label: raw_label.clone(),
            display_label: raw_label
                .as_deref()
                .map(friendly_fan_label)
                .unwrap_or_default(),
            rpm,
        });
    }
    out.sort_by(|a, b| {
        a.hwmon_device
            .to_ascii_lowercase()
            .cmp(&b.hwmon_device.to_ascii_lowercase())
            .then_with(|| a.hwmon_path.cmp(&b.hwmon_path))
            .then_with(|| a.input_path.cmp(&b.input_path))
    });
    out
}

fn read_fan_infos(_hwname: &str, hwpath: &Path) -> Vec<FanInfo> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(hwpath) else {
        return out;
    };
    for ent in entries.flatten() {
        let path = ent.path();
        let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !fname.starts_with("fan") || !fname.ends_with("_input") {
            continue;
        }
        let Some(index) = fan_index_from_input_name(fname) else {
            continue;
        };

        let raw_label = fan_label(hwpath, fname);
        let rpm = read_u32(&path);
        let min_rpm = read_u32(&hwpath.join(format!("fan{index}_min")));
        let max_rpm = read_u32(&hwpath.join(format!("fan{index}_max")));
        let fan_target = hwpath.join(format!("fan{index}_target"));
        let pwm = hwpath.join(format!("pwm{index}"));
        let pwm_enable = hwpath.join(format!("pwm{index}_enable"));
        let pwm_mode = hwpath.join(format!("pwm{index}_mode"));

        let mut endpoints = vec![format!("fan_input:{}", path.display())];
        if path_exists(&fan_target) {
            endpoints.push(format!("fan_target:{}", fan_target.display()));
        }
        if path_exists(&pwm) {
            endpoints.push(format!("pwm:{}", pwm.display()));
        }
        if path_exists(&pwm_enable) {
            endpoints.push(format!("pwm_enable:{}", pwm_enable.display()));
        }
        if path_exists(&pwm_mode) {
            endpoints.push(format!("pwm_mode:{}", pwm_mode.display()));
        }
        endpoints.extend(auto_point_endpoints(hwpath, index));

        // Presence or file permissions do not establish safe control semantics.
        // Keep generic hwmon endpoints diagnostic-only until an allow-listed,
        // backend-specific implementation verifies the hardware contract.
        let supports_manual_percent = false;
        let supports_manual_rpm_target = false;
        let supports_auto = false;
        let current_percent = read_u32(&pwm).map(pwm_to_percent);
        let supports_curve = false;

        let mut notes = Vec::new();
        let mut warnings = Vec::new();
        if path_exists(&pwm) || path_exists(&pwm_enable) {
            notes.push(
                "PWM candidate detected; control is withheld until backend semantics and safe restore behavior are verified."
                    .to_string(),
            );
        }
        if path_exists(&fan_target) {
            notes.push(
                "RPM-target candidate detected; control is withheld until the backend is verified."
                    .to_string(),
            );
        }
        if !supports_manual_percent && !supports_manual_rpm_target {
            notes.push(
                "Telemetry only; no writable fan-control endpoint was confirmed.".to_string(),
            );
        }
        if raw_label.is_none() {
            warnings.push(
                "Fan mapping is unlabeled by hwmon; CPU/GPU assignment is uncertain.".to_string(),
            );
        }

        out.push(FanInfo {
            id: format!("{}:fan{index}", stable_hwmon_id(hwpath)),
            index,
            label: raw_label
                .as_deref()
                .map(friendly_fan_label)
                .unwrap_or_default(),
            mapping_confidence: if raw_label.is_some() {
                FanMappingConfidence::HardwareLabel
            } else {
                FanMappingConfidence::Unknown
            },
            current_rpm: rpm,
            min_rpm,
            max_rpm,
            current_percent,
            rpm_readable: rpm.is_some(),
            pwm_endpoint_verified: false,
            direct_write: false,
            privileged_write: false,
            authorization: "not_required".to_string(),
            access_state: if rpm.is_some() {
                "telemetry_only".to_string()
            } else {
                "unsupported".to_string()
            },
            controllable: supports_manual_percent || supports_manual_rpm_target || supports_curve,
            supports_manual_percent,
            supports_manual_rpm_target,
            supports_curve,
            supports_auto,
            backend: if supports_manual_percent || supports_manual_rpm_target {
                "hwmon".to_string()
            } else {
                "hwmon-read-only".to_string()
            },
            endpoints,
            notes,
            warnings,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

pub const ASUS_WMI_CURVE_POINTS: usize = 8;

#[derive(Debug, Clone)]
struct AsusCurveChannel {
    fan_id: String,
    channel: u32,
    rpm_path: PathBuf,
    curve_path: PathBuf,
    direct_write: bool,
}

fn enrich_verified_asus_curve_fans(fans: &mut [FanInfo], hwmons: &[(String, PathBuf)]) {
    let Ok(channels) = discover_verified_asus_curve_channels(hwmons) else {
        return;
    };
    for channel in channels {
        let original_id = format!(
            "{}:fan{}",
            stable_hwmon_id(&channel.rpm_path),
            channel.channel
        );
        let Some(fan) = fans.iter_mut().find(|fan| {
            fan.id == original_id && fan.mapping_confidence == FanMappingConfidence::HardwareLabel
        }) else {
            continue;
        };
        fan.id = channel.fan_id;
        fan.controllable = channel.direct_write;
        fan.supports_auto = true;
        fan.supports_curve = true;
        fan.backend = "asus-wmi-curve".to_string();
        fan.pwm_endpoint_verified = true;
        fan.direct_write = channel.direct_write;
        fan.access_state = if channel.direct_write {
            "direct".to_string()
        } else {
            "authorization_required".to_string()
        };
        fan.authorization = if channel.direct_write {
            "not_required".to_string()
        } else {
            "not_checked".to_string()
        };
        fan.notes.push(
            "ASUS WMI curve channel verified by device identity, hardware label, exact point layout, and Auto reset ABI."
                .to_string(),
        );
    }
}

fn discover_verified_asus_curve_channels(
    hwmons: &[(String, PathBuf)],
) -> RogResult<Vec<AsusCurveChannel>> {
    let rpm_devices = hwmons
        .iter()
        .filter(|(name, _)| name == "asus")
        .collect::<Vec<_>>();
    let curve_devices = hwmons
        .iter()
        .filter(|(name, _)| name == "asus_custom_fan_curve")
        .collect::<Vec<_>>();
    let mut channels = Vec::new();
    for (_, rpm_path) in rpm_devices {
        let Some(rpm_identity) = hwmon_device_identity(rpm_path) else {
            continue;
        };
        if rpm_identity.file_name().and_then(|name| name.to_str()) != Some("asus-nb-wmi") {
            continue;
        }
        for (_, curve_path) in &curve_devices {
            if hwmon_device_identity(curve_path).as_ref() != Some(&rpm_identity) {
                continue;
            }
            for (channel, raw_label, suffix) in [
                (1, "cpu_fan", "cpu"),
                (2, "gpu_fan", "gpu"),
                (3, "mid_fan", "mid"),
            ] {
                if fs::read_to_string(rpm_path.join(format!("fan{channel}_label")))
                    .ok()
                    .map(|value| value.trim().to_string())
                    .as_deref()
                    != Some(raw_label)
                {
                    continue;
                }
                if read_u32(&rpm_path.join(format!("fan{channel}_input"))).is_none()
                    || !verified_asus_curve_layout(curve_path, channel)
                {
                    continue;
                }
                let direct_write = asus_channel_paths(curve_path, channel)
                    .iter()
                    .all(|path| OpenOptions::new().write(true).open(path).is_ok());
                channels.push(AsusCurveChannel {
                    fan_id: format!("asus-wmi:{suffix}"),
                    channel,
                    rpm_path: (*rpm_path).clone(),
                    curve_path: (*curve_path).clone(),
                    direct_write,
                });
            }
        }
    }
    if channels.is_empty() {
        return Err(RogError::NotSupported(
            "no safely mapped ASUS WMI fan-curve channels were found".to_string(),
        ));
    }
    Ok(channels)
}

fn hwmon_device_identity(hwpath: &Path) -> Option<PathBuf> {
    fs::canonicalize(hwpath.join("device")).ok()
}

fn verified_asus_curve_layout(path: &Path, channel: u32) -> bool {
    let enable = path.join(format!("pwm{channel}_enable"));
    if validate_asus_curve_endpoint(&enable).is_err() || !matches!(read_u32(&enable), Some(1 | 2)) {
        return false;
    }
    (1..=ASUS_WMI_CURVE_POINTS).all(|point| {
        let temp = path.join(format!("pwm{channel}_auto_point{point}_temp"));
        let pwm = path.join(format!("pwm{channel}_auto_point{point}_pwm"));
        validate_asus_curve_endpoint(&temp).is_ok()
            && validate_asus_curve_endpoint(&pwm).is_ok()
            && matches!(read_u32(&temp), Some(0..=100))
            && matches!(read_u32(&pwm), Some(0..=255))
    })
}

fn validate_asus_curve_endpoint(path: &Path) -> RogResult<()> {
    let parent = path.parent().ok_or_else(|| {
        RogError::InvalidInput("fan endpoint is outside a verified hwmon device".to_string())
    })?;
    if fs::read_to_string(parent.join("name"))
        .ok()
        .map(|value| value.trim().to_string())
        .as_deref()
        != Some("asus_custom_fan_curve")
        || hwmon_device_identity(parent)
            .as_ref()
            .and_then(|identity| identity.file_name())
            .and_then(|name| name.to_str())
            != Some("asus-nb-wmi")
    {
        return Err(RogError::NotSupported(
            "fan endpoint is not owned by the verified ASUS WMI curve device".to_string(),
        ));
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return Err(RogError::InvalidInput(
            "fan endpoint name is invalid".to_string(),
        ));
    };
    let valid_name = name
        .strip_prefix("pwm")
        .and_then(|rest| {
            let channel_end = rest.find('_')?;
            let channel = rest[..channel_end].parse::<u32>().ok()?;
            let suffix = &rest[channel_end..];
            let valid_channel = (1..=3).contains(&channel);
            let valid_suffix = suffix == "_enable"
                || parse_auto_point_name(name).is_some_and(|(parsed_channel, point, _)| {
                    parsed_channel == channel && (1..=ASUS_WMI_CURVE_POINTS as u32).contains(&point)
                });
            Some(valid_channel && valid_suffix)
        })
        .unwrap_or(false);
    if !valid_name {
        return Err(RogError::InvalidInput(
            "fan endpoint is outside the approved ASUS WMI curve ABI".to_string(),
        ));
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| {
        RogError::TemporarilyUnavailable("verified fan endpoint disappeared".to_string())
    })?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(RogError::InvalidInput(
            "fan endpoint has an unexpected file type".to_string(),
        ));
    }
    let canonical_parent = parent.canonicalize().map_err(|_| {
        RogError::TemporarilyUnavailable("verified fan device disappeared".to_string())
    })?;
    let canonical_endpoint = path.canonicalize().map_err(|_| {
        RogError::TemporarilyUnavailable("verified fan endpoint disappeared".to_string())
    })?;
    if canonical_endpoint.parent() != Some(canonical_parent.as_path())
        || canonical_endpoint.file_name() != path.file_name()
    {
        return Err(RogError::InvalidInput(
            "fan endpoint escaped the verified ASUS WMI curve device".to_string(),
        ));
    }
    Ok(())
}

fn asus_channel_paths(path: &Path, channel: u32) -> Vec<PathBuf> {
    let mut paths = vec![path.join(format!("pwm{channel}_enable"))];
    for point in 1..=ASUS_WMI_CURVE_POINTS {
        paths.push(path.join(format!("pwm{channel}_auto_point{point}_temp")));
        paths.push(path.join(format!("pwm{channel}_auto_point{point}_pwm")));
    }
    paths
}

fn select_verified_channels<'a>(
    channels: &'a [AsusCurveChannel],
    fan_id: Option<&str>,
) -> RogResult<Vec<&'a AsusCurveChannel>> {
    let Some(id) = fan_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return Ok(channels.iter().collect());
    };
    let selected = channels
        .iter()
        .filter(|channel| channel.fan_id == id)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        Err(RogError::InvalidInput(format!(
            "unknown or unsafe fan id '{id}'"
        )))
    } else {
        Ok(selected)
    }
}

fn write_asus_curve_transaction(channel: &AsusCurveChannel, curve: &FanCurve) -> RogResult<()> {
    let result = (|| {
        for (offset, point) in curve.points.iter().enumerate() {
            let index = offset + 1;
            let temp_path = channel
                .curve_path
                .join(format!("pwm{}_auto_point{index}_temp", channel.channel));
            let pwm_path = channel
                .curve_path
                .join(format!("pwm{}_auto_point{index}_pwm", channel.channel));
            write_and_verify(&temp_path, u32::from(point.temp_c))?;
            // This allow-listed kernel ABI defines PWM point values as u8 (0..=255).
            let raw_pwm = ((u32::from(point.duty_percent) * 255) + 50) / 100;
            write_and_verify(&pwm_path, raw_pwm)?;
        }
        let enable = channel
            .curve_path
            .join(format!("pwm{}_enable", channel.channel));
        write_and_verify(&enable, 1)
    })();
    if let Err(error) = result {
        let _ = reset_asus_channel_to_auto(channel);
        return Err(error);
    }
    Ok(())
}

fn reset_asus_channel_to_auto(channel: &AsusCurveChannel) -> RogResult<()> {
    let enable = channel
        .curve_path
        .join(format!("pwm{}_enable", channel.channel));
    write_value(&enable, 3)?;
    match read_u32(&enable) {
        // The ASUS WMI driver reports 2 after processing reset command 3. A regular-file
        // fixture retains 3, which still proves that the complete command was written.
        Some(2 | 3) => Ok(()),
        _ => Err(RogError::Unexpected(
            "fan Auto reset did not read back a safe state".to_string(),
        )),
    }
}

fn write_and_verify(path: &Path, value: u32) -> RogResult<()> {
    write_value(path, value)?;
    if read_u32(path) == Some(value) {
        Ok(())
    } else {
        Err(RogError::Unexpected(
            "fan control write did not match readback".to_string(),
        ))
    }
}

fn write_value(path: &Path, value: u32) -> RogResult<()> {
    validate_asus_curve_endpoint(path)?;
    let mut file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|error| map_fan_io(error, "open fan control endpoint"))?;
    file.write_all(value.to_string().as_bytes())
        .map_err(|error| map_fan_io(error, "write fan control endpoint"))
}

fn map_fan_io(error: io::Error, operation: &str) -> RogError {
    if error.kind() == io::ErrorKind::PermissionDenied {
        RogError::PermissionDenied(format!("{operation}: administrator access is required"))
    } else {
        RogError::Unexpected(format!("{operation} failed"))
    }
}

fn temp_label(hwpath: &Path, input_name: &str) -> Option<String> {
    // tempN_input -> tempN_label
    let label_name = input_name.replace("_input", "_label");
    fs::read_to_string(hwpath.join(label_name))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn fan_label(hwpath: &Path, input_name: &str) -> Option<String> {
    // fanN_input -> fanN_label
    let label_name = input_name.replace("_input", "_label");
    fs::read_to_string(hwpath.join(label_name))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn finalize_fan_display_labels(fan_rows: &mut [FanTelemetry]) {
    let mut unlabeled_index = 1u32;
    let mut used = BTreeMap::<String, u32>::new();

    for row in fan_rows.iter_mut() {
        let base_label = if row.display_label.is_empty() {
            let label = format!("Fan {unlabeled_index}");
            unlabeled_index += 1;
            label
        } else {
            row.display_label.clone()
        };

        let count = used.entry(base_label.clone()).or_insert(0);
        *count += 1;
        row.display_label = if *count == 1 {
            base_label
        } else {
            format!("{base_label} ({})", count)
        };
    }
}

fn finalize_fan_info_labels(fans: &mut [FanInfo]) {
    let mut unlabeled_index = 1u32;
    let mut used = BTreeMap::<String, u32>::new();

    for fan in fans.iter_mut() {
        let base_label = if fan.label.is_empty() {
            let label = format!("Fan {unlabeled_index}");
            unlabeled_index += 1;
            label
        } else {
            fan.label.clone()
        };

        let count = used.entry(base_label.clone()).or_insert(0);
        *count += 1;
        fan.label = if *count == 1 {
            base_label
        } else {
            format!("{base_label} ({})", count)
        };
    }
}

fn fan_index_from_input_name(name: &str) -> Option<u32> {
    let raw = name.strip_prefix("fan")?.strip_suffix("_input")?;
    raw.parse::<u32>().ok()
}

fn read_u32(path: &Path) -> Option<u32> {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
}

fn path_exists(path: &Path) -> bool {
    fs::metadata(path).is_ok()
}

fn stable_hwmon_id(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("hwmon")
        .to_string()
}

fn auto_point_endpoints(hwpath: &Path, index: u32) -> Vec<String> {
    let mut endpoints = Vec::new();
    let Ok(entries) = fs::read_dir(hwpath) else {
        return endpoints;
    };
    let prefix = format!("pwm{index}_auto_point");
    for ent in entries.flatten() {
        let path = ent.path();
        let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if fname.starts_with(&prefix) {
            endpoints.push(format!("auto_point:{}", path.display()));
        }
    }
    endpoints.sort();
    endpoints
}

#[derive(Debug, Default)]
struct FanCurveDiscovery {
    readable: bool,
    endpoints: Vec<String>,
    notes: Vec<String>,
    warnings: Vec<String>,
}

fn discover_asus_curve_points(hwpath: &Path) -> FanCurveDiscovery {
    let mut discovery = FanCurveDiscovery::default();
    let Ok(entries) = fs::read_dir(hwpath) else {
        discovery
            .warnings
            .push("ASUS fan-curve hwmon device could not be inspected.".to_string());
        return discovery;
    };
    let mut points = BTreeMap::<u32, BTreeMap<u32, (bool, bool)>>::new();
    let mut enable_channels = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if let Some(channel) = name
            .strip_prefix("pwm")
            .and_then(|rest| rest.strip_suffix("_enable"))
            .and_then(|raw| raw.parse::<u32>().ok())
        {
            discovery
                .endpoints
                .push(format!("curve_enable_candidate:{}", path.display()));
            if let Some(value) = read_u32(&path) {
                enable_channels.push(format!("{channel}={value}"));
            }
            continue;
        }
        let Some((channel, point, is_temp)) = parse_auto_point_name(name) else {
            continue;
        };
        discovery
            .endpoints
            .push(format!("curve_point_candidate:{}", path.display()));
        let pair = points
            .entry(channel)
            .or_default()
            .entry(point)
            .or_insert((false, false));
        if read_u32(&path).is_some() {
            if is_temp {
                pair.0 = true;
            } else {
                pair.1 = true;
            }
        }
    }

    let complete = !points.is_empty()
        && points
            .values()
            .all(|channel| channel.len() >= 2 && channel.values().all(|pair| pair.0 && pair.1));
    discovery.readable = complete;
    if complete {
        let point_counts = points
            .values()
            .map(|channel| channel.len().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let enables = if enable_channels.is_empty() {
            "unavailable".to_string()
        } else {
            enable_channels.join(", ")
        };
        discovery.notes.push(format!(
            "Readable ASUS WMI fan-curve candidate: {} channel(s), paired point counts [{}], enable values [{}].",
            points.len(), point_counts, enables
        ));
        discovery.notes.push(
            "Curve writes are intentionally disabled; readable sysfs attributes do not prove daemon write access or a complete safe restore contract."
                .to_string(),
        );
    } else if !points.is_empty() {
        discovery.warnings.push(
            "ASUS fan-curve candidate is incomplete or contains unreadable point pairs."
                .to_string(),
        );
    }
    discovery.endpoints.sort();
    discovery
}

fn parse_auto_point_name(name: &str) -> Option<(u32, u32, bool)> {
    let rest = name.strip_prefix("pwm")?;
    let (channel, rest) = rest.split_once("_auto_point")?;
    let channel = channel.parse::<u32>().ok()?;
    let (point, kind) = rest.rsplit_once('_')?;
    let point = point.parse::<u32>().ok()?;
    match kind {
        "temp" => Some((channel, point, true)),
        "pwm" => Some((channel, point, false)),
        _ => None,
    }
}

fn select_fans<'a>(fans: &'a [FanInfo], fan_id: Option<&str>) -> RogResult<Vec<&'a FanInfo>> {
    let Some(fan_id) = fan_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(fans.iter().filter(|fan| fan.controllable).collect());
    };
    let selected = fans
        .iter()
        .filter(|fan| fan.id == fan_id || fan.label.eq_ignore_ascii_case(fan_id))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        Err(RogError::InvalidInput(format!("unknown fan id '{fan_id}'")))
    } else {
        Ok(selected)
    }
}

fn pwm_to_percent(pwm: u32) -> u8 {
    (((pwm.min(255) * 100) + 127) / 255) as u8
}

fn friendly_fan_label(raw: &str) -> String {
    let words = raw
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(friendly_label_word)
        .collect::<Vec<_>>();

    if words.is_empty() {
        raw.trim().to_string()
    } else {
        words.join(" ")
    }
}

fn friendly_label_word(word: &str) -> String {
    let lower = word.to_ascii_lowercase();
    match lower.as_str() {
        "cpu" => "CPU".to_string(),
        "gpu" => "GPU".to_string(),
        "acpi" => "ACPI".to_string(),
        "vrm" => "VRM".to_string(),
        _ => {
            let mut chars = lower.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let mut out = String::new();
            out.push(first.to_ascii_uppercase());
            out.extend(chars);
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rog_core::FanPoint;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fan(display_label: &str, raw_label: Option<&str>) -> FanTelemetry {
        FanTelemetry {
            hwmon_device: "asus".to_string(),
            hwmon_path: "/sys/class/hwmon/hwmon0".to_string(),
            input_path: "/sys/class/hwmon/hwmon0/fan1_input".to_string(),
            raw_label: raw_label.map(|value| value.to_string()),
            display_label: display_label.to_string(),
            rpm: Some(4200),
        }
    }

    #[test]
    fn finalizes_fallback_labels_and_disambiguates_duplicates() {
        let mut rows = vec![
            fan("", None),
            fan("CPU Fan", Some("cpu_fan")),
            fan("CPU Fan", Some("cpu_fan")),
            fan("", None),
        ];

        finalize_fan_display_labels(&mut rows);

        assert_eq!(rows[0].display_label, "Fan 1");
        assert_eq!(rows[1].display_label, "CPU Fan");
        assert_eq!(rows[2].display_label, "CPU Fan (2)");
        assert_eq!(rows[3].display_label, "Fan 2");
    }

    #[test]
    fn normalizes_common_hwmon_fan_labels() {
        assert_eq!(friendly_fan_label("cpu_fan"), "CPU Fan");
        assert_eq!(friendly_fan_label("gpu-fan"), "GPU Fan");
        assert_eq!(friendly_fan_label("mid_fan"), "Mid Fan");
    }

    #[test]
    fn list_fans_reports_read_only_telemetry_without_pwm() {
        let root = temp_hwmon_root("readonly");
        let hwmon0 = root.join("hwmon0");
        fs::create_dir_all(&hwmon0).unwrap();
        fs::write(hwmon0.join("name"), "asus\n").unwrap();
        fs::write(hwmon0.join("fan1_input"), "2400\n").unwrap();
        fs::write(hwmon0.join("fan1_label"), "cpu_fan\n").unwrap();

        let provider = HwmonTelemetryProvider::new(root.clone());
        let fans = provider.list_fans().unwrap();

        assert_eq!(fans.len(), 1);
        assert_eq!(fans[0].label, "CPU Fan");
        assert_eq!(fans[0].current_rpm, Some(2400));
        assert!(!fans[0].controllable);
        assert!(!fans[0].supports_manual_percent);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn list_fans_reports_pwm_as_diagnostic_candidate_only() {
        let root = temp_hwmon_root("pwm");
        let hwmon0 = root.join("hwmon0");
        fs::create_dir_all(&hwmon0).unwrap();
        fs::write(hwmon0.join("name"), "asus\n").unwrap();
        fs::write(hwmon0.join("fan1_input"), "3000\n").unwrap();
        fs::write(hwmon0.join("pwm1"), "128\n").unwrap();
        fs::write(hwmon0.join("pwm1_enable"), "2\n").unwrap();

        let provider = HwmonTelemetryProvider::new(root.clone());
        let fans = provider.list_fans().unwrap();

        assert_eq!(fans.len(), 1);
        assert!(!fans[0].controllable);
        assert!(!fans[0].supports_manual_percent);
        assert!(!fans[0].supports_auto);
        assert!(fans[0]
            .endpoints
            .iter()
            .any(|endpoint| endpoint.starts_with("pwm:")));
        assert_eq!(fans[0].current_percent, Some(50));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn manual_percent_rejects_unverified_generic_pwm() {
        let root = temp_hwmon_root("write");
        let hwmon0 = root.join("hwmon0");
        fs::create_dir_all(&hwmon0).unwrap();
        fs::write(hwmon0.join("name"), "asus\n").unwrap();
        fs::write(hwmon0.join("fan1_input"), "3000\n").unwrap();
        fs::write(hwmon0.join("pwm1"), "0\n").unwrap();
        fs::write(hwmon0.join("pwm1_enable"), "2\n").unwrap();

        let provider = HwmonTelemetryProvider::new(root.clone());
        let fan_id = provider.list_fans().unwrap()[0].id.clone();
        assert!(provider.set_fan_manual_percent(&fan_id, 50).is_err());
        assert_eq!(
            fs::read_to_string(hwmon0.join("pwm1_enable")).unwrap(),
            "2\n"
        );
        assert_eq!(fs::read_to_string(hwmon0.join("pwm1")).unwrap(), "0\n");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discovers_complete_asus_curve_as_read_only() {
        let root = temp_hwmon_root("asus-curve");
        let rpm = root.join("hwmon0");
        let curve = root.join("hwmon1");
        fs::create_dir_all(&rpm).unwrap();
        fs::create_dir_all(&curve).unwrap();
        fs::write(rpm.join("name"), "asus\n").unwrap();
        fs::write(rpm.join("fan1_input"), "2400\n").unwrap();
        fs::write(rpm.join("fan1_label"), "cpu_fan\n").unwrap();
        fs::write(curve.join("name"), "asus_custom_fan_curve\n").unwrap();
        fs::write(curve.join("pwm1_enable"), "2\n").unwrap();
        for (point, temp, pwm) in [(1, 40, 22), (2, 55, 28)] {
            fs::write(
                curve.join(format!("pwm1_auto_point{point}_temp")),
                format!("{temp}\n"),
            )
            .unwrap();
            fs::write(
                curve.join(format!("pwm1_auto_point{point}_pwm")),
                format!("{pwm}\n"),
            )
            .unwrap();
        }

        let caps = HwmonTelemetryProvider::new(root.clone())
            .fan_caps()
            .unwrap();
        assert!(caps.has_fan_reading);
        assert!(caps.fan_curve_readable);
        assert!(!caps.fan_curve_writable);
        assert!(!caps.has_fan_curves);
        assert_eq!(
            caps.fan_mapping_confidence,
            FanMappingConfidence::HardwareLabel
        );
        assert!(caps
            .endpoints
            .iter()
            .any(|endpoint| endpoint.starts_with("curve_point_candidate:")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_malformed_curve_point_names_and_incomplete_pairs() {
        assert_eq!(
            parse_auto_point_name("pwm3_auto_point8_temp"),
            Some((3, 8, true))
        );
        assert_eq!(
            parse_auto_point_name("pwm3_auto_point8_pwm"),
            Some((3, 8, false))
        );
        assert_eq!(parse_auto_point_name("pwm3_auto_point_temp"), None);
        assert_eq!(parse_auto_point_name("fan1_input"), None);

        let root = temp_hwmon_root("incomplete-curve");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("pwm1_auto_point1_temp"), "40\n").unwrap();
        assert!(!discover_asus_curve_points(&root).readable);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    fn verified_asus_fixture(name: &str) -> (PathBuf, PathBuf) {
        let root = temp_hwmon_root(name);
        let rpm = root.join("hwmon0");
        let curve = root.join("hwmon1");
        let device = root.join("asus-nb-wmi");
        fs::create_dir_all(&rpm).unwrap();
        fs::create_dir_all(&curve).unwrap();
        fs::create_dir_all(&device).unwrap();
        symlink(&device, rpm.join("device")).unwrap();
        symlink(&device, curve.join("device")).unwrap();
        fs::write(rpm.join("name"), "asus\n").unwrap();
        fs::write(rpm.join("fan1_input"), "2400\n").unwrap();
        fs::write(rpm.join("fan1_label"), "cpu_fan\n").unwrap();
        fs::write(curve.join("name"), "asus_custom_fan_curve\n").unwrap();
        fs::write(curve.join("pwm1_enable"), "2\n").unwrap();
        for point in 1..=ASUS_WMI_CURVE_POINTS {
            fs::write(
                curve.join(format!("pwm1_auto_point{point}_temp")),
                format!("{}\n", 35 + point * 5),
            )
            .unwrap();
            fs::write(
                curve.join(format!("pwm1_auto_point{point}_pwm")),
                format!("{}\n", 20 + point * 10),
            )
            .unwrap();
        }
        (root, curve)
    }

    #[cfg(unix)]
    #[test]
    fn verified_asus_mapping_exposes_only_curve_and_auto_control() {
        let (root, _) = verified_asus_fixture("verified");
        let provider = HwmonTelemetryProvider::new(root.clone());
        let fans = provider.list_fans().unwrap();
        assert_eq!(fans[0].id, "asus-wmi:cpu");
        assert!(fans[0].rpm_readable);
        assert!(fans[0].pwm_endpoint_verified);
        assert!(fans[0].supports_curve);
        assert!(fans[0].supports_auto);
        assert!(!fans[0].supports_manual_percent);
        assert!(!fans[0].supports_manual_rpm_target);
        assert!(!provider.fan_caps().unwrap().has_fan_boost);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn verified_curve_write_converts_percent_and_can_reset_auto() {
        let (root, curve_path) = verified_asus_fixture("write-curve");
        let provider = HwmonTelemetryProvider::new(root.clone());
        let curve = FanCurve {
            domain: FanDomain::Cpu,
            points: (0..8)
                .map(|point| FanPoint {
                    temp_c: 40 + point * 5,
                    duty_percent: 20 + point * 10,
                })
                .collect(),
        };
        provider.set_fan_curve("asus-wmi:cpu", curve).unwrap();
        assert_eq!(
            fs::read_to_string(curve_path.join("pwm1_auto_point1_pwm"))
                .unwrap()
                .trim(),
            "51"
        );
        assert_eq!(
            fs::read_to_string(curve_path.join("pwm1_enable"))
                .unwrap()
                .trim(),
            "1"
        );
        provider.set_fan_auto(Some("asus-wmi:cpu")).unwrap();
        assert_eq!(
            fs::read_to_string(curve_path.join("pwm1_enable"))
                .unwrap()
                .trim(),
            "3"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn verified_curve_rejects_a_symlinked_write_attribute() {
        let (root, curve_path) = verified_asus_fixture("write-symlink");
        let outside = root.join("outside-pwm");
        fs::write(&outside, "128\n").unwrap();
        let endpoint = curve_path.join("pwm1_auto_point1_pwm");
        fs::remove_file(&endpoint).unwrap();
        symlink(&outside, &endpoint).unwrap();

        let provider = HwmonTelemetryProvider::new(root.clone());
        assert!(matches!(
            provider.verified_asus_curve_channels(),
            Err(RogError::NotSupported(_))
        ));
        assert!(matches!(
            write_value(&endpoint, 51),
            Err(RogError::InvalidInput(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn curve_write_rejects_wrong_point_count_and_unknown_id() {
        let (root, _) = verified_asus_fixture("invalid-curve");
        let provider = HwmonTelemetryProvider::new(root.clone());
        let curve = FanCurve {
            domain: FanDomain::Cpu,
            points: vec![FanPoint {
                temp_c: 50,
                duty_percent: 50,
            }],
        };
        assert!(matches!(
            provider.set_fan_curve("asus-wmi:cpu", curve.clone()),
            Err(RogError::InvalidInput(_))
        ));
        assert!(matches!(
            provider.set_fan_curve(
                "fan1",
                FanCurve {
                    points: (0..8)
                        .map(|point| FanPoint {
                            temp_c: 40 + point * 5,
                            duty_percent: 20 + point * 10
                        })
                        .collect(),
                    ..curve
                }
            ),
            Err(RogError::InvalidInput(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    fn temp_hwmon_root(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rog-helper-hwmon-test-{name}-{}-{stamp}",
            std::process::id()
        ))
    }
}
