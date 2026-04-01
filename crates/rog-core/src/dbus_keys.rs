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
