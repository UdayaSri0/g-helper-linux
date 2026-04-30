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
pub const FAN_INFO_CURRENT_RPM_KEY: &str = "current_rpm";
pub const FAN_INFO_MIN_RPM_KEY: &str = "min_rpm";
pub const FAN_INFO_MAX_RPM_KEY: &str = "max_rpm";
pub const FAN_INFO_CURRENT_PERCENT_KEY: &str = "current_percent";
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
pub const CPU_CONTROL_PATHS_KEY: &str = "paths";
pub const CPU_PATH_PATH_KEY: &str = "path";
pub const CPU_PATH_READABLE_KEY: &str = "readable";
pub const CPU_PATH_WRITABLE_KEY: &str = "writable";

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
