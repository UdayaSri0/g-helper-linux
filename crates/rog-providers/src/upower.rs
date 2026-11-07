use std::{
    collections::HashMap,
    fs,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rog_core::{BatteryState, PowerSource, RogError, RogResult};
use tracing::debug;
use zvariant::{OwnedObjectPath, OwnedValue};

pub const BATTERY_SOURCE_MAX_AGE: Duration = Duration::from_secs(60);
const UPOWER_READ_TIMEOUT: Duration = Duration::from_secs(3);
const UPOWER_DEVICE_INTERFACE: &str = "org.freedesktop.UPower.Device";
const UPOWER_SERVICE: &str = "org.freedesktop.UPower";

#[derive(Debug)]
pub struct UPowerProvider {
    conn: zbus::Connection,
}

#[derive(Debug, Clone, Copy)]
pub struct UPowerStatus {
    pub power_source: Option<PowerSource>,
    pub battery_percent: Option<f32>,
    pub ac_online: Option<bool>,
    pub battery_state: Option<BatteryState>,
    pub battery_health_percent: Option<f32>,
    pub battery_cycle_count: Option<u32>,
    pub battery_charge_power_w: Option<f32>,
    pub battery_discharge_power_w: Option<f32>,
    pub battery_time_to_empty_s: Option<u64>,
    pub battery_time_to_full_s: Option<u64>,
    pub timestamp_ms: u64,
    pub battery_source_sample: BatterySourceSample,
}

#[derive(Debug, Clone, Copy)]
pub struct BatterySourceSample {
    pub source: PowerSource,
    pub update_time_secs: u64,
    pub age_at_sample_ms: u64,
    pub sampled_at_boottime_ms: u64,
}

impl BatterySourceSample {
    pub fn age_ms_at(&self, boottime_ms: u64) -> Option<u64> {
        battery_sample_age_ms(
            self.age_at_sample_ms,
            self.sampled_at_boottime_ms,
            boottime_ms,
        )
    }

    pub fn is_fresh_at(&self, boottime_ms: u64) -> bool {
        self.age_ms_at(boottime_ms)
            .is_some_and(|age| age <= BATTERY_SOURCE_MAX_AGE.as_millis() as u64)
    }
}

#[derive(Debug, Clone)]
struct BatteryDeviceProperties {
    native_path: String,
    device_type: u32,
    power_supply: bool,
    is_present: bool,
    update_time_secs: u64,
    percentage: f64,
    state: u32,
    energy: Option<f64>,
    energy_empty: Option<f64>,
    energy_full: Option<f64>,
    energy_full_design: Option<f64>,
    energy_rate: Option<f64>,
    time_to_empty: Option<i64>,
    time_to_full: Option<i64>,
    cycle_count: Option<u32>,
}

#[derive(Debug, Clone)]
struct ValidatedBatteryDevice {
    properties: BatteryDeviceProperties,
    source: PowerSource,
    age_at_sample_ms: u64,
    sampled_at_boottime_ms: u64,
}

impl UPowerProvider {
    pub async fn connect_system() -> RogResult<Self> {
        let conn = zbus::Connection::system()
            .await
            .map_err(|e| RogError::DependencyMissing(format!("system dbus unavailable: {e}")))?;
        Ok(Self { conn })
    }

    pub async fn read_status(&self) -> RogResult<UPowerStatus> {
        tokio::time::timeout(UPOWER_READ_TIMEOUT, self.read_status_inner())
            .await
            .map_err(|_| RogError::DependencyMissing("UPower read timed out".into()))?
    }

    async fn read_status_inner(&self) -> RogResult<UPowerStatus> {
        let ts = now_ms();

        let proxy = zbus::Proxy::new(
            &self.conn,
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower",
            "org.freedesktop.UPower",
        )
        .await
        .map_err(|e| RogError::DependencyMissing(format!("UPower not available: {e}")))?;

        let on_battery: bool = proxy
            .get_property("OnBattery")
            .await
            .map_err(|e| RogError::Unexpected(format!("UPower OnBattery read failed: {e}")))?;
        let paths: Vec<OwnedObjectPath> = proxy
            .call("EnumerateDevices", &())
            .await
            .map_err(|e| RogError::Unexpected(format!("UPower EnumerateDevices failed: {e}")))?;
        debug!("UPower devices: {}", paths.len());

        let sysfs_root = Path::new("/sys/class/power_supply");
        let physical_batteries = physical_battery_names(sysfs_root)?;
        if physical_batteries.len() != 1 {
            return Err(RogError::Unexpected(format!(
                "expected one physical battery, found {}",
                physical_batteries.len()
            )));
        }
        let battery_path = format!(
            "/org/freedesktop/UPower/devices/battery_{}",
            physical_batteries[0]
        );
        let mut battery = None;
        let mut line_power_online = None;
        for path in paths {
            let object_path = path.as_str();
            if object_path == battery_path {
                battery = Some(parse_battery_properties(
                    get_device_properties(&self.conn, &path).await?,
                )?);
            } else if object_path
                .rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with("line_power_"))
            {
                let properties = get_device_properties(&self.conn, &path).await?;
                if property::<u32>(&properties, "Type")? == 1 {
                    let online = property::<bool>(&properties, "Online")?;
                    if line_power_online.is_some_and(|previous| previous != online) {
                        return Err(RogError::Unexpected(
                            "UPower line-power devices disagree about online state".into(),
                        ));
                    }
                    line_power_online = Some(online);
                }
            }
        }

        let battery = battery.ok_or_else(|| {
            RogError::Unexpected("UPower object for the physical battery is unavailable".into())
        })?;
        select_physical_battery(std::slice::from_ref(&battery), sysfs_root)?;
        let ac_online = line_power_online;
        let source = validated_power_source(on_battery, ac_online).ok_or_else(|| {
            RogError::Unexpected(
                "UPower and line-power source readings disagree or are unavailable".into(),
            )
        })?;
        let wall_now_ms = now_epoch_ms()?;
        let sampled_at_boottime_ms = clock_boottime_ms()?;
        let validated = validate_battery_device(
            &battery,
            sysfs_root,
            wall_now_ms,
            sampled_at_boottime_ms,
            source,
        )?;
        let details = battery_details(&validated.properties);
        let battery_source_sample = BatterySourceSample {
            source: validated.source,
            update_time_secs: validated.properties.update_time_secs,
            age_at_sample_ms: validated.age_at_sample_ms,
            sampled_at_boottime_ms: validated.sampled_at_boottime_ms,
        };
        if !battery_source_sample.is_fresh_at(clock_boottime_ms()?) {
            return Err(RogError::Unexpected(
                "battery source sample became stale during UPower read".into(),
            ));
        }

        Ok(UPowerStatus {
            power_source: Some(source),
            battery_percent: details.battery_percent,
            ac_online,
            battery_state: details.battery_state,
            battery_health_percent: details.battery_health_percent,
            battery_cycle_count: details.battery_cycle_count,
            battery_charge_power_w: details.battery_charge_power_w,
            battery_discharge_power_w: details.battery_discharge_power_w,
            battery_time_to_empty_s: details.battery_time_to_empty_s,
            battery_time_to_full_s: details.battery_time_to_full_s,
            timestamp_ms: ts,
            battery_source_sample,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct BatteryDetails {
    battery_percent: Option<f32>,
    battery_state: Option<BatteryState>,
    battery_health_percent: Option<f32>,
    battery_cycle_count: Option<u32>,
    battery_charge_power_w: Option<f32>,
    battery_discharge_power_w: Option<f32>,
    battery_time_to_empty_s: Option<u64>,
    battery_time_to_full_s: Option<u64>,
}

fn battery_details(properties: &BatteryDeviceProperties) -> BatteryDetails {
    let battery_percent = Some(properties.percentage as f32);
    let battery_state = Some(battery_state_from_upower(properties.state));
    let energy_rate_w = properties
        .energy_rate
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value as f32);
    let battery_time_to_empty_s = positive_time(properties.time_to_empty);
    let battery_time_to_full_s = positive_time(properties.time_to_full);
    let energy_full = properties.energy_full;
    let energy_full_design = properties.energy_full_design;

    let battery_health_percent = match (energy_full, energy_full_design) {
        (Some(full), Some(design)) if design > 0.0 => {
            let pct = (full / design) * 100.0;
            pct.is_finite().then_some(pct as f32)
        }
        _ => None,
    };

    // Newer UPower exposes ChargeCycles (uint32). Some implementations may use CycleCount.
    let battery_cycle_count = properties.cycle_count;

    let mut battery_charge_power_w = None;
    let mut battery_discharge_power_w = None;
    if let (Some(st), Some(rate)) = (battery_state, energy_rate_w) {
        match st {
            BatteryState::Charging | BatteryState::PendingCharge => {
                battery_charge_power_w = Some(rate);
            }
            BatteryState::Discharging | BatteryState::PendingDischarge => {
                battery_discharge_power_w = Some(rate);
            }
            _ => {}
        }
    }

    BatteryDetails {
        battery_percent,
        battery_state,
        battery_health_percent,
        battery_cycle_count,
        battery_charge_power_w,
        battery_discharge_power_w,
        battery_time_to_empty_s,
        battery_time_to_full_s,
    }
}

async fn get_device_properties(
    conn: &zbus::Connection,
    path: &OwnedObjectPath,
) -> RogResult<HashMap<String, OwnedValue>> {
    let proxy = zbus::fdo::PropertiesProxy::builder(conn)
        .destination(UPOWER_SERVICE)
        .map_err(|e| {
            RogError::Unexpected(format!("UPower properties proxy destination failed: {e}"))
        })?
        .path(path.as_str())
        .map_err(|e| RogError::Unexpected(format!("UPower properties proxy path failed: {e}")))?
        .build()
        .await
        .map_err(|e| RogError::Unexpected(format!("UPower properties proxy build failed: {e}")))?;
    let interface = UPOWER_DEVICE_INTERFACE
        .try_into()
        .map_err(|e| RogError::Unexpected(format!("UPower interface name is invalid: {e}")))?;
    proxy
        .get_all(interface)
        .await
        .map_err(|e| RogError::Unexpected(format!("UPower GetAll failed: {e}")))
}

fn property<T>(properties: &HashMap<String, OwnedValue>, name: &str) -> RogResult<T>
where
    T: TryFrom<OwnedValue>,
    <T as TryFrom<OwnedValue>>::Error: std::fmt::Display,
{
    properties
        .get(name)
        .cloned()
        .ok_or_else(|| RogError::Unexpected(format!("UPower property {name} is missing")))?
        .try_into()
        .map_err(|e: <T as TryFrom<OwnedValue>>::Error| {
            RogError::Unexpected(format!("UPower property {name} has an invalid type: {e}"))
        })
}

fn optional_property<T>(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    properties.get(name)?.clone().try_into().ok()
}

fn parse_battery_properties(
    properties: HashMap<String, OwnedValue>,
) -> RogResult<BatteryDeviceProperties> {
    Ok(BatteryDeviceProperties {
        native_path: property(&properties, "NativePath")?,
        device_type: property(&properties, "Type")?,
        power_supply: property(&properties, "PowerSupply")?,
        is_present: property(&properties, "IsPresent")?,
        update_time_secs: property(&properties, "UpdateTime")?,
        percentage: property(&properties, "Percentage")?,
        state: property(&properties, "State")?,
        energy: optional_property(&properties, "Energy"),
        energy_empty: optional_property(&properties, "EnergyEmpty"),
        energy_full: optional_property(&properties, "EnergyFull"),
        energy_full_design: optional_property(&properties, "EnergyFullDesign"),
        energy_rate: optional_property(&properties, "EnergyRate"),
        time_to_empty: optional_time_property(&properties, "TimeToEmpty"),
        time_to_full: optional_time_property(&properties, "TimeToFull"),
        cycle_count: optional_property(&properties, "ChargeCycles")
            .or_else(|| optional_property(&properties, "CycleCount")),
    })
}

fn optional_time_property(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<i64> {
    optional_property::<i64>(properties, name).or_else(|| {
        optional_property::<u64>(properties, name).and_then(|value| i64::try_from(value).ok())
    })
}

fn physical_battery_names(sysfs_root: &Path) -> RogResult<Vec<String>> {
    let entries = fs::read_dir(sysfs_root)
        .map_err(|e| RogError::Unexpected(format!("power-supply devices unavailable: {e}")))?;
    let mut batteries = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|e| RogError::Unexpected(format!("power-supply entry unavailable: {e}")))?;
        let kind = fs::read_to_string(entry.path().join("type"))
            .map_err(|e| RogError::Unexpected(format!("power-supply type unavailable: {e}")))?;
        if kind.trim() == "Battery" {
            batteries.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(batteries)
}

fn select_physical_battery<'a>(
    devices: &'a [BatteryDeviceProperties],
    sysfs_root: &Path,
) -> RogResult<&'a BatteryDeviceProperties> {
    let sysfs_batteries = physical_battery_names(sysfs_root)?;
    if sysfs_batteries.len() != 1 {
        return Err(RogError::Unexpected(format!(
            "expected one physical battery, found {}",
            sysfs_batteries.len()
        )));
    }
    let name = &sysfs_batteries[0];
    let matching: Vec<_> = devices
        .iter()
        .filter(|device| device.native_path == *name)
        .collect();
    if matching.len() != 1 {
        return Err(RogError::Unexpected(format!(
            "expected one UPower object for physical battery {name}, found {}",
            matching.len()
        )));
    }
    let device = matching[0];
    if device.device_type != 2 || !device.power_supply || !device.is_present {
        return Err(RogError::Unexpected(
            "UPower physical battery identity is absent or inconsistent".into(),
        ));
    }
    Ok(device)
}

fn validated_power_source(on_battery: bool, ac_online: Option<bool>) -> Option<PowerSource> {
    match (on_battery, ac_online) {
        (true, Some(false)) => Some(PowerSource::Battery),
        (false, Some(true)) => Some(PowerSource::Ac),
        _ => None,
    }
}

fn validated_update_age_ms(update_time_secs: u64, wall_now_ms: u64) -> Option<u64> {
    if update_time_secs == 0 {
        return None;
    }
    let update_time_ms = update_time_secs.checked_mul(1_000)?;
    let age_ms = wall_now_ms.checked_sub(update_time_ms)?;
    (age_ms <= BATTERY_SOURCE_MAX_AGE.as_millis() as u64).then_some(age_ms)
}

#[cfg(test)]
fn validated_update_age_secs(update_time_secs: u64, wall_now_secs: u64) -> Option<u64> {
    validated_update_age_ms(update_time_secs, wall_now_secs.checked_mul(1_000)?)
        .map(|age| age / 1_000)
}

fn battery_sample_age_ms(
    age_at_sample_ms: u64,
    sampled_at_boottime_ms: u64,
    now_boottime_ms: u64,
) -> Option<u64> {
    age_at_sample_ms.checked_add(now_boottime_ms.checked_sub(sampled_at_boottime_ms)?)
}

fn validate_battery_device(
    properties: &BatteryDeviceProperties,
    sysfs_root: &Path,
    wall_now_ms: u64,
    sampled_at_boottime_ms: u64,
    source: PowerSource,
) -> RogResult<ValidatedBatteryDevice> {
    if select_physical_battery(std::slice::from_ref(properties), sysfs_root).is_err() {
        return Err(RogError::Unexpected(
            "battery is not one unambiguous, present physical power-supply device".into(),
        ));
    }
    let age_at_sample_ms = validated_update_age_ms(properties.update_time_secs, wall_now_ms)
        .ok_or_else(|| {
            RogError::Unexpected("battery UpdateTime is missing, future-dated, or stale".into())
        })?;
    if !properties.percentage.is_finite() || !(0.0..=100.0).contains(&properties.percentage) {
        return Err(RogError::Unexpected("battery Percentage is invalid".into()));
    }
    if properties.state == 0 || properties.state > 6 {
        return Err(RogError::Unexpected(
            "battery State is unknown or invalid".into(),
        ));
    }
    if properties.percentage == 0.0 && properties.state != 3 {
        return Err(RogError::Unexpected(
            "battery Percentage is 0 but State is not Empty".into(),
        ));
    }
    let (Some(energy), Some(energy_full)) = (properties.energy, properties.energy_full) else {
        return Err(RogError::Unexpected(
            "battery Energy and EnergyFull are required to validate Percentage".into(),
        ));
    };
    if !energy.is_finite()
        || !energy_full.is_finite()
        || energy < 0.0
        || energy_full <= 0.0
        || energy > energy_full
        || properties.energy_empty.is_some_and(|empty| {
            !empty.is_finite() || empty < 0.0 || empty >= energy_full || energy < empty
        })
    {
        return Err(RogError::Unexpected(
            "battery energy values are inconsistent".into(),
        ));
    }
    Ok(ValidatedBatteryDevice {
        properties: properties.clone(),
        source,
        age_at_sample_ms,
        sampled_at_boottime_ms,
    })
}

fn positive_time(value: Option<i64>) -> Option<u64> {
    value.filter(|value| *value > 0).map(|value| value as u64)
}

fn now_epoch_ms() -> RogResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .map_err(|e| RogError::Unexpected(format!("system clock is before Unix epoch: {e}")))
}

pub fn clock_boottime_ms() -> RogResult<u64> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // CLOCK_BOOTTIME includes time spent suspended, so cached samples age across resume.
    let result = unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut time) };
    if result != 0 || time.tv_sec < 0 || time.tv_nsec < 0 {
        return Err(RogError::Unexpected("CLOCK_BOOTTIME read failed".into()));
    }
    (time.tv_sec as u64)
        .checked_mul(1_000)
        .and_then(|millis| millis.checked_add(time.tv_nsec as u64 / 1_000_000))
        .ok_or_else(|| RogError::Unexpected("CLOCK_BOOTTIME value overflowed".into()))
}

