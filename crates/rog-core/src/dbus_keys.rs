//! Stable keys for the public `a{sv}` session-DBus payloads.
//!
//! Keep wire names backwards compatible. Grouped constants are shared by the
//! daemon encoders and UI decoders so a spelling change cannot silently split
//! the contract.

pub const PROFILE_ACCESS_PREFIX: &str = "profile_access";
pub const CHARGE_LIMIT_ACCESS_PREFIX: &str = "charge_limit_access";
pub const GPU_MODE_ACCESS_PREFIX: &str = "gpu_mode_access";
pub const KBD_BACKLIGHT_ACCESS_PREFIX: &str = "kbd_backlight_access";

pub const FEATURE_ACCESS_STATUS_SUFFIX: &str = "_status";
pub const FEATURE_ACCESS_REASON_SUFFIX: &str = "_reason";

pub const TELEMETRY_FAN_ROWS_KEY: &str = "fan_rows";
pub const FAN_ROW_HWMON_DEVICE_KEY: &str = "hwmon_device";
pub const FAN_ROW_HWMON_PATH_KEY: &str = "hwmon_path";
pub const FAN_ROW_INPUT_PATH_KEY: &str = "input_path";
pub const FAN_ROW_RAW_LABEL_KEY: &str = "raw_label";
pub const FAN_ROW_DISPLAY_LABEL_KEY: &str = "display_label";
pub const FAN_ROW_RPM_KEY: &str = "rpm";

pub const FAN_STATE_KEY: &str = "fan_state";
pub const FAN_CAPS_KEY: &str = "fan_caps";
pub const FAN_INFOS_KEY: &str = "fans";
pub const FAN_INFO_ID_KEY: &str = "id";
pub const FAN_INFO_INDEX_KEY: &str = "index";
pub const FAN_INFO_LABEL_KEY: &str = "label";
pub const FAN_INFO_MAPPING_CONFIDENCE_KEY: &str = "mapping_confidence";
pub const FAN_INFO_CURRENT_RPM_KEY: &str = "current_rpm";
pub const FAN_INFO_MIN_RPM_KEY: &str = "min_rpm";
pub const FAN_INFO_MAX_RPM_KEY: &str = "max_rpm";
pub const FAN_INFO_CURRENT_PERCENT_KEY: &str = "current_percent";
pub const FAN_INFO_RPM_READABLE_KEY: &str = "rpm_readable";
pub const FAN_INFO_PWM_ENDPOINT_VERIFIED_KEY: &str = "pwm_endpoint_verified";
pub const FAN_INFO_DIRECT_WRITE_KEY: &str = "direct_write";
pub const FAN_INFO_PRIVILEGED_WRITE_KEY: &str = "privileged_write";
pub const FAN_INFO_AUTHORIZATION_KEY: &str = "authorization";
pub const FAN_INFO_ACCESS_STATE_KEY: &str = "access_state";
pub const FAN_INFO_CONTROLLABLE_KEY: &str = "controllable";
pub const FAN_INFO_SUPPORTS_MANUAL_PERCENT_KEY: &str = "supports_manual_percent";
pub const FAN_INFO_SUPPORTS_MANUAL_RPM_TARGET_KEY: &str = "supports_manual_rpm_target";
pub const FAN_INFO_SUPPORTS_CURVE_KEY: &str = "supports_curve";
pub const FAN_INFO_SUPPORTS_AUTO_KEY: &str = "supports_auto";
pub const FAN_INFO_BACKEND_KEY: &str = "backend";
pub const FAN_INFO_ENDPOINTS_KEY: &str = "endpoints";
pub const FAN_INFO_NOTES_KEY: &str = "notes";
pub const FAN_INFO_WARNINGS_KEY: &str = "warnings";

pub const CPU_CONTROL_ACCESS_KEY: &str = "control_access";
pub const CPU_CONTROL_KIND_KEY: &str = "kind";
pub const CPU_CONTROL_STATUS_KEY: &str = "status";
pub const CPU_CONTROL_REASON_KEY: &str = "reason";
pub const CPU_CONTROL_DIRECT_WRITE_KEY: &str = "direct_write";
pub const CPU_CONTROL_PRIVILEGED_WRITE_KEY: &str = "privileged_write";
pub const CPU_CONTROL_AUTHORIZATION_KEY: &str = "authorization";
pub const CPU_CONTROL_PATHS_KEY: &str = "paths";
pub const CPU_PATH_PATH_KEY: &str = "path";
pub const CPU_PATH_READABLE_KEY: &str = "readable";
pub const CPU_PATH_WRITABLE_KEY: &str = "writable";

