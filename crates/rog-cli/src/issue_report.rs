//! Read-only, allow-listed diagnostic collection; never exports configuration or process telemetry.

use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;
use std::time::Duration;

use rog_core::{dbus_keys, BUILD_COMMIT};
use rog_providers::{display, hwmon::HwmonTelemetryProvider};
use zbus::zvariant::OwnedValue;

pub async fn print() -> anyhow::Result<()> {
    let mut report = String::from("# ROG Helper Issue Report\n\nRead-only collection. Serial numbers, hostname, user identity, saved profiles, process lists, and unrelated DBus content are omitted. Home paths and terminal control characters are redacted.\n\n");
    writeln!(report, "## Build and host\n\n- Package version: {}\n- Build base commit: {BUILD_COMMIT} (may include local edits)\n- Package route (inferred): {}\n- Distro: {}\n- Kernel: {}\n- Session: {}\n- Model: {}\n- Board: {}\n- Firmware: {}\n",
        env!("CARGO_PKG_VERSION"), package_route(), super::distro_name(),
        super::command_line("uname", &["-srmo"]), super::desktop_name(), super::machine_model(),
        super::read_trimmed("/sys/class/dmi/id/board_name"), super::read_trimmed("/sys/class/dmi/id/bios_version"))?;
    report.push_str("## Relevant services\n\n");
    writeln!(
        report,
        "- rog-helperd (user): {}",
        super::systemctl_user_unit_status("rog-helperd").unwrap_or_else(|| "unavailable".into())
    )?;
    for service in ["asusd", "supergfxd", "upower", "rog-helper-privileged"] {
        writeln!(
            report,
            "- {service}: {}",
            super::systemctl_unit_status(service).unwrap_or_else(|| "unavailable".into())
        )?;
    }

    match super::probe_device_caps().await {
        Ok((caps, lighting)) => {
            section(&mut report, "Independent provider preflight (best effort)", &format!("profiles: {}\nfan curves: {}\nfan telemetry: {}\ncharge limit: {}\nGPU modes: {}\nAura: {}\nkeyboard brightness: {}", caps.has_profiles, caps.has_fan_curves, caps.has_fan_reading, caps.has_charge_limit, caps.has_gpu_modes, caps.has_aura, caps.has_kbd_backlight));
            section(
                &mut report,
                "Lighting identity / descriptor / backend",
                &lighting_summary(&lighting),
            );
        }
        Err(error) => section(
            &mut report,
            "Capability / lighting probe error",
            &error.to_string(),
        ),
    }
    let hwmon = HwmonTelemetryProvider::default();
    match hwmon.fan_state() {
        Ok(state) => section(
            &mut report,
            "Fan identity / mapping / readback",
            &format!("{state:#?}"),
        ),
        Err(error) => section(&mut report, "Fan probe error", &error.to_string()),
    }

    match super::daemon_connection().await {
        Ok(connection) => match zbus::Proxy::new(
            &connection,
            "io.github.roghelper.Daemon",
            "/io/github/roghelper/Daemon",
            "io.github.roghelper.Daemon1",
        )
        .await
        {
            Ok(proxy) => {
                for (method, title, keys) in [
                    (
                        "GetDaemonInfo",
                        "Daemon identity",
                        &[
                            dbus_keys::daemon_info::API_VERSION,
                            dbus_keys::daemon_info::PACKAGE_VERSION,
                        ][..],
                    ),
                    (
                        "GetPrivilegedStatus",
                        "Helper API / readiness",
                        &[
                            dbus_keys::privileged_status::SYSTEM_BUS_CONNECTED,
                            dbus_keys::privileged_status::HELPER_INSTALLED,
                            dbus_keys::privileged_status::HELPER_REACHABLE,
                            dbus_keys::privileged_status::HELPER_COMPATIBLE,
                            dbus_keys::privileged_status::HELPER_VERSION,
                            dbus_keys::privileged_status::POLKIT_AVAILABLE,
                            dbus_keys::privileged_status::AUTHORIZATION_BACKEND,
                            dbus_keys::privileged_status::AUTHORIZATION_STATE,
                            dbus_keys::privileged_status::CATEGORIES_AVAILABLE,
                        ][..],
                    ),
                ] {
                    match session_map(&proxy, method).await {
                        Ok(map) => section(&mut report, title, &selected_fields(&map, keys)),
                        Err(error) => section(&mut report, title, &error),
                    }
                }
                match session_map(&proxy, "GetCaps").await {
                    Ok(caps) => section(
                        &mut report,
                        "Daemon capability matrix",
                        &selected_fields(
                            &caps,
                            &[
                                dbus_keys::caps::HAS_PROFILES,
                                dbus_keys::caps::HAS_FAN_CURVES,
                                dbus_keys::caps::HAS_FAN_READING,
                                dbus_keys::caps::HAS_CHARGE_LIMIT,
                                dbus_keys::caps::HAS_GPU_MODES,
                                dbus_keys::caps::HAS_AURA,
                                dbus_keys::caps::HAS_KBD_BACKLIGHT,
                                dbus_keys::caps::HAS_FAN_MANUAL_PERCENT,
                                dbus_keys::caps::HAS_FAN_MANUAL_RPM_TARGET,
                                dbus_keys::caps::HAS_INDIVIDUAL_FAN_CONTROL,
                                dbus_keys::caps::HAS_FAN_SYNC_CONTROL,
                                dbus_keys::caps::HAS_FAN_BOOST,
                                dbus_keys::caps::FAN_COUNT,
                                dbus_keys::caps::FAN_BACKEND,
                                dbus_keys::caps::GPU_BACKEND,
                                dbus_keys::caps::BATTERY_LIMIT_BACKEND,
                                dbus_keys::caps::BATTERY_LIMIT_DIRECT_WRITE,
                                dbus_keys::caps::BATTERY_LIMIT_PRIVILEGED_WRITE,
                                dbus_keys::caps::BATTERY_LIMIT_AUTHORIZATION,
                                dbus_keys::caps::CONTROL_PRIVILEGE_MATRIX,
                                dbus_keys::caps::GPU_SUPPORTED_MODES,
                                dbus_keys::caps::GPU_EXTERNAL_AUTHORIZATION,
                                dbus_keys::caps::GPU_SWITCH_STATE,
                                dbus_keys::caps::GPU_SWITCH_HINT,
                            ],
                        ),
                    ),
                    Err(error) => section(&mut report, "Daemon capability matrix", &error),
                }
                match session_map(&proxy, "GetState").await {
                    Ok(state) => {
                        let automation = state
                            .get(dbus_keys::state::AUTOMATION)
                            .cloned()
                            .and_then(|value| HashMap::<String, OwnedValue>::try_from(value).ok())
                            .unwrap_or_default();
                        section(
                            &mut report,
                            "Policy engine state",
                            &selected_fields(
                                &automation,
                                &[
                                    dbus_keys::state::AUTOMATION_STATE,
                                    dbus_keys::state::AUTOMATION_LAST_TRANSITION_MS,
                                ],
                            ),
                        );
                        section(
                            &mut report,
                            "Daemon warnings",
                            &format!("{} warning(s); inspect locally with diagnostics. Freeform provider errors are omitted from the shareable report.", state.get(dbus_keys::state::WARNINGS).cloned().and_then(|value| Vec::<String>::try_from(value).ok()).map_or(0, |warnings| warnings.len())),
                        );
                    }
                    Err(error) => section(&mut report, "Daemon state", &error),
                }
            }
            Err(error) => section(
                &mut report,
                "Session daemon unavailable",
                &error.to_string(),
            ),
        },
        Err(error) => section(&mut report, "Session bus unavailable", &error.to_string()),
    }
    section(
        &mut report,
        "Display backend (query only)",
        &format!("{:#?}", display::discover()),
    );
    section(&mut report, "Hotkey backend", "Desktop-owned custom shortcuts; manual CLI binding. No ROG Helper global registration or keyboard capture. Current mappings are not discoverable.");
    let home = std::env::var("HOME").ok();
    println!("{}", redact(&report, home.as_deref()));
    Ok(())
}

