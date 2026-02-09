use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{RogError, RogResult, ValidationWarning};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerSource {
    Ac,
    Battery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatteryState {
    Unknown,
    Charging,
    Discharging,
    Empty,
    Full,
    PendingCharge,
    PendingDischarge,
    NotCharging,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerformanceProfile {
    Silent,
    Balanced,
    Turbo,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuMode {
    Integrated,
    Hybrid,
    Dedicated,
    Other(String),
}

pub const DEFAULT_BATTERY_LIMIT_MIN_PERCENT: u8 = 40;
pub const DEFAULT_BATTERY_LIMIT_MAX_PERCENT: u8 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatteryLimitPercent(pub u8);

impl BatteryLimitPercent {
    pub fn clamp(value: u8, min: u8, max: u8) -> Self {
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        Self(value.clamp(min, max))
    }

    pub fn clamp_default(value: u8) -> Self {
        Self::clamp(
            value,
            DEFAULT_BATTERY_LIMIT_MIN_PERCENT,
            DEFAULT_BATTERY_LIMIT_MAX_PERCENT,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FanDomain {
    Cpu,
    Gpu,
    Mid,
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanPoint {
    pub temp_c: u8,
    pub duty_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanCurve {
    pub domain: FanDomain,
    pub points: Vec<FanPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeFloor {
    pub temp_c: u8,
    pub min_duty_percent: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanCurvePolicy {
    pub temp_min_c: u8,
    pub temp_max_c: u8,
    pub duty_min_percent: u8,
    pub duty_max_percent: u8,
    pub enforce_monotonic: bool,
    pub safe_floor: Option<SafeFloor>,
}

impl Default for FanCurvePolicy {
    fn default() -> Self {
        Self {
            temp_min_c: 20,
            temp_max_c: 100,
            duty_min_percent: 0,
            duty_max_percent: 100,
            enforce_monotonic: true,
            // Safety-first: avoid 0% fan at higher temps.
            safe_floor: Some(SafeFloor {
                temp_c: 70,
                min_duty_percent: 20,
            }),
        }
    }
}

impl FanCurve {
    /// Validate + sanitize in-place (clamps, sorts, monotonic fix, safe floor).
    ///
    /// Returns warnings describing any corrections.
    pub fn sanitize(&mut self, policy: FanCurvePolicy) -> RogResult<Vec<ValidationWarning>> {
        if self.points.is_empty() {
            return Err(RogError::InvalidInput(
                "fan curve must have at least 1 point".to_string(),
            ));
        }

        let mut warnings = Vec::new();

        // Clamp.
        for p in &mut self.points {
            let orig = *p;
            p.temp_c = p.temp_c.clamp(policy.temp_min_c, policy.temp_max_c);
            p.duty_percent = p
                .duty_percent
                .clamp(policy.duty_min_percent, policy.duty_max_percent);
            if *p != orig {
                warnings.push(ValidationWarning::new(
                    "clamp",
                    format!(
                        "Clamped point from ({}C, {}%) to ({}C, {}%).",
                        orig.temp_c, orig.duty_percent, p.temp_c, p.duty_percent
                    ),
                ));
            }
        }

        // Sort by temperature.
        let mut sorted = self.points.clone();
        sorted.sort_by_key(|p| p.temp_c);
        if sorted != self.points {
            self.points = sorted;
            warnings.push(ValidationWarning::new(
                "sort",
                "Sorted fan curve points by temperature.",
            ));
        }

        // Deduplicate identical temps (keep the highest duty).
        let mut deduped: Vec<FanPoint> = Vec::with_capacity(self.points.len());
        for p in self.points.drain(..) {
            if let Some(last) = deduped.last_mut() {
                if last.temp_c == p.temp_c {
                    if p.duty_percent > last.duty_percent {
                        last.duty_percent = p.duty_percent;
                    }
                    warnings.push(ValidationWarning::new(
                        "dedup",
                        format!("Merged duplicate temperature point at {}C.", p.temp_c),
                    ));
                    continue;
                }
            }
            deduped.push(p);
        }
        self.points = deduped;

        // Safe floor.
        if let Some(floor) = policy.safe_floor {
            for p in &mut self.points {
                if p.temp_c >= floor.temp_c && p.duty_percent < floor.min_duty_percent {
                    let old = p.duty_percent;
                    p.duty_percent = floor.min_duty_percent;
                    warnings.push(ValidationWarning::new(
                        "safe_floor",
                        format!(
                            "Raised duty at {}C from {}% to safe floor {}%.",
                            p.temp_c, old, floor.min_duty_percent
                        ),
                    ));
                }
            }
        }

        // Monotonic duty.
        if policy.enforce_monotonic && self.points.len() >= 2 {
            let mut last = self.points[0].duty_percent;
            for p in &mut self.points[1..] {
                if p.duty_percent < last {
                    let old = p.duty_percent;
                    p.duty_percent = last;
                    warnings.push(ValidationWarning::new(
                        "monotonic",
                        format!(
                            "Adjusted duty at {}C from {}% to {}% to keep monotonic curve.",
                            p.temp_c, old, p.duty_percent
                        ),
                    ));
                }
                last = p.duty_percent;
            }
        }

        Ok(warnings)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightingMode {
    Off,
    Static,
    Breathing,
    Rainbow,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightingState {
    pub brightness: u8,
    pub mode: LightingMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCaps {
    pub has_profiles: bool,
    pub has_fan_curves: bool,
    pub has_fan_reading: bool,
    pub has_charge_limit: bool,
    pub has_gpu_modes: bool,
    pub has_aura: bool,
    pub has_kbd_backlight: bool,
    pub requires_reboot_for_gpu_switch: bool,

    /// Raw discovery info for diagnostics (bus names, interfaces, sysfs hints, etc.).
    pub endpoints: Vec<String>,
    pub notes: Vec<String>,
}

impl DeviceCaps {
    pub fn unknown() -> Self {
        Self {
            has_profiles: false,
            has_fan_curves: false,
            has_fan_reading: false,
            has_charge_limit: false,
            has_gpu_modes: false,
            has_aura: false,
            has_kbd_backlight: false,
            requires_reboot_for_gpu_switch: false,
            endpoints: Vec::new(),
            notes: vec!["Capabilities not probed yet.".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
    pub timestamp_ms: u64,

    pub cpu_temp_c: Option<f32>,
    pub gpu_temp_c: Option<f32>,
    pub temps_c: BTreeMap<String, f32>,

    pub fans_rpm: BTreeMap<String, u32>,

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
}

impl TelemetrySnapshot {
    pub fn empty_now(timestamp_ms: u64) -> Self {
        Self {
            timestamp_ms,
            cpu_temp_c: None,
            gpu_temp_c: None,
            temps_c: BTreeMap::new(),
            fans_rpm: BTreeMap::new(),
            power_source: None,
            battery_percent: None,
            ac_online: None,
            battery_state: None,
            battery_health_percent: None,
            battery_cycle_count: None,
            battery_charge_power_w: None,
            battery_discharge_power_w: None,
            battery_time_to_empty_s: None,
            battery_time_to_full_s: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub caps: DeviceCaps,
    pub telemetry: TelemetrySnapshot,
    pub warnings: Vec<String>,
}

impl AppState {
    pub fn new(caps: DeviceCaps, telemetry: TelemetrySnapshot) -> Self {
        Self {
            caps,
            telemetry,
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fan_curve_sanitize_sorts_and_enforces_monotonic() {
        let mut curve = FanCurve {
            domain: FanDomain::Cpu,
            points: vec![
                FanPoint {
                    temp_c: 80,
                    duty_percent: 10,
                },
                FanPoint {
                    temp_c: 60,
                    duty_percent: 30,
                },
                FanPoint {
                    temp_c: 90,
                    duty_percent: 20,
                },
            ],
        };

        let warnings = curve.sanitize(FanCurvePolicy::default()).unwrap();
        assert!(!warnings.is_empty());

        assert_eq!(curve.points[0].temp_c, 60);
        assert_eq!(curve.points[1].temp_c, 80);
        assert_eq!(curve.points[2].temp_c, 90);

        // Safe floor should raise 80C/90C to >=20%.
        assert!(curve.points[1].duty_percent >= 20);
        assert!(curve.points[2].duty_percent >= curve.points[1].duty_percent);
    }

    #[test]
    fn fan_curve_sanitize_clamps() {
        let mut curve = FanCurve {
            domain: FanDomain::Cpu,
            points: vec![FanPoint {
                temp_c: 5,
                duty_percent: 250,
            }],
        };
        let _warnings = curve.sanitize(FanCurvePolicy::default()).unwrap();
        assert_eq!(curve.points[0].temp_c, 20);
        assert_eq!(curve.points[0].duty_percent, 100);
    }

    #[test]
    fn battery_limit_clamps_default() {
        assert_eq!(BatteryLimitPercent::clamp_default(10).0, 40);
        assert_eq!(BatteryLimitPercent::clamp_default(80).0, 80);
        assert_eq!(BatteryLimitPercent::clamp_default(150).0, 100);
    }
}
