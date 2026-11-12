//! Optional, read-only SMART data from smartmontools.

use std::{
    collections::HashMap,
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use rog_core::storage::{StorageAttribute, StorageDevice, StorageHealth};
use serde_json::Value;
use zbus::Proxy;
use zvariant::{OwnedObjectPath, OwnedValue};

const MAX_OUTPUT: u64 = 256 * 1024;
const TIMEOUT: Duration = Duration::from_secs(3);
const CACHE_TIMEOUT: Duration = Duration::from_secs(2);
const CACHE_MAX_AGE: Duration = Duration::from_secs(20 * 60);

/// Reads UDisks2's existing SMART cache without requesting a hardware refresh.
pub async fn read_cached_udisks_health(device: &StorageDevice) -> Option<StorageHealth> {
    if !valid_device_name(&device.name) {
        return None;
    }
    tokio::time::timeout(CACHE_TIMEOUT, read_cached_inner(device))
        .await
        .ok()
        .flatten()
}

async fn read_cached_inner(device: &StorageDevice) -> Option<StorageHealth> {
    let connection = zbus::Connection::system().await.ok()?;
    let block_path = format!("/org/freedesktop/UDisks2/block_devices/{}", device.name);
    let block = Proxy::new(
        &connection,
        "org.freedesktop.UDisks2",
        block_path.as_str(),
        "org.freedesktop.UDisks2.Block",
    )
    .await
    .ok()?;
    let drive_path: OwnedObjectPath = block.get_property("Drive").await.ok()?;
    if drive_path.as_str() == "/" {
        return None;
    }

    let nvme = Proxy::new(
        &connection,
        "org.freedesktop.UDisks2",
        drive_path.clone(),
        "org.freedesktop.UDisks2.NVMe.Controller",
    )
    .await
    .ok()?;
    if let Ok(updated) = nvme.get_property::<u64>("SmartUpdated").await {
        if updated != 0 {
            let warnings: Vec<String> = nvme.get_property("SmartCriticalWarning").await.ok()?;
            let mut health = cached_health(updated, warnings.is_empty());
            for warning in warnings {
                health.attributes.push(StorageAttribute {
                    key: "critical_warning".into(),
                    value: warning,
                    unit: None,
                    warning: true,
                });
            }
            if health.state == "critical" {
                health.reason = "NVMe controller reports a critical warning".into();
            }
            if let Ok(hours) = nvme.get_property::<u64>("SmartPowerOnHours").await {
                health.attributes.push(StorageAttribute {
                    key: "power_on_hours".into(),
                    value: hours.to_string(),
                    unit: Some("h".into()),
                    warning: false,
                });
            }
            // SmartGetAttributes reads UDisks' cached log; SmartUpdate is the hardware refresh.
            if let Ok(attributes) = nvme
                .call::<_, _, HashMap<String, OwnedValue>>(
                    "SmartGetAttributes",
                    &HashMap::<String, OwnedValue>::new(),
                )
                .await
            {
                for (key, unit) in [
                    ("avail_spare", Some("%")),
                    ("spare_thresh", Some("%")),
                    ("percent_used", Some("%")),
                ] {
                    if let Some(value) = attributes
                        .get(key)
                        .and_then(|value| u8::try_from(value.clone()).ok())
                    {
                        health.attributes.push(StorageAttribute {
                            key: key.into(),
                            value: value.to_string(),
                            unit: unit.map(str::to_string),
                            warning: false,
                        });
                    }
                }
                for (key, unit) in [
                    ("total_data_read", Some("bytes")),
                    ("total_data_written", Some("bytes")),
                    ("ctrl_busy_time", Some("min")),
                    ("power_cycles", None),
                    ("unsafe_shutdowns", None),
                    ("media_errors", None),
                    ("num_err_log_entries", None),
                ] {
                    if let Some(value) = attributes
                        .get(key)
                        .and_then(|value| u64::try_from(value.clone()).ok())
                    {
                        health.attributes.push(StorageAttribute {
                            key: key.into(),
                            value: value.to_string(),
                            unit: unit.map(str::to_string),
                            warning: key == "media_errors" && value > 0,
                        });
                        if key == "media_errors" && value > 0 && health.state == "healthy" {
                            health.state = "attention".into();
                            health.reason = "NVMe media errors reported".into();
                        }
                    }
                }
            }
            return Some(health);
        }
    }

    let ata = Proxy::new(
        &connection,
        "org.freedesktop.UDisks2",
        drive_path,
        "org.freedesktop.UDisks2.Drive.Ata",
    )
    .await
    .ok()?;
    if !ata.get_property::<bool>("SmartSupported").await.ok()? {
        return Some(StorageHealth {
            state: "unsupported".into(),
            source: "udisks2".into(),
            reason: "ATA SMART is unsupported".into(),
            ..StorageHealth::default()
        });
    }
    let updated: u64 = ata.get_property("SmartUpdated").await.ok()?;
    if updated == 0 {
        return None;
    }
    let failing: bool = ata.get_property("SmartFailing").await.ok()?;
    let mut health = cached_health(updated, !failing);
    let failing_count = ata
        .get_property::<i32>("SmartNumAttributesFailing")
        .await
        .ok();
    let bad_sectors = ata.get_property::<i64>("SmartNumBadSectors").await.ok();
    apply_ata_cache_warnings(&mut health, failing, failing_count, bad_sectors);
    Some(health)
}

fn apply_ata_cache_warnings(
    health: &mut StorageHealth,
    failing: bool,
    failing_count: Option<i32>,
    bad_sectors: Option<i64>,
) {
    let stale = health.state == "stale";
    if failing && !stale {
        health.reason = "ATA SMART reports impending failure".into();
    }
    if let Some(count) = failing_count.filter(|count| *count > 0) {
        if !stale {
            health.state = "critical".into();
            health.reason = "ATA SMART attributes are failing".into();
        }
        health.attributes.push(StorageAttribute {
            key: "attributes_failing".into(),
            value: count.to_string(),
            unit: None,
            warning: true,
        });
    }
    if let Some(count) = bad_sectors.filter(|count| *count > 0) {
        if !stale && health.state != "critical" {
            health.state = "attention".into();
            health.reason = "ATA SMART reports bad sectors".into();
        }
        health.attributes.push(StorageAttribute {
            key: "bad_sectors".into(),
            value: count.to_string(),
            unit: None,
            warning: true,
        });
    }
}

fn cached_health(updated_seconds: u64, passing: bool) -> StorageHealth {
    let sampled_at_ms = updated_seconds.saturating_mul(1000);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let age = now.as_secs().saturating_sub(updated_seconds);
    let stale = age > CACHE_MAX_AGE.as_secs() || updated_seconds > now.as_secs() + 300;
    StorageHealth {
        state: if stale {
            "stale"
        } else if passing {
            "healthy"
        } else {
            "critical"
        }
        .into(),
        source: "udisks2".into(),
        sampled_at_ms: Some(sampled_at_ms),
        reason: if stale {
            "Cached SMART reading is stale"
        } else if passing {
            "Cached SMART health reports no current warning"
        } else {
            "Cached SMART health reports a critical warning"
        }
        .into(),
        ..StorageHealth::default()
    }
}

pub fn read_health(device: &StorageDevice) -> StorageHealth {
    if !valid_device_name(&device.name) {
        return unavailable("unknown", "Invalid block device name");
    }
    // shortcut: rotational SMART uses the UDisks cache only; add explicit on-demand checks when requested.
    if device.rotational {
        return unavailable(
            "unknown",
            "Background SMART check skipped for rotational drive",
        );
    }

    let executable = ["/usr/sbin/smartctl", "/usr/bin/smartctl"]
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file())
        .unwrap_or("smartctl");
    let path = format!("/dev/{}", device.name);
    let mut child = match Command::new(executable)
        .args([
            "--json",
            "--health",
            "--attributes",
            "-n",
            "standby,0",
            &path,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return unavailable("unsupported", "smartctl is not installed")
        }
        Err(_) => return unavailable("unknown", "SMART backend could not start"),
    };

    let stdout = child.stdout.take().expect("piped stdout");
    let reader = thread::spawn(move || {
        let mut output = Vec::new();
        let result = stdout.take(MAX_OUTPUT + 1).read_to_end(&mut output);
        (result, output)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() < TIMEOUT => thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let Ok((Ok(_), output)) = reader.join() else {
        return unavailable("unknown", "SMART output could not be read");
    };
    if output.len() as u64 > MAX_OUTPUT {
        return unavailable("unknown", "SMART output exceeded the limit");
    }
    let Some(status) = status else {
        return unavailable("unknown", "SMART query timed out");
    };
    let Ok(json) = serde_json::from_slice::<Value>(&output) else {
        return unavailable("unknown", "SMART output was malformed");
    };
    parse_health(&json, status.code().unwrap_or(1) as u8)
}

fn valid_device_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn unavailable(state: &str, reason: &str) -> StorageHealth {
    StorageHealth {
        state: state.into(),
        source: "smartctl".into(),
        reason: reason.into(),
        ..StorageHealth::default()
    }
}

fn parse_health(json: &Value, exit: u8) -> StorageHealth {
    if exit & 1 != 0 {
        return unavailable("unknown", "SMART query was rejected");
    }
    if exit & 2 != 0 {
        let denied = json
            .pointer("/smartctl/messages")
            .and_then(Value::as_array)
            .is_some_and(|messages| {
                messages.iter().any(|message| {
                    message
                        .get("string")
                        .and_then(Value::as_str)
                        .is_some_and(|text| {
                            text.contains("Permission denied")
                                || text.contains("Operation not permitted")
                        })
                })
            });
        return unavailable(
            if denied {
                "permission_denied"
            } else {
                "unknown"
            },
            "SMART device could not be opened",
        );
    }
    let mut health = StorageHealth {
        state: "unknown".into(),
        source: "smartctl".into(),
        sampled_at_ms: Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        ),
        ..StorageHealth::default()
    };
    let passed = json
        .pointer("/smart_status/passed")
        .and_then(Value::as_bool);
    let nvme = json.get("nvme_smart_health_information_log");
    if let Some(log) = nvme {
        for (key, unit) in [
            ("critical_warning", None),
            ("available_spare", Some("%")),
            ("available_spare_threshold", Some("%")),
            ("percentage_used", Some("%")),
            ("data_units_read", None),
            ("data_units_written", None),
            ("host_reads", None),
            ("host_writes", None),
            ("controller_busy_time", Some("min")),
            ("power_cycles", None),
            ("power_on_hours", Some("h")),
            ("unsafe_shutdowns", None),
            ("media_errors", None),
            ("num_err_log_entries", None),
            ("warning_temp_time", Some("min")),
            ("critical_comp_time", Some("min")),
        ] {
            if let Some(value) = log.get(key).and_then(Value::as_u64) {
                health.attributes.push(StorageAttribute {
                    key: key.into(),
                    value: value.to_string(),
                    unit: unit.map(str::to_string),
                    warning: key == "critical_warning" && value != 0,
                });
            }
        }
        if let Some(warning) = log.get("critical_warning").and_then(Value::as_u64) {
            health.state = if warning != 0 { "critical" } else { "healthy" }.into();
            if warning != 0 {
                health.reason = format!("NVMe critical warning bits: {warning}");
            }
        }
        if log.get("media_errors").and_then(Value::as_u64).unwrap_or(0) > 0
            && health.state != "critical"
        {
            health.state = "attention".into();
            health.reason = "NVMe media errors reported".into();
        }
    }
    if let Some(rows) = json
        .pointer("/ata_smart_attributes/table")
        .and_then(Value::as_array)
    {
        for row in rows {
            let (Some(id), Some(raw)) = (
                row.get("id").and_then(Value::as_u64),
                row.pointer("/raw/value").and_then(Value::as_u64),
            ) else {
                continue;
            };
            let (key, warning) = match id {
                5 => ("reallocated_sectors", raw > 0),
                9 => ("power_on_hours", false),
                12 => ("power_cycles", false),
                187 => ("reported_uncorrectable", raw > 0),
                194 => ("temperature", false),
                197 => ("current_pending_sectors", raw > 0),
                198 => ("offline_uncorrectable", raw > 0),
                199 => ("interface_crc_errors", raw > 0),
                _ => ("vendor_attribute", false),
            };
            health.attributes.push(StorageAttribute {
                key: if key == "vendor_attribute" {
                    format!("ata_attribute_{id}")
                } else {
                    key.into()
                },
                value: raw.to_string(),
                unit: (id == 194).then(|| "°C".into()),
                warning,
            });
            if warning && health.state != "critical" {
                health.state = "attention".into();
                health.reason = format!("ATA SMART attribute {id} is nonzero");
            }
        }
    }
    if exit & (1 << 3) != 0 || passed == Some(false) {
        health.state = "critical".into();
        health.reason = "SMART overall health test failed".into();
    } else if health.state == "unknown" && passed == Some(true) {
        health.state = "healthy".into();
    }
    if exit & (1 << 4) != 0 && health.state != "critical" {
        health.state = "attention".into();
        health.reason = "SMART prefail attribute reported".into();
    }
    if exit & (1 << 5) != 0 && health.state == "healthy" {
        health.state = "attention".into();
        health.reason = "SMART attribute failed in the past".into();
    }
    if exit & (1 << 6 | 1 << 7) != 0 && health.state == "healthy" {
        health.state = "attention".into();
        health.reason = "SMART error or self-test log reports an issue".into();
    }
    if health.state == "unknown" {
        health.reason = "No supported SMART health result".into();
    }
    health
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_caller_paths() {
        assert!(!valid_device_name("../sda"));
        assert!(!valid_device_name("sda --all"));
        assert!(valid_device_name("nvme0n1"));
    }

    #[test]
    fn nvme_warning_is_critical_and_wear_is_not_health_score() {
        let health = parse_health(
            &json!({"nvme_smart_health_information_log": {"critical_warning": 1, "percentage_used": 120}}),
            0,
        );
        assert_eq!(health.state, "critical");
        assert_eq!(health.attributes[1].value, "120");
    }

    #[test]
    fn nvme_error_count_is_informational_without_warning_evidence() {
        let json = json!({"smart_status": {"passed": true}, "nvme_smart_health_information_log": {"critical_warning": 0, "num_err_log_entries": 7}});
        assert_eq!(parse_health(&json, 0).state, "healthy");
        assert_eq!(parse_health(&json, 1 << 6).state, "attention");
    }

    #[test]
    fn missing_health_test_stays_unknown() {
        assert_eq!(parse_health(&json!({}), 0).state, "unknown");
        assert_eq!(
            parse_health(&json!({"smart_status": {"passed": true}}), 0).state,
            "healthy"
        );
    }

    #[test]
    fn ata_warning_and_exit_bits_are_used() {
        let json = json!({"smart_status": {"passed": true}, "ata_smart_attributes": {"table": [{"id": 197, "raw": {"value": 2}}]}});
        assert_eq!(parse_health(&json, 0).state, "attention");
        assert_eq!(parse_health(&json, 8).state, "critical");
    }

    #[test]
    fn ata_standard_error_counters_have_meaningful_names() {
        let json = json!({"smart_status": {"passed": true}, "ata_smart_attributes": {"table": [
            {"id": 187, "raw": {"value": 2}}, {"id": 199, "raw": {"value": 3}}
        ]}});
        let health = parse_health(&json, 0);
        assert_eq!(health.state, "attention");
        assert_eq!(health.attributes[0].key, "reported_uncorrectable");
        assert_eq!(health.attributes[1].key, "interface_crc_errors");
    }

    #[test]
    fn permission_denial_requires_access_evidence() {
        let denied = json!({"smartctl": {"messages": [{"string": "Permission denied"}]}});
        assert_eq!(parse_health(&denied, 2).state, "permission_denied");
        assert_eq!(parse_health(&json!({}), 2).state, "unknown");
    }

    #[test]
    fn stale_cache_never_reports_healthy() {
        assert_eq!(cached_health(1, true).state, "stale");
    }

    #[test]
    fn stale_ata_warning_cannot_claim_current_health() {
        let mut health = cached_health(1, false);
        apply_ata_cache_warnings(&mut health, true, Some(1), Some(3));
        assert_eq!(health.state, "stale");
        assert_eq!(health.attributes.len(), 2);
    }
}
