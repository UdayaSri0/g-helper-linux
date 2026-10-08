use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 3;
pub const PROFILE_SCHEMA_VERSION: u32 = 1;
pub const CONFIG_DIR_NAME: &str = "rog-helper";
pub const CONFIG_FILE_NAME: &str = "config.toml";
pub const LEGACY_UI_FILE_NAME: &str = "ui.toml";

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseBehavior {
    MinimizeToTray,
    ExitApplication,
}

impl CloseBehavior {
    pub fn from_id(value: &str) -> Option<Self> {
        match value {
            "minimize_to_tray" => Some(Self::MinimizeToTray),
            "exit_application" => Some(Self::ExitApplication),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::MinimizeToTray => "minimize_to_tray",
            Self::ExitApplication => "exit_application",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPreferences {
    pub close_behavior: CloseBehavior,
    pub launch_on_login: bool,
    pub start_minimized_to_tray: bool,
    pub close_to_tray_hint_shown: bool,
    pub fan_warning_acknowledged: bool,
    #[serde(flatten)]
    pub future_fields: BTreeMap<String, toml::Value>,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            close_behavior: CloseBehavior::MinimizeToTray,
            launch_on_login: false,
            start_minimized_to_tray: false,
            close_to_tray_hint_shown: false,
            fan_warning_acknowledged: false,
            future_fields: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DashboardPreferences {
    pub show_system_health: bool,
    pub show_nvme: bool,
    pub show_cooling_snapshot: bool,
    pub compact: bool,
    #[serde(flatten)]
    pub future_fields: BTreeMap<String, toml::Value>,
}

impl Default for DashboardPreferences {
    fn default() -> Self {
        Self {
            show_system_health: true,
            show_nvme: true,
            show_cooling_snapshot: true,
            compact: false,
            future_fields: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_charge_limit: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_manual_profile: Option<String>,
    pub fan_sync_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_profile_id: Option<String>,
    #[serde(flatten)]
    pub future_fields: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AutomationPreferences {
    pub enabled: bool,
    pub ac_profile_id: Option<String>,
    pub battery_profile_id: Option<String>,
    pub battery_threshold_percent: Option<u8>,
    pub manual_override: bool,
    #[serde(flatten)]
    pub future_fields: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileFanRole {
    Cpu,
    Gpu,
    Mid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanCurveProvenance {
    UserDefined,
    Imported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "curve", rename_all = "snake_case")]
pub enum ProfileFanControl {
    Auto,
    Curve(SavedFanCurve),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedFanCurve {
    pub schema_version: u32,
    pub role: ProfileFanRole,
    pub points: Vec<crate::FanPoint>,
    pub provenance: FanCurveProvenance,
    #[serde(flatten)]
    pub future_fields: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProfileLighting {
    /// Kept as a semantic backend label so a future effect survives on older hardware.
    pub effect: String,
    pub primary_rgb: Option<crate::RgbColor>,
    pub secondary_rgb: Option<crate::RgbColor>,
    pub speed: Option<String>,
    pub direction: Option<String>,
    #[serde(flatten)]
    pub future_fields: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProfileSettings {
    pub platform_profile: Option<crate::PerformanceProfile>,
    pub battery_charge_limit: Option<u8>,
    pub gpu_mode: Option<crate::GpuMode>,
    pub fan_controls: BTreeMap<ProfileFanRole, ProfileFanControl>,
    pub lighting: Option<ProfileLighting>,
    #[serde(flatten)]
    pub future_fields: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedProfile {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub settings: ProfileSettings,
    #[serde(flatten)]
    pub future_fields: BTreeMap<String, toml::Value>,
}

impl NamedProfile {
    pub fn empty(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION,
            id: id.into(),
            name: name.into(),
            settings: ProfileSettings::default(),
            future_fields: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub version: u32,
    pub ui: UiPreferences,
    pub dashboard: DashboardPreferences,
    pub controls: ControlPreferences,
    pub automation: AutomationPreferences,
    pub profiles: Vec<NamedProfile>,
    #[serde(flatten)]
    pub future_fields: BTreeMap<String, toml::Value>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            ui: UiPreferences::default(),
            dashboard: DashboardPreferences::default(),
            controls: ControlPreferences::default(),
            automation: AutomationPreferences::default(),
            profiles: Vec::new(),
            future_fields: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigSource {
    Defaults,
    File,
    LegacyUi,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigLoad {
    pub config: AppConfig,
    pub source: ConfigSource,
    pub warnings: Vec<String>,
}

impl ConfigLoad {
    fn defaults(warning: Option<String>) -> Self {
        Self {
            config: AppConfig::default(),
            source: ConfigSource::Defaults,
            warnings: warning.into_iter().collect(),
        }
    }
}

pub fn config_path() -> Result<PathBuf, String> {
    Ok(xdg_config_home()?
        .join(CONFIG_DIR_NAME)
        .join(CONFIG_FILE_NAME))
}

pub fn legacy_ui_config_path() -> Result<PathBuf, String> {
    Ok(xdg_config_home()?
        .join(CONFIG_DIR_NAME)
        .join(LEGACY_UI_FILE_NAME))
}

pub fn xdg_config_home() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME") {
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config"))
        .ok_or_else(|| "HOME is not set, so configuration cannot be located.".to_string())
}

pub fn load_config(path: &Path) -> ConfigLoad {
    match fs::read_to_string(path) {
        Ok(contents) => parse_config(&contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => ConfigLoad::defaults(None),
        Err(error) => ConfigLoad::defaults(Some(format!(
            "Unable to read configuration {}: {error}",
            path.display()
        ))),
    }
}

pub fn load_or_migrate(path: &Path, legacy_path: &Path) -> ConfigLoad {
    if path.exists() {
        return load_config(path);
    }
    let contents = match fs::read_to_string(legacy_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return ConfigLoad::defaults(None),
        Err(error) => {
            return ConfigLoad::defaults(Some(format!(
                "Unable to read legacy UI configuration {}: {error}",
                legacy_path.display()
            )))
        }
    };

    let mut loaded = parse_legacy_ui_config(&contents);
    loaded.source = ConfigSource::LegacyUi;
    match save_config_atomic(path, &loaded.config) {
        Ok(()) => loaded.warnings.push(format!(
            "Migrated legacy UI configuration from {} to {}.",
            legacy_path.display(),
            path.display()
        )),
        Err(error) => loaded.warnings.push(format!(
            "Loaded legacy UI configuration, but could not save the migration: {error}"
        )),
    }
    loaded
}

pub fn parse_config(contents: &str) -> ConfigLoad {
    let value = match toml::from_str::<toml::Value>(contents) {
        Ok(value) => value,
        Err(error) => {
            return ConfigLoad::defaults(Some(format!(
                "Configuration is malformed; defaults are active: {error}"
            )))
        }
    };
    let Some(root) = value.as_table() else {
        return ConfigLoad::defaults(Some(
            "Configuration root must be a TOML table; defaults are active.".to_string(),
        ));
    };

    let mut config = AppConfig::default();
    let mut warnings = Vec::new();
    match root.get("version").and_then(toml::Value::as_integer) {
        Some(version) if version >= 1 => {
            let version = u32::try_from(version).unwrap_or(u32::MAX);
            config.version = version.max(CONFIG_VERSION);
            if version > CONFIG_VERSION {
                warnings.push(format!(
                    "Configuration version {version} is newer than supported version {CONFIG_VERSION}; known fields were loaded and unknown fields are preserved."
                ));
            } else if version < CONFIG_VERSION {
                warnings.push(format!(
                    "Migrated configuration version {version} to version {CONFIG_VERSION}; known settings were retained."
                ));
            }
        }
        Some(_) => warnings.push(
            "Invalid configuration version; migrating known fields to the current version.".into(),
        ),
        None => warnings.push(
            "Configuration has no version; migrating known fields to the current version.".into(),
        ),
    }

    if let Some(ui) = table(root, "ui", &mut warnings) {
        if let Some(value) = string(ui, "close_behavior", &mut warnings) {
            if let Some(behavior) = CloseBehavior::from_id(value) {
                config.ui.close_behavior = behavior;
            } else {
                warnings.push(format!(
                    "Invalid ui.close_behavior '{value}'; using the default."
                ));
            }
        }
        bool_field(
            ui,
            "launch_on_login",
            &mut config.ui.launch_on_login,
            &mut warnings,
        );
        bool_field(
            ui,
            "start_minimized_to_tray",
            &mut config.ui.start_minimized_to_tray,
            &mut warnings,
        );
        bool_field(
            ui,
            "close_to_tray_hint_shown",
            &mut config.ui.close_to_tray_hint_shown,
            &mut warnings,
        );
        bool_field(
            ui,
            "fan_warning_acknowledged",
            &mut config.ui.fan_warning_acknowledged,
            &mut warnings,
        );
        preserve_unknown(
            ui,
            &[
                "close_behavior",
                "launch_on_login",
                "start_minimized_to_tray",
                "close_to_tray_hint_shown",
                "fan_warning_acknowledged",
            ],
            &mut config.ui.future_fields,
        );
    }

    if let Some(dashboard) = table(root, "dashboard", &mut warnings) {
        bool_field(
            dashboard,
            "show_system_health",
            &mut config.dashboard.show_system_health,
            &mut warnings,
        );
        bool_field(
            dashboard,
            "show_nvme",
            &mut config.dashboard.show_nvme,
            &mut warnings,
        );
        bool_field(
            dashboard,
            "show_cooling_snapshot",
            &mut config.dashboard.show_cooling_snapshot,
            &mut warnings,
        );
        bool_field(
            dashboard,
            "compact",
            &mut config.dashboard.compact,
            &mut warnings,
        );
        preserve_unknown(
            dashboard,
            &[
                "show_system_health",
                "show_nvme",
                "show_cooling_snapshot",
                "compact",
            ],
            &mut config.dashboard.future_fields,
        );
    }

    if let Some(controls) = table(root, "controls", &mut warnings) {
        if let Some(raw) = controls.get("preferred_charge_limit") {
            match raw.as_integer().and_then(|value| u8::try_from(value).ok()) {
                Some(value @ 40..=100) => config.controls.preferred_charge_limit = Some(value),
                _ => warnings.push("Invalid controls.preferred_charge_limit; expected 40..=100, so no preference is active.".into()),
            }
        }
        if let Some(value) = string(controls, "last_manual_profile", &mut warnings) {
            let value = value.trim();
            if !value.is_empty() && value.len() <= 64 {
                config.controls.last_manual_profile = Some(value.to_string());
            } else {
                warnings.push(
                    "Invalid controls.last_manual_profile; no profile preference is active.".into(),
                );
            }
        }
        bool_field(
            controls,
            "fan_sync_enabled",
            &mut config.controls.fan_sync_enabled,
            &mut warnings,
        );
        if let Some(value) = string(controls, "preferred_profile_id", &mut warnings) {
            if valid_profile_id(value) {
                config.controls.preferred_profile_id = Some(value.to_string());
            } else {
                warnings.push(
                    "Invalid controls.preferred_profile_id; no preferred profile is active.".into(),
                );
            }
        }
        preserve_unknown(
            controls,
            &[
                "preferred_charge_limit",
                "last_manual_profile",
                "fan_sync_enabled",
                "preferred_profile_id",
            ],
            &mut config.controls.future_fields,
        );
    }

    if let Some(automation) = table(root, "automation", &mut warnings) {
        match toml::Value::Table(automation.clone()).try_into::<AutomationPreferences>() {
            Ok(preferences) => config.automation = preferences,
            Err(error) => warnings.push(format!(
                "Invalid automation configuration; automation remains disabled: {error}"
            )),
        }
    }

    if let Some(raw_profiles) = root.get("profiles") {
        if let Some(profiles) = raw_profiles.as_array() {
            for (index, raw_profile) in profiles.iter().enumerate() {
                match raw_profile.clone().try_into::<NamedProfile>() {
                    Ok(profile) => {
                        if let Err(error) = validate_profile(&profile) {
                            warnings.push(format!("Ignoring invalid profile at index {index}: {error}"));
                        } else if config.profiles.iter().any(|existing| existing.id == profile.id || existing.name.eq_ignore_ascii_case(profile.name.trim())) {
                            warnings.push(format!("Ignoring profile at index {index} because its ID or name is duplicated."));
                        } else {
                            config.profiles.push(profile);
                        }
                    }
                    Err(error) => warnings.push(format!("Ignoring unreadable profile at index {index}; other profiles remain available: {error}")),
                }
            }
        } else {
            warnings.push("Invalid profiles section; saved profiles were not loaded.".into());
        }
    }
    preserve_unknown(
        root,
        &[
            "version",
            "ui",
            "dashboard",
            "controls",
            "automation",
            "profiles",
        ],
        &mut config.future_fields,
    );

    if let Some(preferred_id) = config.controls.preferred_profile_id.as_deref() {
        if !config
            .profiles
            .iter()
            .any(|profile| profile.id == preferred_id)
        {
            config.controls.preferred_profile_id = None;
            warnings.push(
                "Preferred profile ID does not exist; no preferred profile is active.".into(),
            );
        }
    }

    ConfigLoad {
        config,
        source: ConfigSource::File,
        warnings,
    }
}

pub fn parse_legacy_ui_config(contents: &str) -> ConfigLoad {
    let value = match toml::from_str::<toml::Value>(contents) {
        Ok(value) => value,
        Err(error) => {
            return ConfigLoad::defaults(Some(format!(
                "Legacy UI configuration is malformed; defaults are active: {error}"
            )))
        }
    };
    let Some(root) = value.as_table() else {
        return ConfigLoad::defaults(Some(
            "Legacy UI configuration root is invalid; defaults are active.".into(),
        ));
    };
    let mut config = AppConfig::default();
    let mut warnings = Vec::new();
    if let Some(value) = string(root, "close_behavior", &mut warnings) {
        if let Some(behavior) = CloseBehavior::from_id(value) {
            config.ui.close_behavior = behavior;
        }
    }
    bool_field(
        root,
        "launch_on_login",
        &mut config.ui.launch_on_login,
        &mut warnings,
    );
    bool_field(
        root,
        "start_minimized_to_tray",
        &mut config.ui.start_minimized_to_tray,
        &mut warnings,
    );
    bool_field(
        root,
        "close_to_tray_hint_shown",
        &mut config.ui.close_to_tray_hint_shown,
        &mut warnings,
    );
    bool_field(
        root,
        "fan_warning_acknowledged",
        &mut config.ui.fan_warning_acknowledged,
        &mut warnings,
    );
    bool_field(
        root,
        "fan_sync_enabled",
        &mut config.controls.fan_sync_enabled,
        &mut warnings,
    );
    ConfigLoad {
        config,
        source: ConfigSource::LegacyUi,
        warnings,
    }
}

pub fn config_to_toml(config: &AppConfig) -> Result<String, String> {
    let mut normalized = config.clone();
    normalized.version = normalized.version.max(CONFIG_VERSION);
    validate_config(&normalized)?;
    toml::to_string_pretty(&normalized)
        .map_err(|error| format!("Unable to serialize configuration: {error}"))
}

pub fn profile_to_toml(profile: &NamedProfile) -> Result<String, String> {
    validate_profile(profile)?;
    toml::to_string(profile).map_err(|error| format!("Unable to serialize saved profile: {error}"))
}

pub fn profile_settings_to_toml(settings: &ProfileSettings) -> Result<String, String> {
    toml::to_string(settings)
        .map_err(|error| format!("Unable to serialize profile settings: {error}"))
}

fn preserve_unknown(
    source: &toml::map::Map<String, toml::Value>,
    known: &[&str],
    target: &mut BTreeMap<String, toml::Value>,
) {
    for (key, value) in source {
        if !known.contains(&key.as_str()) {
            target.insert(key.clone(), value.clone());
        }
    }
}

pub fn validate_config(config: &AppConfig) -> Result<(), String> {
    if let Some(limit) = config.controls.preferred_charge_limit {
        if !(40..=100).contains(&limit) {
            return Err("Preferred charge limit must be between 40 and 100 percent.".into());
        }
    }
    if let Some(profile) = config.controls.last_manual_profile.as_deref() {
        if profile.trim().is_empty() || profile.len() > 64 {
            return Err("Last manual profile must contain 1 to 64 characters.".into());
        }
    }
    if let Some(id) = config.controls.preferred_profile_id.as_deref() {
        if !valid_profile_id(id) || !config.profiles.iter().any(|profile| profile.id == id) {
            return Err("Preferred profile must refer to an existing profile ID.".into());
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut names = std::collections::BTreeSet::new();
    for profile in &config.profiles {
        validate_profile(profile)?;
        if !ids.insert(profile.id.as_str()) {
            return Err(format!("Profile ID '{}' is duplicated.", profile.id));
        }
        if !names.insert(profile.name.trim().to_ascii_lowercase()) {
            return Err(format!("Profile name '{}' is duplicated.", profile.name));
        }
    }
    if config
        .automation
        .battery_threshold_percent
        .is_some_and(|threshold| !(1..=100).contains(&threshold))
    {
        return Err("Automation battery threshold must be between 1 and 100 percent.".into());
    }
    for id in [
        config.automation.ac_profile_id.as_deref(),
        config.automation.battery_profile_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !valid_profile_id(id) {
            return Err("Automation rule profile IDs must be valid saved profile IDs.".into());
        }
    }
    Ok(())
}

pub fn validate_profile(profile: &NamedProfile) -> Result<(), String> {
    if !valid_profile_id(&profile.id) {
        return Err(
            "Profile ID must contain 1 to 64 ASCII letters, digits, '-' or '_' characters.".into(),
        );
    }
    let name = profile.name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err("Profile name must contain 1 to 64 characters.".into());
    }
    if profile.schema_version == 0 {
        return Err("Profile schema version must be greater than zero.".into());
    }
    if let Some(limit) = profile.settings.battery_charge_limit {
        if !(40..=100).contains(&limit) {
            return Err("Profile battery charge limit must be between 40 and 100 percent.".into());
        }
    }
    if let Some(lighting) = &profile.settings.lighting {
        if lighting.effect.trim().is_empty() || lighting.effect.len() > 64 {
            return Err("Profile lighting effect must contain 1 to 64 characters.".into());
        }
        for (label, value) in [
            ("speed", &lighting.speed),
            ("direction", &lighting.direction),
        ] {
            if value
                .as_ref()
                .is_some_and(|value| value.trim().is_empty() || value.len() > 64)
            {
                return Err(format!(
                    "Profile lighting {label} must contain 1 to 64 characters."
                ));
            }
        }
    }
    for (role, control) in &profile.settings.fan_controls {
        if let ProfileFanControl::Curve(curve) = control {
            if curve.role != *role {
                return Err("Saved fan curve target role does not match its profile entry.".into());
            }
            if curve.schema_version == 0 {
                return Err("Saved fan curve schema version must be greater than zero.".into());
            }
            crate::validate_fan_curve_points(
                &curve.points,
                crate::FanCurvePolicy {
                    exact_point_count: Some(8),
                    ..crate::FanCurvePolicy::default()
                },
            )
            .map_err(|error| format!("Saved fan curve is unsafe: {error}"))?;
        }
    }
    Ok(())
}

pub fn profile_lighting_is_available(
    lighting: &ProfileLighting,
    capabilities: &crate::LightingCaps,
) -> bool {
    let saved = crate::LightingMode::from_backend_label(&lighting.effect);
    capabilities
        .supported_modes
        .iter()
        .any(|supported| supported.same_user_mode(&saved))
}

fn valid_profile_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

pub fn insert_profile(config: &mut AppConfig, profile: NamedProfile) -> Result<(), String> {
    validate_profile(&profile)?;
    if config.profiles.iter().any(|existing| {
        existing.id == profile.id || existing.name.eq_ignore_ascii_case(profile.name.trim())
    }) {
        return Err("A profile with this ID or name already exists.".into());
    }
    config.profiles.push(profile);
    Ok(())
}

pub fn replace_profile(config: &mut AppConfig, profile: NamedProfile) -> Result<(), String> {
    validate_profile(&profile)?;
    let Some(index) = config
        .profiles
        .iter()
        .position(|existing| existing.id == profile.id)
    else {
        return Err("Profile was not found.".into());
    };
    if config.profiles.iter().enumerate().any(|(other, existing)| {
        other != index && existing.name.eq_ignore_ascii_case(profile.name.trim())
    }) {
        return Err("A profile with this name already exists.".into());
    }
    if config.profiles[index].schema_version > PROFILE_SCHEMA_VERSION {
        return Err("This profile was created by a newer schema and cannot be edited here.".into());
    }
    config.profiles[index] = profile;
    Ok(())
}

pub fn remove_profile(config: &mut AppConfig, id: &str) -> Result<(), String> {
    let Some(index) = config.profiles.iter().position(|profile| profile.id == id) else {
        return Err("Profile was not found.".into());
    };
    config.profiles.remove(index);
    if config.controls.preferred_profile_id.as_deref() == Some(id) {
        config.controls.preferred_profile_id = None;
    }
    Ok(())
}

pub fn save_config_atomic(path: &Path, config: &AppConfig) -> Result<(), String> {
    let contents = config_to_toml(config)?;
    let parent = path
        .parent()
        .ok_or_else(|| "Unable to determine the configuration directory.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Unable to create configuration directory: {error}"))?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(CONFIG_FILE_NAME);
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    let result = (|| -> io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|error| format!("Unable to save configuration {}: {error}", path.display()))
}

fn table<'a>(
    root: &'a toml::map::Map<String, toml::Value>,
    key: &str,
    warnings: &mut Vec<String>,
) -> Option<&'a toml::map::Map<String, toml::Value>> {
    match root.get(key) {
        Some(value) => match value.as_table() {
            Some(table) => Some(table),
            None => {
                warnings.push(format!(
                    "Invalid {key} section; defaults are active for it."
                ));
                None
            }
        },
        None => None,
    }
}

fn string<'a>(
    table: &'a toml::map::Map<String, toml::Value>,
    key: &str,
    warnings: &mut Vec<String>,
) -> Option<&'a str> {
    match table.get(key) {
        Some(value) => match value.as_str() {
            Some(value) => Some(value),
            None => {
                warnings.push(format!("Invalid {key}; using its default."));
                None
            }
        },
        None => None,
    }
}

fn bool_field(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
    target: &mut bool,
    warnings: &mut Vec<String>,
) {
    if let Some(value) = table.get(key) {
        if let Some(value) = value.as_bool() {
            *target = value;
        } else {
            warnings.push(format!("Invalid {key}; using its default."));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rog-helper-config-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn defaults_are_safe_and_current() {
        let config = AppConfig::default();
        assert_eq!(config.version, CONFIG_VERSION);
        assert_eq!(config.ui.close_behavior, CloseBehavior::MinimizeToTray);
        assert!(config.dashboard.show_system_health);
        assert_eq!(config.controls.preferred_charge_limit, None);
        assert!(config.profiles.is_empty());
        assert!(!config.automation.enabled);
        assert!(!config.automation.manual_override);
    }

    #[test]
    fn automation_rules_round_trip_and_reject_invalid_thresholds() {
        let config = AppConfig {
            automation: AutomationPreferences {
                enabled: true,
                ac_profile_id: Some("quiet".into()),
                battery_profile_id: Some("mobile".into()),
                battery_threshold_percent: Some(25),
                manual_override: true,
                future_fields: BTreeMap::from([(
                    "future_rule_option".into(),
                    toml::Value::String("keep".into()),
                )]),
            },
            ..AppConfig::default()
        };
        let serialized = config_to_toml(&config).unwrap();
        let loaded = parse_config(&serialized).config;
        assert_eq!(loaded.automation, config.automation);

        let invalid = AppConfig {
            automation: AutomationPreferences {
                battery_threshold_percent: Some(0),
                ..AutomationPreferences::default()
            },
            ..AppConfig::default()
        };
        assert!(validate_config(&invalid).is_err());
    }

    #[test]
    fn serialization_round_trip_preserves_values() {
        let mut expected = AppConfig::default();
        expected.ui.launch_on_login = true;
        expected.dashboard.compact = true;
        expected.controls.preferred_charge_limit = Some(80);
        expected.controls.last_manual_profile = Some("Turbo".into());
        let encoded = config_to_toml(&expected).unwrap();
        let loaded = parse_config(&encoded);
        assert_eq!(loaded.config, expected);
        assert!(loaded.warnings.is_empty());
    }

    #[test]
    fn unknown_fields_are_tolerated() {
        let loaded = parse_config(
            "version = 3\nfuture = 'ok'\n[ui]\nlaunch_on_login = true\nfuture_ui = 9\n",
        );
        assert!(loaded.config.ui.launch_on_login);
        assert!(loaded.warnings.is_empty());
        let serialized = config_to_toml(&loaded.config).unwrap();
        assert!(serialized.contains("future = \"ok\""));
        assert!(serialized.contains("future_ui = 9"));
    }

    #[test]
    fn v1_configuration_migrates_to_current_without_changing_known_settings() {
        let loaded = parse_config("version = 1\n[dashboard]\ncompact = true\n");
        assert_eq!(loaded.config.version, CONFIG_VERSION);
        assert!(loaded.config.dashboard.compact);
        assert!(loaded
            .warnings
            .iter()
            .any(|warning| warning.contains("Migrated configuration version 1")));
    }

    #[test]
    fn v2_configuration_migrates_to_v3_with_automation_disabled() {
        let loaded = parse_config(
            "version = 2\n[controls]\nfan_sync_enabled = true\n[ui]\nlaunch_on_login = true\n",
        );
        assert_eq!(loaded.config.version, 3);
        assert!(loaded.config.controls.fan_sync_enabled);
        assert!(loaded.config.ui.launch_on_login);
        assert!(!loaded.config.automation.enabled);
        assert!(loaded
            .warnings
            .iter()
            .any(|warning| warning.contains("Migrated configuration version 2")));
    }

    #[test]
    fn future_configuration_fields_survive_parse_and_atomic_serialization() {
        let source = "version = 99\nfuture_root = { enabled = true }\n[ui]\nfuture_ui = 'keep'\n[dashboard]\nfuture_dashboard = 17\n[controls]\nfuture_control = 'keep'\n";
        let loaded = parse_config(source);
        let serialized = config_to_toml(&loaded.config).unwrap();
        assert!(serialized.contains("version = 99"));
        assert!(serialized.contains("future_root"));
        assert!(serialized.contains("future_ui = \"keep\""));
        assert!(serialized.contains("future_dashboard = 17"));
        assert!(serialized.contains("future_control = \"keep\""));
        assert_eq!(parse_config(&serialized).config.version, 99);

        let daemon_input = toml::from_str::<AppConfig>(source).unwrap();
        let daemon_round_trip = config_to_toml(&daemon_input).unwrap();
        assert!(daemon_round_trip.contains("future_root"));
        assert!(daemon_round_trip.contains("future_ui = \"keep\""));
        assert!(daemon_round_trip.contains("future_dashboard = 17"));
        assert!(daemon_round_trip.contains("future_control = \"keep\""));
    }

    fn safe_saved_curve(role: ProfileFanRole) -> SavedFanCurve {
        SavedFanCurve {
            schema_version: PROFILE_SCHEMA_VERSION,
            role,
            points: crate::FanCurvePreset::Balanced.points(),
            provenance: FanCurveProvenance::UserDefined,
            future_fields: BTreeMap::new(),
        }
    }

    #[test]
    fn named_profile_round_trip_keeps_semantic_curve_and_unknown_lighting_effect() {
        let mut profile = NamedProfile::empty("gaming-1", "Gaming");
        profile.settings.fan_controls.insert(
            ProfileFanRole::Cpu,
            ProfileFanControl::Curve(safe_saved_curve(ProfileFanRole::Cpu)),
        );
        if let Some(ProfileFanControl::Curve(curve)) =
            profile.settings.fan_controls.get_mut(&ProfileFanRole::Cpu)
        {
            curve.future_fields.insert(
                "future_curve_policy".into(),
                toml::Value::String("keep".into()),
            );
        }
        profile.settings.lighting = Some(ProfileLighting {
            effect: "Future Aurora Flow".into(),
            primary_rgb: Some(crate::RgbColor::parse_hex("#23AABB").unwrap()),
            ..ProfileLighting::default()
        });
        profile
            .settings
            .future_fields
            .insert("future_setting".into(), toml::Value::Boolean(true));
        profile
            .settings
            .lighting
            .as_mut()
            .unwrap()
            .future_fields
            .insert("future_light_field".into(), toml::Value::Integer(4));
        profile
            .future_fields
            .insert("future_profile".into(), toml::Value::Integer(8));
        let mut config = AppConfig::default();
        insert_profile(&mut config, profile.clone()).unwrap();

        let serialized = config_to_toml(&config).unwrap();
        let loaded = parse_config(&serialized);
        assert_eq!(loaded.config.profiles[0], profile);
        assert!(serialized.contains("schema_version = 1"));
        assert!(serialized.contains("future_profile = 8"));
        assert!(serialized.contains("future_curve_policy = \"keep\""));
        assert!(serialized.contains("future_setting = true"));
        assert!(serialized.contains("future_light_field = 4"));
    }

    #[test]
    fn corrupt_profile_isolated_without_discarding_valid_profiles() {
        let good = NamedProfile::empty("good", "Good");
        let mut bad = toml::Value::try_from(NamedProfile::empty("bad", "Bad")).unwrap();
        bad.as_table_mut()
            .unwrap()
            .insert("id".into(), toml::Value::Integer(7));
        let mut root = toml::map::Map::new();
        root.insert("version".into(), toml::Value::Integer(2));
        root.insert(
            "profiles".into(),
            toml::Value::Array(vec![bad, toml::Value::try_from(good).unwrap()]),
        );
        let loaded = parse_config(&toml::to_string(&toml::Value::Table(root)).unwrap());
        assert_eq!(loaded.config.profiles.len(), 1);
        assert_eq!(loaded.config.profiles[0].id, "good");
        assert!(loaded
            .warnings
            .iter()
            .any(|warning| warning.contains("profile at index 0")));
    }

    #[test]
    fn saved_curve_requires_exactly_eight_safe_points() {
        let mut profile = NamedProfile::empty("curve", "Curve");
        let mut curve = safe_saved_curve(ProfileFanRole::Cpu);
        curve.points.pop();
        profile
            .settings
            .fan_controls
            .insert(ProfileFanRole::Cpu, ProfileFanControl::Curve(curve));
        assert!(validate_profile(&profile)
            .unwrap_err()
            .contains("exactly 8"));

        let mut curve = safe_saved_curve(ProfileFanRole::Cpu);
        curve.points[6].duty_percent = 10;
        profile
            .settings
            .fan_controls
            .insert(ProfileFanRole::Cpu, ProfileFanControl::Curve(curve));
        assert!(validate_profile(&profile)
            .unwrap_err()
            .contains("non-decreasing"));

        profile.settings.fan_controls.insert(
            ProfileFanRole::Cpu,
            ProfileFanControl::Curve(safe_saved_curve(ProfileFanRole::Cpu)),
        );
        let mut curve = safe_saved_curve(ProfileFanRole::Gpu);
        curve.role = ProfileFanRole::Cpu;
        profile
            .settings
            .fan_controls
            .insert(ProfileFanRole::Gpu, ProfileFanControl::Curve(curve));
        assert!(validate_profile(&profile)
            .unwrap_err()
            .contains("target role"));
    }

    #[test]
    fn unsupported_lighting_mode_remains_saved_but_is_not_available() {
        let saved = ProfileLighting {
            effect: "Future Aurora Flow".into(),
            ..ProfileLighting::default()
        };
        let caps = crate::LightingCaps {
            supported_modes: vec![crate::LightingMode::Static],
            ..crate::LightingCaps::default()
        };
        assert!(!profile_lighting_is_available(&saved, &caps));
        let mut profile = NamedProfile::empty("light", "Lighting");
        profile.settings.lighting = Some(saved.clone());
        let config = AppConfig {
            profiles: vec![profile],
            ..AppConfig::default()
        };
        assert_eq!(
            parse_config(&config_to_toml(&config).unwrap())
                .config
                .profiles[0]
                .settings
                .lighting,
            Some(saved)
        );
    }

    #[test]
    fn profile_ids_and_names_are_unique_case_insensitively_and_delete_clears_preference() {
        let mut config = AppConfig::default();
        insert_profile(&mut config, NamedProfile::empty("one", "Quiet")).unwrap();
        assert!(insert_profile(&mut config, NamedProfile::empty("one", "Other")).is_err());
        assert!(insert_profile(&mut config, NamedProfile::empty("two", "quiet")).is_err());
        config.controls.preferred_profile_id = Some("one".into());
        replace_profile(&mut config, NamedProfile::empty("one", "Renamed")).unwrap();
        assert_eq!(config.profiles[0].id, "one");
        remove_profile(&mut config, "one").unwrap();
        assert!(config.profiles.is_empty());
        assert_eq!(config.controls.preferred_profile_id, None);
    }

    #[test]
    fn failed_atomic_save_does_not_replace_existing_file_or_leave_temporary_file() {
        let root = test_dir("atomic-failure");
        fs::create_dir_all(&root).unwrap();
        let blocker = root.join("not-a-directory");
        fs::write(&blocker, "preserve me").unwrap();
        let target = blocker.join(CONFIG_FILE_NAME);
        let result = save_config_atomic(&target, &AppConfig::default());
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&blocker).unwrap(), "preserve me");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_individual_values_do_not_discard_valid_ones() {
        let loaded = parse_config(
            "version = 1\n[ui]\nlaunch_on_login = true\nclose_behavior = 'explode'\n[dashboard]\nshow_nvme = 'sometimes'\ncompact = true\n[controls]\npreferred_charge_limit = 12\n",
        );
        assert!(loaded.config.ui.launch_on_login);
        assert_eq!(
            loaded.config.ui.close_behavior,
            CloseBehavior::MinimizeToTray
        );
        assert!(loaded.config.dashboard.show_nvme);
        assert!(loaded.config.dashboard.compact);
        assert_eq!(loaded.config.controls.preferred_charge_limit, None);
        assert!(loaded.warnings.len() >= 3);
    }

    #[test]
    fn malformed_toml_falls_back_without_writing() {
        let loaded = parse_config("[ui\nlaunch_on_login = true");
        assert_eq!(loaded.config, AppConfig::default());
        assert_eq!(loaded.source, ConfigSource::Defaults);
        assert_eq!(loaded.warnings.len(), 1);
    }

    #[test]
    fn future_versions_load_known_fields() {
        let loaded = parse_config("version = 99\n[dashboard]\nshow_nvme = false\n");
        assert!(!loaded.config.dashboard.show_nvme);
        assert!(loaded
            .warnings
            .iter()
            .any(|warning| warning.contains("newer")));
        assert_eq!(loaded.config.version, 99);
    }

    #[test]
    fn legacy_flat_file_migrates_without_applying_controls() {
        let root = test_dir("migration");
        fs::create_dir_all(&root).unwrap();
        let current = root.join(CONFIG_FILE_NAME);
        let legacy = root.join(LEGACY_UI_FILE_NAME);
        fs::write(
            &legacy,
            "close_behavior = 'exit_application'\nlaunch_on_login = true\nfan_sync_enabled = true\n",
        )
        .unwrap();
        let loaded = load_or_migrate(&current, &legacy);
        assert_eq!(loaded.source, ConfigSource::LegacyUi);
        assert_eq!(
            loaded.config.ui.close_behavior,
            CloseBehavior::ExitApplication
        );
        assert!(loaded.config.controls.fan_sync_enabled);
        assert!(current.is_file());
        assert!(legacy.is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_save_replaces_complete_file_and_cleans_temporary_file() {
        let root = test_dir("atomic");
        let path = root.join(CONFIG_FILE_NAME);
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, "old = true\n").unwrap();
        let mut expected = AppConfig::default();
        expected.dashboard.show_nvme = false;
        save_config_atomic(&path, &expected).unwrap();
        assert_eq!(load_config(&path).config, expected);
        let leftovers = fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);
        fs::remove_dir_all(root).unwrap();
    }
}
