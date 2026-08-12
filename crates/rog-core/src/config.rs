use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPreferences {
    pub close_behavior: CloseBehavior,
    pub launch_on_login: bool,
    pub start_minimized_to_tray: bool,
    pub close_to_tray_hint_shown: bool,
    pub fan_warning_acknowledged: bool,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            close_behavior: CloseBehavior::MinimizeToTray,
            launch_on_login: false,
            start_minimized_to_tray: false,
            close_to_tray_hint_shown: false,
            fan_warning_acknowledged: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DashboardPreferences {
    pub show_system_health: bool,
    pub show_nvme: bool,
    pub show_cooling_snapshot: bool,
    pub compact: bool,
}

impl Default for DashboardPreferences {
    fn default() -> Self {
        Self {
            show_system_health: true,
            show_nvme: true,
            show_cooling_snapshot: true,
            compact: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_charge_limit: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_manual_profile: Option<String>,
    pub fan_sync_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub version: u32,
    pub ui: UiPreferences,
    pub dashboard: DashboardPreferences,
    pub controls: ControlPreferences,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            ui: UiPreferences::default(),
            dashboard: DashboardPreferences::default(),
            controls: ControlPreferences::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    Defaults,
    File,
    LegacyUi,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
            if version > CONFIG_VERSION {
                warnings.push(format!(
                    "Configuration version {version} is newer than supported version {CONFIG_VERSION}; known fields were loaded and unknown fields preserved only on disk."
                ));
            }
        }
        Some(_) => warnings.push("Invalid configuration version; treating it as version 1.".into()),
        None => warnings.push("Configuration has no version; migrated as version 1.".into()),
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
    }

    if let Some(controls) = table(root, "controls", &mut warnings) {
        if let Some(raw) = controls.get("preferred_charge_limit") {
            match raw.as_integer().and_then(|v| u8::try_from(v).ok()) {
                Some(value @ 40..=100) => config.controls.preferred_charge_limit = Some(value),
                _ => warnings.push(
                    "Invalid controls.preferred_charge_limit; expected 40..=100, so no preference is active."
                        .into(),
                ),
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
    normalized.version = CONFIG_VERSION;
    validate_config(&normalized)?;
    toml::to_string_pretty(&normalized)
        .map_err(|error| format!("Unable to serialize configuration: {error}"))
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
            "version = 1\nfuture = 'ok'\n[ui]\nlaunch_on_login = true\nfuture_ui = 9\n",
        );
        assert!(loaded.config.ui.launch_on_login);
        assert!(loaded.warnings.is_empty());
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
        assert_eq!(loaded.config.version, CONFIG_VERSION);
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
