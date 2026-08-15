use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use rog_core::{BatteryLimitPercent, BatteryState, RogError, RogResult};

#[derive(Debug, Clone)]
pub struct PowerSupplySysfsProvider {
    root: PathBuf,
}

/// Path-free semantic handle to the standard Linux power-supply charge-limit
/// ABI. The endpoint is discovered from an exact `type=Battery` device and is
/// never accepted from, or exposed to, a D-Bus caller.
#[derive(Debug, Clone)]
pub struct BatteryChargeLimitControl {
    discovery_root: PathBuf,
    battery_dir: PathBuf,
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

    pub fn charge_limit_control(&self) -> RogResult<Option<BatteryChargeLimitControl>> {
        let candidates = battery_dirs(&self.root)?
            .into_iter()
            .filter(|path| path.join("charge_control_end_threshold").is_file())
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [] => Ok(None),
            [battery_dir] => {
                let control = BatteryChargeLimitControl {
                    discovery_root: self.root.clone(),
                    battery_dir: battery_dir.clone(),
                };
                control.validate_identity()?;
                Ok(Some(control))
            }
            _ => Err(RogError::NotSupported(
                "multiple batteries expose charge limits; refusing an ambiguous write".to_string(),
            )),
        }
    }
}

impl BatteryChargeLimitControl {
    fn endpoint(&self) -> PathBuf {
        self.battery_dir.join("charge_control_end_threshold")
    }

    fn validate_identity(&self) -> RogResult<()> {
        if self.battery_dir.parent() != Some(self.discovery_root.as_path()) {
            return Err(RogError::NotSupported(
                "battery charge-limit endpoint escaped its discovery hierarchy".to_string(),
            ));
        }
        let current_candidates = battery_dirs(&self.discovery_root)?
            .into_iter()
            .filter(|path| path.join("charge_control_end_threshold").is_file())
            .collect::<Vec<_>>();
        if current_candidates.len() != 1 || current_candidates[0] != self.battery_dir {
            return Err(RogError::NotSupported(
                "battery charge-limit endpoint is no longer the sole unambiguous candidate"
                    .to_string(),
            ));
        }
        let type_path = self.battery_dir.join("type");
        let battery_type = read_string_trimmed(type_path.clone());
        if !battery_type
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("Battery"))
        {
            return Err(RogError::NotSupported(
                "charge-limit endpoint is not attached to a battery power supply".to_string(),
            ));
        }
        let endpoint = self.endpoint();
        if !endpoint.is_file() {
            return Err(RogError::NotSupported(
                "battery charge-limit endpoint is unavailable".to_string(),
            ));
        }
        let canonical_battery = self.battery_dir.canonicalize().map_err(|_| {
            RogError::NotSupported("battery device could not be resolved safely".to_string())
        })?;
        let canonical_endpoint = endpoint.canonicalize().map_err(|_| {
            RogError::NotSupported(
                "battery charge-limit endpoint could not be resolved safely".to_string(),
            )
        })?;
        let canonical_type = type_path.canonicalize().map_err(|_| {
            RogError::NotSupported(
                "battery type attribute could not be resolved safely".to_string(),
            )
        })?;
        let endpoint_metadata = fs::symlink_metadata(&endpoint).map_err(|_| {
            RogError::NotSupported(
                "battery charge-limit endpoint metadata is unavailable".to_string(),
            )
        })?;
        let type_metadata = fs::symlink_metadata(&type_path).map_err(|_| {
            RogError::NotSupported("battery type attribute metadata is unavailable".to_string())
        })?;
        if endpoint_metadata.file_type().is_symlink()
            || !endpoint_metadata.file_type().is_file()
            || type_metadata.file_type().is_symlink()
            || !type_metadata.file_type().is_file()
        {
            return Err(RogError::NotSupported(
                "battery control attributes have an unexpected file type".to_string(),
            ));
        }
        if canonical_endpoint.parent() != Some(canonical_battery.as_path())
            || canonical_endpoint
                .file_name()
                .and_then(|name| name.to_str())
                != Some("charge_control_end_threshold")
        {
            return Err(RogError::NotSupported(
                "battery charge-limit endpoint escaped its battery device".to_string(),
            ));
        }
        if canonical_type.parent() != Some(canonical_battery.as_path())
            || canonical_type.file_name().and_then(|name| name.to_str()) != Some("type")
        {
            return Err(RogError::NotSupported(
                "battery type attribute escaped its battery device".to_string(),
            ));
        }
        Ok(())
    }

    pub fn can_write_directly(&self) -> bool {
        self.validate_identity().is_ok()
            && OpenOptions::new().write(true).open(self.endpoint()).is_ok()
    }

    pub fn read_limit(&self) -> RogResult<BatteryLimitPercent> {
        self.validate_identity()?;
        let raw = fs::read_to_string(self.endpoint()).map_err(|error| {
            RogError::TemporarilyUnavailable(format!("battery charge-limit read failed: {error}"))
        })?;
        let value = raw.trim().parse::<u8>().map_err(|_| {
            RogError::Unexpected("battery charge-limit value is not a percentage".to_string())
        })?;
        if value <= rog_core::BATTERY_CHARGE_LIMIT_MAX_PERCENT {
            Ok(BatteryLimitPercent(value))
        } else {
            Err(RogError::Unexpected(
                "battery charge-limit value exceeds the kernel ABI range".to_string(),
            ))
        }
    }

    pub fn set_limit(&self, limit: BatteryLimitPercent) -> RogResult<BatteryLimitPercent> {
        let limit = limit.validate_control()?;
        self.validate_identity()?;
        let endpoint = self.endpoint();
        let mut file =
            OpenOptions::new()
                .write(true)
                .open(&endpoint)
                .map_err(|error| match error.kind() {
                    std::io::ErrorKind::PermissionDenied => RogError::PermissionDenied(
                        "battery charge-limit endpoint requires administrator access".to_string(),
                    ),
                    _ => RogError::TemporarilyUnavailable(format!(
                        "battery charge-limit endpoint could not be opened: {error}"
                    )),
                })?;
        writeln!(file, "{}", limit.0).map_err(|error| {
            RogError::TransientFailure(format!("battery charge-limit write failed: {error}"))
        })?;
        drop(file);
        self.read_limit()
    }
}

