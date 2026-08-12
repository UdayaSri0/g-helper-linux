use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use clap::{Parser, Subcommand};
use regex::Regex;
use rog_core::{
    DeviceCaps, FanCaps, FanInfo, FeatureAccessState, FeatureAvailability, LightingDiagnostics,
};
use rog_providers::asusd::AsusdPlatformProvider;
use rog_providers::aura::{AuraProbeDiagnostics, AuraProvider};
use rog_providers::cpu::CpuTelemetryProvider;
use rog_providers::dbus;
use rog_providers::hwmon::HwmonTelemetryProvider;
use rog_providers::kbd_backlight::KbdBacklightSysfs;
use rog_providers::lighting::build_lighting_diagnostics;
use rog_providers::nvidia_smi::{NvidiaSmiProbe, NvidiaSmiTelemetryProvider};
use rog_providers::setup::{probe_setup_status, RogHelperdProbe};
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
    /// Check service readiness, hardware access, and write permissions.
    SetupCheck,
    /// Print presence of key services (DBus + systemd).
    Services,
    /// List and optionally introspect system DBus services.
    Dbus {
        /// Regex used to filter bus names (case-insensitive).
        #[arg(
            long,
            default_value = "asus|rog|aura|kbd|keyboard|led|rgb|supergfx|power|upower"
        )]
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
    /// Print detected fan telemetry and control support.
    Fans,
    /// Print fan capability summary only.
    FanCaps,
    /// Print keyboard lighting and RGB/Aura diagnostics only.
    Lighting,
    /// Print keyboard lighting and RGB/Aura diagnostics only.
    LightingDiagnostics,
    /// Print a read-only Markdown record for hardware validation.
    HardwareReport,
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
        Cmd::SetupCheck => cmd_setup_check().await?,
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
        Cmd::Fans => cmd_fans().await?,
        Cmd::FanCaps => cmd_fan_caps().await?,
        Cmd::Lighting | Cmd::LightingDiagnostics => cmd_lighting_diagnostics().await?,
        Cmd::HardwareReport => cmd_hardware_report().await?,
    }

    Ok(())
}

async fn cmd_setup_check() -> anyhow::Result<()> {
    let hwmon = HwmonTelemetryProvider::default();
    let fan_caps = hwmon.fan_caps().unwrap_or_else(|error| {
        warn!("fan capability probe failed during setup check: {error}");
        FanCaps::from_fans(&[])
    });
    let cpu_caps = CpuTelemetryProvider::default().probe_caps();
    let lighting_probe = probe_lighting_diagnostics().await;

    let mut caps = DeviceCaps::unknown();
    caps.has_aura = lighting_probe.aura.is_some();
    caps.has_kbd_backlight = lighting_probe.kbd_detected;
    let sysfs_writable = lighting_probe
        .kbd_backlight
        .as_ref()
        .map(KbdBacklightSysfs::can_set_brightness)
        .unwrap_or(false);
    caps.kbd_backlight_access = lighting_access_from_backend_flags(
        caps.has_aura,
        lighting_probe.kbd_backlight.is_some(),
        sysfs_writable,
        lighting_probe.kbd_detected,
        lighting_probe.kbd_probe_error.as_deref(),
    );
    let keyboard_paths = lighting_probe
        .kbd_backlight
        .as_ref()
        .map(|kbd| vec![kbd.brightness_path().display().to_string()])
        .unwrap_or_default();

    let status = probe_setup_status(
        RogHelperdProbe::ProbeSession,
        &caps,
        &cpu_caps,
        &fan_caps,
        keyboard_paths,
    )
    .await;
    println!("{}", status.to_report_text(true));
    Ok(())
}

