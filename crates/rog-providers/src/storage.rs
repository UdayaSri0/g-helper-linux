//! Read-only Linux block inventory and live I/O sampling.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rog_core::{
    PartitionInfo, StorageDevice, StorageHealth, StorageHistoryPoint, StorageIo,
    StorageProtectionState, StorageState, StorageTemperature, StorageTopology, StorageVolume,
    UnallocatedExtent,
};
use sha2::{Digest, Sha256};
use zbus::fdo::{ManagedObjects, ObjectManagerProxy};
use zvariant::{OwnedObjectPath, OwnedValue};

const HISTORY_LIMIT: usize = 60;
const VOLUME_REFRESH: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy)]
struct Counters {
    reads: u64,
    sectors_read: u64,
    writes: u64,
    sectors_written: u64,
    busy_ms: u64,
}

#[derive(Debug)]
struct Previous {
    at: Instant,
    names: Vec<String>,
    counters: Counters,
}

#[derive(Debug)]
struct Candidate {
    group: String,
    names: Vec<String>,
    model: String,
    firmware: Option<String>,
    identity: Option<String>,
    topology: PathBuf,
    capacity_bytes: u64,
}

#[derive(Debug)]
pub struct StorageProvider {
    sys_block: PathBuf,
    nvme_class: PathBuf,
    mountinfo: PathBuf,
    swaps: PathBuf,
    dev_root: PathBuf,
    by_label: PathBuf,
    previous: HashMap<String, Previous>,
    collision_bases: HashSet<String>,
    history: HashMap<String, VecDeque<StorageHistoryPoint>>,
    volume_cache: HashMap<String, Vec<StorageVolume>>,
    volume_refresh: Option<Instant>,
}

impl Default for StorageProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageProvider {
    pub fn new() -> Self {
        Self {
            sys_block: PathBuf::from("/sys/block"),
            nvme_class: PathBuf::from("/sys/class/nvme"),
            mountinfo: PathBuf::from("/proc/self/mountinfo"),
            swaps: PathBuf::from("/proc/swaps"),
            dev_root: PathBuf::from("/dev"),
            by_label: PathBuf::from("/dev/disk/by-label"),
            previous: HashMap::new(),
            collision_bases: HashSet::new(),
            history: HashMap::new(),
            volume_cache: HashMap::new(),
            volume_refresh: None,
        }
    }

