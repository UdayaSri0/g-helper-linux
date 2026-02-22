use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use regex::Regex;
use rog_core::{
    AppState, BatteryLimitPercent, BatteryState, CpuCaps, CpuTelemetry, DeviceCaps, GpuMode,
    PerformanceProfile, PowerSource, TelemetrySnapshot,
};
use rog_providers::asusd::AsusdPlatformProvider;
use rog_providers::cpu::CpuTelemetryProvider;
use rog_providers::hwmon::HwmonTelemetryProvider;
use rog_providers::kbd_backlight::KbdBacklightSysfs;
use rog_providers::memory::MemoryTelemetryProvider;
use rog_providers::nvidia_smi::NvidiaSmiTelemetryProvider;
use rog_providers::power_supply::PowerSupplySysfsProvider;
use rog_providers::supergfx::SupergfxProvider;
use rog_providers::traits::{BatteryProvider, GpuProvider, ProfileProvider};
use rog_providers::upower::UPowerProvider;
use tokio::time::{interval, Duration};
use tracing::{info, warn};
use zbus::fdo;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{connection, interface};

const DBUS_NAME: &str = "io.github.roghelper.Daemon";
const DBUS_PATH: &str = "/io/github/roghelper/Daemon";
const DBUS_IFACE: &str = "io.github.roghelper.Daemon1";

#[derive(Debug)]
struct SharedState {
    inner: RwLock<AppState>,
    control: RwLock<ControlState>,
}

#[derive(Debug, Clone, Default)]
struct ControlState {
    profile: Option<PerformanceProfile>,
    gpu_mode: Option<GpuMode>,
    battery_limit: Option<BatteryLimitPercent>,
}

#[derive(Debug, Clone)]
struct RogHelperDaemon {
    state: Arc<SharedState>,
    kbd_backlight: Option<KbdBacklightSysfs>,
    asusd: Option<AsusdPlatformProvider>,
    supergfx: Option<SupergfxProvider>,
    cpu: CpuTelemetryProvider,
}

impl RogHelperDaemon {
    fn new(
        state: Arc<SharedState>,
        kbd_backlight: Option<KbdBacklightSysfs>,
        asusd: Option<AsusdPlatformProvider>,
        supergfx: Option<SupergfxProvider>,
        cpu: CpuTelemetryProvider,
    ) -> Self {
        Self {
            state,
            kbd_backlight,
            asusd,
            supergfx,
            cpu,
        }
    }

    fn read_state(&self) -> AppState {
        self.state.inner.read().expect("rwlock poisoned").clone()
    }

    fn set_telemetry(&self, telemetry: TelemetrySnapshot, warnings: Vec<String>) {
        let mut guard = self.state.inner.write().expect("rwlock poisoned");
        guard.telemetry = telemetry;
        guard.warnings = warnings;
    }

    fn set_cpu_telemetry(&self, cpu: CpuTelemetry) {
        let mut guard = self.state.inner.write().expect("rwlock poisoned");
        guard.cpu = cpu;
    }

    fn set_cpu_caps(&self, caps: CpuCaps) {
        let mut guard = self.state.inner.write().expect("rwlock poisoned");
        guard.cpu_caps = caps;
    }

    fn read_control_state(&self) -> ControlState {
        self.state.control.read().expect("rwlock poisoned").clone()
    }

    fn set_control_state(
        &self,
        profile: Option<PerformanceProfile>,
        gpu_mode: Option<GpuMode>,
        battery_limit: Option<BatteryLimitPercent>,
    ) {
        let mut guard = self.state.control.write().expect("rwlock poisoned");
        guard.profile = profile;
        guard.gpu_mode = gpu_mode;
        guard.battery_limit = battery_limit;
    }

    fn refresh_cpu_state(&self) -> rog_core::RogResult<()> {
        let (timestamp_ms, temp_c) = {
            let state = self.state.inner.read().expect("rwlock poisoned");
            (state.telemetry.timestamp_ms, state.telemetry.cpu_temp_c)
        };
        let cpu = self.cpu.read_snapshot(timestamp_ms, temp_c)?;
        self.set_cpu_telemetry(cpu);
        Ok(())
    }
}

#[interface(name = "io.github.roghelper.Daemon1")]
impl RogHelperDaemon {
    fn get_caps(&self) -> HashMap<String, OwnedValue> {
        caps_to_dbus(&self.read_state().caps)
    }

    fn get_state(&self) -> HashMap<String, OwnedValue> {
        let state = self.read_state();
        let mut m = state_to_dbus(&state);
        m.insert(
            "cpu_caps".to_string(),
            ov(cpu_caps_to_dbus(&state.cpu_caps)),
        );
        m.insert("cpu".to_string(), ov(cpu_to_dbus(&state.cpu)));
        if let Some(l) = self.lighting_to_dbus() {
            m.insert("lighting".to_string(), ov(l));
        }
        let control = self.read_control_state();
        if let Some(p) = control.profile {
            m.insert("profile".to_string(), ov(profile_to_str(p)));
        }
        if let Some(g) = control.gpu_mode {
            m.insert("gpu_mode".to_string(), ov(gpu_mode_to_str(g)));
        }
        if let Some(lim) = control.battery_limit {
            m.insert("battery_limit".to_string(), OwnedValue::from(lim.0 as u64));
        }
        m
    }

    fn get_telemetry(&self) -> HashMap<String, OwnedValue> {
        telemetry_to_dbus(&self.read_state().telemetry)
    }

    fn get_cpu_caps(&self) -> HashMap<String, OwnedValue> {
        cpu_caps_to_dbus(&self.read_state().cpu_caps)
    }

    fn get_cpu_telemetry(&self) -> HashMap<String, OwnedValue> {
        cpu_to_dbus(&self.read_state().cpu)
    }

    fn get_cpu_diagnostics(&self) -> String {
        let state = self.read_state();
        cpu_diagnostics_text(&state.cpu_caps, &state.cpu)
    }