async fn cmd_services() -> anyhow::Result<()> {
    let re = Regex::new("(?i)asus|rog|aura|kbd|keyboard|led|rgb|supergfx|power|upower")?;
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
    let nvidia = NvidiaSmiTelemetryProvider::default();
    if let Ok(Some(sample)) = nvidia.read_telemetry().await {
        if let Some(temp) = sample.temperature_c {
            snap.temps_c.insert("nvidia-smi:gpu_temp".to_string(), temp);
            snap.gpu_temp_c.get_or_insert(temp);
        }
        snap.gpu_usage_percent = sample.usage_percent;
        snap.gpu_vram_used_bytes = sample.vram_used_bytes;
        snap.gpu_vram_total_bytes = sample.vram_total_bytes;
        snap.gpu_core_clock_mhz = sample.core_clock_mhz;
        snap.gpu_memory_clock_mhz = sample.memory_clock_mhz;
        snap.gpu_power_w = sample.power_w;
        snap.gpu_name = Some(sample.name);
        snap.gpu_uuid = Some(sample.uuid);
        snap.gpu_pci_bus_id = Some(sample.pci_bus_id);
        snap.gpu_index = Some(sample.index);
        snap.gpu_telemetry_provider = Some("nvidia-smi".to_string());
    }
    println!("  timestamp_ms: {}", snap.timestamp_ms);
    println!("  cpu_temp_c: {:?}", snap.cpu_temp_c);
    println!("  gpu_temp_c: {:?}", snap.gpu_temp_c);
    println!("  gpu_usage_percent: {:?}", snap.gpu_usage_percent);
    println!("  gpu_vram_used_bytes: {:?}", snap.gpu_vram_used_bytes);
    println!("  gpu_vram_total_bytes: {:?}", snap.gpu_vram_total_bytes);
    println!("  gpu_core_clock_mhz: {:?}", snap.gpu_core_clock_mhz);
    println!("  gpu_memory_clock_mhz: {:?}", snap.gpu_memory_clock_mhz);
    println!("  gpu_power_w: {:?}", snap.gpu_power_w);
    println!("  gpu_name: {:?}", snap.gpu_name);
    println!("  gpu_uuid: {:?}", snap.gpu_uuid);
    println!("  temps: {}", snap.temps_c.len());
    println!("  fans: {}", snap.fans_rpm.len());
    for fan in &snap.fan_rows {
        println!(
            "    {}: {} ({})",
            fan.display_label,
            fan.rpm
                .map(|rpm| format!("{rpm} rpm"))
                .unwrap_or_else(|| "RPM unavailable".to_string()),
            fan.input_path
        );
    }

    Ok(())
}

async fn cmd_fans() -> anyhow::Result<()> {
    let provider = HwmonTelemetryProvider::default();
    let fans = provider.list_fans().context("list fans")?;
    print_fan_caps(&provider.fan_caps().context("probe fan caps")?);
    println!();
    println!("Detected fans:");
    if fans.is_empty() {
        println!("  (none)");
        println!("  No fan hwmon inputs are exposed. This can be normal on some laptops/kernels.");
    }
    for fan in &fans {
        print_fan_info(fan);
    }
    println!();
    println!("Notes:");
    println!("  Diagnostics are read-only and do not require root. Candidate PWM/curve files do not authorize fan writes; a backend-specific safety and restore contract must be verified first.");
    Ok(())
}

async fn cmd_fan_caps() -> anyhow::Result<()> {
    let provider = HwmonTelemetryProvider::default();
    let caps = provider.fan_caps().context("probe fan caps")?;
    print_fan_caps(&caps);
    Ok(())
}

async fn cmd_caps() -> anyhow::Result<()> {
    let (caps, lighting_diagnostics) = probe_device_caps().await?;
    println!("{caps:#?}");
    println!();
    println!("{}", lighting_diagnostics.to_report_text());
    Ok(())
}