fn pick_battery_dir(root: &Path) -> RogResult<Option<PathBuf>> {
    Ok(battery_dirs(root)?.into_iter().next())
}

fn battery_dirs(root: &Path) -> RogResult<Vec<PathBuf>> {
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
    Ok(bats.into_iter().map(|(_, p)| p).collect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "rog-helper-power-supply-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("fixture root");
            Self(path)
        }

        fn supply(&self, name: &str, kind: &str, limit: Option<u8>) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(&path).expect("supply directory");
            fs::write(path.join("type"), format!("{kind}\n")).expect("type");
            fs::write(path.join("present"), "1\n").expect("present");
            if let Some(limit) = limit {
                fs::write(
                    path.join("charge_control_end_threshold"),
                    format!("{limit}\n"),
                )
                .expect("limit");
            }
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn charge_limit_uses_only_an_exact_battery_device() {
        let fixture = Fixture::new();
        fixture.supply("AC", "Mains", Some(100));
        let battery = fixture.supply("BAT0", "Battery", Some(80));
        let provider = PowerSupplySysfsProvider::new(&fixture.0);

        let control = provider
            .charge_limit_control()
            .expect("probe")
            .expect("battery control");
        assert_eq!(control.read_limit().expect("read").0, 80);
        assert_eq!(
            control.set_limit(BatteryLimitPercent(75)).expect("write").0,
            75
        );
        assert_eq!(
            fs::read_to_string(battery.join("charge_control_end_threshold"))
                .expect("read fixture")
                .trim(),
            "75"
        );
    }

    #[test]
    fn charge_limit_bounds_are_shared_with_asusd() {
        let fixture = Fixture::new();
        fixture.supply("BAT0", "Battery", Some(80));
        let control = PowerSupplySysfsProvider::new(&fixture.0)
            .charge_limit_control()
            .expect("probe")
            .expect("control");

        assert!(matches!(
            control.set_limit(BatteryLimitPercent(19)),
            Err(RogError::InvalidInput(_))
        ));
        assert!(matches!(
            control.set_limit(BatteryLimitPercent(101)),
            Err(RogError::InvalidInput(_))
        ));
    }

    #[test]
    fn ambiguous_multi_battery_writes_are_rejected() {
        let fixture = Fixture::new();
        fixture.supply("BAT0", "Battery", Some(80));
        fixture.supply("BAT1", "Battery", Some(80));

        assert!(matches!(
            PowerSupplySysfsProvider::new(&fixture.0).charge_limit_control(),
            Err(RogError::NotSupported(_))
        ));
    }

    #[test]
    fn cached_charge_limit_rejects_later_battery_hotplug() {
        let fixture = Fixture::new();
        fixture.supply("BAT0", "Battery", Some(80));
        let control = PowerSupplySysfsProvider::new(&fixture.0)
            .charge_limit_control()
            .expect("probe")
            .expect("control");
        fixture.supply("BAT1", "Battery", Some(80));

        assert!(matches!(
            control.set_limit(BatteryLimitPercent(75)),
            Err(RogError::NotSupported(_))
        ));
    }

    #[test]
    fn telemetry_battery_without_standard_control_remains_read_only() {
        let fixture = Fixture::new();
        fixture.supply("BAT0", "Battery", None);

        assert!(PowerSupplySysfsProvider::new(&fixture.0)
            .charge_limit_control()
            .expect("probe")
            .is_none());
    }

    #[cfg(unix)]
    #[test]
    fn charge_limit_rejects_an_attribute_symlink_escape() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new();
        let battery = fixture.supply("BAT0", "Battery", None);
        let outside = fixture.0.join("outside");
        fs::write(&outside, "80\n").expect("outside file");
        symlink(&outside, battery.join("charge_control_end_threshold")).expect("symlink");

        assert!(matches!(
            PowerSupplySysfsProvider::new(&fixture.0).charge_limit_control(),
            Err(RogError::NotSupported(_))
        ));
    }
}