    pub fn sample(&mut self) -> StorageState {
        let now = Instant::now();
        let sampled_at_ms = now_ms();
        let mut state = StorageState {
            sampled_at_ms,
            status: "available".into(),
            ..StorageState::default()
        };
        let Ok(entries) = fs::read_dir(&self.sys_block) else {
            state.status = "unavailable".into();
            self.previous.clear();
            self.history.clear();
            return state;
        };
        let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !physical_block_name(&name) {
                continue;
            }
            let path = entry.path();
            if path
                .canonicalize()
                .ok()
                .is_none_or(|p| p.to_string_lossy().contains("/virtual/"))
            {
                continue;
            }
            let controller = nvme_controller(&name);
            grouped
                .entry(controller.unwrap_or_else(|| name.clone()))
                .or_default()
                .push(name);
        }
        let mut candidates = Vec::new();
        for (group, mut names) in grouped {
            names.sort();
            let first = &names[0];
            let first_path = self.sys_block.join(first);
            let nvme = group != *first;
            let controller_path = self.nvme_class.join(&group);
            let model = if nvme {
                read_trimmed(controller_path.join("model"))
            } else {
                read_trimmed(first_path.join("device/model"))
            }
            .unwrap_or_else(|| "Unknown drive".into());
            let firmware = if nvme {
                read_trimmed(controller_path.join("firmware_rev"))
            } else {
                read_trimmed(first_path.join("device/rev"))
            };
            let identity = if nvme {
                read_trimmed(controller_path.join("serial"))
            } else {
                read_trimmed(first_path.join("device/wwid"))
                    .or_else(|| read_trimmed(first_path.join("wwid")))
                    .or_else(|| read_trimmed(first_path.join("device/serial")))
            };
            let topology = if nvme {
                controller_path.canonicalize()
            } else {
                first_path.join("device").canonicalize()
            }
            .or_else(|_| first_path.canonicalize())
            .unwrap_or(first_path.clone());
            let capacity_bytes = names
                .iter()
                .filter_map(|name| read_u64(self.sys_block.join(name).join("size")))
                .fold(0_u64, |sum, sectors| {
                    sum.saturating_add(sectors.saturating_mul(512))
                });
            if capacity_bytes == 0 {
                continue;
            }
            candidates.push(Candidate {
                group,
                names,
                model,
                firmware,
                identity,
                topology,
                capacity_bytes,
            });
        }
        let mut id_counts = HashMap::<String, usize>::new();
        for candidate in &candidates {
            *id_counts.entry(base_id(candidate)).or_default() += 1;
        }
        self.collision_bases.extend(
            id_counts
                .iter()
                .filter(|(_, count)| **count > 1)
                .map(|(id, _)| id.clone()),
        );
        let refresh_volumes = self
            .volume_refresh
            .is_none_or(|last| now.duration_since(last) >= VOLUME_REFRESH);
        let mounts = parse_mountinfo(&fs::read_to_string(&self.mountinfo).unwrap_or_default());
        let swaps = parse_swaps(&fs::read_to_string(&self.swaps).unwrap_or_default());
        let labels = read_labels(&self.by_label, &self.dev_root);
        let mut live = Vec::new();
        for Candidate {
            group,
            names,
            model,
            firmware,
            identity,
            topology,
            capacity_bytes,
        } in candidates
        {
            let base = opaque_id(
                identity.as_deref().unwrap_or(&topology.to_string_lossy()),
                &model,
            );
            let id = if self.collision_bases.contains(&base) {
                opaque_id(&format!("{base}\0{}", topology.display()), &model)
            } else {
                base
            };
            live.push(id.clone());
            let first = &names[0];
            let first_path = self.sys_block.join(first);
            let nvme = group != *first;
            let controller_path = self.nvme_class.join(&group);
            let rotational_flag = read_u64(first_path.join("queue/rotational"));
            let rotational = rotational_flag.is_some_and(|v| v != 0);
            let manufacturer = (!nvme)
                .then(|| read_trimmed(first_path.join("device/vendor")))
                .flatten()
                .filter(|vendor| !matches!(vendor.as_str(), "ATA" | "Generic" | "USB"));
            let logical_block_size = read_u64(first_path.join("queue/logical_block_size"));
            let physical_block_size = read_u64(first_path.join("queue/physical_block_size"));
            let transport = if nvme {
                "nvme"
            } else if topology.to_string_lossy().contains("/usb") {
                "usb"
            } else if first.starts_with("mmcblk") {
                "mmc"
            } else {
                "ata/scsi"
            };
            let mut device = StorageDevice {
                id: id.clone(),
                name: first.clone(),
                model,
                manufacturer,
                kind: match (transport, rotational_flag) {
                    ("mmc", Some(0)) => "flash",
                    ("usb", Some(0)) => "non-rotational",
                    (_, Some(0)) => "ssd",
                    (_, Some(_)) => "hdd",
                    (_, None) => "unknown",
                }
                .into(),
                transport: transport.into(),
                capacity_bytes,
                logical_block_size,
                physical_block_size,
                rotational,
                firmware,
                controller: nvme.then_some(group.clone()),
                namespaces: names.clone(),
                health: StorageHealth {
                    state: "unknown".into(),
                    reason: "Health has not been sampled".into(),
                    ..StorageHealth::default()
                },
                ..StorageDevice::default()
            };
            let changed_names = self.previous.get(&id).is_some_and(|old| old.names != names);
            if refresh_volumes || changed_names || !self.volume_cache.contains_key(&id) {
                device.volumes = names
                    .iter()
                    .flat_map(|name| volumes_for_disk(&self.sys_block, name, &mounts, &labels))
                    .collect();
                self.volume_cache.insert(id.clone(), device.volumes.clone());
            } else {
                device.volumes = self.volume_cache.get(&id).cloned().unwrap_or_default();
            }
            device.topologies = names
                .iter()
                .map(|name| {
                    topology_for_namespace(
                        &self.sys_block,
                        name,
                        &id,
                        &mounts,
                        &swaps,
                        &labels,
                        &device.volumes,
                    )
                })
                .collect();
            if nvme {
                device.temperatures = nvme_temperatures(&controller_path, sampled_at_ms);
            }
            let counters = names
                .iter()
                .map(|name| read_counters(&self.sys_block.join(name).join("stat")))
                .collect::<Option<Vec<_>>>()
                .and_then(|rows| sum_counters(&rows));
            let mut point = StorageHistoryPoint {
                sampled_at_ms,
                temperature_c: device.temperatures.first().map(|value| value.celsius),
                ..StorageHistoryPoint::default()
            };
            if let Some(counters) = counters {
                if let Some(prev) = self.previous.get(&id) {
                    let elapsed = now.duration_since(prev.at);
                    if prev.names == names
                        && elapsed >= Duration::from_millis(100)
                        && elapsed <= Duration::from_secs(10)
                    {
                        if let Some(io) =
                            calculate_io(prev.counters, counters, elapsed, sampled_at_ms)
                        {
                            point.read_bytes_per_sec = Some(io.read_bytes_per_sec);
                            point.write_bytes_per_sec = Some(io.write_bytes_per_sec);
                            device.io = Some(io);
                        }
                    }
                }
                self.previous.insert(
                    id.clone(),
                    Previous {
                        at: now,
                        names: names.clone(),
                        counters,
                    },
                );
            } else {
                self.previous.remove(&id);
            }
            let history = self.history.entry(id).or_default();
            history.push_back(point);
            if history.len() > HISTORY_LIMIT {
                history.pop_front();
            }
            device.history = history.iter().cloned().collect();
            state.drives.push(device);
        }
        self.previous.retain(|id, _| live.contains(id));
        self.history.retain(|id, _| live.contains(id));
        self.volume_cache.retain(|id, _| live.contains(id));
        if refresh_volumes {
            self.volume_refresh = Some(now);
        }
        state
    }
}