fn lighting_summary(lighting: &rog_core::LightingDiagnostics) -> String {
    // Do not export broad DBus discovery paths, names, interfaces or freeform errors.
    let mut summary = format!("backend: {:?}\ncapabilities: {:?}\nnative supported: {}\nphysical validation recorded: {}\nwrite readiness: {}", lighting.selected_backend_kind, lighting.capabilities, lighting.native_aura_hid_supported, lighting.physical_validation_recorded, lighting.native_write_readiness);
    for device in &lighting.native_aura_hid_devices {
        let _ = writeln!(summary, "\nHID: {} vendor={:?} product={:?} interface={:?} descriptor_sha256={:?} descriptor_bytes={:?} output_bytes={:?} supported={}", device.hidraw_name, device.vendor_id, device.product_id, device.interface_number, device.report_descriptor_sha256, device.report_descriptor_bytes, device.output_report_total_bytes, device.supported);
    }
    summary
}

async fn session_map(
    proxy: &zbus::Proxy<'_>,
    method: &str,
) -> Result<HashMap<String, OwnedValue>, String> {
    tokio::time::timeout(Duration::from_secs(3), proxy.call(method, &()))
        .await
        .map_err(|_| format!("{method}: timed out after 3 seconds"))?
        .map_err(|error| format!("{method}: unavailable ({error})"))
}