    fn set_lighting(&self, state: HashMap<String, OwnedValue>) -> fdo::Result<()> {
        let Some(kbd) = &self.kbd_backlight else {
            return Err(fdo::Error::NotSupported(
                "lighting not supported on this system".to_string(),
            ));
        };

        let mode = state
            .get("mode")
            .and_then(|v| <&str>::try_from(v).ok())
            .unwrap_or("Static");
        let rgb_hex = state.get("rgb_hex").and_then(|v| <&str>::try_from(v).ok());
        let mut brightness = u64_from_map(&state, "brightness").unwrap_or(0);
        if mode.eq_ignore_ascii_case("off") {
            brightness = 0;
        }

        if rgb_hex.is_some() {
            return Err(fdo::Error::NotSupported(
                "RGB color is not supported by the sysfs LED backend (requires asusd/Aura)."
                    .to_string(),
            ));
        }
        if !mode.eq_ignore_ascii_case("off") && !mode.eq_ignore_ascii_case("static") {
            return Err(fdo::Error::NotSupported(format!(
                "Lighting mode '{mode}' is not supported by the sysfs LED backend."
            )));
        }

        kbd.set_brightness(brightness as u32)
            .map_err(map_rog_error_to_fdo)?;
        Ok(())
    }

    async fn set_profile(&self, profile: &str) -> fdo::Result<()> {
        let Some(asusd) = &self.asusd else {
            return Err(fdo::Error::NotSupported(
                "profile control requires asusd platform interface".to_string(),
            ));
        };

        let profile = parse_profile_request(profile).map_err(fdo::Error::InvalidArgs)?;
        asusd
            .set_profile(profile.clone())
            .await
            .map_err(map_rog_error_to_fdo)?;

        let confirmed = asusd.get_profile().await.unwrap_or(profile);
        let mut guard = self.state.control.write().expect("rwlock poisoned");
        guard.profile = Some(confirmed);
        Ok(())
    }

    async fn set_gpu_mode(&self, mode: &str) -> fdo::Result<()> {
        let Some(supergfx) = &self.supergfx else {
            return Err(fdo::Error::NotSupported(
                "GPU mode control requires supergfxd".to_string(),
            ));
        };

        let mode = parse_gpu_mode_request(mode).map_err(fdo::Error::InvalidArgs)?;
        supergfx
            .set_mode(mode.clone())
            .await
            .map_err(map_rog_error_to_fdo)?;

        let confirmed = supergfx.get_mode().await.unwrap_or(mode);
        let mut guard = self.state.control.write().expect("rwlock poisoned");
        guard.gpu_mode = Some(confirmed);
        Ok(())
    }

    async fn set_battery_limit(&self, limit: u64) -> fdo::Result<()> {
        let Some(asusd) = &self.asusd else {
            return Err(fdo::Error::NotSupported(
                "battery limit control requires asusd platform interface".to_string(),
            ));
        };

        let limit = u8::try_from(limit)
            .map_err(|_| fdo::Error::InvalidArgs("battery limit must fit into u8".to_string()))?;
        asusd
            .set_limit(BatteryLimitPercent(limit))
            .await
            .map_err(map_rog_error_to_fdo)?;

        let confirmed = asusd
            .get_limit()
            .await
            .unwrap_or(BatteryLimitPercent(limit));
        let mut guard = self.state.control.write().expect("rwlock poisoned");
        guard.battery_limit = Some(confirmed);
        Ok(())
    }

    async fn set_cpu_turbo(&self, enabled: bool) -> fdo::Result<()> {
        self.cpu.set_boost(enabled).map_err(map_rog_error_to_fdo)?;
        self.refresh_cpu_state().map_err(map_rog_error_to_fdo)?;
        Ok(())
    }

    async fn set_cpu_power_mode(&self, mode: &str) -> fdo::Result<()> {
        self.cpu
            .set_power_mode(mode)
            .map_err(map_rog_error_to_fdo)?;
        self.refresh_cpu_state().map_err(map_rog_error_to_fdo)?;
        Ok(())
    }

    async fn set_cpu_governor(&self, governor: &str) -> fdo::Result<()> {
        self.cpu
            .set_governor(governor)
            .map_err(map_rog_error_to_fdo)?;
        self.refresh_cpu_state().map_err(map_rog_error_to_fdo)?;
        Ok(())
    }

    async fn set_cpu_epp(&self, epp: &str) -> fdo::Result<()> {
        self.cpu.set_epp(epp).map_err(map_rog_error_to_fdo)?;
        self.refresh_cpu_state().map_err(map_rog_error_to_fdo)?;
        Ok(())
    }

    async fn set_cpu_freq_limits(&self, min_mhz: u64, max_mhz: u64) -> fdo::Result<()> {
        let min = if min_mhz == 0 {
            None
        } else {
            Some(u32::try_from(min_mhz).map_err(|_| {
                fdo::Error::InvalidArgs("min_mhz does not fit into u32".to_string())
            })?)
        };
        let max = if max_mhz == 0 {
            None
        } else {
            Some(u32::try_from(max_mhz).map_err(|_| {
                fdo::Error::InvalidArgs("max_mhz does not fit into u32".to_string())
            })?)
        };

        self.cpu
            .set_freq_limits(min, max)
            .map_err(map_rog_error_to_fdo)?;
        self.refresh_cpu_state().map_err(map_rog_error_to_fdo)?;
        Ok(())
    }

    async fn set_cpu_core_online(&self, core_id: u64, online: bool) -> fdo::Result<()> {
        let core_id = u32::try_from(core_id)
            .map_err(|_| fdo::Error::InvalidArgs("core_id does not fit into u32".to_string()))?;
        self.cpu
            .set_core_online(core_id, online)
            .map_err(map_rog_error_to_fdo)?;
        self.refresh_cpu_state().map_err(map_rog_error_to_fdo)?;
        Ok(())
    }
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

    info!("starting rog-helperd");

