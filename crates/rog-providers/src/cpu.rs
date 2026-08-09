use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{read_dir, read_to_string, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rog_core::{
    CpuAccessState, CpuCaps, CpuControlAccess, CpuControlKind, CpuCoreTelemetry, CpuPathAccess,
    CpuTelemetry, RogError, RogResult,
};

const CPU_SYSFS_ROOT: &str = "/sys/devices/system/cpu";
const CPUFREQ_ROOT: &str = "/sys/devices/system/cpu/cpufreq";
const PROC_STAT_PATH: &str = "/proc/stat";
const INTEL_NO_TURBO_PATH: &str = "/sys/devices/system/cpu/intel_pstate/no_turbo";
const CPUFREQ_BOOST_PATH: &str = "/sys/devices/system/cpu/cpufreq/boost";
const POWERCAP_ROOT: &str = "/sys/class/powercap";

#[derive(Debug, Clone)]
pub struct CpuTelemetryProvider {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    prev_usage: Mutex<Option<UsageSnapshot>>,
    prev_package_energy: Mutex<Option<PackageEnergySample>>,
}

#[derive(Debug, Clone)]
struct PolicyData {
    id: Option<u32>,
    path: PathBuf,
    related_cpus: Vec<u32>,
    cur_freq_khz: Option<u64>,
    min_freq_khz: Option<u64>,
    max_freq_khz: Option<u64>,
    info_min_freq_khz: Option<u64>,
    info_max_freq_khz: Option<u64>,
    governor: Option<String>,
    governors_available: Vec<String>,
    epp: Option<String>,
    epp_available: Vec<String>,
    scaling_driver: Option<String>,
}

#[derive(Debug, Clone)]
struct BoostPath {
    path: PathBuf,
    // Intel no_turbo=1 means boost disabled; cpufreq boost=1 means enabled.
    inverted: bool,
}

#[derive(Debug, Clone, Copy)]
struct CpuTimes {
    total: u64,
    idle: u64,
}

#[derive(Debug, Clone)]
struct UsageSnapshot {
    overall: Option<CpuTimes>,
    per_core: BTreeMap<u32, CpuTimes>,
}

#[derive(Debug, Clone)]
struct PackageEnergySample {
    ts_ms: u64,
    energy_uj: u64,
    max_energy_range_uj: Option<u64>,
}

type CoreFreqMhz = (Option<u32>, Option<u32>, Option<u32>);

#[derive(Debug, Clone)]
struct CpuTopologyEntry {
    package_id: Option<u32>,
    raw_core_id: Option<u32>,
    thread_siblings: Vec<u32>,
}

impl Default for CpuTelemetryProvider {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner {
                prev_usage: Mutex::new(None),
                prev_package_energy: Mutex::new(None),
            }),
        }
    }
}

