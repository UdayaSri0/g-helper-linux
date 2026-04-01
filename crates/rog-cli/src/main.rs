use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use clap::{Parser, Subcommand};
use regex::Regex;
use rog_core::{DeviceCaps, FeatureAccessState, FeatureAvailability};
use rog_providers::asusd::AsusdPlatformProvider;
use rog_providers::dbus;
use rog_providers::hwmon::HwmonTelemetryProvider;
use rog_providers::kbd_backlight::KbdBacklightSysfs;
use rog_providers::nvidia_smi::NvidiaSmiTelemetryProvider;
use rog_providers::supergfx::SupergfxProvider;
use rog_providers::traits::{BatteryProvider, GpuProvider, ProfileProvider};
use tracing::{info, warn};

#[derive(Debug, Parser)]
#[command(
    name = "rog-helper",
    version,
    about = "rog-helper diagnostics + probing tool"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Print presence of key services (DBus + systemd).
    Services,
    /// List and optionally introspect system DBus services.
    Dbus {
        /// Regex used to filter bus names (case-insensitive).
        #[arg(long, default_value = "asus|rog|supergfx|power|upower")]
        filter: String,
        /// Introspect a specific service name.
        #[arg(long)]
        service: Option<String>,
        /// Object path to introspect (defaults to "/").
        #[arg(long, default_value = "/")]
        path: String,
        /// If set, walk the introspection tree up to this depth.
        #[arg(long)]
        depth: Option<usize>,
        /// Maximum number of nodes to introspect when walking.
        #[arg(long, default_value_t = 50)]
        max_nodes: usize,
        /// Introspection call timeout in milliseconds.
        #[arg(long, default_value_t = 1500)]
        timeout_ms: u64,
    },
    /// Print detected hwmon sensors and a one-shot telemetry snapshot.
    Sensors {
        /// Override hwmon root (default: /sys/class/hwmon).
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Print a best-effort DeviceCaps summary (Milestone 1).
    Caps,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,rog_helper=debug".into()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Services => cmd_services().await?,
        Cmd::Dbus {
            filter,
            service,
            path,
            depth,
            max_nodes,
            timeout_ms,
        } => {
            cmd_dbus(
                &filter,
                service.as_deref(),
                &path,
                depth,
                max_nodes,
                timeout_ms,
            )
            .await?
        }
        Cmd::Sensors { root } => cmd_sensors(root.as_deref()).await?,
        Cmd::Caps => cmd_caps().await?,
    }

    Ok(())
}

async fn cmd_services() -> anyhow::Result<()> {
    let re = Regex::new("(?i)asus|rog|supergfx|power|upower")?;
    let names = dbus::list_system_bus_names_matching(&re)
        .await
        .context("list system bus names")?;

    println!("System DBus names (filtered):");
    for n in &names {
        println!("  {n}");
    }
    if names.is_empty() {
        println!("  (none matched)");
    }

    println!();
    println!("systemd services (best-effort):");
    for svc in ["asusd", "supergfxd", "upower"] {
        match systemctl_unit_status(svc) {
            Some(state) => println!("  {svc}: {state}"),
            None => println!("  {svc}: (systemctl not available)"),
        }
    }

    Ok(())
}

async fn cmd_dbus(
    filter: &str,
    service: Option<&str>,
    path: &str,
    depth: Option<usize>,
    max_nodes: usize,
    timeout_ms: u64,
) -> anyhow::Result<()> {
    let re = Regex::new(&format!("(?i){filter}"))?;
    let names = dbus::list_system_bus_names_matching(&re)
        .await
        .context("list system bus names")?;

    println!("System DBus names (filtered):");
    for n in &names {
        println!("  {n}");
    }
    if names.is_empty() {
        println!("  (none matched)");
    }

    if let Some(svc) = service {
        println!();
        if let Some(depth) = depth {
            println!("Introspection walk: {svc} {path} (max depth {depth})");
            let nodes = dbus::system_walk_introspection(svc, path, depth, max_nodes, timeout_ms)
                .await
                .context("introspection walk")?;
            for n in nodes {
                println!();
                println!("Path: {}", n.path);
                if n.interfaces.is_empty() {
                    println!("  Interfaces: (none)");
                } else {
                    println!("  Interfaces:");
                    for i in n.interfaces {
                        println!("    {i}");
                    }
                }
                if !n.children.is_empty() {
                    println!("  Children:");
                    for c in n.children {
                        println!("    {c}");
                    }
                }
            }
        } else {
            println!("Introspect: {svc} {path}");
            let xml = dbus::system_introspect(svc, path, timeout_ms)
                .await
                .context("introspect")?;
            println!("{xml}");
        }
    }

    Ok(())
}