pub const SETUP_DEPENDENCIES_KEY: &str = "dependencies";
pub const SETUP_PERMISSIONS_KEY: &str = "permissions";
pub const SETUP_ISSUES_KEY: &str = "issues";
pub const SETUP_KIND_KEY: &str = "kind";
pub const SETUP_STATE_KEY: &str = "state";
pub const SETUP_SUMMARY_KEY: &str = "summary";
pub const SETUP_REQUIRED_FOR_KEY: &str = "required_for";
pub const SETUP_EVIDENCE_KEY: &str = "evidence";
pub const SETUP_PATHS_KEY: &str = "paths";
pub const SETUP_SEVERITY_KEY: &str = "severity";
pub const SETUP_TITLE_KEY: &str = "title";
pub const SETUP_GUIDANCE_KEY: &str = "guidance";

pub const CPU_TELEMETRY_PER_CORE_KEY: &str = "per_core";
pub const CPU_LOGICAL_CPU_ID_KEY: &str = "logical_cpu_id";
pub const CPU_CORE_ID_COMPAT_KEY: &str = "core_id";
pub const CPU_PHYSICAL_CORE_INDEX_KEY: &str = "physical_core_index";
pub const CPU_POLICY_ID_KEY: &str = "policy_id";
pub const CPU_THREAD_INDEX_KEY: &str = "thread_index";
pub const CPU_THREAD_COUNT_KEY: &str = "thread_count";

pub fn feature_access_status_key(prefix: &str) -> String {
    format!("{prefix}{FEATURE_ACCESS_STATUS_SUFFIX}")
}

pub fn feature_access_reason_key(prefix: &str) -> String {
    format!("{prefix}{FEATURE_ACCESS_REASON_SUFFIX}")
}
pub mod state {
    use super::{FAN_CAPS_KEY, FAN_STATE_KEY};

    pub const CAPS: &str = "caps";
    pub const TELEMETRY: &str = "telemetry";
    pub const CPU_CAPS: &str = "cpu_caps";
    pub const CPU: &str = "cpu";
    pub const LIGHTING: &str = "lighting";
    pub const LIGHTING_DIAGNOSTICS_SUMMARY: &str = "lighting_diagnostics_summary";
    pub const LIGHTING_DIAGNOSTICS_DETAILS: &str = "lighting_diagnostics_details";
    pub const PROFILE: &str = "profile";
    pub const GPU_MODE: &str = "gpu_mode";
    pub const BATTERY_LIMIT: &str = "battery_limit";
    pub const WARNINGS: &str = "warnings";
    pub const FAN_STATE: &str = FAN_STATE_KEY;
    pub const FAN_CAPS: &str = FAN_CAPS_KEY;
}

pub mod caps {
    pub const HAS_PROFILES: &str = "has_profiles";
    pub const HAS_FAN_CURVES: &str = "has_fan_curves";
    pub const HAS_FAN_READING: &str = "has_fan_reading";
    pub const HAS_CHARGE_LIMIT: &str = "has_charge_limit";
    pub const HAS_GPU_MODES: &str = "has_gpu_modes";
    pub const HAS_AURA: &str = "has_aura";
    pub const HAS_KBD_BACKLIGHT: &str = "has_kbd_backlight";
    pub const HAS_FAN_MANUAL_PERCENT: &str = "has_fan_manual_percent";
    pub const HAS_FAN_MANUAL_RPM_TARGET: &str = "has_fan_manual_rpm_target";
    pub const HAS_INDIVIDUAL_FAN_CONTROL: &str = "has_individual_fan_control";
    pub const HAS_FAN_SYNC_CONTROL: &str = "has_fan_sync_control";
    pub const HAS_FAN_BOOST: &str = "has_fan_boost";
    pub const FAN_COUNT: &str = "fan_count";
    pub const FAN_BACKEND: &str = "fan_backend";
    pub const GPU_BACKEND: &str = "gpu_backend";
    pub const BATTERY_LIMIT_BACKEND: &str = "battery_limit_backend";
    pub const BATTERY_LIMIT_DIRECT_WRITE: &str = "battery_limit_direct_write";
    pub const BATTERY_LIMIT_PRIVILEGED_WRITE: &str = "battery_limit_privileged_write";
    pub const BATTERY_LIMIT_AUTHORIZATION: &str = "battery_limit_authorization";
    pub const CONTROL_PRIVILEGE_MATRIX: &str = "control_privilege_matrix";
    pub const GPU_SUPPORTED_MODES: &str = "gpu_supported_modes";
    pub const GPU_EXTERNAL_AUTHORIZATION: &str = "gpu_external_authorization";
    pub const GPU_SWITCH_STATE: &str = "gpu_switch_state";
    pub const GPU_SWITCH_HINT: &str = "gpu_switch_hint";
    pub const REQUIRES_LOGOUT_FOR_GPU_SWITCH: &str = "requires_logout_for_gpu_switch";
    pub const REQUIRES_REBOOT_FOR_GPU_SWITCH: &str = "requires_reboot_for_gpu_switch";
    pub const ENDPOINTS: &str = "endpoints";
    pub const NOTES: &str = "notes";
}