    let upower = UPowerProvider::connect_system()
        .await
        .context("connect to UPower")?;
    let hwmon = HwmonTelemetryProvider::default();
    let cpu = CpuTelemetryProvider::default();
    let nvidia = NvidiaSmiTelemetryProvider::default();
    let memory = MemoryTelemetryProvider::default();
    let power_supply = PowerSupplySysfsProvider::default();
    let kbd_backlight = match KbdBacklightSysfs::probe() {
        Ok(v) => v,
        Err(e) => {
            warn!("kbd backlight probe failed: {e}");
            None
        }
    };
    let asusd = match AsusdPlatformProvider::connect_system().await {
        Ok(v) => v,
        Err(e) => {
            warn!("asusd platform provider probe failed: {e}");
            None
        }
    };
    let supergfx = match SupergfxProvider::connect_system().await {
        Ok(v) => v,
        Err(e) => {
            warn!("supergfx provider probe failed: {e}");
            None
        }
    };

    let mut caps = DeviceCaps::unknown();
    let hw_snapshot = hwmon.read_snapshot().unwrap_or_else(|e| {
        warn!("hwmon read failed: {e}");
        TelemetrySnapshot::empty_now(now_ms())
    });
    let cpu_caps = cpu.probe_caps();
    let initial_cpu = cpu
        .read_snapshot(now_ms(), hw_snapshot.cpu_temp_c)
        .unwrap_or_else(|e| {
            warn!("cpu telemetry read failed: {e}");
            CpuTelemetry::empty_now(now_ms())
        });
    caps.has_fan_reading = !hw_snapshot.fans_rpm.is_empty();
    caps.has_kbd_backlight = kbd_backlight.is_some() || detect_kbd_backlight();
    let mut control_state = ControlState::default();

    if let Some(kbd) = &kbd_backlight {
        caps.endpoints.push(format!("sysfs-led:{}", kbd.led_name()));
        caps.notes.push(format!(
            "Keyboard backlight via {} (max {}, writable={}).",
            kbd.led_name(),
            kbd.max_brightness(),
            kbd.can_set_brightness()
        ));
    }

    caps.notes.push(format!(
        "CPU controls: cpufreq={}, epp={}, boost={}, writable={}.",
        cpu_caps.has_cpufreq, cpu_caps.has_epp, cpu_caps.has_boost_toggle, cpu_caps.policy_writable
    ));
    if cpu_caps.has_package_power {
        caps.notes
            .push("CPU package power telemetry via RAPL is available.".to_string());
    }
    for path in &cpu_caps.sysfs_paths {
        caps.endpoints.push(format!("cpu-sysfs:{path}"));
    }

    match &asusd {
        Some(provider) => {
            caps.endpoints
                .push(format!("asusd-platform:{}", provider.endpoint_tag()));
            match provider.probe_caps().await {
                Ok(pcaps) => {
                    caps.has_profiles = pcaps.has_profiles;
                    caps.has_charge_limit = pcaps.has_charge_limit;
                    if !pcaps.profile_choices.is_empty() {
                        let choices = pcaps
                            .profile_choices
                            .iter()
                            .map(|p| profile_to_str(p.clone()))
                            .collect::<Vec<_>>()
                            .join(", ");
                        caps.notes
                            .push(format!("asusd profiles available: {choices}"));
                    }
                }
                Err(e) => caps
                    .notes
                    .push(format!("asusd platform probing failed: {e}")),
            }

            if caps.has_profiles {
                match provider.get_profile().await {
                    Ok(v) => control_state.profile = Some(v),
                    Err(e) => caps
                        .notes
                        .push(format!("could not read current profile from asusd: {e}")),
                }
            }
            if caps.has_charge_limit {
                match provider.get_limit().await {
                    Ok(v) => control_state.battery_limit = Some(v),
                    Err(e) => caps
                        .notes
                        .push(format!("could not read charge limit from asusd: {e}")),
                }
            }
        }
        None => caps
            .notes
            .push("asusd not detected; profile/charge controls disabled.".to_string()),
    }

    match &supergfx {
        Some(provider) => {
            caps.endpoints
                .push(format!("supergfxd:{}", provider.endpoint_tag()));
            match provider.probe_caps().await {
                Ok(gcaps) => {
                    caps.has_gpu_modes = !gcaps.raw_supported_modes.is_empty();
                    caps.requires_reboot_for_gpu_switch = gcaps.requires_reboot_hint;
                    if !gcaps.raw_supported_modes.is_empty() {
                        caps.notes.push(format!(
                            "supergfxd supported modes: {}",
                            gcaps.raw_supported_modes.join(", ")
                        ));
                    }
                }
                Err(e) => caps.notes.push(format!("supergfxd probing failed: {e}")),
            }

            if caps.has_gpu_modes {
                match provider.get_mode().await {
                    Ok(v) => control_state.gpu_mode = Some(v),
                    Err(e) => caps.notes.push(format!(
                        "could not read current GPU mode from supergfxd: {e}"
                    )),
                }
            }
        }
        None => caps
            .notes
            .push("supergfxd not detected; GPU mode controls disabled.".to_string()),
    }

    // Diagnostics: include relevant system bus names (no asusd/supergfxd hardcoding).
    let re = Regex::new("(?i)asus|rog|supergfx|power|upower").expect("valid regex");
    if let Ok(names) = rog_providers::dbus::list_system_bus_names_matching(&re).await {
        for n in names {
            caps.endpoints.push(format!("system-bus-name:{n}"));
        }
    }

    let has_profiles = caps.has_profiles;
    let has_charge_limit = caps.has_charge_limit;
    let has_gpu_modes = caps.has_gpu_modes;
    let cpu_policy_writable = cpu_caps.policy_writable;

    let mut init_state = AppState::new(caps, hw_snapshot);
    init_state.cpu_caps = cpu_caps.clone();
    init_state.cpu = initial_cpu;
    let shared = Arc::new(SharedState {
        inner: RwLock::new(init_state),
        control: RwLock::new(control_state),
    });