async fn probe_device_caps() -> anyhow::Result<(DeviceCaps, LightingDiagnostics)> {
    info!("probing DeviceCaps");

    let hwmon = HwmonTelemetryProvider::default();
    let snap = hwmon.read_snapshot().unwrap_or_else(|e| {
        warn!("hwmon failed: {e}");
        rog_core::TelemetrySnapshot::empty_now(0)
    });
    let lighting_probe = probe_lighting_diagnostics().await;

    let mut caps = DeviceCaps::unknown();
    caps.has_fan_reading = !snap.fans_rpm.is_empty();
    if let Ok(fan_caps) = hwmon.fan_caps() {
        caps.has_fan_reading = fan_caps.has_fan_reading;
        caps.has_fan_curves = fan_caps.has_fan_curves;
        caps.has_fan_manual_percent = fan_caps.has_fan_manual_percent;
        caps.has_fan_manual_rpm_target = fan_caps.has_fan_manual_rpm_target;
        caps.has_individual_fan_control = fan_caps.has_individual_fan_control;
        caps.has_fan_sync_control = fan_caps.has_fan_sync_control;
        caps.has_fan_boost = fan_caps.has_fan_boost;
        caps.fan_count = fan_caps.fan_count;
        caps.fan_backend = fan_caps.fan_backend;
        for endpoint in fan_caps.endpoints {
            caps.endpoints.push(format!("fan:{endpoint}"));
        }
        caps.notes.extend(fan_caps.notes);
        caps.notes.extend(fan_caps.warnings);
    }
    caps.has_kbd_backlight = lighting_probe.kbd_detected;
    caps.has_aura = lighting_probe.aura.is_some();
    let sysfs_writable = lighting_probe
        .kbd_backlight
        .as_ref()
        .map(KbdBacklightSysfs::can_set_brightness)
        .unwrap_or(false);
    caps.kbd_backlight_access = lighting_access_from_backend_flags(
        caps.has_aura,
        lighting_probe.kbd_backlight.is_some(),
        sysfs_writable,
        caps.has_kbd_backlight,
        lighting_probe.kbd_probe_error.as_deref(),
    );
    if let Some(aura) = &lighting_probe.aura {
        caps.endpoints.push(aura.endpoint_tag());
    }
    if let Some(err) = &lighting_probe.aura_probe_error {
        caps.notes.push(format!("asusd Aura probe failed: {err}"));
    }
    caps.notes.extend(lighting_probe.aura_notes.clone());
    caps.notes.push(lighting_probe.diagnostics.summary_line());
    if let Some(warning) = &lighting_probe.diagnostics.permission_warning {
        caps.notes.push(warning.clone());
    }
    if let Some(reason) = &lighting_probe.diagnostics.unavailable_reason {
        caps.notes.push(reason.clone());
    }
    if let Some(action) = &lighting_probe.diagnostics.recommended_action {
        caps.notes
            .push(format!("Lighting recommended action: {action}"));
    }
    if let Some(kbd) = &lighting_probe.kbd_backlight {
        caps.endpoints.push(format!("sysfs-led:{}", kbd.led_name()));
        caps.notes.push(format!(
            "Keyboard backlight via {} (max {}, writable={}).",
            kbd.led_name(),
            kbd.max_brightness(),
            kbd.can_set_brightness()
        ));
    }
    if let Some(err) = &lighting_probe.kbd_probe_error {
        caps.notes
            .push(format!("keyboard backlight probing failed: {err}"));
    }

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
    let re = Regex::new("(?i)asus|rog|aura|kbd|keyboard|led|rgb|supergfx|power|upower")?;
    if let Ok(names) = dbus::list_system_bus_names_matching(&re).await {
        for n in names {
            caps.endpoints.push(format!("system-bus-name:{n}"));
        }
    }

    Ok((caps, lighting_probe.diagnostics))
}