pub mod lighting {
    pub const BACKEND: &str = "backend";
    pub const DEVICE: &str = "device";
    pub const BRIGHTNESS: &str = "brightness";
    pub const MAX_BRIGHTNESS: &str = "max_brightness";
    pub const CAN_SET: &str = "can_set";
    pub const WRITABLE: &str = "writable";
    pub const DIRECT_WRITABLE: &str = "direct_writable";
    pub const PRIVILEGED_WRITABLE: &str = "privileged_writable";
    pub const AUTHORIZATION_REQUIRED: &str = "authorization_required";
    pub const AUTHORIZATION: &str = "authorization";
    pub const MODE: &str = "mode";
    pub const SUPPORTED_MODES: &str = "supported_modes";
    pub const SUPPORTS_BRIGHTNESS: &str = "supports_brightness";
    pub const SUPPORTS_MODES: &str = "supports_modes";
    pub const SUPPORTS_RGB: &str = "supports_rgb";
    pub const RGB_HEX: &str = "rgb_hex";
    pub const SUPPORTS_SPEED: &str = "supports_speed";
    pub const SUPPORTED_SPEEDS: &str = "supported_speeds";
    pub const SUPPORTED_ZONES: &str = "supported_zones";
    pub const STATUS: &str = "status";
    pub const LAST_ERROR: &str = "last_error";
    pub const DIAGNOSTICS_SUMMARY: &str = "diagnostics_summary";
    pub const DIAGNOSTICS_DETAILS: &str = "diagnostics_details";
    pub const FALLBACK_REASON: &str = "fallback_reason";
    pub const UNAVAILABLE_REASON: &str = "unavailable_reason";
    pub const PERMISSION_WARNING: &str = "permission_warning";
}

