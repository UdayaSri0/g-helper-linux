use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rog_core::{
    validate_fan_percent, validate_fan_request, FanCaps, FanControlMode, FanControlRequest,
    FanCurve, FanDomain, FanInfo, FanState, FanTelemetry, RogError, RogResult, TelemetrySnapshot,
};
use tracing::debug;

#[derive(Debug, Clone)]
pub struct HwmonTelemetryProvider {
    root: PathBuf,
}

impl Default for HwmonTelemetryProvider {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/sys/class/hwmon"),
        }
    }
}

impl HwmonTelemetryProvider {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
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
        for (hwname, hwpath) in hwmons {
            out.extend(read_fan_infos(&hwname, &hwpath));
        }
        finalize_fan_info_labels(&mut out);
        Ok(out)
    }

    pub fn fan_caps(&self) -> RogResult<FanCaps> {
        self.list_fans().map(|fans| FanCaps::from_fans(&fans))
    }

    pub fn fan_state(&self) -> RogResult<FanState> {
        self.list_fans().map(FanState::from_fans)
    }

    pub fn set_fan_auto(&self, fan_id: Option<&str>) -> RogResult<()> {
        let fans = self.list_fans()?;
        let selected = select_fans(&fans, fan_id)?;
        if selected.is_empty() {
            return Err(RogError::NotSupported(
                "no fans exposing Auto/BIOS restore were detected".to_string(),
            ));
        }

        let mut errors = Vec::new();
        for fan in selected {
            if !fan.supports_auto {
                errors.push(format!("{} does not expose writable pwm_enable", fan.label));
                continue;
            }
            let Some(path) = endpoint_path(fan, "pwm_enable:") else {
                errors.push(format!("{} has no pwm_enable endpoint", fan.label));
                continue;
            };
            if let Err(err) = write_pwm_enable_auto(&path) {
                errors.push(format!("{}: {err}", fan.label));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(RogError::PermissionDenied(format!(
                "failed to restore Auto for one or more fans: {}",
                errors.join("; ")
            )))
        }
    }

    pub fn set_fan_manual_percent(&self, fan_id: &str, percent: u8) -> RogResult<()> {
        validate_fan_percent(percent)?;
        let fans = self.list_fans()?;
        let selected = select_fans(&fans, Some(fan_id))?;
        if selected.is_empty() {
            return Err(RogError::NotSupported(
                "no fans exposing manual percentage control were detected".to_string(),
            ));
        }

        let mut applied = Vec::new();
        let mut errors = Vec::new();
        for fan in selected {
            if !fan.supports_manual_percent {
                errors.push(format!(
                    "{} does not support manual percentage control",
                    fan.label
                ));
                continue;
            }
            let Some(enable_path) = endpoint_path(fan, "pwm_enable:") else {
                errors.push(format!("{} has no pwm_enable endpoint", fan.label));
                continue;
            };
            let Some(pwm_path) = endpoint_path(fan, "pwm:") else {
                errors.push(format!("{} has no pwm endpoint", fan.label));
                continue;
            };
            if let Err(err) = write_sysfs(&enable_path, "1\n") {
                errors.push(format!(
                    "{}: failed to enable manual mode: {err}",
                    fan.label
                ));
                continue;
            }
            applied.push(fan.id.clone());
            let pwm_value = percent_to_pwm(percent);
            if let Err(err) = write_sysfs(&pwm_path, &format!("{pwm_value}\n")) {
                errors.push(format!("{}: failed to write PWM value: {err}", fan.label));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            let _ = self.set_fan_auto(None);
            Err(RogError::PermissionDenied(format!(
                "fan manual write failed after applying to [{}]: {}",
                applied.join(", "),
                errors.join("; ")
            )))
        }
    }

    pub fn set_fan_rpm_target(&self, fan_id: &str, rpm: u32) -> RogResult<()> {
        if rpm == 0 {
            return Err(RogError::InvalidInput(
                "fan RPM target must be greater than zero".to_string(),
            ));
        }
        let fans = self.list_fans()?;
        let selected = select_fans(&fans, Some(fan_id))?;
        if selected.len() != 1 {
            return Err(RogError::InvalidInput(
                "RPM target requires one explicit fan id".to_string(),
            ));
        }
        let fan = selected[0];
        if !fan.supports_manual_rpm_target {
            return Err(RogError::NotSupported(format!(
                "{} does not expose writable fan target RPM control",
                fan.label
            )));
        }
        let Some(path) = endpoint_path(fan, "fan_target:") else {
            return Err(RogError::NotSupported(format!(
                "{} has no fan target endpoint",
                fan.label
            )));
        };
        write_sysfs(&path, &format!("{rpm}\n"))
    }

    pub fn set_fan_curve(&self, fan_id: &str, curve: FanCurve) -> RogResult<()> {
        curve.validate_safe(rog_core::FanCurvePolicy::default())?;
        let fans = self.list_fans()?;
        let selected = select_fans(&fans, Some(fan_id))?;
        if selected.iter().any(|fan| !fan.supports_curve) {
            return Err(RogError::NotSupported(
                "generic hwmon fan curve writes are not enabled for this backend".to_string(),
            ));
        }
        Err(RogError::NotSupported(
            "generic hwmon fan curve write format is hardware-specific and was not confirmed"
                .to_string(),
        ))
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

        let pwm_writable = path_is_writable(&pwm);
        let pwm_enable_writable = path_is_writable(&pwm_enable);
        let fan_target_writable = path_is_writable(&fan_target);
        let supports_manual_percent = pwm_writable && pwm_enable_writable;
        let supports_manual_rpm_target = fan_target_writable;
        let supports_auto = pwm_enable_writable;
        let current_percent = read_u32(&pwm).map(pwm_to_percent);
        let supports_curve = false;

        let mut notes = Vec::new();
        let mut warnings = Vec::new();
        if path_exists(&pwm) && !pwm_writable {
            warnings
                .push("PWM endpoint exists but is not writable by the daemon user.".to_string());
        }
        if path_exists(&fan_target) && !fan_target_writable {
            warnings.push(
                "RPM target endpoint exists but is not writable by the daemon user.".to_string(),
            );
        }
        if !supports_manual_percent && !supports_manual_rpm_target {
            notes.push(
                "Telemetry only; no writable fan-control endpoint was confirmed.".to_string(),
            );
        } else {
            notes.push("Writable hwmon fan-control endpoint confirmed.".to_string());
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
            current_rpm: rpm,
            min_rpm,
            max_rpm,
            current_percent,
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

fn path_is_writable(path: &Path) -> bool {
    OpenOptions::new().write(true).open(path).is_ok()
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

fn endpoint_path(fan: &FanInfo, prefix: &str) -> Option<PathBuf> {
    fan.endpoints
        .iter()
        .find_map(|endpoint| endpoint.strip_prefix(prefix))
        .map(PathBuf::from)
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

fn percent_to_pwm(percent: u8) -> u32 {
    ((percent as u32) * 255 + 50) / 100
}

fn pwm_to_percent(pwm: u32) -> u8 {
    (((pwm.min(255) * 100) + 127) / 255) as u8
}

fn write_pwm_enable_auto(path: &Path) -> RogResult<()> {
    match write_sysfs(path, "2\n") {
        Ok(()) => Ok(()),
        Err(first) => match write_sysfs(path, "0\n") {
            Ok(()) => Ok(()),
            Err(second) => Err(RogError::PermissionDenied(format!(
                "auto restore rejected values 2 ({first}) and 0 ({second})"
            ))),
        },
    }
}

fn write_sysfs(path: &Path, value: &str) -> RogResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => {
                RogError::PermissionDenied(format!("{}: {e}", path.display()))
            }
            _ => RogError::TemporarilyUnavailable(format!("{}: {e}", path.display())),
        })?;
    file.write_all(value.as_bytes())
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => {
                RogError::PermissionDenied(format!("{}: {e}", path.display()))
            }
            _ => RogError::TemporarilyUnavailable(format!("{}: {e}", path.display())),
        })
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
    fn list_fans_reports_writable_pwm_support_and_endpoints() {
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
        assert!(fans[0].supports_manual_percent);
        assert!(fans[0].supports_auto);
        assert!(fans[0]
            .endpoints
            .iter()
            .any(|endpoint| endpoint.starts_with("pwm:")));
        assert_eq!(fans[0].current_percent, Some(50));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn manual_percent_writes_pwm_and_manual_enable() {
        let root = temp_hwmon_root("write");
        let hwmon0 = root.join("hwmon0");
        fs::create_dir_all(&hwmon0).unwrap();
        fs::write(hwmon0.join("name"), "asus\n").unwrap();
        fs::write(hwmon0.join("fan1_input"), "3000\n").unwrap();
        fs::write(hwmon0.join("pwm1"), "0\n").unwrap();
        fs::write(hwmon0.join("pwm1_enable"), "2\n").unwrap();

        let provider = HwmonTelemetryProvider::new(root.clone());
        let fan_id = provider.list_fans().unwrap()[0].id.clone();
        provider.set_fan_manual_percent(&fan_id, 50).unwrap();

        assert_eq!(
            fs::read_to_string(hwmon0.join("pwm1_enable")).unwrap(),
            "1\n"
        );
        assert_eq!(fs::read_to_string(hwmon0.join("pwm1")).unwrap(), "128\n");

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