async fn cmd_sensors(root: Option<&std::path::Path>) -> anyhow::Result<()> {
    let provider = root
        .map(|p| HwmonTelemetryProvider::new(p.to_path_buf()))
        .unwrap_or_default();

    println!("hwmon devices:");
    let hwmons = provider.list_hwmon().context("list hwmon")?;
    for (name, path) in &hwmons {
        println!("  {name}: {}", path.display());
    }
    if hwmons.is_empty() {
        println!("  (none)");
    }

    println!();
    println!("telemetry snapshot:");
    let mut snap = provider.read_snapshot().context("read hwmon snapshot")?;
    if snap.gpu_temp_c.is_none() {
        let nvidia = NvidiaSmiTelemetryProvider::default();
        if let Ok(Some(temp)) = nvidia.read_gpu_temp_c().await {
            snap.gpu_temp_c = Some(temp);
            snap.temps_c.insert("nvidia-smi:gpu_temp".to_string(), temp);
        }
    }
    println!("  timestamp_ms: {}", snap.timestamp_ms);
    println!("  cpu_temp_c: {:?}", snap.cpu_temp_c);
    println!("  gpu_temp_c: {:?}", snap.gpu_temp_c);
    println!("  temps: {}", snap.temps_c.len());
    println!("  fans: {}", snap.fans_rpm.len());

    Ok(())
}