fn topology_for_namespace(
    sys_block: &Path,
    name: &str,
    drive_id: &str,
    mounts: &HashMap<String, Vec<Mount>>,
    swaps: &HashSet<String>,
    labels: &HashMap<String, String>,
    volumes: &[StorageVolume],
) -> StorageTopology {
    let disk = sys_block.join(name);
    let capacity_bytes = read_u64(disk.join("size")).and_then(|n| n.checked_mul(512));
    let logical_block_size = read_u64(disk.join("queue/logical_block_size"));
    let physical_block_size = read_u64(disk.join("queue/physical_block_size"));
    let mut result = StorageTopology {
        namespace: name.into(),
        capacity_bytes: capacity_bytes.unwrap_or(0),
        status: "uncertain".into(),
        logical_block_size,
        physical_block_size,
        read_only: read_u64(disk.join("ro")).map(|v| v != 0),
        removable: read_u64(disk.join("removable")).map(|v| v != 0),
        ..StorageTopology::default()
    };
    let Ok(entries) = fs::read_dir(&disk) else {
        result.status = "unavailable".into();
        return result;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.join("partition").is_file() {
            continue;
        }
        let Some(part_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let (Some(number), Some(start), Some(size)) = (
            read_u64(path.join("partition")).and_then(|n| u32::try_from(n).ok()),
            read_u64(path.join("start")).and_then(|n| n.checked_mul(512)),
            read_u64(path.join("size")).and_then(|n| n.checked_mul(512)),
        ) else {
            result.status = "unavailable".into();
            result.partitions.clear();
            return result;
        };
        let dev = read_trimmed(path.join("dev"));
        let mount_points: Vec<String> = dev
            .as_deref()
            .and_then(|id| mounts.get(id))
            .map(|rows| rows.iter().map(|m| m.point.clone()).collect())
            .unwrap_or_default();
        let protected_reason = if mount_points.iter().any(|m| is_system_mount(m)) {
            Some("Mounted system filesystem")
        } else if swaps.contains(part_name) {
            Some("Active swap")
        } else if fs::read_dir(path.join("holders"))
            .ok()
            .is_some_and(|mut d| d.next().is_some())
        {
            Some("Dependent block device")
        } else {
            None
        };
        let protection = if protected_reason.is_some() {
            StorageProtectionState::Protected
        } else {
            StorageProtectionState::Uncertain
        };
        let volume = volumes.iter().find(|v| v.name == part_name);
        result.partitions.push(PartitionInfo {
            id: opaque_id(
                &format!("{drive_id}\0{name}\0{number}\0{start}\0{size}"),
                "partition",
            ),
            name: part_name.into(),
            number,
            start_bytes: start,
            length_bytes: size,
            filesystem: volume.and_then(|v| v.filesystem.clone()),
            label: labels.get(part_name).cloned(),
            mount_points,
            protection,
            protection_reason: protected_reason
                .unwrap_or("Storage relationships require verification")
                .into(),
            ..PartitionInfo::default()
        });
    }
    result.partitions.sort_by_key(|p| p.start_bytes);
    result.protection = if result
        .partitions
        .iter()
        .any(|p| p.protection == StorageProtectionState::Protected)
        || read_trimmed(disk.join("dev"))
            .as_deref()
            .and_then(|id| mounts.get(id))
            .is_some_and(|rows| rows.iter().any(|m| is_system_mount(&m.point)))
        || swaps.contains(name)
        || fs::read_dir(disk.join("holders"))
            .ok()
            .is_some_and(|mut entries| entries.next().is_some())
    {
        StorageProtectionState::Protected
    } else {
        StorageProtectionState::Uncertain
    };
    // Sysfs supplies geometry but no authoritative partition-table type; UDisks2 completes verification.
    result
}

fn is_system_mount(mount: &str) -> bool {
    mount == "/"
        || mount == "/boot"
        || mount.starts_with("/boot/")
        || mount == "/usr"
        || mount.starts_with("/usr/")
        || mount == "/var"
        || mount.starts_with("/var/")
}

fn parse_swaps(text: &str) -> HashSet<String> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let path = line.split_whitespace().next()?;
            Path::new(path).file_name()?.to_str().map(str::to_owned)
        })
        .collect()
}

/// Cross-checks sysfs geometry against UDisks2's read-only ObjectManager snapshot.
pub async fn enrich_topology_from_udisks(state: &mut StorageState) {
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        let connection = zbus::Connection::system().await.ok()?;
        let manager = ObjectManagerProxy::builder(&connection)
            .destination("org.freedesktop.UDisks2")
            .ok()?
            .path("/org/freedesktop/UDisks2")
            .ok()?
            .build()
            .await
            .ok()?;
        manager.get_managed_objects().await.ok()
    })
    .await
    .ok()
    .flatten();
    let Some(objects) = result else {
        for drive in &mut state.drives {
            for topology in &mut drive.topologies {
                topology.status = "unavailable".into();
            }
        }
        return;
    };
    for drive in &mut state.drives {
        for topology in &mut drive.topologies {
            verify_udisks_topology(topology, &drive.id, &objects, Path::new("/sys/block"));
        }
    }
}

fn property<'a>(
    objects: &'a ManagedObjects,
    path: &OwnedObjectPath,
    interface: &str,
    key: &str,
) -> Option<&'a OwnedValue> {
    objects
        .get(path)?
        .iter()
        .find(|(name, _)| name.as_str() == interface)?
        .1
        .get(key)
}

fn property_u64(
    objects: &ManagedObjects,
    path: &OwnedObjectPath,
    interface: &str,
    key: &str,
) -> Option<u64> {
    let value = property(objects, path, interface, key)?.clone();
    u64::try_from(value.clone())
        .ok()
        .or_else(|| u32::try_from(value).ok().map(u64::from))
}

fn property_string(
    objects: &ManagedObjects,
    path: &OwnedObjectPath,
    interface: &str,
    key: &str,
) -> Option<String> {
    String::try_from(property(objects, path, interface, key)?.clone()).ok()
}

fn property_path(
    objects: &ManagedObjects,
    path: &OwnedObjectPath,
    interface: &str,
    key: &str,
) -> Option<OwnedObjectPath> {
    OwnedObjectPath::try_from(property(objects, path, interface, key)?.clone()).ok()
}

fn sys_device_number(path: &Path) -> Option<u64> {
    let dev = read_trimmed(path.join("dev"))?;
    let (major, minor) = dev.split_once(':')?;
    Some(libc::makedev(major.parse().ok()?, minor.parse().ok()?))
}

