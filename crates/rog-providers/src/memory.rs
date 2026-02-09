use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rog_core::{RogError, RogResult, TopProcessMem};

#[derive(Debug, Clone)]
pub struct MemoryTelemetryProvider {
    proc_root: PathBuf,
    zswap_enabled_path: PathBuf,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MemorySnapshot {
    pub mem_total_bytes: Option<u64>,
    pub mem_used_bytes: Option<u64>,
    pub mem_used_percent: Option<f32>,
    pub mem_available_bytes: Option<u64>,
    pub mem_free_bytes: Option<u64>,
    pub mem_cached_bytes: Option<u64>,
    pub mem_buffers_bytes: Option<u64>,
    pub mem_shared_bytes: Option<u64>,
    pub mem_anon_bytes: Option<u64>,

    pub swap_total_bytes: Option<u64>,
    pub swap_used_bytes: Option<u64>,
    pub swap_free_bytes: Option<u64>,

    pub zram_total_bytes: Option<u64>,
    pub zram_used_bytes: Option<u64>,
    pub zswap_enabled: Option<bool>,

    pub psi_mem_some_avg10: Option<f32>,
    pub psi_mem_some_avg60: Option<f32>,
    pub psi_mem_some_avg300: Option<f32>,
    pub psi_mem_full_avg10: Option<f32>,
    pub psi_mem_full_avg60: Option<f32>,
    pub psi_mem_full_avg300: Option<f32>,

    pub mem_active_bytes: Option<u64>,
    pub mem_inactive_bytes: Option<u64>,
    pub mem_dirty_bytes: Option<u64>,
    pub mem_writeback_bytes: Option<u64>,
    pub mem_slab_bytes: Option<u64>,
    pub mem_sreclaimable_bytes: Option<u64>,
    pub mem_sunreclaim_bytes: Option<u64>,
    pub mem_pagetables_bytes: Option<u64>,
    pub mem_kernelstack_bytes: Option<u64>,
    pub mem_mapped_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub struct VmstatSwapCounters {
    pub pswpin_pages: u64,
    pub pswpout_pages: u64,
}

impl Default for MemoryTelemetryProvider {
    fn default() -> Self {
        Self {
            proc_root: PathBuf::from("/proc"),
            zswap_enabled_path: PathBuf::from("/sys/module/zswap/parameters/enabled"),
        }
    }
}

impl MemoryTelemetryProvider {
    pub fn new(proc_root: impl Into<PathBuf>) -> Self {
        Self {
            proc_root: proc_root.into(),
            ..Default::default()
        }
    }

    pub fn read_snapshot(&self) -> RogResult<MemorySnapshot> {
        let meminfo_path = self.proc_root.join("meminfo");
        let txt = fs::read_to_string(&meminfo_path)
            .map_err(|e| RogError::DependencyMissing(format!("meminfo unavailable: {e}")))?;
        let mem = parse_meminfo_bytes(&txt);

        let mem_total_bytes = mem.get("MemTotal").copied();
        let mem_available_bytes = mem
            .get("MemAvailable")
            .copied()
            // Fallback for older kernels; this overestimates a bit, but is better than nothing.
            .or_else(|| {
                let free = mem.get("MemFree").copied()?;
                let buffers = mem.get("Buffers").copied().unwrap_or(0);
                let cached = mem.get("Cached").copied().unwrap_or(0);
                Some(free.saturating_add(buffers).saturating_add(cached))
            });

        let (mem_used_bytes, mem_used_percent) = match (mem_total_bytes, mem_available_bytes) {
            (Some(total), Some(avail)) if total > 0 => {
                let used = total.saturating_sub(avail);
                let pct = (used as f64) / (total as f64) * 100.0;
                let pct = pct.is_finite().then_some(pct as f32);
                (Some(used), pct)
            }
            _ => (None, None),
        };

        let swap_total_bytes = mem.get("SwapTotal").copied();
        let swap_free_bytes = mem.get("SwapFree").copied();
        let swap_used_bytes = match (swap_total_bytes, swap_free_bytes) {
            (Some(total), Some(free)) => Some(total.saturating_sub(free)),
            _ => None,
        };

        let (zram_total_bytes, zram_used_bytes) =
            read_zram_swap_bytes(&self.proc_root.join("swaps"));
        let zswap_enabled = read_zswap_enabled(&self.zswap_enabled_path);
        let psi = read_psi_memory(&self.proc_root.join("pressure").join("memory"));

        Ok(MemorySnapshot {
            mem_total_bytes,
            mem_used_bytes,
            mem_used_percent,
            mem_available_bytes,
            mem_free_bytes: mem.get("MemFree").copied(),
            mem_cached_bytes: mem.get("Cached").copied(),
            mem_buffers_bytes: mem.get("Buffers").copied(),
            mem_shared_bytes: mem.get("Shmem").copied(),
            mem_anon_bytes: mem.get("AnonPages").copied(),

            swap_total_bytes,
            swap_used_bytes,
            swap_free_bytes,

            zram_total_bytes,
            zram_used_bytes,
            zswap_enabled,

            psi_mem_some_avg10: psi.as_ref().and_then(|p| p.some_avg10),
            psi_mem_some_avg60: psi.as_ref().and_then(|p| p.some_avg60),
            psi_mem_some_avg300: psi.as_ref().and_then(|p| p.some_avg300),
            psi_mem_full_avg10: psi.as_ref().and_then(|p| p.full_avg10),
            psi_mem_full_avg60: psi.as_ref().and_then(|p| p.full_avg60),
            psi_mem_full_avg300: psi.as_ref().and_then(|p| p.full_avg300),

            mem_active_bytes: mem.get("Active").copied(),
            mem_inactive_bytes: mem.get("Inactive").copied(),
            mem_dirty_bytes: mem.get("Dirty").copied(),
            mem_writeback_bytes: mem.get("Writeback").copied(),
            mem_slab_bytes: mem.get("Slab").copied(),
            mem_sreclaimable_bytes: mem.get("SReclaimable").copied(),
            mem_sunreclaim_bytes: mem.get("SUnreclaim").copied(),
            mem_pagetables_bytes: mem.get("PageTables").copied(),
            mem_kernelstack_bytes: mem.get("KernelStack").copied(),
            mem_mapped_bytes: mem.get("Mapped").copied(),
        })
    }

    pub fn read_vmstat_swap_counters(&self) -> RogResult<Option<VmstatSwapCounters>> {
        let path = self.proc_root.join("vmstat");
        let txt = fs::read_to_string(&path)
            .map_err(|e| RogError::DependencyMissing(format!("vmstat unavailable: {e}")))?;
        let mut pswpin = None;
        let mut pswpout = None;
        for line in txt.lines() {
            let mut it = line.split_whitespace();
            let Some(k) = it.next() else { continue };
            let Some(v) = it.next() else { continue };
            let Ok(n) = v.parse::<u64>() else { continue };
            match k {
                "pswpin" => pswpin = Some(n),
                "pswpout" => pswpout = Some(n),
                _ => {}
            }
        }
        match (pswpin, pswpout) {
            (Some(i), Some(o)) => Ok(Some(VmstatSwapCounters {
                pswpin_pages: i,
                pswpout_pages: o,
            })),
            _ => Ok(None),
        }
    }

    pub fn read_top_memory_processes_rows(&self, limit: usize) -> RogResult<Vec<TopProcessMem>> {
        let proc_dir = &self.proc_root;
        let passwd_map = read_passwd_users();

        let mut procs: Vec<ProcMem> = Vec::new();
        let entries = fs::read_dir(proc_dir)
            .map_err(|e| RogError::DependencyMissing(format!("procfs unavailable: {e}")))?;
        for ent in entries.flatten() {
            let path = ent.path();
            if !path.is_dir() {
                continue;
            }
            let Some(pid) = path
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };

            let status_path = path.join("status");
            let Ok(status) = fs::read_to_string(status_path) else {
                continue;
            };
            if let Some(p) = parse_proc_status(pid, &status) {
                procs.push(p);
            }
        }

        procs.sort_by(|a, b| {
            b.rss_bytes
                .cmp(&a.rss_bytes)
                .then_with(|| b.swap_bytes.cmp(&a.swap_bytes))
                .then_with(|| a.pid.cmp(&b.pid))
        });

        let mut out = Vec::new();
        for p in procs.into_iter().take(limit.max(1).min(15)) {
            let user = passwd_map
                .get(&p.uid)
                .cloned()
                .unwrap_or_else(|| p.uid.to_string());
            out.push(TopProcessMem {
                user,
                pid: p.pid,
                rss_bytes: p.rss_bytes,
                swap_bytes: p.swap_bytes,
                name: p.name,
            });
        }
        Ok(out)
    }

    pub fn read_top_memory_processes(&self, limit: usize) -> RogResult<String> {
        let rows = self.read_top_memory_processes_rows(limit)?;
        let mut out = String::new();
        out.push_str("USER        PID      RSS     SWAP  NAME\n");
        out.push_str("----------------------------------------\n");
        for p in rows {
            out.push_str(&format!(
                "{user:<10} {pid:>6} {rss:>8} {swap:>8}  {name}\n",
                user = p.user,
                pid = p.pid,
                rss = format_bytes_short(p.rss_bytes),
                swap = format_bytes_short(p.swap_bytes),
                name = p.name
            ));
        }
        Ok(out)
    }
}

#[derive(Debug, Clone)]
struct ProcMem {
    pid: u32,
    uid: u32,
    name: String,
    rss_bytes: u64,
    swap_bytes: u64,
}

fn parse_proc_status(pid: u32, status: &str) -> Option<ProcMem> {
    let mut name = None;
    let mut rss_bytes = None;
    let mut swap_bytes = None;
    let mut uid = None;

    for line in status.lines() {
        if let Some(v) = line.strip_prefix("Name:") {
            let v = v.trim();
            if !v.is_empty() {
                name = Some(v.to_string());
            }
            continue;
        }
        if let Some(v) = line.strip_prefix("VmRSS:") {
            rss_bytes = parse_kib_to_bytes(v);
            continue;
        }
        if let Some(v) = line.strip_prefix("VmSwap:") {
            swap_bytes = parse_kib_to_bytes(v);
            continue;
        }
        if let Some(v) = line.strip_prefix("Uid:") {
            uid = v
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok());
            continue;
        }
    }