async fn cmd_caps() -> anyhow::Result<()> {
    info!("probing DeviceCaps");

    let hwmon = HwmonTelemetryProvider::default();
    let snap = hwmon.read_snapshot().unwrap_or_else(|e| {
        warn!("hwmon failed: {e}");
        rog_core::TelemetrySnapshot::empty_now(0)
    });
    let (kbd_backlight, kbd_backlight_probe_error) = match KbdBacklightSysfs::probe() {
        Ok(v) => (v, None),
        Err(e) => {
            warn!("kbd backlight probe failed: {e}");
            (None, Some(e.to_string()))
        }
    };

    let mut caps = DeviceCaps::unknown();
    caps.has_fan_reading = !snap.fans_rpm.is_empty();
    caps.has_kbd_backlight = kbd_backlight.is_some() || detect_kbd_backlight();
    caps.kbd_backlight_access = if let Some(kbd) = &kbd_backlight {
        if kbd.can_set_brightness() {
            FeatureAvailability::new(
                FeatureAccessState::Available,
                "Keyboard backlight is available through the sysfs LED backend.",
            )
        } else {
            FeatureAvailability::new(
                FeatureAccessState::PermissionDenied,
                "Keyboard backlight is detected, but writes are blocked for the current user.",
            )
        }
    } else if kbd_backlight_probe_error.is_some() || caps.has_kbd_backlight {
        FeatureAvailability::new(
            FeatureAccessState::TemporarilyUnavailable,
            "Keyboard backlight hardware was detected, but the backend could not be opened right now.",
        )
    } else {
        FeatureAvailability::new(
            FeatureAccessState::Unsupported,
            "No keyboard backlight control was detected on this machine.",
        )
    };

    let (asusd, asusd_connect_error) = match AsusdPlatformProvider::connect_system().await {
        Ok(v) => (v, None),
        Err(e) => {
            warn!("asusd connect failed: {e}");
            (None, Some(e.to_string()))
        }
    };

    if let Some(asusd) = asusd {
        caps.endpoints
            .push(format!("asusd-platform:{}", asusd.endpoint_tag()));
        match asusd.probe_caps().await {
            Ok(pcaps) => {
                caps.has_profiles = pcaps.has_profiles;
                caps.has_charge_limit = pcaps.has_charge_limit;
                caps.profile_access = if pcaps.has_profiles {
                    FeatureAvailability::new(
                        FeatureAccessState::Available,
                        "Performance profiles are available through asusd.",
                    )
                } else {
                    FeatureAvailability::new(
                        FeatureAccessState::Unsupported,
                        "asusd is available, but this machine does not expose ASUS performance profiles.",
                    )
                };
                caps.charge_limit_access = if pcaps.has_charge_limit {
                    FeatureAvailability::new(
                        FeatureAccessState::Available,
                        "Battery charge-limit control is available through asusd.",
                    )
                } else {
                    FeatureAvailability::new(
                        FeatureAccessState::Unsupported,
                        "asusd is available, but this machine does not expose battery charge-limit control.",
                    )
                };
            }
            Err(e) => {
                caps.profile_access = FeatureAvailability::new(
                    FeatureAccessState::TemporarilyUnavailable,
                    "asusd is present, but profile support could not be confirmed right now.",
                );
                caps.charge_limit_access = FeatureAvailability::new(
                    FeatureAccessState::TemporarilyUnavailable,
                    "asusd is present, but charge-limit support could not be confirmed right now.",
                );
                caps.notes
                    .push(format!("asusd platform probing failed: {e}"));
            }
        }
        if caps.has_profiles {
            match asusd.get_profile().await {
                Ok(p) => caps.notes.push(format!("Current profile: {p:?}")),
                Err(e) => caps
                    .notes
                    .push(format!("could not read current profile from asusd: {e}")),
            }
        }
        if caps.has_charge_limit {
            match asusd.get_limit().await {
                Ok(l) => caps.notes.push(format!("Current charge limit: {}%", l.0)),
                Err(e) => caps
                    .notes
                    .push(format!("could not read charge limit from asusd: {e}")),
            }
        }
    } else if let Some(err) = &asusd_connect_error {
        caps.profile_access = backend_access_from_connect_error(
            Some(err.as_str()),
            "Install and start asusd to enable performance profiles.",
            "Performance profiles need asusd, but the system backend could not be reached right now.",
        );
        caps.charge_limit_access = backend_access_from_connect_error(
            Some(err.as_str()),
            "Install and start asusd to enable charge-limit control.",
            "Charge-limit control needs asusd, but the system backend could not be reached right now.",
        );
        caps.notes.push(format!("asusd connect failed: {err}"));
    } else {
        caps.profile_access = backend_access_from_connect_error(
            None,
            "Install and start asusd to enable performance profiles.",
            "Performance profiles need asusd, but the system backend could not be reached right now.",
        );
        caps.charge_limit_access = backend_access_from_connect_error(
            None,
            "Install and start asusd to enable charge-limit control.",
            "Charge-limit control needs asusd, but the system backend could not be reached right now.",
        );
        caps.notes
            .push("asusd not detected; profile/charge controls disabled.".to_string());
    }

    let (supergfx, supergfx_connect_error) = match SupergfxProvider::connect_system().await {
        Ok(v) => (v, None),
        Err(e) => {
            warn!("supergfxd connect failed: {e}");
            (None, Some(e.to_string()))
        }
    };

    if let Some(supergfx) = supergfx {
        caps.endpoints
            .push(format!("supergfxd:{}", supergfx.endpoint_tag()));
        match supergfx.probe_caps().await {
            Ok(gcaps) => {
                caps.has_gpu_modes = !gcaps.raw_supported_modes.is_empty();
                caps.requires_reboot_for_gpu_switch = gcaps.requires_reboot_hint;
                caps.gpu_mode_access = if caps.has_gpu_modes {
                    FeatureAvailability::new(
                        FeatureAccessState::Available,
                        "GPU mode switching is available through supergfxd.",
                    )
                } else {
                    FeatureAvailability::new(
                        FeatureAccessState::Unsupported,
                        "supergfxd is available, but this machine does not expose switchable GPU modes.",
                    )
                };
                if !gcaps.raw_supported_modes.is_empty() {
                    caps.notes.push(format!(
                        "supergfxd supported modes: {}",
                        gcaps.raw_supported_modes.join(", ")
                    ));
                }
            }
            Err(e) => {
                caps.gpu_mode_access = FeatureAvailability::new(
                    FeatureAccessState::TemporarilyUnavailable,
                    "supergfxd is present, but GPU mode support could not be confirmed right now.",
                );
                caps.notes.push(format!("supergfxd probing failed: {e}"));
            }
        }
        if caps.has_gpu_modes {
            match supergfx.get_mode().await {
                Ok(m) => caps.notes.push(format!("Current GPU mode: {m:?}")),
                Err(e) => caps.notes.push(format!(
                    "could not read current GPU mode from supergfxd: {e}"
                )),
            }
        }
    } else if let Some(err) = &supergfx_connect_error {
        caps.gpu_mode_access = backend_access_from_connect_error(
            Some(err.as_str()),
            "Install and start supergfxd to enable GPU mode switching.",
            "GPU mode switching needs supergfxd, but the system backend could not be reached right now.",
        );
        caps.notes.push(format!("supergfxd connect failed: {err}"));
    } else {
        caps.gpu_mode_access = backend_access_from_connect_error(
            None,
            "Install and start supergfxd to enable GPU mode switching.",
            "GPU mode switching needs supergfxd, but the system backend could not be reached right now.",
        );
        caps.notes
            .push("supergfxd not detected; GPU mode controls disabled.".to_string());
    }

    // Include filtered DBus names as endpoints for diagnostics.
    let re = Regex::new("(?i)asus|rog|supergfx|power|upower")?;
    if let Ok(names) = dbus::list_system_bus_names_matching(&re).await {
        for n in names {
            caps.endpoints.push(format!("system-bus-name:{n}"));
        }
    }

    println!("{:#?}", caps);
    Ok(())
}