async fn cmd_hardware_report() -> anyhow::Result<()> {
    let (caps, lighting) = probe_device_caps().await?;
    let hwmon = HwmonTelemetryProvider::default();
    let fan_caps = hwmon.fan_caps().unwrap_or_else(|_| FanCaps::from_fans(&[]));
    let fans = hwmon.list_fans().unwrap_or_default();
    let cpu_caps = CpuTelemetryProvider::default().probe_caps();
    let keyboard_paths = lighting
        .keyboard_backlight_brightness_path
        .clone()
        .into_iter()
        .collect();
    let setup = probe_setup_status(
        RogHelperdProbe::ProbeSession,
        &caps,
        &cpu_caps,
        &fan_caps,
        keyboard_paths,
    )
    .await;
    let nvidia = NvidiaSmiTelemetryProvider::default();
    let nvidia_status = match nvidia.probe().await {
        NvidiaSmiProbe::Available { gpu_names } => {
            format!("available ({})", gpu_names.join(", "))
        }
        NvidiaSmiProbe::NotFound => "not installed".to_string(),
        NvidiaSmiProbe::Unavailable(reason) => format!("unavailable ({reason})"),
    };
    let nvidia_sample = nvidia.read_telemetry().await.ok().flatten();

    println!("# rog-helper Hardware Validation Report");
    println!();
    println!("Generated by `rog-helper hardware-report` using read-only probes.");
    println!();
    println!("## Metadata");
    println!();
    println!(
        "- Tested date: {}",
        command_line("date", &["--iso-8601=seconds"])
    );
    println!("- rog-helper version: {}", env!("CARGO_PKG_VERSION"));
    println!("- Tester: (fill in)");
    println!();
    println!("## Machine");
    println!();
    println!("- Model: {}", machine_model());
    println!("- Distro: {}", distro_name());
    println!("- Kernel: {}", command_line("uname", &["-srmo"]));
    println!("- Desktop environment: {}", desktop_name());
    println!("- CPU: {}", cpu_name());
    println!("- GPU: {}", gpu_name(nvidia_sample.as_ref()));
    println!(
        "- BIOS / firmware: {}",
        read_trimmed("/sys/class/dmi/id/bios_version")
    );
    println!(
        "- `asusd` version: {}",
        first_available_command(&[("asusctl", &["--version"]), ("asusd", &["--version"])])
    );
    println!(
        "- `supergfxd` version: {}",
        first_available_command(&[
            ("supergfxctl", &["--version"]),
            ("supergfxd", &["--version"])
        ])
    );
    println!();
    println!("## Service Status");
    println!();
    println!(
        "- `rog-helperd`: {}",
        systemctl_user_unit_status("rog-helperd")
            .unwrap_or_else(|| "systemctl --user unavailable".to_string())
    );
    for service in ["asusd", "supergfxd", "upower"] {
        println!(
            "- `{service}`: {}",
            systemctl_unit_status(service).unwrap_or_else(|| "systemctl unavailable".to_string())
        );
    }
    println!("- `nvidia-smi`: {nvidia_status}");
    println!();
    println!("## Capability Summary");
    println!();
    println!("```text");
    println!("{caps:#?}");
    println!("```");
    println!();
    println!("## Sensors and Fan Mapping");
    println!();
    println!("- Fan backend: {}", fan_caps.fan_backend);
    println!("- Fan count: {}", fan_caps.fan_count);
    println!(
        "- Fan mapping confidence: {}",
        fan_caps.fan_mapping_confidence.as_str()
    );
    if fans.is_empty() {
        println!("- Fan endpoints: none detected");
    } else {
        for fan in &fans {
            println!(
                "- {} (`{}`): RPM {}, mapping {}, controllable {}, endpoints {}",
                fan.label,
                fan.id,
                fan.current_rpm
                    .map(|rpm| rpm.to_string())
                    .unwrap_or_else(|| "unavailable".to_string()),
                fan.mapping_confidence.as_str(),
                fan.controllable,
                fan.endpoints.join(", ")
            );
        }
    }
    println!();
    println!("## Permission and Dependency States");
    println!();
    println!("```text");
    println!("{}", setup.to_report_text(true));
    println!("```");
    println!();
    println!("## Lighting Backend");
    println!();
    println!("```text");
    println!("{}", lighting.to_report_text());
    println!("```");
    println!();
    println!("## Manual Results (fill in after testing)");
    println!();
    for field in [
        "Session daemon startup",
        "UI startup",
        "Tray visibility",
        "Session DBus introspection",
        "Profile control",
        "GPU mode control",
        "Battery limit",
        "Keyboard backlight",
        "CPU telemetry",
        "CPU write controls",
        "Unsupported or unverified items",
    ] {
        println!("- {field}:");
    }
    println!();
    println!("This report records observations only; it does not certify write support.");
    Ok(())
}

fn read_trimmed(path: &str) -> String {
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unavailable".to_string())
}

fn command_line(program: &str, args: &[&str]) -> String {
    let Ok(output) = Command::new(program).args(args).output() else {
        return "missing".to_string();
    };
    if !output.status.success() {
        return "unavailable".to_string();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("unavailable")
        .to_string()
}

fn first_available_command(commands: &[(&str, &[&str])]) -> String {
    commands
        .iter()
        .map(|(program, args)| command_line(program, args))
        .find(|value| value != "missing" && value != "unavailable")
        .unwrap_or_else(|| "missing".to_string())
}

fn machine_model() -> String {
    let parts = [
        read_trimmed("/sys/class/dmi/id/sys_vendor"),
        read_trimmed("/sys/class/dmi/id/product_name"),
        read_trimmed("/sys/class/dmi/id/product_version"),
    ];
    let model = parts
        .into_iter()
        .filter(|value| value != "unavailable")
        .collect::<Vec<_>>()
        .join(" ");
    if model.is_empty() {
        "unavailable".to_string()
    } else {
        model
    }
}

fn distro_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                line.strip_prefix("PRETTY_NAME=")
                    .map(|value| value.trim_matches('"').to_string())
            })
        })
        .unwrap_or_else(|| "unavailable".to_string())
}

