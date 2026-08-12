use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use rog_core::{
    CpuAccessState, CpuCaps, DependencyKind, DependencyState, DependencyStatus, DeviceCaps,
    FanCaps, PermissionKind, PermissionState, PermissionStatus, SetupIssue, SetupSeverity,
    SetupStatus,
};
use tokio::time::{timeout, Duration};

use crate::asusd::AsusdPlatformProvider;
use crate::nvidia_smi::{NvidiaSmiProbe, NvidiaSmiTelemetryProvider};
use crate::supergfx::SupergfxProvider;
use crate::upower::UPowerProvider;

const DAEMON_DBUS_NAME: &str = "io.github.roghelper.Daemon";
const DAEMON_DBUS_PATH: &str = "/io/github/roghelper/Daemon";
const DAEMON_DBUS_IFACE: &str = "io.github.roghelper.Daemon1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RogHelperdProbe {
    AssumeConnected,
    ProbeSession,
}

pub async fn probe_setup_status(
    daemon_probe: RogHelperdProbe,
    caps: &DeviceCaps,
    cpu_caps: &CpuCaps,
    fan_caps: &FanCaps,
    keyboard_paths: Vec<String>,
) -> SetupStatus {
    let daemon_future = async move {
        match daemon_probe {
            RogHelperdProbe::AssumeConnected => connected_daemon_status(),
            RogHelperdProbe::ProbeSession => probe_session_daemon().await,
        }
    };
    let (daemon, asusd, supergfxd, upower, nvidia_smi) = tokio::join!(
        bounded_probe(
            DependencyKind::RogHelperd,
            vec!["application communication".to_string()],
            daemon_future,
        ),
        bounded_probe(
            DependencyKind::Asusd,
            vec![
                "performance profiles".to_string(),
                "battery charge limit".to_string(),
            ],
            probe_asusd(),
        ),
        bounded_probe(
            DependencyKind::Supergfxd,
            vec!["GPU mode switching".to_string()],
            probe_supergfxd(),
        ),
        bounded_probe(
            DependencyKind::UPower,
            vec!["battery and AC telemetry".to_string()],
            probe_upower(),
        ),
        bounded_probe(
            DependencyKind::NvidiaSmi,
            vec!["NVIDIA GPU telemetry".to_string()],
            probe_nvidia_smi(),
        ),
    );

    let dependencies = vec![daemon, asusd, supergfxd, upower, nvidia_smi];
    let permissions = vec![
        cpu_permission_status(cpu_caps),
        keyboard_permission_status(caps, keyboard_paths),
        fan_permission_status(fan_caps),
    ];
    let issues = build_issues(&dependencies, &permissions);

    SetupStatus {
        checked_at_ms: now_ms(),
        dependencies,
        permissions,
        issues,
    }
}

async fn bounded_probe<F>(
    kind: DependencyKind,
    required_for: Vec<String>,
    future: F,
) -> DependencyStatus
where
    F: std::future::Future<Output = DependencyStatus>,
{
    match timeout(Duration::from_secs(4), future).await {
        Ok(status) => status,
        Err(_) => DependencyStatus {
            kind,
            state: DependencyState::Unreachable,
            summary: "The readiness check timed out before the expected API responded.".to_string(),
            required_for,
            evidence: vec!["live API check timed out after 4 seconds".to_string()],
        },
    }
}

fn connected_daemon_status() -> DependencyStatus {
    DependencyStatus {
        kind: DependencyKind::RogHelperd,
        state: DependencyState::Connected,
        summary: "The session daemon API is responding.".to_string(),
        required_for: vec!["application communication".to_string()],
        evidence: vec![format!(
            "DBus API: {DAEMON_DBUS_NAME} {DAEMON_DBUS_PATH} ({DAEMON_DBUS_IFACE})"
        )],
    }
}