fn verify_udisks_topology(
    topology: &mut StorageTopology,
    drive_id: &str,
    objects: &ManagedObjects,
    sys_block: &Path,
) {
    const BLOCK: &str = "org.freedesktop.UDisks2.Block";
    const TABLE: &str = "org.freedesktop.UDisks2.PartitionTable";
    const PART: &str = "org.freedesktop.UDisks2.Partition";
    let path = format!(
        "/org/freedesktop/UDisks2/block_devices/{}",
        topology.namespace
    );
    let Ok(disk_path) = OwnedObjectPath::try_from(path) else {
        return;
    };
    let Some(table_type) = property_string(objects, &disk_path, TABLE, "Type") else {
        return;
    };
    let Some(capacity) = property_u64(objects, &disk_path, BLOCK, "Size") else {
        return;
    };
    if capacity != topology.capacity_bytes
        || !matches!(table_type.as_str(), "gpt" | "dos")
        || property_u64(objects, &disk_path, BLOCK, "DeviceNumber")
            != sys_device_number(&sys_block.join(&topology.namespace))
    {
        return;
    }
    let mut partitions = topology.partitions.clone();
    for partition in &mut partitions {
        let mut matching = objects.iter().filter(|(path, _)| {
            property_path(objects, path, PART, "Table").as_ref() == Some(&disk_path)
                && property_u64(objects, path, PART, "Number") == Some(u64::from(partition.number))
        });
        let Some((part_path, _)) = matching.next() else {
            return;
        };
        if matching.next().is_some()
            || property_u64(objects, part_path, PART, "Offset") != Some(partition.start_bytes)
            || property_u64(objects, part_path, PART, "Size") != Some(partition.length_bytes)
            || property_u64(objects, part_path, BLOCK, "DeviceNumber")
                != sys_device_number(&sys_block.join(&topology.namespace).join(&partition.name))
        {
            return;
        }
        partition.partition_type = property_string(objects, part_path, PART, "Type");
        if let Some(uuid) =
            property_string(objects, part_path, PART, "UUID").filter(|s| !s.is_empty())
        {
            partition.id = opaque_id(
                &format!(
                    "{drive_id}\0{uuid}\0{}\0{}",
                    partition.start_bytes, partition.length_bytes
                ),
                "partition",
            );
        }
        if partition.partition_type.as_deref().is_some_and(|kind| {
            ["0xef", "0xef00", "c12a7328-f81f-11d2-ba4b-00a0c93ec93b"]
                .iter()
                .any(|efi| kind.eq_ignore_ascii_case(efi))
        }) {
            partition.protection = StorageProtectionState::Protected;
            partition.protection_reason = "EFI system partition".into();
        }
        if partition.filesystem.is_none() {
            partition.filesystem = property_string(objects, part_path, BLOCK, "IdType");
        }
    }
    // Reject extra UDisks partitions absent from sysfs, including a mid-enumeration change.
    let reported = objects
        .keys()
        .filter(|path| property_path(objects, path, PART, "Table").as_ref() == Some(&disk_path))
        .count();
    if reported != partitions.len() {
        return;
    }
    topology.partitions = partitions;
    topology.table_type = Some(table_type);
    topology.protection = if topology
        .partitions
        .iter()
        .any(|p| p.protection == StorageProtectionState::Protected)
    {
        StorageProtectionState::Protected
    } else {
        StorageProtectionState::Uncertain
    };
    if let Some(gaps) = verified_gaps(topology) {
        topology.unallocated_extents = gaps;
        topology.status = "verified".into();
    }
}

fn verified_gaps(topology: &StorageTopology) -> Option<Vec<UnallocatedExtent>> {
    let logical = topology.logical_block_size?;
    if !matches!(logical, 512 | 1024 | 2048 | 4096)
        || topology.capacity_bytes == 0
        || !matches!(topology.table_type.as_deref(), Some("gpt" | "dos"))
    {
        return None;
    }
    let mut end = 0_u64;
    let mut gaps = Vec::new();
    for partition in &topology.partitions {
        let next = partition.start_bytes.checked_add(partition.length_bytes)?;
        if partition.length_bytes == 0
            || partition.start_bytes < end
            || next > topology.capacity_bytes
            || partition.start_bytes % logical != 0
            || partition.length_bytes % logical != 0
        {
            return None;
        }
        if end != 0 && partition.start_bytes > end {
            gaps.push(UnallocatedExtent {
                start_bytes: end,
                length_bytes: partition.start_bytes - end,
            });
        }
        end = next;
    }
    // shortcut: table metadata makes edge gaps ambiguous; expose them after usable-LBA verification.
    Some(gaps)
}

fn base_id(candidate: &Candidate) -> String {
    opaque_id(
        candidate
            .identity
            .as_deref()
            .unwrap_or(&candidate.topology.to_string_lossy()),
        &candidate.model,
    )
}

fn physical_block_name(name: &str) -> bool {
    let valid = !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric());
    valid
        && (name.starts_with("nvme") && nvme_controller(name).is_some()
            || name.starts_with("sd")
                && name.len() > 2
                && name[2..].bytes().all(|b| b.is_ascii_lowercase())
            || name.starts_with("hd")
                && name.len() > 2
                && name[2..].bytes().all(|b| b.is_ascii_lowercase())
            || name.starts_with("mmcblk")
                && name.len() > 6
                && name[6..].bytes().all(|b| b.is_ascii_digit()))
}

fn nvme_controller(name: &str) -> Option<String> {
    let tail = name.strip_prefix("nvme")?;
    let (controller, ns) = tail.split_once('n')?;
    if controller.is_empty()
        || ns.is_empty()
        || !controller.bytes().all(|b| b.is_ascii_digit())
        || !ns.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some(format!("nvme{controller}"))
}