pub fn clock_monotonic_ms() -> RogResult<u64> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let result = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) };
    if result != 0 || time.tv_sec < 0 || time.tv_nsec < 0 {
        return Err(RogError::Unexpected("CLOCK_MONOTONIC read failed".into()));
    }
    (time.tv_sec as u64)
        .checked_mul(1_000)
        .and_then(|millis| millis.checked_add(time.tv_nsec as u64 / 1_000_000))
        .ok_or_else(|| RogError::Unexpected("CLOCK_MONOTONIC value overflowed".into()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn battery_state_from_upower(code: u32) -> BatteryState {
    match code {
        1 => BatteryState::Charging,
        2 => BatteryState::Discharging,
        3 => BatteryState::Empty,
        4 => BatteryState::Full,
        5 => BatteryState::PendingCharge,
        6 => BatteryState::PendingDischarge,
        _ => BatteryState::Unknown,
    }
}

#[cfg(test)]
mod battery_source_tests {
    use super::*;
    use std::fs;

    fn device() -> BatteryDeviceProperties {
        BatteryDeviceProperties {
            native_path: "BAT0".into(),
            device_type: 2,
            power_supply: true,
            is_present: true,
            update_time_secs: 940,
            percentage: 60.0,
            state: 2,
            energy: Some(60.0),
            energy_empty: Some(0.0),
            energy_full: Some(100.0),
            energy_full_design: None,
            energy_rate: None,
            time_to_empty: None,
            time_to_full: None,
            cycle_count: None,
        }
    }

    fn sysfs_battery(root: &std::path::Path) {
        fs::create_dir_all(root.join("BAT0")).unwrap();
        fs::write(root.join("BAT0/type"), "Battery\n").unwrap();
    }

    struct TestDir(std::path::PathBuf);

    impl TestDir {
        fn new() -> Self {
            static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "rog-helper-upower-test-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn validates_update_time_and_uses_boottime_for_ageing() {
        assert_eq!(validated_update_age_secs(940, 1_000), Some(60));
        assert_eq!(validated_update_age_secs(939, 1_000), None);
        assert_eq!(validated_update_age_secs(0, 1_000), None);
        assert_eq!(validated_update_age_secs(1_001, 1_000), None);
        assert_eq!(battery_sample_age_ms(60_000, 10_000, 10_000), Some(60_000));
        assert_eq!(battery_sample_age_ms(60_000, 10_000, 20_000), Some(70_000));
        assert_eq!(battery_sample_age_ms(60_000, 20_000, 10_000), None);
        let sample = BatterySourceSample {
            source: PowerSource::Battery,
            update_time_secs: 940,
            age_at_sample_ms: 60_000,
            sampled_at_boottime_ms: 10_000,
        };
        assert!(sample.is_fresh_at(10_000));
        assert!(!sample.is_fresh_at(10_001));
    }

    #[test]
    fn battery_sample_requires_physical_present_device_and_plausible_values() {
        let dir = TestDir::new();
        let sysfs = dir.0.join("power_supply");
        sysfs_battery(&sysfs);
        assert!(validate_battery_device(
            &device(),
            &sysfs,
            1_000_000,
            10_000,
            PowerSource::Battery
        )
        .is_ok());

        let mut missing_energy = device();
        missing_energy.energy = None;
        assert!(validate_battery_device(
            &missing_energy,
            &sysfs,
            1_000_000,
            10_000,
            PowerSource::Battery
        )
        .is_err());

        let mut zero_not_empty = device();
        zero_not_empty.percentage = 0.0;
        assert!(validate_battery_device(
            &zero_not_empty,
            &sysfs,
            1_000_000,
            10_000,
            PowerSource::Battery
        )
        .is_err());

        let mut inconsistent_empty = device();
        inconsistent_empty.energy_empty = Some(70.0);
        assert!(validate_battery_device(
            &inconsistent_empty,
            &sysfs,
            1_000_000,
            10_000,
            PowerSource::Battery
        )
        .is_err());

        let mut empty = zero_not_empty;
        empty.state = 3;
        empty.energy = Some(0.0);
        assert!(
            validate_battery_device(&empty, &sysfs, 1_000_000, 10_000, PowerSource::Battery)
                .is_ok()
        );

        let mut not_present = device();
        not_present.is_present = false;
        assert!(validate_battery_device(
            &not_present,
            &sysfs,
            1_000_000,
            10_000,
            PowerSource::Battery
        )
        .is_err());
    }

    #[test]
    fn rejects_ambiguous_battery_source_and_conflicting_power_source() {
        let dir = TestDir::new();
        let sysfs = dir.0.join("power_supply");
        sysfs_battery(&sysfs);
        assert!(select_physical_battery(&[device()], &sysfs).is_ok());
        assert!(select_physical_battery(&[], &sysfs).is_err());
        assert!(select_physical_battery(&[device(), device()], &sysfs).is_err());
        assert_eq!(
            validated_power_source(true, Some(false)),
            Some(PowerSource::Battery)
        );
        assert_eq!(
            validated_power_source(false, Some(true)),
            Some(PowerSource::Ac)
        );
        assert_eq!(validated_power_source(true, Some(true)), None);
        assert_eq!(validated_power_source(false, Some(false)), None);
        assert_eq!(validated_power_source(true, None), None);
    }
}
