use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rog_core::{FanTelemetry, RogError, RogResult, TelemetrySnapshot};
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
}
