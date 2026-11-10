use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use rog_core::{
    dbus_keys::storage as key, StorageAttribute, StorageDevice, StorageHealth, StorageHistoryPoint,
    StorageIo, StorageState, StorageTemperature, StorageVolume,
};
use zbus::zvariant::OwnedValue;

use crate::ov;

type Map = HashMap<String, OwnedValue>;

pub fn encode(state: &StorageState, detailed: bool) -> Map {
    HashMap::from([
        (key::STATUS.into(), ov(state.status.clone())),
        (key::SAMPLED_AT_MS.into(), ov(state.sampled_at_ms)),
        (
            key::DRIVES.into(),
            ov(state
                .drives
                .iter()
                .map(|drive| encode_drive(drive, detailed))
                .collect::<Vec<_>>()),
        ),
    ])
}

fn encode_drive(drive: &StorageDevice, detailed: bool) -> Map {
    let mut map = HashMap::from([
        (key::ID.into(), ov(drive.id.clone())),
        (key::NAME.into(), ov(drive.name.clone())),
        (key::MODEL.into(), ov(drive.model.clone())),
        (key::KIND.into(), ov(drive.kind.clone())),
        (key::TRANSPORT.into(), ov(drive.transport.clone())),
        (key::CAPACITY_BYTES.into(), ov(drive.capacity_bytes)),
        (key::ROTATIONAL.into(), ov(drive.rotational)),
        (
            key::VOLUMES.into(),
            ov(drive.volumes.iter().map(encode_volume).collect::<Vec<_>>()),
        ),
        (
            key::TEMPERATURES.into(),
            ov(drive
                .temperatures
                .iter()
                .map(encode_temperature)
                .collect::<Vec<_>>()),
        ),
        (
            key::HEALTH.into(),
            ov(encode_health(&drive.health, detailed)),
        ),
        (
            key::HISTORY.into(),
            ov(drive.history.iter().map(encode_history).collect::<Vec<_>>()),
        ),
    ]);
    if let Some(firmware) = &drive.firmware {
        map.insert(key::FIRMWARE.into(), ov(firmware.clone()));
    }
    if let Some(manufacturer) = &drive.manufacturer {
        map.insert(key::MANUFACTURER.into(), ov(manufacturer.clone()));
    }
    if let Some(size) = drive.logical_block_size {
        map.insert(key::LOGICAL_BLOCK_SIZE.into(), ov(size));
    }
    if let Some(size) = drive.physical_block_size {
        map.insert(key::PHYSICAL_BLOCK_SIZE.into(), ov(size));
    }
    if let Some(controller) = &drive.controller {
        map.insert(key::CONTROLLER.into(), ov(controller.clone()));
    }
    if let Some(io) = &drive.io {
        map.insert(key::IO.into(), ov(encode_io(io)));
    }
    map
}

fn encode_volume(volume: &StorageVolume) -> Map {
    let mut map = HashMap::from([
        ("name".into(), ov(volume.name.clone())),
        ("size_bytes".into(), ov(volume.size_bytes)),
        ("mount_points".into(), ov(volume.mount_points.clone())),
        ("status".into(), ov(volume.status.clone())),
    ]);
    if let Some(value) = &volume.filesystem {
        map.insert("filesystem".into(), ov(value.clone()));
    }
    if let Some(value) = &volume.label {
        map.insert("label".into(), ov(value.clone()));
    }
    if let Some(value) = volume.used_bytes {
        map.insert("used_bytes".into(), ov(value));
    }
    if let Some(value) = volume.available_bytes {
        map.insert("available_bytes".into(), ov(value));
    }
    if let Some(value) = volume.filesystem_size_bytes {
        map.insert("filesystem_size_bytes".into(), ov(value));
    }
    map
}

fn encode_io(io: &StorageIo) -> Map {
    let mut map = HashMap::from([
        ("read_bytes_per_sec".into(), ov(io.read_bytes_per_sec)),
        ("write_bytes_per_sec".into(), ov(io.write_bytes_per_sec)),
        ("read_iops".into(), ov(io.read_iops)),
        ("write_iops".into(), ov(io.write_iops)),
        ("read_bytes_since_boot".into(), ov(io.read_bytes_since_boot)),
        (
            "write_bytes_since_boot".into(),
            ov(io.write_bytes_since_boot),
        ),
        ("sampled_at_ms".into(), ov(io.sampled_at_ms)),
    ]);
    if let Some(value) = io.busy_percent {
        map.insert("busy_percent".into(), ov(u64::from(value)));
    }
    map
}