fn desktop_name() -> String {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("DESKTOP_SESSION"))
        .unwrap_or_else(|_| "unavailable".to_string());
    let session =
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown session".to_string());
    format!("{desktop} ({session})")
}

fn cpu_name() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                line.strip_prefix("model name")
                    .and_then(|line| line.split_once(':'))
                    .map(|(_, value)| value.trim().to_string())
            })
        })
        .unwrap_or_else(|| "unavailable".to_string())
}

fn gpu_name(nvidia: Option<&rog_providers::nvidia_smi::NvidiaGpuTelemetry>) -> String {
    if let Some(sample) = nvidia {
        return format!("{} ({})", sample.name, sample.pci_bus_id);
    }
    let Ok(output) = Command::new("lspci").output() else {
        return "unavailable".to_string();
    };
    if !output.status.success() {
        return "unavailable".to_string();
    }
    let gpus = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("vga compatible controller")
                || lower.contains("3d controller")
                || lower.contains("display controller")
        })
        .collect::<Vec<_>>()
        .join("; ");
    if gpus.is_empty() {
        "unavailable".to_string()
    } else {
        gpus
    }
}

fn print_fan_caps(caps: &FanCaps) {
    println!("Fan capabilities:");
    println!("  backend: {}", caps.fan_backend);
    println!("  fan_count: {}", caps.fan_count);
    println!("  has_fan_reading: {}", caps.has_fan_reading);
    println!("  has_fan_manual_percent: {}", caps.has_fan_manual_percent);
    println!(
        "  has_fan_manual_rpm_target: {}",
        caps.has_fan_manual_rpm_target
    );
    println!("  has_fan_curves: {}", caps.has_fan_curves);
    println!("  fan_curve_readable: {}", caps.fan_curve_readable);
    println!("  fan_curve_writable: {}", caps.fan_curve_writable);
    println!(
        "  fan_mapping_confidence: {}",
        caps.fan_mapping_confidence.as_str()
    );
    println!(
        "  has_individual_fan_control: {}",
        caps.has_individual_fan_control
    );
    println!("  has_fan_sync_control: {}", caps.has_fan_sync_control);
    println!("  has_fan_boost: {}", caps.has_fan_boost);
    if !caps.endpoints.is_empty() {
        println!("  endpoints:");
        for endpoint in &caps.endpoints {
            println!("    {endpoint}");
        }
    }
    if !caps.notes.is_empty() {
        println!("  notes:");
        for note in &caps.notes {
            println!("    {note}");
        }
    }
    if !caps.warnings.is_empty() {
        println!("  warnings:");
        for warning in &caps.warnings {
            println!("    {warning}");
        }
    }
}

fn print_fan_info(fan: &FanInfo) {
    println!("  {} [{}]", fan.label, fan.id);
    println!(
        "    rpm: {}",
        fan.current_rpm
            .map(|rpm| rpm.to_string())
            .unwrap_or_else(|| "(unavailable)".to_string())
    );
    println!(
        "    percent: {}",
        fan.current_percent
            .map(|percent| format!("{percent}%"))
            .unwrap_or_else(|| "(not reported)".to_string())
    );
    println!("    backend: {}", fan.backend);
    println!("    mapping: {}", fan.mapping_confidence.as_str());
    println!("    controllable: {}", fan.controllable);
    println!("    manual_percent: {}", fan.supports_manual_percent);
    println!("    rpm_target: {}", fan.supports_manual_rpm_target);
    println!("    curve: {}", fan.supports_curve);
    println!("    auto_restore: {}", fan.supports_auto);
    if !fan.endpoints.is_empty() {
        println!("    endpoints:");
        for endpoint in &fan.endpoints {
            println!("      {endpoint}");
        }
    }
    for note in &fan.notes {
        println!("    note: {note}");
    }
    for warning in &fan.warnings {
        println!("    warning: {warning}");
    }
}

async fn cmd_lighting_diagnostics() -> anyhow::Result<()> {
    let lighting_probe = probe_lighting_diagnostics().await;
    println!("{}", lighting_probe.diagnostics.to_report_text());
    Ok(())
}

