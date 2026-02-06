use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use clap::{Parser, Subcommand};
use regex::Regex;
use rog_core::DeviceCaps;
use rog_providers::dbus;
use rog_providers::hwmon::HwmonTelemetryProvider;
use rog_providers::nvidia_smi::NvidiaSmiTelemetryProvider;
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
    info!("probing DeviceCaps (milestone 1)");

    let hwmon = HwmonTelemetryProvider::default();
    let snap = hwmon.read_snapshot().unwrap_or_else(|e| {
        warn!("hwmon failed: {e}");
        rog_core::TelemetrySnapshot::empty_now(0)
    });

    let mut caps = DeviceCaps::unknown();
    caps.has_fan_reading = !snap.fans_rpm.is_empty();
    caps.has_kbd_backlight = detect_kbd_backlight();
    caps.notes
        .push("Milestone 1: asusd/supergfxd not integrated yet.".to_string());

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
