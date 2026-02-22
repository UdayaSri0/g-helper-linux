use std::fs;
use std::path::{Path, PathBuf};

use rog_core::{BatteryState, RogError, RogResult};

#[derive(Debug, Clone)]
pub struct PowerSupplySysfsProvider {
    root: PathBuf,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PowerSupplyBatteryStatus {
    pub battery_state: Option<BatteryState>,
    pub battery_health_percent: Option<f32>,
    pub battery_cycle_count: Option<u32>,
    pub battery_charge_power_w: Option<f32>,
    pub battery_discharge_power_w: Option<f32>,
}

impl Default for PowerSupplySysfsProvider {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/sys/class/power_supply"),
        }
    }
}

impl PowerSupplySysfsProvider {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn read_battery_status(&self) -> RogResult<PowerSupplyBatteryStatus> {
        let bat = match pick_battery_dir(&self.root)? {
            Some(v) => v,
            None => return Ok(PowerSupplyBatteryStatus::default()),
        };

        let mut out = PowerSupplyBatteryStatus {
            battery_state: read_string_trimmed(bat.join("status")).and_then(parse_battery_state),
            battery_cycle_count: read_u32(bat.join("cycle_count")),
            battery_health_percent: read_battery_health_percent(&bat),
            ..Default::default()
        };

        // Power rate: prefer power_now (microwatts), else current_now * voltage_now.
        let power_w = read_power_w(&bat);
        if let (Some(state), Some(w)) = (out.battery_state, power_w) {
            match state {
                BatteryState::Charging | BatteryState::PendingCharge => {
                    out.battery_charge_power_w = Some(w);
                }
                BatteryState::Discharging | BatteryState::PendingDischarge => {
                    out.battery_discharge_power_w = Some(w);
                }
                _ => {}
            }
        }

        Ok(out)
    }
}

fn pick_battery_dir(root: &Path) -> RogResult<Option<PathBuf>> {
    let entries = fs::read_dir(root)
        .map_err(|e| RogError::DependencyMissing(format!("power_supply sysfs unavailable: {e}")))?;

    let mut bats: Vec<(String, PathBuf)> = Vec::new();
    for ent in entries.flatten() {
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }

        let Some(name) = ent.file_name().to_str().map(|s| s.to_string()) else {
            continue;
        };

        let typ = read_string_trimmed(path.join("type"));
        if !typ
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case("Battery"))
            .unwrap_or(false)
        {
            continue;
        }

        // If present exists and is 0, ignore this battery.
        if let Some(present) = read_u32(path.join("present")) {
            if present == 0 {
                continue;
            }
        }

        bats.push((name, path));
    }

    bats.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    Ok(bats.into_iter().map(|(_, p)| p).next())
}

fn read_string_trimmed(path: PathBuf) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn read_u32(path: PathBuf) -> Option<u32> {
    let s = fs::read_to_string(path).ok()?;
    s.trim().parse::<u32>().ok()
}

fn read_u64(path: PathBuf) -> Option<u64> {
    let s = fs::read_to_string(path).ok()?;
    s.trim().parse::<u64>().ok()
}

fn read_i64(path: PathBuf) -> Option<i64> {
    let s = fs::read_to_string(path).ok()?;
    s.trim().parse::<i64>().ok()
}

fn parse_battery_state(raw: String) -> Option<BatteryState> {
    let s = raw.trim().to_ascii_lowercase();
    match s.as_str() {
        "charging" => Some(BatteryState::Charging),
        "discharging" => Some(BatteryState::Discharging),
        "full" => Some(BatteryState::Full),
        "empty" => Some(BatteryState::Empty),
        "not charging" => Some(BatteryState::NotCharging),
        "unknown" => Some(BatteryState::Unknown),
        _ => None,
    }
}

fn read_battery_health_percent(bat: &Path) -> Option<f32> {
    // Prefer energy_* (uWh). If not present, fall back to charge_* (uAh).
    let (full, design) = match (
        read_u64(bat.join("energy_full")),
        read_u64(bat.join("energy_full_design")),
    ) {
        (Some(f), Some(d)) if d > 0 => (f, d),
        _ => match (
            read_u64(bat.join("charge_full")),
            read_u64(bat.join("charge_full_design")),
        ) {
            (Some(f), Some(d)) if d > 0 => (f, d),
            _ => return None,
        },
    };

    let pct = (full as f64) / (design as f64) * 100.0;
    if pct.is_finite() {
        Some(pct as f32)
    } else {
        None
    }
}

fn read_power_w(bat: &Path) -> Option<f32> {
    // power_now is typically microwatts.
    if let Some(microw) = read_i64(bat.join("power_now"))
        .or_else(|| read_u64(bat.join("power_now")).map(|v| v as i64))
    {
        let w = (microw.abs() as f64) / 1_000_000.0;
        return w.is_finite().then_some(w as f32);
    }

    // current_now (uA) * voltage_now (uV) = 1e-12 W.
    let current = read_i64(bat.join("current_now"))
        .or_else(|| read_u64(bat.join("current_now")).map(|v| v as i64))?;
    let voltage = read_i64(bat.join("voltage_now"))
        .or_else(|| read_u64(bat.join("voltage_now")).map(|v| v as i64))?;
    let p = (current.abs() as i128) * (voltage.abs() as i128);
    let w = (p as f64) / 1_000_000_000_000.0;
    w.is_finite().then_some(w as f32)
}