pub mod telemetry {
    pub const TIMESTAMP_MS: &str = "timestamp_ms";
    pub const CPU_TEMP_C: &str = "cpu_temp_c";
    pub const GPU_TEMP_C: &str = "gpu_temp_c";
    pub const GPU_USAGE_PERCENT: &str = "gpu_usage_percent";
    pub const GPU_VRAM_USED_BYTES: &str = "gpu_vram_used_bytes";
    pub const GPU_VRAM_TOTAL_BYTES: &str = "gpu_vram_total_bytes";
    pub const GPU_CORE_CLOCK_MHZ: &str = "gpu_core_clock_mhz";
    pub const GPU_MEMORY_CLOCK_MHZ: &str = "gpu_memory_clock_mhz";
    pub const GPU_POWER_W: &str = "gpu_power_w";
    pub const GPU_INDEX: &str = "gpu_index";
    pub const GPU_NAME: &str = "gpu_name";
    pub const GPU_UUID: &str = "gpu_uuid";
    pub const GPU_PCI_BUS_ID: &str = "gpu_pci_bus_id";
    pub const GPU_TELEMETRY_PROVIDER: &str = "gpu_telemetry_provider";
    pub const GPU_TELEMETRY_STATUS: &str = "gpu_telemetry_status";
    pub const TEMPS_C: &str = "temps_c";
    pub const FANS_RPM: &str = "fans_rpm";
    pub const POWER_SOURCE: &str = "power_source";
    pub const BATTERY_PERCENT: &str = "battery_percent";
    pub const AC_ONLINE: &str = "ac_online";
    pub const BATTERY_STATE: &str = "battery_state";
    pub const BATTERY_HEALTH_PERCENT: &str = "battery_health_percent";
    pub const BATTERY_CYCLE_COUNT: &str = "battery_cycle_count";
    pub const BATTERY_CHARGE_POWER_W: &str = "battery_charge_power_w";
    pub const BATTERY_DISCHARGE_POWER_W: &str = "battery_discharge_power_w";
    pub const BATTERY_TIME_TO_EMPTY_S: &str = "battery_time_to_empty_s";
    pub const BATTERY_TIME_TO_FULL_S: &str = "battery_time_to_full_s";
    pub const MEM_TOTAL_BYTES: &str = "mem_total_bytes";
    pub const MEM_USED_BYTES: &str = "mem_used_bytes";
    pub const MEM_USED_PERCENT: &str = "mem_used_percent";
    pub const MEM_AVAILABLE_BYTES: &str = "mem_available_bytes";
    pub const MEM_FREE_BYTES: &str = "mem_free_bytes";
    pub const MEM_CACHED_BYTES: &str = "mem_cached_bytes";
    pub const MEM_BUFFERS_BYTES: &str = "mem_buffers_bytes";
    pub const MEM_SHARED_BYTES: &str = "mem_shared_bytes";
    pub const MEM_ANON_BYTES: &str = "mem_anon_bytes";
    pub const SWAP_TOTAL_BYTES: &str = "swap_total_bytes";
    pub const SWAP_USED_BYTES: &str = "swap_used_bytes";
    pub const SWAP_FREE_BYTES: &str = "swap_free_bytes";
    pub const SWAP_IN_PAGES_PER_S: &str = "swap_in_pages_per_s";
    pub const SWAP_OUT_PAGES_PER_S: &str = "swap_out_pages_per_s";
    pub const ZRAM_TOTAL_BYTES: &str = "zram_total_bytes";
    pub const ZRAM_USED_BYTES: &str = "zram_used_bytes";
    pub const ZSWAP_ENABLED: &str = "zswap_enabled";
    pub const PSI_MEM_SOME_AVG10: &str = "psi_mem_some_avg10";
    pub const PSI_MEM_SOME_AVG60: &str = "psi_mem_some_avg60";
    pub const PSI_MEM_SOME_AVG300: &str = "psi_mem_some_avg300";
    pub const PSI_MEM_FULL_AVG10: &str = "psi_mem_full_avg10";
    pub const PSI_MEM_FULL_AVG60: &str = "psi_mem_full_avg60";
    pub const PSI_MEM_FULL_AVG300: &str = "psi_mem_full_avg300";
    pub const MEM_ACTIVE_BYTES: &str = "mem_active_bytes";
    pub const MEM_INACTIVE_BYTES: &str = "mem_inactive_bytes";
    pub const MEM_DIRTY_BYTES: &str = "mem_dirty_bytes";
    pub const MEM_WRITEBACK_BYTES: &str = "mem_writeback_bytes";
    pub const MEM_SLAB_BYTES: &str = "mem_slab_bytes";
    pub const MEM_SRECLAIMABLE_BYTES: &str = "mem_sreclaimable_bytes";
    pub const MEM_SUNRECLAIM_BYTES: &str = "mem_sunreclaim_bytes";
    pub const MEM_PAGETABLES_BYTES: &str = "mem_pagetables_bytes";
    pub const MEM_KERNELSTACK_BYTES: &str = "mem_kernelstack_bytes";
    pub const MEM_MAPPED_BYTES: &str = "mem_mapped_bytes";
    pub const MEM_TOP_PROCESSES: &str = "mem_top_processes";
    pub const MEM_TOP_PROCESS_ROWS: &str = "mem_top_processes_rows";
    pub const TOP_PROCESS_USER: &str = "user";
    pub const TOP_PROCESS_PID: &str = "pid";
    pub const TOP_PROCESS_RSS_BYTES: &str = "rss_bytes";
    pub const TOP_PROCESS_SWAP_BYTES: &str = "swap_bytes";
    pub const TOP_PROCESS_NAME: &str = "name";
}

pub mod cpu {
    pub const TIMESTAMP_MS: &str = "timestamp_ms";
    pub const TEMP_C: &str = "temp_c";
    pub const USAGE_PERCENT: &str = "usage_percent";
    pub const AVG_FREQ_MHZ: &str = "avg_freq_mhz";
    pub const PACKAGE_POWER_W: &str = "package_power_w";
    pub const STATUS: &str = "status";
    pub const TURBO_BOOST_ENABLED: &str = "turbo_boost_enabled";
    pub const GOVERNOR: &str = "governor";
    pub const EPP: &str = "epp";
    pub const MIN_FREQ_MHZ: &str = "min_freq_mhz";
    pub const MAX_FREQ_MHZ: &str = "max_freq_mhz";
    pub const CURRENT_FREQ_MHZ: &str = "current_freq_mhz";
    pub const ONLINE: &str = "online";
}