    // Export DBus service on the session bus.
    let daemon = RogHelperDaemon::new(
        shared.clone(),
        kbd_backlight.clone(),
        asusd.clone(),
        supergfx.clone(),
        cpu.clone(),
    );
    daemon.set_cpu_caps(cpu_caps.clone());
    let daemon_iface = daemon.clone();
    let _conn = connection::Builder::session()?
        .name(DBUS_NAME)?
        .serve_at(DBUS_PATH, daemon_iface)?
        .build()
        .await?;

    info!("DBus service online: {DBUS_NAME} {DBUS_PATH} ({DBUS_IFACE})");

    // Telemetry loop.
    let mut ticker = interval(Duration::from_secs(1));
    let mut last_swap: Option<(u64, u64, u64)> = None; // (ts_ms, pswpin, pswpout)
    let mut next_top_update_ms: u64 = 0;
    let mut cached_top_rows: Option<Vec<rog_core::TopProcessMem>> = None;
    let mut cached_top_text: Option<String> = None;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let mut warnings = Vec::new();

                let mut telemetry = hwmon.read_snapshot().unwrap_or_else(|e| {
                    warnings.push(format!("hwmon unavailable: {e}"));
                    TelemetrySnapshot::empty_now(now_ms())
                });

                // Best-effort memory (RAM/swap) telemetry from procfs.
                match memory.read_snapshot() {
                    Ok(mem) => {
                        telemetry.mem_total_bytes = mem.mem_total_bytes;
                        telemetry.mem_used_bytes = mem.mem_used_bytes;
                        telemetry.mem_used_percent = mem.mem_used_percent;
                        telemetry.mem_available_bytes = mem.mem_available_bytes;
                        telemetry.mem_free_bytes = mem.mem_free_bytes;
                        telemetry.mem_cached_bytes = mem.mem_cached_bytes;
                        telemetry.mem_buffers_bytes = mem.mem_buffers_bytes;
                        telemetry.mem_shared_bytes = mem.mem_shared_bytes;
                        telemetry.mem_anon_bytes = mem.mem_anon_bytes;

                        telemetry.swap_total_bytes = mem.swap_total_bytes;
                        telemetry.swap_used_bytes = mem.swap_used_bytes;
                        telemetry.swap_free_bytes = mem.swap_free_bytes;
                        telemetry.zram_total_bytes = mem.zram_total_bytes;
                        telemetry.zram_used_bytes = mem.zram_used_bytes;
                        telemetry.zswap_enabled = mem.zswap_enabled;

                        telemetry.psi_mem_some_avg10 = mem.psi_mem_some_avg10;
                        telemetry.psi_mem_some_avg60 = mem.psi_mem_some_avg60;
                        telemetry.psi_mem_some_avg300 = mem.psi_mem_some_avg300;
                        telemetry.psi_mem_full_avg10 = mem.psi_mem_full_avg10;
                        telemetry.psi_mem_full_avg60 = mem.psi_mem_full_avg60;
                        telemetry.psi_mem_full_avg300 = mem.psi_mem_full_avg300;

                        telemetry.mem_active_bytes = mem.mem_active_bytes;
                        telemetry.mem_inactive_bytes = mem.mem_inactive_bytes;
                        telemetry.mem_dirty_bytes = mem.mem_dirty_bytes;
                        telemetry.mem_writeback_bytes = mem.mem_writeback_bytes;
                        telemetry.mem_slab_bytes = mem.mem_slab_bytes;
                        telemetry.mem_sreclaimable_bytes = mem.mem_sreclaimable_bytes;
                        telemetry.mem_sunreclaim_bytes = mem.mem_sunreclaim_bytes;
                        telemetry.mem_pagetables_bytes = mem.mem_pagetables_bytes;
                        telemetry.mem_kernelstack_bytes = mem.mem_kernelstack_bytes;
                        telemetry.mem_mapped_bytes = mem.mem_mapped_bytes;
                    }
                    Err(e) => warnings.push(format!("memory telemetry unavailable: {e}")),
                }

                // Swap in/out rate (vmstat deltas).
                match memory.read_vmstat_swap_counters() {
                    Ok(Some(c)) => {
                        let now = now_ms();
                        if let Some((prev_ts, prev_in, prev_out)) = last_swap {
                            let dt_ms = now.saturating_sub(prev_ts);
                            if dt_ms > 0 {
                                let dt_s = (dt_ms as f32) / 1000.0;
                                if c.pswpin_pages >= prev_in {
                                    let din = (c.pswpin_pages - prev_in) as f32 / dt_s;
                                    telemetry.swap_in_pages_per_s = Some(din);
                                }
                                if c.pswpout_pages >= prev_out {
                                    let dout = (c.pswpout_pages - prev_out) as f32 / dt_s;
                                    telemetry.swap_out_pages_per_s = Some(dout);
                                }
                            }
                        }
                        last_swap = Some((now, c.pswpin_pages, c.pswpout_pages));
                    }
                    Ok(None) => {}
                    Err(e) => warnings.push(format!("vmstat unavailable: {e}")),
                }

                // Top memory users (update less frequently to keep proc scanning cheap).
                let now = now_ms();
                if now >= next_top_update_ms {
                    match memory.read_top_memory_processes_rows(10) {
                        Ok(rows) => {
                            cached_top_text = Some(format_top_processes_text(&rows));
                            cached_top_rows = Some(rows);
                        }
                        Err(e) => warnings.push(format!("top processes unavailable: {e}")),
                    }
                    next_top_update_ms = now.saturating_add(5_000);
                }
                telemetry.mem_top_processes = cached_top_text.clone();
                telemetry.mem_top_processes_rows = cached_top_rows.clone();

                // Best-effort battery details via sysfs (fills gaps UPower might not expose).
                match power_supply.read_battery_status() {
                    Ok(st) => {
                        telemetry.battery_health_percent =
                            telemetry.battery_health_percent.or(st.battery_health_percent);
                        telemetry.battery_cycle_count =
                            telemetry.battery_cycle_count.or(st.battery_cycle_count);
                        telemetry.battery_charge_power_w =
                            telemetry.battery_charge_power_w.or(st.battery_charge_power_w);
                        telemetry.battery_discharge_power_w =
                            telemetry.battery_discharge_power_w.or(st.battery_discharge_power_w);
                        telemetry.battery_state = telemetry.battery_state.or(st.battery_state);
                    }
                    Err(e) => warnings.push(format!("power_supply sysfs unavailable: {e}")),
                }