fn opaque_id(identity: &str, model: &str) -> String {
    let digest = Sha256::digest(format!("{identity}\0{model}").as_bytes());
    format!(
        "drive-{}",
        digest
            .iter()
            .take(12)
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_u64(path: impl AsRef<Path>) -> Option<u64> {
    read_trimmed(path)?.parse().ok()
}

fn read_counters(path: &Path) -> Option<Counters> {
    let text = fs::read_to_string(path).ok()?;
    let fields = text
        .split_whitespace()
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if fields.len() < 10 {
        return None;
    }
    Some(Counters {
        reads: fields[0],
        sectors_read: fields[2],
        writes: fields[4],
        sectors_written: fields[6],
        busy_ms: fields[9],
    })
}

fn sum_counters(rows: &[Counters]) -> Option<Counters> {
    let mut result = Counters {
        reads: 0,
        sectors_read: 0,
        writes: 0,
        sectors_written: 0,
        busy_ms: 0,
    };
    for row in rows {
        result.reads = result.reads.checked_add(row.reads)?;
        result.sectors_read = result.sectors_read.checked_add(row.sectors_read)?;
        result.writes = result.writes.checked_add(row.writes)?;
        result.sectors_written = result.sectors_written.checked_add(row.sectors_written)?;
        result.busy_ms = result.busy_ms.checked_add(row.busy_ms)?;
    }
    Some(result)
}

fn calculate_io(
    previous: Counters,
    current: Counters,
    elapsed: Duration,
    at: u64,
) -> Option<StorageIo> {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return None;
    }
    let reads = current.reads.checked_sub(previous.reads)?;
    let writes = current.writes.checked_sub(previous.writes)?;
    let read_sectors = current.sectors_read.checked_sub(previous.sectors_read)?;
    let written_sectors = current
        .sectors_written
        .checked_sub(previous.sectors_written)?;
    let busy = current.busy_ms.checked_sub(previous.busy_ms)?;
    Some(StorageIo {
        read_bytes_per_sec: ((read_sectors as f64 * 512.0) / seconds) as u64,
        write_bytes_per_sec: ((written_sectors as f64 * 512.0) / seconds) as u64,
        read_iops: (reads as f64 / seconds) as u64,
        write_iops: (writes as f64 / seconds) as u64,
        read_bytes_since_boot: current.sectors_read.saturating_mul(512),
        write_bytes_since_boot: current.sectors_written.saturating_mul(512),
        busy_percent: Some(
            ((busy as f64 / elapsed.as_millis() as f64) * 100.0).clamp(0.0, 100.0) as u8,
        ),
        sampled_at_ms: at,
    })
}

#[derive(Debug, Clone)]
struct Mount {
    point: String,
    filesystem: String,
}

fn parse_mountinfo(text: &str) -> HashMap<String, Vec<Mount>> {
    let mut result: HashMap<String, Vec<Mount>> = HashMap::new();
    for line in text.lines() {
        let Some((left, right)) = line.split_once(" - ") else {
            continue;
        };
        let fields = left.split_whitespace().collect::<Vec<_>>();
        let fs = right.split_whitespace().next().unwrap_or("");
        if fields.len() < 5 || fs.is_empty() {
            continue;
        }
        let mount = Mount {
            point: decode_mount_path(fields[4]),
            filesystem: fs.into(),
        };
        let mounts = result.entry(fields[2].into()).or_default();
        if !mounts.iter().any(|m| m.point == mount.point) {
            mounts.push(mount);
        }
    }
    result
}

fn decode_mount_path(value: &str) -> String {
    let mut out = Vec::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1..i + 4]
                .iter()
                .all(|b| (b'0'..=b'7').contains(b))
        {
            let octal = u16::from(bytes[i + 1] - b'0') * 64
                + u16::from(bytes[i + 2] - b'0') * 8
                + u16::from(bytes[i + 3] - b'0');
            if let Ok(decoded) = u8::try_from(octal) {
                out.push(decoded);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn read_labels(by_label: &Path, dev_root: &Path) -> HashMap<String, String> {
    let mut labels = HashMap::new();
    let Ok(dev_root) = dev_root.canonicalize() else {
        return labels;
    };
    let Ok(entries) = fs::read_dir(by_label) else {
        return labels;
    };
    for entry in entries.flatten() {
        let Ok(target) = entry.path().canonicalize() else {
            continue;
        };
        if target.parent() != Some(dev_root.as_path()) {
            continue;
        }
        let (Some(device), Some(label)) = (
            target.file_name().and_then(|s| s.to_str()),
            entry.file_name().to_str().map(str::to_owned),
        ) else {
            continue;
        };
        if !label.is_empty() {
            labels.entry(device.to_owned()).or_insert(label);
        }
    }
    labels
}

fn volumes_for_disk(
    sys_block: &Path,
    name: &str,
    mounts: &HashMap<String, Vec<Mount>>,
    labels: &HashMap<String, String>,
) -> Vec<StorageVolume> {
    let disk = sys_block.join(name);
    let mut paths = fs::read_dir(&disk)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten().map(|e| e.path()))
        .filter(|path| path.join("partition").is_file())
        .collect::<Vec<_>>();
    if paths.is_empty() {
        paths.push(disk);
    }
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            let size_bytes = read_u64(path.join("size")).unwrap_or(0).saturating_mul(512);
            let dev = read_trimmed(path.join("dev"));
            let associated = dev.as_deref().and_then(|id| mounts.get(id));
            let mount_points = associated
                .map(|rows| rows.iter().map(|row| row.point.clone()).collect())
                .unwrap_or_default();
            let filesystem =
                associated.and_then(|rows| rows.first().map(|row| row.filesystem.clone()));
            let (used_bytes, available_bytes, filesystem_size_bytes) = associated
                .and_then(|rows| rows.first())
                .and_then(|row| filesystem_usage(&row.point))
                .map_or((None, None, None), |(used, avail, total)| {
                    (Some(used), Some(avail), Some(total))
                });
            Some(StorageVolume {
                label: labels.get(&name).cloned(),
                name,
                size_bytes,
                filesystem,
                mount_points,
                used_bytes,
                available_bytes,
                filesystem_size_bytes,
                status: if associated.is_some() {
                    "mounted"
                } else {
                    "not_mounted"
                }
                .into(),
            })
        })
        .collect()
}