fn encode_temperature(temperature: &StorageTemperature) -> Map {
    HashMap::from([
        ("label".into(), ov(temperature.label.clone())),
        ("celsius".into(), ov(temperature.celsius)),
        ("source".into(), ov(temperature.source.clone())),
        ("sampled_at_ms".into(), ov(temperature.sampled_at_ms)),
    ])
}

fn encode_health(health: &StorageHealth, detailed: bool) -> Map {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let health = fresh_health(health, now_ms);
    let mut map = HashMap::from([
        ("state".into(), ov(health.state.clone())),
        ("source".into(), ov(health.source.clone())),
        ("reason".into(), ov(health.reason.clone())),
    ]);
    if let Some(value) = health.sampled_at_ms {
        map.insert("sampled_at_ms".into(), ov(value));
    }
    if detailed {
        map.insert(
            "attributes".into(),
            ov(health
                .attributes
                .iter()
                .map(encode_attribute)
                .collect::<Vec<_>>()),
        );
    }
    map
}

fn fresh_health(health: &StorageHealth, now_ms: u64) -> StorageHealth {
    let mut health = health.clone();
    if matches!(health.state.as_str(), "healthy" | "attention" | "critical")
        && health.sampled_at_ms.is_none_or(|at| {
            now_ms.saturating_sub(at) > 20 * 60 * 1000 || at > now_ms.saturating_add(5 * 60 * 1000)
        })
    {
        health.state = "stale".into();
        health.reason = "Last SMART result is stale".into();
    }
    health
}

fn encode_attribute(attribute: &StorageAttribute) -> Map {
    let mut map = HashMap::from([
        ("key".into(), ov(attribute.key.clone())),
        ("value".into(), ov(attribute.value.clone())),
        ("warning".into(), ov(attribute.warning)),
    ]);
    if let Some(value) = &attribute.unit {
        map.insert("unit".into(), ov(value.clone()));
    }
    map
}

fn encode_history(point: &StorageHistoryPoint) -> Map {
    let mut map = HashMap::from([("sampled_at_ms".into(), ov(point.sampled_at_ms))]);
    if let Some(value) = point.read_bytes_per_sec {
        map.insert("read_bytes_per_sec".into(), ov(value));
    }
    if let Some(value) = point.write_bytes_per_sec {
        map.insert("write_bytes_per_sec".into(), ov(value));
    }
    if let Some(value) = point.temperature_c {
        map.insert("temperature_c".into(), ov(value));
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_state_omits_smart_attributes_and_unknown_io() {
        let mut device = StorageDevice {
            id: "opaque-id".into(),
            name: "nvme0n1".into(),
            ..StorageDevice::default()
        };
        device.health.attributes.push(StorageAttribute {
            key: "power_on_hours".into(),
            value: "12".into(),
            ..StorageAttribute::default()
        });
        let state = StorageState {
            status: "available".into(),
            drives: vec![device],
            ..StorageState::default()
        };
        let compact = encode_drive(&state.drives[0], false);
        let detailed = encode_drive(&state.drives[0], true);
        assert!(!compact.contains_key(key::IO));
        let compact_health: Map = compact[key::HEALTH].clone().try_into().unwrap();
        let detailed_health: Map = detailed[key::HEALTH].clone().try_into().unwrap();
        assert!(!compact_health.contains_key("attributes"));
        assert!(detailed_health.contains_key("attributes"));
    }

    #[test]
    fn expired_healthy_result_is_stale_on_wire() {
        let health = StorageHealth {
            state: "healthy".into(),
            sampled_at_ms: Some(1),
            ..StorageHealth::default()
        };
        assert_eq!(fresh_health(&health, 1_300_002).state, "stale");
    }

    #[test]
    fn volume_wire_separates_partition_and_filesystem_capacity() {
        let volume = StorageVolume {
            size_bytes: 50,
            filesystem_size_bytes: Some(40),
            used_bytes: Some(10),
            available_bytes: Some(25),
            ..StorageVolume::default()
        };
        let encoded = encode_volume(&volume);
        assert_eq!(u64::try_from(encoded["size_bytes"].clone()).unwrap(), 50);
        assert_eq!(
            u64::try_from(encoded["filesystem_size_bytes"].clone()).unwrap(),
            40
        );
    }
}