                // Best-effort NVIDIA telemetry (read-only).
                if telemetry.gpu_temp_c.is_none() {
                    match nvidia.read_gpu_temp_c().await {
                        Ok(Some(temp)) => {
                            telemetry.gpu_temp_c = Some(temp);
                            telemetry
                                .temps_c
                                .insert("nvidia-smi:gpu_temp".to_string(), temp);
                        }
                        Ok(None) => {}
                        Err(e) => warnings.push(format!("nvidia-smi telemetry unavailable: {e}")),
                    }
                }

                match upower.read_status().await {
                    Ok(st) => {
                        telemetry.power_source = st.power_source;
                        telemetry.battery_percent = st.battery_percent;
                        telemetry.ac_online = st.ac_online;
                        telemetry.battery_state = st.battery_state.or(telemetry.battery_state);
                        telemetry.battery_health_percent =
                            telemetry.battery_health_percent.or(st.battery_health_percent);
                        telemetry.battery_cycle_count =
                            telemetry.battery_cycle_count.or(st.battery_cycle_count);
                        telemetry.battery_charge_power_w =
                            st.battery_charge_power_w.or(telemetry.battery_charge_power_w);
                        telemetry.battery_discharge_power_w =
                            st.battery_discharge_power_w.or(telemetry.battery_discharge_power_w);
                        telemetry.battery_time_to_empty_s = st.battery_time_to_empty_s;
                        telemetry.battery_time_to_full_s = st.battery_time_to_full_s;
                        // Use UPower's timestamp as the authoritative timestamp if hwmon failed.
                        telemetry.timestamp_ms = telemetry.timestamp_ms.max(st.timestamp_ms);
                    }
                    Err(e) => {
                        warnings.push(format!("UPower unavailable: {e}"));
                    }
                }

                if let Some(kbd) = &kbd_backlight {
                    if !kbd.can_set_brightness() {
                        warnings.push(format!(
                            "Keyboard backlight ({}) is read-only (need asusd or a udev rule).",
                            kbd.led_name()
                        ));
                    }
                }
                if !cpu_policy_writable {
                    warnings.push(
                        "CPU controls are read-only for this user (check daemon permissions)."
                            .to_string(),
                    );
                }

                match cpu.read_snapshot(now_ms(), telemetry.cpu_temp_c) {
                    Ok(cpu_snapshot) => daemon.set_cpu_telemetry(cpu_snapshot),
                    Err(e) => warnings.push(format!("cpu telemetry unavailable: {e}")),
                }

                let mut current_profile = None;
                let mut current_gpu_mode = None;
                let mut current_battery_limit = None;

                if let Some(provider) = &asusd {
                    if has_profiles {
                        match provider.get_profile().await {
                            Ok(v) => current_profile = Some(v),
                            Err(e) => warnings.push(format!("profile read failed: {e}")),
                        }
                    }
                    if has_charge_limit {
                        match provider.get_limit().await {
                            Ok(v) => current_battery_limit = Some(v),
                            Err(e) => warnings.push(format!("charge limit read failed: {e}")),
                        }
                    }
                }

                if let Some(provider) = &supergfx {
                    if has_gpu_modes {
                        match provider.get_mode().await {
                            Ok(v) => current_gpu_mode = Some(v),
                            Err(e) => warnings.push(format!("GPU mode read failed: {e}")),
                        }
                        if let Ok(Some(hint)) = provider.can_switch_now().await {
                            warnings.push(format!("GPU switch hint: {hint}"));
                        }
                    }
                }

                daemon.set_control_state(current_profile, current_gpu_mode, current_battery_limit);
                daemon.set_telemetry(telemetry, warnings);
            }
            _ = tokio::signal::ctrl_c() => {
                info!("ctrl-c received; exiting");
                break;
            }
        }
    }

    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn detect_kbd_backlight() -> bool {
    // Best-effort sysfs check; capability-driven UI still needs provider support later.
    let Ok(entries) = std::fs::read_dir("/sys/class/leds") else {
        return false;
    };
    for ent in entries.flatten() {
        let Some(name) = ent.file_name().to_str().map(|s| s.to_ascii_lowercase()) else {
            continue;
        };
        if name.contains("kbd")
            || name.contains("kbd_backlight")
            || (name.contains("asus") && name.contains("keyboard"))
        {
            return true;
        }
    }
    false
}

fn state_to_dbus(s: &AppState) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert("caps".to_string(), ov(caps_to_dbus(&s.caps)));
    m.insert("telemetry".to_string(), ov(telemetry_to_dbus(&s.telemetry)));
    m.insert("warnings".to_string(), ov(s.warnings.clone()));
    m
}

fn caps_to_dbus(c: &DeviceCaps) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert("has_profiles".to_string(), OwnedValue::from(c.has_profiles));
    m.insert(
        "has_fan_curves".to_string(),
        OwnedValue::from(c.has_fan_curves),
    );
    m.insert(
        "has_fan_reading".to_string(),
        OwnedValue::from(c.has_fan_reading),
    );
    m.insert(
        "has_charge_limit".to_string(),
        OwnedValue::from(c.has_charge_limit),
    );
    m.insert(
        "has_gpu_modes".to_string(),
        OwnedValue::from(c.has_gpu_modes),
    );
    m.insert("has_aura".to_string(), OwnedValue::from(c.has_aura));
    m.insert(
        "has_kbd_backlight".to_string(),
        OwnedValue::from(c.has_kbd_backlight),
    );
    m.insert(
        "requires_reboot_for_gpu_switch".to_string(),
        OwnedValue::from(c.requires_reboot_for_gpu_switch),
    );
    m.insert("endpoints".to_string(), ov(c.endpoints.clone()));
    m.insert("notes".to_string(), ov(c.notes.clone()));
    m
}