async fn probe_session_daemon() -> DependencyStatus {
    let mut evidence = service_evidence("rog-helperd", &["rog-helperd.service"], true);
    let result = async {
        let conn = zbus::Connection::session().await?;
        let names = zbus::fdo::DBusProxy::new(&conn).await?.list_names().await?;
        if !names.iter().any(|name| name.as_str() == DAEMON_DBUS_NAME) {
            return Err(zbus::Error::Failure(
                "rog-helperd has no owner on the session bus".to_string(),
            ));
        }
        let proxy = zbus::Proxy::new(
            &conn,
            DAEMON_DBUS_NAME,
            DAEMON_DBUS_PATH,
            "org.freedesktop.DBus.Introspectable",
        )
        .await?;
        let xml = proxy.call::<_, _, String>("Introspect", &()).await?;
        if xml.contains(DAEMON_DBUS_IFACE) {
            Ok(())
        } else {
            Err(zbus::Error::Failure(format!(
                "expected interface {DAEMON_DBUS_IFACE} was not introspectable"
            )))
        }
    }
    .await;

    match result {
        Ok(_) => connected_daemon_status(),
        Err(error) => {
            evidence.push(format!("DBus API check failed: {error}"));
            dependency_from_failed_api(
                DependencyKind::RogHelperd,
                "The session daemon API did not respond.",
                vec!["application communication".to_string()],
                evidence,
                &["rog-helperd.service"],
                true,
            )
        }
    }
}

async fn probe_asusd() -> DependencyStatus {
    let required_for = vec![
        "performance profiles".to_string(),
        "battery charge limit".to_string(),
    ];
    let mut evidence = service_evidence("asusd", &["asusd.service"], false);
    match AsusdPlatformProvider::connect_system().await {
        Ok(Some(provider)) => {
            evidence.push(format!("DBus API: {}", provider.endpoint_tag()));
            match provider.probe_caps().await {
                Ok(_) => DependencyStatus {
                    kind: DependencyKind::Asusd,
                    state: DependencyState::Ready,
                    summary: "A compatible asusd platform API responded.".to_string(),
                    required_for,
                    evidence,
                },
                Err(error) => {
                    evidence.push(format!("API capability check failed: {error}"));
                    DependencyStatus {
                        kind: DependencyKind::Asusd,
                        state: DependencyState::Unreachable,
                        summary: "asusd is visible, but its expected platform API did not respond correctly."
                            .to_string(),
                        required_for,
                        evidence,
                    }
                }
            }
        }
        Ok(None) => dependency_from_failed_api(
            DependencyKind::Asusd,
            "No compatible asusd platform API was found.",
            required_for,
            evidence,
            &["asusd.service"],
            false,
        ),
        Err(error) => {
            evidence.push(format!("DBus connection failed: {error}"));
            DependencyStatus {
                kind: DependencyKind::Asusd,
                state: DependencyState::Unreachable,
                summary: "The system bus or asusd API could not be reached.".to_string(),
                required_for,
                evidence,
            }
        }
    }
}

async fn probe_supergfxd() -> DependencyStatus {
    let required_for = vec!["GPU mode switching".to_string()];
    let mut evidence = service_evidence("supergfxctl", &["supergfxd.service"], false);
    match SupergfxProvider::connect_system().await {
        Ok(Some(provider)) => {
            evidence.push(format!("DBus API: {}", provider.endpoint_tag()));
            match provider.probe_caps().await {
                Ok(_) => DependencyStatus {
                    kind: DependencyKind::Supergfxd,
                    state: DependencyState::Ready,
                    summary: "A compatible supergfxd API responded.".to_string(),
                    required_for,
                    evidence,
                },
                Err(error) => {
                    evidence.push(format!("API capability check failed: {error}"));
                    DependencyStatus {
                        kind: DependencyKind::Supergfxd,
                        state: DependencyState::Unreachable,
                        summary:
                            "supergfxd is visible, but its expected GPU API did not respond correctly."
                                .to_string(),
                        required_for,
                        evidence,
                    }
                }
            }
        }
        Ok(None) => dependency_from_failed_api(
            DependencyKind::Supergfxd,
            "No compatible supergfxd API was found.",
            required_for,
            evidence,
            &["supergfxd.service"],
            false,
        ),
        Err(error) => {
            evidence.push(format!("DBus connection failed: {error}"));
            DependencyStatus {
                kind: DependencyKind::Supergfxd,
                state: DependencyState::Unreachable,
                summary: "The system bus or supergfxd API could not be reached.".to_string(),
                required_for,
                evidence,
            }
        }
    }
}