fn section(report: &mut String, title: &str, contents: &str) {
    let _ = writeln!(report, "\n## {title}\n\n```text\n{contents}\n```\n");
}

fn selected_fields(map: &HashMap<String, OwnedValue>, keys: &[&str]) -> String {
    if map.is_empty() {
        return "unavailable".into();
    }
    let mut entries = map
        .iter()
        .filter(|(key, _)| keys.contains(&key.as_str()))
        .collect::<Vec<_>>();
    entries.sort_by_key(|(key, _)| *key);
    entries
        .into_iter()
        .map(|(key, value)| format!("{key}: {value:?}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn package_route() -> &'static str {
    if std::env::var_os("FLATPAK_ID").is_some() {
        return "Flatpak sandbox";
    }
    if std::env::var_os("APPIMAGE").is_some() {
        return "AppImage";
    }
    let executable = std::env::current_exe().unwrap_or_default();
    if executable.starts_with(Path::new("/usr/bin")) {
        return "native /usr package (package manager unknown)";
    }
    if executable.starts_with(Path::new("/usr/local")) {
        return "local prefix / portable install";
    }
    "development or user-local executable (unmanaged)"
}

fn redact(report: &str, home: Option<&str>) -> String {
    let report = report
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .collect::<String>();
    let report = match home.filter(|path| path.len() > 1) {
        Some(path) => report.replace(path, "<HOME>"),
        None => report,
    };
    let homes = regex::Regex::new(r#"/home/[^/\s\"'<>]+|/root\b|/run/user/[0-9]+"#)
        .expect("static redaction expression is valid");
    homes.replace_all(&report, "<USER_PATH>").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_removes_home_and_control_bytes_without_destroying_system_paths() {
        assert_eq!(
            redact(
                "/home/alice/.config\n/sys/class/hwmon\u{1b}\u{0}",
                Some("/home/alice")
            ),
            "<HOME>/.config\n/sys/class/hwmon"
        );
        assert_eq!(redact("/sys/class", Some("/")), "/sys/class");
        assert_eq!(
            redact("/home/bob/config /root/config /run/user/1000/bus", None),
            "<USER_PATH>/config <USER_PATH>/config <USER_PATH>/bus"
        );
    }

    #[test]
    fn daemon_state_export_omits_unrelated_nested_process_and_configuration_content() {
        let mut state = HashMap::new();
        state.insert("telemetry".into(), OwnedValue::from(123u64));
        state.insert("configuration".into(), OwnedValue::from(456u64));
        state.insert("explanation".into(), OwnedValue::from(789u64));
        state.insert("warnings".into(), OwnedValue::from(1u64));
        let output = selected_fields(&state, &["warnings"]);
        assert!(output.contains("warnings"));
        assert!(!output.contains("telemetry"));
        assert!(!output.contains("configuration"));
        assert!(!output.contains("explanation"));
    }
}
#[test]
fn lighting_summary_omits_unrelated_bus_discovery_and_errors() {
    let mut lighting = rog_core::LightingDiagnostics::unknown();
    lighting
        .asusd_services_checked
        .push("private-service".into());
    lighting
        .asusd_object_paths_checked
        .push("private-path".into());
    lighting.probe_errors.push("private-error".into());
    let output = lighting_summary(&lighting);
    assert!(!output.contains("private-"));
    assert!(output.contains("backend:"));
}