fn cpu_caps_to_dbus(c: &CpuCaps) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert("has_cpufreq".to_string(), OwnedValue::from(c.has_cpufreq));
    m.insert("has_epp".to_string(), OwnedValue::from(c.has_epp));
    m.insert(
        "has_boost_toggle".to_string(),
        OwnedValue::from(c.has_boost_toggle),
    );
    m.insert(
        "has_package_power".to_string(),
        OwnedValue::from(c.has_package_power),
    );
    m.insert(
        "policy_writable".to_string(),
        OwnedValue::from(c.policy_writable),
    );
    m.insert(
        "has_min_freq_limit".to_string(),
        OwnedValue::from(c.has_min_freq_limit),
    );
    m.insert(
        "has_max_freq_limit".to_string(),
        OwnedValue::from(c.has_max_freq_limit),
    );
    m.insert("has_governor".to_string(), OwnedValue::from(c.has_governor));
    m.insert(
        "has_core_online".to_string(),
        OwnedValue::from(c.has_core_online),
    );
    if let Some(v) = &c.scaling_driver {
        m.insert("scaling_driver".to_string(), ov(v.clone()));
    }
    m.insert(
        "cpu_count".to_string(),
        OwnedValue::from(c.cpu_count as u64),
    );
    m.insert(
        "thread_count".to_string(),
        OwnedValue::from(c.thread_count as u64),
    );
    m.insert(
        "governor_choices".to_string(),
        ov(c.governor_choices.clone()),
    );
    m.insert("epp_choices".to_string(), ov(c.epp_choices.clone()));
    m.insert("sysfs_paths".to_string(), ov(c.sysfs_paths.clone()));
    m
}