pub mod cpu_caps {
    pub const HAS_CPUFREQ: &str = "has_cpufreq";
    pub const HAS_EPP: &str = "has_epp";
    pub const HAS_BOOST_TOGGLE: &str = "has_boost_toggle";
    pub const HAS_PACKAGE_POWER: &str = "has_package_power";
    pub const POLICY_WRITABLE: &str = "policy_writable";
    pub const HAS_MIN_FREQ_LIMIT: &str = "has_min_freq_limit";
    pub const HAS_MAX_FREQ_LIMIT: &str = "has_max_freq_limit";
    pub const HAS_GOVERNOR: &str = "has_governor";
    pub const HAS_CORE_ONLINE: &str = "has_core_online";
    pub const SCALING_DRIVER: &str = "scaling_driver";
    pub const CPU_COUNT: &str = "cpu_count";
    pub const THREAD_COUNT: &str = "thread_count";
    pub const GOVERNOR_CHOICES: &str = "governor_choices";
    pub const EPP_CHOICES: &str = "epp_choices";
    pub const SYSFS_PATHS: &str = "sysfs_paths";
    pub const CONTROL_ACCESS: &str = "control_access";
}

pub mod fan_caps {
    pub const FAN_CURVE_READABLE: &str = "fan_curve_readable";
    pub const FAN_CURVE_WRITABLE: &str = "fan_curve_writable";
    pub const FAN_MAPPING_CONFIDENCE: &str = "fan_mapping_confidence";
}

pub mod fan_state {
    pub const MODE: &str = "mode";
    pub const SYNC_ENABLED: &str = "sync_enabled";
    pub const LAST_ACTION: &str = "last_action";
    pub const ACTIVE_BOOST_FAN_ID: &str = "active_boost_fan_id";
    pub const ACTIVE_BOOST_UNTIL_MS: &str = "active_boost_until_ms";
    pub const ACTIVE_CURVE_SUMMARY: &str = "active_curve_summary";
    pub const WARNINGS: &str = "warnings";
}

pub mod setup {
    pub const CHECKED_AT_MS: &str = "checked_at_ms";
}

pub mod privileged_status {
    pub const SYSTEM_BUS_CONNECTED: &str = "system_bus_connected";
    pub const HELPER_INSTALLED: &str = "privileged_helper_installed";
    pub const HELPER_REACHABLE: &str = "privileged_helper_reachable";
    pub const HELPER_COMPATIBLE: &str = "privileged_helper_compatible";
    pub const HELPER_VERSION: &str = "privileged_helper_version";
    pub const POLKIT_AVAILABLE: &str = "polkit_available";
    pub const AUTHORIZATION_BACKEND: &str = "authorization_backend";
    pub const AUTHORIZATION_STATE: &str = "authorization_state";
    pub const CATEGORIES_AVAILABLE: &str = "privileged_categories_available";
}

pub mod fan_curves {
    pub const SUPPORTED: &str = "supported";
    pub const REASON: &str = "reason";
    pub const POINTS: &str = "points";
    pub const TEMP_C: &str = "temp_c";
    pub const SPEED_PERCENT: &str = "speed_percent";
    pub const DUTY_PERCENT_COMPAT: &str = "duty_percent";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_wire_names_remain_stable() {
        assert_eq!(state::TELEMETRY, "telemetry");
        assert_eq!(caps::HAS_PROFILES, "has_profiles");
        assert_eq!(telemetry::GPU_USAGE_PERCENT, "gpu_usage_percent");
        assert_eq!(telemetry::MEM_TOP_PROCESS_ROWS, "mem_top_processes_rows");
        assert_eq!(cpu_caps::CONTROL_ACCESS, CPU_CONTROL_ACCESS_KEY);
        assert_eq!(lighting::RGB_HEX, "rgb_hex");
        assert_eq!(fan_curves::DUTY_PERCENT_COMPAT, "duty_percent");
        assert_eq!(
            privileged_status::HELPER_INSTALLED,
            "privileged_helper_installed"
        );
    }
}
