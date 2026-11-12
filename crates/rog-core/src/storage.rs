//! Read-only physical storage state shared by providers and clients.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageState {
    pub status: String,
    pub sampled_at_ms: u64,
    pub drives: Vec<StorageDevice>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageDevice {
    /// Opaque stable identifier; never a caller-selected device path.
    pub id: String,
    pub name: String,
    pub model: String,
    pub manufacturer: Option<String>,
    pub kind: String,
    pub transport: String,
    pub capacity_bytes: u64,
    pub logical_block_size: Option<u64>,
    pub physical_block_size: Option<u64>,
    pub rotational: bool,
    pub firmware: Option<String>,
    pub controller: Option<String>,
    pub namespaces: Vec<String>,
    pub volumes: Vec<StorageVolume>,
    pub io: Option<StorageIo>,
    pub temperatures: Vec<StorageTemperature>,
    pub health: StorageHealth,
    pub history: Vec<StorageHistoryPoint>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageVolume {
    pub name: String,
    pub size_bytes: u64,
    pub filesystem: Option<String>,
    pub label: Option<String>,
    pub mount_points: Vec<String>,
    pub used_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub filesystem_size_bytes: Option<u64>,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageIo {
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
    pub read_iops: u64,
    pub write_iops: u64,
    pub read_bytes_since_boot: u64,
    pub write_bytes_since_boot: u64,
    pub busy_percent: Option<u8>,
    pub sampled_at_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageHistoryPoint {
    pub sampled_at_ms: u64,
    pub read_bytes_per_sec: Option<u64>,
    pub write_bytes_per_sec: Option<u64>,
    pub temperature_c: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageTemperature {
    pub label: String,
    pub celsius: f64,
    pub source: String,
    pub sampled_at_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageHealth {
    /// One of healthy, attention, critical, unknown, unsupported,
    /// permission_denied, or stale.
    pub state: String,
    pub source: String,
    pub sampled_at_ms: Option<u64>,
    pub reason: String,
    pub attributes: Vec<StorageAttribute>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageAttribute {
    pub key: String,
    pub value: String,
    pub unit: Option<String>,
    pub warning: bool,
}