#[derive(Debug)]
struct LightingProbe {
    kbd_backlight: Option<KbdBacklightSysfs>,
    kbd_detected: bool,
    kbd_probe_error: Option<String>,
    aura: Option<AuraProvider>,
    aura_probe_error: Option<String>,
    aura_notes: Vec<String>,
    diagnostics: LightingDiagnostics,
}

async fn probe_lighting_diagnostics() -> LightingProbe {
    let (kbd_backlight, kbd_probe_error) = match KbdBacklightSysfs::probe() {
        Ok(v) => (v, None),
        Err(e) => {
            warn!("kbd backlight probe failed: {e}");
            (None, Some(e.to_string()))
        }
    };
    let kbd_detected = kbd_backlight.is_some() || detect_kbd_backlight();

    let (aura, aura_probe, aura_notes, aura_probe_error) = match AuraProvider::probe_system().await
    {
        Ok(probe) => (probe.provider, probe.diagnostics, probe.notes, None),
        Err(e) => {
            warn!("asusd Aura probe failed: {e}");
            let mut diagnostics = AuraProbeDiagnostics::default();
            diagnostics.probe_errors.push(e.to_string());
            (
                None,
                diagnostics,
                vec!["asusd Aura probe could not run to completion.".to_string()],
                Some(e.to_string()),
            )
        }
    };

    let (aura_state, aura_state_error) = if let Some(aura) = &aura {
        match aura.read_state().await {
            Ok(state) => (Some(state), None),
            Err(e) => (None, Some(e.to_string())),
        }
    } else {
        (None, None)
    };

    let diagnostics = build_lighting_diagnostics(
        kbd_backlight.as_ref(),
        kbd_detected,
        kbd_probe_error.as_deref(),
        aura.as_ref(),
        &aura_probe,
        aura_state.as_ref(),
        aura_state_error.as_deref(),
    );

    LightingProbe {
        kbd_backlight,
        kbd_detected,
        kbd_probe_error,
        aura,
        aura_probe_error,
        aura_notes,
        diagnostics,
    }
}

fn systemctl_unit_status(service: &str) -> Option<String> {
    systemctl_unit_status_with_scope(service, false)
}

fn systemctl_user_unit_status(service: &str) -> Option<String> {
    systemctl_unit_status_with_scope(service, true)
}

fn systemctl_unit_status_with_scope(service: &str, user: bool) -> Option<String> {
    let mut command = Command::new("systemctl");
    if user {
        command.arg("--user");
    }
    let output = command
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

fn lighting_access_from_backend_flags(
    aura_available: bool,
    sysfs_available: bool,
    sysfs_writable: bool,
    sysfs_detected: bool,
    sysfs_probe_error: Option<&str>,
) -> FeatureAvailability {
    if aura_available {
        return FeatureAvailability::new(
            FeatureAccessState::Available,
            "Keyboard lighting is available through ASUS Aura/RGB.",
        );
    }

    if sysfs_available {
        if sysfs_writable {
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
    } else if sysfs_probe_error.is_some() || sysfs_detected {
        FeatureAvailability::new(
            FeatureAccessState::TemporarilyUnavailable,
            "Keyboard backlight hardware was detected, but the backend could not be opened right now.",
        )
    } else {
        FeatureAvailability::new(
            FeatureAccessState::Unsupported,
            "No keyboard lighting control was detected on this machine.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_check_is_a_first_class_cli_command() {
        let cli = Cli::try_parse_from(["rog-helper", "setup-check"]).unwrap();
        assert!(matches!(cli.cmd, Cmd::SetupCheck));
    }

    #[test]
    fn hardware_report_is_a_first_class_cli_command() {
        let cli = Cli::try_parse_from(["rog-helper", "hardware-report"]).unwrap();
        assert!(matches!(cli.cmd, Cmd::HardwareReport));
    }

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

    #[test]
    fn lighting_access_fallbacks_match_provider_priority() {
        assert_eq!(
            lighting_access_from_backend_flags(true, false, false, false, None).status,
            FeatureAccessState::Available
        );
        assert_eq!(
            lighting_access_from_backend_flags(false, true, true, true, None).status,
            FeatureAccessState::Available
        );
        assert_eq!(
            lighting_access_from_backend_flags(false, false, false, false, None).status,
            FeatureAccessState::Unsupported
        );
    }
}