impl CpuTelemetryProvider {
    pub fn probe_caps(&self) -> CpuCaps {
        let policies = read_policies();
        let boost = detect_boost_path();
        let rapl = detect_package_energy_path();
        let cores = read_core_ids();
        let topology = read_cpu_topology(&cores);
        let physical_cores = count_physical_cores(&topology).unwrap_or(cores.len() as u32);

        let mut governor_choices = BTreeSet::new();
        let mut epp_choices = BTreeSet::new();
        let mut scaling_driver = None;
        let mut has_governor = false;
        let mut has_epp = false;
        let mut has_min_freq = false;
        let mut has_max_freq = false;

        for p in &policies {
            let governor_path = p.path.join("scaling_governor");
            let epp_path = p.path.join("energy_performance_preference");
            let min_path = p.path.join("scaling_min_freq");
            let max_path = p.path.join("scaling_max_freq");

            has_governor |= governor_path.exists();
            has_epp |= epp_path.exists();
            has_min_freq |= min_path.exists();
            has_max_freq |= max_path.exists();
            if scaling_driver.is_none() {
                scaling_driver = p.scaling_driver.clone();
            }

            for g in &p.governors_available {
                governor_choices.insert(g.clone());
            }
            for e in &p.epp_available {
                epp_choices.insert(e.clone());
            }
        }

        let has_core_online = cores
            .iter()
            .any(|id| PathBuf::from(format!("{CPU_SYSFS_ROOT}/cpu{id}/online")).exists());
        let governor_access = control_access_from_paths(
            CpuControlKind::Governor,
            policies
                .iter()
                .map(|p| p.path.join("scaling_governor"))
                .filter(|path| path.exists())
                .collect(),
            if policies.is_empty() {
                CpuAccessState::MissingBackend
            } else {
                CpuAccessState::Unsupported
            },
            if policies.is_empty() {
                format!("No cpufreq policies were detected under {CPUFREQ_ROOT}.")
            } else {
                "This system does not expose scaling_governor controls.".to_string()
            },
        );
        let epp_access = control_access_from_paths(
            CpuControlKind::Epp,
            policies
                .iter()
                .map(|p| p.path.join("energy_performance_preference"))
                .filter(|path| path.exists())
                .collect(),
            if policies.is_empty() {
                CpuAccessState::MissingBackend
            } else {
                CpuAccessState::Unsupported
            },
            if policies.is_empty() {
                format!("No cpufreq policies were detected under {CPUFREQ_ROOT}.")
            } else {
                "This system does not expose energy_performance_preference controls.".to_string()
            },
        );
        let freq_limit_paths = policies
            .iter()
            .filter_map(|p| {
                let min_path = p.path.join("scaling_min_freq");
                let max_path = p.path.join("scaling_max_freq");
                if min_path.exists() && max_path.exists() {
                    Some([min_path, max_path])
                } else {
                    None
                }
            })
            .flatten()
            .collect::<Vec<_>>();
        let freq_limits_access = control_access_from_paths(
            CpuControlKind::FreqLimits,
            freq_limit_paths,
            if policies.is_empty() {
                CpuAccessState::MissingBackend
            } else {
                CpuAccessState::Unsupported
            },
            if policies.is_empty() {
                format!("No cpufreq policies were detected under {CPUFREQ_ROOT}.")
            } else {
                "This system does not expose writable min/max frequency policy files together."
                    .to_string()
            },
        );
        let boost_access = control_access_from_paths(
            CpuControlKind::Boost,
            boost
                .as_ref()
                .map(|path| path.path.clone())
                .into_iter()
                .collect(),
            CpuAccessState::Unsupported,
            "Turbo boost control files were not detected on this system.".to_string(),
        );
        let core_online_access = control_access_from_paths(
            CpuControlKind::CoreOnline,
            cores
                .iter()
                .map(|id| PathBuf::from(format!("{CPU_SYSFS_ROOT}/cpu{id}/online")))
                .filter(|path| path.exists())
                .collect(),
            CpuAccessState::Unsupported,
            "This system does not expose per-core online/offline control files.".to_string(),
        );
        let power_mode_access =
            power_mode_access(&governor_access, &epp_access, policies.is_empty());
        let control_access = vec![
            boost_access.clone(),
            power_mode_access,
            governor_access.clone(),
            epp_access.clone(),
            freq_limits_access.clone(),
            core_online_access.clone(),
        ];

        let mut sysfs_paths = Vec::new();
        if !policies.is_empty() {
            sysfs_paths.push(CPUFREQ_ROOT.to_string());
        }
        for p in &policies {
            sysfs_paths.push(p.path.display().to_string());
        }
        if let Some(boost) = &boost {
            sysfs_paths.push(boost.path.display().to_string());
        }
        if let Some((path, _max)) = &rapl {
            sysfs_paths.push(path.display().to_string());
        }

        CpuCaps {
            has_cpufreq: !policies.is_empty(),
            has_epp,
            has_boost_toggle: boost.is_some(),
            has_package_power: rapl.is_some(),
            policy_writable: control_access
                .iter()
                .any(|entry| entry.status.is_writable()),
            has_min_freq_limit: has_min_freq,
            has_max_freq_limit: has_max_freq,
            has_governor,
            has_core_online,
            scaling_driver,
            cpu_count: physical_cores,
            thread_count: cores.len() as u32,
            governor_choices: governor_choices.into_iter().collect(),
            epp_choices: epp_choices.into_iter().collect(),
            sysfs_paths,
            control_access,
        }
    }