fn filesystem_usage(path: &str) -> Option<(u64, u64, u64)> {
    let cpath = CString::new(path).ok()?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `cpath` is NUL-terminated and `stats` points to writable memory.
    if unsafe { libc::statvfs(cpath.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: statvfs initialized the structure on success.
    let stats = unsafe { stats.assume_init() };
    let unit = stats.f_frsize;
    Some((
        stats
            .f_blocks
            .saturating_sub(stats.f_bfree)
            .saturating_mul(unit),
        stats.f_bavail.saturating_mul(unit),
        stats.f_blocks.saturating_mul(unit),
    ))
}

fn nvme_temperatures(controller: &Path, sampled_at_ms: u64) -> Vec<StorageTemperature> {
    let mut temperatures = Vec::new();
    let Ok(entries) = fs::read_dir(controller) else {
        return temperatures;
    };
    for hwmon in entries.flatten().map(|e| e.path()).filter(|p| {
        p.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("hwmon"))
    }) {
        let Ok(sensors) = fs::read_dir(hwmon) else {
            continue;
        };
        for sensor in sensors.flatten() {
            let name = sensor.file_name().to_string_lossy().into_owned();
            let Some(index) = name
                .strip_prefix("temp")
                .and_then(|s| s.strip_suffix("_input"))
            else {
                continue;
            };
            if index.parse::<u8>().is_err() {
                continue;
            }
            let Some(milli_c) = read_trimmed(sensor.path()).and_then(|s| s.parse::<i64>().ok())
            else {
                continue;
            };
            if !(-40_000..=150_000).contains(&milli_c) {
                continue;
            }
            let label = read_trimmed(sensor.path().with_file_name(format!("temp{index}_label")))
                .unwrap_or_else(|| {
                    if index == "1" {
                        "Composite".into()
                    } else {
                        format!("Sensor {index}")
                    }
                });
            temperatures.push(StorageTemperature {
                label,
                celsius: milli_c as f64 / 1000.0,
                source: "nvme-hwmon".into(),
                sampled_at_ms,
            });
        }
    }
    temperatures.sort_by(|a, b| a.label.cmp(&b.label));
    temperatures
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
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> (PathBuf, StorageProvider) {
        let root = std::env::temp_dir().join(format!(
            "rog-storage-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let sys_block = root.join("block");
        let nvme_class = root.join("nvme");
        let dev_root = root.join("dev");
        let by_label = root.join("by-label");
        fs::create_dir_all(&sys_block).unwrap();
        fs::create_dir_all(&nvme_class).unwrap();
        fs::create_dir_all(&dev_root).unwrap();
        fs::create_dir_all(&by_label).unwrap();
        let mut provider = StorageProvider::new();
        provider.sys_block = sys_block;
        provider.nvme_class = nvme_class;
        provider.mountinfo = root.join("mountinfo");
        provider.swaps = root.join("swaps");
        provider.dev_root = dev_root;
        provider.by_label = by_label;
        fs::write(&provider.mountinfo, "").unwrap();
        fs::write(&provider.swaps, "Filename Type Size Used Priority\n").unwrap();
        (root, provider)
    }

    fn disk(provider: &StorageProvider, name: &str, sectors: u64) {
        let path = provider.sys_block.join(name);
        fs::create_dir_all(path.join("queue")).unwrap();
        fs::write(path.join("size"), sectors.to_string()).unwrap();
        fs::write(path.join("queue/rotational"), "0").unwrap();
        fs::write(path.join("stat"), "1 0 2 0 3 0 4 0 0 5 0").unwrap();
    }

    #[test]
    fn block_names_reject_partitions_and_paths() {
        for name in [
            "nvme0n1p1",
            "sda1",
            "dm-0",
            "loop0",
            "../sda",
            "nvme0n1/../sda",
        ] {
            assert!(!physical_block_name(name), "{name}");
        }
        for name in ["nvme0n1", "nvme10n24", "sda", "mmcblk0"] {
            assert!(physical_block_name(name), "{name}");
        }
    }

    #[test]
    fn counters_use_fixed_512_byte_sectors_and_reject_reset() {
        let before = Counters {
            reads: 10,
            sectors_read: 100,
            writes: 5,
            sectors_written: 20,
            busy_ms: 10,
        };
        let after = Counters {
            reads: 12,
            sectors_read: 108,
            writes: 6,
            sectors_written: 24,
            busy_ms: 110,
        };
        let io = calculate_io(before, after, Duration::from_secs(1), 1).unwrap();
        assert_eq!(io.read_bytes_per_sec, 4096);
        assert_eq!(io.write_bytes_per_sec, 2048);
        assert_eq!(io.read_iops, 2);
        assert_eq!(io.busy_percent, Some(10));
        assert!(calculate_io(after, before, Duration::from_secs(1), 2).is_none());
    }

    #[test]
    fn mountinfo_groups_multiple_mounts_and_decodes_paths() {
        let rows = parse_mountinfo("1 0 8:1 / /mnt/a\\040b rw - ext4 /dev/sda1 rw\n2 0 8:1 / /mnt/other rw - ext4 /dev/sda1 rw");
        assert_eq!(rows["8:1"].len(), 2);
        assert_eq!(rows["8:1"][0].point, "/mnt/a b");
        assert_eq!(decode_mount_path("/mnt/\\777"), "/mnt/\\777");
    }

    #[test]
    fn filesystem_usage_keeps_full_capacity_separate_from_available_bytes() {
        let (used, available, total) = filesystem_usage("/").unwrap();
        assert!(used <= total);
        assert!(available <= total);
        assert!(used.saturating_add(available) <= total);
    }

    #[test]
    fn empty_inventory_is_available_without_fabricated_io() {
        let (root, mut provider) = fixture();
        disk(&provider, "sda", 0);
        let state = provider.sample();
        assert_eq!(state.status, "available");
        assert!(state.drives.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn multiple_namespaces_are_one_physical_controller() {
        let (root, mut provider) = fixture();
        disk(&provider, "nvme0n1", 100);
        disk(&provider, "nvme0n2", 200);
        disk(&provider, "nvme1n1", 300);
        fs::create_dir_all(provider.nvme_class.join("nvme0")).unwrap();
        fs::create_dir_all(provider.nvme_class.join("nvme1")).unwrap();
        fs::write(provider.nvme_class.join("nvme0/model"), "Disk A").unwrap();
        fs::write(provider.nvme_class.join("nvme1/model"), "Disk B").unwrap();
        fs::write(provider.nvme_class.join("nvme0/serial"), "private-A").unwrap();
        fs::write(provider.nvme_class.join("nvme1/serial"), "private-B").unwrap();
        let state = provider.sample();
        assert_eq!(state.drives.len(), 2);
        let first = state
            .drives
            .iter()
            .find(|d| d.controller.as_deref() == Some("nvme0"))
            .unwrap();
        assert_eq!(first.capacity_bytes, 300 * 512);
        assert_eq!(first.namespaces, ["nvme0n1", "nvme0n2"]);
        assert!(first.io.is_none());
        assert!(!first.id.contains("private"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn duplicate_reported_wwids_receive_distinct_ids() {
        let (root, mut provider) = fixture();
        for name in ["sda", "sdb"] {
            disk(&provider, name, 100);
            let device = provider.sys_block.join(name).join("device");
            fs::create_dir_all(&device).unwrap();
            fs::write(device.join("model"), "Same model").unwrap();
            fs::write(device.join("wwid"), "same-private-wwid").unwrap();
        }
        let first = provider.sample();
        let second = provider.sample();
        assert_eq!(first.drives.len(), 2);
        assert_ne!(first.drives[0].id, first.drives[1].id);
        assert_eq!(first.drives[0].id, second.drives[0].id);
        assert_eq!(first.drives[1].id, second.drives[1].id);
        fs::remove_dir_all(provider.sys_block.join("sdb")).unwrap();
        let after_unplug = provider.sample();
        assert_eq!(after_unplug.drives.len(), 1);
        assert_eq!(first.drives[0].id, after_unplug.drives[0].id);
        assert!(first
            .drives
            .iter()
            .all(|drive| !drive.id.contains("private")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn block_sizes_and_meaningful_vendor_are_optional_metadata() {
        let (root, mut provider) = fixture();
        disk(&provider, "sda", 100);
        let disk = provider.sys_block.join("sda");
        fs::create_dir_all(disk.join("device")).unwrap();
        fs::write(disk.join("device/vendor"), "Acme").unwrap();
        fs::write(disk.join("queue/logical_block_size"), "512").unwrap();
        fs::write(disk.join("queue/physical_block_size"), "4096").unwrap();
        let device = provider.sample().drives.remove(0);
        assert_eq!(device.manufacturer.as_deref(), Some("Acme"));
        assert_eq!(device.logical_block_size, Some(512));
        assert_eq!(device.physical_block_size, Some(4096));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn partition_label_is_mapped_only_to_matching_device() {
        use std::os::unix::fs::symlink;

        let (root, mut provider) = fixture();
        disk(&provider, "sda", 100);
        let partition = provider.sys_block.join("sda/sda1");
        fs::create_dir_all(&partition).unwrap();
        fs::write(partition.join("partition"), "1").unwrap();
        fs::write(partition.join("size"), "50").unwrap();
        fs::write(partition.join("dev"), "8:1").unwrap();
        fs::write(provider.dev_root.join("sda1"), "").unwrap();
        symlink(
            provider.dev_root.join("sda1"),
            provider.by_label.join("Data"),
        )
        .unwrap();
        symlink("/etc/passwd", provider.by_label.join("Wrong")).unwrap();
        let drive = provider.sample().drives.remove(0);
        assert_eq!(drive.volumes.len(), 1);
        assert_eq!(drive.volumes[0].label.as_deref(), Some("Data"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stable_identity_survives_device_rename_and_counter_reset() {
        let (root, mut provider) = fixture();
        disk(&provider, "sda", 100);
        fs::create_dir_all(provider.sys_block.join("sda/device")).unwrap();
        fs::write(provider.sys_block.join("sda/device/model"), "External").unwrap();
        fs::write(provider.sys_block.join("sda/device/wwid"), "private-wwid").unwrap();
        let first = provider.sample().drives.remove(0);
        assert!(first.io.is_none());
        fs::rename(
            provider.sys_block.join("sda"),
            provider.sys_block.join("sdb"),
        )
        .unwrap();
        let renamed = provider.sample().drives.remove(0);
        assert_eq!(renamed.id, first.id);
        assert!(renamed.io.is_none());
        std::thread::sleep(Duration::from_millis(110));
        fs::write(
            provider.sys_block.join("sdb/stat"),
            "2 0 10 0 4 0 8 0 0 10 0",
        )
        .unwrap();
        let active = provider.sample().drives.remove(0);
        assert!(active
            .io
            .as_ref()
            .is_some_and(|io| io.read_bytes_per_sec > 0));
        fs::write(provider.sys_block.join("sdb/stat"), "1 0 1 0 1 0 1 0 0 1 0").unwrap();
        assert!(provider.sample().drives.remove(0).io.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verified_geometry_reports_only_interior_unallocated_extents() {
        let topology = StorageTopology {
            capacity_bytes: 100 * 512,
            logical_block_size: Some(512),
            table_type: Some("gpt".into()),
            partitions: vec![
                PartitionInfo {
                    start_bytes: 10 * 512,
                    length_bytes: 20 * 512,
                    ..PartitionInfo::default()
                },
                PartitionInfo {
                    start_bytes: 40 * 512,
                    length_bytes: 20 * 512,
                    ..PartitionInfo::default()
                },
            ],
            ..StorageTopology::default()
        };
        let gaps = verified_gaps(&topology).unwrap();
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].start_bytes, 30 * 512);
        assert_eq!(gaps[0].length_bytes, 10 * 512);
        assert!(verified_gaps(&StorageTopology {
            table_type: None,
            ..topology.clone()
        })
        .is_none());
        assert_eq!(
            verified_gaps(&StorageTopology {
                table_type: Some("dos".into()),
                ..topology
            })
            .unwrap()
            .len(),
            1
        );
    }

    #[test]
    fn overlapping_unaligned_and_overflowing_partitions_are_not_verified() {
        let base = StorageTopology {
            capacity_bytes: 100 * 512,
            logical_block_size: Some(4096),
            table_type: Some("gpt".into()),
            partitions: vec![PartitionInfo {
                start_bytes: 8 * 512,
                length_bytes: 16 * 512,
                ..PartitionInfo::default()
            }],
            ..StorageTopology::default()
        };
        assert!(verified_gaps(&base).is_some());
        let mut invalid = base.clone();
        invalid.partitions.push(PartitionInfo {
            start_bytes: 16 * 512,
            length_bytes: 16 * 512,
            ..PartitionInfo::default()
        });
        assert!(verified_gaps(&invalid).is_none());
        invalid.partitions[1].start_bytes = 25 * 512;
        assert!(verified_gaps(&invalid).is_none());
        invalid.partitions[1].start_bytes = u64::MAX - 7;
        assert!(verified_gaps(&invalid).is_none());
    }

    #[test]
    fn sysfs_child_relationship_and_system_protection_are_conservative() {
        let (root, mut provider) = fixture();
        disk(&provider, "sda", 100);
        let partition = provider.sys_block.join("sda/sda1");
        fs::create_dir_all(partition.join("holders")).unwrap();
        fs::write(partition.join("partition"), "1").unwrap();
        fs::write(partition.join("start"), "8").unwrap();
        fs::write(partition.join("size"), "80").unwrap();
        fs::write(partition.join("dev"), "8:1").unwrap();
        fs::write(&provider.mountinfo, "1 0 8:1 / / rw - ext4 /dev/sda1 rw\n").unwrap();
        let drive = provider.sample().drives.remove(0);
        assert_eq!(drive.topologies.len(), 1);
        assert_eq!(drive.topologies[0].status, "uncertain");
        assert!(drive.topologies[0].unallocated_extents.is_empty());
        assert_eq!(
            drive.topologies[0].partitions[0].protection,
            StorageProtectionState::Protected
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swap_and_holder_dependencies_are_protected() {
        let (root, mut provider) = fixture();
        disk(&provider, "sda", 100);
        for (number, name) in [(1, "sda1"), (2, "sda2")] {
            let partition = provider.sys_block.join("sda").join(name);
            fs::create_dir_all(partition.join("holders")).unwrap();
            fs::write(partition.join("partition"), number.to_string()).unwrap();
            fs::write(partition.join("start"), (number * 16).to_string()).unwrap();
            fs::write(partition.join("size"), "8").unwrap();
        }
        fs::write(provider.sys_block.join("sda/sda2/holders/dm-0"), "").unwrap();
        fs::write(
            &provider.swaps,
            "Filename Type Size Used Priority\n/dev/sda1 partition 100 0 -2\n",
        )
        .unwrap();
        let drive = provider.sample().drives.remove(0);
        assert_eq!(drive.topologies[0].partitions.len(), 2);
        assert!(drive.topologies[0]
            .partitions
            .iter()
            .all(|p| p.protection == StorageProtectionState::Protected));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_udisks_objects_never_verify_geometry() {
        let (root, mut provider) = fixture();
        disk(&provider, "sda", 100);
        let mut topology = provider.sample().drives.remove(0).topologies.remove(0);
        verify_udisks_topology(
            &mut topology,
            "drive-test",
            &ManagedObjects::new(),
            &provider.sys_block,
        );
        assert_eq!(topology.status, "uncertain");
        assert!(topology.table_type.is_none());
        assert!(topology.unallocated_extents.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn udisks_snapshot_must_match_device_number_and_partition_geometry() {
        use zbus::names::OwnedInterfaceName;

        fn text_value(value: &str) -> OwnedValue {
            OwnedValue::try_from(zvariant::Value::from(value)).unwrap()
        }
        fn interface(name: &str) -> OwnedInterfaceName {
            OwnedInterfaceName::try_from(name).unwrap()
        }

        let (root, mut provider) = fixture();
        disk(&provider, "sda", 100);
        fs::write(provider.sys_block.join("sda/dev"), "8:0").unwrap();
        fs::write(
            provider.sys_block.join("sda/queue/logical_block_size"),
            "512",
        )
        .unwrap();
        let part = provider.sys_block.join("sda/sda1");
        fs::create_dir_all(&part).unwrap();
        fs::write(part.join("partition"), "1").unwrap();
        fs::write(part.join("start"), "8").unwrap();
        fs::write(part.join("size"), "40").unwrap();
        fs::write(part.join("dev"), "8:1").unwrap();
        let original = provider.sample().drives.remove(0).topologies.remove(0);
        let disk_path =
            OwnedObjectPath::try_from("/org/freedesktop/UDisks2/block_devices/sda").unwrap();
        let part_path =
            OwnedObjectPath::try_from("/org/freedesktop/UDisks2/block_devices/sda1").unwrap();
        let mut objects = ManagedObjects::new();
        for table_type in ["gpt", "dos"] {
            objects.insert(
                disk_path.clone(),
                HashMap::from([
                    (
                        interface("org.freedesktop.UDisks2.Block"),
                        HashMap::from([
                            ("Size".into(), OwnedValue::from(100_u64 * 512)),
                            ("DeviceNumber".into(), OwnedValue::from(libc::makedev(8, 0))),
                        ]),
                    ),
                    (
                        interface("org.freedesktop.UDisks2.PartitionTable"),
                        HashMap::from([("Type".into(), text_value(table_type))]),
                    ),
                ]),
            );
            objects.insert(
                part_path.clone(),
                HashMap::from([
                    (
                        interface("org.freedesktop.UDisks2.Block"),
                        HashMap::from([(
                            "DeviceNumber".into(),
                            OwnedValue::from(libc::makedev(8, 1)),
                        )]),
                    ),
                    (
                        interface("org.freedesktop.UDisks2.Partition"),
                        HashMap::from([
                            (
                                "Table".into(),
                                OwnedValue::try_from(zvariant::Value::from(disk_path.clone()))
                                    .unwrap(),
                            ),
                            ("Number".into(), OwnedValue::from(1_u32)),
                            ("Offset".into(), OwnedValue::from(8_u64 * 512)),
                            ("Size".into(), OwnedValue::from(40_u64 * 512)),
                            ("Type".into(), text_value("0x83")),
                        ]),
                    ),
                ]),
            );
            let mut verified = original.clone();
            verify_udisks_topology(&mut verified, "drive-test", &objects, &provider.sys_block);
            assert_eq!(verified.status, "verified");
            assert_eq!(verified.table_type.as_deref(), Some(table_type));
            assert_eq!(
                verified.partitions[0].partition_type.as_deref(),
                Some("0x83")
            );
            let partition_interface = objects
                .get_mut(&part_path)
                .unwrap()
                .get_mut(&interface("org.freedesktop.UDisks2.Partition"))
                .unwrap();
            partition_interface.insert("Offset".into(), OwnedValue::from(9_u64 * 512));
            let mut mismatch = original.clone();
            verify_udisks_topology(&mut mismatch, "drive-test", &objects, &provider.sys_block);
            assert_eq!(mismatch.status, "uncertain");
            assert!(mismatch.table_type.is_none());
            assert!(mismatch.partitions[0].partition_type.is_none());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mounted_whole_disk_root_is_protected() {
        let (root, mut provider) = fixture();
        disk(&provider, "sda", 100);
        fs::write(provider.sys_block.join("sda/dev"), "8:0").unwrap();
        fs::write(&provider.mountinfo, "1 0 8:0 / / rw - ext4 /dev/sda rw\n").unwrap();
        let topology = provider.sample().drives.remove(0).topologies.remove(0);
        assert_eq!(topology.protection, StorageProtectionState::Protected);
        fs::remove_dir_all(root).unwrap();
    }
}