fn systemctl_unit_status(service: &str) -> Option<String> {
    let output = Command::new("systemctl")
        .args([
            "show",
            service,
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

    let txt = String::from_utf8_lossy(&output.stdout);
    let mut load = None;
    let mut active = None;
    let mut sub = None;
    for line in txt.lines() {
        if let Some(v) = line.strip_prefix("LoadState=") {
            load = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("ActiveState=") {
            active = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("SubState=") {
            sub = Some(v.trim().to_string());
        }
    }

    let load = load?;
    let active = active.unwrap_or_else(|| "unknown".to_string());
    let sub = sub.unwrap_or_else(|| "unknown".to_string());

    if load == "not-found" {
        Some("not-found".to_string())
    } else {
        Some(format!("{active} ({sub})"))
    }
}

fn detect_kbd_backlight() -> bool {
    let Ok(entries) = std::fs::read_dir("/sys/class/leds") else {
        return false;
    };
    for ent in entries.flatten() {
        let Some(name) = ent.file_name().to_str().map(|s| s.to_ascii_lowercase()) else {
            continue;
        };
        if name.contains("kbd") || (name.contains("asus") && name.contains("keyboard")) {
            return true;
        }
    }
    false
}

fn backend_access_from_connect_error(
    connect_error: Option<&str>,
    missing_backend_reason: &'static str,
    temporarily_unavailable_reason: &'static str,
) -> FeatureAvailability {
    if connect_error.is_some() {
        FeatureAvailability::new(
            FeatureAccessState::TemporarilyUnavailable,
            temporarily_unavailable_reason,
        )
    } else {
        FeatureAvailability::new(FeatureAccessState::MissingBackend, missing_backend_reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_connect_status_maps_missing_backend_without_error() {
        let availability = backend_access_from_connect_error(
            None,
            "missing backend reason",
            "temporarily unavailable reason",
        );

        assert_eq!(availability.status, FeatureAccessState::MissingBackend);
        assert_eq!(availability.reason, "missing backend reason");
    }

    #[test]
    fn backend_connect_status_maps_error_to_temporarily_unavailable() {
        let availability = backend_access_from_connect_error(
            Some("dbus timeout"),
            "missing backend reason",
            "temporarily unavailable reason",
        );

        assert_eq!(
            availability.status,
            FeatureAccessState::TemporarilyUnavailable
        );
        assert_eq!(availability.reason, "temporarily unavailable reason");
    }
}