fn cpu_to_dbus(c: &CpuTelemetry) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert("timestamp_ms".to_string(), OwnedValue::from(c.timestamp_ms));
    if let Some(v) = c.temp_c {
        m.insert("temp_c".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = c.usage_percent {
        m.insert("usage_percent".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = c.avg_freq_mhz {
        m.insert("avg_freq_mhz".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = c.package_power_w {
        m.insert("package_power_w".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = &c.status {
        m.insert("status".to_string(), ov(v.clone()));
    }
    if let Some(v) = c.turbo_boost_enabled {
        m.insert("turbo_boost_enabled".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = &c.governor {
        m.insert("governor".to_string(), ov(v.clone()));
    }
    if let Some(v) = &c.epp {
        m.insert("epp".to_string(), ov(v.clone()));
    }
    if let Some(v) = c.min_freq_mhz {
        m.insert("min_freq_mhz".to_string(), OwnedValue::from(v as u64));
    }
    if let Some(v) = c.max_freq_mhz {
        m.insert("max_freq_mhz".to_string(), OwnedValue::from(v as u64));
    }
    if !c.per_core.is_empty() {
        let mut rows: Vec<HashMap<String, OwnedValue>> = Vec::with_capacity(c.per_core.len());
        for core in &c.per_core {
            let mut row = HashMap::new();
            row.insert("core_id".to_string(), OwnedValue::from(core.core_id as u64));
            if let Some(v) = core.usage_percent {
                row.insert("usage_percent".to_string(), OwnedValue::from(v as f64));
            }
            if let Some(v) = core.current_freq_mhz {
                row.insert("current_freq_mhz".to_string(), OwnedValue::from(v as u64));
            }
            if let Some(v) = core.min_freq_mhz {
                row.insert("min_freq_mhz".to_string(), OwnedValue::from(v as u64));
            }
            if let Some(v) = core.max_freq_mhz {
                row.insert("max_freq_mhz".to_string(), OwnedValue::from(v as u64));
            }
            row.insert("online".to_string(), OwnedValue::from(core.online));
            rows.push(row);
        }
        m.insert("per_core".to_string(), ov(rows));
    }
    m
}

fn telemetry_to_dbus(t: &TelemetrySnapshot) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    m.insert("timestamp_ms".to_string(), OwnedValue::from(t.timestamp_ms));

    if let Some(v) = t.cpu_temp_c {
        m.insert("cpu_temp_c".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.gpu_temp_c {
        m.insert("gpu_temp_c".to_string(), OwnedValue::from(v as f64));
    }
    if !t.temps_c.is_empty() {
        let temps: HashMap<String, f64> = t
            .temps_c
            .iter()
            .map(|(k, v)| (k.clone(), *v as f64))
            .collect();
        m.insert("temps_c".to_string(), OwnedValue::from(temps));
    }
    if !t.fans_rpm.is_empty() {
        let fans: HashMap<String, u32> = t.fans_rpm.iter().map(|(k, v)| (k.clone(), *v)).collect();
        m.insert("fans_rpm".to_string(), OwnedValue::from(fans));
    }
    if let Some(ps) = t.power_source {
        m.insert("power_source".to_string(), ov(power_source_to_str(ps)));
    }
    if let Some(v) = t.battery_percent {
        m.insert("battery_percent".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.ac_online {
        m.insert("ac_online".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.battery_state {
        m.insert("battery_state".to_string(), ov(battery_state_to_str(v)));
    }
    if let Some(v) = t.battery_health_percent {
        m.insert(
            "battery_health_percent".to_string(),
            OwnedValue::from(v as f64),
        );
    }
    if let Some(v) = t.battery_cycle_count {
        m.insert("battery_cycle_count".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.battery_charge_power_w {
        m.insert(
            "battery_charge_power_w".to_string(),
            OwnedValue::from(v as f64),
        );
    }
    if let Some(v) = t.battery_discharge_power_w {
        m.insert(
            "battery_discharge_power_w".to_string(),
            OwnedValue::from(v as f64),
        );
    }
    if let Some(v) = t.battery_time_to_empty_s {
        m.insert("battery_time_to_empty_s".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.battery_time_to_full_s {
        m.insert("battery_time_to_full_s".to_string(), OwnedValue::from(v));
    }

    // Memory.
    if let Some(v) = t.mem_total_bytes {
        m.insert("mem_total_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_used_bytes {
        m.insert("mem_used_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_used_percent {
        m.insert("mem_used_percent".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.mem_available_bytes {
        m.insert("mem_available_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_free_bytes {
        m.insert("mem_free_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_cached_bytes {
        m.insert("mem_cached_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_buffers_bytes {
        m.insert("mem_buffers_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_shared_bytes {
        m.insert("mem_shared_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_anon_bytes {
        m.insert("mem_anon_bytes".to_string(), OwnedValue::from(v));
    }

    if let Some(v) = t.swap_total_bytes {
        m.insert("swap_total_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.swap_used_bytes {
        m.insert("swap_used_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.swap_free_bytes {
        m.insert("swap_free_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.swap_in_pages_per_s {
        m.insert(
            "swap_in_pages_per_s".to_string(),
            OwnedValue::from(v as f64),
        );
    }
    if let Some(v) = t.swap_out_pages_per_s {
        m.insert(
            "swap_out_pages_per_s".to_string(),
            OwnedValue::from(v as f64),
        );
    }
    if let Some(v) = t.zram_total_bytes {
        m.insert("zram_total_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.zram_used_bytes {
        m.insert("zram_used_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.zswap_enabled {
        m.insert("zswap_enabled".to_string(), OwnedValue::from(v));
    }

    if let Some(v) = t.psi_mem_some_avg10 {
        m.insert("psi_mem_some_avg10".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.psi_mem_some_avg60 {
        m.insert("psi_mem_some_avg60".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.psi_mem_some_avg300 {
        m.insert(
            "psi_mem_some_avg300".to_string(),
            OwnedValue::from(v as f64),
        );
    }
    if let Some(v) = t.psi_mem_full_avg10 {
        m.insert("psi_mem_full_avg10".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.psi_mem_full_avg60 {
        m.insert("psi_mem_full_avg60".to_string(), OwnedValue::from(v as f64));
    }
    if let Some(v) = t.psi_mem_full_avg300 {
        m.insert(
            "psi_mem_full_avg300".to_string(),
            OwnedValue::from(v as f64),
        );
    }

    if let Some(v) = t.mem_active_bytes {
        m.insert("mem_active_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_inactive_bytes {
        m.insert("mem_inactive_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_dirty_bytes {
        m.insert("mem_dirty_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_writeback_bytes {
        m.insert("mem_writeback_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_slab_bytes {
        m.insert("mem_slab_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_sreclaimable_bytes {
        m.insert("mem_sreclaimable_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_sunreclaim_bytes {
        m.insert("mem_sunreclaim_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_pagetables_bytes {
        m.insert("mem_pagetables_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_kernelstack_bytes {
        m.insert("mem_kernelstack_bytes".to_string(), OwnedValue::from(v));
    }
    if let Some(v) = t.mem_mapped_bytes {
        m.insert("mem_mapped_bytes".to_string(), OwnedValue::from(v));
    }

    if let Some(ref v) = t.mem_top_processes {
        m.insert("mem_top_processes".to_string(), ov(v.clone()));
    }
    if let Some(ref rows) = t.mem_top_processes_rows {
        let mut out_rows: Vec<HashMap<String, OwnedValue>> = Vec::with_capacity(rows.len());
        for p in rows {
            let mut r = HashMap::new();
            r.insert("user".to_string(), ov(p.user.clone()));
            r.insert("pid".to_string(), OwnedValue::from(p.pid));
            r.insert("rss_bytes".to_string(), OwnedValue::from(p.rss_bytes));
            r.insert("swap_bytes".to_string(), OwnedValue::from(p.swap_bytes));
            r.insert("name".to_string(), ov(p.name.clone()));
            out_rows.push(r);
        }
        m.insert("mem_top_processes_rows".to_string(), ov(out_rows));
    }
    m
}

fn power_source_to_str(p: PowerSource) -> &'static str {
    match p {
        PowerSource::Ac => "Ac",
        PowerSource::Battery => "Battery",
    }
}

fn battery_state_to_str(s: BatteryState) -> &'static str {
    match s {
        BatteryState::Unknown => "Unknown",
        BatteryState::Charging => "Charging",
        BatteryState::Discharging => "Discharging",
        BatteryState::Empty => "Empty",
        BatteryState::Full => "Full",
        BatteryState::PendingCharge => "PendingCharge",
        BatteryState::PendingDischarge => "PendingDischarge",
        BatteryState::NotCharging => "NotCharging",
    }
}

fn profile_to_str(profile: PerformanceProfile) -> &'static str {
    match profile {
        PerformanceProfile::Silent => "Silent",
        PerformanceProfile::Balanced => "Balanced",
        PerformanceProfile::Turbo => "Turbo",
        PerformanceProfile::Custom(_) => "Custom",
    }
}

fn gpu_mode_to_str(mode: GpuMode) -> &'static str {
    match mode {
        GpuMode::Integrated => "Integrated",
        GpuMode::Hybrid => "Hybrid",
        GpuMode::Dedicated => "Dedicated",
        GpuMode::Other(_) => "Other",
    }
}

fn parse_profile_request(v: &str) -> Result<PerformanceProfile, String> {
    let key: String = v
        .to_ascii_lowercase()
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect();

    match key.as_str() {
        "silent" | "quiet" | "lowpower" => Ok(PerformanceProfile::Silent),
        "balanced" => Ok(PerformanceProfile::Balanced),
        "turbo" | "performance" => Ok(PerformanceProfile::Turbo),
        "custom" => Ok(PerformanceProfile::Custom("Custom".to_string())),
        _ => Err(format!(
            "unknown profile '{v}', expected Silent|Balanced|Turbo"
        )),
    }
}

fn parse_gpu_mode_request(v: &str) -> Result<GpuMode, String> {
    let key: String = v
        .to_ascii_lowercase()
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect();

    match key.as_str() {
        "integrated" => Ok(GpuMode::Integrated),
        "hybrid" | "dynamic" | "optimus" => Ok(GpuMode::Hybrid),
        "dedicated" | "discrete" | "asusmuxdgpu" | "asusegpu" | "vfio" => Ok(GpuMode::Dedicated),
        "other" => Ok(GpuMode::Other("Other".to_string())),
        _ => Err(format!(
            "unknown GPU mode '{v}', expected Integrated|Hybrid|Dedicated"
        )),
    }
}

fn cpu_diagnostics_text(caps: &CpuCaps, cpu: &CpuTelemetry) -> String {
    let mut out = String::new();
    out.push_str("CPU Diagnostics\n");
    out.push_str("================\n");
    out.push_str(&format!("has_cpufreq: {}\n", caps.has_cpufreq));
    out.push_str(&format!("has_governor: {}\n", caps.has_governor));
    out.push_str(&format!("has_epp: {}\n", caps.has_epp));
    out.push_str(&format!("has_boost_toggle: {}\n", caps.has_boost_toggle));
    out.push_str(&format!("has_package_power: {}\n", caps.has_package_power));
    out.push_str(&format!("policy_writable: {}\n", caps.policy_writable));
    out.push_str(&format!("cpu_count: {}\n", caps.cpu_count));
    out.push_str(&format!("thread_count: {}\n", caps.thread_count));
    if let Some(driver) = &caps.scaling_driver {
        out.push_str(&format!("scaling_driver: {driver}\n"));
    }
    if !caps.governor_choices.is_empty() {
        out.push_str(&format!(
            "governor_choices: {}\n",
            caps.governor_choices.join(", ")
        ));
    }
    if !caps.epp_choices.is_empty() {
        out.push_str(&format!("epp_choices: {}\n", caps.epp_choices.join(", ")));
    }

    out.push_str("\nCurrent\n");
    out.push_str("-------\n");
    out.push_str(&format!("temp_c: {:?}\n", cpu.temp_c));
    out.push_str(&format!("usage_percent: {:?}\n", cpu.usage_percent));
    out.push_str(&format!("avg_freq_mhz: {:?}\n", cpu.avg_freq_mhz));
    out.push_str(&format!("package_power_w: {:?}\n", cpu.package_power_w));
    out.push_str(&format!("status: {:?}\n", cpu.status));
    out.push_str(&format!(
        "turbo_boost_enabled: {:?}\n",
        cpu.turbo_boost_enabled
    ));
    out.push_str(&format!("governor: {:?}\n", cpu.governor));
    out.push_str(&format!("epp: {:?}\n", cpu.epp));
    out.push_str(&format!("min_freq_mhz: {:?}\n", cpu.min_freq_mhz));
    out.push_str(&format!("max_freq_mhz: {:?}\n", cpu.max_freq_mhz));

    out.push_str("\nPer-core\n");
    out.push_str("--------\n");
    out.push_str("core  online  usage%  cur_mhz  min_mhz  max_mhz\n");
    for core in &cpu.per_core {
        out.push_str(&format!(
            "{:<4}  {:<6}  {:<6}  {:<7}  {:<7}  {:<7}\n",
            core.core_id,
            if core.online { "yes" } else { "no" },
            core.usage_percent
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".to_string()),
            core.current_freq_mhz
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
            core.min_freq_mhz
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
            core.max_freq_mhz
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ));
    }

    if !caps.sysfs_paths.is_empty() {
        out.push_str("\nDetected sysfs paths\n");
        out.push_str("-------------------\n");
        for p in &caps.sysfs_paths {
            out.push_str(&format!("{p}\n"));
        }
    }

    out
}

fn format_top_processes_text(rows: &[rog_core::TopProcessMem]) -> String {
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

fn ov<T>(v: T) -> OwnedValue
where
    T: Into<Value<'static>>,
{
    OwnedValue::try_from(v.into()).expect("OwnedValue conversion should succeed")
}

fn u64_from_map(map: &HashMap<String, OwnedValue>, key: &str) -> Option<u64> {
    let v = map.get(key)?;
    u64::try_from(v)
        .ok()
        .or_else(|| u32::try_from(v).ok().map(|v| v as u64))
}

fn map_rog_error_to_fdo(e: rog_core::RogError) -> fdo::Error {
    match e {
        rog_core::RogError::DependencyMissing(msg) => fdo::Error::Failed(msg),
        rog_core::RogError::NotSupported(msg) => fdo::Error::NotSupported(msg),
        rog_core::RogError::PermissionDenied(msg) => fdo::Error::AccessDenied(msg),
        rog_core::RogError::InvalidInput(msg) => fdo::Error::InvalidArgs(msg),
        rog_core::RogError::TransientFailure(msg) => fdo::Error::TimedOut(msg),
        rog_core::RogError::Unexpected(msg) => fdo::Error::Failed(msg),
    }
}

impl RogHelperDaemon {
    fn lighting_to_dbus(&self) -> Option<HashMap<String, OwnedValue>> {
        let kbd = self.kbd_backlight.as_ref()?;
        let brightness = kbd.read_brightness().ok()?;
        let max = kbd.max_brightness();
        let can_set = kbd.can_set_brightness();
        let mode = if brightness == 0 { "Off" } else { "Static" };

        let mut m = HashMap::new();
        m.insert("backend".to_string(), ov("sysfs-led"));
        m.insert("device".to_string(), ov(kbd.led_name().to_string()));
        m.insert(
            "brightness".to_string(),
            OwnedValue::from(brightness as u64),
        );
        m.insert("max_brightness".to_string(), OwnedValue::from(max as u64));
        m.insert("can_set".to_string(), OwnedValue::from(can_set));
        m.insert("mode".to_string(), ov(mode));
        m.insert("supports_rgb".to_string(), OwnedValue::from(false));
        m.insert(
            "supported_modes".to_string(),
            ov(vec!["Off".to_string(), "Static".to_string()]),
        );
        Some(m)
    }
}