async fn probe_upower() -> DependencyStatus {
    let required_for = vec!["battery and AC telemetry".to_string()];
    let mut evidence = service_evidence("upowerd", &["upower.service", "upowerd.service"], false);
    match UPowerProvider::connect_system().await {
        Ok(provider) => match provider.read_status().await {
            Ok(_) => {
                evidence
                    .push("DBus API: org.freedesktop.UPower /org/freedesktop/UPower".to_string());
                DependencyStatus {
                    kind: DependencyKind::UPower,
                    state: DependencyState::Ready,
                    summary: "UPower properties and device enumeration responded.".to_string(),
                    required_for,
                    evidence,
                }
            }
            Err(error) => {
                evidence.push(format!("UPower API check failed: {error}"));
                dependency_from_failed_api(
                    DependencyKind::UPower,
                    "UPower is present, but its expected API did not respond.",
                    required_for,
                    evidence,
                    &["upower.service", "upowerd.service"],
                    false,
                )
            }
        },
        Err(error) => {
            evidence.push(format!("DBus connection failed: {error}"));
            DependencyStatus {
                kind: DependencyKind::UPower,
                state: DependencyState::Unreachable,
                summary: "The system bus required for UPower could not be reached.".to_string(),
                required_for,
                evidence,
            }
        }
    }
}

async fn probe_nvidia_smi() -> DependencyStatus {
    let hardware_present = nvidia_hardware_present();
    let mut evidence = vec![format!(
        "NVIDIA PCI display hardware: {}",
        if hardware_present {
            "detected"
        } else {
            "not detected"
        }
    )];
    evidence.extend(binary_evidence("nvidia-smi"));
    let required_for = vec!["NVIDIA GPU telemetry".to_string()];

    match NvidiaSmiTelemetryProvider::default().probe().await {
        NvidiaSmiProbe::Available { gpu_names } => {
            if !gpu_names.is_empty() {
                evidence.push(format!("reported GPUs: {}", gpu_names.join(", ")));
            }
            DependencyStatus {
                kind: DependencyKind::NvidiaSmi,
                state: DependencyState::Ready,
                summary: "nvidia-smi completed a live GPU query.".to_string(),
                required_for,
                evidence,
            }
        }
        NvidiaSmiProbe::NotFound if !hardware_present => DependencyStatus {
            kind: DependencyKind::NvidiaSmi,
            state: DependencyState::NotRelevant,
            summary: "No NVIDIA display hardware was detected, so nvidia-smi is not needed."
                .to_string(),
            required_for,
            evidence,
        },
        NvidiaSmiProbe::NotFound => DependencyStatus {
            kind: DependencyKind::NvidiaSmi,
            state: DependencyState::NotAvailable,
            summary: "NVIDIA hardware is present, but nvidia-smi was not found.".to_string(),
            required_for,
            evidence,
        },
        NvidiaSmiProbe::Unavailable(error) if !hardware_present => {
            evidence.push(format!("live query failed: {error}"));
            DependencyStatus {
                kind: DependencyKind::NvidiaSmi,
                state: DependencyState::NotRelevant,
                summary: "nvidia-smi is not usable, but no NVIDIA display hardware was detected."
                    .to_string(),
                required_for,
                evidence,
            }
        }
        NvidiaSmiProbe::Unavailable(error) => {
            evidence.push(format!("live query failed: {error}"));
            DependencyStatus {
                kind: DependencyKind::NvidiaSmi,
                state: DependencyState::Unreachable,
                summary: "nvidia-smi exists, but a live GPU query failed.".to_string(),
                required_for,
                evidence,
            }
        }
    }
}