    let rss_bytes = rss_bytes?;
    let name = name.unwrap_or_else(|| format!("pid:{pid}"));
    Some(ProcMem {
        pid,
        uid: uid.unwrap_or(0),
        name,
        rss_bytes,
        swap_bytes: swap_bytes.unwrap_or(0),
    })
}

fn parse_kib_to_bytes(s: &str) -> Option<u64> {
    let mut it = s.split_whitespace();
    let v = it.next()?.parse::<u64>().ok()?;
    // meminfo/status use kB meaning KiB.
    Some(v.saturating_mul(1024))
}

fn parse_meminfo_bytes(txt: &str) -> HashMap<String, u64> {
    let mut out = HashMap::new();
    for line in txt.lines() {
        let (k, rest) = match line.split_once(':') {
            Some(v) => v,
            None => continue,
        };
        let mut it = rest.split_whitespace();
        let Some(v) = it.next() else { continue };
        let Some(unit) = it.next() else { continue };
        if unit != "kB" {
            continue;
        }
        let Ok(n) = v.parse::<u64>() else { continue };
        out.insert(k.trim().to_string(), n.saturating_mul(1024));
    }
    out
}

fn read_zram_swap_bytes(swaps_path: &Path) -> (Option<u64>, Option<u64>) {
    let Ok(txt) = fs::read_to_string(swaps_path) else {
        return (None, None);
    };
    let mut total = 0u64;
    let mut used = 0u64;
    let mut found_any = false;

    for (idx, line) in txt.lines().enumerate() {
        if idx == 0 {
            continue; // header
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 {
            continue;
        }
        let filename = cols[0];
        if !filename.contains("zram") {
            continue;
        }
        let Ok(size_kib) = cols[2].parse::<u64>() else {
            continue;
        };
        let Ok(used_kib) = cols[3].parse::<u64>() else {
            continue;
        };
        found_any = true;
        total = total.saturating_add(size_kib.saturating_mul(1024));
        used = used.saturating_add(used_kib.saturating_mul(1024));
    }

    if found_any {
        (Some(total), Some(used))
    } else {
        (None, None)
    }
}

fn read_zswap_enabled(path: &Path) -> Option<bool> {
    let s = fs::read_to_string(path).ok()?;
    let v = s.trim();
    match v {
        "1" | "Y" | "y" | "yes" | "Yes" | "enabled" | "Enabled" => Some(true),
        "0" | "N" | "n" | "no" | "No" | "disabled" | "Disabled" => Some(false),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PsiMemory {
    some_avg10: Option<f32>,
    some_avg60: Option<f32>,
    some_avg300: Option<f32>,
    full_avg10: Option<f32>,
    full_avg60: Option<f32>,
    full_avg300: Option<f32>,
}

fn read_psi_memory(path: &Path) -> Option<PsiMemory> {
    let txt = fs::read_to_string(path).ok()?;
    let mut psi = PsiMemory::default();
    for line in txt.lines() {
        if let Some(rest) = line.strip_prefix("some ") {
            let (a10, a60, a300) = parse_psi_avgs(rest);
            psi.some_avg10 = a10;
            psi.some_avg60 = a60;
            psi.some_avg300 = a300;
        } else if let Some(rest) = line.strip_prefix("full ") {
            let (a10, a60, a300) = parse_psi_avgs(rest);
            psi.full_avg10 = a10;
            psi.full_avg60 = a60;
            psi.full_avg300 = a300;
        }
    }
    Some(psi)
}

fn parse_psi_avgs(rest: &str) -> (Option<f32>, Option<f32>, Option<f32>) {
    let mut a10 = None;
    let mut a60 = None;
    let mut a300 = None;
    for part in rest.split_whitespace() {
        let (k, v) = match part.split_once('=') {
            Some(v) => v,
            None => continue,
        };
        let Ok(n) = v.parse::<f32>() else { continue };
        match k {
            "avg10" => a10 = Some(n),
            "avg60" => a60 = Some(n),
            "avg300" => a300 = Some(n),
            _ => {}
        }
    }
    (a10, a60, a300)
}

fn read_passwd_users() -> HashMap<u32, String> {
    let mut out = HashMap::new();
    let Ok(txt) = fs::read_to_string("/etc/passwd") else {
        return out;
    };
    for line in txt.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split(':');
        let Some(user) = parts.next() else { continue };
        let _pw = parts.next();
        let Some(uid) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        out.insert(uid, user.to_string());
    }
    out
}

fn format_bytes_short(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

    let b = bytes as f64;
    if b >= 10.0 * GIB {
        format!("{:.0}G", b / GIB)
    } else if b >= GIB {
        format!("{:.1}G", b / GIB)
    } else if b >= 10.0 * MIB {
        format!("{:.0}M", b / MIB)
    } else if b >= MIB {
        format!("{:.1}M", b / MIB)
    } else if b >= 10.0 * KIB {
        format!("{:.0}K", b / KIB)
    } else if b >= KIB {
        format!("{:.1}K", b / KIB)
    } else {
        format!("{bytes}B")
    }
}
