//! Path-free CPU control requests and validation shared by both daemon routes.

use crate::{CpuTelemetry, RogError, RogResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpuControlRequest {
    Turbo(bool),
    PowerMode(CpuPowerMode),
    Governor(String),
    Epp(String),
    FrequencyLimits {
        min_mhz: Option<u32>,
        max_mhz: Option<u32>,
    },
    CoreOnline {
        core_id: u32,
        online: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuPowerMode {
    Quiet,
    Balanced,
    Performance,
}

impl CpuPowerMode {
    pub fn parse(value: &str) -> RogResult<Self> {
        match normalize_cpu_token(value).as_str() {
            "quiet" | "silent" => Ok(Self::Quiet),
            "balanced" => Ok(Self::Balanced),
            "performance" | "turbo" => Ok(Self::Performance),
            _ => Err(RogError::InvalidInput(
                "CPU power mode must be Quiet, Balanced, or Performance".to_string(),
            )),
        }
    }

    pub const fn governor_preferences(self) -> &'static [&'static str] {
        match self {
            Self::Quiet => &["powersave", "schedutil", "ondemand"],
            Self::Balanced => &["schedutil", "ondemand", "powersave"],
            Self::Performance => &["performance", "schedutil"],
        }
    }

    pub const fn epp_preferences(self) -> &'static [&'static str] {
        match self {
            Self::Quiet => &["power", "balance_power", "balanced"],
            Self::Balanced => &["balanced", "balance_performance", "balance_power"],
            Self::Performance => &["performance", "balance_performance"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuFrequencyBounds {
    pub min_mhz: u32,
    pub max_mhz: u32,
}

impl CpuFrequencyBounds {
    pub fn new(min_mhz: u32, max_mhz: u32) -> RogResult<Self> {
        if min_mhz == 0 || max_mhz == 0 || min_mhz > max_mhz {
            return Err(RogError::TemporarilyUnavailable(
                "CPU hardware frequency bounds are unavailable".to_string(),
            ));
        }
        Ok(Self { min_mhz, max_mhz })
    }
}

pub fn validate_cpu_choice(value: &str, allowed: &[String], label: &str) -> RogResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(RogError::InvalidInput(format!("{label} cannot be empty")));
    }
    if allowed.is_empty() {
        return Err(RogError::NotSupported(format!(
            "no supported {label} choices were detected"
        )));
    }
    allowed
        .iter()
        .find(|candidate| candidate.as_str() == value)
        .cloned()
        .ok_or_else(|| {
            RogError::InvalidInput(format!(
                "{label} is not in the detected hardware allow-list"
            ))
        })
}

pub fn validate_cpu_frequency_limits(
    min_mhz: Option<u32>,
    max_mhz: Option<u32>,
    bounds: CpuFrequencyBounds,
) -> RogResult<(Option<u32>, Option<u32>)> {
    if min_mhz.is_none() && max_mhz.is_none() {
        return Err(RogError::InvalidInput(
            "at least one CPU frequency limit must be provided".to_string(),
        ));
    }
    if let Some(minimum) = min_mhz {
        if !(bounds.min_mhz..=bounds.max_mhz).contains(&minimum) {
            return Err(RogError::InvalidInput(
                "minimum CPU frequency is outside detected hardware bounds".to_string(),
            ));
        }
    }
    if let Some(maximum) = max_mhz {
        if !(bounds.min_mhz..=bounds.max_mhz).contains(&maximum) {
            return Err(RogError::InvalidInput(
                "maximum CPU frequency is outside detected hardware bounds".to_string(),
            ));
        }
    }
    if let (Some(minimum), Some(maximum)) = (min_mhz, max_mhz) {
        if minimum > maximum {
            return Err(RogError::InvalidInput(
                "minimum CPU frequency cannot exceed maximum CPU frequency".to_string(),
            ));
        }
    }
    Ok((min_mhz, max_mhz))
}

pub fn validate_cpu_core_request(
    core_id: u32,
    online: bool,
    detected_core_ids: &[u32],
) -> RogResult<()> {
    if !detected_core_ids.contains(&core_id) {
        return Err(RogError::InvalidInput(
            "logical CPU id was not detected on this system".to_string(),
        ));
    }
    if core_id == 0 && !online {
        return Err(RogError::InvalidInput(
            "boot CPU 0 cannot be taken offline".to_string(),
        ));
    }
    Ok(())
}

pub fn cpu_request_readback_matches(
    request: &CpuControlRequest,
    telemetry: &CpuTelemetry,
) -> Option<bool> {
    match request {
        CpuControlRequest::Turbo(expected) => telemetry
            .turbo_boost_enabled
            .map(|actual| actual == *expected),
        CpuControlRequest::PowerMode(mode) => {
            let governor_matches = telemetry.governor.as_deref().map(|value| {
                mode.governor_preferences()
                    .contains(&normalize_cpu_token(value).as_str())
            });
            let epp_matches = telemetry.epp.as_deref().map(|value| {
                mode.epp_preferences()
                    .contains(&normalize_cpu_token(value).as_str())
            });
            match (governor_matches, epp_matches) {
                (Some(governor), Some(epp)) => Some(governor && epp),
                (Some(governor), None) => Some(governor),
                (None, Some(epp)) => Some(epp),
                (None, None) => None,
            }
        }
        CpuControlRequest::Governor(expected) => telemetry
            .governor
            .as_deref()
            .map(|actual| actual == expected),
        CpuControlRequest::Epp(expected) => {
            telemetry.epp.as_deref().map(|actual| actual == expected)
        }
        CpuControlRequest::FrequencyLimits { min_mhz, max_mhz } => {
            let min_matches = min_mhz.map(|expected| telemetry.min_freq_mhz == Some(expected));
            let max_matches = max_mhz.map(|expected| telemetry.max_freq_mhz == Some(expected));
            match (min_matches, max_matches) {
                (Some(minimum), Some(maximum)) => Some(minimum && maximum),
                (Some(minimum), None) => Some(minimum),
                (None, Some(maximum)) => Some(maximum),
                (None, None) => None,
            }
        }
        CpuControlRequest::CoreOnline { core_id, online } => telemetry
            .per_core
            .iter()
            .find(|core| core.logical_cpu_id == *core_id)
            .map(|core| core.online == *online),
    }
}

fn normalize_cpu_token(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn governor_allow_list_validation_is_exact() {
        let allowed = vec!["powersave".to_string(), "performance".to_string()];
        assert_eq!(
            validate_cpu_choice("performance", &allowed, "governor").unwrap(),
            "performance"
        );
        assert!(matches!(
            validate_cpu_choice("userspace", &allowed, "governor"),
            Err(RogError::InvalidInput(_))
        ));
    }

    #[test]
    fn epp_validation_rejects_values_not_reported_by_hardware() {
        let allowed = vec!["balance_power".to_string(), "performance".to_string()];
        assert!(validate_cpu_choice("balance_power", &allowed, "EPP").is_ok());
        assert!(matches!(
            validate_cpu_choice("default", &allowed, "EPP"),
            Err(RogError::InvalidInput(_))
        ));
    }

    #[test]
    fn frequency_bounds_and_inversion_are_rejected() {
        let bounds = CpuFrequencyBounds::new(400, 5200).unwrap();
        assert!(validate_cpu_frequency_limits(Some(800), Some(4200), bounds).is_ok());
        assert!(matches!(
            validate_cpu_frequency_limits(Some(200), Some(4200), bounds),
            Err(RogError::InvalidInput(_))
        ));
        assert!(matches!(
            validate_cpu_frequency_limits(Some(4200), Some(800), bounds),
            Err(RogError::InvalidInput(_))
        ));
    }

    #[test]
    fn core_validation_rejects_unknown_and_boot_cpu_offline() {
        let cores = [0, 1, 2, 3];
        assert!(validate_cpu_core_request(2, false, &cores).is_ok());
        assert!(matches!(
            validate_cpu_core_request(7, true, &cores),
            Err(RogError::InvalidInput(_))
        ));
        assert!(matches!(
            validate_cpu_core_request(0, false, &cores),
            Err(RogError::InvalidInput(_))
        ));
    }
}