    pub fn read_snapshot(&self, timestamp_ms: u64, temp_c: Option<f32>) -> RogResult<CpuTelemetry> {
        let policies = read_policies();
        let core_ids = read_core_ids();
        let topology = read_cpu_topology(&core_ids);
        let physical_core_indices = physical_core_indices(&topology);
        let usage_current = read_proc_stat()?;
        let usage_prev = self
            .inner
            .prev_usage
            .lock()
            .map_err(|_| RogError::Unexpected("cpu usage lock poisoned".to_string()))?
            .replace(usage_current.clone());

        let overall_usage = match (
            usage_current.overall,
            usage_prev.as_ref().and_then(|p| p.overall),
        ) {
            (Some(curr), Some(prev)) => usage_from_delta(curr, prev),
            _ => None,
        };

        let mut per_core_usage = BTreeMap::new();
        if let Some(prev) = &usage_prev {
            for (core_id, curr) in &usage_current.per_core {
                let usage = prev
                    .per_core
                    .get(core_id)
                    .copied()
                    .and_then(|old| usage_from_delta(*curr, old));
                per_core_usage.insert(*core_id, usage);
            }
        }

        let mut freq_by_core: HashMap<u32, CoreFreqMhz> = HashMap::new();
        let mut policy_by_core: HashMap<u32, u32> = HashMap::new();
        let mut avg_freq_sum = 0.0f64;
        let mut avg_freq_count = 0usize;
        let mut governors = BTreeSet::new();
        let mut epps = BTreeSet::new();
        let mut min_limit = None;
        let mut max_limit = None;

        for p in &policies {
            let cur_mhz = p.cur_freq_khz.and_then(khz_to_mhz_u32);
            let min_mhz = p.min_freq_khz.and_then(khz_to_mhz_u32);
            let max_mhz = p.max_freq_khz.and_then(khz_to_mhz_u32);
            if let Some(cur) = cur_mhz {
                avg_freq_sum += cur as f64;
                avg_freq_count += 1;
            }
            if let Some(g) = &p.governor {
                governors.insert(g.clone());
            }
            if let Some(e) = &p.epp {
                epps.insert(e.clone());
            }
            if min_limit.is_none() {
                min_limit = min_mhz;
            }
            if max_limit.is_none() {
                max_limit = max_mhz;
            }

            for core in &p.related_cpus {
                freq_by_core.insert(*core, (cur_mhz, min_mhz, max_mhz));
                if let Some(policy_id) = p.id {
                    policy_by_core.insert(*core, policy_id);
                }
            }
        }

        let avg_freq_mhz = if avg_freq_count > 0 {
            Some((avg_freq_sum / avg_freq_count as f64) as f32)
        } else {
            None
        };

        let governor = if governors.len() == 1 {
            governors.iter().next().cloned()
        } else if governors.len() > 1 {
            Some("mixed".to_string())
        } else {
            None
        };
        let epp = if epps.len() == 1 {
            epps.iter().next().cloned()
        } else if epps.len() > 1 {
            Some("mixed".to_string())
        } else {
            None
        };

        let turbo_boost_enabled = read_boost_state();
        let package_power_w = self.read_package_power_w(timestamp_ms);

        let mut cores = BTreeSet::new();
        for core in usage_current.per_core.keys() {
            cores.insert(*core);
        }
        for core in freq_by_core.keys() {
            cores.insert(*core);
        }
        for core in core_ids {
            cores.insert(core);
        }

        let per_core = cores
            .into_iter()
            .map(|logical_cpu_id| {
                let usage_percent = per_core_usage.get(&logical_cpu_id).copied().flatten();
                let (current_freq_mhz, min_freq_mhz, max_freq_mhz) = freq_by_core
                    .get(&logical_cpu_id)
                    .copied()
                    .unwrap_or((None, None, None));
                let online = read_core_online(logical_cpu_id).unwrap_or(true);
                let topology_entry = topology.get(&logical_cpu_id);
                let thread_index = topology_entry.and_then(|entry| {
                    entry
                        .thread_siblings
                        .iter()
                        .position(|cpu| *cpu == logical_cpu_id)
                        .map(|idx| idx as u32 + 1)
                });
                let thread_count = topology_entry.and_then(|entry| {
                    if entry.thread_siblings.is_empty() {
                        None
                    } else {
                        Some(entry.thread_siblings.len() as u32)
                    }
                });
                CpuCoreTelemetry {
                    logical_cpu_id,
                    physical_core_index: physical_core_indices.get(&logical_cpu_id).copied(),
                    policy_id: policy_by_core.get(&logical_cpu_id).copied(),
                    thread_index,
                    thread_count,
                    usage_percent,
                    current_freq_mhz,
                    min_freq_mhz,
                    max_freq_mhz,
                    online,
                }
            })
            .collect::<Vec<_>>();

        let status = cpu_status(temp_c, package_power_w);

        Ok(CpuTelemetry {
            timestamp_ms,
            temp_c,
            usage_percent: overall_usage,
            avg_freq_mhz,
            package_power_w,
            status,
            turbo_boost_enabled,
            governor,
            epp,
            min_freq_mhz: min_limit,
            max_freq_mhz: max_limit,
            per_core,
        })
    }

    pub fn set_boost(&self, enabled: bool) -> RogResult<()> {
        let Some(boost) = detect_boost_path() else {
            return Err(RogError::NotSupported(
                "turbo boost control is not supported".to_string(),
            ));
        };
        let raw = if boost.inverted {
            if enabled {
                "0"
            } else {
                "1"
            }
        } else if enabled {
            "1"
        } else {
            "0"
        };
        write_sysfs(&boost.path, raw)
    }