fn dependency_from_failed_api(
    kind: DependencyKind,
    summary: &str,
    required_for: Vec<String>,
    evidence: Vec<String>,
    units: &[&str],
    user_unit: bool,
) -> DependencyStatus {
    let state = best_unit_status(units, user_unit)
        .map(|unit| {
            if unit.active == "active" {
                DependencyState::Unreachable
            } else if unit.load == "loaded" {
                DependencyState::Inactive
            } else {
                DependencyState::NotAvailable
            }
        })
        .unwrap_or(DependencyState::NotAvailable);
    DependencyStatus {
        kind,
        state,
        summary: summary.to_string(),
        required_for,
        evidence,
    }
}

fn cpu_permission_status(caps: &CpuCaps) -> PermissionStatus {
    let relevant = caps
        .control_access
        .iter()
        .filter(|entry| entry.status != CpuAccessState::Unsupported)
        .collect::<Vec<_>>();
    let writable = relevant
        .iter()
        .filter(|entry| entry.status == CpuAccessState::Available)
        .count();
    let paths = relevant
        .iter()
        .flat_map(|entry| &entry.paths)
        .filter(|path| !path.writable)
        .map(|path| path.path.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let (state, summary) = if relevant.is_empty() {
        (
            PermissionState::Unsupported,
            "No writable CPU policy interface is exposed by this platform.".to_string(),
        )
    } else if writable == relevant.len() {
        (
            PermissionState::Writable,
            format!("All {writable} detected CPU control areas are writable."),
        )
    } else if relevant
        .iter()
        .any(|entry| entry.status == CpuAccessState::PermissionDenied)
    {
        (
            PermissionState::ReadOnly,
            format!(
                "{writable} of {} detected CPU control areas are writable; at least one is blocked by permissions.",
                relevant.len()
            ),
        )
    } else {
        (
            PermissionState::Unavailable,
            "CPU control interfaces were detected, but are not currently available.".to_string(),
        )
    };

    PermissionStatus {
        kind: PermissionKind::CpuPolicyWrites,
        state,
        summary,
        paths,
    }
}

fn keyboard_permission_status(caps: &DeviceCaps, paths: Vec<String>) -> PermissionStatus {
    let (state, summary) = match caps.kbd_backlight_access.status {
        rog_core::FeatureAccessState::Available => (
            PermissionState::Writable,
            "The selected keyboard lighting backend accepts writes.".to_string(),
        ),
        rog_core::FeatureAccessState::PermissionDenied => (
            PermissionState::ReadOnly,
            "Keyboard lighting is detected, but writes are blocked for the current daemon user."
                .to_string(),
        ),
        rog_core::FeatureAccessState::Unsupported => (
            PermissionState::Unsupported,
            "No supported keyboard lighting control was detected.".to_string(),
        ),
        rog_core::FeatureAccessState::Unknown => (
            PermissionState::Unknown,
            "Keyboard lighting access has not been checked yet.".to_string(),
        ),
        _ => (
            PermissionState::Unavailable,
            caps.kbd_backlight_access.reason.clone(),
        ),
    };
    PermissionStatus {
        kind: PermissionKind::KeyboardLightingWrites,
        state,
        summary,
        paths,
    }
}

fn fan_permission_status(caps: &FanCaps) -> PermissionStatus {
    let paths = caps
        .endpoints
        .iter()
        .filter_map(|endpoint| endpoint.split_once(':').map(|(_, path)| path.to_string()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if caps.has_individual_fan_control
        || caps.has_fan_manual_percent
        || caps.has_fan_manual_rpm_target
        || caps.has_fan_curves
    {
        return PermissionStatus {
            kind: PermissionKind::FanControls,
            state: PermissionState::Writable,
            summary: "At least one explicit fan-control endpoint is writable.".to_string(),
            paths,
        };
    }
    if caps.has_fan_reading || caps.fan_count > 0 {
        return PermissionStatus {
            kind: PermissionKind::FanControls,
            state: PermissionState::ReadOnly,
            summary: "Fan RPM telemetry is available, but no writable manual-control endpoint was confirmed."
                .to_string(),
            paths,
        };
    }
    PermissionStatus {
        kind: PermissionKind::FanControls,
        state: PermissionState::Unsupported,
        summary: "No fan telemetry or supported fan-control endpoint was detected.".to_string(),
        paths,
    }
}

fn build_issues(
    dependencies: &[DependencyStatus],
    permissions: &[PermissionStatus],
) -> Vec<SetupIssue> {
    let mut issues = Vec::new();
    for dependency in dependencies {
        if dependency.state.is_ready() || dependency.state == DependencyState::Unknown {
            continue;
        }
        let severity = if dependency.kind == DependencyKind::RogHelperd {
            SetupSeverity::Error
        } else {
            SetupSeverity::Warning
        };
        let required = if dependency.required_for.is_empty() {
            String::new()
        } else {
            format!(
                " It is required for {}.",
                dependency.required_for.join(" and ")
            )
        };
        issues.push(SetupIssue {
            severity,
            title: format!("{} needs attention", dependency.kind.label()),
            summary: format!("{}{required}", dependency.summary),
            guidance: format!(
                "Check that {} is installed and running, then refresh. See installation documentation for validated distribution-specific setup steps.",
                dependency.kind.label()
            ),
        });
    }

    for permission in permissions {
        let (severity, guidance) = match permission.state {
            PermissionState::ReadOnly => (
                SetupSeverity::Warning,
                "The application remains unprivileged. Review the permission documentation for the detected backend; no permission changes are applied automatically.",
            ),
            PermissionState::Unavailable => (
                SetupSeverity::Warning,
                "Refresh after the backend becomes available. See installation or troubleshooting documentation if the state persists.",
            ),
            _ => continue,
        };
        issues.push(SetupIssue {
            severity,
            title: format!("{}: {}", permission.kind.label(), permission.state.label()),
            summary: permission.summary.clone(),
            guidance: guidance.to_string(),
        });
    }
    issues
}

#[derive(Debug, Clone)]
struct UnitStatus {
    name: String,
    load: String,
    active: String,
    sub: String,
}

fn service_evidence(binary: &str, units: &[&str], user_unit: bool) -> Vec<String> {
    let mut evidence = binary_evidence(binary);
    for unit in units {
        if let Some(status) = systemctl_unit_status(unit, user_unit) {
            evidence.push(format!(
                "systemd {}: load={}, active={}, sub={}",
                status.name, status.load, status.active, status.sub
            ));
        }
    }
    evidence
}

fn binary_evidence(binary: &str) -> Vec<String> {
    match find_binary(binary) {
        Some(path) => vec![format!("binary: {}", path.display())],
        None => vec![format!("binary: {binary} not found on PATH")],
    }
}

fn find_binary(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
}

fn best_unit_status(units: &[&str], user_unit: bool) -> Option<UnitStatus> {
    units
        .iter()
        .filter_map(|unit| systemctl_unit_status(unit, user_unit))
        .max_by_key(
            |status| match (status.load.as_str(), status.active.as_str()) {
                ("loaded", "active") => 3,
                ("loaded", _) => 2,
                ("not-found", _) => 0,
                _ => 1,
            },
        )
}

fn systemctl_unit_status(unit: &str, user_unit: bool) -> Option<UnitStatus> {
    let mut command = Command::new("systemctl");
    if user_unit {
        command.arg("--user");
    }
    let output = command
        .args([
            "show",
            unit,
            "-p",
            "LoadState",
            "-p",
            "ActiveState",
            "-p",
            "SubState",
            "--no-pager",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut load = None;
    let mut active = None;
    let mut sub = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("LoadState=") {
            load = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("ActiveState=") {
            active = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("SubState=") {
            sub = Some(value.trim().to_string());
        }
    }
    Some(UnitStatus {
        name: unit.to_string(),
        load: load?,
        active: active.unwrap_or_else(|| "unknown".to_string()),
        sub: sub.unwrap_or_else(|| "unknown".to_string()),
    })
}

fn nvidia_hardware_present() -> bool {
    let root = Path::new("/sys/bus/pci/devices");
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        let vendor = fs::read_to_string(path.join("vendor"))
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let class = fs::read_to_string(path.join("class"))
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        vendor == "0x10de" && class.starts_with("0x03")
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use rog_core::{
        CpuControlAccess, CpuControlKind, CpuPathAccess, FeatureAccessState, FeatureAvailability,
    };

    #[test]
    fn cpu_permission_reports_read_only_paths() {
        let mut caps = CpuCaps::unknown();
        caps.control_access.push(CpuControlAccess {
            kind: CpuControlKind::Governor,
            status: CpuAccessState::PermissionDenied,
            reason: "blocked".to_string(),
            paths: vec![CpuPathAccess {
                path: "/sys/example".to_string(),
                readable: true,
                writable: false,
            }],
        });
        let status = cpu_permission_status(&caps);
        assert_eq!(status.state, PermissionState::ReadOnly);
        assert_eq!(status.paths, vec!["/sys/example"]);
    }

    #[test]
    fn keyboard_permission_distinguishes_read_only_and_unsupported() {
        let mut caps = DeviceCaps::unknown();
        caps.kbd_backlight_access =
            FeatureAvailability::new(FeatureAccessState::PermissionDenied, "blocked");
        assert_eq!(
            keyboard_permission_status(&caps, Vec::new()).state,
            PermissionState::ReadOnly
        );
        caps.kbd_backlight_access =
            FeatureAvailability::new(FeatureAccessState::Unsupported, "not detected");
        assert_eq!(
            keyboard_permission_status(&caps, Vec::new()).state,
            PermissionState::Unsupported
        );
    }

    #[test]
    fn fan_telemetry_without_writes_is_read_only() {
        let mut caps = FanCaps::from_fans(&[]);
        caps.has_fan_reading = true;
        caps.fan_count = 2;
        assert_eq!(
            fan_permission_status(&caps).state,
            PermissionState::ReadOnly
        );
    }

    #[test]
    fn connected_daemon_does_not_create_an_issue() {
        let dependencies = vec![connected_daemon_status()];
        assert!(build_issues(&dependencies, &[]).is_empty());
    }

    #[test]
    fn unavailable_daemon_is_an_error_and_control_issue() {
        let status = SetupStatus::daemon_unavailable("session API missing");
        assert_eq!(status.issues[0].severity, SetupSeverity::Error);
        assert_eq!(status.control_issue_count(), 1);
    }

    #[test]
    fn missing_control_services_create_separate_guidance() {
        let dependencies = [DependencyKind::Asusd, DependencyKind::Supergfxd]
            .into_iter()
            .map(|kind| DependencyStatus {
                kind,
                state: DependencyState::NotAvailable,
                summary: "API missing".to_string(),
                required_for: vec!["test control".to_string()],
                evidence: Vec::new(),
            })
            .collect::<Vec<_>>();
        let issues = build_issues(&dependencies, &[]);
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().any(|issue| issue.title.contains("asusd")));
        assert!(issues.iter().any(|issue| issue.title.contains("supergfxd")));
    }

    #[test]
    fn read_only_cpu_and_keyboard_are_actionable() {
        let permissions = vec![
            PermissionStatus {
                kind: PermissionKind::CpuPolicyWrites,
                state: PermissionState::ReadOnly,
                summary: "CPU writes blocked".to_string(),
                paths: Vec::new(),
            },
            PermissionStatus {
                kind: PermissionKind::KeyboardLightingWrites,
                state: PermissionState::ReadOnly,
                summary: "Lighting writes blocked".to_string(),
                paths: Vec::new(),
            },
        ];
        let issues = build_issues(&[], &permissions);
        assert_eq!(issues.len(), 2);
        assert!(issues
            .iter()
            .all(|issue| issue.guidance.contains("remains unprivileged")));
    }
}
