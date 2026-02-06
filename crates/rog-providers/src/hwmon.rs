use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rog_core::{RogError, RogResult, TelemetrySnapshot};
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
            for (key, val) in read_fan_inputs(&hwname, &hwpath) {
                fans.insert(key, val);
            }
        }

        snapshot.cpu_temp_c = cpu_candidates.into_iter().max_by(|a, b| a.total_cmp(b));
        snapshot.gpu_temp_c = gpu_candidates.into_iter().max_by(|a, b| a.total_cmp(b));

        snapshot.temps_c = temps;
        snapshot.fans_rpm = fans;
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

fn read_fan_inputs(hwname: &str, hwpath: &Path) -> Vec<(String, u32)> {
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
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(rpm) = raw.trim().parse::<u32>() else {
            continue;
        };

        let label = fan_label(hwpath, fname)
            .or_else(|| Some(fname.replace("_input", "")))
            .unwrap_or_else(|| "fan".to_string());

        let key = format!("{hwname}:{label}");
        out.push((key, rpm));
    }
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