    pub fn set_power_mode(&self, mode: &str) -> RogResult<()> {
        let key = normalize_token(mode);
        let policies = read_policies();
        if policies.is_empty() {
            return Err(RogError::NotSupported(
                "cpufreq policies were not detected".to_string(),
            ));
        }

        let all_governors = collect_governors(&policies);
        let all_epp = collect_epp_values(&policies);

        let (governor_pref, epp_pref): (&[&str], &[&str]) = match key.as_str() {
            "quiet" | "silent" => (
                &["powersave", "schedutil", "ondemand"],
                &["power", "balance_power", "balanced"],
            ),
            "balanced" => (
                &["schedutil", "ondemand", "powersave"],
                &["balanced", "balance_performance", "balance_power"],
            ),
            "performance" | "turbo" => (
                &["performance", "schedutil"],
                &["performance", "balance_performance"],
            ),
            _ => {
                return Err(RogError::InvalidInput(format!(
                    "unknown CPU power mode '{mode}'"
                )))
            }
        };

        let mut attempted = 0usize;
        let mut succeeded = 0usize;
        let mut errors = Vec::new();

        if let Some(gov) = choose_best_token(governor_pref, &all_governors) {
            attempted += 1;
            match self.set_governor(&gov) {
                Ok(()) => succeeded += 1,
                Err(err) => errors.push(err),
            }
        }
        if let Some(epp) = choose_best_token(epp_pref, &all_epp) {
            attempted += 1;
            match self.set_epp(&epp) {
                Ok(()) => succeeded += 1,
                Err(err) => errors.push(err),
            }
        }

        if succeeded > 0 {
            return Ok(());
        }
        if attempted == 0 {
            return Err(RogError::NotSupported(
                "CPU power mode requires governor and/or EPP support".to_string(),
            ));
        }

        if errors
            .iter()
            .all(|err| matches!(err, RogError::PermissionDenied(_)))
        {
            let details = errors
                .into_iter()
                .map(|err| err.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(RogError::PermissionDenied(format!(
                "CPU power mode could not be applied: {details}"
            )));
        }

        if let Some(err) = errors.into_iter().next() {
            return Err(err);
        }

        Err(RogError::Unexpected(
            "CPU power mode could not be applied".to_string(),
        ))
    }

    pub fn set_governor(&self, governor: &str) -> RogResult<()> {
        let governor = governor.trim();
        if governor.is_empty() {
            return Err(RogError::InvalidInput(
                "governor cannot be empty".to_string(),
            ));
        }
        let policies = read_policies();
        if policies.is_empty() {
            return Err(RogError::NotSupported(
                "cpufreq policies were not detected".to_string(),
            ));
        }
        let mut wrote_any = false;
        for p in policies {
            let path = p.path.join("scaling_governor");
            if !path.exists() {
                continue;
            }
            if !is_writable(&path) {
                continue;
            }
            write_sysfs(&path, governor)?;
            wrote_any = true;
        }
        if !wrote_any {
            return Err(RogError::PermissionDenied(
                "scaling_governor is read-only for this user".to_string(),
            ));
        }
        Ok(())
    }

    pub fn set_epp(&self, epp: &str) -> RogResult<()> {
        let epp = epp.trim();
        if epp.is_empty() {
            return Err(RogError::InvalidInput("EPP cannot be empty".to_string()));
        }
        let policies = read_policies();
        if policies.is_empty() {
            return Err(RogError::NotSupported(
                "cpufreq policies were not detected".to_string(),
            ));
        }

        let mut wrote_any = false;
        for p in policies {
            let path = p.path.join("energy_performance_preference");
            if !path.exists() {
                continue;
            }
            if !is_writable(&path) {
                continue;
            }
            write_sysfs(&path, epp)?;
            wrote_any = true;
        }
        if !wrote_any {
            return Err(RogError::PermissionDenied(
                "energy_performance_preference is read-only for this user".to_string(),
            ));
        }
        Ok(())
    }

    pub fn set_freq_limits(&self, min_mhz: Option<u32>, max_mhz: Option<u32>) -> RogResult<()> {
        if min_mhz.is_none() && max_mhz.is_none() {
            return Err(RogError::InvalidInput(
                "at least one of min/max frequency must be provided".to_string(),
            ));
        }

        let policies = read_policies();
        if policies.is_empty() {
            return Err(RogError::NotSupported(
                "cpufreq policies were not detected".to_string(),
            ));
        }

        let mut wrote_any = false;
        for p in policies {
            let min_path = p.path.join("scaling_min_freq");
            let max_path = p.path.join("scaling_max_freq");
            if !min_path.exists() || !max_path.exists() {
                continue;
            }
            if !is_writable(&min_path) || !is_writable(&max_path) {
                continue;
            }

            let info_min = p.info_min_freq_khz.or(p.min_freq_khz).unwrap_or(0);
            let info_max = p.info_max_freq_khz.or(p.max_freq_khz).unwrap_or(u64::MAX);
            let current_min = p.min_freq_khz.unwrap_or(info_min);
            let current_max = p.max_freq_khz.unwrap_or(info_max);

            let mut target_min = min_mhz.map(|v| (v as u64) * 1000).unwrap_or(current_min);
            let mut target_max = max_mhz.map(|v| (v as u64) * 1000).unwrap_or(current_max);

            target_min = target_min.clamp(info_min, info_max);
            target_max = target_max.clamp(info_min, info_max);
            if target_min > target_max {
                target_min = target_max;
            }

            if target_max < current_min {
                write_sysfs(&min_path, &target_min.to_string())?;
                write_sysfs(&max_path, &target_max.to_string())?;
            } else if target_min > current_max {
                write_sysfs(&max_path, &target_max.to_string())?;
                write_sysfs(&min_path, &target_min.to_string())?;
            } else {
                if target_max != current_max {
                    write_sysfs(&max_path, &target_max.to_string())?;
                }
                if target_min != current_min {
                    write_sysfs(&min_path, &target_min.to_string())?;
                }
            }

            wrote_any = true;
        }

        if !wrote_any {
            return Err(RogError::PermissionDenied(
                "frequency limit controls are read-only for this user".to_string(),
            ));
        }
        Ok(())
    }

    pub fn set_core_online(&self, core_id: u32, online: bool) -> RogResult<()> {
        let path = PathBuf::from(format!("{CPU_SYSFS_ROOT}/cpu{core_id}/online"));
        if !path.exists() {
            return Err(RogError::NotSupported(format!(
                "logical CPU {core_id} online/offline control is not supported"
            )));
        }
        let raw = if online { "1" } else { "0" };
        write_sysfs(&path, raw)
    }

    fn read_package_power_w(&self, timestamp_ms: u64) -> Option<f32> {
        let (energy_path, max_energy_range_uj) = detect_package_energy_path()?;
        let energy_uj = read_u64(&energy_path).ok()?;

        let mut lock = self.inner.prev_package_energy.lock().ok()?;
        let prev = lock.replace(PackageEnergySample {
            ts_ms: timestamp_ms,
            energy_uj,
            max_energy_range_uj,
        })?;

        let dt_ms = timestamp_ms.saturating_sub(prev.ts_ms);
        if dt_ms == 0 {
            return None;
        }
        let dt_s = dt_ms as f64 / 1000.0;

        let delta_energy_uj = if energy_uj >= prev.energy_uj {
            energy_uj - prev.energy_uj
        } else {
            let max = prev.max_energy_range_uj?;
            (max.saturating_sub(prev.energy_uj)).saturating_add(energy_uj)
        };

        let watts = (delta_energy_uj as f64 / 1_000_000.0) / dt_s;
        if watts.is_finite() && watts >= 0.0 {
            Some(watts as f32)
        } else {
            None
        }
    }
}

fn cpu_status(temp_c: Option<f32>, package_power_w: Option<f32>) -> Option<String> {
    if let Some(temp) = temp_c {
        if temp >= 95.0 {
            return Some("Throttling".to_string());
        }
    }
    if let Some(p) = package_power_w {
        if p >= 45.0 {
            return Some("Power-limited".to_string());
        }
    }
    Some("Normal".to_string())
}

fn detect_boost_path() -> Option<BoostPath> {
    let intel = PathBuf::from(INTEL_NO_TURBO_PATH);
    if intel.exists() {
        return Some(BoostPath {
            path: intel,
            inverted: true,
        });
    }

    let cpufreq = PathBuf::from(CPUFREQ_BOOST_PATH);
    if cpufreq.exists() {
        return Some(BoostPath {
            path: cpufreq,
            inverted: false,
        });
    }
    None
}

fn read_boost_state() -> Option<bool> {
    let boost = detect_boost_path()?;
    let raw = read_to_string(&boost.path).ok()?;
    let n: u8 = raw.trim().parse().ok()?;
    if boost.inverted {
        Some(n == 0)
    } else {
        Some(n != 0)
    }
}

fn detect_package_energy_path() -> Option<(PathBuf, Option<u64>)> {
    let root = Path::new(POWERCAP_ROOT);
    let entries = read_dir(root).ok()?;

    let mut chosen: Option<PathBuf> = None;
    for ent in entries.flatten() {
        let file_name = ent.file_name();
        let file_name = file_name.to_string_lossy();
        if !file_name.starts_with("intel-rapl:") {
            continue;
        }

        let name_path = ent.path().join("name");
        let name = read_to_string(&name_path).unwrap_or_default();
        if name.to_ascii_lowercase().contains("package") {
            chosen = Some(ent.path());
            break;
        }
        if chosen.is_none() {
            chosen = Some(ent.path());
        }
    }

    let chosen = chosen?;
    let energy = chosen.join("energy_uj");
    if !energy.exists() {
        return None;
    }
    let max = read_u64(&chosen.join("max_energy_range_uj")).ok();
    Some((energy, max))
}

fn read_policies() -> Vec<PolicyData> {
    let mut out = Vec::new();
    let root = Path::new(CPUFREQ_ROOT);
    let Ok(entries) = read_dir(root) else {
        return out;
    };

    for ent in entries.flatten() {
        let path = ent.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.starts_with("policy") {
            continue;
        }

        let related_cpus = read_cpu_list(&path.join("related_cpus")).unwrap_or_default();
        let cur_freq_khz = read_u64(&path.join("scaling_cur_freq")).ok();
        let min_freq_khz = read_u64(&path.join("scaling_min_freq")).ok();
        let max_freq_khz = read_u64(&path.join("scaling_max_freq")).ok();
        let info_min_freq_khz = read_u64(&path.join("cpuinfo_min_freq")).ok();
        let info_max_freq_khz = read_u64(&path.join("cpuinfo_max_freq")).ok();
        let governor = read_trimmed(&path.join("scaling_governor")).ok();
        let governors_available =
            read_space_split(&path.join("scaling_available_governors")).unwrap_or_default();
        let epp = read_trimmed(&path.join("energy_performance_preference")).ok();
        let epp_available =
            read_space_split(&path.join("energy_performance_available_preferences"))
                .unwrap_or_default();
        let scaling_driver = read_trimmed(&path.join("scaling_driver")).ok();
        let id = name
            .strip_prefix("policy")
            .and_then(|value| value.parse::<u32>().ok());

        out.push(PolicyData {
            id,
            path,
            related_cpus,
            cur_freq_khz,
            min_freq_khz,
            max_freq_khz,
            info_min_freq_khz,
            info_max_freq_khz,
            governor,
            governors_available,
            epp,
            epp_available,
            scaling_driver,
        });
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn read_proc_stat() -> RogResult<UsageSnapshot> {
    let text = read_to_string(PROC_STAT_PATH)
        .map_err(|e| RogError::Unexpected(format!("failed to read {PROC_STAT_PATH}: {e}")))?;
    let mut overall = None;
    let mut per_core = BTreeMap::new();

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("cpu ") {
            overall = parse_cpu_times(rest);
            continue;
        }
        if let Some(rest) = line.strip_prefix("cpu") {
            let mut split = rest.split_whitespace();
            let Some(core_id_text) = split.next() else {
                continue;
            };
            if core_id_text.is_empty() {
                continue;
            }
            let Ok(core_id) = core_id_text.parse::<u32>() else {
                continue;
            };

            let stats = split.collect::<Vec<_>>().join(" ");
            if let Some(times) = parse_cpu_times(&stats) {
                per_core.insert(core_id, times);
            }
        }
    }

    Ok(UsageSnapshot { overall, per_core })
}

fn parse_cpu_times(stats: &str) -> Option<CpuTimes> {
    let nums = stats
        .split_whitespace()
        .filter_map(|v| v.parse::<u64>().ok())
        .collect::<Vec<_>>();
    if nums.len() < 4 {
        return None;
    }
    let idle = nums.get(3).copied().unwrap_or(0) + nums.get(4).copied().unwrap_or(0);
    let total = nums.iter().sum();
    Some(CpuTimes { total, idle })
}

fn usage_from_delta(curr: CpuTimes, prev: CpuTimes) -> Option<f32> {
    let total_delta = curr.total.saturating_sub(prev.total);
    let idle_delta = curr.idle.saturating_sub(prev.idle);
    if total_delta == 0 {
        return None;
    }
    let busy = total_delta.saturating_sub(idle_delta);
    Some((busy as f64 * 100.0 / total_delta as f64) as f32)
}

fn read_core_ids() -> Vec<u32> {
    let mut out = Vec::new();
    let root = Path::new(CPU_SYSFS_ROOT);
    let Ok(entries) = read_dir(root) else {
        return out;
    };
    for ent in entries.flatten() {
        let Some(name) = ent.file_name().to_str().map(|s| s.to_string()) else {
            continue;
        };
        if !name.starts_with("cpu") {
            continue;
        }
        let tail = &name[3..];
        if let Ok(id) = tail.parse::<u32>() {
            out.push(id);
        }
    }
    out.sort_unstable();
    out
}

fn read_cpu_topology(core_ids: &[u32]) -> BTreeMap<u32, CpuTopologyEntry> {
    let mut out = BTreeMap::new();
    for id in core_ids {
        let package_id = read_trimmed(&PathBuf::from(format!(
            "{CPU_SYSFS_ROOT}/cpu{id}/topology/physical_package_id"
        )))
        .ok()
        .and_then(|value| value.parse::<u32>().ok());
        let raw_core_id = read_trimmed(&PathBuf::from(format!(
            "{CPU_SYSFS_ROOT}/cpu{id}/topology/core_id"
        )))
        .ok()
        .and_then(|value| value.parse::<u32>().ok());
        let mut thread_siblings = read_cpu_list(&PathBuf::from(format!(
            "{CPU_SYSFS_ROOT}/cpu{id}/topology/thread_siblings_list"
        )))
        .unwrap_or_default();
        if thread_siblings.is_empty() {
            thread_siblings.push(*id);
        }
        out.insert(
            *id,
            CpuTopologyEntry {
                package_id,
                raw_core_id,
                thread_siblings,
            },
        );
    }
    out
}

fn physical_core_indices(topology: &BTreeMap<u32, CpuTopologyEntry>) -> BTreeMap<u32, u32> {
    let mut key_to_index = BTreeMap::new();
    let mut next_index = 0u32;
    let mut out = BTreeMap::new();

    for (logical_cpu_id, entry) in topology {
        let Some(raw_core_id) = entry.raw_core_id else {
            continue;
        };
        let key = (entry.package_id, raw_core_id);
        let index = *key_to_index.entry(key).or_insert_with(|| {
            let current = next_index;
            next_index += 1;
            current
        });
        out.insert(*logical_cpu_id, index);
    }

    out
}

fn count_physical_cores(topology: &BTreeMap<u32, CpuTopologyEntry>) -> Option<u32> {
    let indices = physical_core_indices(topology);
    if indices.is_empty() {
        None
    } else {
        Some(indices.values().copied().collect::<BTreeSet<_>>().len() as u32)
    }
}

fn read_core_online(core_id: u32) -> Option<bool> {
    let path = PathBuf::from(format!("{CPU_SYSFS_ROOT}/cpu{core_id}/online"));
    if !path.exists() {
        // cpu0 usually has no "online" file and is always online.
        return Some(true);
    }
    let raw = read_trimmed(&path).ok()?;
    Some(raw == "1")
}

fn read_cpu_list(path: &Path) -> RogResult<Vec<u32>> {
    let raw = read_trimmed(path)?;
    let mut out = Vec::new();
    for part in raw.split(',') {
        if let Some((a, b)) = part.split_once('-') {
            let start = a.trim().parse::<u32>().map_err(|e| {
                RogError::Unexpected(format!(
                    "failed to parse cpu range start in {:?}: {e}",
                    path
                ))
            })?;
            let end = b.trim().parse::<u32>().map_err(|e| {
                RogError::Unexpected(format!("failed to parse cpu range end in {:?}: {e}", path))
            })?;
            for id in start..=end {
                out.push(id);
            }
        } else if !part.trim().is_empty() {
            let id = part.trim().parse::<u32>().map_err(|e| {
                RogError::Unexpected(format!("failed to parse cpu id in {:?}: {e}", path))
            })?;
            out.push(id);
        }
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

fn read_space_split(path: &Path) -> RogResult<Vec<String>> {
    let raw = read_trimmed(path)?;
    let mut out = raw
        .split_whitespace()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    Ok(out)
}

fn collect_governors(policies: &[PolicyData]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for p in policies {
        for g in &p.governors_available {
            out.insert(g.clone());
        }
        if let Some(g) = &p.governor {
            out.insert(g.clone());
        }
    }
    out.into_iter().collect()
}

fn collect_epp_values(policies: &[PolicyData]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for p in policies {
        for e in &p.epp_available {
            out.insert(e.clone());
        }
        if let Some(e) = &p.epp {
            out.insert(e.clone());
        }
    }
    out.into_iter().collect()
}

fn choose_best_token(preferred: &[&str], candidates: &[String]) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    let normalized = candidates
        .iter()
        .map(|c| (normalize_token(c), c.clone()))
        .collect::<Vec<_>>();
    for p in preferred {
        let key = normalize_token(p);
        if let Some((_, original)) = normalized.iter().find(|(cand, _)| *cand == key) {
            return Some(original.clone());
        }
    }
    None
}

fn normalize_token(v: &str) -> String {
    v.to_ascii_lowercase()
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

fn read_trimmed(path: &Path) -> RogResult<String> {
    read_to_string(path)
        .map(|s| s.trim().to_string())
        .map_err(|e| RogError::Unexpected(format!("failed to read {}: {e}", path.display())))
}

fn read_u64(path: &Path) -> RogResult<u64> {
    let raw = read_trimmed(path)?;
    raw.parse::<u64>().map_err(|e| {
        RogError::Unexpected(format!("failed to parse {} as u64: {e}", path.display()))
    })
}

fn khz_to_mhz_u32(v: u64) -> Option<u32> {
    u32::try_from(v / 1000).ok()
}

fn power_mode_access(
    governor: &CpuControlAccess,
    epp: &CpuControlAccess,
    missing_backend: bool,
) -> CpuControlAccess {
    let mut paths = governor.paths.clone();
    for path in &epp.paths {
        if !paths.iter().any(|existing| existing.path == path.path) {
            paths.push(path.clone());
        }
    }

    let status = if governor.status.is_writable() || epp.status.is_writable() {
        CpuAccessState::Available
    } else if missing_backend {
        CpuAccessState::MissingBackend
    } else if matches!(governor.status, CpuAccessState::PermissionDenied)
        || matches!(epp.status, CpuAccessState::PermissionDenied)
    {
        CpuAccessState::PermissionDenied
    } else if matches!(governor.status, CpuAccessState::TemporarilyUnavailable)
        || matches!(epp.status, CpuAccessState::TemporarilyUnavailable)
    {
        CpuAccessState::TemporarilyUnavailable
    } else {
        CpuAccessState::Unsupported
    };

    let reason = match status {
        CpuAccessState::Available if governor.status.is_writable() && epp.status.is_writable() => {
            "Power mode can update both governor and EPP settings.".to_string()
        }
        CpuAccessState::Available if governor.status.is_writable() => {
            "Power mode can update governor settings; EPP will remain unchanged.".to_string()
        }
        CpuAccessState::Available => {
            "Power mode can update EPP settings; governor will remain unchanged.".to_string()
        }
        CpuAccessState::MissingBackend => {
            format!("No cpufreq policies were detected under {CPUFREQ_ROOT}.")
        }
        CpuAccessState::PermissionDenied => {
            "Power mode depends on governor and/or EPP sysfs files, but the detected paths are not writable by the current user."
                .to_string()
        }
        CpuAccessState::TemporarilyUnavailable => {
            "Power mode paths were detected, but they could not be opened reliably right now."
                .to_string()
        }
        CpuAccessState::Unsupported | CpuAccessState::Unknown => {
            "Power mode requires governor and/or EPP support, but this system does not expose those CPU policy controls."
                .to_string()
        }
    };

    CpuControlAccess {
        kind: CpuControlKind::PowerMode,
        status,
        reason,
        paths,
    }
}

fn control_access_from_paths(
    kind: CpuControlKind,
    paths: Vec<PathBuf>,
    missing_status: CpuAccessState,
    missing_reason: String,
) -> CpuControlAccess {
    let paths = probe_path_accesses(paths);
    let (status, reason) = if paths.is_empty() {
        (missing_status, missing_reason)
    } else if paths.iter().any(|path| path.writable) {
        (
            CpuAccessState::Available,
            format!(
                "{} can write at least one detected sysfs control path.",
                kind.label()
            ),
        )
    } else if paths.iter().any(|path| path.readable) {
        (
            CpuAccessState::PermissionDenied,
            format!(
                "{} paths are readable but not writable by the current user.",
                kind.label()
            ),
        )
    } else {
        (
            CpuAccessState::TemporarilyUnavailable,
            format!(
                "{} paths were detected, but they could not be opened for read or write.",
                kind.label()
            ),
        )
    };

    CpuControlAccess {
        kind,
        status,
        reason,
        paths,
    }
}

fn probe_path_accesses(paths: Vec<PathBuf>) -> Vec<CpuPathAccess> {
    let mut deduped = BTreeSet::new();
    let mut out = Vec::new();
    for path in paths {
        let path_str = path.display().to_string();
        if !deduped.insert(path_str.clone()) {
            continue;
        }
        out.push(CpuPathAccess {
            path: path_str,
            readable: can_read(&path),
            writable: is_writable(&path),
        });
    }
    out
}

fn can_read(path: &Path) -> bool {
    OpenOptions::new().read(true).open(path).is_ok()
}

fn is_writable(path: &Path) -> bool {
    OpenOptions::new().write(true).open(path).is_ok()
}

fn write_sysfs(path: &Path, value: &str) -> RogResult<()> {
    let mut file = OpenOptions::new().write(true).open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            RogError::PermissionDenied(format!("{} not writable: {e}", path.display()))
        } else {
            RogError::Unexpected(format!("failed to open {} for write: {e}", path.display()))
        }
    })?;
    writeln!(file, "{value}")
        .map_err(|e| RogError::Unexpected(format!("failed to write {}: {e}", path.display())))
}
