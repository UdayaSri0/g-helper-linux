use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use adw::prelude::*;
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk4 as gtk;
use ksni::menu::{MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{ToolTip, Tray, TrayMethods};
use rog_core::{
    config_path, config_to_toml, dbus_keys, legacy_ui_config_path, load_config, parse_config,
    parse_legacy_ui_config, validate_config, AppConfig, AuthorizationState, BatteryState,
    CloseBehavior, ContractStatus, CpuAccessState, CpuAuthorization, CpuCaps, CpuControlAccess,
    CpuControlKind, CpuCoreTelemetry, CpuPathAccess, CpuTelemetry, DependencyKind, DependencyState,
    DependencyStatus, DeviceCaps, ErrorCategory, FanCaps, FanControlMode, FanInfo,
    FanMappingConfidence, FanState, FanTelemetry, FeatureAccessState, FeatureAvailability,
    GpuSwitchState, PermissionKind, PermissionState, PermissionStatus, PowerSource,
    PrivilegedCategory, PrivilegedStatus, RgbColor, SetupIssue, SetupSeverity, SetupStatus,
    TelemetrySnapshot, TopProcessMem,
};
use serde::Deserialize;
use tracing::{debug, info, warn};
use zbus::zvariant::{OwnedValue, Value};

mod dbus_decode;
mod fan_widgets;
mod shell;
mod theme;
mod widgets;

use fan_widgets::{CurvePreview, FanRotor, GaugeAccent, RotorStatus, TempGauge};
use shell::NavigationItem;
use widgets::{
    page_container, page_header, page_header_group, HistoryGraph, MetricCard, Sparkline,
};

const DAEMON_DBUS_NAME: &str = "io.github.roghelper.Daemon";
const DAEMON_DBUS_PATH: &str = "/io/github/roghelper/Daemon";
const DAEMON_DBUS_IFACE: &str = "io.github.roghelper.Daemon1";
const APP_BINARY_NAME: &str = "rog-helper-ui";
const APP_ICON_NAME: &str = "rog-helper";
const APP_AUTHORS_FALLBACK: &str = "rog-helper contributors";
const APP_REPOSITORY_FALLBACK_URL: &str = "https://github.com/UdayaSri0/g-helper-linux";
const AUTOSTART_DESKTOP_FILE_NAME: &str = "rog-helper.desktop";
const START_MINIMIZED_ARG: &str = "--minimized-to-tray";
const ICON_RESOURCE_PATH: &str = "/io/github/roghelper/icons";
const ICON_DASHBOARD: &str = "rog-dashboard-symbolic";
const ICON_CPU: &str = "rog-cpu-symbolic";
const ICON_GPU: &str = "rog-gpu-symbolic";
const ICON_BATTERY: &str = "rog-battery-symbolic";
const ICON_RAM: &str = "rog-memory-symbolic";
const ICON_STORAGE: &str = "rog-storage-symbolic";
const ICON_POWER: &str = "rog-power-symbolic";
const ICON_LIGHTING: &str = "rog-lighting-symbolic";
const ICON_FANS: &str = "rog-fan-symbolic";
const ICON_DIAGNOSTICS: &str = "rog-diagnostics-symbolic";
const ICON_SETTINGS: &str = "preferences-system-symbolic";
const ICON_ABOUT: &str = "rog-about-symbolic";

#[zbus::proxy(
    interface = "io.github.roghelper.Daemon1",
    default_service = "io.github.roghelper.Daemon",
    default_path = "/io/github/roghelper/Daemon"
)]
trait Daemon1 {
    fn get_configuration(&self) -> zbus::Result<String>;
    fn set_configuration(&self, contents: &str) -> zbus::Result<()>;
    fn reset_configuration(&self) -> zbus::Result<String>;
    fn get_caps(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_state(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_telemetry(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_cpu_caps(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_cpu_telemetry(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_cpu_diagnostics(&self) -> zbus::Result<String>;
    fn get_setup_status(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_privileged_status(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_fan_caps(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_fan_state(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_fan_curves(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn set_fan_auto(&self, fan_id: &str) -> zbus::Result<()>;
    fn set_fan_manual_percent(&self, fan_id: &str, percent: u64) -> zbus::Result<()>;
    fn set_fan_rpm_target(&self, fan_id: &str, rpm: u64) -> zbus::Result<()>;
    fn set_fan_curve(&self, fan_id: &str, curve: HashMap<String, OwnedValue>) -> zbus::Result<()>;
    fn set_fan_sync(&self, enabled: bool) -> zbus::Result<()>;
    fn set_fan_boost(&self, fan_id: &str, percent: u64, duration_seconds: u64) -> zbus::Result<()>;
    fn reset_fans_to_auto(&self) -> zbus::Result<()>;
    fn set_lighting(&self, state: HashMap<String, OwnedValue>) -> zbus::Result<()>;
    fn set_profile(&self, profile: &str) -> zbus::Result<()>;
    fn set_gpu_mode(&self, mode: &str) -> zbus::Result<()>;
    fn set_battery_limit(&self, limit: u64) -> zbus::Result<()>;
    fn set_cpu_turbo(&self, enabled: bool) -> zbus::Result<()>;
    fn set_cpu_power_mode(&self, mode: &str) -> zbus::Result<()>;
    fn set_cpu_governor(&self, governor: &str) -> zbus::Result<()>;
    fn set_cpu_epp(&self, epp: &str) -> zbus::Result<()>;
    fn set_cpu_freq_limits(&self, min_mhz: u64, max_mhz: u64) -> zbus::Result<()>;
    fn set_cpu_core_online(&self, core_id: u64, online: bool) -> zbus::Result<()>;
}

#[derive(Debug, Clone)]
struct AppMetadata {
    binary_name: String,
    version: String,
    license: String,
    authors: String,
    repository_url: Option<String>,
    maintainer_name: String,
    maintainer_url: Option<String>,
    issues_url: Option<String>,
    releases_url: Option<String>,
}

impl AppMetadata {
    fn detect() -> Self {
        let repository_url = detect_repository_url();
        let authors = package_authors();
        let maintainer_name = detect_maintainer_name(&authors, repository_url.as_deref());
        let maintainer_url = repository_url
            .as_deref()
            .and_then(github_repo_from_url)
            .map(|(owner, _)| format!("https://github.com/{owner}"));
        let issues_url = repository_url
            .as_deref()
            .and_then(github_repo_from_url)
            .map(|(owner, repo)| format!("https://github.com/{owner}/{repo}/issues"));
        let releases_url = repository_url
            .as_deref()
            .and_then(github_repo_from_url)
            .map(|(owner, repo)| format!("https://github.com/{owner}/{repo}/releases/latest"));

        Self {
            binary_name: APP_BINARY_NAME.to_string(),
            version: package_value(env!("CARGO_PKG_VERSION"), "unknown"),
            license: package_value(env!("CARGO_PKG_LICENSE"), "License not configured"),
            authors,
            repository_url,
            maintainer_name,
            maintainer_url,
            issues_url,
            releases_url,
        }
    }
}

#[derive(Debug, Clone)]
struct ReleaseAsset {
    name: String,
    download_url: String,
}

#[derive(Debug, Clone)]
struct UpdateState {
    latest_version: Option<String>,
    status_text: String,
    last_result_text: String,
    last_checked_unix: Option<i64>,
    release_notes_preview: Option<String>,
    release_notes_full: Option<String>,
    release_page_url: Option<String>,
    downloadable_asset: Option<ReleaseAsset>,
    update_action_label: String,
    update_action_subtitle: String,
    supports_in_place_update: bool,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self {
            latest_version: None,
            status_text: "No update check has been performed yet.".to_string(),
            last_result_text:
                "Manual checks use the GitHub Releases API when a repository URL is configured."
                    .to_string(),
            last_checked_unix: None,
            release_notes_preview: None,
            release_notes_full: None,
            release_page_url: None,
            downloadable_asset: None,
            update_action_label: "Open Releases".to_string(),
            update_action_subtitle: "Open the latest release page in your browser.".to_string(),
            supports_in_place_update: false,
        }
    }
}

#[derive(Debug, Clone)]
struct OpenLinkRequest {
    url: String,
    error_message: String,
}

#[derive(Debug)]
struct SharedUiState {
    render_revision: u64,
    telemetry: Option<TelemetrySnapshot>,
    cpu: Option<CpuTelemetry>,
    cpu_caps: CpuCaps,
    cpu_usage_history: Vec<f32>,
    cpu_temp_history: Vec<f32>,
    cpu_power_history: Vec<f32>,
    gpu_temp_history: Vec<f32>,
    gpu_usage_history: Vec<f32>,
    memory_usage_history: Vec<f32>,
    caps: DeviceCaps,
    caps_text: String,
    setup_status: SetupStatus,
    privileged_status: Option<PrivilegedStatus>,
    pending_setup_refresh: bool,
    setup_refreshing: bool,
    warnings: Vec<String>,
    profile: Option<String>,
    gpu_mode: Option<String>,
    battery_limit: Option<u8>,
    lighting: Option<LightingInfo>,
    fan_state: FanState,
    pending_profile: Option<String>,
    pending_gpu_mode: Option<String>,
    pending_battery_limit: Option<u8>,
    pending_cpu_actions: Vec<PendingCpuAction>,
    pending_lighting: Option<PendingLighting>,
    pending_fan_action: Option<PendingFanAction>,
    lighting_error: Option<String>,
    action_error: Option<String>,
    update_state: UpdateState,
    pending_update_check: bool,
    update_check_in_progress: bool,
    pending_update_action: bool,
    update_action_in_progress: bool,
    pending_open_link: Option<OpenLinkRequest>,
    pending_toast: Option<(String, bool)>,
    daemon_error: Option<String>,
    settings: AppConfig,
    pending_config_save: Option<AppConfig>,
    pending_config_reset: bool,
    tray_available: Option<bool>,
    show_window: bool,
    show_about: bool,
    show_gpu_page: bool,
    quit: bool,
}

impl Default for SharedUiState {
    fn default() -> Self {
        Self {
            render_revision: 1,
            telemetry: None,
            cpu: None,
            cpu_caps: CpuCaps::unknown(),
            cpu_usage_history: Vec::new(),
            cpu_temp_history: Vec::new(),
            cpu_power_history: Vec::new(),
            gpu_temp_history: Vec::new(),
            gpu_usage_history: Vec::new(),
            memory_usage_history: Vec::new(),
            caps: DeviceCaps::unknown(),
            caps_text: String::new(),
            setup_status: SetupStatus::unknown(),
            privileged_status: None,
            pending_setup_refresh: true,
            setup_refreshing: false,
            warnings: Vec::new(),
            profile: None,
            gpu_mode: None,
            battery_limit: None,
            lighting: None,
            fan_state: FanState::from_fans(Vec::new()),
            pending_profile: None,
            pending_gpu_mode: None,
            pending_battery_limit: None,
            pending_cpu_actions: Vec::new(),
            pending_lighting: None,
            pending_fan_action: None,
            lighting_error: None,
            action_error: None,
            update_state: UpdateState::default(),
            pending_update_check: false,
            update_check_in_progress: false,
            pending_update_action: false,
            update_action_in_progress: false,
            pending_open_link: None,
            pending_toast: None,
            daemon_error: None,
            settings: AppConfig::default(),
            pending_config_save: None,
            pending_config_reset: false,
            tray_available: None,
            show_window: false,
            show_about: false,
            show_gpu_page: false,
            quit: false,
        }
    }
}

impl SharedUiState {
    fn mark_render_dirty(&mut self) {
        self.render_revision = self.render_revision.wrapping_add(1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessUiState {
    Available,
    AvailableWithAdministrator,
    Authorized,
    AuthorizationDenied,
    NotRequired,
    ReadOnly,
    BackendMissing,
    PrivilegedHelperMissing,
    UnsupportedHardware,
    UnsafeMapping,
    SetupRequired,
    ExternalServiceRequired,
    Unavailable,
    Checking,
}

impl AccessUiState {
    const fn label(self) -> &'static str {
        match self {
            Self::Available => "Available",
            Self::AvailableWithAdministrator => "Administrator access required",
            Self::Authorized => "Authorized",
            Self::AuthorizationDenied => "Authorization denied",
            Self::NotRequired => "Not required",
            Self::ReadOnly => "Read only",
            Self::BackendMissing => "Backend missing",
            Self::PrivilegedHelperMissing => "Privileged helper missing",
            Self::UnsupportedHardware => "Unsupported hardware",
            Self::UnsafeMapping => "Unsafe mapping",
            Self::SetupRequired => "Setup required",
            Self::ExternalServiceRequired => "External service required",
            Self::Unavailable => "Unavailable",
            Self::Checking => "Checking",
        }
    }

    const fn css_class(self) -> &'static str {
        match self {
            Self::Available | Self::Authorized | Self::NotRequired => "status-ok",
            Self::AvailableWithAdministrator
            | Self::ReadOnly
            | Self::BackendMissing
            | Self::PrivilegedHelperMissing
            | Self::UnsafeMapping
            | Self::SetupRequired
            | Self::ExternalServiceRequired => "status-warning",
            Self::AuthorizationDenied | Self::Unavailable => "status-error",
            Self::UnsupportedHardware | Self::Checking => "status-info",
        }
    }
}

#[derive(Clone)]
struct SetupAccessRow {
    row: adw::ActionRow,
    value: gtk::Label,
}

#[derive(Clone)]
struct AdministratorAccessRows {
    helper: SetupAccessRow,
    system_bus: SetupAccessRow,
    polkit: SetupAccessRow,
    cpu: SetupAccessRow,
    fans: SetupAccessRow,
    lighting: SetupAccessRow,
    gpu: SetupAccessRow,
    battery: SetupAccessRow,
}

#[derive(Debug, Clone)]
enum PendingCpuAction {
    Turbo(bool),
    PowerMode(String),
    Governor(String),
    Epp(String),
    FreqLimits {
        min_mhz: Option<u32>,
        max_mhz: Option<u32>,
    },
    CoreOnline {
        core_id: u32,
        online: bool,
    },
}

#[derive(Debug, Clone)]
struct LightingInfo {
    backend: String,
    backend_kind: String,
    device: String,
    brightness: u64,
    max_brightness: u64,
    can_set: bool,
    direct_writable: bool,
    privileged_writable: bool,
    authorization_required: bool,
    authorization: String,
    mode: String,
    supports_brightness: bool,
    supports_modes: bool,
    supports_rgb: bool,
    supports_argb: bool,
    supports_zones: bool,
    supports_per_key: bool,
    rgb_hex: Option<String>,
    secondary_rgb_hex: Option<String>,
    supported_modes: Vec<String>,
    supports_speed: bool,
    supported_speeds: Vec<String>,
    speed: Option<String>,
    supported_directions: Vec<String>,
    direction: Option<String>,
    supported_zones: Vec<String>,
    active_zone: Option<String>,
    apply_outcome: Option<String>,
    status: String,
    last_error: Option<String>,
    diagnostics_summary: Option<String>,
    diagnostics_details: Option<String>,
    fallback_reason: Option<String>,
    unavailable_reason: Option<String>,
    permission_warning: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingLighting {
    brightness: Option<u64>,
    mode: Option<String>,
    rgb_hex: Option<String>,
    secondary_rgb_hex: Option<String>,
    speed: Option<String>,
    direction: Option<String>,
    zone: Option<String>,
}

#[derive(Debug, Clone)]
enum PendingFanAction {
    Auto {
        fan_id: String,
    },
    ManualPercent {
        fan_id: String,
        percent: u8,
    },
    Boost {
        fan_id: String,
        duration_seconds: u64,
    },
    Curve {
        fan_id: String,
        points: Vec<(u8, u8)>,
    },
    Sync(bool),
}

#[derive(Clone)]
struct FanVisualSlot {
    root: gtk::Box,
    rotor: FanRotor,
    label: gtk::Label,
    rpm: gtk::Label,
    status: gtk::Label,
}

#[derive(Clone)]
struct FanCardSlot {
    root: gtk::Box,
    rotor: FanRotor,
    title: gtk::Label,
    id: gtk::Label,
    rpm: gtk::Label,
    percent: gtk::Label,
    status: gtk::Label,
    backend: gtk::Label,
    details: gtk::Label,
    warning: gtk::Label,
}

#[derive(Debug, Clone)]
struct DetailRow {
    row: adw::ActionRow,
    value_label: gtk::Label,
}

#[derive(Clone)]
struct DashboardControlTile {
    root: gtk::Box,
    subtitle: gtk::Label,
}

impl DashboardControlTile {
    fn new(title: &str, control: &impl IsA<gtk::Widget>) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.add_css_class("dashboard-control-tile");
        root.set_hexpand(true);

        let title = gtk::Label::new(Some(title));
        title.set_xalign(0.0);
        title.add_css_class("dashboard-control-title");

        let subtitle = gtk::Label::new(Some("Checking support…"));
        subtitle.set_xalign(0.0);
        subtitle.set_wrap(true);
        subtitle.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        subtitle.add_css_class("dashboard-control-subtitle");

        root.append(&title);
        root.append(&subtitle);
        root.append(control);

        Self { root, subtitle }
    }

    fn widget(&self) -> &gtk::Box {
        &self.root
    }

    fn set_subtitle(&self, subtitle: &str) {
        self.subtitle.set_text(subtitle);
    }

    fn set_visible(&self, visible: bool) {
        self.root.set_visible(visible);
    }
}

#[derive(Debug)]
struct RogTray {
    shared: Arc<Mutex<SharedUiState>>,
    summary: String,
}

impl Tray for RogTray {
    fn id(&self) -> String {
        "rog-helper".to_string()
    }

    fn title(&self) -> String {
        "rog-helper".to_string()
    }

    fn icon_name(&self) -> String {
        APP_ICON_NAME.to_string()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "rog-helper".to_string(),
            description: self.summary.clone(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        if let Ok(mut st) = self.shared.lock() {
            st.show_window = true;
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let summary = if self.summary.is_empty() {
            "rog-helper".to_string()
        } else {
            self.summary.clone()
        };

        let (has_profiles, gpu_available, gpu_modes, current_profile, current_gpu_mode, gpu_hint) =
            self.shared
                .lock()
                .map(|st| {
                    (
                        st.caps.has_profiles,
                        st.caps.gpu_mode_access.is_available(),
                        st.caps.gpu_supported_modes.clone(),
                        st.profile.clone(),
                        st.gpu_mode.clone(),
                        gpu_switch_hint_text(&st.caps, &[]),
                    )
                })
                .unwrap_or((
                    false,
                    false,
                    Vec::new(),
                    None,
                    None,
                    "supergfxd unavailable".to_string(),
                ));

        let profile_selected = current_profile
            .as_deref()
            .map(tray_profile_index_from_text)
            .unwrap_or(1);
        let gpu_selected = current_gpu_mode
            .as_deref()
            .and_then(|mode| gpu_mode_index_in_supported(mode, &gpu_modes))
            .unwrap_or(0);
        let gpu_options = gpu_modes
            .iter()
            .map(|mode| RadioItem {
                label: mode.clone(),
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let gpu_submenu = if gpu_options.is_empty() {
            vec![StandardItem {
                label: gpu_hint,
                enabled: false,
                ..Default::default()
            }
            .into()]
        } else {
            vec![
                RadioGroup {
                    selected: gpu_selected,
                    select: Box::new(|this: &mut RogTray, index: usize| {
                        if let Ok(mut st) = this.shared.lock() {
                            if let Some(mode) = st.caps.gpu_supported_modes.get(index).cloned() {
                                st.pending_gpu_mode = Some(mode);
                                st.action_error = None;
                            }
                        }
                    }),
                    options: gpu_options,
                }
                .into(),
                StandardItem {
                    label: gpu_hint,
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            ]
        };

        vec![
            StandardItem {
                label: summary,
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Open Main Window".to_string(),
                activate: Box::new(|this: &mut RogTray| {
                    if let Ok(mut st) = this.shared.lock() {
                        st.show_window = true;
                    }
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Open GPU Page".to_string(),
                activate: Box::new(|this: &mut RogTray| {
                    if let Ok(mut st) = this.shared.lock() {
                        st.show_window = true;
                        st.show_gpu_page = true;
                    }
                }),
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: "Profile".to_string(),
                enabled: has_profiles,
                submenu: vec![RadioGroup {
                    selected: profile_selected,
                    select: Box::new(|this: &mut RogTray, index: usize| {
                        let profile = match index {
                            0 => "Silent",
                            1 => "Balanced",
                            _ => "Turbo",
                        };
                        if let Ok(mut st) = this.shared.lock() {
                            st.pending_profile = Some(profile.to_string());
                            st.action_error = None;
                        }
                    }),
                    options: vec![
                        RadioItem {
                            label: "Silent".to_string(),
                            ..Default::default()
                        },
                        RadioItem {
                            label: "Balanced".to_string(),
                            ..Default::default()
                        },
                        RadioItem {
                            label: "Turbo".to_string(),
                            ..Default::default()
                        },
                    ],
                }
                .into()],
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: "GPU Mode".to_string(),
                enabled: gpu_available,
                submenu: gpu_submenu,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "About".to_string(),
                activate: Box::new(|this: &mut RogTray| {
                    if let Ok(mut st) = this.shared.lock() {
                        st.show_window = true;
                        st.show_about = true;
                    }
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Quit".to_string(),
                activate: Box::new(|this: &mut RogTray| {
                    if let Ok(mut st) = this.shared.lock() {
                        st.quit = true;
                    }
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,rog_helper=debug".into()),
        )
        .with_target(false)
        .init();

    info!("starting rog-helper-ui");

    let _ = adw::init();
    gio::resources_register_include!("rog-ui.gresource")
        .expect("failed to register rog-ui resources");

    let raw_args = std::env::args().collect::<Vec<_>>();
    let start_minimized_from_cli = raw_args.iter().skip(1).any(|arg| {
        matches!(
            arg.as_str(),
            START_MINIMIZED_ARG | "--background" | "--hidden"
        )
    });
    let app_args = raw_args
        .into_iter()
        .filter(|arg| {
            !matches!(
                arg.as_str(),
                START_MINIMIZED_ARG | "--background" | "--hidden"
            )
        })
        .collect::<Vec<_>>();

    let app = adw::Application::builder()
        .application_id("io.github.roghelper.UI")
        .build();

    let ui_started = std::rc::Rc::new(std::cell::Cell::new(false));
    app.connect_activate(move |app| {
        if ui_started.replace(true) {
            if let Some(window) = app
                .active_window()
                .or_else(|| app.windows().into_iter().next())
            {
                window.present();
            }
            return;
        }

        build_ui(app, start_minimized_from_cli);
    });
    app.run_with_args(&app_args);
    Ok(())
}

fn register_app_icon_paths() {
    let Some(display) = gdk::Display::default() else {
        return;
    };

    let icon_theme = gtk::IconTheme::for_display(&display);
    icon_theme.add_resource_path(ICON_RESOURCE_PATH);

    let mut candidate_paths = vec![
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/icons"),
        PathBuf::from("/usr/share/icons"),
        PathBuf::from("/usr/local/share/icons"),
    ];

    if let Some(appdir) = std::env::var_os("APPDIR") {
        candidate_paths.push(PathBuf::from(appdir).join("usr/share/icons"));
    }

    for path in candidate_paths {
        if path.is_dir() {
            icon_theme.add_search_path(path);
        }
    }
}

fn warn_if_fan_icon_missing() {
    let Some(display) = gdk::Display::default() else {
        return;
    };

    let icon_theme = gtk::IconTheme::for_display(&display);

    if !icon_theme.has_icon(ICON_FANS) {
        eprintln!(
            "Warning: rog-fan-symbolic icon was not found. Check crates/rog-ui/assets/icons/hicolor/scalable/actions/rog-fan-symbolic.svg and packaging icon installation."
        );
    }
}

fn build_ui(app: &adw::Application, start_minimized_from_cli: bool) {
    register_app_icon_paths();
    warn_if_fan_icon_missing();

    let app_metadata = AppMetadata::detect();
    let app_settings = load_app_settings();
    let start_hidden = start_minimized_from_cli || app_settings.ui.start_minimized_to_tray;
    let shared = Arc::new(Mutex::new(SharedUiState::default()));
    if let Ok(mut st) = shared.lock() {
        st.update_state = initial_update_state(&app_metadata);
        st.settings = app_settings.clone();
    }
    spawn_background(shared.clone(), app_metadata.clone());

    theme::install();

    let stack = adw::ViewStack::new();
    stack.set_vexpand(true);
    stack.set_hhomogeneous(false);
    stack.set_vhomogeneous(false);

    let cpu_card = MetricCard::new("CPU");
    let gpu_card = MetricCard::new("GPU");
    let system_card = MetricCard::new("System State");
    let batt_card = MetricCard::new("Battery");
    let power_card = MetricCard::new("Power");
    let fans_card = MetricCard::new("Cooling");
    let memory_card = MetricCard::new("Memory");
    let nvme_card = MetricCard::new("NVMe");
    cpu_card.add_css_class("metric-card-primary");
    gpu_card.add_css_class("metric-card-primary");
    system_card.add_css_class("metric-card-primary");
    cpu_card.set_icon_name(ICON_CPU);
    gpu_card.set_icon_name(ICON_GPU);
    system_card.set_icon_name(ICON_DIAGNOSTICS);
    batt_card.set_icon_name(ICON_BATTERY);
    power_card.set_icon_name(ICON_POWER);
    fans_card.set_icon_name(ICON_FANS);
    memory_card.set_icon_name(ICON_RAM);
    nvme_card.set_icon_name(ICON_STORAGE);
    for card in [
        &batt_card,
        &power_card,
        &fans_card,
        &memory_card,
        &nvme_card,
    ] {
        card.add_css_class("metric-card-compact");
        card.add_css_class("dashboard-secondary-card");
        card.set_subtitle_width_chars(22);
    }
    let dashboard_cpu_sparkline = Sparkline::new((0.30, 0.64, 1.0));
    dashboard_cpu_sparkline
        .widget()
        .set_tooltip_text(Some("CPU utilisation over the last 60 seconds"));
    cpu_card.widget().append(dashboard_cpu_sparkline.widget());

    let system_state_details = gtk::Label::new(Some("Controls  —  ·  Setup  —"));
    system_state_details.set_xalign(0.0);
    system_state_details.set_wrap(true);
    system_state_details.add_css_class("dashboard-system-details");
    system_card.widget().append(&system_state_details);
    let system_review_button = gtk::Button::with_label("Review Setup");
    system_review_button.add_css_class("flat");
    system_review_button.set_halign(gtk::Align::Start);
    system_review_button.update_property(&[gtk::accessible::Property::Label(
        "Review Setup and Access status",
    )]);
    {
        let stack = stack.clone();
        system_review_button.connect_clicked(move |_| {
            stack.set_visible_child_name("setup");
        });
    }
    system_card.widget().append(&system_review_button);

    let primary_metrics_grid = gtk::FlowBox::new();
    primary_metrics_grid.set_selection_mode(gtk::SelectionMode::None);
    primary_metrics_grid.set_min_children_per_line(1);
    primary_metrics_grid.set_max_children_per_line(3);
    primary_metrics_grid.set_row_spacing(16);
    primary_metrics_grid.set_column_spacing(16);
    primary_metrics_grid.set_homogeneous(true);
    primary_metrics_grid.insert(
        &dashboard_nav_card(&cpu_card, &stack, "cpu", "Open the CPU page"),
        -1,
    );
    primary_metrics_grid.insert(
        &dashboard_nav_card(&gpu_card, &stack, "gpu", "Open the GPU page"),
        -1,
    );
    primary_metrics_grid.insert(system_card.widget(), -1);

    let secondary_metrics_grid = gtk::FlowBox::new();
    secondary_metrics_grid.set_selection_mode(gtk::SelectionMode::None);
    secondary_metrics_grid.set_min_children_per_line(1);
    secondary_metrics_grid.set_max_children_per_line(5);
    secondary_metrics_grid.set_row_spacing(16);
    secondary_metrics_grid.set_column_spacing(16);
    secondary_metrics_grid.set_homogeneous(true);
    secondary_metrics_grid.insert(
        &dashboard_nav_card(&batt_card, &stack, "battery", "Open the Battery page"),
        -1,
    );
    secondary_metrics_grid.insert(
        &dashboard_nav_card(&fans_card, &stack, "fans", "Open the Cooling page"),
        -1,
    );
    secondary_metrics_grid.insert(
        &dashboard_nav_card(&memory_card, &stack, "ram", "Open the Memory page"),
        -1,
    );
    secondary_metrics_grid.insert(power_card.widget(), -1);
    secondary_metrics_grid.insert(nvme_card.widget(), -1);

    let current_mode_strip = gtk::Box::new(gtk::Orientation::Vertical, 8);
    current_mode_strip.add_css_class("surface-card");
    current_mode_strip.add_css_class("dashboard-mode-strip");
    let current_mode_title = gtk::Label::new(Some("Current System Mode"));
    current_mode_title.set_xalign(0.0);
    current_mode_title.add_css_class("dashboard-panel-title");
    current_mode_strip.append(&current_mode_title);
    let current_mode_flow = gtk::FlowBox::new();
    current_mode_flow.set_selection_mode(gtk::SelectionMode::None);
    current_mode_flow.set_min_children_per_line(1);
    current_mode_flow.set_max_children_per_line(5);
    current_mode_flow.set_column_spacing(8);
    current_mode_flow.set_row_spacing(8);
    current_mode_flow.set_homogeneous(true);
    let (mode_power_item, mode_power) = dashboard_mode_item("Power");
    let (mode_profile_item, mode_profile) = dashboard_mode_item("Profile");
    let (mode_gpu_item, mode_gpu) = dashboard_mode_item("GPU Mode");
    let (mode_cooling_item, mode_cooling) = dashboard_mode_item("Cooling");
    let (mode_battery_item, mode_battery) = dashboard_mode_item("Battery");
    for item in [
        &mode_power_item,
        &mode_profile_item,
        &mode_gpu_item,
        &mode_cooling_item,
        &mode_battery_item,
    ] {
        current_mode_flow.insert(item, -1);
    }
    current_mode_strip.append(&current_mode_flow);

    let warning_banner = adw::Banner::new("Some controls need attention");
    warning_banner.add_css_class("dashboard-warning");
    warning_banner.set_button_label(Some("Review"));
    warning_banner.set_revealed(false);

    let warning_subtitle = gtk::Label::new(Some(""));
    warning_subtitle.set_xalign(0.0);
    warning_subtitle.set_wrap(true);
    warning_subtitle.add_css_class("dim-label");
    warning_subtitle.set_visible(false);

    let open_diagnostics_button = gtk::Button::with_label("Open Diagnostics");
    open_diagnostics_button.add_css_class("flat");
    open_diagnostics_button.set_halign(gtk::Align::Start);
    open_diagnostics_button.set_visible(false);

    let warning_area = gtk::Box::new(gtk::Orientation::Vertical, 6);
    warning_area.append(&warning_banner);
    warning_area.append(&warning_subtitle);
    warning_area.append(&open_diagnostics_button);
    warning_area.set_visible(false);

    let primary_title = gtk::Label::new(Some("At a Glance"));
    primary_title.set_xalign(0.0);
    primary_title.add_css_class("title-3");

    let secondary_title = gtk::Label::new(Some("System Overview"));
    secondary_title.set_xalign(0.0);
    secondary_title.add_css_class("title-3");

    let profile_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    style_linked_toggle_row(&profile_buttons);
    let profile_quiet = gtk::ToggleButton::with_label("Quiet");
    let profile_balanced = gtk::ToggleButton::with_label("Balanced");
    let profile_turbo = gtk::ToggleButton::with_label("Turbo");
    profile_balanced.set_group(Some(&profile_quiet));
    profile_turbo.set_group(Some(&profile_quiet));
    profile_balanced.set_active(true);
    profile_buttons.append(&profile_quiet);
    profile_buttons.append(&profile_balanced);
    profile_buttons.append(&profile_turbo);

    profile_buttons.set_halign(gtk::Align::Start);
    let profile_row = DashboardControlTile::new("Performance Profile", &profile_buttons);

    {
        let shared = shared.clone();
        profile_quiet.connect_toggled(move |btn| {
            if !btn.is_active() {
                return;
            }
            if let Ok(mut st) = shared.lock() {
                st.pending_profile = Some("Silent".to_string());
                st.action_error = None;
            }
        });
    }
    {
        let shared = shared.clone();
        profile_balanced.connect_toggled(move |btn| {
            if !btn.is_active() {
                return;
            }
            if let Ok(mut st) = shared.lock() {
                st.pending_profile = Some("Balanced".to_string());
                st.action_error = None;
            }
        });
    }
    {
        let shared = shared.clone();
        profile_turbo.connect_toggled(move |btn| {
            if !btn.is_active() {
                return;
            }
            if let Ok(mut st) = shared.lock() {
                st.pending_profile = Some("Turbo".to_string());
                st.action_error = None;
            }
        });
    }

    let gpu_control_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let gpu_mode_dropdown = gtk::DropDown::from_strings(&[]);
    style_dropdown_control(&gpu_mode_dropdown);
    gpu_mode_dropdown.set_sensitive(false);
    let gpu_apply_button = gtk::Button::with_label("Apply");
    style_apply_button(&gpu_apply_button);
    gpu_apply_button.set_size_request(82, -1);
    gpu_apply_button.set_sensitive(false);
    style_inline_control_box(&gpu_control_box);
    gpu_control_box.append(&gpu_mode_dropdown);
    gpu_control_box.append(&gpu_apply_button);
    gpu_control_box.set_halign(gtk::Align::Fill);
    let gpu_mode_row = DashboardControlTile::new("GPU Mode", &gpu_control_box);
    {
        let shared = shared.clone();
        let gpu_mode_dropdown = gpu_mode_dropdown.clone();
        gpu_apply_button.connect_clicked(move |_| {
            let Some(item) = gpu_mode_dropdown.selected_item() else {
                return;
            };
            let Ok(item) = item.downcast::<gtk::StringObject>() else {
                return;
            };
            if let Ok(mut st) = shared.lock() {
                st.pending_gpu_mode = Some(item.string().to_string());
                st.action_error = None;
            }
        });
    }

    let charge_limit_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let charge_limit_spin = gtk::SpinButton::with_range(50.0, 100.0, 5.0);
    style_spin_control(&charge_limit_spin, 5);
    charge_limit_spin.set_value(80.0);
    charge_limit_spin.set_sensitive(false);
    charge_limit_spin.set_numeric(true);
    let charge_apply_button = gtk::Button::with_label("Apply");
    style_apply_button(&charge_apply_button);
    charge_apply_button.set_size_request(82, -1);
    charge_apply_button.set_sensitive(false);
    style_inline_control_box(&charge_limit_box);
    charge_limit_box.append(&charge_limit_spin);
    charge_limit_box.append(&charge_apply_button);
    charge_limit_box.set_halign(gtk::Align::Start);
    let charge_limit_row = DashboardControlTile::new("Charge Limit", &charge_limit_box);
    {
        let shared = shared.clone();
        let charge_limit_spin = charge_limit_spin.clone();
        charge_apply_button.connect_clicked(move |_| {
            let limit = charge_limit_spin.value().round().clamp(0.0, 100.0) as u8;
            if let Ok(mut st) = shared.lock() {
                st.pending_battery_limit = Some(limit);
                st.action_error = None;
            }
        });
    }

    let kbd_control_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let kbd_brightness_spin = gtk::SpinButton::with_range(0.0, 3.0, 1.0);
    style_spin_control(&kbd_brightness_spin, 4);
    kbd_brightness_spin.set_sensitive(false);
    kbd_brightness_spin.set_numeric(true);
    let kbd_apply_button = gtk::Button::with_label("Apply");
    style_apply_button(&kbd_apply_button);
    kbd_apply_button.set_size_request(82, -1);
    kbd_apply_button.set_sensitive(false);
    style_inline_control_box(&kbd_control_box);
    kbd_control_box.append(&kbd_brightness_spin);
    kbd_control_box.append(&kbd_apply_button);

    kbd_control_box.set_halign(gtk::Align::Start);
    let kbd_backlight_row = DashboardControlTile::new("Keyboard Brightness", &kbd_control_box);

    {
        let shared = shared.clone();
        let kbd_brightness_spin = kbd_brightness_spin.clone();
        kbd_apply_button.connect_clicked(move |_| {
            let desired_brightness = kbd_brightness_spin.value().round().max(0.0) as u64;
            if let Ok(mut st) = shared.lock() {
                st.pending_lighting = Some(PendingLighting {
                    brightness: Some(desired_brightness),
                    mode: None,
                    rgb_hex: None,
                    secondary_rgb_hex: None,
                    speed: None,
                    direction: None,
                    zone: None,
                });
                st.lighting_error = None;
            }
        });
    }

    let (quick_performance_panel, quick_performance_body) =
        dashboard_panel("Quick Performance", ICON_DASHBOARD);
    quick_performance_panel.add_css_class("dashboard-quick-panel");
    let quick_performance_grid = gtk::FlowBox::new();
    quick_performance_grid.set_selection_mode(gtk::SelectionMode::None);
    quick_performance_grid.set_min_children_per_line(1);
    quick_performance_grid.set_max_children_per_line(2);
    quick_performance_grid.set_row_spacing(10);
    quick_performance_grid.set_column_spacing(10);
    quick_performance_grid.set_homogeneous(true);
    for tile in [
        &profile_row,
        &gpu_mode_row,
        &charge_limit_row,
        &kbd_backlight_row,
    ] {
        quick_performance_grid.insert(tile.widget(), -1);
    }
    quick_performance_body.append(&quick_performance_grid);

    let (cooling_snapshot_panel, cooling_snapshot_body) =
        dashboard_panel("Cooling Snapshot", ICON_FANS);
    cooling_snapshot_panel.add_css_class("dashboard-cooling-panel");
    let thermal_strip = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    thermal_strip.add_css_class("dashboard-thermal-strip");
    thermal_strip.set_homogeneous(true);
    let thermal_cpu = gtk::Label::new(Some("CPU  —"));
    let thermal_gpu = gtk::Label::new(Some("GPU  —"));
    let thermal_nvme = gtk::Label::new(Some("NVMe  —"));
    for label in [&thermal_cpu, &thermal_gpu, &thermal_nvme] {
        label.add_css_class("dashboard-thermal-value");
        thermal_strip.append(label);
    }
    cooling_snapshot_body.append(&thermal_strip);
    let cooling_empty_hint = gtk::Label::new(Some("Waiting for fan telemetry…"));
    cooling_empty_hint.set_xalign(0.0);
    cooling_empty_hint.add_css_class("dim-label");
    cooling_snapshot_body.append(&cooling_empty_hint);
    let dashboard_fan_rows = (0..4)
        .map(|_| {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            row.add_css_class("dashboard-fan-row");
            row.set_visible(false);
            let icon = gtk::Image::from_icon_name(ICON_FANS);
            icon.set_pixel_size(15);
            icon.add_css_class("dashboard-fan-icon");
            let title = gtk::Label::new(None);
            title.set_xalign(0.0);
            title.set_hexpand(true);
            let rpm = gtk::Label::new(None);
            rpm.set_xalign(1.0);
            rpm.add_css_class("monospace");
            let state = gtk::Label::new(Some("●"));
            state.add_css_class("dashboard-fan-dot");
            row.append(&icon);
            row.append(&title);
            row.append(&rpm);
            row.append(&state);
            cooling_snapshot_body.append(&row);
            (row, title, rpm, state)
        })
        .collect::<Vec<_>>();

    let cockpit_middle_grid = gtk::FlowBox::new();
    cockpit_middle_grid.set_selection_mode(gtk::SelectionMode::None);
    cockpit_middle_grid.set_min_children_per_line(1);
    cockpit_middle_grid.set_max_children_per_line(2);
    cockpit_middle_grid.set_row_spacing(16);
    cockpit_middle_grid.set_column_spacing(16);
    cockpit_middle_grid.set_homogeneous(false);
    cockpit_middle_grid.insert(&quick_performance_panel, -1);
    cockpit_middle_grid.insert(&cooling_snapshot_panel, -1);

    let (live_performance_panel, live_performance_body) =
        dashboard_panel("Live Performance", ICON_CPU);
    let live_performance_grid = gtk::FlowBox::new();
    live_performance_grid.set_selection_mode(gtk::SelectionMode::None);
    live_performance_grid.set_min_children_per_line(1);
    live_performance_grid.set_max_children_per_line(2);
    live_performance_grid.set_row_spacing(10);
    live_performance_grid.set_column_spacing(10);
    live_performance_grid.set_homogeneous(true);
    let live_cpu_usage_card = MetricCard::new("CPU Usage");
    let live_cpu_temp_card = MetricCard::new("CPU Temperature");
    let live_gpu_temp_card = MetricCard::new("GPU Temperature");
    let live_memory_card = MetricCard::new("Memory Usage");
    let live_cpu_usage_sparkline = Sparkline::new((0.30, 0.64, 1.0));
    let live_cpu_temp_sparkline = Sparkline::new((0.24, 0.76, 0.67));
    let live_gpu_temp_sparkline = Sparkline::new((0.68, 0.48, 0.96));
    let live_memory_sparkline = Sparkline::new((0.93, 0.62, 0.25));
    for (card, sparkline) in [
        (&live_cpu_usage_card, &live_cpu_usage_sparkline),
        (&live_cpu_temp_card, &live_cpu_temp_sparkline),
        (&live_gpu_temp_card, &live_gpu_temp_sparkline),
        (&live_memory_card, &live_memory_sparkline),
    ] {
        card.add_css_class("dashboard-trend-card");
        sparkline.widget().set_content_height(42);
        card.widget().append(sparkline.widget());
        live_performance_grid.insert(card.widget(), -1);
    }
    live_performance_body.append(&live_performance_grid);

    let (system_health_panel, system_health_body) =
        dashboard_panel("System Health", ICON_DIAGNOSTICS);
    let (health_daemon_row, health_daemon_indicator, health_daemon) =
        dashboard_status_row("rog-helperd");
    let (health_asusd_row, health_asusd_indicator, health_asusd) = dashboard_status_row("asusd");
    let (health_supergfxd_row, health_supergfxd_indicator, health_supergfxd) =
        dashboard_status_row("supergfxd");
    let (health_cpu_row, health_cpu_indicator, health_cpu) = dashboard_status_row("CPU controls");
    let (health_lighting_row, health_lighting_indicator, health_lighting) =
        dashboard_status_row("Lighting");
    let (health_fans_row, health_fans_indicator, health_fans) = dashboard_status_row("Fan control");
    let (health_warnings_row, health_warnings_indicator, health_warnings) =
        dashboard_status_row("Warnings");
    for row in [
        &health_daemon_row,
        &health_asusd_row,
        &health_supergfxd_row,
        &health_cpu_row,
        &health_lighting_row,
        &health_fans_row,
        &health_warnings_row,
    ] {
        system_health_body.append(row);
    }
    let health_review_button = gtk::Button::with_label("Review Setup");
    health_review_button.add_css_class("suggested-action");
    health_review_button.add_css_class("pill");
    health_review_button.set_halign(gtk::Align::Start);
    health_review_button.update_property(&[gtk::accessible::Property::Label(
        "Review Setup and Access status",
    )]);
    {
        let stack = stack.clone();
        health_review_button.connect_clicked(move |_| {
            stack.set_visible_child_name("setup");
        });
    }
    system_health_body.append(&health_review_button);

    let cockpit_lower_grid = gtk::FlowBox::new();
    cockpit_lower_grid.set_selection_mode(gtk::SelectionMode::None);
    cockpit_lower_grid.set_min_children_per_line(1);
    cockpit_lower_grid.set_max_children_per_line(2);
    cockpit_lower_grid.set_row_spacing(16);
    cockpit_lower_grid.set_column_spacing(16);
    cockpit_lower_grid.set_homogeneous(false);
    cockpit_lower_grid.insert(&live_performance_panel, -1);
    cockpit_lower_grid.insert(&system_health_panel, -1);

    let details_temp_group = adw::PreferencesGroup::builder()
        .title("Temperatures")
        .build();
    let details_fan_group = adw::PreferencesGroup::builder().title("Fans").build();
    let details_endpoints_group = adw::PreferencesGroup::builder().title("Endpoints").build();
    let detail_temp_rows = build_detail_rows(&details_temp_group, 8);
    let detail_fan_rows = std::rc::Rc::new(std::cell::RefCell::new(build_detail_rows(
        &details_fan_group,
        0,
    )));
    let detail_endpoint_rows = build_detail_rows(&details_endpoints_group, 6);

    let details_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    details_box.append(&details_temp_group);
    details_box.append(&details_fan_group);
    details_box.append(&details_endpoints_group);

    let details_expander = gtk::Expander::new(Some("Advanced Sensor Details"));
    details_expander.set_expanded(false);
    details_expander.set_child(Some(&details_box));

    let status_label = gtk::Label::new(Some(""));
    status_label.set_xalign(0.0);
    status_label.set_wrap(true);
    status_label.add_css_class("dim-label");

    let dash_root = gtk::Box::new(gtk::Orientation::Vertical, 20);
    dash_root.set_margin_top(20);
    dash_root.set_margin_bottom(28);
    dash_root.set_margin_start(24);
    dash_root.set_margin_end(24);
    dash_root.append(&page_header(
        "Dashboard",
        "Live system status and performance controls.",
    ));
    dash_root.append(&warning_area);
    dash_root.append(&primary_title);
    dash_root.append(&primary_metrics_grid);
    dash_root.append(&current_mode_strip);
    dash_root.append(&secondary_title);
    dash_root.append(&secondary_metrics_grid);
    dash_root.append(&cockpit_middle_grid);
    dash_root.append(&cockpit_lower_grid);
    dash_root.append(&details_expander);
    dash_root.append(&status_label);

    let dash = clamped_scroller(&dash_root);

    let cpu_page = page_container();
    cpu_page.append(&page_header_group(
        "CPU",
        "Processor telemetry, performance policy, and capability-gated Linux controls.",
    ));

    let cpu_overview_group = adw::PreferencesGroup::builder().title("Overview").build();
    let cpu_card_temp = MetricCard::new("CPU Temperature");
    let cpu_card_usage = MetricCard::new("CPU Usage");
    let cpu_card_power = MetricCard::new("CPU Package Power");
    let cpu_card_clock = MetricCard::new("Avg Clock");
    let cpu_metrics_grid = gtk::FlowBox::new();
    cpu_metrics_grid.set_selection_mode(gtk::SelectionMode::None);
    cpu_metrics_grid.set_min_children_per_line(1);
    cpu_metrics_grid.set_max_children_per_line(2);
    cpu_metrics_grid.set_row_spacing(12);
    cpu_metrics_grid.set_column_spacing(12);
    cpu_metrics_grid.insert(cpu_card_temp.widget(), -1);
    cpu_metrics_grid.insert(cpu_card_usage.widget(), -1);
    cpu_metrics_grid.insert(cpu_card_power.widget(), -1);
    cpu_metrics_grid.insert(cpu_card_clock.widget(), -1);
    cpu_overview_group.add(&cpu_metrics_grid);
    cpu_page.append(&cpu_overview_group);

    let cpu_history_group = adw::PreferencesGroup::builder()
        .title("Live Telemetry")
        .description("Recent samples cached by the existing one-second telemetry poll.")
        .build();
    let cpu_history_grid = gtk::FlowBox::new();
    cpu_history_grid.set_selection_mode(gtk::SelectionMode::None);
    cpu_history_grid.set_min_children_per_line(1);
    cpu_history_grid.set_max_children_per_line(2);
    cpu_history_grid.set_row_spacing(16);
    cpu_history_grid.set_column_spacing(16);
    cpu_history_grid.set_homogeneous(true);
    let cpu_usage_graph = HistoryGraph::new("CPU Usage", 0.0, 100.0, "%", (0.30, 0.64, 1.0));
    let cpu_temp_graph = HistoryGraph::new("CPU Temperature", 30.0, 90.0, "°C", (0.24, 0.76, 0.67));
    cpu_usage_graph
        .widget()
        .set_tooltip_text(Some("Recent CPU utilisation history."));
    cpu_temp_graph
        .widget()
        .set_tooltip_text(Some("Recent CPU temperature history."));
    cpu_history_grid.insert(cpu_usage_graph.widget(), -1);
    cpu_history_grid.insert(cpu_temp_graph.widget(), -1);
    cpu_history_group.add(&cpu_history_grid);
    cpu_page.append(&cpu_history_group);

    let cpu_banner_group = adw::PreferencesGroup::builder()
        .title("Control Capability")
        .description("Telemetry remains available when policy controls are read-only.")
        .build();
    let cpu_read_only_banner = adw::Banner::new("Some CPU controls are unavailable");
    cpu_read_only_banner.add_css_class("compact-banner");
    cpu_read_only_banner.set_button_label(Some("Review Access"));
    cpu_read_only_banner.set_revealed(false);
    cpu_banner_group.add(&cpu_read_only_banner);
    cpu_page.append(&cpu_banner_group);

    {
        let stack = stack.clone();
        cpu_read_only_banner.connect_button_clicked(move |_| {
            stack.set_visible_child_name("setup");
        });
    }

    let cpu_access_title = gtk::Label::new(Some("CPU Sysfs Access"));
    cpu_access_title.set_xalign(0.0);
    cpu_access_title.add_css_class("heading");

    let cpu_access_report = gtk::TextBuffer::new(None);
    cpu_access_report.set_text("Loading CPU access diagnostics...");
    let cpu_access_report_view = gtk::TextView::with_buffer(&cpu_access_report);
    cpu_access_report_view.set_editable(false);
    cpu_access_report_view.set_cursor_visible(false);
    cpu_access_report_view.set_wrap_mode(gtk::WrapMode::WordChar);
    cpu_access_report_view.add_css_class("monospace");
    let cpu_access_report_scroll = gtk::ScrolledWindow::new();
    cpu_access_report_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    cpu_access_report_scroll.set_min_content_height(160);
    cpu_access_report_scroll.set_child(Some(&cpu_access_report_view));

    let cpu_access_hint = gtk::Label::new(Some(
        "rog-helper does not bundle an automatic privilege escalation path for CPU sysfs writes. Inspect the report below before changing local system policy.",
    ));
    cpu_access_hint.set_xalign(0.0);
    cpu_access_hint.set_wrap(true);
    cpu_access_hint.add_css_class("dim-label");

    let cpu_quick_group = adw::PreferencesGroup::builder()
        .title("Quick Controls")
        .description("Stage common CPU changes here, then apply them together.")
        .build();

    let cpu_turbo_switch = gtk::Switch::new();
    cpu_turbo_switch.set_valign(gtk::Align::Center);
    let cpu_turbo_row = adw::ActionRow::builder()
        .title("Turbo Boost")
        .subtitle("Checking support...")
        .build();
    cpu_turbo_row.add_suffix(&cpu_turbo_switch);
    cpu_turbo_row.set_activatable(false);
    cpu_quick_group.add(&cpu_turbo_row);

    let cpu_power_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    style_linked_toggle_row(&cpu_power_buttons);
    let cpu_power_quiet = gtk::ToggleButton::with_label("Quiet");
    let cpu_power_balanced = gtk::ToggleButton::with_label("Balanced");
    let cpu_power_perf = gtk::ToggleButton::with_label("Performance");
    cpu_power_balanced.set_group(Some(&cpu_power_quiet));
    cpu_power_perf.set_group(Some(&cpu_power_quiet));
    cpu_power_balanced.set_active(true);
    cpu_power_buttons.append(&cpu_power_quiet);
    cpu_power_buttons.append(&cpu_power_balanced);
    cpu_power_buttons.append(&cpu_power_perf);
    let cpu_power_row = adw::ActionRow::builder()
        .title("Power Preset")
        .subtitle("Checking support...")
        .build();
    cpu_power_row.add_suffix(&cpu_power_buttons);
    cpu_power_row.set_activatable(false);
    cpu_quick_group.add(&cpu_power_row);

    let cpu_min_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 400.0, 6000.0, 50.0);
    style_scale_control(&cpu_min_scale);
    cpu_min_scale.set_draw_value(true);
    cpu_min_scale.set_value_pos(gtk::PositionType::Right);
    cpu_min_scale.set_hexpand(true);
    let cpu_min_row = adw::ActionRow::builder()
        .title("Min Frequency Limit")
        .subtitle("Checking support...")
        .build();
    cpu_min_row.add_suffix(&cpu_min_scale);
    cpu_min_row.set_activatable(false);
    cpu_quick_group.add(&cpu_min_row);

    let cpu_max_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 400.0, 6000.0, 50.0);
    style_scale_control(&cpu_max_scale);
    cpu_max_scale.set_draw_value(true);
    cpu_max_scale.set_value_pos(gtk::PositionType::Right);
    cpu_max_scale.set_hexpand(true);
    let cpu_max_row = adw::ActionRow::builder()
        .title("Max Frequency Limit")
        .subtitle("Checking support...")
        .build();
    cpu_max_row.add_suffix(&cpu_max_scale);
    cpu_max_row.set_activatable(false);
    cpu_quick_group.add(&cpu_max_row);

    let cpu_apply_quick = gtk::Button::with_label("Apply");
    style_apply_button(&cpu_apply_quick);
    cpu_apply_quick.add_css_class("suggested-action");
    let cpu_apply_row = adw::ActionRow::builder()
        .title("Apply Quick Controls")
        .subtitle("Turbo, preset, and frequency changes are sent together.")
        .build();
    cpu_apply_row.add_suffix(&cpu_apply_quick);
    cpu_apply_row.set_activatable(false);
    cpu_quick_group.add(&cpu_apply_row);

    {
        let shared = shared.clone();
        let cpu_turbo_switch = cpu_turbo_switch.clone();
        let cpu_min_scale = cpu_min_scale.clone();
        let cpu_max_scale = cpu_max_scale.clone();
        let cpu_power_quiet = cpu_power_quiet.clone();
        let cpu_power_balanced = cpu_power_balanced.clone();
        cpu_apply_quick.connect_clicked(move |_| {
            let mut actions = Vec::new();
            let caps = shared
                .lock()
                .ok()
                .map(|st| st.cpu_caps.clone())
                .unwrap_or_else(CpuCaps::unknown);

            if caps.control_actionable(CpuControlKind::Boost) {
                actions.push(PendingCpuAction::Turbo(cpu_turbo_switch.is_active()));
            }

            let mode = if cpu_power_quiet.is_active() {
                "Quiet"
            } else if cpu_power_balanced.is_active() {
                "Balanced"
            } else {
                "Performance"
            };
            if caps.control_actionable(CpuControlKind::PowerMode) {
                actions.push(PendingCpuAction::PowerMode(mode.to_string()));
            }

            if caps.control_actionable(CpuControlKind::FreqLimits)
                && (caps.has_min_freq_limit || caps.has_max_freq_limit)
            {
                let min_mhz = if caps.has_min_freq_limit {
                    Some(cpu_min_scale.value().round().max(0.0) as u32)
                } else {
                    None
                };
                let max_mhz = if caps.has_max_freq_limit {
                    Some(cpu_max_scale.value().round().max(0.0) as u32)
                } else {
                    None
                };
                actions.push(PendingCpuAction::FreqLimits { min_mhz, max_mhz });
            }

            if let Ok(mut st) = shared.lock() {
                st.pending_cpu_actions.extend(actions);
                st.action_error = None;
            }
        });
    }

    cpu_page.append(&cpu_quick_group);

    let cpu_policy_group = adw::PreferencesGroup::builder()
        .title("Policy")
        .description("Direct Linux governor and EPP controls for finer tuning.")
        .build();
    let cpu_scaling_driver = pref_value_row(&cpu_policy_group, "Scaling driver", false);
    let cpu_cpu_count = pref_value_row(&cpu_policy_group, "Physical cores", false);
    let cpu_thread_count = pref_value_row(&cpu_policy_group, "Logical threads", false);

    let cpu_governor_combo = gtk::ComboBoxText::new();
    style_combo_control(&cpu_governor_combo);
    cpu_governor_combo.set_sensitive(false);
    let cpu_governor_row = adw::ActionRow::builder()
        .title("Governor")
        .subtitle("Checking support...")
        .build();
    cpu_governor_row.add_suffix(&cpu_governor_combo);
    cpu_governor_row.set_activatable(false);
    cpu_policy_group.add(&cpu_governor_row);

    let cpu_epp_combo = gtk::ComboBoxText::new();
    style_combo_control(&cpu_epp_combo);
    cpu_epp_combo.set_sensitive(false);
    let cpu_epp_row = adw::ActionRow::builder()
        .title("Energy Performance Preference")
        .subtitle("Checking support...")
        .build();
    cpu_epp_row.add_suffix(&cpu_epp_combo);
    cpu_epp_row.set_activatable(false);
    cpu_policy_group.add(&cpu_epp_row);

    let cpu_policy_hint = gtk::Label::new(Some(
        "Lower EPP favors performance; higher EPP favors battery life and lower thermals.",
    ));
    cpu_policy_hint.set_wrap(true);
    cpu_policy_hint.set_xalign(0.0);
    cpu_policy_hint.add_css_class("dim-label");
    cpu_policy_group.add(&cpu_policy_hint);

    let cpu_apply_policy = gtk::Button::with_label("Apply");
    style_apply_button(&cpu_apply_policy);
    let cpu_apply_policy_row = adw::ActionRow::builder()
        .title("Apply Governor / EPP")
        .subtitle("Governor and EPP changes are sent together.")
        .build();
    cpu_apply_policy_row.add_suffix(&cpu_apply_policy);
    cpu_apply_policy_row.set_activatable(false);
    cpu_policy_group.add(&cpu_apply_policy_row);

    {
        let shared = shared.clone();
        let cpu_governor_combo = cpu_governor_combo.clone();
        let cpu_epp_combo = cpu_epp_combo.clone();
        cpu_apply_policy.connect_clicked(move |_| {
            let caps = shared
                .lock()
                .ok()
                .map(|st| st.cpu_caps.clone())
                .unwrap_or_else(CpuCaps::unknown);
            if let Ok(mut st) = shared.lock() {
                if caps.control_actionable(CpuControlKind::Governor) {
                    if let Some(gov) = cpu_governor_combo.active_id() {
                        st.pending_cpu_actions
                            .push(PendingCpuAction::Governor(gov.to_string()));
                    }
                }
                if caps.control_actionable(CpuControlKind::Epp) {
                    if let Some(epp) = cpu_epp_combo.active_id() {
                        st.pending_cpu_actions
                            .push(PendingCpuAction::Epp(epp.to_string()));
                    }
                }
                st.action_error = None;
            }
        });
    }

    cpu_page.append(&cpu_policy_group);

    let cpu_per_core_group = adw::PreferencesGroup::builder()
        .title("Logical CPUs / Threads")
        .description(
            "Scrollable table of logical CPUs showing topology, policy id, online state, usage, and current frequency limits.",
        )
        .build();
    let cpu_per_core_buffer = gtk::TextBuffer::new(None);
    cpu_per_core_buffer.set_text("Loading logical CPUs / threads...");
    let cpu_per_core_view = gtk::TextView::with_buffer(&cpu_per_core_buffer);
    cpu_per_core_view.set_editable(false);
    cpu_per_core_view.set_cursor_visible(false);
    cpu_per_core_view.set_wrap_mode(gtk::WrapMode::None);
    cpu_per_core_view.add_css_class("monospace");
    let cpu_per_core_scroll = gtk::ScrolledWindow::new();
    cpu_per_core_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    cpu_per_core_scroll.set_min_content_height(320);
    cpu_per_core_scroll.set_child(Some(&cpu_per_core_view));
    cpu_per_core_group.add(&cpu_per_core_scroll);
    cpu_page.append(&cpu_per_core_group);

    let cpu_adv_group = adw::PreferencesGroup::builder()
        .title("Advanced &amp; Access")
        .build();
    let cpu_advanced_expander = gtk::Expander::new(Some("Logical CPU Toggles and Access Details"));
    cpu_advanced_expander.set_expanded(false);
    let cpu_adv_box = gtk::Box::new(gtk::Orientation::Vertical, 8);

    let cpu_core_toggle_title = gtk::Label::new(Some("Logical CPU Online / Offline"));
    cpu_core_toggle_title.set_xalign(0.0);
    cpu_core_toggle_title.add_css_class("heading");
    let cpu_core_toggle_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
    cpu_adv_box.append(&cpu_core_toggle_title);
    cpu_adv_box.append(&cpu_core_toggle_box);

    cpu_adv_box.append(&cpu_access_title);
    cpu_adv_box.append(&cpu_access_hint);
    cpu_adv_box.append(&cpu_access_report_scroll);

    let cpu_copy_diag = gtk::Button::with_label("Copy CPU details");
    style_apply_button(&cpu_copy_diag);
    {
        let shared = shared.clone();
        cpu_copy_diag.connect_clicked(move |_| {
            let Some(display) = gtk::gdk::Display::default() else {
                return;
            };
            let text = shared
                .lock()
                .ok()
                .map(|st| cpu_diagnostics_text(&st.cpu_caps, st.cpu.as_ref()))
                .unwrap_or_else(|| "CPU diagnostics unavailable".to_string());
            display.clipboard().set_text(&text);
        });
    }
    cpu_adv_box.append(&cpu_copy_diag);

    cpu_advanced_expander.set_child(Some(&cpu_adv_box));
    cpu_adv_group.add(&cpu_advanced_expander);
    cpu_page.append(&cpu_adv_group);

    let cpu_view = clamped_scroller(&cpu_page);

    let battery_page = page_container();
    battery_page.append(&page_header_group(
        "Battery",
        "Charge state, power flow, battery health, and charging protection.",
    ));

    let battery_hero_group = adw::PreferencesGroup::builder()
        .title("Battery Overview")
        .build();
    let battery_hero_card = MetricCard::new("Current Charge");
    battery_hero_card.add_css_class("metric-card-primary");
    let battery_progress = gtk::ProgressBar::new();
    battery_progress.add_css_class("battery-progress");
    battery_progress.set_show_text(false);
    battery_hero_card.widget().append(&battery_progress);
    battery_hero_group.add(battery_hero_card.widget());
    battery_page.append(&battery_hero_group);

    let battery_charge_group = adw::PreferencesGroup::builder()
        .title("Charging Protection")
        .description("Charge-limit changes are routed through rog-helperd and asusd.")
        .build();
    let battery_charge_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let battery_charge_spin = gtk::SpinButton::with_range(50.0, 100.0, 5.0);
    style_spin_control(&battery_charge_spin, 5);
    battery_charge_spin.set_value(80.0);
    battery_charge_spin.set_sensitive(false);
    battery_charge_spin.set_numeric(true);
    let battery_charge_apply = gtk::Button::with_label("Apply");
    style_apply_button(&battery_charge_apply);
    battery_charge_apply.set_sensitive(false);
    style_inline_control_box(&battery_charge_box);
    battery_charge_box.append(&battery_charge_spin);
    battery_charge_box.append(&battery_charge_apply);
    let battery_charge_row = adw::ActionRow::builder()
        .title("Charge Limit")
        .subtitle("Checking support...")
        .build();
    battery_charge_row.add_suffix(&battery_charge_box);
    battery_charge_row.set_activatable(false);
    battery_charge_group.add(&battery_charge_row);
    battery_page.append(&battery_charge_group);
    {
        let shared = shared.clone();
        let battery_charge_spin = battery_charge_spin.clone();
        battery_charge_apply.connect_clicked(move |_| {
            let limit = battery_charge_spin.value().round().clamp(0.0, 100.0) as u8;
            if let Ok(mut st) = shared.lock() {
                st.pending_battery_limit = Some(limit);
                st.action_error = None;
            }
        });
    }

    let battery_metrics_group = adw::PreferencesGroup::builder()
        .title("Battery Details")
        .description("Live values reported by the existing battery provider.")
        .build();
    let battery_metrics = gtk::FlowBox::new();
    battery_metrics.set_selection_mode(gtk::SelectionMode::None);
    battery_metrics.set_min_children_per_line(2);
    battery_metrics.set_max_children_per_line(5);
    battery_metrics.set_row_spacing(10);
    battery_metrics.set_column_spacing(10);
    battery_metrics.set_homogeneous(true);
    let battery_health_card = MetricCard::new("Health");
    let battery_cycles_card = MetricCard::new("Cycles");
    let battery_power_card = MetricCard::new("Power");
    let battery_time_card = MetricCard::new("Time");
    let battery_source_card = MetricCard::new("Source");
    for card in [
        &battery_health_card,
        &battery_cycles_card,
        &battery_power_card,
        &battery_time_card,
        &battery_source_card,
    ] {
        card.add_css_class("metric-card-compact");
        battery_metrics.insert(card.widget(), -1);
    }
    battery_metrics_group.add(&battery_metrics);
    battery_page.append(&battery_metrics_group);

    let battery = clamped_scroller(&battery_page);

    let ram_page = page_container();
    ram_page.append(&page_header_group(
        "Memory",
        "Physical memory, swap pressure, and the processes using the most RAM.",
    ));
    let memory_hero_group = adw::PreferencesGroup::builder()
        .title("Memory Overview")
        .build();
    let memory_hero_card = MetricCard::new("Physical Memory");
    memory_hero_card.add_css_class("metric-card-primary");
    let memory_progress = gtk::ProgressBar::new();
    memory_progress.add_css_class("memory-progress");
    memory_progress.set_show_text(false);
    memory_hero_card.widget().append(&memory_progress);
    memory_hero_group.add(memory_hero_card.widget());
    ram_page.append(&memory_hero_group);

    let memory_quick_group = adw::PreferencesGroup::builder()
        .title("At a Glance")
        .build();
    let memory_quick_grid = gtk::FlowBox::new();
    memory_quick_grid.set_selection_mode(gtk::SelectionMode::None);
    memory_quick_grid.set_min_children_per_line(2);
    memory_quick_grid.set_max_children_per_line(4);
    memory_quick_grid.set_row_spacing(10);
    memory_quick_grid.set_column_spacing(10);
    memory_quick_grid.set_homogeneous(true);
    let memory_cache_card = MetricCard::new("Cache");
    let memory_shared_card = MetricCard::new("Shared");
    let memory_buffers_card = MetricCard::new("Buffers");
    let memory_swap_card = MetricCard::new("Swap");
    for card in [
        &memory_cache_card,
        &memory_shared_card,
        &memory_buffers_card,
        &memory_swap_card,
    ] {
        card.add_css_class("metric-card-compact");
        memory_quick_grid.insert(card.widget(), -1);
    }
    memory_quick_group.add(&memory_quick_grid);
    ram_page.append(&memory_quick_group);

    let ram_group_core = adw::PreferencesGroup::builder()
        .title("Memory Details")
        .description("Values are read from procfs when available.")
        .build();
    let ram_total = pref_value_row(&ram_group_core, "Total RAM", false);
    let ram_used = pref_value_row(&ram_group_core, "Used RAM", false);
    let ram_avail = pref_value_row(&ram_group_core, "Available", false);
    let ram_free = pref_value_row(&ram_group_core, "Free", false);
    let ram_cached = pref_value_row(&ram_group_core, "Cached", false);
    let ram_buffers = pref_value_row(&ram_group_core, "Buffers", false);
    let ram_shared = pref_value_row(&ram_group_core, "Shared", false);
    let ram_anon = pref_value_row(&ram_group_core, "App/Anon", false);

    let ram_group_swap = adw::PreferencesGroup::builder().title("Swap").build();
    let ram_swap_total = pref_value_row(&ram_group_swap, "Swap Total", false);
    let ram_swap_used = pref_value_row(&ram_group_swap, "Swap Used", false);
    let ram_swap_free = pref_value_row(&ram_group_swap, "Swap Free", false);
    let ram_swap_in = pref_value_row(&ram_group_swap, "Swap In", false);
    let ram_swap_out = pref_value_row(&ram_group_swap, "Swap Out", false);
    let ram_zram = pref_value_row(&ram_group_swap, "zram", false);
    let ram_zswap = pref_value_row(&ram_group_swap, "zswap", false);
    let ram_details_box = gtk::Box::new(gtk::Orientation::Vertical, 16);
    ram_details_box.append(&ram_group_core);
    ram_details_box.append(&ram_group_swap);
    let ram_details_expander = gtk::Expander::new(Some("Memory & swap details"));
    ram_details_expander.set_expanded(false);
    ram_details_expander.set_child(Some(&ram_details_box));
    let ram_details_container = adw::PreferencesGroup::new();
    ram_details_container.add(&ram_details_expander);
    ram_page.append(&ram_details_container);

    let ram_group_pressure = adw::PreferencesGroup::builder()
        .title("Memory Pressure (PSI)")
        .build();
    let ram_psi_some = pref_value_row(&ram_group_pressure, "some (10/60/300)", true);
    let ram_psi_full = pref_value_row(&ram_group_pressure, "full (10/60/300)", true);

    let ram_group_adv = adw::PreferencesGroup::builder()
        .title("Advanced Breakdown")
        .build();
    let ram_active = pref_value_row(&ram_group_adv, "Active", false);
    let ram_inactive = pref_value_row(&ram_group_adv, "Inactive", false);
    let ram_slab = pref_value_row(&ram_group_adv, "Slab", false);
    let ram_sreclaim = pref_value_row(&ram_group_adv, "SReclaimable", false);
    let ram_sunreclaim = pref_value_row(&ram_group_adv, "SUnreclaim", false);
    let ram_mapped = pref_value_row(&ram_group_adv, "Mapped", false);
    let ram_dirty = pref_value_row(&ram_group_adv, "Dirty", false);
    let ram_writeback = pref_value_row(&ram_group_adv, "Writeback", false);
    let ram_pagetables = pref_value_row(&ram_group_adv, "PageTables", false);
    let ram_kernelstack = pref_value_row(&ram_group_adv, "KernelStack", false);
    let ram_advanced_box = gtk::Box::new(gtk::Orientation::Vertical, 16);
    ram_advanced_box.append(&ram_group_pressure);
    ram_advanced_box.append(&ram_group_adv);
    let ram_advanced_expander = gtk::Expander::new(Some("Kernel Memory & Pressure Details"));
    ram_advanced_expander.set_expanded(false);
    ram_advanced_expander.set_child(Some(&ram_advanced_box));
    let ram_advanced_container = adw::PreferencesGroup::builder()
        .title("Advanced")
        .description("Detailed kernel counters for troubleshooting.")
        .build();
    ram_advanced_container.add(&ram_advanced_expander);
    ram_page.append(&ram_advanced_container);

    let copy_top_button = gtk::Button::with_label("Copy");
    let ram_group_top = adw::PreferencesGroup::builder()
        .title("Top Memory Users")
        .description("Sorted by RSS (top 10).")
        .header_suffix(&copy_top_button)
        .build();
    let process_header = gtk::Grid::new();
    process_header.set_column_spacing(12);
    process_header.add_css_class("process-header");
    for (column, title) in ["Process", "PID", "User", "RAM", "Swap"].iter().enumerate() {
        let label = gtk::Label::new(Some(title));
        label.set_xalign(if column == 0 { 0.0 } else { 1.0 });
        label.set_hexpand(column == 0);
        process_header.attach(&label, column as i32, 0, 1, 1);
    }
    ram_group_top.add(&process_header);
    let mut ram_top_rows: Vec<(gtk::Grid, [gtk::Label; 5])> = Vec::new();
    for _ in 0..10 {
        let row = gtk::Grid::new();
        row.set_column_spacing(12);
        row.add_css_class("process-row");
        let labels = std::array::from_fn(|column| {
            let label = gtk::Label::new(None);
            label.set_xalign(if column == 0 { 0.0 } else { 1.0 });
            label.set_hexpand(column == 0);
            label.set_ellipsize(if column == 0 {
                gtk::pango::EllipsizeMode::End
            } else {
                gtk::pango::EllipsizeMode::None
            });
            label.set_tooltip_text(Some(
                "Memory use reported by the existing telemetry provider",
            ));
            row.attach(&label, column as i32, 0, 1, 1);
            label
        });
        ram_group_top.add(&row);
        ram_top_rows.push((row, labels));
    }
    ram_page.append(&ram_group_top);

    {
        let shared = shared.clone();
        copy_top_button.connect_clicked(move |_| {
            let Some(display) = gtk::gdk::Display::default() else {
                return;
            };
            let text = shared
                .lock()
                .ok()
                .and_then(|st| st.telemetry.clone())
                .and_then(|t| {
                    if let Some(txt) = t.mem_top_processes {
                        return Some(txt);
                    }
                    t.mem_top_processes_rows
                        .as_deref()
                        .map(format_top_processes_text)
                })
                .unwrap_or_else(|| "Top processes: (n/a)".to_string());
            display.clipboard().set_text(&text);
        });
    }

    let ram = clamped_scroller(&ram_page);

    let gpu_page = page_container();
    gpu_page.append(&page_header_group(
        "GPU",
        "Graphics mode, thermal state, and ASUS performance profile.",
    ));

    let gpu_overview_group = adw::PreferencesGroup::builder().title("Overview").build();
    let gpu_page_temp_card = MetricCard::new("GPU Temperature");
    gpu_page_temp_card.add_css_class("metric-card-primary");
    let gpu_page_usage_card = MetricCard::new("GPU Utilisation");
    let gpu_page_vram_card = MetricCard::new("VRAM");
    let gpu_page_power_card = MetricCard::new("GPU Power");
    let gpu_page_mode_card = MetricCard::new("GPU Mode");
    let gpu_page_profile_card = MetricCard::new("Performance Profile");
    let gpu_overview_grid = gtk::FlowBox::new();
    gpu_overview_grid.set_selection_mode(gtk::SelectionMode::None);
    gpu_overview_grid.set_min_children_per_line(1);
    gpu_overview_grid.set_max_children_per_line(3);
    gpu_overview_grid.set_row_spacing(16);
    gpu_overview_grid.set_column_spacing(16);
    gpu_overview_grid.set_homogeneous(true);
    gpu_overview_grid.insert(gpu_page_temp_card.widget(), -1);
    gpu_overview_grid.insert(gpu_page_usage_card.widget(), -1);
    gpu_overview_grid.insert(gpu_page_vram_card.widget(), -1);
    gpu_overview_grid.insert(gpu_page_power_card.widget(), -1);
    gpu_overview_grid.insert(gpu_page_mode_card.widget(), -1);
    gpu_overview_grid.insert(gpu_page_profile_card.widget(), -1);
    gpu_overview_group.add(&gpu_overview_grid);
    gpu_page.append(&gpu_overview_group);

    let gpu_history_group = adw::PreferencesGroup::builder()
        .title("Live Telemetry")
        .description(
            "Recent real samples; NVIDIA metrics are refreshed every three seconds by the daemon.",
        )
        .build();
    let gpu_history_grid = gtk::FlowBox::new();
    gpu_history_grid.set_selection_mode(gtk::SelectionMode::None);
    gpu_history_grid.set_min_children_per_line(1);
    gpu_history_grid.set_max_children_per_line(2);
    gpu_history_grid.set_row_spacing(16);
    gpu_history_grid.set_column_spacing(16);
    gpu_history_grid.set_homogeneous(true);
    let gpu_usage_graph = HistoryGraph::new("GPU Utilisation", 0.0, 100.0, "%", (0.30, 0.64, 1.0));
    let gpu_temp_graph = HistoryGraph::new("GPU Temperature", 30.0, 95.0, "°C", (0.68, 0.48, 0.96));
    gpu_history_grid.insert(gpu_usage_graph.widget(), -1);
    gpu_history_grid.insert(gpu_temp_graph.widget(), -1);
    gpu_history_group.add(&gpu_history_grid);
    gpu_history_group.set_visible(false);
    gpu_page.append(&gpu_history_group);

    let gpu_clocks_group = adw::PreferencesGroup::builder()
        .title("Clocks")
        .description("Current read-only clocks reported by the selected telemetry GPU.")
        .build();
    let gpu_page_core_clock = pref_value_row(&gpu_clocks_group, "Core", false);
    let gpu_page_memory_clock = pref_value_row(&gpu_clocks_group, "Memory", false);
    gpu_page_core_clock.set_text("Unavailable");
    gpu_page_memory_clock.set_text("Unavailable");
    gpu_page.append(&gpu_clocks_group);

    let gpu_state_group = adw::PreferencesGroup::builder()
        .title("Dependencies & Telemetry")
        .description(
            "Telemetry remains independent when optional ASUS control services are absent.",
        )
        .build();
    let gpu_page_current_profile = pref_value_row(&gpu_state_group, "asusd · Profiles", false);
    let gpu_page_current_mode = pref_value_row(&gpu_state_group, "supergfxd · GPU Modes", false);
    let gpu_page_switch_hint = pref_value_row(&gpu_state_group, "Switch Hint", false);
    let gpu_page_telemetry_provider = pref_value_row(&gpu_state_group, "NVIDIA Telemetry", false);
    gpu_page_current_profile.set_text("Checking support...");
    gpu_page_current_mode.set_text("Checking support...");
    gpu_page_switch_hint.set_text("Checking support...");
    gpu_page_telemetry_provider.set_text("Checking provider...");
    gpu_page.append(&gpu_state_group);

    let gpu_controls_group = adw::PreferencesGroup::builder()
        .title("Controls")
        .description("Unavailable selectors refer to the single dependency summary above.")
        .build();

    let gpu_page_profile_combo = gtk::ComboBoxText::new();
    style_combo_control(&gpu_page_profile_combo);
    for label in ["Silent", "Balanced", "Turbo"] {
        gpu_page_profile_combo.append(Some(label), label);
    }
    gpu_page_profile_combo.set_active_id(Some("Balanced"));
    gpu_page_profile_combo.set_sensitive(false);
    let gpu_page_profile_apply = gtk::Button::with_label("Apply");
    style_apply_button(&gpu_page_profile_apply);
    gpu_page_profile_apply.set_sensitive(false);
    let gpu_page_profile_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    style_inline_control_box(&gpu_page_profile_box);
    gpu_page_profile_box.append(&gpu_page_profile_combo);
    gpu_page_profile_box.append(&gpu_page_profile_apply);
    let gpu_page_profile_row = adw::ActionRow::builder()
        .title("Performance Profile")
        .subtitle("Checking support...")
        .build();
    gpu_page_profile_row.add_suffix(&gpu_page_profile_box);
    gpu_page_profile_row.set_activatable(false);
    gpu_controls_group.add(&gpu_page_profile_row);

    {
        let shared = shared.clone();
        let gpu_page_profile_combo = gpu_page_profile_combo.clone();
        gpu_page_profile_apply.connect_clicked(move |_| {
            let Some(profile) = gpu_page_profile_combo.active_id() else {
                return;
            };
            if let Ok(mut st) = shared.lock() {
                st.pending_profile = Some(profile.to_string());
                st.action_error = None;
            }
        });
    }

    let gpu_page_mode_combo = gtk::ComboBoxText::new();
    style_combo_control(&gpu_page_mode_combo);
    gpu_page_mode_combo.set_sensitive(false);
    let gpu_page_mode_apply = gtk::Button::with_label("Apply");
    style_apply_button(&gpu_page_mode_apply);
    gpu_page_mode_apply.set_sensitive(false);
    let gpu_page_mode_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    style_inline_control_box(&gpu_page_mode_box);
    gpu_page_mode_box.append(&gpu_page_mode_combo);
    gpu_page_mode_box.append(&gpu_page_mode_apply);
    let gpu_page_mode_row = adw::ActionRow::builder()
        .title("GPU Mode")
        .subtitle("Checking support...")
        .build();
    gpu_page_mode_row.add_suffix(&gpu_page_mode_box);
    gpu_page_mode_row.set_activatable(false);
    gpu_controls_group.add(&gpu_page_mode_row);

    {
        let shared = shared.clone();
        let gpu_page_mode_combo = gpu_page_mode_combo.clone();
        gpu_page_mode_apply.connect_clicked(move |_| {
            let Some(mode) = gpu_page_mode_combo.active_id() else {
                return;
            };
            if let Ok(mut st) = shared.lock() {
                st.pending_gpu_mode = Some(mode.to_string());
                st.action_error = None;
            }
        });
    }

    gpu_page.append(&gpu_controls_group);

    let gpu_status_group = adw::PreferencesGroup::builder()
        .title("Recent Action")
        .description(
            "One summary is shown here instead of repeating dependency messages per control.",
        )
        .build();
    let gpu_page_last_action = pref_value_row(&gpu_status_group, "Summary", false);
    gpu_page_last_action.set_text("Checking support...");
    gpu_page.append(&gpu_status_group);

    let gpu = clamped_scroller(&gpu_page);

    let setup_root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    setup_root.set_margin_top(24);
    setup_root.set_margin_bottom(32);
    setup_root.set_margin_start(24);
    setup_root.set_margin_end(24);
    setup_root.append(&page_header(
        "Setup & Access",
        "Understand which services, permissions, or hardware capabilities affect controls.",
    ));

    let setup_safety_banner = adw::Banner::new(
        "Checks are read-only. rog-helper never runs sudo, changes permissions, or installs packages.",
    );
    setup_safety_banner.add_css_class("compact-banner");
    setup_safety_banner.set_revealed(true);
    setup_root.append(&setup_safety_banner);

    let setup_session_group = adw::PreferencesGroup::builder()
        .title("Session Daemon")
        .description("The unprivileged session service used by the desktop application.")
        .build();
    let (setup_daemon_row, setup_daemon_value) =
        setup_pref_row(&setup_session_group, "rog-helperd");
    setup_root.append(&setup_session_group);

    let setup_administrator_group = adw::PreferencesGroup::builder()
        .title("Administrator Access")
        .description(
            "Authorization is requested per operation. Opening this page never starts authentication.",
        )
        .build();
    let administrator_access_rows = AdministratorAccessRows {
        helper: setup_access_row(&setup_administrator_group, "Privileged helper"),
        system_bus: setup_access_row(&setup_administrator_group, "System DBus"),
        polkit: setup_access_row(&setup_administrator_group, "PolicyKit"),
        cpu: setup_access_row(&setup_administrator_group, "CPU privileged controls"),
        fans: setup_access_row(&setup_administrator_group, "Fan privileged controls"),
        lighting: setup_access_row(&setup_administrator_group, "Lighting privileged controls"),
        gpu: setup_access_row(&setup_administrator_group, "GPU switching"),
        battery: setup_access_row(&setup_administrator_group, "Battery limit"),
    };
    setup_root.append(&setup_administrator_group);

    let setup_services_group = adw::PreferencesGroup::builder()
        .title("Control Services")
        .description("A live API check is required; a binary alone is not considered ready.")
        .build();
    let (setup_asusd_row, setup_asusd_value) = setup_pref_row(&setup_services_group, "asusd");
    let (setup_supergfxd_row, setup_supergfxd_value) =
        setup_pref_row(&setup_services_group, "supergfxd");
    let (setup_upower_row, setup_upower_value) = setup_pref_row(&setup_services_group, "UPower");
    let (setup_nvidia_row, setup_nvidia_value) =
        setup_pref_row(&setup_services_group, "nvidia-smi");
    setup_root.append(&setup_services_group);

    let setup_permissions_group = adw::PreferencesGroup::builder()
        .title("Permissions")
        .description("Detected device paths are shown only in Advanced details below.")
        .build();
    let (setup_cpu_row, setup_cpu_value) =
        setup_pref_row(&setup_permissions_group, "CPU policy writes");
    let (setup_lighting_row, setup_lighting_value) =
        setup_pref_row(&setup_permissions_group, "Keyboard lighting writes");
    let (setup_fans_row, setup_fans_value) =
        setup_pref_row(&setup_permissions_group, "Fan controls");
    setup_root.append(&setup_permissions_group);

    let setup_hardware_group = adw::PreferencesGroup::builder()
        .title("Hardware Support")
        .description(
            "Unsupported hardware remains visible and is not presented as a permission problem.",
        )
        .build();
    let (setup_profiles_row, setup_profiles_value) =
        setup_pref_row(&setup_hardware_group, "Performance profiles");
    let (setup_charge_row, setup_charge_value) =
        setup_pref_row(&setup_hardware_group, "Battery charge limit");
    let (setup_gpu_row, setup_gpu_value) =
        setup_pref_row(&setup_hardware_group, "GPU mode switching");
    let (setup_kbd_support_row, setup_kbd_support_value) =
        setup_pref_row(&setup_hardware_group, "Keyboard lighting");
    let (setup_fan_support_row, setup_fan_support_value) =
        setup_pref_row(&setup_hardware_group, "Fan telemetry and control");
    setup_root.append(&setup_hardware_group);

    let setup_issues_group = adw::PreferencesGroup::builder()
        .title("Recommended Next Steps")
        .build();
    let setup_issues_row = adw::ActionRow::builder()
        .title("Checking setup readiness…")
        .subtitle("Refresh checks to verify services and permissions again.")
        .build();
    setup_issues_row.set_activatable(false);
    setup_issues_group.add(&setup_issues_row);
    setup_root.append(&setup_issues_group);

    let refresh_setup_button = gtk::Button::with_label("Refresh checks");
    refresh_setup_button.add_css_class("suggested-action");
    {
        let shared = shared.clone();
        refresh_setup_button.connect_clicked(move |button| {
            button.set_sensitive(false);
            if let Ok(mut state) = shared.lock() {
                state.pending_setup_refresh = true;
                state.setup_refreshing = true;
                state.mark_render_dirty();
            }
        });
    }
    let copy_setup_button = gtk::Button::with_label("Copy setup diagnostics");
    {
        let shared = shared.clone();
        copy_setup_button.connect_clicked(move |_| {
            let text = shared
                .lock()
                .map(|state| state.setup_status.to_report_text(true))
                .unwrap_or_default();
            if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(&text);
            }
        });
    }
    let copy_privilege_button = gtk::Button::with_label("Copy privilege diagnostics");
    {
        let shared = shared.clone();
        copy_privilege_button.connect_clicked(move |_| {
            let text = shared
                .lock()
                .map(|state| {
                    privilege_diagnostics_text(
                        state.privileged_status.as_ref(),
                        &state.cpu_caps,
                        &state.fan_state,
                        state.lighting.as_ref(),
                        &state.caps,
                    )
                })
                .unwrap_or_else(|_| "Privilege diagnostics unavailable".to_string());
            if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(&text);
            }
        });
    }
    let setup_open_diagnostics = gtk::Button::with_label("Open full Diagnostics");
    {
        let stack = stack.clone();
        setup_open_diagnostics.connect_clicked(move |_| {
            stack.set_visible_child_name("diagnostics");
        });
    }
    let setup_actions = gtk::FlowBox::new();
    setup_actions.set_selection_mode(gtk::SelectionMode::None);
    setup_actions.set_min_children_per_line(1);
    setup_actions.set_max_children_per_line(2);
    setup_actions.set_column_spacing(8);
    setup_actions.set_row_spacing(8);
    setup_actions.set_homogeneous(false);
    setup_actions.set_halign(gtk::Align::Start);
    setup_actions.insert(&refresh_setup_button, -1);
    setup_actions.insert(&copy_setup_button, -1);
    setup_actions.insert(&copy_privilege_button, -1);
    setup_actions.insert(&setup_open_diagnostics, -1);
    setup_root.append(&setup_actions);

    let setup_advanced_buffer = gtk::TextBuffer::new(None);
    setup_advanced_buffer.set_text("Run setup checks to load technical evidence.");
    let setup_advanced_view = gtk::TextView::with_buffer(&setup_advanced_buffer);
    setup_advanced_view.set_editable(false);
    setup_advanced_view.set_cursor_visible(false);
    setup_advanced_view.set_wrap_mode(gtk::WrapMode::WordChar);
    setup_advanced_view.add_css_class("monospace");
    let setup_advanced_scroller = gtk::ScrolledWindow::new();
    setup_advanced_scroller.set_min_content_height(280);
    setup_advanced_scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    setup_advanced_scroller.set_child(Some(&setup_advanced_view));
    let setup_advanced_expander = gtk::Expander::new(Some("Advanced details"));
    setup_advanced_expander.set_expanded(false);
    setup_advanced_expander.set_child(Some(&setup_advanced_scroller));
    setup_root.append(&setup_advanced_expander);
    let setup = clamped_scroller(&setup_root);

    let diag_buffer = gtk::TextBuffer::new(None);
    diag_buffer.set_text("Loading diagnostics...");
    let diag_view = gtk::TextView::with_buffer(&diag_buffer);
    diag_view.set_editable(false);
    diag_view.set_cursor_visible(false);
    diag_view.set_wrap_mode(gtk::WrapMode::WordChar);
    diag_view.add_css_class("monospace");

    let diag_scroller = gtk::ScrolledWindow::new();
    diag_scroller.set_min_content_height(320);
    diag_scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    diag_scroller.set_child(Some(&diag_view));

    let copy_diag_button = gtk::Button::with_label("Copy diagnostics");
    {
        let shared = shared.clone();
        copy_diag_button.connect_clicked(move |_| {
            let text = shared
                .lock()
                .map(|st| {
                    format!(
                        "{}\n\n{}",
                        st.caps_text,
                        privilege_diagnostics_text(
                            st.privileged_status.as_ref(),
                            &st.cpu_caps,
                            &st.fan_state,
                            st.lighting.as_ref(),
                            &st.caps,
                        )
                    )
                })
                .unwrap_or_default();
            let Some(display) = gtk::gdk::Display::default() else {
                return;
            };
            display.clipboard().set_text(&text);
        });
    }

    let diag_root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    diag_root.set_margin_top(24);
    diag_root.set_margin_bottom(32);
    diag_root.set_margin_start(24);
    diag_root.set_margin_end(24);
    diag_root.append(&page_header(
        "Diagnostics",
        "Services, capabilities, permissions, sensor endpoints, and troubleshooting details.",
    ));
    let diag_setup_button = gtk::Button::with_label("Open Setup & Access");
    diag_setup_button.add_css_class("suggested-action");
    diag_setup_button.set_halign(gtk::Align::Start);
    {
        let stack = stack.clone();
        diag_setup_button.connect_clicked(move |_| {
            stack.set_visible_child_name("setup");
        });
    }
    diag_root.append(&diag_setup_button);
    let diag_overview = gtk::FlowBox::new();
    diag_overview.set_selection_mode(gtk::SelectionMode::None);
    diag_overview.set_min_children_per_line(1);
    diag_overview.set_max_children_per_line(3);
    diag_overview.set_row_spacing(16);
    diag_overview.set_column_spacing(16);
    diag_overview.set_homogeneous(true);
    let diag_daemon_card = MetricCard::new("Session Daemon");
    let diag_capabilities_card = MetricCard::new("Available Controls");
    let diag_warnings_card = MetricCard::new("Warnings");
    diag_overview.insert(diag_daemon_card.widget(), -1);
    diag_overview.insert(diag_capabilities_card.widget(), -1);
    diag_overview.insert(diag_warnings_card.widget(), -1);
    diag_root.append(&diag_overview);

    let diag_services_group = adw::PreferencesGroup::builder().title("Services").build();
    let diag_service_daemon = pref_value_row(&diag_services_group, "rog-helperd", false);
    let diag_service_asusd = pref_value_row(&diag_services_group, "asusd", false);
    let diag_service_supergfxd = pref_value_row(&diag_services_group, "supergfxd", false);
    let diag_service_nvidia = pref_value_row(&diag_services_group, "nvidia-smi", false);
    diag_root.append(&diag_services_group);

    let diag_capabilities_group = adw::PreferencesGroup::builder()
        .title("Capabilities")
        .build();
    let diag_capability_profiles =
        pref_value_row(&diag_capabilities_group, "Performance profiles", false);
    let diag_capability_gpu = pref_value_row(&diag_capabilities_group, "GPU modes", false);
    let diag_capability_charge = pref_value_row(&diag_capabilities_group, "Charge limit", false);
    let diag_capability_lighting =
        pref_value_row(&diag_capabilities_group, "Keyboard lighting", false);
    diag_root.append(&diag_capabilities_group);

    let diag_permissions_group = adw::PreferencesGroup::builder()
        .title("Permissions")
        .build();
    let diag_permission_cpu = pref_value_row(&diag_permissions_group, "CPU controls", false);
    let diag_permission_lighting =
        pref_value_row(&diag_permissions_group, "Keyboard lighting", false);
    let diag_permission_fans = pref_value_row(&diag_permissions_group, "Fan control", false);
    diag_root.append(&diag_permissions_group);

    let diag_sensors_group = adw::PreferencesGroup::builder().title("Sensors").build();
    let diag_sensor_cpu = pref_value_row(&diag_sensors_group, "CPU temperature", false);
    let diag_sensor_gpu = pref_value_row(&diag_sensors_group, "GPU temperature", false);
    let diag_sensor_fans = pref_value_row(&diag_sensors_group, "Fan RPM", false);
    let diag_sensor_battery = pref_value_row(&diag_sensors_group, "Battery", false);
    diag_root.append(&diag_sensors_group);

    let diag_warning_group = adw::PreferencesGroup::builder().title("Warnings").build();
    let diag_warning_summary = pref_value_row(&diag_warning_group, "Current summary", false);
    diag_root.append(&diag_warning_group);

    let diag_report_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    copy_diag_button.set_halign(gtk::Align::End);
    diag_report_box.append(&copy_diag_button);
    diag_report_box.append(&diag_scroller);
    let diag_report_expander = gtk::Expander::new(Some("Raw technical report"));
    diag_report_expander.set_expanded(false);
    diag_report_expander.set_child(Some(&diag_report_box));
    let diag_report_group = adw::PreferencesGroup::builder()
        .description("Expand for the complete copyable daemon and provider report.")
        .build();
    diag_report_group.add(&diag_report_expander);
    diag_root.append(&diag_report_group);

    let diag = clamped_scroller(&diag_root);

    let settings_page = page_container();
    settings_page.append(&page_header_group(
        "Settings",
        "Application behavior, dashboard layout, remembered controls, and reset options.",
    ));

    let about_page = page_container();
    about_page.append(&page_header_group(
        "About",
        "Application identity, runtime information, updates, and project links.",
    ));

    let about_identity = gtk::Box::new(gtk::Orientation::Horizontal, 18);
    about_identity.add_css_class("surface-card");
    about_identity.add_css_class("about-identity");
    let about_icon = gtk::Image::from_icon_name(APP_ICON_NAME);
    about_icon.set_pixel_size(64);
    about_icon.add_css_class("about-icon");
    let about_identity_text = gtk::Box::new(gtk::Orientation::Vertical, 4);
    about_identity_text.set_hexpand(true);
    let about_identity_title = gtk::Label::new(Some("ROG Helper"));
    about_identity_title.set_xalign(0.0);
    about_identity_title.add_css_class("title-1");
    let about_identity_subtitle = gtk::Label::new(Some("Hardware Control Centre"));
    about_identity_subtitle.set_xalign(0.0);
    about_identity_subtitle.add_css_class("title-4");
    let about_identity_description = gtk::Label::new(Some(
        "A Linux-native interface for telemetry and capability-gated ASUS ROG laptop controls.",
    ));
    about_identity_description.set_xalign(0.0);
    about_identity_description.set_wrap(true);
    about_identity_description.add_css_class("dim-label");
    let about_version_badge = gtk::Label::new(Some(&format!("v{}", app_metadata.version)));
    about_version_badge.set_halign(gtk::Align::Start);
    about_version_badge.add_css_class("status-chip");
    about_version_badge.add_css_class("status-chip-info");
    about_identity_text.append(&about_identity_title);
    about_identity_text.append(&about_identity_subtitle);
    about_identity_text.append(&about_identity_description);
    about_identity_text.append(&about_version_badge);
    about_identity.append(&about_icon);
    about_identity.append(&about_identity_text);
    about_page.append(&about_identity);

    let about_app_group = adw::PreferencesGroup::builder()
        .title("Application Details")
        .build();
    let about_binary = pref_value_row(&about_app_group, "Binary", false);
    let about_license = pref_value_row(&about_app_group, "License", false);
    let about_dbus = pref_value_row(&about_app_group, "Session DBus API", true);
    about_page.append(&about_app_group);

    let lifecycle_group = adw::PreferencesGroup::builder()
        .title("Startup & Tray")
        .description("Window, tray, and login startup behavior.")
        .build();

    let close_behavior_combo = gtk::ComboBoxText::new();
    style_combo_control(&close_behavior_combo);
    close_behavior_combo.append(Some("minimize_to_tray"), "Minimize to tray");
    close_behavior_combo.append(Some("exit_application"), "Exit application");
    close_behavior_combo.set_active_id(Some(app_settings.ui.close_behavior.id()));
    let close_behavior_row = adw::ActionRow::builder()
        .title("On Close")
        .subtitle("Choose what the window close button does.")
        .build();
    close_behavior_row.add_suffix(&close_behavior_combo);
    close_behavior_row.set_activatable(false);
    lifecycle_group.add(&close_behavior_row);

    let launch_on_login_switch = gtk::Switch::new();
    launch_on_login_switch.set_active(app_settings.ui.launch_on_login);
    launch_on_login_switch.set_valign(gtk::Align::Center);
    let launch_on_login_row = adw::ActionRow::builder()
        .title("Launch on Login")
        .subtitle("Start rog-helper when the user session starts.")
        .build();
    launch_on_login_row.add_suffix(&launch_on_login_switch);
    launch_on_login_row.set_activatable(false);
    lifecycle_group.add(&launch_on_login_row);

    let start_minimized_switch = gtk::Switch::new();
    start_minimized_switch.set_active(app_settings.ui.start_minimized_to_tray);
    start_minimized_switch.set_valign(gtk::Align::Center);
    let start_minimized_row = adw::ActionRow::builder()
        .title("Start Minimized to Tray")
        .subtitle("Open quietly in the background on startup.")
        .build();
    start_minimized_row.add_suffix(&start_minimized_switch);
    start_minimized_row.set_activatable(false);
    lifecycle_group.add(&start_minimized_row);

    let quit_app_button = gtk::Button::with_label("Quit");
    style_apply_button(&quit_app_button);
    quit_app_button.set_size_request(96, -1);
    let quit_app_row = adw::ActionRow::builder()
        .title("Exit Completely")
        .subtitle("Stop telemetry refresh, tray updates, and the background UI process.")
        .build();
    quit_app_row.add_suffix(&quit_app_button);
    quit_app_row.set_activatable(false);
    lifecycle_group.add(&quit_app_row);

    settings_page.append(&lifecycle_group);

    let dashboard_settings_group = adw::PreferencesGroup::builder()
        .title("Dashboard")
        .description("Choose which optional dashboard areas are visible.")
        .build();
    let settings_health_switch = settings_switch_row(
        &dashboard_settings_group,
        "Advanced System Health",
        "Show backend, access, and warning status on the dashboard.",
        app_settings.dashboard.show_system_health,
    );
    let settings_nvme_switch = settings_switch_row(
        &dashboard_settings_group,
        "NVMe Card",
        "Show the NVMe temperature card when a sensor is available.",
        app_settings.dashboard.show_nvme,
    );
    let settings_cooling_switch = settings_switch_row(
        &dashboard_settings_group,
        "Cooling Snapshot",
        "Show the dashboard thermal strip and fan RPM summary.",
        app_settings.dashboard.show_cooling_snapshot,
    );
    let settings_compact_switch = settings_switch_row(
        &dashboard_settings_group,
        "Compact Dashboard",
        "Reduce vertical spacing for smaller displays.",
        app_settings.dashboard.compact,
    );
    settings_page.append(&dashboard_settings_group);

    let control_settings_group = adw::PreferencesGroup::builder()
        .title("Control Preferences")
        .description("Remembered values are suggestions only and are never applied at startup.")
        .build();
    let remember_charge_switch = gtk::Switch::new();
    remember_charge_switch.set_valign(gtk::Align::Center);
    remember_charge_switch.set_active(app_settings.controls.preferred_charge_limit.is_some());
    let preferred_charge_spin = gtk::SpinButton::with_range(40.0, 100.0, 5.0);
    style_spin_control(&preferred_charge_spin, 5);
    preferred_charge_spin.set_numeric(true);
    preferred_charge_spin
        .set_value(app_settings.controls.preferred_charge_limit.unwrap_or(80) as f64);
    preferred_charge_spin.set_sensitive(remember_charge_switch.is_active());
    let preferred_charge_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    preferred_charge_box.append(&remember_charge_switch);
    preferred_charge_box.append(&preferred_charge_spin);
    let preferred_charge_row = adw::ActionRow::builder()
        .title("Preferred Charge Limit")
        .subtitle("Remember a value for manual use; enabling this does not change the battery.")
        .build();
    preferred_charge_row.add_suffix(&preferred_charge_box);
    preferred_charge_row.set_activatable(false);
    control_settings_group.add(&preferred_charge_row);

    let preferred_profile_combo = gtk::ComboBoxText::new();
    style_combo_control(&preferred_profile_combo);
    preferred_profile_combo.append(Some("none"), "Do not remember");
    preferred_profile_combo.append(Some("Silent"), "Quiet");
    preferred_profile_combo.append(Some("Balanced"), "Balanced");
    preferred_profile_combo.append(Some("Turbo"), "Turbo");
    preferred_profile_combo.set_active_id(Some(
        app_settings
            .controls
            .last_manual_profile
            .as_deref()
            .unwrap_or("none"),
    ));
    let preferred_profile_row = adw::ActionRow::builder()
        .title("Last Manual Profile")
        .subtitle("Remember the last manually selected profile without applying it on launch.")
        .build();
    preferred_profile_row.add_suffix(&preferred_profile_combo);
    preferred_profile_row.set_activatable(false);
    control_settings_group.add(&preferred_profile_row);
    settings_page.append(&control_settings_group);

    let automation_group = adw::PreferencesGroup::builder()
        .title("Automation")
        .description("Automatic hardware changes are intentionally unavailable in this release.")
        .build();
    let automation_row = adw::ActionRow::builder()
        .title("Startup Hardware Actions")
        .subtitle("Disabled — saved control preferences are never auto-applied at boot or login.")
        .build();
    automation_row.set_activatable(false);
    automation_group.add(&automation_row);
    settings_page.append(&automation_group);

    let reset_group = adw::PreferencesGroup::builder()
        .title("Reset")
        .description(
            "Reset only rog-helper configuration. Hardware state and other files are untouched.",
        )
        .build();
    let reset_settings_button = gtk::Button::with_label("Reset to Defaults");
    reset_settings_button.add_css_class("destructive-action");
    let reset_settings_row = adw::ActionRow::builder()
        .title("Reset All Settings")
        .subtitle("Restore the versioned configuration defaults after confirmation.")
        .build();
    reset_settings_row.add_suffix(&reset_settings_button);
    reset_settings_row.set_activatable(false);
    reset_group.add(&reset_settings_row);
    settings_page.append(&reset_group);
    let settings = clamped_scroller(&settings_page);

    let about_project_group = adw::PreferencesGroup::builder().title("Project").build();
    let about_authors = pref_value_row(&about_project_group, "Authors", false);
    let about_repository = pref_value_row(&about_project_group, "Repository", false);
    let about_maintainer = pref_value_row(&about_project_group, "Maintainer", false);
    let about_maintainer_github = pref_value_row(&about_project_group, "Maintainer GitHub", false);
    about_page.append(&about_project_group);

    let about_support_group = adw::PreferencesGroup::builder()
        .title("Update &amp; Support")
        .description(format!(
            "Installed version: {}. Manual checks use the GitHub Releases API and fall back to opening the latest release page when automatic replacement is not safe for the current installation.",
            app_metadata.version
        ))
        .build();
    let about_latest_release = pref_value_row(&about_support_group, "Latest Release", false);
    let about_update_status = pref_value_row(&about_support_group, "Status", false);
    let about_last_check = pref_value_row(&about_support_group, "Last Checked", false);
    let about_last_result = pref_value_row(&about_support_group, "Last Result", false);
    let about_release_notes = pref_value_row(&about_support_group, "Release Notes", false);

    let release_notes_full = gtk::Label::new(None);
    release_notes_full.set_xalign(0.0);
    release_notes_full.set_wrap(true);
    release_notes_full.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    release_notes_full.set_selectable(true);
    release_notes_full.add_css_class("dim-label");
    let release_notes_expander = gtk::Expander::new(Some("Show full release notes"));
    release_notes_expander.set_child(Some(&release_notes_full));
    release_notes_expander.set_visible(false);
    about_support_group.add(&release_notes_expander);

    let check_updates_button = gtk::Button::with_label("Check for Updates");
    style_apply_button(&check_updates_button);
    check_updates_button.set_size_request(156, -1);
    let check_updates_row = adw::ActionRow::builder()
        .title("Manual Check")
        .subtitle("Query the latest GitHub release without blocking the UI.")
        .build();
    check_updates_row.add_suffix(&check_updates_button);
    check_updates_row.set_activatable(false);
    about_support_group.add(&check_updates_row);

    let update_action_button = gtk::Button::with_label("Open Releases");
    style_apply_button(&update_action_button);
    update_action_button.set_size_request(132, -1);
    let update_action_row = adw::ActionRow::builder()
        .title("Update")
        .subtitle("Open the latest release page in your browser.")
        .build();
    update_action_row.add_suffix(&update_action_button);
    update_action_row.set_activatable(false);
    about_support_group.add(&update_action_row);

    let open_source_button = gtk::Button::with_label("Open GitHub");
    style_apply_button(&open_source_button);
    open_source_button.set_size_request(124, -1);
    let open_source_row = adw::ActionRow::builder()
        .title("Source")
        .subtitle("Open the project repository in your browser.")
        .build();
    open_source_row.add_suffix(&open_source_button);
    open_source_row.set_activatable(false);
    about_support_group.add(&open_source_row);

    let support_button = gtk::Button::with_label("Open Repository");
    style_apply_button(&support_button);
    support_button.set_size_request(140, -1);
    let support_row = adw::ActionRow::builder()
        .title("Support")
        .subtitle("Repository home, README, and release downloads.")
        .build();
    support_row.add_suffix(&support_button);
    support_row.set_activatable(false);
    about_support_group.add(&support_row);

    let report_issue_button = gtk::Button::with_label("Report Issue");
    style_apply_button(&report_issue_button);
    report_issue_button.set_size_request(126, -1);
    let report_issue_row = adw::ActionRow::builder()
        .title("Issue Reporting")
        .subtitle("Open the GitHub issues page for this project.")
        .build();
    report_issue_row.add_suffix(&report_issue_button);
    report_issue_row.set_activatable(false);
    about_support_group.add(&report_issue_row);

    about_page.append(&about_support_group);

    about_binary.set_text(&app_metadata.binary_name);
    about_license.set_text(&app_metadata.license);
    about_dbus.set_text(&format!(
        "{DAEMON_DBUS_NAME} {DAEMON_DBUS_PATH} ({DAEMON_DBUS_IFACE})"
    ));
    about_authors.set_text(&app_metadata.authors);
    about_repository.set_text(
        app_metadata
            .repository_url
            .as_deref()
            .unwrap_or("Repository URL is not configured."),
    );
    about_maintainer.set_text(&app_metadata.maintainer_name);
    about_maintainer_github.set_text(
        app_metadata
            .maintainer_url
            .as_deref()
            .unwrap_or("Maintainer GitHub URL is not configured."),
    );

    let initial_update_state = shared
        .lock()
        .ok()
        .map(|st| st.update_state.clone())
        .unwrap_or_default();
    about_latest_release.set_text(
        initial_update_state
            .latest_version
            .as_deref()
            .unwrap_or("Waiting for a manual check."),
    );
    about_update_status.set_text(&initial_update_state.status_text);
    about_last_check.set_text(
        &initial_update_state
            .last_checked_unix
            .map(format_timestamp_local)
            .unwrap_or_else(|| "Not checked in this session.".to_string()),
    );
    about_last_result.set_text(&initial_update_state.last_result_text);
    about_release_notes.set_text(
        initial_update_state
            .release_notes_preview
            .as_deref()
            .unwrap_or("Release notes from the latest checked release will appear here."),
    );

    if let Some(notes) = initial_update_state.release_notes_full.as_deref() {
        release_notes_full.set_text(notes);
        release_notes_expander.set_visible(true);
    } else {
        release_notes_full.set_text("");
        release_notes_expander.set_visible(false);
    }

    open_source_button.set_sensitive(app_metadata.repository_url.is_some());
    support_button.set_sensitive(app_metadata.repository_url.is_some());
    report_issue_button.set_sensitive(app_metadata.issues_url.is_some());

    {
        let shared = shared.clone();
        close_behavior_combo.connect_changed(move |combo| {
            let behavior = combo
                .active_id()
                .and_then(|id| CloseBehavior::from_id(&id))
                .unwrap_or(CloseBehavior::MinimizeToTray);
            if let Err(error) = persist_settings_change(&shared, |settings| {
                settings.ui.close_behavior = behavior;
            }) {
                if let Ok(mut st) = shared.lock() {
                    st.pending_toast = Some((error, true));
                }
            }
        });
    }
    {
        let shared = shared.clone();
        launch_on_login_switch.connect_state_set(move |_, enabled| {
            match persist_settings_change(&shared, |settings| {
                settings.ui.launch_on_login = enabled;
            }) {
                Ok(_) => glib::Propagation::Proceed,
                Err(error) => {
                    if let Ok(mut st) = shared.lock() {
                        st.pending_toast = Some((error, true));
                    }
                    glib::Propagation::Stop
                }
            }
        });
    }
    {
        let shared = shared.clone();
        start_minimized_switch.connect_state_set(move |_, enabled| {
            match persist_settings_change(&shared, |settings| {
                settings.ui.start_minimized_to_tray = enabled;
            }) {
                Ok(_) => glib::Propagation::Proceed,
                Err(error) => {
                    if let Ok(mut st) = shared.lock() {
                        st.pending_toast = Some((error, true));
                    }
                    glib::Propagation::Stop
                }
            }
        });
    }
    for (switch, update) in [
        (settings_health_switch.clone(), 0_u8),
        (settings_nvme_switch.clone(), 1_u8),
        (settings_cooling_switch.clone(), 2_u8),
        (settings_compact_switch.clone(), 3_u8),
    ] {
        let shared = shared.clone();
        switch.connect_state_set(move |_, enabled| {
            let result = persist_settings_change(&shared, |settings| match update {
                0 => settings.dashboard.show_system_health = enabled,
                1 => settings.dashboard.show_nvme = enabled,
                2 => settings.dashboard.show_cooling_snapshot = enabled,
                _ => settings.dashboard.compact = enabled,
            });
            if let Err(error) = result {
                if let Ok(mut st) = shared.lock() {
                    st.pending_toast = Some((error, true));
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    {
        let shared = shared.clone();
        let preferred_charge_spin = preferred_charge_spin.clone();
        remember_charge_switch.connect_state_set(move |_, enabled| {
            preferred_charge_spin.set_sensitive(enabled);
            let value = preferred_charge_spin.value().round().clamp(40.0, 100.0) as u8;
            match persist_settings_change(&shared, |settings| {
                settings.controls.preferred_charge_limit = enabled.then_some(value);
            }) {
                Ok(_) => glib::Propagation::Proceed,
                Err(error) => {
                    if let Ok(mut st) = shared.lock() {
                        st.pending_toast = Some((error, true));
                    }
                    glib::Propagation::Stop
                }
            }
        });
    }
    {
        let shared = shared.clone();
        preferred_charge_spin.connect_value_changed(move |spin| {
            let enabled = shared
                .lock()
                .map(|st| st.settings.controls.preferred_charge_limit.is_some())
                .unwrap_or(false);
            if enabled {
                let value = spin.value().round().clamp(40.0, 100.0) as u8;
                let _ = persist_settings_change(&shared, |settings| {
                    settings.controls.preferred_charge_limit = Some(value);
                });
            }
        });
    }
    {
        let shared = shared.clone();
        preferred_profile_combo.connect_changed(move |combo| {
            let profile = combo
                .active_id()
                .filter(|value| value.as_str() != "none")
                .map(|value| value.to_string());
            if let Err(error) = persist_settings_change(&shared, |settings| {
                settings.controls.last_manual_profile = profile;
            }) {
                if let Ok(mut st) = shared.lock() {
                    st.pending_toast = Some((error, true));
                }
            }
        });
    }
    {
        let shared = shared.clone();
        reset_settings_button.connect_clicked(move |_| {
            let dialog = adw::MessageDialog::new(
                None::<&gtk::Window>,
                Some("Reset all settings?"),
                Some("This resets only rog-helper's config.toml. It does not apply hardware controls or remove unrelated files."),
            );
            dialog.add_responses(&[("cancel", "Cancel"), ("reset", "Reset Settings")]);
            dialog.set_close_response("cancel");
            dialog.set_default_response(Some("cancel"));
            dialog.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
            let shared_reset = shared.clone();
            dialog.connect_response(Some("reset"), move |dialog, _| {
                if let Ok(mut st) = shared_reset.lock() {
                    st.pending_config_reset = true;
                }
                dialog.close();
            });
            dialog.connect_response(Some("cancel"), |dialog, _| dialog.close());
            dialog.present();
        });
    }
    {
        let shared = shared.clone();
        let app = app.clone();
        quit_app_button.connect_clicked(move |_| {
            if let Ok(mut st) = shared.lock() {
                st.quit = true;
            }
            app.quit();
        });
    }
    {
        let shared = shared.clone();
        check_updates_button.connect_clicked(move |_| {
            if let Ok(mut st) = shared.lock() {
                if st.update_check_in_progress || st.update_action_in_progress {
                    return;
                }
                st.pending_update_check = true;
                st.update_check_in_progress = true;
                st.update_state.status_text =
                    "Checking GitHub for the latest release...".to_string();
                st.update_state.last_result_text =
                    "Waiting for the latest release information from GitHub.".to_string();
                st.mark_render_dirty();
            }
        });
    }
    {
        let shared = shared.clone();
        let fallback_releases_url = app_metadata.releases_url.clone();
        update_action_button.connect_clicked(move |_| {
            let release_page_url = {
                let mut st = match shared.lock() {
                    Ok(st) => st,
                    Err(_) => return,
                };
                if st.update_check_in_progress || st.update_action_in_progress {
                    return;
                }
                if st.update_state.supports_in_place_update
                    && st.update_state.downloadable_asset.is_some()
                {
                    st.pending_update_action = true;
                    st.update_action_in_progress = true;
                    st.update_state.status_text = "Preparing the update...".to_string();
                    st.update_state.last_result_text =
                        "Downloading the latest release asset.".to_string();
                    st.mark_render_dirty();
                    None
                } else {
                    st.update_state.last_result_text =
                        "Automatic in-place update is not supported for this installation. Opening the latest release page instead.".to_string();
                    st.mark_render_dirty();
                    st.update_state
                        .release_page_url
                        .clone()
                        .or_else(|| fallback_releases_url.clone())
                }
            };

            if let Some(url) = release_page_url {
                open_uri_with_feedback(
                    &shared,
                    &url,
                    "Unable to open the latest release page right now.",
                );
            } else if let Ok(mut st) = shared.lock() {
                st.pending_toast = Some(("Repository URL is not configured.".to_string(), true));
            }
        });
    }
    {
        let shared = shared.clone();
        let repository_url = app_metadata.repository_url.clone();
        open_source_button.connect_clicked(move |_| {
            if let Some(url) = repository_url.as_deref() {
                open_uri_with_feedback(&shared, url, "Unable to open the project repository.");
            } else if let Ok(mut st) = shared.lock() {
                st.pending_toast = Some(("Repository URL is not configured.".to_string(), true));
            }
        });
    }
    {
        let shared = shared.clone();
        let repository_url = app_metadata.repository_url.clone();
        support_button.connect_clicked(move |_| {
            if let Some(url) = repository_url.as_deref() {
                open_uri_with_feedback(&shared, url, "Unable to open the support repository page.");
            } else if let Ok(mut st) = shared.lock() {
                st.pending_toast = Some(("Repository URL is not configured.".to_string(), true));
            }
        });
    }
    {
        let shared = shared.clone();
        let issues_url = app_metadata.issues_url.clone();
        report_issue_button.connect_clicked(move |_| {
            if let Some(url) = issues_url.as_deref() {
                open_uri_with_feedback(&shared, url, "Unable to open the issue tracker.");
            } else if let Ok(mut st) = shared.lock() {
                st.pending_toast =
                    Some(("Issue reporting URL is not configured.".to_string(), true));
            }
        });
    }

    let about = clamped_scroller(&about_page);

    let lighting_page = page_container();
    lighting_page.append(&page_header_group(
        "Lighting",
        "Stage an effect locally, then apply one validated change through the detected backend.",
    ));
    let lighting_overview_group = adw::PreferencesGroup::builder()
        .title("Lighting Device")
        .description("Hardware identity and capabilities reported by the active backend.")
        .build();
    let lighting_device = pref_value_row(&lighting_overview_group, "Device", false);
    let lighting_backend = pref_value_row(&lighting_overview_group, "Backend", false);
    let lighting_current = pref_value_row(&lighting_overview_group, "Brightness", false);
    let lighting_mode = pref_value_row(&lighting_overview_group, "Mode", false);
    let lighting_rgb_current = pref_value_row(&lighting_overview_group, "RGB Colour", false);
    let lighting_rgb_capability = pref_value_row(&lighting_overview_group, "RGB control", false);
    let lighting_argb_capability = pref_value_row(
        &lighting_overview_group,
        "Addressable / multi-zone RGB",
        false,
    );
    let lighting_per_key_capability =
        pref_value_row(&lighting_overview_group, "Per-key RGB", false);

    let lighting_capability_banner = adw::Banner::new("Lighting support is being checked");
    lighting_capability_banner.add_css_class("compact-banner");
    lighting_capability_banner.set_revealed(true);
    lighting_page.append(&lighting_capability_banner);

    let preview_colour = std::rc::Rc::new(std::cell::RefCell::new(gdk::RGBA::new(
        0.28, 0.31, 0.36, 1.0,
    )));
    let preview_draw_colour = preview_colour.clone();
    let lighting_preview = gtk::DrawingArea::new();
    lighting_preview.set_content_width(520);
    lighting_preview.set_content_height(150);
    lighting_preview.set_hexpand(true);
    lighting_preview.set_draw_func(move |_, ctx, width, height| {
        draw_keyboard_lighting_preview(ctx, width, height, &preview_draw_colour.borrow());
    });
    let preview_caption = gtk::Label::new(Some("Checking keyboard lighting topology…"));
    preview_caption.set_xalign(0.0);
    preview_caption.set_wrap(true);
    preview_caption.add_css_class("dim-label");
    let preview_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    preview_box.append(&lighting_preview);
    preview_box.append(&preview_caption);
    let lighting_preview_group = adw::PreferencesGroup::builder()
        .title("Keyboard Preview")
        .description("This preview is local. Hardware is written only when Apply is clicked.")
        .build();
    lighting_preview_group.add(&preview_box);
    lighting_page.append(&lighting_preview_group);

    let lighting_controls_group = adw::PreferencesGroup::builder().title("Controls").build();
    let mode_combo = gtk::ComboBoxText::new();
    mode_combo.set_sensitive(false);
    let mode_row = adw::ActionRow::builder().title("Lighting Mode").build();
    mode_row.add_suffix(&mode_combo);
    mode_row.set_activatable(false);
    lighting_controls_group.add(&mode_row);

    let brightness_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 3.0, 1.0);
    brightness_scale.set_hexpand(true);
    brightness_scale.set_draw_value(true);
    brightness_scale.set_value_pos(gtk::PositionType::Right);
    let brightness_row = adw::ActionRow::builder().title("Brightness").build();
    brightness_row.add_suffix(&brightness_scale);
    brightness_row.set_activatable(false);
    lighting_controls_group.add(&brightness_row);

    let rgb_button = gtk::ColorButton::new();
    rgb_button.set_use_alpha(false);
    rgb_button.set_title("Keyboard RGB");
    rgb_button.set_sensitive(false);
    let rgb_row = adw::ActionRow::builder()
        .title("RGB Colour")
        .subtitle("Checking whether RGB colour control is available.")
        .build();
    rgb_row.set_visible(false);
    rgb_row.add_suffix(&rgb_button);
    rgb_row.set_activatable(false);
    lighting_controls_group.add(&rgb_row);

    let secondary_rgb_button = gtk::ColorButton::new();
    secondary_rgb_button.set_use_alpha(false);
    secondary_rgb_button.set_title("Secondary RGB colour");
    secondary_rgb_button.set_sensitive(false);
    let secondary_rgb_row = adw::ActionRow::builder()
        .title("Secondary Colour")
        .subtitle("Used only by the Breathe effect on supported hardware.")
        .build();
    secondary_rgb_row.set_visible(false);
    secondary_rgb_row.add_suffix(&secondary_rgb_button);
    secondary_rgb_row.set_activatable(false);
    lighting_controls_group.add(&secondary_rgb_row);

    let speed_combo = gtk::ComboBoxText::new();
    speed_combo.set_sensitive(false);
    let speed_row = adw::ActionRow::builder()
        .title("Effect Speed")
        .subtitle("Speed is available only for effects that support it.")
        .build();
    speed_row.set_visible(false);
    speed_row.add_suffix(&speed_combo);
    speed_row.set_activatable(false);
    lighting_controls_group.add(&speed_row);

    let direction_combo = gtk::ComboBoxText::new();
    direction_combo.set_sensitive(false);
    let direction_row = adw::ActionRow::builder()
        .title("Direction")
        .subtitle("Direction applies only to Rainbow Wave.")
        .build();
    direction_row.set_visible(false);
    direction_row.add_suffix(&direction_combo);
    direction_row.set_activatable(false);
    lighting_controls_group.add(&direction_row);

    let zone_combo = gtk::ComboBoxText::new();
    zone_combo.set_sensitive(false);
    let zone_row = adw::ActionRow::builder()
        .title("Lighting Zone")
        .subtitle("Only zones reported by the selected backend are listed.")
        .build();
    zone_row.set_visible(false);
    zone_row.add_suffix(&zone_combo);
    zone_row.set_activatable(false);
    lighting_controls_group.add(&zone_row);

    let apply_lighting = gtk::Button::with_label("Apply lighting");
    apply_lighting.add_css_class("suggested-action");
    let apply_row = adw::ActionRow::builder().title("Apply Changes").build();
    apply_row.add_suffix(&apply_lighting);
    apply_row.set_activatable(false);
    lighting_controls_group.add(&apply_row);
    lighting_page.append(&lighting_controls_group);
    let lighting_rgb_note = gtk::Label::new(Some(
        "RGB effects are not exposed by the current lighting backend.",
    ));
    lighting_rgb_note.set_xalign(0.0);
    lighting_rgb_note.set_wrap(true);
    lighting_rgb_note.add_css_class("dim-label");
    lighting_rgb_note.set_visible(false);
    lighting_page.append(&lighting_rgb_note);

    let lighting_status_group = adw::PreferencesGroup::builder().title("Status").build();
    let lighting_availability = pref_value_row(&lighting_status_group, "Availability", false);
    let lighting_last_action = pref_value_row(&lighting_status_group, "Last action", false);
    let copy_lighting_diagnostics = gtk::Button::with_label("Copy Diagnostics");
    let copy_lighting_row = adw::ActionRow::builder()
        .title("Lighting Diagnostics")
        .subtitle("Copy the local capability and backend report to the clipboard.")
        .build();
    copy_lighting_row.add_suffix(&copy_lighting_diagnostics);
    copy_lighting_row.set_activatable(false);
    lighting_status_group.add(&copy_lighting_row);
    lighting_page.append(&lighting_status_group);
    {
        let shared = shared.clone();
        copy_lighting_diagnostics.connect_clicked(move |_| {
            let Some(display) = gtk::gdk::Display::default() else {
                return;
            };
            let text = shared
                .lock()
                .ok()
                .and_then(|state| state.lighting.as_ref().map(lighting_diagnostics_text))
                .unwrap_or_else(|| "Lighting diagnostics are not available yet.".to_string());
            display.clipboard().set_text(&text);
        });
    }

    let lighting_backend_expander = gtk::Expander::new(Some("Backend details"));
    lighting_backend_expander.set_expanded(false);
    lighting_backend_expander.set_child(Some(&lighting_overview_group));
    let lighting_backend_container = adw::PreferencesGroup::builder()
        .description("Technical provider values are available when troubleshooting is needed.")
        .build();
    lighting_backend_container.add(&lighting_backend_expander);
    lighting_page.append(&lighting_backend_container);

    let lighting_unknown = FeatureAvailability::unknown();
    lighting_device.set_text(&lighting_placeholder_value(&lighting_unknown));
    lighting_backend.set_text(&feature_value_label(&lighting_unknown));
    lighting_current.set_text(&lighting_placeholder_value(&lighting_unknown));
    lighting_mode.set_text(&lighting_placeholder_value(&lighting_unknown));
    lighting_rgb_current.set_text(&lighting_placeholder_value(&lighting_unknown));
    lighting_rgb_capability.set_text("Checking support");
    lighting_argb_capability.set_text("Checking support");
    lighting_per_key_capability.set_text("Checking support");
    lighting_availability.set_text(&feature_value_label(&lighting_unknown));
    lighting_last_action.set_text(&lighting_last_action_text(None, &lighting_unknown));
    brightness_row.set_subtitle(&lighting_brightness_subtitle(None, &lighting_unknown));
    mode_row.set_subtitle(&lighting_mode_subtitle(None, &lighting_unknown));
    apply_row.set_subtitle(&lighting_apply_subtitle(None, &lighting_unknown));
    rgb_row.set_subtitle(&lighting_rgb_subtitle(None, &lighting_unknown));

    {
        let shared = shared.clone();
        let brightness_scale = brightness_scale.clone();
        let mode_combo = mode_combo.clone();
        let rgb_button = rgb_button.clone();
        let secondary_rgb_button = secondary_rgb_button.clone();
        let speed_combo = speed_combo.clone();
        let direction_combo = direction_combo.clone();
        let zone_combo = zone_combo.clone();
        apply_lighting.connect_clicked(move |_| {
            let desired_brightness = brightness_scale.value().round().max(0.0) as u64;
            let desired_mode = mode_combo.active_id().map(|s| s.to_string());

            if let Ok(mut st) = shared.lock() {
                let Some(lighting) = st.lighting.as_ref() else {
                    return;
                };
                let selected_mode = desired_mode
                    .as_deref()
                    .unwrap_or(&lighting.mode)
                    .to_string();
                let brightness = lighting.supports_brightness.then_some(desired_brightness);
                let mode = lighting.supports_modes.then_some(desired_mode).flatten();
                let rgb_hex = (lighting.supports_rgb
                    && lighting_mode_supports_primary_colour(&selected_mode))
                .then(|| rgba_to_hex(&rgb_button.rgba()));
                let secondary_rgb_hex = (lighting.supports_rgb
                    && lighting_mode_supports_secondary_colour(&selected_mode))
                .then(|| rgba_to_hex(&secondary_rgb_button.rgba()));
                let speed = (lighting.supports_speed
                    && lighting_mode_supports_speed(&selected_mode))
                .then(|| speed_combo.active_id().map(|value| value.to_string()))
                .flatten();
                let direction = lighting_mode_supports_direction(&selected_mode)
                    .then(|| direction_combo.active_id().map(|value| value.to_string()))
                    .flatten();
                let zone = (lighting.supports_zones && !lighting.supported_zones.is_empty())
                    .then(|| zone_combo.active_id().map(|value| value.to_string()))
                    .flatten();
                if brightness.is_none()
                    && mode.is_none()
                    && rgb_hex.is_none()
                    && secondary_rgb_hex.is_none()
                    && speed.is_none()
                    && direction.is_none()
                    && zone.is_none()
                {
                    return;
                }
                st.pending_lighting = Some(PendingLighting {
                    brightness,
                    mode,
                    rgb_hex,
                    secondary_rgb_hex,
                    speed,
                    direction,
                    zone,
                });
                st.lighting_error = None;
            }
        });
    }

    {
        let preview_colour = preview_colour.clone();
        let lighting_preview = lighting_preview.clone();
        rgb_button.connect_color_set(move |button| {
            *preview_colour.borrow_mut() = button.rgba();
            lighting_preview.queue_draw();
        });
    }
    {
        let shared = shared.clone();
        let secondary_rgb_row = secondary_rgb_row.clone();
        let speed_row = speed_row.clone();
        let direction_row = direction_row.clone();
        mode_combo.connect_changed(move |combo| {
            let selected_mode = combo.active_id().map(|value| value.to_string());
            let lighting = shared.lock().ok().and_then(|state| state.lighting.clone());
            let Some(lighting) = lighting else {
                secondary_rgb_row.set_visible(false);
                speed_row.set_visible(false);
                direction_row.set_visible(false);
                return;
            };
            let mode = selected_mode.as_deref().unwrap_or(&lighting.mode);
            secondary_rgb_row.set_visible(
                lighting.supports_rgb && lighting_mode_supports_secondary_colour(mode),
            );
            speed_row.set_visible(
                lighting.supports_speed
                    && !lighting.supported_speeds.is_empty()
                    && lighting_mode_supports_speed(mode),
            );
            direction_row.set_visible(
                !lighting.supported_directions.is_empty() && lighting_mode_supports_direction(mode),
            );
        });
    }

    let lighting = clamped_scroller(&lighting_page);

    let fans_root = gtk::Box::new(gtk::Orientation::Vertical, 20);
    fans_root.add_css_class("fans-page");
    fans_root.set_margin_top(24);
    fans_root.set_margin_bottom(32);
    fans_root.set_margin_start(24);
    fans_root.set_margin_end(24);

    let fans_header = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let fans_pills = gtk::FlowBox::new();
    fans_pills.set_selection_mode(gtk::SelectionMode::None);
    fans_pills.set_min_children_per_line(1);
    fans_pills.set_max_children_per_line(4);
    fans_pills.set_column_spacing(8);
    fans_pills.set_row_spacing(8);
    let fan_backend_label = fans_status_pill("backend: checking");
    let fan_count_label = fans_status_pill("0 fans detected");
    let fan_mode_label = fans_status_pill("Checking mode");
    let fan_mapping_label = fans_status_pill("Mapping OK");
    fans_pills.insert(&fan_backend_label, -1);
    fans_pills.insert(&fan_count_label, -1);
    fans_pills.insert(&fan_mode_label, -1);
    fans_pills.insert(&fan_mapping_label, -1);
    fans_header.append(&page_header(
        "Cooling",
        "Temperatures, live fan RPM, backend status, and safe controls when supported.",
    ));
    fans_header.append(&fans_pills);
    fans_root.append(&fans_header);

    let fans_banner = adw::Banner::new("Fan control is capability-dependent");
    fans_banner.add_css_class("fans-warning-banner");
    fans_banner.set_revealed(true);
    fans_banner.set_button_label(Some("Review Access"));
    fans_root.append(&fans_banner);
    {
        let stack = stack.clone();
        fans_banner.connect_button_clicked(move |_| {
            stack.set_visible_child_name("setup");
        });
    }

    let hero = gtk::Box::new(gtk::Orientation::Vertical, 14);
    hero.add_css_class("fans-hero");
    let gauge_row = gtk::FlowBox::new();
    gauge_row.add_css_class("fans-gauge-row");
    gauge_row.set_selection_mode(gtk::SelectionMode::None);
    gauge_row.set_min_children_per_line(1);
    gauge_row.set_max_children_per_line(2);
    gauge_row.set_column_spacing(16);
    gauge_row.set_row_spacing(16);
    let gpu_temp_gauge = TempGauge::new("GPU", GaugeAccent::Blue);
    let cpu_temp_gauge = TempGauge::new("CPU", GaugeAccent::Magenta);
    gpu_temp_gauge.set_speed_metrics("Core Clock", None, Some("Memory Clock"), None);
    cpu_temp_gauge.set_speed_metrics("Avg Clock", None, None, None);
    let gpu_gauge_card = build_fans_gauge_card(&gpu_temp_gauge);
    let cpu_gauge_card = build_fans_gauge_card(&cpu_temp_gauge);
    gauge_row.insert(&cpu_gauge_card, -1);
    gauge_row.insert(&gpu_gauge_card, -1);
    hero.append(&gauge_row);

    let mode_panel = gtk::Box::new(gtk::Orientation::Vertical, 10);
    mode_panel.add_css_class("fan-control-panel");
    mode_panel.add_css_class("fans-cooling-mode-card");
    let mode_title = gtk::Label::new(Some("Cooling Mode"));
    mode_title.set_xalign(0.0);
    mode_title.add_css_class("title-3");
    let mode_hint = gtk::Label::new(Some(
        "Controls will be enabled automatically when a supported fan-control backend is detected.",
    ));
    mode_hint.set_xalign(0.0);
    mode_hint.set_wrap(true);
    mode_hint.add_css_class("dim-label");
    let fan_last_action_label = gtk::Label::new(Some("No fan action applied in this session."));
    fan_last_action_label.set_xalign(0.0);
    fan_last_action_label.set_wrap(true);
    fan_last_action_label.add_css_class("fan-panel-note");
    mode_panel.append(&mode_title);
    mode_panel.append(&mode_hint);
    mode_panel.append(&fan_last_action_label);
    hero.append(&mode_panel);

    let hero_fans_flow = gtk::FlowBox::new();
    hero_fans_flow.add_css_class("fans-rotor-grid");
    hero_fans_flow.set_selection_mode(gtk::SelectionMode::None);
    hero_fans_flow.set_min_children_per_line(1);
    hero_fans_flow.set_max_children_per_line(4);
    hero_fans_flow.set_column_spacing(12);
    hero_fans_flow.set_row_spacing(12);
    let hero_fan_slots = (0..4)
        .map(|_| {
            let slot = build_fan_visual_slot();
            hero_fans_flow.insert(&slot.root, -1);
            slot
        })
        .collect::<Vec<_>>();
    hero.append(&hero_fans_flow);
    fans_root.append(&hero);

    let fans_controls = gtk::Box::new(gtk::Orientation::Vertical, 12);
    fans_controls.add_css_class("fan-control-panel");
    let controls_title = gtk::Label::new(Some("Safe Controls"));
    controls_title.set_xalign(0.0);
    controls_title.add_css_class("title-3");
    let controls_hint = gtk::Label::new(Some(
        "Fan RPM telemetry is available, but manual controls need a writable fan-control endpoint confirmed by rog-helperd.",
    ));
    controls_hint.set_xalign(0.0);
    controls_hint.set_wrap(true);
    controls_hint.add_css_class("dim-label");
    fans_controls.append(&controls_title);
    fans_controls.append(&controls_hint);

    let fan_mode_combo = gtk::ComboBoxText::new();
    style_combo_control(&fan_mode_combo);
    for mode in ["Auto / BIOS Default", "Manual", "Curve", "Full Speed Boost"] {
        fan_mode_combo.append(Some(mode), mode);
    }
    fan_mode_combo.set_active_id(Some("Auto / BIOS Default"));
    fan_mode_combo.set_sensitive(false);
    fan_mode_combo.set_tooltip_text(Some(
        "Mode selection is unavailable because no writable fan-control endpoint was confirmed.",
    ));
    let fan_mode_row = adw::ActionRow::builder()
        .title("Mode")
        .subtitle("Auto / BIOS Default is active.")
        .build();
    fan_mode_row.add_suffix(&fan_mode_combo);
    fan_mode_row.set_activatable(false);
    fans_controls.append(&fan_mode_row);

    let fan_sync_switch = gtk::Switch::new();
    fan_sync_switch.set_valign(gtk::Align::Center);
    fan_sync_switch.set_active(app_settings.controls.fan_sync_enabled);
    fan_sync_switch.set_tooltip_text(Some(
        "Sync fans is unavailable until more than one controllable fan is detected.",
    ));
    let fan_sync_row = adw::ActionRow::builder()
        .title("Sync Fans")
        .subtitle("Use one setting for every controllable fan.")
        .build();
    fan_sync_row.add_suffix(&fan_sync_switch);
    fan_sync_row.set_activatable(false);
    fans_controls.append(&fan_sync_row);
    {
        let shared = shared.clone();
        fan_sync_switch.connect_state_set(move |_, enabled| {
            if let Err(error) = persist_settings_change(&shared, |settings| {
                settings.controls.fan_sync_enabled = enabled;
            }) {
                if let Ok(mut st) = shared.lock() {
                    st.pending_toast = Some((error, true));
                }
                return glib::Propagation::Stop;
            }
            if let Ok(mut st) = shared.lock() {
                st.pending_fan_action = Some(PendingFanAction::Sync(enabled));
                st.action_error = None;
            }
            glib::Propagation::Proceed
        });
    }

    let fan_manual_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 5.0);
    style_scale_control(&fan_manual_scale);
    fan_manual_scale.set_draw_value(true);
    fan_manual_scale.set_value_pos(gtk::PositionType::Right);
    fan_manual_scale.set_value(60.0);
    fan_manual_scale.set_sensitive(false);
    fan_manual_scale.set_tooltip_text(Some(
        "Manual fan speed is unavailable because no writable fan-control endpoint was confirmed.",
    ));
    let fan_manual_apply = gtk::Button::with_label("Apply");
    style_apply_button(&fan_manual_apply);
    fan_manual_apply.set_sensitive(false);
    fan_manual_apply.set_tooltip_text(Some(
        "Manual fan speed is unavailable because no writable fan-control endpoint was confirmed.",
    ));
    let fan_manual_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    style_inline_control_box(&fan_manual_box);
    fan_manual_box.append(&fan_manual_scale);
    fan_manual_box.append(&fan_manual_apply);
    let fan_manual_row = adw::ActionRow::builder()
        .title("Manual Speed")
        .subtitle("Manual percentage control is unavailable until a writable backend is detected.")
        .build();
    fan_manual_row.add_suffix(&fan_manual_box);
    fan_manual_row.set_activatable(false);
    fans_controls.append(&fan_manual_row);
    {
        let shared = shared.clone();
        let fan_manual_scale = fan_manual_scale.clone();
        fan_manual_apply.connect_clicked(move |_| {
            let percent = fan_manual_scale.value().round().clamp(0.0, 100.0) as u8;
            let needs_ack = shared
                .lock()
                .map(|st| !st.settings.ui.fan_warning_acknowledged)
                .unwrap_or(true);
            if needs_ack {
                let dialog = adw::MessageDialog::new(
                    None::<&gtk::Window>,
                    Some("Manual fan control"),
                    Some("Manual fan control may increase noise, reduce cooling if configured incorrectly, or be overridden by firmware. Use Auto mode if you are unsure."),
                );
                dialog.add_responses(&[
                    ("cancel", "Cancel"),
                    ("apply", "Enable Manual Control"),
                ]);
                dialog.set_close_response("cancel");
                dialog.set_default_response(Some("cancel"));
                let shared_apply = shared.clone();
                dialog.connect_response(Some("apply"), move |d, _| {
                    let _ = persist_settings_change(&shared_apply, |settings| {
                        settings.ui.fan_warning_acknowledged = true;
                    });
                    if let Ok(mut st) = shared_apply.lock() {
                        st.pending_fan_action = Some(PendingFanAction::ManualPercent {
                            fan_id: String::new(),
                            percent,
                        });
                        st.action_error = None;
                    }
                    d.close();
                });
                dialog.connect_response(Some("cancel"), |d, _| d.close());
                dialog.present();
            } else if let Ok(mut st) = shared.lock() {
                st.pending_fan_action = Some(PendingFanAction::ManualPercent {
                    fan_id: String::new(),
                    percent,
                });
                st.action_error = None;
            }
        });
    }

    let fans_boost_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    style_inline_control_box(&fans_boost_box);
    let fan_boost_5 = gtk::Button::with_label("5 min");
    let fan_boost_10 = gtk::Button::with_label("10 min");
    let fan_boost_15 = gtk::Button::with_label("15 min");
    let fan_stop_boost = gtk::Button::with_label("Stop");
    for button in [&fan_boost_5, &fan_boost_10, &fan_boost_15, &fan_stop_boost] {
        style_apply_button(button);
        button.set_sensitive(false);
        button.set_tooltip_text(Some(
            "Boost is unavailable because manual percentage fan control is not supported by the active backend.",
        ));
        fans_boost_box.append(button);
    }
    let fan_boost_row = adw::ActionRow::builder()
        .title("Full Speed Boost")
        .subtitle("Boost is time-limited and restores Auto when the timer ends.")
        .build();
    fan_boost_row.add_suffix(&fans_boost_box);
    fan_boost_row.set_activatable(false);
    fans_controls.append(&fan_boost_row);
    for (button, duration_seconds) in [
        (fan_boost_5.clone(), 300_u64),
        (fan_boost_10.clone(), 600_u64),
        (fan_boost_15.clone(), 900_u64),
    ] {
        let shared = shared.clone();
        button.connect_clicked(move |_| {
            if let Ok(mut st) = shared.lock() {
                st.pending_fan_action = Some(PendingFanAction::Boost {
                    fan_id: String::new(),
                    duration_seconds,
                });
                st.action_error = None;
            }
        });
    }
    {
        let shared = shared.clone();
        fan_stop_boost.connect_clicked(move |_| {
            if let Ok(mut st) = shared.lock() {
                st.pending_fan_action = Some(PendingFanAction::Auto {
                    fan_id: String::new(),
                });
                st.action_error = None;
            }
        });
    }

    let fan_auto_button = gtk::Button::with_label("Return to Auto");
    style_apply_button(&fan_auto_button);
    fan_auto_button.set_tooltip_text(Some(
        "Return to Auto becomes active when a controllable fan backend is detected.",
    ));
    let fan_copy_diag_button = gtk::Button::with_label("Copy fan diagnostics");
    style_apply_button(&fan_copy_diag_button);
    let fans_action_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    style_inline_control_box(&fans_action_box);
    fans_action_box.append(&fan_auto_button);
    fans_action_box.append(&fan_copy_diag_button);
    let fan_auto_row = adw::ActionRow::builder()
        .title("BIOS Default")
        .subtitle("Return controllable fans to firmware/backend automatic mode.")
        .build();
    fan_auto_row.add_suffix(&fans_action_box);
    fan_auto_row.set_activatable(false);
    fans_controls.append(&fan_auto_row);
    {
        let shared = shared.clone();
        fan_auto_button.connect_clicked(move |_| {
            if let Ok(mut st) = shared.lock() {
                st.pending_fan_action = Some(PendingFanAction::Auto {
                    fan_id: String::new(),
                });
                st.action_error = None;
            }
        });
    }
    {
        let shared = shared.clone();
        fan_copy_diag_button.connect_clicked(move |_| {
            let Some(display) = gtk::gdk::Display::default() else {
                return;
            };
            let text = shared
                .lock()
                .map(|st| fan_state_diagnostics_text(&st.fan_state))
                .unwrap_or_else(|_| "Fan diagnostics unavailable".to_string());
            display.clipboard().set_text(&text);
        });
    }
    fans_root.append(&fans_controls);

    let curve_card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    curve_card.add_css_class("fan-curve-card");
    let curve_title = gtk::Label::new(Some("Fan Curve Preview"));
    curve_title.set_xalign(0.0);
    curve_title.add_css_class("title-3");
    let curve_status = gtk::Label::new(Some("Fan curve editing is not available on this backend."));
    curve_status.set_xalign(0.0);
    curve_status.set_wrap(true);
    curve_status.add_css_class("dim-label");
    let curve_preview = CurvePreview::new();
    let curve_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    curve_actions.set_halign(gtk::Align::End);
    let curve_apply = gtk::Button::with_label("Apply curve");
    let curve_reset = gtk::Button::with_label("Reset to Auto");
    for button in [&curve_apply, &curve_reset] {
        style_apply_button(button);
        button.set_sensitive(false);
        button.set_tooltip_text(Some(
            "Fan curve editing is unavailable on the active fan backend.",
        ));
        curve_actions.append(button);
    }
    curve_card.append(&curve_title);
    curve_card.append(&curve_status);
    curve_card.append(curve_preview.widget());
    curve_card.append(&curve_actions);
    fans_root.append(&curve_card);
    {
        let shared = shared.clone();
        let preview = curve_preview.clone();
        curve_apply.connect_clicked(move |_| {
            if let Ok(mut state) = shared.lock() {
                let fan_id = state
                    .fan_state
                    .fans
                    .iter()
                    .find(|fan| fan.supports_curve && fan.controllable)
                    .map(|fan| fan.id.clone());
                if let Some(fan_id) = fan_id {
                    state.pending_fan_action = Some(PendingFanAction::Curve {
                        fan_id,
                        points: preview.points(),
                    });
                    state.action_error = None;
                }
            }
        });
    }
    {
        let shared = shared.clone();
        curve_reset.connect_clicked(move |_| {
            if let Ok(mut state) = shared.lock() {
                state.pending_fan_action = Some(PendingFanAction::Auto {
                    fan_id: String::new(),
                });
                state.action_error = None;
            }
        });
    }

    let fan_cards_title = gtk::Label::new(Some("Detected Fans"));
    fan_cards_title.set_xalign(0.0);
    fan_cards_title.add_css_class("title-3");
    fans_root.append(&fan_cards_title);

    let fan_cards_flow = gtk::FlowBox::new();
    fan_cards_flow.set_selection_mode(gtk::SelectionMode::None);
    fan_cards_flow.set_min_children_per_line(1);
    fan_cards_flow.set_max_children_per_line(2);
    fan_cards_flow.set_column_spacing(12);
    fan_cards_flow.set_row_spacing(12);
    let fan_card_slots = (0..8)
        .map(|_| {
            let slot = build_fan_card_slot();
            fan_cards_flow.insert(&slot.root, -1);
            slot
        })
        .collect::<Vec<_>>();
    fans_root.append(&fan_cards_flow);

    let fan_cards_buffer = gtk::TextBuffer::new(None);
    fan_cards_buffer.set_text("Loading fan state...");
    let fan_cards_view = gtk::TextView::with_buffer(&fan_cards_buffer);
    fan_cards_view.set_editable(false);
    fan_cards_view.set_cursor_visible(false);
    fan_cards_view.set_wrap_mode(gtk::WrapMode::WordChar);
    fan_cards_view.add_css_class("monospace");
    let fan_cards_scroll = gtk::ScrolledWindow::new();
    fan_cards_scroll.set_min_content_height(260);
    fan_cards_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    fan_cards_scroll.set_child(Some(&fan_cards_view));
    let diagnostics_expander = gtk::Expander::new(Some("Show fan diagnostics"));
    diagnostics_expander.add_css_class("fan-diagnostics-box");
    diagnostics_expander.set_expanded(false);
    diagnostics_expander.set_child(Some(&fan_cards_scroll));
    diagnostics_expander.connect_expanded_notify(|expander| {
        expander.set_label(Some(if expander.is_expanded() {
            "Hide fan diagnostics"
        } else {
            "Show fan diagnostics"
        }));
    });
    fans_root.append(&diagnostics_expander);

    let fans = clamped_scroller(&fans_root);

    stack.add_titled_with_icon(&dash, Some("dashboard"), "Dashboard", ICON_DASHBOARD);
    stack.add_titled_with_icon(&cpu_view, Some("cpu"), "CPU", ICON_CPU);
    stack.add_titled_with_icon(&gpu, Some("gpu"), "GPU", ICON_GPU);
    stack.add_titled_with_icon(&battery, Some("battery"), "Battery", ICON_BATTERY);
    stack.add_titled_with_icon(&ram, Some("ram"), "Memory", ICON_RAM);
    stack.add_titled_with_icon(&lighting, Some("lighting"), "Lighting", ICON_LIGHTING);
    stack.add_titled_with_icon(&fans, Some("fans"), "Cooling", ICON_FANS);
    stack.add_titled_with_icon(&setup, Some("setup"), "Setup & Access", ICON_DIAGNOSTICS);
    stack.add_titled_with_icon(&settings, Some("settings"), "Settings", ICON_SETTINGS);
    stack.add_titled_with_icon(&diag, Some("diagnostics"), "Diagnostics", ICON_DIAGNOSTICS);
    stack.add_titled_with_icon(&about, Some("about"), "About", ICON_ABOUT);

    let connection_status = gtk::Label::new(Some("Connecting…"));
    let navigation_items = [
        NavigationItem {
            name: "dashboard",
            label: "Dashboard",
            icon: ICON_DASHBOARD,
        },
        NavigationItem {
            name: "cpu",
            label: "CPU",
            icon: ICON_CPU,
        },
        NavigationItem {
            name: "gpu",
            label: "GPU",
            icon: ICON_GPU,
        },
        NavigationItem {
            name: "battery",
            label: "Battery",
            icon: ICON_BATTERY,
        },
        NavigationItem {
            name: "ram",
            label: "Memory",
            icon: ICON_RAM,
        },
        NavigationItem {
            name: "lighting",
            label: "Lighting",
            icon: ICON_LIGHTING,
        },
        NavigationItem {
            name: "fans",
            label: "Cooling",
            icon: ICON_FANS,
        },
        NavigationItem {
            name: "setup",
            label: "Setup & Access",
            icon: ICON_DIAGNOSTICS,
        },
        NavigationItem {
            name: "settings",
            label: "Settings",
            icon: ICON_SETTINGS,
        },
        NavigationItem {
            name: "diagnostics",
            label: "Diagnostics",
            icon: ICON_DIAGNOSTICS,
        },
        NavigationItem {
            name: "about",
            label: "About",
            icon: ICON_ABOUT,
        },
    ];
    let view = shell::build_shell(&stack, &connection_status, &navigation_items, APP_ICON_NAME);

    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&view));

    let win = adw::ApplicationWindow::builder()
        .application(app)
        .title("rog-helper")
        .default_width(1180)
        .default_height(800)
        .build();
    win.set_icon_name(Some(APP_ICON_NAME));
    win.set_content(Some(&toast_overlay));

    {
        let stack = stack.clone();
        win.connect_maximized_notify(move |window| {
            debug!(
                maximized = window.is_maximized(),
                width = window.width(),
                height = window.height(),
                default_width = window.default_width(),
                default_height = window.default_height(),
                visible_page = stack.visible_child_name().as_deref().unwrap_or("unknown"),
                "top-level window maximize state changed"
            );
        });
    }

    let app_keepalive = app.hold();
    {
        let shared = shared.clone();
        let app_clone = app.clone();
        win.connect_close_request(move |window| {
            let _keepalive = &app_keepalive;
            let (close_behavior, tray_available) = shared
                .lock()
                .map(|st| (st.settings.ui.close_behavior, st.tray_available))
                .unwrap_or((CloseBehavior::ExitApplication, Some(false)));

            if close_behavior == CloseBehavior::MinimizeToTray && tray_available != Some(false) {
                window.hide();
                if let Ok(mut st) = shared.lock() {
                    if !st.settings.ui.close_to_tray_hint_shown {
                        st.settings.ui.close_to_tray_hint_shown = true;
                        st.pending_toast = Some((
                            "rog-helper is still running in the tray.".to_string(),
                            false,
                        ));
                        st.pending_config_save = Some(st.settings.clone());
                    }
                }
            } else if let Ok(mut st) = shared.lock() {
                st.quit = true;
                app_clone.quit();
            } else {
                app_clone.quit();
            }

            glib::Propagation::Stop
        });
    }

    if start_hidden {
        win.hide();
    } else {
        win.present();
    }

    {
        let stack = stack.clone();
        warning_banner.connect_button_clicked(move |_| {
            stack.set_visible_child_name("setup");
        });
    }
    {
        let stack = stack.clone();
        open_diagnostics_button.connect_clicked(move |_| {
            stack.set_visible_child_name("diagnostics");
        });
    }

    // Periodically refresh UI from the background thread state.
    let shared_clone = shared.clone();
    let win_clone = win.clone();
    let app_clone = app.clone();
    let stack_clone = stack.clone();
    let toast_overlay_clone = toast_overlay.clone();
    let update_check_available = app_metadata.repository_url.is_some();
    let update_action_available = app_metadata.releases_url.is_some();
    let last_supported_modes = std::rc::Rc::new(std::cell::RefCell::<Vec<String>>::new(Vec::new()));
    let last_supported_speeds =
        std::rc::Rc::new(std::cell::RefCell::<Vec<String>>::new(Vec::new()));
    let last_supported_directions =
        std::rc::Rc::new(std::cell::RefCell::<Vec<String>>::new(Vec::new()));
    let last_supported_zones = std::rc::Rc::new(std::cell::RefCell::<Vec<String>>::new(Vec::new()));
    let last_gpu_supported_modes =
        std::rc::Rc::new(std::cell::RefCell::<Vec<String>>::new(Vec::new()));
    let last_cpu_governors = std::rc::Rc::new(std::cell::RefCell::<Vec<String>>::new(Vec::new()));
    let last_cpu_epp = std::rc::Rc::new(std::cell::RefCell::<Vec<String>>::new(Vec::new()));
    let last_rendered_revision = std::rc::Rc::new(std::cell::Cell::new(None::<u64>));
    let last_dashboard_layout = std::rc::Rc::new(std::cell::Cell::new(None::<DashboardLayout>));
    let detail_fan_rows_ref = detail_fan_rows.clone();
    let details_fan_group_ref = details_fan_group.clone();
    let fan_animation_rotors = hero_fan_slots
        .iter()
        .map(|slot| slot.rotor.clone())
        .chain(fan_card_slots.iter().map(|slot| slot.rotor.clone()))
        .collect::<Vec<_>>();
    let fan_animation_last = std::rc::Rc::new(std::cell::RefCell::new(SystemTime::now()));
    {
        let fan_animation_last = fan_animation_last.clone();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            let now = SystemTime::now();
            let delta = now
                .duration_since(*fan_animation_last.borrow())
                .unwrap_or_default()
                .as_secs_f64()
                .clamp(0.0, 0.10);
            *fan_animation_last.borrow_mut() = now;
            let reduced_motion = gtk::Settings::default()
                .map(|settings| !settings.is_gtk_enable_animations())
                .unwrap_or(false);
            for rotor in &fan_animation_rotors {
                rotor.tick(delta, reduced_motion);
            }
            glib::ControlFlow::Continue
        });
    }
    glib::timeout_add_local(Duration::from_millis(250), move || {
        let (
            telemetry,
            cpu,
            cpu_caps,
            cpu_usage_history,
            cpu_temp_history,
            gpu_temp_history,
            gpu_usage_history,
            memory_usage_history,
            caps,
            caps_text,
            setup_status,
            privileged_status,
            setup_refreshing,
            warnings,
            profile,
            gpu_mode,
            battery_limit,
            lighting,
            fan_state,
            lighting_error_txt,
            action_error_txt,
            update_state,
            update_check_in_progress,
            update_action_in_progress,
            pending_open_link,
            daemon_error,
            settings,
            show_window,
            show_about,
            show_gpu_page,
            pending_toast,
            quit,
            render_revision,
        ) = {
            let mut st = match shared_clone.lock() {
                Ok(st) => st,
                Err(_) => return glib::ControlFlow::Continue,
            };
            let show_window = st.show_window;
            st.show_window = false;
            let show_about = st.show_about;
            st.show_about = false;
            let show_gpu_page = st.show_gpu_page;
            st.show_gpu_page = false;
            let pending_toast = st.pending_toast.take();
            (
                st.telemetry.clone(),
                st.cpu.clone(),
                st.cpu_caps.clone(),
                st.cpu_usage_history.clone(),
                st.cpu_temp_history.clone(),
                st.gpu_temp_history.clone(),
                st.gpu_usage_history.clone(),
                st.memory_usage_history.clone(),
                st.caps.clone(),
                st.caps_text.clone(),
                st.setup_status.clone(),
                st.privileged_status.clone(),
                st.setup_refreshing,
                st.warnings.clone(),
                st.profile.clone(),
                st.gpu_mode.clone(),
                st.battery_limit,
                st.lighting.clone(),
                st.fan_state.clone(),
                st.lighting_error.clone(),
                st.action_error.clone(),
                st.update_state.clone(),
                st.update_check_in_progress,
                st.update_action_in_progress,
                st.pending_open_link.take(),
                st.daemon_error.clone(),
                st.settings.clone(),
                show_window,
                show_about,
                show_gpu_page,
                pending_toast,
                st.quit,
                st.render_revision,
            )
        };

        if show_window {
            win_clone.present();
        }
        if show_about {
            stack_clone.set_visible_child_name("about");
            win_clone.present();
        }
        if show_gpu_page {
            stack_clone.set_visible_child_name("gpu");
            win_clone.present();
        }
        if let Some((msg, is_error)) = pending_toast {
            let toast = adw::Toast::new(&msg);
            toast.set_timeout(if is_error { 4 } else { 2 });
            toast_overlay_clone.add_toast(toast);
        }
        if let Some(open_link) = pending_open_link {
            open_uri_with_feedback(&shared_clone, &open_link.url, &open_link.error_message);
        }

        if quit {
            app_clone.quit();
            return glib::ControlFlow::Break;
        }

        update_dashboard_flow_layout(
            dash.width(),
            &primary_metrics_grid,
            &secondary_metrics_grid,
            &current_mode_flow,
            &quick_performance_grid,
            &cockpit_middle_grid,
            &live_performance_grid,
            &cockpit_lower_grid,
            &last_dashboard_layout,
        );
        if last_rendered_revision.get() == Some(render_revision) {
            return glib::ControlFlow::Continue;
        }
        last_rendered_revision.set(Some(render_revision));

        system_health_panel.set_visible(settings.dashboard.show_system_health);
        cooling_snapshot_panel.set_visible(settings.dashboard.show_cooling_snapshot);
        dash_root.set_spacing(if settings.dashboard.compact { 12 } else { 20 });
        dash_root.set_margin_top(if settings.dashboard.compact { 12 } else { 20 });
        dash_root.set_margin_bottom(if settings.dashboard.compact { 18 } else { 28 });
        settings_health_switch.set_active(settings.dashboard.show_system_health);
        settings_nvme_switch.set_active(settings.dashboard.show_nvme);
        settings_cooling_switch.set_active(settings.dashboard.show_cooling_snapshot);
        settings_compact_switch.set_active(settings.dashboard.compact);
        close_behavior_combo.set_active_id(Some(settings.ui.close_behavior.id()));
        launch_on_login_switch.set_active(settings.ui.launch_on_login);
        start_minimized_switch.set_active(settings.ui.start_minimized_to_tray);
        remember_charge_switch.set_active(settings.controls.preferred_charge_limit.is_some());
        preferred_charge_spin.set_sensitive(settings.controls.preferred_charge_limit.is_some());
        if let Some(limit) = settings.controls.preferred_charge_limit {
            if preferred_charge_spin.value().round() as u8 != limit {
                preferred_charge_spin.set_value(limit as f64);
            }
        }
        preferred_profile_combo.set_active_id(Some(
            settings
                .controls
                .last_manual_profile
                .as_deref()
                .unwrap_or("none"),
        ));
        cpu_usage_graph.set_samples(&cpu_usage_history);
        cpu_temp_graph.set_samples(&cpu_temp_history);
        dashboard_cpu_sparkline.set_samples(&cpu_usage_history);
        live_cpu_usage_sparkline.set_samples(&cpu_usage_history);
        live_cpu_temp_sparkline.set_samples(&cpu_temp_history);
        live_gpu_temp_sparkline.set_samples(&gpu_temp_history);
        gpu_temp_graph.set_samples(&gpu_temp_history);
        gpu_usage_graph.set_samples(&gpu_usage_history);
        gpu_history_group
            .set_visible(!gpu_temp_history.is_empty() || !gpu_usage_history.is_empty());
        live_memory_sparkline.set_samples(&memory_usage_history);

        connection_status.remove_css_class("connected");
        connection_status.remove_css_class("disconnected");
        let state_received = telemetry.is_some();

        update_setup_dependency_row(
            &setup_status,
            DependencyKind::RogHelperd,
            &setup_daemon_row,
            &setup_daemon_value,
        );
        update_setup_dependency_row(
            &setup_status,
            DependencyKind::Asusd,
            &setup_asusd_row,
            &setup_asusd_value,
        );
        update_setup_dependency_row(
            &setup_status,
            DependencyKind::Supergfxd,
            &setup_supergfxd_row,
            &setup_supergfxd_value,
        );
        update_setup_dependency_row(
            &setup_status,
            DependencyKind::UPower,
            &setup_upower_row,
            &setup_upower_value,
        );
        update_setup_dependency_row(
            &setup_status,
            DependencyKind::NvidiaSmi,
            &setup_nvidia_row,
            &setup_nvidia_value,
        );
        update_setup_permission_row(
            &setup_status,
            PermissionKind::CpuPolicyWrites,
            &setup_cpu_row,
            &setup_cpu_value,
        );
        update_setup_permission_row(
            &setup_status,
            PermissionKind::KeyboardLightingWrites,
            &setup_lighting_row,
            &setup_lighting_value,
        );
        update_setup_permission_row(
            &setup_status,
            PermissionKind::FanControls,
            &setup_fans_row,
            &setup_fans_value,
        );
        update_administrator_access_rows(
            privileged_status.as_ref(),
            &cpu_caps,
            &fan_state,
            lighting.as_ref(),
            &caps,
            &administrator_access_rows,
        );
        refresh_setup_button.set_sensitive(!setup_refreshing);
        refresh_setup_button.set_label(if setup_refreshing {
            "Refreshing…"
        } else {
            "Refresh checks"
        });
        setup_advanced_buffer.set_text(&setup_status.to_report_text(true));
        if setup_status.issues.is_empty() {
            setup_issues_row.set_title("No setup issues detected");
            setup_issues_row
                .set_subtitle("All relevant live service and permission checks currently pass.");
        } else {
            setup_issues_row.set_title(&format!(
                "{} control areas need setup",
                setup_status.issue_count()
            ));
            setup_issues_row.set_subtitle(
                &setup_status
                    .issues
                    .iter()
                    .map(|issue| format!("{} — {}", issue.title, issue.guidance))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        update_setup_feature_row(
            &setup_profiles_row,
            &setup_profiles_value,
            &caps.profile_access,
        );
        update_setup_feature_row(
            &setup_charge_row,
            &setup_charge_value,
            &caps.charge_limit_access,
        );
        update_setup_feature_row(&setup_gpu_row, &setup_gpu_value, &caps.gpu_mode_access);
        update_setup_feature_row(
            &setup_kbd_support_row,
            &setup_kbd_support_value,
            &caps.kbd_backlight_access,
        );
        setup_fan_support_value.set_text(if fan_state.caps.has_individual_fan_control {
            "Available"
        } else if !fan_state.fans.is_empty() {
            "Telemetry only"
        } else {
            "Unsupported"
        });
        setup_fan_support_row.set_subtitle(if fan_state.caps.has_individual_fan_control {
            "Fan telemetry and at least one writable manual-control endpoint are available."
        } else if !fan_state.fans.is_empty() {
            "Fan RPM is available, but no writable manual-control endpoint was confirmed."
        } else {
            "No supported fan telemetry or control endpoint was detected."
        });
        if daemon_error.is_some() {
            connection_status.set_text("Daemon offline");
            connection_status.add_css_class("disconnected");
            diag_daemon_card.set_value("Offline");
            diag_daemon_card.set_status_chip(Some("Error"));
            diag_daemon_card.set_subtitle(Some(DAEMON_DBUS_NAME));
        } else if !state_received {
            connection_status.set_text("Connecting…");
            diag_daemon_card.set_value("Connecting");
            diag_daemon_card.set_status_chip(Some("Pending"));
            diag_daemon_card.set_subtitle(Some(DAEMON_DBUS_NAME));
        } else {
            connection_status.set_text("System connected");
            connection_status.add_css_class("connected");
            diag_daemon_card.set_value("Connected");
            diag_daemon_card.set_status_chip(Some("Available"));
            diag_daemon_card.set_subtitle(Some(DAEMON_DBUS_NAME));
        }
        let available_controls = [
            caps.profile_access.is_available(),
            caps.gpu_mode_access.is_available(),
            caps.charge_limit_access.is_available(),
            caps.kbd_backlight_access.is_available(),
            fan_state.caps.has_individual_fan_control,
        ]
        .into_iter()
        .filter(|available| *available)
        .count()
            + cpu_caps
                .control_access
                .iter()
                .filter(|control| control.status.is_writable())
                .count();
        if daemon_error.is_some() {
            system_card.set_status_chip(Some("Daemon disconnected"));
            set_dashboard_status(
                &health_daemon_indicator,
                &health_daemon,
                "Disconnected",
                "status-error",
            );
        } else if state_received {
            system_card.set_status_chip(Some("Daemon connected"));
            set_dashboard_status(
                &health_daemon_indicator,
                &health_daemon,
                "Connected",
                "status-ok",
            );
        } else {
            system_card.set_status_chip(Some("Connecting"));
            set_dashboard_status(
                &health_daemon_indicator,
                &health_daemon,
                "Connecting",
                "status-info",
            );
        }

        let asusd_available =
            caps.profile_access.is_available() || caps.charge_limit_access.is_available();
        let asusd_missing = [caps.profile_access.status, caps.charge_limit_access.status]
            .into_iter()
            .any(|status| status == FeatureAccessState::MissingBackend);
        if asusd_available {
            set_dashboard_status(
                &health_asusd_indicator,
                &health_asusd,
                "Available",
                "status-ok",
            );
        } else if asusd_missing {
            set_dashboard_status(
                &health_asusd_indicator,
                &health_asusd,
                "Missing",
                "status-warning",
            );
        } else {
            set_dashboard_status(
                &health_asusd_indicator,
                &health_asusd,
                "Unavailable",
                "status-info",
            );
        }
        if caps.gpu_mode_access.is_available() {
            set_dashboard_status(
                &health_supergfxd_indicator,
                &health_supergfxd,
                "Available",
                "status-ok",
            );
        } else if caps.gpu_mode_access.status == FeatureAccessState::MissingBackend {
            set_dashboard_status(
                &health_supergfxd_indicator,
                &health_supergfxd,
                "Missing",
                "status-warning",
            );
        } else {
            set_dashboard_status(
                &health_supergfxd_indicator,
                &health_supergfxd,
                "Unavailable",
                "status-info",
            );
        }

        let writable_cpu_controls = cpu_caps
            .control_access
            .iter()
            .filter(|control| control.status.is_writable())
            .count();
        if writable_cpu_controls > 0 {
            set_dashboard_status(
                &health_cpu_indicator,
                &health_cpu,
                &format!("{writable_cpu_controls} writable"),
                "status-ok",
            );
        } else {
            set_dashboard_status(
                &health_cpu_indicator,
                &health_cpu,
                "Read-only",
                "status-info",
            );
        }
        match caps.kbd_backlight_access.status {
            FeatureAccessState::Available => set_dashboard_status(
                &health_lighting_indicator,
                &health_lighting,
                "Available",
                "status-ok",
            ),
            FeatureAccessState::PermissionDenied => set_dashboard_status(
                &health_lighting_indicator,
                &health_lighting,
                "Read-only",
                "status-info",
            ),
            FeatureAccessState::MissingBackend => set_dashboard_status(
                &health_lighting_indicator,
                &health_lighting,
                "Missing backend",
                "status-warning",
            ),
            _ => set_dashboard_status(
                &health_lighting_indicator,
                &health_lighting,
                "Unavailable",
                "status-info",
            ),
        }
        if fan_state.caps.has_individual_fan_control {
            set_dashboard_status(
                &health_fans_indicator,
                &health_fans,
                "Available",
                "status-ok",
            );
        } else if !fan_state.fans.is_empty() {
            set_dashboard_status(
                &health_fans_indicator,
                &health_fans,
                "Telemetry only",
                "status-info",
            );
        } else {
            set_dashboard_status(
                &health_fans_indicator,
                &health_fans,
                "Unavailable",
                "status-info",
            );
        }
        diag_capabilities_card.set_value(available_controls.to_string());
        diag_capabilities_card.set_subtitle(Some("Capability-gated control groups"));
        diag_warnings_card.set_value(warnings.len().to_string());
        diag_warnings_card.set_subtitle(Some(if warnings.is_empty() {
            "No daemon warnings reported"
        } else {
            "Review the report below"
        }));
        diag_warnings_card.set_status_chip(Some(if warnings.is_empty() {
            "Normal"
        } else {
            "Attention"
        }));
        diag_service_daemon.set_text(if daemon_error.is_some() {
            "Unavailable"
        } else if state_received {
            "Connected"
        } else {
            "Connecting"
        });
        diag_service_asusd.set_text(&feature_value_label(&caps.profile_access));
        diag_service_supergfxd.set_text(&feature_value_label(&caps.gpu_mode_access));
        diag_capability_profiles.set_text(&feature_value_label(&caps.profile_access));
        diag_capability_gpu.set_text(&feature_value_label(&caps.gpu_mode_access));
        diag_capability_charge.set_text(&feature_value_label(&caps.charge_limit_access));
        diag_capability_lighting.set_text(&feature_value_label(&caps.kbd_backlight_access));
        diag_permission_cpu.set_text(&format!("{writable_cpu_controls} writable controls"));
        diag_permission_lighting.set_text(&feature_value_label(&caps.kbd_backlight_access));
        diag_permission_fans.set_text(if fan_state.caps.has_individual_fan_control {
            "Available"
        } else {
            "Read-only"
        });
        diag_warning_summary.set_text(if warnings.is_empty() {
            "No active warnings"
        } else if warnings.len() == 1 {
            &warnings[0]
        } else {
            "Multiple warnings · expand the raw report for details"
        });

        fan_backend_label.set_text(&format!("backend: {}", fan_state.caps.fan_backend));
        update_backend_pill_class(&fan_backend_label, &fan_state.caps.fan_backend);
        let fan_word = if fan_state.caps.fan_count == 1 {
            "fan"
        } else {
            "fans"
        };
        fan_count_label.set_text(&format!("{} {fan_word} detected", fan_state.caps.fan_count));
        fan_mode_label.set_text(fan_mode_label_text(fan_state.mode));
        mode_profile.set_text(profile.as_deref().unwrap_or("—"));
        mode_gpu.set_text(gpu_mode.as_deref().unwrap_or("—"));
        mode_cooling.set_text(match fan_state.mode {
            FanControlMode::Auto => "BIOS Auto",
            FanControlMode::Unsupported => "—",
            mode => fan_mode_label_text(mode),
        });
        let mapping_uncertain =
            fan_state.caps.fan_mapping_confidence != FanMappingConfidence::HardwareLabel;
        fan_mapping_label.set_text(if mapping_uncertain {
            "Mapping uncertain"
        } else {
            "Mapping OK"
        });
        fan_mapping_label.set_visible(mapping_uncertain);
        fan_last_action_label.set_text(
            fan_state
                .last_action
                .as_deref()
                .unwrap_or("No fan action applied in this session."),
        );
        fan_sync_switch.set_sensitive(fan_state.caps.has_fan_sync_control);
        if fan_sync_switch.is_active() != fan_state.sync_enabled {
            fan_sync_switch.set_active(fan_state.sync_enabled);
        }
        let manual_available = fan_state.caps.has_fan_manual_percent;
        let fan_authorization_required = fan_state
            .fans
            .iter()
            .any(|fan| fan.access_state == "authorization_required");
        fan_manual_scale.set_sensitive(manual_available);
        fan_manual_apply.set_sensitive(manual_available);
        fan_manual_apply.set_label(if fan_authorization_required {
            "Unlock & Apply"
        } else {
            "Apply"
        });
        fan_boost_5.set_sensitive(fan_state.caps.has_fan_boost);
        fan_boost_10.set_sensitive(fan_state.caps.has_fan_boost);
        fan_boost_15.set_sensitive(fan_state.caps.has_fan_boost);
        fan_stop_boost.set_sensitive(matches!(fan_state.mode, FanControlMode::FullSpeedBoost));
        fan_auto_button.set_sensitive(fan_state.caps.has_individual_fan_control);
        fan_mode_combo.set_sensitive(
            fan_state.caps.has_fan_manual_percent
                || fan_state.caps.fan_curve_writable
                || fan_state.caps.has_fan_boost,
        );
        controls_hint.set_text(&fan_controls_hint(&fan_state));
        curve_preview.set_enabled(fan_state.caps.fan_curve_writable);
        curve_apply.set_sensitive(fan_state.caps.fan_curve_writable);
        curve_apply.set_label(if fan_authorization_required {
            "Unlock & Apply curve"
        } else {
            "Apply curve"
        });
        curve_reset.set_sensitive(fan_state.caps.fan_curve_writable);
        curve_status.set_text(if fan_authorization_required {
            "Administrator access required. Authentication starts only when Apply is used."
        } else if fan_state.caps.fan_curve_writable {
            "Curve editing is available for the active backend."
        } else if fan_state.caps.has_fan_curves {
            "Fan curves are supported, but the privileged helper is unavailable."
        } else {
            "Fan curve editing is not available on this backend."
        });
        fan_sync_row.set_subtitle(&fan_sync_subtitle(&fan_state));
        fan_manual_row.set_subtitle(&fan_manual_subtitle(&fan_state));
        fan_boost_row.set_subtitle(&fan_boost_subtitle(&fan_state));
        fan_mode_row.set_subtitle(fan_mode_label_text(fan_state.mode));
        fans_banner.set_title(&fan_banner_title(&fan_state));
        fans_banner.set_revealed(
            !fan_state.caps.has_individual_fan_control || !fan_state.caps.warnings.is_empty(),
        );
        update_fan_visual_slots(&hero_fan_slots, &fan_state.fans);
        update_fan_card_slots(&fan_card_slots, &fan_state.fans);
        update_diagnostics_buffer(
            &fan_cards_buffer,
            &fan_cards_scroll,
            &fan_state_diagnostics_text(&fan_state),
        );

        if let Some(t) = telemetry {
            diag_service_nvidia.set_text(
                t.gpu_telemetry_status
                    .as_deref()
                    .unwrap_or("Optional provider unavailable"),
            );
            diag_sensor_cpu.set_text(if t.cpu_temp_c.is_some() {
                "Reporting"
            } else {
                "Unavailable"
            });
            diag_sensor_gpu.set_text(if t.gpu_temp_c.is_some() {
                "Reporting"
            } else {
                "Unavailable"
            });
            diag_sensor_fans.set_text(if t.fan_rows.iter().any(|fan| fan.rpm.is_some()) {
                "Reporting"
            } else {
                "Unavailable"
            });
            diag_sensor_battery.set_text(if t.battery_percent.is_some() {
                "Reporting"
            } else {
                "Unavailable"
            });
            if let Some(v) = t.cpu_temp_c {
                cpu_card.set_value(format!("{v:.1}"));
                cpu_card.set_unit(Some("°C"));
                cpu_temp_gauge.set_temp(Some(v));
                live_cpu_temp_card.set_value(format!("{v:.1}"));
                live_cpu_temp_card.set_unit(Some("°C"));
                thermal_cpu.set_text(&format!("CPU  {v:.0}°C"));
            } else {
                cpu_card.set_value("--");
                cpu_card.set_unit(None);
                cpu_temp_gauge.set_temp(None);
                live_cpu_temp_card.set_value("—");
                live_cpu_temp_card.set_unit(None);
                thermal_cpu.set_text("CPU  —");
            }
            cpu_card.set_subtitle(cpu.as_ref().map(cpu_dashboard_summary).as_deref());

            if let Some(v) = t.gpu_temp_c {
                gpu_card.set_value(format!("{v:.1}"));
                gpu_card.set_unit(Some("°C"));
                gpu_temp_gauge.set_temp(Some(v));
                gpu_page_temp_card.set_value(format!("{v:.1}"));
                gpu_page_temp_card.set_unit(Some("°C"));
                gpu_page_temp_card.set_status_chip(Some("Telemetry available"));
                gpu_card.set_status_chip(Some("Telemetry available"));
                live_gpu_temp_card.set_value(format!("{v:.1}"));
                live_gpu_temp_card.set_unit(Some("°C"));
                thermal_gpu.set_text(&format!("GPU  {v:.0}°C"));
            } else {
                gpu_card.set_value("--");
                gpu_card.set_unit(None);
                gpu_temp_gauge.set_temp(None);
                gpu_page_temp_card.set_value("—");
                gpu_page_temp_card.set_unit(None);
                gpu_page_temp_card.set_status_chip(Some("Telemetry unavailable"));
                gpu_card.set_status_chip(Some("Telemetry unavailable"));
                live_gpu_temp_card.set_value("—");
                live_gpu_temp_card.set_unit(None);
                thermal_gpu.set_text("GPU  —");
            }
            gpu_card.set_subtitle(Some(&gpu_dashboard_summary(&t, gpu_mode.as_deref())));
            if let Some(value) = t.gpu_usage_percent {
                gpu_page_usage_card.set_value(format!("{value:.0}"));
                gpu_page_usage_card.set_unit(Some("%"));
                gpu_page_usage_card.set_status_chip(Some("Reporting"));
            } else {
                gpu_page_usage_card.set_value("—");
                gpu_page_usage_card.set_unit(None);
                gpu_page_usage_card.set_status_chip(Some("Unavailable"));
            }
            match (t.gpu_vram_used_bytes, t.gpu_vram_total_bytes) {
                (Some(used), Some(total)) if total > 0 => {
                    gpu_page_vram_card.set_value(format!(
                        "{:.1} / {:.1}",
                        used as f64 / 1024_f64.powi(3),
                        total as f64 / 1024_f64.powi(3)
                    ));
                    gpu_page_vram_card.set_unit(Some("GiB"));
                    gpu_page_vram_card.set_status_chip(Some("Reporting"));
                }
                _ => {
                    gpu_page_vram_card.set_value("—");
                    gpu_page_vram_card.set_unit(None);
                    gpu_page_vram_card.set_status_chip(Some("Unavailable"));
                }
            }
            if let Some(value) = t.gpu_power_w {
                gpu_page_power_card.set_value(format!("{value:.1}"));
                gpu_page_power_card.set_unit(Some("W"));
                gpu_page_power_card.set_status_chip(Some("Reporting"));
            } else {
                gpu_page_power_card.set_value("—");
                gpu_page_power_card.set_unit(None);
                gpu_page_power_card.set_status_chip(Some("Unavailable"));
            }
            gpu_page_core_clock.set_text(
                &t.gpu_core_clock_mhz
                    .map(|value| format!("{value} MHz"))
                    .unwrap_or_else(|| "Unavailable".to_string()),
            );
            gpu_page_memory_clock.set_text(
                &t.gpu_memory_clock_mhz
                    .map(|value| format!("{value} MHz"))
                    .unwrap_or_else(|| "Unavailable".to_string()),
            );
            gpu_page_telemetry_provider.set_text(
                t.gpu_telemetry_status
                    .as_deref()
                    .unwrap_or("NVIDIA telemetry is optional and currently unavailable."),
            );
            gpu_temp_gauge.set_speed_metrics(
                "Core Clock",
                t.gpu_core_clock_mhz.map(u64::from),
                Some("Memory Clock"),
                t.gpu_memory_clock_mhz.map(u64::from),
            );

            if let Some(v) = t.battery_percent {
                batt_card.set_value(format!("{v:.0}"));
                batt_card.set_unit(Some("%"));
            } else {
                batt_card.set_value("--");
                batt_card.set_unit(None);
            }
            let battery_chip = t.battery_state.map(|s| match s {
                BatteryState::Unknown => "Unknown".to_string(),
                BatteryState::Charging => "Charging".to_string(),
                BatteryState::Discharging => "Discharging".to_string(),
                BatteryState::Empty => "Empty".to_string(),
                BatteryState::Full => "Full".to_string(),
                BatteryState::PendingCharge => "Pending Charge".to_string(),
                BatteryState::PendingDischarge => "Pending Discharge".to_string(),
                BatteryState::NotCharging => "Not Charging".to_string(),
            });
            mode_power.set_text(match t.power_source {
                Some(PowerSource::Ac) => "AC",
                Some(PowerSource::Battery) => "Battery",
                None => "—",
            });
            mode_battery.set_text(battery_chip.as_deref().unwrap_or("—"));
            batt_card.set_status_chip(battery_chip.as_deref());
            battery_hero_card.set_status_chip(battery_chip.as_deref());
            if let Some(v) = t.battery_percent {
                battery_hero_card.set_value(format!("{v:.0}"));
                battery_hero_card.set_unit(Some("%"));
                battery_progress.set_fraction((v as f64 / 100.0).clamp(0.0, 1.0));
            } else {
                battery_hero_card.set_value("—");
                battery_hero_card.set_unit(None);
                battery_progress.set_fraction(0.0);
            }
            let battery_time = match t.battery_state {
                Some(BatteryState::Charging) => t
                    .battery_time_to_full_s
                    .map(|seconds| format!("{} until full", format_duration_short(seconds))),
                Some(BatteryState::Discharging) => t
                    .battery_time_to_empty_s
                    .map(|seconds| format!("{} remaining", format_duration_short(seconds))),
                _ => None,
            };
            let battery_source = match t.power_source {
                Some(PowerSource::Ac) if t.ac_online == Some(true) => "AC connected",
                Some(PowerSource::Ac) => "AC power",
                Some(PowerSource::Battery) => "Battery power",
                None => "Source unavailable",
            };
            let battery_hero_subtitle = battery_time
                .as_deref()
                .map(|time| format!("{battery_source} · {time}"))
                .unwrap_or_else(|| battery_source.to_string());
            battery_hero_card.set_subtitle(Some(&battery_hero_subtitle));

            let mut battery_summary = Vec::new();
            if let Some(state) = battery_chip.as_deref() {
                battery_summary.push(state.to_string());
            }
            if let Some(health) = t.battery_health_percent {
                battery_summary.push(format!("{health:.0}% health"));
            }
            if let Some(time) = battery_time.as_deref() {
                battery_summary.push(time.to_string());
            }
            let battery_summary =
                (!battery_summary.is_empty()).then(|| battery_summary.join(" · "));
            batt_card.set_subtitle(battery_summary.as_deref());

            system_card.set_value(match t.power_source {
                Some(PowerSource::Ac) => "AC Power",
                Some(PowerSource::Battery) => "Battery",
                None => "System",
            });
            system_card.set_unit(None);
            let system_battery = match (t.battery_percent, battery_chip.as_deref()) {
                (Some(percent), Some(state)) => format!("Battery {percent:.0}% · {state}"),
                (Some(percent), None) => format!("Battery {percent:.0}%"),
                (None, Some(state)) => format!("Battery · {state}"),
                (None, None) => "Battery state unavailable".to_string(),
            };
            system_card.set_subtitle(Some(&system_battery));

            power_card.set_value(match t.power_source {
                Some(PowerSource::Ac) => "AC".to_string(),
                Some(PowerSource::Battery) => "Battery".to_string(),
                None => "--".to_string(),
            });
            power_card.set_unit(None);
            let power_summary = match (t.power_source, t.battery_charge_power_w) {
                (Some(PowerSource::Ac), Some(watts)) => {
                    // This is the daemon's measured battery charge input, not an inferred draw.
                    format!("{watts:.1} W input")
                }
                (Some(PowerSource::Ac), None) => "Connected".to_string(),
                (Some(PowerSource::Battery), _) => "Discharging".to_string(),
                (None, _) => "Source unavailable".to_string(),
            };
            power_card.set_subtitle(Some(&power_summary));

            if let Some(percent) = t.mem_used_percent {
                memory_card.set_value(format!("{percent:.0}"));
                memory_card.set_unit(Some("%"));
                memory_card.set_subtitle(Some(&format!(
                    "{} / {}",
                    format_bytes_gib_opt(t.mem_used_bytes),
                    format_bytes_gib_opt(t.mem_total_bytes)
                )));
                memory_hero_card.set_value(format!("{percent:.0}"));
                memory_hero_card.set_unit(Some("% used"));
                memory_hero_card.set_subtitle(Some(&format!(
                    "{} used / {} total",
                    format_bytes_gib_opt(t.mem_used_bytes),
                    format_bytes_gib_opt(t.mem_total_bytes),
                )));
                memory_progress.set_fraction((percent as f64 / 100.0).clamp(0.0, 1.0));
                live_memory_card.set_value(format!("{percent:.0}"));
                live_memory_card.set_unit(Some("%"));
            } else {
                memory_card.set_value("—");
                memory_card.set_unit(None);
                memory_card.set_subtitle(Some("Not reported"));
                memory_hero_card.set_value("—");
                memory_hero_card.set_unit(None);
                memory_hero_card.set_subtitle(Some("Memory telemetry unavailable"));
                memory_progress.set_fraction(0.0);
                live_memory_card.set_value("—");
                live_memory_card.set_unit(None);
            }

            if t.fan_rows.is_empty() {
                fans_card.set_value("--");
                fans_card.set_unit(None);
                fans_card.set_subtitle(Some("No fan RPM sensors exposed"));
            } else {
                let reporting_count = t.fan_rows.iter().filter(|fan| fan.rpm.is_some()).count();
                let max_rpm = t.fan_rows.iter().filter_map(|fan| fan.rpm).max();
                if let Some(max_rpm) = max_rpm {
                    fans_card.set_value(max_rpm.to_string());
                    fans_card.set_unit(Some("RPM"));
                } else {
                    fans_card.set_value("--");
                    fans_card.set_unit(None);
                }
                let count = t.fan_rows.len();
                let fan_word = if count == 1 { "fan" } else { "fans" };
                if reporting_count == count {
                    fans_card.set_subtitle(Some(&format!("{count} {fan_word} reporting")));
                } else {
                    fans_card.set_subtitle(Some(&format!(
                        "{reporting_count} of {count} {fan_word} reporting"
                    )));
                }
            }
            cooling_empty_hint.set_visible(t.fan_rows.is_empty());
            for (index, (row, title, rpm, state)) in dashboard_fan_rows.iter().enumerate() {
                if let Some(fan) = t.fan_rows.get(index) {
                    title.set_text(&fan.display_label);
                    if let Some(value) = fan.rpm {
                        rpm.set_text(&format!("{value} RPM"));
                        set_dashboard_status(state, state, "●", "status-ok");
                    } else {
                        rpm.set_text("Unavailable");
                        set_dashboard_status(state, state, "●", "status-info");
                    }
                    row.set_visible(true);
                } else {
                    row.set_visible(false);
                }
            }

            let nvme_temp = t
                .temps_c
                .iter()
                .filter_map(|(name, value)| {
                    if name.to_ascii_lowercase().contains("nvme") {
                        Some(*value)
                    } else {
                        None
                    }
                })
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            if let Some(v) = nvme_temp {
                nvme_card.widget().set_visible(settings.dashboard.show_nvme);
                nvme_card.set_value(format!("{v:.1}"));
                nvme_card.set_unit(Some("°C"));
                nvme_card.set_subtitle(Some("Temperature"));
                thermal_nvme.set_text(&format!("NVMe  {v:.0}°C"));
            } else {
                nvme_card.widget().set_visible(false);
                nvme_card.set_value("--");
                nvme_card.set_unit(None);
                thermal_nvme.set_text("NVMe  —");
            }

            battery_health_card.set_value(
                t.battery_health_percent
                    .map(|v| format!("{v:.0}"))
                    .unwrap_or_else(|| "—".to_string()),
            );
            battery_health_card.set_unit(t.battery_health_percent.map(|_| "%"));
            battery_cycles_card.set_value(
                t.battery_cycle_count
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "—".to_string()),
            );
            let active_power = match t.battery_state {
                Some(BatteryState::Charging) => t.battery_charge_power_w,
                _ => t.battery_discharge_power_w.or(t.battery_charge_power_w),
            };
            battery_power_card.set_value(
                active_power
                    .map(|v| format!("{v:.1}"))
                    .unwrap_or_else(|| "—".to_string()),
            );
            battery_power_card.set_unit(active_power.map(|_| "W"));
            battery_time_card.set_value(
                battery_time
                    .as_deref()
                    .and_then(|value| value.split_whitespace().next())
                    .unwrap_or("—"),
            );
            battery_time_card.set_subtitle(battery_time.as_deref());
            battery_source_card.set_value(match t.power_source {
                Some(PowerSource::Ac) => "AC",
                Some(PowerSource::Battery) => "Battery",
                None => "—",
            });
            battery_source_card.set_status_chip(t.ac_online.map(|online| {
                if online {
                    "Connected"
                } else {
                    "On battery"
                }
            }));

            // RAM.
            ram_total.set_text(&format_bytes_gib_opt(t.mem_total_bytes));
            ram_used.set_text(&format_used_ram(t.mem_used_bytes, t.mem_used_percent));
            ram_avail.set_text(&format_bytes_gib_opt(t.mem_available_bytes));
            ram_free.set_text(&format_bytes_gib_opt(t.mem_free_bytes));
            ram_cached.set_text(&format_bytes_gib_opt(t.mem_cached_bytes));
            ram_buffers.set_text(&format_bytes_gib_opt(t.mem_buffers_bytes));
            ram_shared.set_text(&format_bytes_gib_opt(t.mem_shared_bytes));
            ram_anon.set_text(&format_bytes_gib_opt(t.mem_anon_bytes));
            memory_cache_card.set_value(format_bytes_gib_opt(t.mem_cached_bytes));
            memory_shared_card.set_value(format_bytes_gib_opt(t.mem_shared_bytes));
            memory_buffers_card.set_value(format_bytes_gib_opt(t.mem_buffers_bytes));
            memory_swap_card.set_value(format_bytes_gib_opt(t.swap_used_bytes));
            memory_swap_card.set_subtitle(Some(&format!(
                "of {}",
                format_bytes_gib_opt(t.swap_total_bytes)
            )));

            ram_swap_total.set_text(&format_bytes_gib_opt(t.swap_total_bytes));
            ram_swap_used.set_text(&format_bytes_gib_opt(t.swap_used_bytes));
            ram_swap_free.set_text(&format_bytes_gib_opt(t.swap_free_bytes));
            ram_swap_in.set_text(&format_rate_pages_opt(t.swap_in_pages_per_s));
            ram_swap_out.set_text(&format_rate_pages_opt(t.swap_out_pages_per_s));
            ram_zram.set_text(&format_zram_opt(t.zram_used_bytes, t.zram_total_bytes));
            ram_zswap.set_text(&format_bool_opt(t.zswap_enabled));

            ram_psi_some.set_text(&format_psi_opt(
                t.psi_mem_some_avg10,
                t.psi_mem_some_avg60,
                t.psi_mem_some_avg300,
            ));
            ram_psi_full.set_text(&format_psi_opt(
                t.psi_mem_full_avg10,
                t.psi_mem_full_avg60,
                t.psi_mem_full_avg300,
            ));

            ram_active.set_text(&format_bytes_gib_opt(t.mem_active_bytes));
            ram_inactive.set_text(&format_bytes_gib_opt(t.mem_inactive_bytes));
            ram_slab.set_text(&format_bytes_gib_opt(t.mem_slab_bytes));
            ram_sreclaim.set_text(&format_bytes_gib_opt(t.mem_sreclaimable_bytes));
            ram_sunreclaim.set_text(&format_bytes_gib_opt(t.mem_sunreclaim_bytes));
            ram_mapped.set_text(&format_bytes_gib_opt(t.mem_mapped_bytes));
            ram_dirty.set_text(&format_bytes_mib_opt(t.mem_dirty_bytes));
            ram_writeback.set_text(&format_bytes_mib_opt(t.mem_writeback_bytes));
            ram_pagetables.set_text(&format_bytes_mib_opt(t.mem_pagetables_bytes));
            ram_kernelstack.set_text(&format_bytes_mib_opt(t.mem_kernelstack_bytes));

            let rows = t.mem_top_processes_rows.as_deref().unwrap_or_default();
            for (idx, (row, labels)) in ram_top_rows.iter().enumerate() {
                if let Some(p) = rows.get(idx) {
                    labels[0].set_text(&p.name);
                    labels[0].set_tooltip_text(Some(&p.name));
                    labels[1].set_text(&p.pid.to_string());
                    labels[2].set_text(&p.user);
                    labels[3].set_text(&format_bytes_human(p.rss_bytes));
                    labels[4].set_text(&format_bytes_human(p.swap_bytes));
                    row.set_visible(true);
                } else if idx == 0 {
                    labels[0].set_text("(n/a)");
                    for label in labels.iter().skip(1) {
                        label.set_text("");
                    }
                    row.set_visible(true);
                } else {
                    row.set_visible(false);
                }
            }

            let mut temp_entries: Vec<(String, String)> = t
                .temps_c
                .iter()
                .map(|(k, v)| (k.clone(), format!("{v:.1} °C")))
                .collect();
            temp_entries.sort_by(|a, b| a.0.cmp(&b.0));
            update_detail_rows(&detail_temp_rows, &temp_entries);
            details_temp_group.set_visible(!temp_entries.is_empty());

            let fan_entries: Vec<(String, String)> = if t.fan_rows.is_empty() {
                vec![(
                    "Fan telemetry".to_string(),
                    "Unavailable on this platform".to_string(),
                )]
            } else {
                t.fan_rows
                    .iter()
                    .map(|fan| {
                        (
                            fan.display_label.clone(),
                            fan.rpm
                                .map(|rpm| format!("{rpm} rpm"))
                                .unwrap_or_else(|| "(unavailable)".to_string()),
                        )
                    })
                    .collect()
            };
            update_detail_rows_dynamic(
                &details_fan_group_ref,
                &mut detail_fan_rows_ref.borrow_mut(),
                &fan_entries,
            );
            details_fan_group_ref.set_visible(true);
        } else {
            diag_service_nvidia.set_text("Waiting for telemetry");
            diag_sensor_cpu.set_text("Waiting for telemetry");
            diag_sensor_gpu.set_text("Waiting for telemetry");
            diag_sensor_fans.set_text("Waiting for telemetry");
            diag_sensor_battery.set_text("Waiting for telemetry");
            gpu_temp_gauge.set_temp(None);
            gpu_temp_gauge.set_speed_metrics("Core Clock", None, Some("Memory Clock"), None);
            cpu_card.set_value("—");
            cpu_card.set_unit(None);
            cpu_card.set_subtitle(Some("Waiting for telemetry"));
            gpu_card.set_value("—");
            gpu_card.set_unit(None);
            gpu_card.set_subtitle(Some("Waiting for telemetry"));
            gpu_card.set_status_chip(Some("Pending"));
            batt_card.set_value("—");
            batt_card.set_unit(None);
            batt_card.set_subtitle(Some("Waiting for telemetry"));
            power_card.set_value("—");
            power_card.set_subtitle(Some("Waiting for telemetry"));
            fans_card.set_value("—");
            fans_card.set_unit(None);
            fans_card.set_subtitle(Some("Waiting for telemetry"));
            nvme_card.widget().set_visible(false);
            mode_power.set_text("—");
            mode_battery.set_text("—");
            system_card.set_value("Connecting");
            system_card.set_subtitle(Some("Waiting for system state"));
            thermal_cpu.set_text("CPU  —");
            thermal_gpu.set_text("GPU  —");
            thermal_nvme.set_text("NVMe  —");
            cooling_empty_hint.set_visible(true);
            for (row, _, _, _) in &dashboard_fan_rows {
                row.set_visible(false);
            }
            gpu_page_temp_card.set_value("—");
            gpu_page_temp_card.set_unit(None);
            for card in [
                &gpu_page_usage_card,
                &gpu_page_vram_card,
                &gpu_page_power_card,
            ] {
                card.set_value("—");
                card.set_unit(None);
                card.set_status_chip(Some("Pending"));
            }
            gpu_page_core_clock.set_text("Unavailable");
            gpu_page_memory_clock.set_text("Unavailable");
            gpu_page_telemetry_provider.set_text("Waiting for daemon telemetry...");
            battery_hero_card.set_value("—");
            battery_hero_card.set_unit(None);
            battery_hero_card.set_subtitle(Some("Battery telemetry unavailable"));
            battery_progress.set_fraction(0.0);
            memory_card.set_value("—");
            memory_card.set_unit(None);
            memory_hero_card.set_value("—");
            memory_hero_card.set_unit(None);
            memory_hero_card.set_subtitle(Some("Memory telemetry unavailable"));
            memory_progress.set_fraction(0.0);
            live_cpu_temp_card.set_value("—");
            live_cpu_temp_card.set_unit(None);
            live_gpu_temp_card.set_value("—");
            live_gpu_temp_card.set_unit(None);
            live_memory_card.set_value("—");
            live_memory_card.set_unit(None);
        }

        // CPU page + cards.
        if let Some(cpu_data) = cpu.as_ref() {
            if let Some(v) = cpu_data.temp_c {
                cpu_card_temp.set_value(format!("{v:.1}"));
                cpu_card_temp.set_unit(Some("°C"));
            } else {
                cpu_card_temp.set_value("--");
                cpu_card_temp.set_unit(None);
            }
            if let Some(v) = cpu_data.usage_percent {
                cpu_card_usage.set_value(format!("{v:.0}"));
                cpu_card_usage.set_unit(Some("%"));
                live_cpu_usage_card.set_value(format!("{v:.0}"));
                live_cpu_usage_card.set_unit(Some("%"));
            } else {
                cpu_card_usage.set_value("--");
                cpu_card_usage.set_unit(None);
                live_cpu_usage_card.set_value("—");
                live_cpu_usage_card.set_unit(None);
            }
            if let Some(v) = cpu_data.package_power_w {
                cpu_card_power.widget().set_visible(true);
                cpu_card_power.set_value(format!("{v:.1}"));
                cpu_card_power.set_unit(Some("W"));
            } else {
                cpu_card_power
                    .widget()
                    .set_visible(cpu_caps.has_package_power);
                cpu_card_power.set_value("--");
                cpu_card_power.set_unit(None);
            }
            if let Some(v) = cpu_data.avg_freq_mhz {
                cpu_card_clock.set_value(format!("{:.2}", v / 1000.0));
                cpu_card_clock.set_unit(Some("GHz"));
            } else {
                cpu_card_clock.set_value("--");
                cpu_card_clock.set_unit(None);
            }
            cpu_temp_gauge.set_speed_metrics(
                "Avg Clock",
                cpu_data.avg_freq_mhz.map(|mhz| mhz.round().max(0.0) as u64),
                None,
                None,
            );
            cpu_card_temp.set_status_chip(cpu_data.status.as_deref());
            cpu_card.set_status_chip(cpu_data.status.as_deref().or(Some("Telemetry available")));

            cpu_scaling_driver
                .set_text(cpu_caps.scaling_driver.as_deref().unwrap_or("Not detected"));
            cpu_cpu_count.set_text(&cpu_caps.cpu_count.to_string());
            cpu_thread_count.set_text(&cpu_caps.thread_count.to_string());

            let turbo_writable = cpu_caps.control_actionable(CpuControlKind::Boost);
            let power_mode_writable = cpu_caps.control_actionable(CpuControlKind::PowerMode);
            let governor_writable = cpu_caps.control_actionable(CpuControlKind::Governor);
            let epp_writable = cpu_caps.control_actionable(CpuControlKind::Epp);
            let freq_limits_writable = cpu_caps.control_actionable(CpuControlKind::FreqLimits);
            let core_online_writable = cpu_caps.control_actionable(CpuControlKind::CoreOnline);
            let quick_sensitive = turbo_writable || power_mode_writable || freq_limits_writable;
            let turbo_available_text = match cpu_data.turbo_boost_enabled {
                Some(true) => "Currently enabled. Toggle to stage a change.".to_string(),
                Some(false) => "Currently disabled. Toggle to stage a change.".to_string(),
                None => "Toggle turbo boost and apply when ready.".to_string(),
            };
            let power_preset_subtitle = cpu_power_preset_summary(cpu_data);
            let min_limit_subtitle = format!(
                "Current minimum: {}. Drag to stage a new limit.",
                format_cpu_freq_value(cpu_data.min_freq_mhz)
            );
            let max_limit_subtitle = format!(
                "Current maximum: {}. Drag to stage a new limit.",
                format_cpu_freq_value(cpu_data.max_freq_mhz)
            );
            let governor_subtitle = if let Some(governor) = cpu_data.governor.as_deref() {
                format!("Current: {governor}. Choose a governor to apply.")
            } else {
                "Choose the active CPU governor.".to_string()
            };
            let epp_subtitle = if let Some(epp) = cpu_data.epp.as_deref() {
                format!("Current: {epp}. Choose an EPP value to apply.")
            } else {
                "Choose the energy performance preference.".to_string()
            };

            cpu_turbo_switch.set_sensitive(turbo_writable);
            if let Some(v) = cpu_data.turbo_boost_enabled {
                if !cpu_turbo_switch.has_focus() {
                    cpu_turbo_switch.set_active(v);
                }
            }
            cpu_turbo_row.set_subtitle(&cpu_control_subtitle(
                &cpu_caps,
                CpuControlKind::Boost,
                &turbo_available_text,
                "Turbo boost control is not supported on this system.",
            ));

            cpu_power_quiet.set_sensitive(power_mode_writable);
            cpu_power_balanced.set_sensitive(power_mode_writable);
            cpu_power_perf.set_sensitive(power_mode_writable);
            cpu_apply_quick.set_sensitive(quick_sensitive);
            cpu_apply_quick.set_label(
                if cpu_controls_require_authorization(
                    &cpu_caps,
                    &[
                        CpuControlKind::Boost,
                        CpuControlKind::PowerMode,
                        CpuControlKind::FreqLimits,
                    ],
                ) {
                    "Unlock & Apply"
                } else {
                    "Apply"
                },
            );
            cpu_power_row.set_subtitle(&cpu_control_subtitle(
                &cpu_caps,
                CpuControlKind::PowerMode,
                &power_preset_subtitle,
                "Power mode presets are not supported on this system.",
            ));
            cpu_min_scale.set_sensitive(freq_limits_writable && cpu_caps.has_min_freq_limit);
            cpu_max_scale.set_sensitive(freq_limits_writable && cpu_caps.has_max_freq_limit);
            cpu_min_row.set_subtitle(&cpu_control_subtitle(
                &cpu_caps,
                CpuControlKind::FreqLimits,
                &min_limit_subtitle,
                "Minimum frequency limits are not supported on this system.",
            ));
            cpu_max_row.set_subtitle(&cpu_control_subtitle(
                &cpu_caps,
                CpuControlKind::FreqLimits,
                &max_limit_subtitle,
                "Maximum frequency limits are not supported on this system.",
            ));
            cpu_apply_row.set_subtitle(&cpu_quick_apply_subtitle(&cpu_caps));
            if let Some(v) = cpu_data.min_freq_mhz {
                if !cpu_min_scale.has_focus() {
                    cpu_min_scale.set_value(v as f64);
                }
            }
            if let Some(v) = cpu_data.max_freq_mhz {
                if !cpu_max_scale.has_focus() {
                    cpu_max_scale.set_value(v as f64);
                }
            }

            let power_mode_hint = cpu_data
                .epp
                .as_deref()
                .map(normalize_label)
                .unwrap_or_default();
            if power_mode_hint.contains("power") {
                cpu_power_quiet.set_active(true);
            } else if power_mode_hint.contains("perform") {
                cpu_power_perf.set_active(true);
            } else {
                cpu_power_balanced.set_active(true);
            }

            cpu_governor_combo.set_sensitive(governor_writable && cpu_caps.has_governor);
            cpu_epp_combo.set_sensitive(epp_writable && cpu_caps.has_epp);
            cpu_governor_row.set_subtitle(&cpu_control_subtitle(
                &cpu_caps,
                CpuControlKind::Governor,
                &governor_subtitle,
                "Governor control is not supported on this system.",
            ));
            cpu_epp_row.set_subtitle(&cpu_control_subtitle(
                &cpu_caps,
                CpuControlKind::Epp,
                &epp_subtitle,
                "Energy Performance Preference is not supported on this system.",
            ));
            cpu_apply_policy.set_sensitive(governor_writable || epp_writable);
            cpu_apply_policy.set_label(
                if cpu_controls_require_authorization(
                    &cpu_caps,
                    &[CpuControlKind::Governor, CpuControlKind::Epp],
                ) {
                    "Unlock & Apply"
                } else {
                    "Apply"
                },
            );
            cpu_apply_policy_row.set_subtitle(&cpu_policy_apply_subtitle(&cpu_caps));

            {
                let mut last = last_cpu_governors.borrow_mut();
                if *last != cpu_caps.governor_choices {
                    cpu_governor_combo.remove_all();
                    for gov in &cpu_caps.governor_choices {
                        cpu_governor_combo.append(Some(gov), gov);
                    }
                    *last = cpu_caps.governor_choices.clone();
                }
            }
            {
                let mut last = last_cpu_epp.borrow_mut();
                if *last != cpu_caps.epp_choices {
                    cpu_epp_combo.remove_all();
                    for epp in &cpu_caps.epp_choices {
                        cpu_epp_combo.append(Some(epp), epp);
                    }
                    *last = cpu_caps.epp_choices.clone();
                }
            }
            if let Some(ref gov) = cpu_data.governor {
                if !cpu_governor_combo.has_focus() {
                    cpu_governor_combo.set_active_id(Some(gov));
                }
            }
            if let Some(ref epp) = cpu_data.epp {
                if !cpu_epp_combo.has_focus() {
                    cpu_epp_combo.set_active_id(Some(epp));
                }
            }

            cpu_read_only_banner.set_revealed(cpu_has_access_issue(&cpu_caps));
            cpu_read_only_banner.set_title(&cpu_banner_title(&cpu_caps));

            let mut table = String::new();
            table
                .push_str("CPU  PCORE  THR   POL   ONLINE  USAGE%  CUR(GHz)  MIN(GHz)  MAX(GHz)\n");
            table.push_str(
                "------------------------------------------------------------------------\n",
            );
            for core in &cpu_data.per_core {
                let thread_label = format_cpu_thread_position(core.thread_index, core.thread_count);
                table.push_str(&format!(
                    "{:<3}  {:<5}  {:<4}  {:<4}  {:<6}  {:>6}  {:>8}  {:>8}  {:>8}\n",
                    core.logical_cpu_id,
                    core.physical_core_index
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    thread_label,
                    core.policy_id
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    if core.online { "yes" } else { "no" },
                    core.usage_percent
                        .map(|v| format!("{v:.1}"))
                        .unwrap_or_else(|| "-".to_string()),
                    core.current_freq_mhz
                        .map(|v| format!("{:.2}", v as f32 / 1000.0))
                        .unwrap_or_else(|| "-".to_string()),
                    core.min_freq_mhz
                        .map(|v| format!("{:.2}", v as f32 / 1000.0))
                        .unwrap_or_else(|| "-".to_string()),
                    core.max_freq_mhz
                        .map(|v| format!("{:.2}", v as f32 / 1000.0))
                        .unwrap_or_else(|| "-".to_string()),
                ));
            }
            cpu_per_core_buffer.set_text(&table);

            while let Some(child) = cpu_core_toggle_box.first_child() {
                cpu_core_toggle_box.remove(&child);
            }
            for core in &cpu_data.per_core {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                row.add_css_class("cpu-toggle-row");
                let label_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
                label_box.set_hexpand(true);

                let title = gtk::Label::new(Some(&format!("Logical CPU {}", core.logical_cpu_id)));
                title.set_xalign(0.0);
                title.add_css_class("cpu-toggle-title");

                let subtitle = gtk::Label::new(Some(&cpu_toggle_row_subtitle(
                    core,
                    &cpu_caps,
                    core_online_writable,
                )));
                subtitle.set_xalign(0.0);
                subtitle.set_wrap(true);
                subtitle.add_css_class("dim-label");
                subtitle.add_css_class("cpu-toggle-subtitle");
                subtitle.set_hexpand(true);

                label_box.append(&title);
                label_box.append(&subtitle);

                let switch = gtk::Switch::new();
                switch.set_active(core.online);
                switch.set_valign(gtk::Align::Center);
                let allow_toggle =
                    cpu_caps.has_core_online && core_online_writable && core.logical_cpu_id != 0;
                switch.set_sensitive(allow_toggle);
                switch.set_tooltip_text(Some(if core.logical_cpu_id == 0 {
                    "CPU 0 stays online."
                } else if !cpu_caps.has_core_online {
                    "This platform does not expose logical CPU online/offline control."
                } else if !core_online_writable {
                    "Readable, but not writable by the current user."
                } else if core.online {
                    "Turn this logical CPU off after confirmation."
                } else {
                    "Bring this logical CPU back online after confirmation."
                }));
                if allow_toggle {
                    let shared = shared_clone.clone();
                    let win = win_clone.clone();
                    let core_id = core.logical_cpu_id;
                    let core_for_dialog = core.clone();
                    let original_online = core.online;
                    switch.connect_active_notify(move |sw| {
                        let desired_online = sw.is_active();
                        if desired_online == original_online {
                            return;
                        }
                        let dialog = adw::MessageDialog::new(
                            Some(&win),
                            Some(cpu_toggle_dialog_title(desired_online)),
                            Some(&cpu_toggle_dialog_body(&core_for_dialog, desired_online)),
                        );
                        dialog.add_responses(&[
                            ("cancel", "Keep Current State"),
                            ("apply", "Apply Change"),
                        ]);
                        dialog.set_close_response("cancel");
                        dialog.set_default_response(Some("cancel"));

                        let shared_apply = shared.clone();
                        let sw_apply = sw.clone();
                        dialog.connect_response(Some("apply"), move |d, _| {
                            if let Ok(mut st) = shared_apply.lock() {
                                st.pending_cpu_actions.push(PendingCpuAction::CoreOnline {
                                    core_id,
                                    online: desired_online,
                                });
                                st.action_error = None;
                            }
                            d.close();
                            sw_apply.set_sensitive(false);
                        });

                        let sw_cancel = sw.clone();
                        dialog.connect_response(Some("cancel"), move |d, _| {
                            sw_cancel.set_active(original_online);
                            d.close();
                        });
                        dialog.present();
                    });
                }
                row.append(&label_box);
                row.append(&switch);
                cpu_core_toggle_box.append(&row);
            }

            cpu_access_report.set_text(&cpu_access_report_text(&cpu_caps));
        } else {
            cpu_card_temp.set_value("--");
            cpu_card_usage.set_value("--");
            cpu_card_power.set_value("--");
            cpu_card_clock.set_value("--");
            cpu_card.set_status_chip(Some("Telemetry unavailable"));
            live_cpu_usage_card.set_value("—");
            live_cpu_usage_card.set_unit(None);
            cpu_temp_gauge.set_speed_metrics("Avg Clock", None, None, None);
            cpu_read_only_banner.set_revealed(false);
            cpu_per_core_buffer.set_text("Logical CPU / thread telemetry unavailable.");
            cpu_access_report.set_text(&cpu_access_report_text(&cpu_caps));
        }

        let full_diagnostics = if caps_text.is_empty() {
            "Loading diagnostics...".to_string()
        } else {
            format!(
                "{caps_text}\n\n{}",
                privilege_diagnostics_text(
                    privileged_status.as_ref(),
                    &cpu_caps,
                    &fan_state,
                    lighting.as_ref(),
                    &caps,
                )
            )
        };
        update_diagnostics_buffer(&diag_buffer, &diag_scroller, &full_diagnostics);

        if daemon_error.is_some() {
            status_label.set_text(
                "rog-helperd is not reachable on the current session. Open Diagnostics for DBus endpoint details.",
            );
        } else if let Some(ref e) = action_error_txt {
            status_label.set_text(&friendly_action_error(e));
        } else {
            status_label.set_text("");
        }

        let support_messages = support_messages(&caps, &cpu_caps);
        let profile_read_failed = warning_detail(&warnings, "profile read failed: ");
        let gpu_mode_read_failed = warning_detail(&warnings, "GPU mode read failed: ");
        let charge_limit_read_failed = warning_detail(&warnings, "charge limit read failed: ");

        let missing_services = setup_status.control_issue_count();
        let control_state = if available_controls > 0 {
            format!("{available_controls} available")
        } else {
            "Read-only".to_string()
        };
        system_state_details.set_text(&format!(
            "Controls  {control_state}  ·  Setup issues  {missing_services}"
        ));

        let health_warning_count = support_messages.len() + warnings.len();
        if health_warning_count == 0 {
            set_dashboard_status(
                &health_warnings_indicator,
                &health_warnings,
                "None",
                "status-ok",
            );
        } else {
            set_dashboard_status(
                &health_warnings_indicator,
                &health_warnings,
                &health_warning_count.to_string(),
                "status-warning",
            );
        }

        if daemon_error.is_some() {
            warning_banner.remove_css_class("warning-info");
            warning_banner.remove_css_class("warning-warning");
            warning_banner.remove_css_class("warning-error");
            warning_banner.add_css_class("warning-error");
            warning_banner.set_title("rog-helperd is disconnected");
            warning_banner.set_button_label(Some("Open Diagnostics"));
            warning_subtitle.set_text("Live telemetry and controls are currently unavailable.");
            warning_subtitle.set_visible(true);
            warning_banner.set_revealed(true);
            warning_area.set_visible(true);
            open_diagnostics_button.set_visible(false);
        } else if setup_status.control_issue_count() > 0 {
            warning_banner.remove_css_class("warning-info");
            warning_banner.remove_css_class("warning-error");
            warning_banner.add_css_class("warning-warning");
            warning_banner.set_title(&format!(
                "{} control areas need setup",
                setup_status.control_issue_count()
            ));
            warning_banner.set_button_label(Some("Review"));
            warning_subtitle.set_text(
                "Telemetry is still available. Review optional dependencies and permissions in Setup & Access.",
            );
            warning_subtitle.set_visible(true);
            warning_banner.set_revealed(true);
            warning_area.set_visible(true);
            open_diagnostics_button.set_visible(false);
        } else if !warnings.is_empty() {
            warning_banner.remove_css_class("warning-info");
            warning_banner.remove_css_class("warning-error");
            warning_banner.add_css_class("warning-warning");
            warning_banner.set_title("Some readings need attention");
            warning_banner.set_button_label(Some("Open Diagnostics"));
            warning_subtitle.set_text(if warnings.len() == 1 {
                &warnings[0]
            } else {
                "Multiple telemetry warnings were reported. Open Diagnostics for details."
            });
            warning_subtitle.set_visible(true);
            warning_banner.set_revealed(true);
            warning_area.set_visible(true);
            open_diagnostics_button.set_visible(false);
        } else {
            warning_banner.remove_css_class("warning-warning");
            warning_banner.remove_css_class("warning-error");
            warning_banner.add_css_class("warning-info");
            warning_banner.set_revealed(false);
            warning_subtitle.set_visible(false);
            warning_subtitle.set_text("");
            warning_area.set_visible(false);
            open_diagnostics_button.set_visible(false);
        }

        profile_quiet.set_sensitive(caps.profile_access.is_available());
        profile_balanced.set_sensitive(caps.profile_access.is_available());
        profile_turbo.set_sensitive(caps.profile_access.is_available());
        gpu_page_profile_combo.set_sensitive(caps.profile_access.is_available());
        gpu_page_profile_apply.set_sensitive(caps.profile_access.is_available());
        if caps.profile_access.is_available() {
            if let Some(current) = profile.as_deref() {
                if profile_name_is(current, "silent") {
                    profile_quiet.set_active(true);
                } else if profile_name_is(current, "balanced") {
                    profile_balanced.set_active(true);
                } else if profile_name_is(current, "turbo")
                    || profile_name_is(current, "performance")
                {
                    profile_turbo.set_active(true);
                }
                profile_row.set_subtitle(&format!("Current: {current}"));
                gpu_page_current_profile.set_text(current);
                gpu_page_profile_card.set_value(current);
                gpu_page_profile_card.set_status_chip(Some("Available"));
                if !gpu_page_profile_combo.has_focus() {
                    if profile_name_is(current, "silent") {
                        gpu_page_profile_combo.set_active_id(Some("Silent"));
                    } else if profile_name_is(current, "balanced") {
                        gpu_page_profile_combo.set_active_id(Some("Balanced"));
                    } else if profile_name_is(current, "turbo")
                        || profile_name_is(current, "performance")
                    {
                        gpu_page_profile_combo.set_active_id(Some("Turbo"));
                    }
                }
                profile_row.set_subtitle(&format!("Current: {current}"));
            } else if profile_read_failed.is_some() {
                profile_row
                    .set_subtitle("Profile detected, but the current value could not be read.");
            } else {
                profile_row.set_subtitle("Quiet / Balanced / Turbo");
            }
            gpu_page_current_profile.set_text(&feature_state_value(
                profile.as_deref(),
                &caps.profile_access,
                profile_read_failed,
            ));
            if profile.is_none() {
                gpu_page_profile_card.set_value("—");
                gpu_page_profile_card.set_subtitle(Some("Current profile not reported"));
            }
            gpu_page_profile_row.set_subtitle("Apply profile through daemon");
        } else {
            profile_row.set_subtitle(&dashboard_control_subtitle(
                &caps.profile_access,
                "Quiet / Balanced / Turbo",
            ));
            gpu_page_current_profile.set_text(&feature_state_value(
                None,
                &caps.profile_access,
                profile_read_failed,
            ));
            gpu_page_profile_card.set_value("—");
            gpu_page_profile_card.set_subtitle(Some(&feature_value_label(&caps.profile_access)));
            gpu_page_profile_card
                .set_status_chip(Some(feature_status_label(caps.profile_access.status)));
            gpu_page_profile_row.set_subtitle(&feature_control_subtitle(
                &caps.profile_access,
                "Apply profile through daemon",
            ));
        }

        if *last_gpu_supported_modes.borrow() != caps.gpu_supported_modes {
            let mode_refs = caps
                .gpu_supported_modes
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let model = gtk::StringList::new(&mode_refs);
            gpu_mode_dropdown.set_model(Some(&model));
            gpu_page_mode_combo.remove_all();
            for mode in &caps.gpu_supported_modes {
                gpu_page_mode_combo.append(Some(mode), mode);
            }
            *last_gpu_supported_modes.borrow_mut() = caps.gpu_supported_modes.clone();
        }

        gpu_mode_row.set_visible(true);
        gpu_page_mode_combo.set_sensitive(caps.gpu_mode_access.is_available());
        gpu_page_mode_apply.set_sensitive(caps.gpu_mode_access.is_available());
        if caps.gpu_mode_access.is_available() {
            gpu_mode_dropdown.set_sensitive(true);
            gpu_apply_button.set_sensitive(true);
            if let Some(current) = gpu_mode.as_deref() {
                if let Some(index) = gpu_mode_index_in_supported(current, &caps.gpu_supported_modes)
                {
                    gpu_mode_dropdown.set_selected(index as u32);
                }
                gpu_mode_row.set_subtitle(&format!("Current: {current}"));
                gpu_page_mode_card.set_value(current);
                gpu_page_mode_card.set_status_chip(Some("Available"));
                if !gpu_page_mode_combo.has_focus() {
                    if let Some(index) =
                        gpu_mode_index_in_supported(current, &caps.gpu_supported_modes)
                    {
                        gpu_page_mode_combo.set_active_id(Some(&caps.gpu_supported_modes[index]));
                    }
                }
            } else if gpu_mode_read_failed.is_some() {
                gpu_mode_row
                    .set_subtitle("GPU mode is detected, but the current value could not be read.");
            } else {
                gpu_mode_row.set_subtitle("Select mode and apply");
            }
            gpu_page_current_mode.set_text(&feature_state_value(
                gpu_mode.as_deref(),
                &caps.gpu_mode_access,
                gpu_mode_read_failed,
            ));
            if gpu_mode.is_none() {
                gpu_page_mode_card.set_value("—");
                gpu_page_mode_card.set_subtitle(Some("Current mode not reported"));
            }
            gpu_page_switch_hint.set_text(&gpu_switch_hint_text(&caps, &warnings));
            gpu_page_mode_row.set_subtitle("Apply GPU mode through daemon");
        } else {
            gpu_mode_dropdown.set_sensitive(false);
            gpu_apply_button.set_sensitive(false);
            gpu_mode_row.set_subtitle(&dashboard_control_subtitle(
                &caps.gpu_mode_access,
                "Select mode and apply",
            ));
            gpu_page_current_mode.set_text(&feature_state_value(
                None,
                &caps.gpu_mode_access,
                gpu_mode_read_failed,
            ));
            gpu_page_mode_card.set_value("—");
            gpu_page_mode_card.set_subtitle(Some(&feature_value_label(&caps.gpu_mode_access)));
            gpu_page_mode_card
                .set_status_chip(Some(feature_status_label(caps.gpu_mode_access.status)));
            gpu_page_switch_hint.set_text(&gpu_switch_hint_text(&caps, &warnings));
            gpu_page_mode_row.set_subtitle(&feature_control_subtitle(
                &caps.gpu_mode_access,
                "Apply GPU mode through daemon",
            ));
        }

        if !caps.profile_access.is_available() {
            gpu_page_profile_card.set_subtitle(Some("See Control Dependencies"));
            gpu_page_profile_card.set_status_chip(Some("Control unavailable"));
            gpu_page_profile_row.set_subtitle("Unavailable · see Control Dependencies");
        }
        if !caps.gpu_mode_access.is_available() {
            gpu_page_mode_card.set_subtitle(Some("See Control Dependencies"));
            gpu_page_mode_card.set_status_chip(Some("Control unavailable"));
            gpu_page_mode_row.set_subtitle("Unavailable · see Control Dependencies");
        }

        charge_limit_row.set_visible(true);
        if caps.charge_limit_access.is_available() {
            charge_limit_spin.set_sensitive(true);
            charge_apply_button.set_sensitive(true);
            battery_charge_spin.set_sensitive(true);
            battery_charge_apply.set_sensitive(true);
            let battery_authorization_required = caps.battery_limit_backend == "power_supply_sysfs"
                && caps.battery_limit_privileged_write
                && caps.battery_limit_authorization != "authorized";
            battery_charge_apply.set_label(if battery_authorization_required {
                "Unlock & Apply"
            } else {
                "Apply"
            });
            charge_apply_button.set_label(if battery_authorization_required {
                "Unlock & Apply"
            } else {
                "Apply"
            });
            if let Some(limit) = battery_limit {
                if !charge_limit_spin.has_focus() {
                    charge_limit_spin.set_value(limit as f64);
                }
                if !battery_charge_spin.has_focus() {
                    battery_charge_spin.set_value(limit as f64);
                }
                charge_limit_row.set_subtitle(&format!("Current: {limit}%"));
                battery_charge_row.set_subtitle(&format!("Current limit: {limit}%"));
            } else if charge_limit_read_failed.is_some() {
                charge_limit_row.set_subtitle(
                    "Charge limit is detected, but the current value could not be read.",
                );
                battery_charge_row.set_subtitle(
                    "Charge limit is detected, but the current value could not be read.",
                );
            } else {
                charge_limit_row.set_subtitle("Set desired limit and apply");
                battery_charge_row.set_subtitle("Set desired limit and apply");
            }
        } else {
            charge_limit_spin.set_sensitive(false);
            charge_apply_button.set_sensitive(false);
            battery_charge_spin.set_sensitive(false);
            battery_charge_apply.set_sensitive(false);
            charge_limit_row.set_subtitle(&dashboard_control_subtitle(
                &caps.charge_limit_access,
                "Set desired limit and apply",
            ));
            battery_charge_row.set_subtitle(&feature_control_subtitle(
                &caps.charge_limit_access,
                "Set desired limit and apply",
            ));
        }

        if let Some(ref msg) = action_error_txt {
            gpu_page_last_action.set_text(&friendly_action_error(msg));
        } else if daemon_error.is_some() {
            gpu_page_last_action.set_text("Daemon unavailable.");
        } else {
            gpu_page_last_action.set_text(&gpu_status_summary(&caps, &warnings));
        }

        let has_kbd_backlight = caps.has_kbd_backlight
            || lighting
                .as_ref()
                .is_some_and(|lighting| lighting.supports_brightness);
        kbd_backlight_row.set_visible(has_kbd_backlight);
        if has_kbd_backlight {
            if let Some(ref l) = lighting {
                kbd_brightness_spin.set_range(0.0, l.max_brightness as f64);
                if !kbd_brightness_spin.has_focus() {
                    kbd_brightness_spin.set_value(l.brightness as f64);
                }
                let can_set_brightness = l.can_set && l.supports_brightness;
                kbd_brightness_spin.set_sensitive(can_set_brightness);
                kbd_apply_button.set_sensitive(can_set_brightness);
                if l.authorization_required {
                    kbd_backlight_row.set_subtitle(
                        "Administrator access required to change keyboard lighting. Apply opens the PolicyKit prompt.",
                    );
                } else if can_set_brightness {
                    kbd_backlight_row.set_subtitle("Adjust brightness");
                } else {
                    kbd_backlight_row
                        .set_subtitle(&feature_control_subtitle(&caps.kbd_backlight_access, ""));
                }
            } else {
                kbd_brightness_spin.set_sensitive(false);
                kbd_apply_button.set_sensitive(false);
                kbd_backlight_row
                    .set_subtitle(&feature_control_subtitle(&caps.kbd_backlight_access, ""));
            }
        }

        let endpoint_entries: Vec<(String, String)> = caps
            .endpoints
            .iter()
            .take(6)
            .enumerate()
            .map(|(idx, endpoint)| (format!("Endpoint {}", idx + 1), endpoint.clone()))
            .collect();
        update_detail_rows(&detail_endpoint_rows, &endpoint_entries);
        details_endpoints_group.set_visible(!endpoint_entries.is_empty());

        // Lighting page (capability-driven).
        lighting_availability.set_text(&lighting_availability_label(
            lighting.as_ref(),
            &caps.kbd_backlight_access,
        ));
        if let Some(ref l) = lighting {
            lighting_overview_group.set_title(&lighting_device_title(l));
            lighting_device.set_text(&lighting_device_label(l));
            lighting_backend.set_text(&lighting_backend_label(l));
            lighting_current.set_text(&lighting_current_label(l));
            lighting_mode.set_text(&lighting_mode_label(l));
            lighting_rgb_current.set_text(&lighting_rgb_current_label(l));
            lighting_rgb_capability.set_text(if l.supports_rgb {
                "Available"
            } else {
                "Not available"
            });
            lighting_argb_capability.set_text(if l.supports_argb || l.supports_zones {
                "Supported by backend"
            } else {
                "Not reported by backend"
            });
            lighting_per_key_capability.set_text(if l.supports_per_key {
                "Supported by backend"
            } else {
                "Not reported by backend"
            });
            lighting_preview_group.set_visible(true);
            preview_caption.set_text(&lighting_preview_caption(l));
            if let Some(rgba) = l.rgb_hex.as_deref().and_then(rgb_hex_to_rgba) {
                if !rgb_button.is_focus() {
                    *preview_colour.borrow_mut() = rgba;
                    lighting_preview.queue_draw();
                }
            }

            brightness_scale.set_range(0.0, l.max_brightness as f64);
            if !brightness_scale.is_focus() {
                brightness_scale.set_value(l.brightness as f64);
            }
            let can_set = l.can_set;
            lighting_capability_banner.set_title(if l.authorization_required {
                "Administrator access required to change keyboard lighting"
            } else if l.authorization == "denied" {
                "Administrator authorization was denied"
            } else if can_set {
                "Lighting controls are available"
            } else {
                "Lighting telemetry is available, but controls are read-only"
            });
            lighting_capability_banner
                .set_revealed(!can_set || l.authorization_required || l.authorization == "denied");
            brightness_row.set_visible(l.supports_brightness);
            brightness_scale.set_sensitive(can_set && l.supports_brightness);
            brightness_row.set_subtitle(&lighting_brightness_subtitle(
                Some(l),
                &caps.kbd_backlight_access,
            ));
            mode_row.set_subtitle(&lighting_mode_subtitle(Some(l), &caps.kbd_backlight_access));
            apply_row.set_subtitle(&lighting_apply_subtitle(
                Some(l),
                &caps.kbd_backlight_access,
            ));
            if !can_set {
                brightness_row.set_subtitle("Unavailable · see capability status above");
                mode_row.set_subtitle("Unavailable · see capability status above");
                apply_row.set_subtitle("Unavailable · see capability status above");
            }

            let modes = l.supported_modes.clone();
            mode_row.set_visible(l.supports_modes && !modes.is_empty());
            mode_combo.set_sensitive(can_set && l.supports_modes && !modes.is_empty());
            if !mode_combo.is_focus() {
                let mut last = last_supported_modes.borrow_mut();
                if *last != modes {
                    mode_combo.remove_all();
                    for m in &modes {
                        mode_combo.append(Some(m), m);
                    }
                    *last = modes.clone();
                }

                if let Some(desired) = modes
                    .iter()
                    .find(|m| m.as_str().eq_ignore_ascii_case(&l.mode))
                    .cloned()
                    .or_else(|| modes.first().cloned())
                {
                    mode_combo.set_active_id(Some(&desired));
                } else {
                    mode_combo.set_active_id(None);
                }
            }

            let selected_mode = mode_combo
                .active_id()
                .map(|value| value.to_string())
                .unwrap_or_else(|| l.mode.clone());

            let show_primary_colour =
                l.supports_rgb && lighting_mode_supports_primary_colour(&selected_mode);
            rgb_button.set_sensitive(can_set && show_primary_colour);
            rgb_row.set_visible(show_primary_colour);
            secondary_rgb_row.set_visible(
                l.supports_rgb && lighting_mode_supports_secondary_colour(&selected_mode),
            );
            secondary_rgb_button.set_sensitive(can_set && l.supports_rgb);
            if let Some(rgba) = l
                .secondary_rgb_hex
                .as_deref()
                .and_then(rgb_hex_to_rgba)
                .filter(|_| !secondary_rgb_button.is_focus())
            {
                secondary_rgb_button.set_rgba(&rgba);
            }

            let speeds = l.supported_speeds.clone();
            let show_speed = l.supports_speed
                && !speeds.is_empty()
                && lighting_mode_supports_speed(&selected_mode);
            speed_row.set_visible(show_speed);
            speed_combo.set_sensitive(can_set && show_speed);
            if !speed_combo.is_focus() {
                let mut last = last_supported_speeds.borrow_mut();
                if *last != speeds {
                    speed_combo.remove_all();
                    for speed in &speeds {
                        speed_combo.append(Some(speed), speed);
                    }
                    *last = speeds.clone();
                }
                select_combo_value(&speed_combo, &speeds, l.speed.as_deref());
            }

            let directions = l.supported_directions.clone();
            let show_direction =
                !directions.is_empty() && lighting_mode_supports_direction(&selected_mode);
            direction_row.set_visible(show_direction);
            direction_combo.set_sensitive(can_set && show_direction);
            if !direction_combo.is_focus() {
                let mut last = last_supported_directions.borrow_mut();
                if *last != directions {
                    direction_combo.remove_all();
                    for direction in &directions {
                        direction_combo.append(Some(direction), direction);
                    }
                    *last = directions.clone();
                }
                select_combo_value(&direction_combo, &directions, l.direction.as_deref());
            }

            let zones = l.supported_zones.clone();
            let show_zones = l.supports_zones && !zones.is_empty();
            zone_row.set_visible(show_zones);
            zone_combo.set_sensitive(can_set && show_zones);
            if !zone_combo.is_focus() {
                let mut last = last_supported_zones.borrow_mut();
                if *last != zones {
                    zone_combo.remove_all();
                    for zone in &zones {
                        zone_combo.append(Some(zone), zone);
                    }
                    *last = zones.clone();
                }
                select_combo_value(&zone_combo, &zones, l.active_zone.as_deref());
            }
            let has_visible_controls =
                l.supports_brightness || (l.supports_modes && !modes.is_empty()) || l.supports_rgb;
            lighting_controls_group.set_visible(has_visible_controls);
            apply_row.set_visible(can_set && has_visible_controls);
            apply_lighting.set_sensitive(can_set && has_visible_controls);
            apply_lighting.set_label(if l.authorization_required {
                "Unlock & Apply lighting"
            } else {
                "Apply lighting"
            });
            lighting_rgb_note.set_visible(!l.supports_rgb);
            if let Some(rgba) = l
                .rgb_hex
                .as_deref()
                .and_then(rgb_hex_to_rgba)
                .filter(|_| !rgb_button.is_focus())
            {
                rgb_button.set_rgba(&rgba);
            }
            rgb_row.set_subtitle(&lighting_rgb_subtitle(Some(l), &caps.kbd_backlight_access));
        } else {
            lighting_overview_group.set_title("Lighting Device");
            lighting_capability_banner.set_title(&format!(
                "Lighting controls unavailable · {}",
                feature_value_label(&caps.kbd_backlight_access)
            ));
            lighting_capability_banner.set_revealed(true);
            lighting_backend.set_text(&feature_value_label(&caps.kbd_backlight_access));
            lighting_device.set_text(&lighting_placeholder_value(&caps.kbd_backlight_access));
            lighting_current.set_text(&lighting_placeholder_value(&caps.kbd_backlight_access));
            lighting_mode.set_text(&lighting_placeholder_value(&caps.kbd_backlight_access));
            lighting_rgb_current.set_text(&lighting_placeholder_value(&caps.kbd_backlight_access));
            lighting_rgb_capability.set_text("Not available");
            lighting_argb_capability.set_text("Not available");
            lighting_per_key_capability.set_text("Not available");
            lighting_preview_group.set_visible(false);
            brightness_scale.set_range(0.0, 3.0);
            if !brightness_scale.is_focus() {
                brightness_scale.set_value(0.0);
            }
            brightness_scale.set_sensitive(false);
            mode_combo.set_sensitive(false);
            apply_lighting.set_sensitive(false);
            rgb_button.set_sensitive(false);
            lighting_controls_group.set_visible(false);
            brightness_row.set_visible(false);
            mode_row.set_visible(false);
            apply_row.set_visible(false);
            rgb_row.set_visible(false);
            secondary_rgb_row.set_visible(false);
            speed_row.set_visible(false);
            direction_row.set_visible(false);
            zone_row.set_visible(false);
            lighting_rgb_note.set_visible(true);
            brightness_row.set_subtitle(&lighting_brightness_subtitle(
                None,
                &caps.kbd_backlight_access,
            ));
            mode_row.set_subtitle(&lighting_mode_subtitle(None, &caps.kbd_backlight_access));
            apply_row.set_subtitle(&lighting_apply_subtitle(None, &caps.kbd_backlight_access));
            rgb_row.set_subtitle(&lighting_rgb_subtitle(None, &caps.kbd_backlight_access));
            brightness_row.set_subtitle("Unavailable · see capability status above");
            mode_row.set_subtitle("Unavailable · see capability status above");
            apply_row.set_subtitle("Unavailable · see capability status above");
            if !mode_combo.is_focus() {
                mode_combo.remove_all();
                last_supported_modes.borrow_mut().clear();
            }
            speed_combo.remove_all();
            direction_combo.remove_all();
            zone_combo.remove_all();
            last_supported_speeds.borrow_mut().clear();
            last_supported_directions.borrow_mut().clear();
            last_supported_zones.borrow_mut().clear();
        }

        if let Some(msg) = lighting_error_txt {
            lighting_last_action.set_text(&friendly_action_error(&msg));
        } else {
            lighting_last_action.set_text(&lighting_last_action_text(
                lighting.as_ref(),
                &caps.kbd_backlight_access,
            ));
        }

        about_latest_release.set_text(
            update_state
                .latest_version
                .as_deref()
                .unwrap_or("Waiting for a manual check."),
        );
        about_update_status.set_text(&update_state.status_text);
        about_last_check.set_text(
            &update_state
                .last_checked_unix
                .map(format_timestamp_local)
                .unwrap_or_else(|| "Not checked in this session.".to_string()),
        );
        about_last_result.set_text(&update_state.last_result_text);
        about_release_notes.set_text(
            update_state
                .release_notes_preview
                .as_deref()
                .unwrap_or("Release notes from the latest checked release will appear here."),
        );
        if let Some(full_notes) = update_state.release_notes_full.as_deref() {
            release_notes_full.set_text(full_notes);
            release_notes_expander.set_visible(true);
        } else {
            release_notes_full.set_text("");
            release_notes_expander.set_visible(false);
        }

        let check_subtitle = if update_check_in_progress {
            "Checking the latest GitHub release right now."
        } else if update_action_in_progress {
            "Wait for the current update action to finish before checking again."
        } else {
            "Query the latest GitHub release without blocking the UI."
        };
        check_updates_row.set_subtitle(check_subtitle);
        check_updates_button.set_sensitive(
            update_check_available && !update_check_in_progress && !update_action_in_progress,
        );

        update_action_button.set_label(&update_state.update_action_label);
        if update_action_in_progress {
            update_action_row.set_subtitle("Applying the latest release asset. Keep this window open until the action finishes.");
        } else {
            update_action_row.set_subtitle(&update_state.update_action_subtitle);
        }
        update_action_button.set_sensitive(
            !update_check_in_progress
                && !update_action_in_progress
                && (update_state.release_page_url.is_some() || update_action_available),
        );

        glib::ControlFlow::Continue
    });
}

fn spawn_background(shared: Arc<Mutex<SharedUiState>>, app_metadata: AppMetadata) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                if let Ok(mut st) = shared.lock() {
                    st.daemon_error = Some(format!("failed to start tokio runtime: {e}"));
                    st.mark_render_dirty();
                }
                return;
            }
        };

        rt.block_on(async move {
            let tray = RogTray {
                shared: shared.clone(),
                summary: "rog-helper".to_string(),
            };

            // If SNI is missing (common on GNOME without an extension), continue without a tray.
            let tray_handle = match tray.assume_sni_available(true).spawn().await {
                Ok(h) => Some(h),
                Err(e) => {
                    warn!("tray unavailable: {e}");
                    None
                }
            };
            if let Ok(mut st) = shared.lock() {
                st.tray_available = Some(tray_handle.is_some());
                if tray_handle.is_none()
                    && st.settings.ui.close_behavior == CloseBehavior::MinimizeToTray
                {
                    st.show_window = true;
                    st.pending_toast =
                        Some(("Tray is unavailable; keeping the main window open.".to_string(), true));
                }
                st.mark_render_dirty();
            }

            match fetch_configuration().await {
                Ok(config) => {
                    if let Err(error) = sync_autostart(&config.ui) {
                        warn!("failed to synchronize autostart state: {error}");
                    }
                    if let Ok(mut st) = shared.lock() {
                        if st.pending_config_save.is_none() && !st.pending_config_reset {
                            st.settings = config;
                            st.mark_render_dirty();
                        }
                    }
                }
                Err(error) => warn!("unable to read daemon configuration: {error}"),
            }

            loop {
                if shared.lock().map(|st| st.quit).unwrap_or(true) {
                    break;
                }

                let pending_config_reset = shared
                    .lock()
                    .ok()
                    .map(|mut st| std::mem::take(&mut st.pending_config_reset))
                    .unwrap_or(false);
                if pending_config_reset {
                    match reset_configuration().await {
                        Ok(config) => {
                            if let Err(error) = sync_autostart(&config.ui) {
                                warn!("failed to reset autostart state: {error}");
                            }
                            if let Ok(mut st) = shared.lock() {
                                st.settings = config;
                                st.pending_config_save = None;
                                st.pending_toast =
                                    Some(("Settings reset to defaults.".to_string(), false));
                                st.mark_render_dirty();
                            }
                        }
                        Err(error) => {
                            if let Ok(mut st) = shared.lock() {
                                st.pending_toast = Some((error, true));
                            }
                        }
                    }
                }

                let pending_config = shared
                    .lock()
                    .ok()
                    .and_then(|mut st| st.pending_config_save.take());
                if let Some(config) = pending_config {
                    if let Err(error) = save_configuration(&config).await {
                        let persisted = fetch_configuration().await.ok();
                        if let Some(config) = persisted.as_ref() {
                            let _ = sync_autostart(&config.ui);
                        }
                        if let Ok(mut st) = shared.lock() {
                            if let Some(config) = persisted {
                                st.settings = config;
                            }
                            st.pending_toast = Some((error, true));
                            st.mark_render_dirty();
                        }
                    }
                }

                let pending_profile = shared
                    .lock()
                    .ok()
                    .and_then(|mut st| st.pending_profile.take());
                if let Some(profile) = pending_profile {
                    if let Err(e) = apply_profile(profile).await {
                        if let Ok(mut st) = shared.lock() {
                            st.action_error = Some(e);
                        }
                    }
                }

                let pending_gpu_mode = shared
                    .lock()
                    .ok()
                    .and_then(|mut st| st.pending_gpu_mode.take());
                if let Some(mode) = pending_gpu_mode {
                    match apply_gpu_mode(mode).await {
                        Ok(()) => {
                            if let Ok(mut st) = shared.lock() {
                                st.action_error = None;
                            }
                        }
                        Err(e) => {
                            if let Ok(mut st) = shared.lock() {
                                st.action_error = Some(e);
                            }
                        }
                    }
                }

                let pending_battery_limit = shared
                    .lock()
                    .ok()
                    .and_then(|mut st| st.pending_battery_limit.take());
                if let Some(limit) = pending_battery_limit {
                    if let Err(e) = apply_battery_limit(limit).await {
                        if let Ok(mut st) = shared.lock() {
                            st.action_error = Some(e);
                        }
                    }
                }

                let pending_cpu_actions = shared
                    .lock()
                    .ok()
                    .map(|mut st| std::mem::take(&mut st.pending_cpu_actions))
                    .unwrap_or_default();
                if !pending_cpu_actions.is_empty() {
                    match apply_cpu_actions(pending_cpu_actions).await {
                        Ok(()) => {
                            if let Ok(mut st) = shared.lock() {
                                st.action_error = None;
                                st.pending_toast =
                                    Some(("CPU settings applied".to_string(), false));
                            }
                        }
                        Err(e) => {
                            if let Ok(mut st) = shared.lock() {
                                st.action_error = Some(e.clone());
                                st.pending_toast = Some((e, true));
                            }
                        }
                    }
                }

                // Apply pending lighting action before refreshing state.
                let pending = shared
                    .lock()
                    .ok()
                    .and_then(|mut st| st.pending_lighting.take());
                if let Some(p) = pending {
                    if let Err(e) = apply_lighting(p).await {
                        if let Ok(mut st) = shared.lock() {
                            st.lighting_error = Some(e);
                        }
                    }
                }

                let pending_fan_action = shared
                    .lock()
                    .ok()
                    .and_then(|mut st| st.pending_fan_action.take());
                if let Some(action) = pending_fan_action {
                    match apply_fan_action(action).await {
                        Ok(()) => {
                            if let Ok(mut st) = shared.lock() {
                                st.action_error = None;
                                st.pending_toast = Some(("Fan setting applied".to_string(), false));
                            }
                        }
                        Err(e) => {
                            if let Ok(mut st) = shared.lock() {
                                st.action_error = Some(e.clone());
                                st.pending_toast = Some((e, true));
                            }
                        }
                    }
                }

                let pending_update_check = shared
                    .lock()
                    .ok()
                    .map(|mut st| std::mem::take(&mut st.pending_update_check))
                    .unwrap_or(false);
                if pending_update_check {
                    match check_for_updates(&app_metadata).await {
                        Ok(update_state) => {
                            if let Ok(mut st) = shared.lock() {
                                st.update_state = update_state;
                                st.update_check_in_progress = false;
                                st.mark_render_dirty();
                            }
                        }
                        Err(error) => {
                            let previous_state = shared
                                .lock()
                                .ok()
                                .map(|st| st.update_state.clone())
                                .unwrap_or_default();
                            if let Ok(mut st) = shared.lock() {
                                st.update_state =
                                    update_check_failed_state(&app_metadata, &error, &previous_state);
                                st.update_check_in_progress = false;
                                st.pending_toast =
                                    Some(("Unable to check for updates right now.".to_string(), true));
                                st.mark_render_dirty();
                            }
                        }
                    }
                }

                let pending_update_action = shared
                    .lock()
                    .ok()
                    .map(|mut st| std::mem::take(&mut st.pending_update_action))
                    .unwrap_or(false);
                if pending_update_action {
                    let update_state = shared
                        .lock()
                        .ok()
                        .map(|st| st.update_state.clone())
                        .unwrap_or_default();
                    match apply_update_action(&app_metadata, &update_state).await {
                        Ok(UpdateActionOutcome::Replaced { version }) => {
                            if let Ok(mut st) = shared.lock() {
                                st.update_action_in_progress = false;
                                st.update_state.status_text = format!(
                                    "Update downloaded. Restart required to switch to {version}."
                                );
                                st.update_state.last_result_text = format!(
                                    "The binary on disk was replaced with {version}. Restart rog-helper to use the new version."
                                );
                                st.update_state.update_action_label = "Updated on Disk".to_string();
                                st.update_state.update_action_subtitle =
                                    "Restart rog-helper to finish switching to the downloaded binary."
                                        .to_string();
                                st.update_state.supports_in_place_update = false;
                                st.pending_toast = Some((
                                    format!(
                                        "Update downloaded. Restart rog-helper to use {version}."
                                    ),
                                    false,
                                ));
                                st.mark_render_dirty();
                            }
                        }
                        Ok(UpdateActionOutcome::OpenReleasePage { url, message }) => {
                            if let Ok(mut st) = shared.lock() {
                                st.update_action_in_progress = false;
                                st.update_state.status_text =
                                    "Automatic in-place update is not supported here.".to_string();
                                st.update_state.last_result_text = message.clone();
                                st.pending_open_link = Some(OpenLinkRequest {
                                    url,
                                    error_message:
                                        "Unable to open the latest release page right now."
                                            .to_string(),
                                });
                                st.pending_toast = Some((message, false));
                                st.mark_render_dirty();
                            }
                        }
                        Err(error) => {
                            if let Ok(mut st) = shared.lock() {
                                st.update_action_in_progress = false;
                                st.update_state.status_text =
                                    "Automatic in-place update could not be completed."
                                        .to_string();
                                st.update_state.last_result_text = error.clone();
                                st.pending_toast = Some((error, true));
                                st.mark_render_dirty();
                            }
                        }
                    }
                }

                match fetch_state().await {
                    Ok((
                        t,
                        cpu,
                        cpu_caps,
                        caps,
                        caps_text,
                        warnings,
                        profile,
                        gpu_mode,
                        battery_limit,
                        lighting,
                        fan_state,
                    )) => {
                        let summary = summary_from_state(&t, profile.as_deref(), gpu_mode.as_deref());
                        if let Some(h) = &tray_handle {
                            let _ = h.update(|tray| tray.summary = summary.clone()).await;
                        }
                        if let Ok(mut st) = shared.lock() {
                            let gpu_temp_c = t.gpu_temp_c;
                            let gpu_usage_percent = t.gpu_usage_percent;
                            let memory_used_percent = t.mem_used_percent;
                            st.telemetry = Some(t);
                            st.cpu = cpu.clone();
                            st.cpu_caps = cpu_caps;
                            push_history(
                                &mut st.cpu_usage_history,
                                cpu.as_ref().and_then(|c| c.usage_percent),
                                60,
                            );
                            push_history(
                                &mut st.cpu_temp_history,
                                cpu.as_ref().and_then(|c| c.temp_c),
                                60,
                            );
                            push_history(
                                &mut st.cpu_power_history,
                                cpu.as_ref().and_then(|c| c.package_power_w),
                                60,
                            );
                            push_history(&mut st.gpu_temp_history, gpu_temp_c, 60);
                            push_history(&mut st.gpu_usage_history, gpu_usage_percent, 60);
                            push_history(
                                &mut st.memory_usage_history,
                                memory_used_percent,
                                60,
                            );
                            st.caps = caps;
                            st.caps_text = caps_text;
                            st.warnings = warnings;
                            st.profile = profile;
                            st.gpu_mode = gpu_mode;
                            st.battery_limit = battery_limit;
                            st.lighting = lighting;
                            st.fan_state = fan_state;
                            st.daemon_error = None;
                            st.mark_render_dirty();
                        }
                    }
                    Err(e) => {
                        warn!("{e}");
                        if let Ok(mut st) = shared.lock() {
                            st.setup_status = SetupStatus::daemon_unavailable(e.clone());
                            st.daemon_error = Some(e);
                            st.mark_render_dirty();
                        }
                    }
                }

                let refresh_setup = shared
                    .lock()
                    .ok()
                    .map(|mut state| std::mem::take(&mut state.pending_setup_refresh))
                    .unwrap_or(false);
                if refresh_setup {
                    if let Ok(mut state) = shared.lock() {
                        state.setup_refreshing = true;
                        state.mark_render_dirty();
                    }
                    let (setup_result, privileged_result) =
                        tokio::join!(fetch_setup_status(), fetch_privileged_status());
                    if let Ok(mut state) = shared.lock() {
                        state.privileged_status = Some(privileged_result.unwrap_or_else(|_| {
                            PrivilegedStatus::unavailable(false, false)
                        }));
                    }
                    match setup_result {
                        Ok(status) => {
                            if let Ok(mut state) = shared.lock() {
                                state.setup_status = status;
                                state.setup_refreshing = false;
                                state.pending_toast = Some((
                                    "Setup and access checks refreshed.".to_string(),
                                    false,
                                ));
                                state.mark_render_dirty();
                            }
                        }
                        Err(error) => {
                            if let Ok(mut state) = shared.lock() {
                                state.setup_status = SetupStatus::daemon_unavailable(error.clone());
                                state.setup_refreshing = false;
                                state.pending_toast = Some((
                                    "Setup checks could not reach rog-helperd.".to_string(),
                                    true,
                                ));
                                state.mark_render_dirty();
                            }
                        }
                    }
                }

                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            if let Some(h) = tray_handle {
                h.shutdown().await;
            }
        });
    });
}

async fn fetch_setup_status() -> Result<SetupStatus, String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|error| format!("session DBus unavailable: {error}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|error| format!("failed to connect to daemon proxy: {error}"))?;
    let map = proxy
        .get_setup_status()
        .await
        .map_err(|error| format!("daemon GetSetupStatus failed: {error}"))?;
    Ok(setup_status_from_dbus(&map))
}

async fn fetch_privileged_status() -> Result<PrivilegedStatus, String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|error| format!("session DBus unavailable: {error}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|error| format!("failed to connect to daemon proxy: {error}"))?;
    let map = proxy
        .get_privileged_status()
        .await
        .map_err(|error| format!("daemon GetPrivilegedStatus failed: {error}"))?;
    Ok(privileged_status_from_dbus(&map))
}

async fn save_configuration(config: &AppConfig) -> Result<(), String> {
    let contents = config_to_toml(config)?;
    let conn = zbus::Connection::session()
        .await
        .map_err(|error| format!("session DBus unavailable: {error}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|error| format!("failed to connect to daemon proxy: {error}"))?;
    proxy
        .set_configuration(&contents)
        .await
        .map_err(|error| format!("Unable to save settings through rog-helperd: {error}"))
}

async fn fetch_configuration() -> Result<AppConfig, String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|error| format!("session DBus unavailable: {error}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|error| format!("failed to connect to daemon proxy: {error}"))?;
    let contents = proxy
        .get_configuration()
        .await
        .map_err(|error| format!("daemon GetConfiguration failed: {error}"))?;
    let loaded = parse_config(&contents);
    for warning in loaded.warnings {
        warn!("daemon configuration warning: {warning}");
    }
    Ok(loaded.config)
}

async fn reset_configuration() -> Result<AppConfig, String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|error| format!("session DBus unavailable: {error}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|error| format!("failed to connect to daemon proxy: {error}"))?;
    let contents = proxy
        .reset_configuration()
        .await
        .map_err(|error| format!("Unable to reset settings through rog-helperd: {error}"))?;
    Ok(parse_config(&contents).config)
}

async fn fetch_state() -> Result<
    (
        TelemetrySnapshot,
        Option<CpuTelemetry>,
        CpuCaps,
        DeviceCaps,
        String,
        Vec<String>,
        Option<String>,
        Option<String>,
        Option<u8>,
        Option<LightingInfo>,
        FanState,
    ),
    String,
> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| format!("session DBus unavailable: {e}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|e| format!("failed to connect to daemon proxy: {e}"))?;
    let state = proxy
        .get_state()
        .await
        .map_err(|e| format!("daemon GetState failed: {e}"))?;

    let caps_map = dbus_decode::nested_map(&state, dbus_keys::state::CAPS).unwrap_or_default();
    let telemetry_map =
        dbus_decode::nested_map(&state, dbus_keys::state::TELEMETRY).unwrap_or_default();
    let cpu_map = dbus_decode::nested_map(&state, dbus_keys::state::CPU).unwrap_or_default();
    let cpu_caps_map =
        dbus_decode::nested_map(&state, dbus_keys::state::CPU_CAPS).unwrap_or_default();
    let warnings = dbus_decode::strings(&state, dbus_keys::state::WARNINGS).unwrap_or_default();
    let profile = dbus_decode::string(&state, dbus_keys::state::PROFILE);
    let gpu_mode = dbus_decode::string(&state, dbus_keys::state::GPU_MODE);
    let battery_limit = dbus_decode::unsigned(&state, dbus_keys::state::BATTERY_LIMIT)
        .and_then(|v| u8::try_from(v).ok());
    let lighting =
        dbus_decode::nested_map(&state, dbus_keys::state::LIGHTING).and_then(lighting_from_dbus);
    let fan_state = state
        .get(dbus_keys::FAN_STATE_KEY)
        .cloned()
        .and_then(|v| HashMap::<String, OwnedValue>::try_from(v).ok())
        .map(fan_state_from_dbus)
        .unwrap_or_else(|| {
            FanState::from_fans(
                telemetry_from_dbus(telemetry_map.clone())
                    .fan_rows
                    .iter()
                    .enumerate()
                    .map(|(idx, fan)| FanInfo::read_only_from_telemetry((idx + 1) as u32, fan))
                    .collect(),
            )
        });
    let lighting_diagnostics_details =
        dbus_decode::string(&state, dbus_keys::state::LIGHTING_DIAGNOSTICS_DETAILS);

    let caps = caps_from_dbus(&caps_map);
    let cpu_caps = cpu_caps_from_dbus(&cpu_caps_map);
    let cpu = if cpu_map.is_empty() {
        None
    } else {
        Some(cpu_telemetry_from_dbus(cpu_map))
    };
    let telemetry = telemetry_from_dbus(telemetry_map);
    let mut caps_text = troubleshooting_summary_text(&caps, &cpu_caps, &warnings);
    caps_text.push_str("\n\n");
    caps_text.push_str(&caps_text_from_dbus(caps_map));
    let fan_text = fan_diagnostics_text(&telemetry);
    if !fan_text.is_empty() {
        caps_text.push_str("\n\n");
        caps_text.push_str(&fan_text);
    }
    if let Some(lighting) = lighting.as_ref() {
        caps_text.push_str("\n\n");
        caps_text.push_str(&lighting_diagnostics_text(lighting));
    } else if let Some(details) = lighting_diagnostics_details.as_deref() {
        caps_text.push_str("\n\n");
        caps_text.push_str(details);
    }
    let cpu_access_text = cpu_access_report_text(&cpu_caps);
    if !cpu_access_text.is_empty() {
        caps_text.push_str("\n\n");
        caps_text.push_str(&cpu_access_text);
    }
    if !warnings.is_empty() {
        caps_text.push_str("\n\nWarnings:");
        for w in &warnings {
            caps_text.push_str(&format!("\n  {w}"));
        }
    }

    Ok((
        telemetry,
        cpu,
        cpu_caps,
        caps,
        caps_text,
        warnings,
        profile,
        gpu_mode,
        battery_limit,
        lighting,
        fan_state,
    ))
}

async fn apply_lighting(p: PendingLighting) -> Result<(), String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| format!("session DBus unavailable: {e}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|e| format!("failed to connect to daemon proxy: {e}"))?;

    let m = lighting_request_to_dbus(p);
    proxy
        .set_lighting(m)
        .await
        .map_err(|e| format!("daemon SetLighting failed: {e}"))?;
    Ok(())
}

fn lighting_request_to_dbus(p: PendingLighting) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    if let Some(brightness) = p.brightness {
        m.insert(
            dbus_keys::lighting::BRIGHTNESS.to_string(),
            OwnedValue::from(brightness),
        );
    }
    if let Some(mode) = p.mode {
        m.insert(dbus_keys::lighting::MODE.to_string(), ov(mode));
    }
    if let Some(rgb) = p.rgb_hex {
        m.insert(dbus_keys::lighting::RGB_HEX.to_string(), ov(rgb));
    }
    if let Some(rgb) = p.secondary_rgb_hex {
        m.insert(dbus_keys::lighting::SECONDARY_RGB_HEX.to_string(), ov(rgb));
    }
    if let Some(speed) = p.speed {
        m.insert(dbus_keys::lighting::SPEED.to_string(), ov(speed));
    }
    if let Some(direction) = p.direction {
        m.insert(dbus_keys::lighting::DIRECTION.to_string(), ov(direction));
    }
    if let Some(zone) = p.zone {
        m.insert("zone".to_string(), ov(zone));
    }
    m
}

async fn apply_fan_action(action: PendingFanAction) -> Result<(), String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| format!("session DBus unavailable: {e}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|e| format!("failed to connect to daemon proxy: {e}"))?;

    match action {
        PendingFanAction::Auto { fan_id } => proxy
            .set_fan_auto(&fan_id)
            .await
            .map_err(|e| format!("daemon SetFanAuto failed: {e}"))?,
        PendingFanAction::ManualPercent { fan_id, percent } => proxy
            .set_fan_manual_percent(&fan_id, percent as u64)
            .await
            .map_err(|e| format!("daemon SetFanManualPercent failed: {e}"))?,
        PendingFanAction::Boost {
            fan_id,
            duration_seconds,
        } => proxy
            .set_fan_boost(&fan_id, 100, duration_seconds)
            .await
            .map_err(|e| format!("daemon SetFanBoost failed: {e}"))?,
        PendingFanAction::Curve { fan_id, points } => {
            let rows = points
                .into_iter()
                .map(|(temp_c, duty_percent)| {
                    let mut row = HashMap::new();
                    row.insert(
                        dbus_keys::fan_curves::TEMP_C.to_string(),
                        OwnedValue::from(u64::from(temp_c)),
                    );
                    row.insert(
                        dbus_keys::fan_curves::SPEED_PERCENT.to_string(),
                        OwnedValue::from(u64::from(duty_percent)),
                    );
                    row
                })
                .collect::<Vec<_>>();
            let mut curve = HashMap::new();
            curve.insert(dbus_keys::fan_curves::POINTS.to_string(), ov(rows));
            proxy
                .set_fan_curve(&fan_id, curve)
                .await
                .map_err(|e| format!("daemon SetFanCurve failed: {e}"))?;
        }
        PendingFanAction::Sync(enabled) => proxy
            .set_fan_sync(enabled)
            .await
            .map_err(|e| format!("daemon SetFanSync failed: {e}"))?,
    }
    Ok(())
}

async fn apply_profile(profile: String) -> Result<(), String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| format!("session DBus unavailable: {e}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|e| format!("failed to connect to daemon proxy: {e}"))?;
    proxy
        .set_profile(&profile)
        .await
        .map_err(|e| format!("daemon SetProfile failed: {e}"))?;
    Ok(())
}

async fn apply_gpu_mode(mode: String) -> Result<(), String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| format!("session DBus unavailable: {e}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|e| format!("failed to connect to daemon proxy: {e}"))?;
    proxy
        .set_gpu_mode(&mode)
        .await
        .map_err(|e| format!("daemon SetGpuMode failed: {e}"))?;
    Ok(())
}

async fn apply_battery_limit(limit: u8) -> Result<(), String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| format!("session DBus unavailable: {e}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|e| format!("failed to connect to daemon proxy: {e}"))?;
    proxy
        .set_battery_limit(limit as u64)
        .await
        .map_err(|e| format!("daemon SetBatteryLimit failed: {e}"))?;
    Ok(())
}

async fn apply_cpu_actions(actions: Vec<PendingCpuAction>) -> Result<(), String> {
    if actions.is_empty() {
        return Ok(());
    }

    let conn = zbus::Connection::session()
        .await
        .map_err(|e| format!("session DBus unavailable: {e}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|e| format!("failed to connect to daemon proxy: {e}"))?;

    for action in actions {
        match action {
            PendingCpuAction::Turbo(enabled) => {
                proxy
                    .set_cpu_turbo(enabled)
                    .await
                    .map_err(|e| format!("daemon SetCpuTurbo failed: {e}"))?;
            }
            PendingCpuAction::PowerMode(mode) => {
                proxy
                    .set_cpu_power_mode(&mode)
                    .await
                    .map_err(|e| format!("daemon SetCpuPowerMode failed: {e}"))?;
            }
            PendingCpuAction::Governor(governor) => {
                proxy
                    .set_cpu_governor(&governor)
                    .await
                    .map_err(|e| format!("daemon SetCpuGovernor failed: {e}"))?;
            }
            PendingCpuAction::Epp(epp) => {
                proxy
                    .set_cpu_epp(&epp)
                    .await
                    .map_err(|e| format!("daemon SetCpuEpp failed: {e}"))?;
            }
            PendingCpuAction::FreqLimits { min_mhz, max_mhz } => {
                let min_mhz = min_mhz.unwrap_or(0) as u64;
                let max_mhz = max_mhz.unwrap_or(0) as u64;
                if min_mhz > 0 && max_mhz > 0 && min_mhz > max_mhz {
                    return Err(format!(
                        "min frequency ({min_mhz} MHz) cannot exceed max frequency ({max_mhz} MHz)"
                    ));
                }
                proxy
                    .set_cpu_freq_limits(min_mhz, max_mhz)
                    .await
                    .map_err(|e| format!("daemon SetCpuFreqLimits failed: {e}"))?;
            }
            PendingCpuAction::CoreOnline { core_id, online } => {
                proxy
                    .set_cpu_core_online(core_id as u64, online)
                    .await
                    .map_err(|e| format!("daemon SetCpuCoreOnline failed: {e}"))?;
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
enum UpdateActionOutcome {
    Replaced { version: String },
    OpenReleasePage { url: String, message: String },
}

#[derive(Debug, Deserialize)]
struct GithubReleaseResponse {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    assets: Vec<GithubReleaseAssetResponse>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAssetResponse {
    name: String,
    browser_download_url: String,
}

#[derive(Debug)]
struct UpdateCapability {
    asset: Option<ReleaseAsset>,
    supports_in_place_update: bool,
    explanation: String,
}

async fn check_for_updates(app_metadata: &AppMetadata) -> Result<UpdateState, String> {
    let repository_url = app_metadata
        .repository_url
        .as_deref()
        .ok_or_else(|| "Repository URL is not configured.".to_string())?;
    let (owner, repo) = github_repo_from_url(repository_url)
        .ok_or_else(|| "Repository URL is not a supported GitHub repository.".to_string())?;

    let client = build_github_client(app_metadata)?;
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| "Unable to reach GitHub right now.".to_string())?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("No published GitHub releases were found for this project yet.".to_string());
    }
    if !response.status().is_success() {
        return Err(format!(
            "GitHub returned {} while checking the latest release.",
            response.status()
        ));
    }

    let release = response
        .json::<GithubReleaseResponse>()
        .await
        .map_err(|_| "GitHub returned an unexpected release response.".to_string())?;

    let last_checked_unix = now_unix_timestamp();
    let latest_version = release.tag_name.trim().to_string();
    let release_notes_full = trim_release_notes(release.body);
    let release_notes_preview = release_notes_full
        .as_deref()
        .map(release_notes_preview_text);
    let comparison = compare_versions(&app_metadata.version, &release.tag_name);
    let capability = detect_update_capability(&release.assets);

    let (status_text, last_result_text, update_action_label, update_action_subtitle) =
        match comparison {
            Some(std::cmp::Ordering::Less) => {
                let action_label = if capability.supports_in_place_update {
                    "Update Now".to_string()
                } else {
                    "Download Latest".to_string()
                };
                let action_subtitle = if capability.supports_in_place_update {
                    capability.explanation.clone()
                } else {
                    format!(
                        "{} Opening the latest release page is still available.",
                        capability.explanation
                    )
                };
                (
                    format!("Update available: {latest_version}"),
                    format!("Latest GitHub release: {latest_version}"),
                    action_label,
                    action_subtitle,
                )
            }
            Some(std::cmp::Ordering::Equal) => (
                "You are up to date".to_string(),
                format!(
                    "Installed version {} matches the latest GitHub release.",
                    app_metadata.version
                ),
                "Open Releases".to_string(),
                "Review release notes or download another build from GitHub.".to_string(),
            ),
            Some(std::cmp::Ordering::Greater) => (
                "Installed build is newer than the latest release".to_string(),
                format!(
                    "Installed version {} is newer than the latest published release {latest_version}.",
                    app_metadata.version
                ),
                "Open Releases".to_string(),
                "Review the latest published release in your browser.".to_string(),
            ),
            None => (
                "Latest release found, but versions could not be compared".to_string(),
                format!(
                    "Latest GitHub release: {latest_version}. The installed and latest version strings could not be compared automatically."
                ),
                "Open Releases".to_string(),
                "Version comparison was inconclusive, so the latest release page will open instead of attempting an in-place update."
                    .to_string(),
            ),
        };

    Ok(UpdateState {
        latest_version: Some(latest_version),
        status_text,
        last_result_text,
        last_checked_unix: Some(last_checked_unix),
        release_notes_preview,
        release_notes_full,
        release_page_url: Some(release.html_url),
        downloadable_asset: if capability.supports_in_place_update {
            capability.asset
        } else {
            None
        },
        update_action_label,
        update_action_subtitle,
        supports_in_place_update: capability.supports_in_place_update,
    })
}

fn initial_update_state(app_metadata: &AppMetadata) -> UpdateState {
    let mut state = UpdateState {
        release_page_url: app_metadata.releases_url.clone(),
        ..UpdateState::default()
    };
    if app_metadata.releases_url.is_none() {
        state.status_text = "Repository URL is not configured.".to_string();
        state.last_result_text =
            "Manual update checks are unavailable because the repository URL is missing."
                .to_string();
        state.update_action_subtitle = "Repository URL is not configured.".to_string();
    }
    state
}

fn update_check_failed_state(
    app_metadata: &AppMetadata,
    error: &str,
    previous_state: &UpdateState,
) -> UpdateState {
    let mut state = previous_state.clone();
    if state.release_page_url.is_none() {
        state.release_page_url = app_metadata.releases_url.clone();
    }
    if state.update_action_label.is_empty() {
        state.update_action_label = "Open Releases".to_string();
    }
    state.status_text = "Could not check for updates".to_string();
    state.last_checked_unix = Some(now_unix_timestamp());
    state.last_result_text = friendly_update_error(error);
    state
}

async fn apply_update_action(
    app_metadata: &AppMetadata,
    update_state: &UpdateState,
) -> Result<UpdateActionOutcome, String> {
    let release_page_url = update_state
        .release_page_url
        .clone()
        .or_else(|| app_metadata.releases_url.clone())
        .ok_or_else(|| "Repository URL is not configured.".to_string())?;

    if !update_state.supports_in_place_update {
        return Ok(UpdateActionOutcome::OpenReleasePage {
            url: release_page_url,
            message:
                "Automatic in-place update is not supported for this installation. Opening the latest release page instead."
                    .to_string(),
        });
    }

    let asset = update_state.downloadable_asset.as_ref().ok_or_else(|| {
        "No matching direct release asset was found for automatic replacement.".to_string()
    })?;
    let Some(executable_path) = supported_update_target_path(asset) else {
        return Ok(UpdateActionOutcome::OpenReleasePage {
            url: release_page_url,
            message:
                "Automatic in-place update is not supported for this installation. Opening the latest release page instead."
                    .to_string(),
        });
    };

    let client = build_github_client(app_metadata)?;
    let response = client
        .get(&asset.download_url)
        .send()
        .await
        .map_err(|_| "Unable to download the latest release asset right now.".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "GitHub returned {} while downloading the latest release asset.",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|_| "The latest release asset could not be downloaded completely.".to_string())?;
    if bytes.is_empty() {
        return Ok(UpdateActionOutcome::OpenReleasePage {
            url: release_page_url,
            message:
                "The latest release asset was empty, so automatic replacement was skipped. Opening the latest release page instead."
                    .to_string(),
        });
    }
    if !bytes.starts_with(b"\x7fELF") {
        return Ok(UpdateActionOutcome::OpenReleasePage {
            url: release_page_url,
            message:
                "The latest release asset could not be validated as a Linux executable. Opening the latest release page instead."
                    .to_string(),
        });
    }

    let current_permissions = fs::metadata(&executable_path)
        .map_err(|_| "The current executable permissions could not be read.".to_string())?
        .permissions();
    let parent = executable_path
        .parent()
        .ok_or_else(|| "The current executable directory could not be determined.".to_string())?;
    let temp_path = temp_update_path(parent, &asset.name);
    if fs::write(&temp_path, &bytes).is_err() {
        return Ok(UpdateActionOutcome::OpenReleasePage {
            url: release_page_url,
            message:
                "The downloaded update could not be written next to the current binary. Opening the latest release page instead."
                    .to_string(),
        });
    }
    if fs::set_permissions(&temp_path, current_permissions).is_err() {
        let _ = fs::remove_file(&temp_path);
        return Ok(UpdateActionOutcome::OpenReleasePage {
            url: release_page_url,
            message:
                "Execute permissions could not be preserved for the downloaded update. Opening the latest release page instead."
                    .to_string(),
        });
    }
    if fs::rename(&temp_path, &executable_path).is_err() {
        let _ = fs::remove_file(&temp_path);
        return Ok(UpdateActionOutcome::OpenReleasePage {
            url: release_page_url,
            message:
                "The downloaded update could not replace the current binary. Opening the latest release page instead."
                    .to_string(),
        });
    }

    Ok(UpdateActionOutcome::Replaced {
        version: update_state
            .latest_version
            .clone()
            .unwrap_or_else(|| asset.name.clone()),
    })
}

fn build_github_client(app_metadata: &AppMetadata) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(format!("rog-helper-ui/{}", app_metadata.version))
        .build()
        .map_err(|_| "The update checker could not initialize its GitHub client.".to_string())
}

fn detect_update_capability(assets: &[GithubReleaseAssetResponse]) -> UpdateCapability {
    let executable_path = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => {
            return UpdateCapability {
                asset: None,
                supports_in_place_update: false,
                explanation:
                    "The current executable path could not be determined for automatic replacement."
                        .to_string(),
            };
        }
    };
    let executable_name = match executable_path.file_name().and_then(|name| name.to_str()) {
        Some(name) => name,
        None => {
            return UpdateCapability {
                asset: None,
                supports_in_place_update: false,
                explanation:
                    "The current executable name could not be determined for automatic replacement."
                        .to_string(),
            };
        }
    };

    let asset = match matching_release_asset(assets, executable_name) {
        Some(asset) => asset,
        None => {
            return UpdateCapability {
                asset: None,
                supports_in_place_update: false,
                explanation:
                    "No direct release asset matches the current binary name, so automatic replacement is disabled."
                        .to_string(),
            };
        }
    };

    let Some(home_dir) = std::env::var_os("HOME").map(PathBuf::from) else {
        return UpdateCapability {
            asset: None,
            supports_in_place_update: false,
            explanation:
                "The HOME directory is not available, so a safe user-local update path could not be confirmed."
                    .to_string(),
        };
    };
    if !executable_path.starts_with(&home_dir) {
        return UpdateCapability {
            asset: None,
            supports_in_place_update: false,
            explanation:
                "The current binary is not installed in a user-local path, so in-place replacement is disabled."
                    .to_string(),
        };
    }

    let metadata = match fs::symlink_metadata(&executable_path) {
        Ok(metadata) => metadata,
        Err(_) => {
            return UpdateCapability {
                asset: None,
                supports_in_place_update: false,
                explanation:
                    "The current executable could not be inspected, so in-place replacement is disabled."
                        .to_string(),
            };
        }
    };
    if metadata.file_type().is_symlink() {
        return UpdateCapability {
            asset: None,
            supports_in_place_update: false,
            explanation:
                "The current executable is a symlink, so automatic replacement is disabled."
                    .to_string(),
        };
    }

    let Some(parent) = executable_path.parent() else {
        return UpdateCapability {
            asset: None,
            supports_in_place_update: false,
            explanation:
                "The current executable directory could not be determined, so in-place replacement is disabled."
                    .to_string(),
        };
    };
    if !can_stage_update_in_dir(parent, executable_name) {
        return UpdateCapability {
            asset: None,
            supports_in_place_update: false,
            explanation:
                "The current executable directory is not writable by the current user, so in-place replacement is disabled."
                    .to_string(),
        };
    }

    UpdateCapability {
        asset: Some(asset),
        supports_in_place_update: true,
        explanation:
            "A matching direct binary release asset can safely replace this user-local installation after validation. Restart rog-helper afterward."
                .to_string(),
    }
}

fn supported_update_target_path(asset: &ReleaseAsset) -> Option<PathBuf> {
    let executable_path = std::env::current_exe().ok()?;
    let executable_name = executable_path.file_name()?.to_str()?;
    if executable_name != asset.name {
        return None;
    }

    let home_dir = std::env::var_os("HOME").map(PathBuf::from)?;
    if !executable_path.starts_with(&home_dir) {
        return None;
    }

    let metadata = fs::symlink_metadata(&executable_path).ok()?;
    if metadata.file_type().is_symlink() {
        return None;
    }

    let parent = executable_path.parent()?;
    if !can_stage_update_in_dir(parent, executable_name) {
        return None;
    }

    Some(executable_path)
}

fn matching_release_asset(
    assets: &[GithubReleaseAssetResponse],
    executable_name: &str,
) -> Option<ReleaseAsset> {
    assets
        .iter()
        .find(|asset| asset.name == executable_name)
        .map(|asset| ReleaseAsset {
            name: asset.name.clone(),
            download_url: asset.browser_download_url.clone(),
        })
}

fn can_stage_update_in_dir(dir: &Path, executable_name: &str) -> bool {
    let probe_path = temp_update_path(dir, executable_name);
    let write_result = fs::write(&probe_path, []);
    let _ = fs::remove_file(&probe_path);
    write_result.is_ok()
}

fn temp_update_path(dir: &Path, executable_name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    dir.join(format!(
        ".{executable_name}.rog-helper-update-{}-{stamp}",
        std::process::id()
    ))
}

fn package_value(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn package_authors() -> String {
    if env!("CARGO_PKG_AUTHORS").trim().is_empty() {
        APP_AUTHORS_FALLBACK.to_string()
    } else {
        env!("CARGO_PKG_AUTHORS").replace(':', ", ")
    }
}

fn detect_repository_url() -> Option<String> {
    let manifest_repository = env!("CARGO_PKG_REPOSITORY").trim();
    if !manifest_repository.is_empty() {
        return canonicalize_git_remote_url(manifest_repository)
            .or_else(|| Some(manifest_repository.to_string()));
    }

    detect_git_remote_repository_url().or_else(|| {
        canonicalize_git_remote_url(APP_REPOSITORY_FALLBACK_URL)
            .or_else(|| Some(APP_REPOSITORY_FALLBACK_URL.to_string()))
    })
}

fn detect_git_remote_repository_url() -> Option<String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    if !manifest_dir.exists() {
        return None;
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(manifest_dir)
        .arg("config")
        .arg("--get")
        .arg("remote.origin.url")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    canonicalize_git_remote_url(raw.trim())
}

fn canonicalize_git_remote_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let rest = trimmed
        .strip_prefix("git@github.com:")
        .or_else(|| trimmed.strip_prefix("ssh://git@github.com/"))
        .or_else(|| trimmed.strip_prefix("https://github.com/"))
        .or_else(|| trimmed.strip_prefix("http://github.com/"))?;

    let canonical = rest.trim_end_matches(".git").trim_end_matches('/');
    Some(format!("https://github.com/{canonical}"))
}

fn github_repo_from_url(url: &str) -> Option<(String, String)> {
    let canonical = canonicalize_git_remote_url(url)?;
    let path = canonical.strip_prefix("https://github.com/")?;
    let mut parts = path.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

fn detect_maintainer_name(authors: &str, repository_url: Option<&str>) -> String {
    let preferred_author = authors
        .split(',')
        .map(str::trim)
        .filter(|author| !author.is_empty())
        .find(|author| {
            let normalized = author.to_ascii_lowercase();
            !normalized.contains("contributors")
        })
        .map(|author| {
            author
                .split('<')
                .next()
                .unwrap_or(author)
                .trim()
                .to_string()
        });

    preferred_author
        .or_else(|| {
            repository_url
                .and_then(github_repo_from_url)
                .map(|(owner, _)| owner)
        })
        .unwrap_or_else(|| APP_AUTHORS_FALLBACK.to_string())
}

fn release_notes_preview_text(notes: &str) -> String {
    let preview = notes
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join(" ");
    truncate_text(&preview, 220)
}

fn trim_release_notes(body: Option<String>) -> Option<String> {
    body.and_then(|body| {
        let trimmed = body.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let mut truncated = text.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn compare_versions(current: &str, latest: &str) -> Option<std::cmp::Ordering> {
    let current = parse_version_for_compare(current)?;
    let latest = parse_version_for_compare(latest)?;
    Some(current.cmp(&latest))
}

fn parse_version_for_compare(raw: &str) -> Option<semver::Version> {
    let trimmed = raw.trim();
    let start = trimmed.find(|ch: char| ch.is_ascii_digit())?;
    let tail = &trimmed[start..];
    let end = tail
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '+')))
        .unwrap_or(tail.len());
    let candidate = &tail[..end];

    let suffix_start = candidate.find(['-', '+']).unwrap_or(candidate.len());
    let core = &candidate[..suffix_start];
    let suffix = &candidate[suffix_start..];

    let parts = core.split('.').collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    if parts
        .iter()
        .any(|part| part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()))
    {
        return None;
    }

    let mut normalized_parts = parts;
    while normalized_parts.len() < 3 {
        normalized_parts.push("0");
    }

    semver::Version::parse(&format!(
        "{}.{}.{}{}",
        normalized_parts[0], normalized_parts[1], normalized_parts[2], suffix
    ))
    .ok()
}

fn now_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn format_timestamp_local(timestamp: i64) -> String {
    if let Ok(datetime) = glib::DateTime::from_unix_local(timestamp) {
        if let Ok(formatted) = datetime.format("%Y-%m-%d %H:%M:%S") {
            return formatted.to_string();
        }
    }
    format!("unix:{timestamp}")
}

fn load_app_settings() -> AppConfig {
    let mut loaded = match config_path() {
        Ok(path) if path.exists() => load_config(&path),
        Ok(_) => legacy_ui_config_path()
            .ok()
            .and_then(|path| fs::read_to_string(path).ok())
            .map(|contents| parse_legacy_ui_config(&contents))
            .unwrap_or_else(|| rog_core::ConfigLoad {
                config: AppConfig::default(),
                source: rog_core::ConfigSource::Defaults,
                warnings: Vec::new(),
            }),
        Err(error) => {
            warn!("{error}");
            return AppConfig::default();
        }
    };
    for warning in loaded.warnings.drain(..) {
        warn!("{warning}");
    }
    if let Ok(enabled) = autostart_enabled() {
        loaded.config.ui.launch_on_login = enabled;
    }
    loaded.config
}

fn persist_settings_change<F>(
    shared: &Arc<Mutex<SharedUiState>>,
    update: F,
) -> Result<AppConfig, String>
where
    F: FnOnce(&mut AppConfig),
{
    let current = shared
        .lock()
        .map_err(|_| "Unable to update settings right now.".to_string())?
        .settings
        .clone();
    let mut next = current.clone();
    update(&mut next);
    validate_config(&next)?;
    if current.ui.launch_on_login != next.ui.launch_on_login
        || current.ui.start_minimized_to_tray != next.ui.start_minimized_to_tray
    {
        sync_autostart(&next.ui)?;
    }

    let mut st = shared
        .lock()
        .map_err(|_| "Unable to update settings right now.".to_string())?;
    st.settings = next.clone();
    st.pending_config_save = Some(next.clone());
    st.mark_render_dirty();
    Ok(next)
}

fn autostart_file_path() -> Result<PathBuf, String> {
    Ok(xdg_config_home()?
        .join("autostart")
        .join(AUTOSTART_DESKTOP_FILE_NAME))
}

fn xdg_config_home() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME") {
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }

    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config"))
        .ok_or_else(|| "HOME is not set, so settings cannot be saved.".to_string())
}

fn autostart_enabled() -> Result<bool, String> {
    let path = autostart_file_path()?;
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("Unable to read autostart entry: {error}")),
    };

    let mut disabled = false;
    for line in contents.lines().map(str::trim) {
        if key_value_is(line, "Hidden", "true")
            || key_value_is(line, "X-GNOME-Autostart-enabled", "false")
        {
            disabled = true;
            break;
        }
    }

    Ok(!disabled)
}

fn key_value_is(line: &str, key: &str, expected_value: &str) -> bool {
    let Some((raw_key, raw_value)) = line.split_once('=') else {
        return false;
    };
    raw_key.trim().eq_ignore_ascii_case(key)
        && raw_value.trim().eq_ignore_ascii_case(expected_value)
}

fn sync_autostart(settings: &rog_core::UiPreferences) -> Result<(), String> {
    let path = autostart_file_path()?;
    if !settings.launch_on_login {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Unable to remove autostart entry: {error}")),
        }
        return Ok(());
    }

    let parent = path
        .parent()
        .ok_or_else(|| "Unable to determine the autostart directory.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Unable to create autostart directory: {error}"))?;
    fs::write(&path, autostart_desktop_entry(settings))
        .map_err(|error| format!("Unable to save autostart entry: {error}"))
}

fn autostart_desktop_entry(settings: &rog_core::UiPreferences) -> String {
    let exec = if settings.start_minimized_to_tray {
        format!("{APP_BINARY_NAME} {START_MINIMIZED_ARG}")
    } else {
        APP_BINARY_NAME.to_string()
    };

    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=rog-helper\n\
         Comment=Control ASUS ROG power, GPU, battery, and diagnostics on Linux\n\
         Exec={exec}\n\
         Icon={APP_ICON_NAME}\n\
         Terminal=false\n\
         StartupNotify=false\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

fn open_uri_with_feedback(shared: &Arc<Mutex<SharedUiState>>, uri: &str, error_message: &str) {
    if uri.trim().is_empty() {
        if let Ok(mut st) = shared.lock() {
            st.pending_toast = Some((error_message.to_string(), true));
        }
        return;
    }

    if let Err(error) =
        gtk::gio::AppInfo::launch_default_for_uri(uri, None::<&gtk::gio::AppLaunchContext>)
    {
        warn!("failed to open external URI {uri}: {error}");
        if let Ok(mut st) = shared.lock() {
            st.pending_toast = Some((error_message.to_string(), true));
        }
    }
}

fn friendly_update_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("repository url is not configured") {
        "Repository URL is not configured.".to_string()
    } else if lower.contains("supported github repository") {
        "The repository URL is not a supported GitHub repository.".to_string()
    } else if lower.contains("no published github releases") {
        "No published GitHub releases were found for this project yet.".to_string()
    } else if lower.contains("unable to reach github") {
        "Unable to reach GitHub right now.".to_string()
    } else if lower.contains("unexpected release response") {
        "GitHub returned an unexpected release response.".to_string()
    } else if lower.contains("github returned") {
        "GitHub did not accept the update check right now.".to_string()
    } else {
        "Unable to check for updates right now.".to_string()
    }
}

fn ov<T>(v: T) -> OwnedValue
where
    T: Into<Value<'static>>,
{
    OwnedValue::try_from(v.into()).expect("OwnedValue conversion should succeed")
}

fn summary_from_state(
    t: &TelemetrySnapshot,
    profile: Option<&str>,
    gpu_mode: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    if let Some(p) = t.power_source {
        parts.push(power_source_label(p).to_string());
    }
    if let Some(b) = t.battery_percent {
        parts.push(format!("{b:.0}%"));
    }
    if let Some(c) = t.cpu_temp_c {
        parts.push(format!("CPU {c:.0}C"));
    }
    if let Some(g) = t.gpu_temp_c {
        parts.push(format!("GPU {g:.0}C"));
    }
    if let Some(profile) = profile.filter(|value| !value.trim().is_empty()) {
        parts.push(format!("Profile {profile}"));
    }
    if let Some(gpu_mode) = gpu_mode.filter(|value| !value.trim().is_empty()) {
        parts.push(format!("Mode {gpu_mode}"));
    }
    if parts.is_empty() {
        "rog-helper".to_string()
    } else {
        parts.join(" | ")
    }
}

fn power_source_label(source: PowerSource) -> &'static str {
    match source {
        PowerSource::Ac => "AC",
        PowerSource::Battery => "Battery",
    }
}

fn push_history(buf: &mut Vec<f32>, value: Option<f32>, max_samples: usize) {
    let Some(value) = value else {
        return;
    };
    if !value.is_finite() {
        return;
    }
    buf.push(value);
    if buf.len() > max_samples {
        let drop_n = buf.len() - max_samples;
        buf.drain(..drop_n);
    }
}

fn telemetry_from_dbus(map: HashMap<String, OwnedValue>) -> TelemetrySnapshot {
    let ts = dbus_decode::unsigned(&map, dbus_keys::telemetry::TIMESTAMP_MS).unwrap_or(0);

    let mut t = TelemetrySnapshot::empty_now(ts);

    if let Some(v) = dbus_decode::float(&map, dbus_keys::telemetry::CPU_TEMP_C) {
        t.cpu_temp_c = Some(v as f32);
    }
    if let Some(v) = dbus_decode::float(&map, dbus_keys::telemetry::GPU_TEMP_C) {
        t.gpu_temp_c = Some(v as f32);
    }
    if let Some(v) = dbus_decode::float(&map, dbus_keys::telemetry::GPU_USAGE_PERCENT) {
        t.gpu_usage_percent = Some(v as f32);
    }
    t.gpu_vram_used_bytes = dbus_decode::unsigned(&map, dbus_keys::telemetry::GPU_VRAM_USED_BYTES);
    t.gpu_vram_total_bytes =
        dbus_decode::unsigned(&map, dbus_keys::telemetry::GPU_VRAM_TOTAL_BYTES);
    t.gpu_core_clock_mhz =
        dbus_decode::unsigned_u32(&map, dbus_keys::telemetry::GPU_CORE_CLOCK_MHZ);
    t.gpu_memory_clock_mhz =
        dbus_decode::unsigned_u32(&map, dbus_keys::telemetry::GPU_MEMORY_CLOCK_MHZ);
    if let Some(v) = dbus_decode::float(&map, dbus_keys::telemetry::GPU_POWER_W) {
        t.gpu_power_w = Some(v as f32);
    }
    t.gpu_index = dbus_decode::unsigned_u32(&map, dbus_keys::telemetry::GPU_INDEX);
    t.gpu_name = dbus_decode::string(&map, dbus_keys::telemetry::GPU_NAME);
    t.gpu_uuid = dbus_decode::string(&map, dbus_keys::telemetry::GPU_UUID);
    t.gpu_pci_bus_id = dbus_decode::string(&map, dbus_keys::telemetry::GPU_PCI_BUS_ID);
    t.gpu_telemetry_provider =
        dbus_decode::string(&map, dbus_keys::telemetry::GPU_TELEMETRY_PROVIDER);
    t.gpu_telemetry_status = dbus_decode::string(&map, dbus_keys::telemetry::GPU_TELEMETRY_STATUS);
    if let Some(temps) = map
        .get(dbus_keys::telemetry::TEMPS_C)
        .cloned()
        .and_then(|v| HashMap::<String, f64>::try_from(v).ok())
    {
        for (k, v) in temps {
            t.temps_c.insert(k, v as f32);
        }
    }
    if let Some(fans) = map
        .get(dbus_keys::telemetry::FANS_RPM)
        .cloned()
        .and_then(|v| HashMap::<String, u32>::try_from(v).ok())
    {
        for (k, v) in fans {
            t.fans_rpm.insert(k, v);
        }
    }
    if let Some(rows) = dbus_decode::rows(&map, dbus_keys::TELEMETRY_FAN_ROWS_KEY) {
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            if let Some(fan) = fan_row_from_dbus(&row) {
                out.push(fan);
            }
        }
        t.fan_rows = out;
    } else if !t.fans_rpm.is_empty() {
        t.fan_rows = t
            .fans_rpm
            .iter()
            .map(|(label, rpm)| FanTelemetry {
                hwmon_device: String::new(),
                hwmon_path: String::new(),
                input_path: String::new(),
                raw_label: None,
                display_label: label.clone(),
                rpm: Some(*rpm),
            })
            .collect();
    }
    if let Some(v) = dbus_decode::float(&map, dbus_keys::telemetry::BATTERY_PERCENT) {
        t.battery_percent = Some(v as f32);
    }
    if let Some(v) = dbus_decode::boolean(&map, dbus_keys::telemetry::AC_ONLINE) {
        t.ac_online = Some(v);
    }
    if let Some(src) = map
        .get(dbus_keys::telemetry::POWER_SOURCE)
        .and_then(|v| <&str>::try_from(v).ok())
    {
        t.power_source = match src {
            "Ac" => Some(PowerSource::Ac),
            "Battery" => Some(PowerSource::Battery),
            _ => None,
        };
    }

    if let Some(src) = map
        .get("battery_state")
        .and_then(|v| <&str>::try_from(v).ok())
    {
        t.battery_state = Some(match src {
            "Charging" => BatteryState::Charging,
            "Discharging" => BatteryState::Discharging,
            "Empty" => BatteryState::Empty,
            "Full" => BatteryState::Full,
            "PendingCharge" => BatteryState::PendingCharge,
            "PendingDischarge" => BatteryState::PendingDischarge,
            "NotCharging" => BatteryState::NotCharging,
            _ => BatteryState::Unknown,
        });
    }

    if let Some(v) = map
        .get("battery_health_percent")
        .and_then(|v| f64::try_from(v).ok())
    {
        t.battery_health_percent = Some(v as f32);
    }

    if let Some(v) = map
        .get("battery_cycle_count")
        .and_then(u64_from_value)
        .and_then(|v| u32::try_from(v).ok())
    {
        t.battery_cycle_count = Some(v);
    }

    if let Some(v) = map
        .get("battery_charge_power_w")
        .and_then(|v| f64::try_from(v).ok())
    {
        t.battery_charge_power_w = Some(v as f32);
    }

    if let Some(v) = map
        .get("battery_discharge_power_w")
        .and_then(|v| f64::try_from(v).ok())
    {
        t.battery_discharge_power_w = Some(v as f32);
    }

    t.battery_time_to_empty_s = map.get("battery_time_to_empty_s").and_then(u64_from_value);
    t.battery_time_to_full_s = map.get("battery_time_to_full_s").and_then(u64_from_value);

    // Memory.
    t.mem_total_bytes = map.get("mem_total_bytes").and_then(u64_from_value);
    t.mem_used_bytes = map.get("mem_used_bytes").and_then(u64_from_value);
    if let Some(v) = map
        .get("mem_used_percent")
        .and_then(|v| f64::try_from(v).ok())
    {
        t.mem_used_percent = Some(v as f32);
    }
    t.mem_available_bytes = map.get("mem_available_bytes").and_then(u64_from_value);
    t.mem_free_bytes = map.get("mem_free_bytes").and_then(u64_from_value);
    t.mem_cached_bytes = map.get("mem_cached_bytes").and_then(u64_from_value);
    t.mem_buffers_bytes = map.get("mem_buffers_bytes").and_then(u64_from_value);
    t.mem_shared_bytes = map.get("mem_shared_bytes").and_then(u64_from_value);
    t.mem_anon_bytes = map.get("mem_anon_bytes").and_then(u64_from_value);

    t.swap_total_bytes = map.get("swap_total_bytes").and_then(u64_from_value);
    t.swap_used_bytes = map.get("swap_used_bytes").and_then(u64_from_value);
    t.swap_free_bytes = map.get("swap_free_bytes").and_then(u64_from_value);
    if let Some(v) = map
        .get("swap_in_pages_per_s")
        .and_then(|v| f64::try_from(v).ok())
    {
        t.swap_in_pages_per_s = Some(v as f32);
    }
    if let Some(v) = map
        .get("swap_out_pages_per_s")
        .and_then(|v| f64::try_from(v).ok())
    {
        t.swap_out_pages_per_s = Some(v as f32);
    }
    t.zram_total_bytes = map.get("zram_total_bytes").and_then(u64_from_value);
    t.zram_used_bytes = map.get("zram_used_bytes").and_then(u64_from_value);
    t.zswap_enabled = map
        .get("zswap_enabled")
        .and_then(|v| bool::try_from(v).ok());

    if let Some(v) = map
        .get("psi_mem_some_avg10")
        .and_then(|v| f64::try_from(v).ok())
    {
        t.psi_mem_some_avg10 = Some(v as f32);
    }
    if let Some(v) = map
        .get("psi_mem_some_avg60")
        .and_then(|v| f64::try_from(v).ok())
    {
        t.psi_mem_some_avg60 = Some(v as f32);
    }
    if let Some(v) = map
        .get("psi_mem_some_avg300")
        .and_then(|v| f64::try_from(v).ok())
    {
        t.psi_mem_some_avg300 = Some(v as f32);
    }
    if let Some(v) = map
        .get("psi_mem_full_avg10")
        .and_then(|v| f64::try_from(v).ok())
    {
        t.psi_mem_full_avg10 = Some(v as f32);
    }
    if let Some(v) = map
        .get("psi_mem_full_avg60")
        .and_then(|v| f64::try_from(v).ok())
    {
        t.psi_mem_full_avg60 = Some(v as f32);
    }
    if let Some(v) = map
        .get("psi_mem_full_avg300")
        .and_then(|v| f64::try_from(v).ok())
    {
        t.psi_mem_full_avg300 = Some(v as f32);
    }

    t.mem_active_bytes = map.get("mem_active_bytes").and_then(u64_from_value);
    t.mem_inactive_bytes = map.get("mem_inactive_bytes").and_then(u64_from_value);
    t.mem_dirty_bytes = map.get("mem_dirty_bytes").and_then(u64_from_value);
    t.mem_writeback_bytes = map.get("mem_writeback_bytes").and_then(u64_from_value);
    t.mem_slab_bytes = map.get("mem_slab_bytes").and_then(u64_from_value);
    t.mem_sreclaimable_bytes = map.get("mem_sreclaimable_bytes").and_then(u64_from_value);
    t.mem_sunreclaim_bytes = map.get("mem_sunreclaim_bytes").and_then(u64_from_value);
    t.mem_pagetables_bytes = map.get("mem_pagetables_bytes").and_then(u64_from_value);
    t.mem_kernelstack_bytes = map.get("mem_kernelstack_bytes").and_then(u64_from_value);
    t.mem_mapped_bytes = map.get("mem_mapped_bytes").and_then(u64_from_value);

    if let Some(v) = map
        .get("mem_top_processes")
        .and_then(|v| <&str>::try_from(v).ok())
    {
        t.mem_top_processes = Some(v.to_string());
    }

    if let Some(rows) = map
        .get("mem_top_processes_rows")
        .cloned()
        .and_then(|v| Vec::<HashMap<String, OwnedValue>>::try_from(v).ok())
    {
        let mut out = Vec::new();
        for r in rows {
            let user = r
                .get("user")
                .and_then(|v| <&str>::try_from(v).ok())
                .map(|v| v.to_string());
            let pid = r
                .get("pid")
                .and_then(u64_from_value)
                .and_then(|v| u32::try_from(v).ok());
            let rss_bytes = r.get("rss_bytes").and_then(u64_from_value);
            let swap_bytes = r.get("swap_bytes").and_then(u64_from_value);
            let name = r
                .get("name")
                .and_then(|v| <&str>::try_from(v).ok())
                .map(|v| v.to_string());

            let (Some(user), Some(pid), Some(rss_bytes), Some(swap_bytes), Some(name)) =
                (user, pid, rss_bytes, swap_bytes, name)
            else {
                continue;
            };
            out.push(TopProcessMem {
                user,
                pid,
                rss_bytes,
                swap_bytes,
                name,
            });
        }
        if !out.is_empty() {
            t.mem_top_processes_rows = Some(out);
        }
    }

    t
}

fn cpu_telemetry_from_dbus(map: HashMap<String, OwnedValue>) -> CpuTelemetry {
    let timestamp_ms = map
        .get("timestamp_ms")
        .and_then(u64_from_value)
        .unwrap_or_default();
    let mut out = CpuTelemetry::empty_now(timestamp_ms);

    out.temp_c = map
        .get("temp_c")
        .and_then(|v| f64::try_from(v).ok())
        .map(|v| v as f32);
    out.usage_percent = map
        .get("usage_percent")
        .and_then(|v| f64::try_from(v).ok())
        .map(|v| v as f32);
    out.avg_freq_mhz = map
        .get("avg_freq_mhz")
        .and_then(|v| f64::try_from(v).ok())
        .map(|v| v as f32);
    out.package_power_w = map
        .get("package_power_w")
        .and_then(|v| f64::try_from(v).ok())
        .map(|v| v as f32);
    out.status = map
        .get("status")
        .and_then(|v| <&str>::try_from(v).ok())
        .map(|v| v.to_string());
    out.turbo_boost_enabled = map
        .get("turbo_boost_enabled")
        .and_then(|v| bool::try_from(v).ok());
    out.governor = map
        .get("governor")
        .and_then(|v| <&str>::try_from(v).ok())
        .map(|v| v.to_string());
    out.epp = map
        .get("epp")
        .and_then(|v| <&str>::try_from(v).ok())
        .map(|v| v.to_string());
    out.min_freq_mhz = map
        .get("min_freq_mhz")
        .and_then(u64_from_value)
        .and_then(|v| u32::try_from(v).ok());
    out.max_freq_mhz = map
        .get("max_freq_mhz")
        .and_then(u64_from_value)
        .and_then(|v| u32::try_from(v).ok());

    if let Some(rows) = map
        .get(dbus_keys::CPU_TELEMETRY_PER_CORE_KEY)
        .cloned()
        .and_then(|v| Vec::<HashMap<String, OwnedValue>>::try_from(v).ok())
    {
        let mut per_core = Vec::with_capacity(rows.len());
        for row in rows {
            let logical_cpu_id = row
                .get(dbus_keys::CPU_LOGICAL_CPU_ID_KEY)
                .or_else(|| row.get(dbus_keys::CPU_CORE_ID_COMPAT_KEY))
                .and_then(u64_from_value)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or_default();
            per_core.push(rog_core::CpuCoreTelemetry {
                logical_cpu_id,
                physical_core_index: row
                    .get(dbus_keys::CPU_PHYSICAL_CORE_INDEX_KEY)
                    .and_then(u64_from_value)
                    .and_then(|v| u32::try_from(v).ok()),
                policy_id: row
                    .get(dbus_keys::CPU_POLICY_ID_KEY)
                    .and_then(u64_from_value)
                    .and_then(|v| u32::try_from(v).ok()),
                thread_index: row
                    .get(dbus_keys::CPU_THREAD_INDEX_KEY)
                    .and_then(u64_from_value)
                    .and_then(|v| u32::try_from(v).ok()),
                thread_count: row
                    .get(dbus_keys::CPU_THREAD_COUNT_KEY)
                    .and_then(u64_from_value)
                    .and_then(|v| u32::try_from(v).ok()),
                usage_percent: row
                    .get("usage_percent")
                    .and_then(|v| f64::try_from(v).ok())
                    .map(|v| v as f32),
                current_freq_mhz: row
                    .get("current_freq_mhz")
                    .and_then(u64_from_value)
                    .and_then(|v| u32::try_from(v).ok()),
                min_freq_mhz: row
                    .get("min_freq_mhz")
                    .and_then(u64_from_value)
                    .and_then(|v| u32::try_from(v).ok()),
                max_freq_mhz: row
                    .get("max_freq_mhz")
                    .and_then(u64_from_value)
                    .and_then(|v| u32::try_from(v).ok()),
                online: row
                    .get("online")
                    .and_then(|v| bool::try_from(v).ok())
                    .unwrap_or(true),
            });
        }
        out.per_core = per_core;
    }

    out
}

fn u64_from_value(v: &OwnedValue) -> Option<u64> {
    dbus_decode::unsigned_value(v)
}

fn format_duration_short(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

fn format_bytes_gib_opt(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "(n/a)".to_string();
    };
    let gib = (bytes as f64) / (1024.0 * 1024.0 * 1024.0);
    format!("{gib:.1} GiB")
}

fn format_bytes_mib_opt(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "(n/a)".to_string();
    };
    let mib = (bytes as f64) / (1024.0 * 1024.0);
    format!("{mib:.0} MiB")
}

fn format_used_ram(used_bytes: Option<u64>, used_percent: Option<f32>) -> String {
    match (used_bytes, used_percent) {
        (Some(b), Some(p)) => {
            let gib = (b as f64) / (1024.0 * 1024.0 * 1024.0);
            format!("{gib:.1} GiB ({p:.0}%)")
        }
        (Some(b), None) => format_bytes_gib_opt(Some(b)),
        _ => "(n/a)".to_string(),
    }
}

fn format_rate_pages_opt(rate: Option<f32>) -> String {
    let Some(rate) = rate else {
        return "(n/a)".to_string();
    };
    if !rate.is_finite() {
        return "(n/a)".to_string();
    }
    format!("{rate:.0} pages/s")
}

fn format_zram_opt(used_bytes: Option<u64>, total_bytes: Option<u64>) -> String {
    match (used_bytes, total_bytes) {
        (Some(used), Some(total)) if total > 0 => {
            let used_gib = (used as f64) / (1024.0 * 1024.0 * 1024.0);
            let total_gib = (total as f64) / (1024.0 * 1024.0 * 1024.0);
            let pct = (used as f64) / (total as f64) * 100.0;
            format!("{used_gib:.1}/{total_gib:.1} GiB ({pct:.0}%)")
        }
        (Some(used), _) => format_bytes_gib_opt(Some(used)),
        _ => "(n/a)".to_string(),
    }
}

fn format_bool_opt(v: Option<bool>) -> String {
    match v {
        Some(true) => "Yes".to_string(),
        Some(false) => "No".to_string(),
        None => "(n/a)".to_string(),
    }
}

fn format_psi_opt(avg10: Option<f32>, avg60: Option<f32>, avg300: Option<f32>) -> String {
    match (avg10, avg60, avg300) {
        (Some(a10), Some(a60), Some(a300)) => format!("{a10:.2} / {a60:.2} / {a300:.2}"),
        _ => "(n/a)".to_string(),
    }
}

fn format_bytes_human(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

    let b = bytes as f64;
    if b >= 10.0 * GIB {
        format!("{:.0} GiB", b / GIB)
    } else if b >= GIB {
        format!("{:.1} GiB", b / GIB)
    } else if b >= 10.0 * MIB {
        format!("{:.0} MiB", b / MIB)
    } else if b >= MIB {
        format!("{:.1} MiB", b / MIB)
    } else if b >= 10.0 * KIB {
        format!("{:.0} KiB", b / KIB)
    } else if b >= KIB {
        format!("{:.1} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn format_top_processes_text(rows: &[TopProcessMem]) -> String {
    let mut out = String::new();
    out.push_str("PROCESS             RSS       SWAP     PID USER\n");
    out.push_str("------------------------------------------------\n");
    for p in rows {
        out.push_str(&format!(
            "{name:<18} {rss:>9} {swap:>9} {pid:>7} {user}\n",
            name = p.name,
            rss = format_bytes_human(p.rss_bytes),
            swap = format_bytes_human(p.swap_bytes),
            pid = p.pid,
            user = p.user,
        ));
    }
    out
}

fn format_cpu_thread_position(thread_index: Option<u32>, thread_count: Option<u32>) -> String {
    match (thread_index, thread_count) {
        (Some(index), Some(count)) if count > 0 => format!("{index}/{count}"),
        _ => "-".to_string(),
    }
}

fn format_cpu_toggle_summary(
    physical_core_index: Option<u32>,
    thread_index: Option<u32>,
    thread_count: Option<u32>,
) -> String {
    let mut parts = Vec::new();
    if let Some(physical_core_index) = physical_core_index {
        parts.push(format!("physical core {physical_core_index}"));
    }
    let thread_label = format_cpu_thread_position(thread_index, thread_count);
    if thread_label != "-" {
        parts.push(format!("thread {thread_label}"));
    }
    if parts.is_empty() {
        "topology unavailable".to_string()
    } else {
        parts.join(" · ")
    }
}

fn warning_detail<'a>(warnings: &'a [String], prefix: &str) -> Option<&'a str> {
    warnings
        .iter()
        .find_map(|warning| warning.strip_prefix(prefix).map(str::trim))
}

fn feature_status_label(status: FeatureAccessState) -> &'static str {
    match status.contract_status() {
        ContractStatus::Available => "Available",
        ContractStatus::Unsupported => "Not supported on this machine",
        ContractStatus::MissingDependency => "Missing required service",
        ContractStatus::PermissionDenied => "Permission denied",
        ContractStatus::TransientFailure => "Temporarily unavailable",
        ContractStatus::ReadOnly => "Read-only",
        ContractStatus::Unavailable => "Unavailable",
        ContractStatus::Unknown => "Checking support",
    }
}

fn feature_control_subtitle(access: &FeatureAvailability, available_text: &str) -> String {
    if access.is_available() {
        available_text.to_string()
    } else if access.reason.is_empty() {
        feature_status_label(access.status).to_string()
    } else {
        access.reason.clone()
    }
}

fn dashboard_control_subtitle(access: &FeatureAvailability, available_text: &str) -> String {
    if access.status == FeatureAccessState::MissingBackend {
        "Unavailable · review Setup & Access".to_string()
    } else {
        feature_control_subtitle(access, available_text)
    }
}

fn feature_value_label(access: &FeatureAvailability) -> String {
    match access.status {
        FeatureAccessState::Available => "Available".to_string(),
        FeatureAccessState::Unsupported => "Not supported".to_string(),
        FeatureAccessState::MissingBackend => {
            let reason = access.reason.to_ascii_lowercase();
            if reason.contains("asusd") {
                "Missing asusd".to_string()
            } else if reason.contains("supergfxd") {
                "Missing supergfxd".to_string()
            } else {
                "Missing backend".to_string()
            }
        }
        FeatureAccessState::PermissionDenied => "Read-only".to_string(),
        FeatureAccessState::TemporarilyUnavailable => "Temporarily unavailable".to_string(),
        FeatureAccessState::Unknown => "Checking support".to_string(),
    }
}

fn feature_state_value(
    current: Option<&str>,
    access: &FeatureAvailability,
    read_warning: Option<&str>,
) -> String {
    if let Some(current) = current {
        current.to_string()
    } else if read_warning.is_some() {
        "Temporarily unavailable".to_string()
    } else if access.is_available() {
        "Waiting for current value".to_string()
    } else {
        feature_value_label(access)
    }
}

fn lighting_placeholder_value(access: &FeatureAvailability) -> String {
    feature_state_value(None, access, None)
}

fn lighting_backend_label(lighting: &LightingInfo) -> String {
    let backend_value = if is_placeholder_text(&lighting.backend_kind) {
        &lighting.backend
    } else {
        &lighting.backend_kind
    };
    let backend = if is_placeholder_text(backend_value) {
        "Detected lighting backend".to_string()
    } else {
        match normalize_label(backend_value).as_str() {
            "sysfsled" => "Sysfs keyboard brightness".to_string(),
            "asusdaura" => "Verified asusd Aura".to_string(),
            "nativeaurahid" => "Verified native ASUS Aura HID".to_string(),
            "none" => "No lighting backend".to_string(),
            _ => backend_value.clone(),
        }
    };
    backend
}

fn lighting_device_title(lighting: &LightingInfo) -> String {
    if is_placeholder_text(&lighting.device)
        || lighting.device.eq_ignore_ascii_case("unknown device")
    {
        if normalize_label(&lighting.backend_kind) == "sysfsled" {
            "Keyboard Backlight".to_string()
        } else {
            "Lighting Device".to_string()
        }
    } else {
        lighting.device.clone()
    }
}

fn lighting_device_label(lighting: &LightingInfo) -> String {
    lighting_device_title(lighting)
}

fn lighting_preview_caption(lighting: &LightingInfo) -> String {
    if lighting.supports_per_key {
        "Per-key RGB is reported. The preview shows the staged target colour, not an editable key map."
            .to_string()
    } else if lighting.supports_argb || lighting.supports_zones {
        let zone = lighting.active_zone.as_deref().unwrap_or("reported zone");
        format!("Multi-zone/addressable RGB is reported. Previewing staged colour for {zone}.")
    } else if lighting.supports_rgb {
        "Single-target RGB: the selected colour applies to the entire keyboard target.".to_string()
    } else {
        "Brightness-only keyboard backlight. RGB and ARGB controls were not reported.".to_string()
    }
}

fn lighting_mode_supports_secondary_colour(mode: &str) -> bool {
    normalize_label(mode) == "breathe"
}

fn lighting_mode_supports_primary_colour(mode: &str) -> bool {
    matches!(
        normalize_label(mode).as_str(),
        "static"
            | "breathe"
            | "star"
            | "highlight"
            | "laser"
            | "ripple"
            | "pulse"
            | "comet"
            | "flash"
    )
}

fn lighting_mode_supports_speed(mode: &str) -> bool {
    matches!(
        normalize_label(mode).as_str(),
        "breathe" | "rainbowcycle" | "rainbowwave" | "pulse"
    )
}

fn lighting_mode_supports_direction(mode: &str) -> bool {
    normalize_label(mode) == "rainbowwave"
}

fn select_combo_value(combo: &gtk::ComboBoxText, values: &[String], current: Option<&str>) {
    let selected = current
        .and_then(|current| {
            values
                .iter()
                .find(|value| value.eq_ignore_ascii_case(current))
        })
        .or_else(|| values.first());
    combo.set_active_id(selected.map(String::as_str));
}

fn draw_keyboard_lighting_preview(
    ctx: &gtk::cairo::Context,
    width: i32,
    height: i32,
    colour: &gdk::RGBA,
) {
    let width = f64::from(width);
    let height = f64::from(height);
    let margin = 12.0;
    let keyboard_width = (width - margin * 2.0).max(1.0);
    let keyboard_height = (height - margin * 2.0).max(1.0);
    ctx.set_source_rgba(0.08, 0.09, 0.11, 1.0);
    ctx.rectangle(margin, margin, keyboard_width, keyboard_height);
    let _ = ctx.fill();

    let rows = 5;
    let columns = 15;
    let gap = 4.0;
    let key_width = (keyboard_width - gap * (f64::from(columns) + 1.0)) / f64::from(columns);
    let key_height = (keyboard_height - gap * (f64::from(rows) + 1.0)) / f64::from(rows);
    let red = f64::from(colour.red());
    let green = f64::from(colour.green());
    let blue = f64::from(colour.blue());
    for row in 0..rows {
        for column in 0..columns {
            let x = margin + gap + f64::from(column) * (key_width + gap);
            let y = margin + gap + f64::from(row) * (key_height + gap);
            ctx.set_source_rgba(red, green, blue, 0.88);
            ctx.rectangle(x, y, key_width.max(1.0), key_height.max(1.0));
            let _ = ctx.fill();
        }
    }
}

fn lighting_current_label(lighting: &LightingInfo) -> String {
    if lighting.max_brightness == 0 {
        "Brightness not reported".to_string()
    } else {
        format!("{}/{}", lighting.brightness, lighting.max_brightness)
    }
}

fn lighting_mode_label(lighting: &LightingInfo) -> String {
    if is_placeholder_text(&lighting.mode) {
        "Current mode not reported".to_string()
    } else {
        lighting.mode.clone()
    }
}

fn lighting_rgb_current_label(lighting: &LightingInfo) -> String {
    if lighting.supports_rgb {
        lighting
            .rgb_hex
            .clone()
            .unwrap_or_else(|| "Current RGB colour not reported".to_string())
    } else if lighting.backend.eq_ignore_ascii_case("sysfs-led") {
        "Unsupported by sysfs brightness backend".to_string()
    } else if lighting.status == "rgb_not_exposed" {
        "Not exposed by asusd".to_string()
    } else {
        "Unsupported by current backend".to_string()
    }
}

fn lighting_availability_label(
    lighting: Option<&LightingInfo>,
    access: &FeatureAvailability,
) -> String {
    if let Some(lighting) = lighting {
        if let Some(summary) = lighting.diagnostics_summary.as_deref() {
            if !summary.trim().is_empty() && lighting.last_error.is_none() {
                return summary.to_string();
            }
        }
        if let Some(error) = &lighting.last_error {
            return format!("backend_error ({error})");
        }
        return match lighting.status.as_str() {
            "available" => "available".to_string(),
            "rgb_not_exposed" => "available (RGB colour is not exposed by asusd)".to_string(),
            "rgb_read_only" => "available (RGB colour is read-only)".to_string(),
            "rgb_unsupported" => "available (brightness-only sysfs fallback)".to_string(),
            "aura_backend_error" | "backend_error" => "temporarily_unavailable".to_string(),
            other if !other.is_empty() => other.to_string(),
            _ => feature_value_label(access),
        };
    }

    feature_value_label(access)
}

fn lighting_brightness_subtitle(
    lighting: Option<&LightingInfo>,
    access: &FeatureAvailability,
) -> String {
    if let Some(lighting) = lighting {
        if lighting.authorization_required {
            "Administrator access required to change keyboard lighting. Authentication starts only when Apply is used."
                .to_string()
        } else if lighting.can_set {
            format!(
                "Current: {}. Drag to stage a change.",
                lighting_current_label(lighting)
            )
        } else {
            feature_control_subtitle(access, "Adjust brightness")
        }
    } else {
        feature_control_subtitle(access, "Adjust brightness")
    }
}

fn lighting_mode_subtitle(lighting: Option<&LightingInfo>, access: &FeatureAvailability) -> String {
    if let Some(lighting) = lighting {
        if lighting.can_set {
            let supported_modes = if lighting.supported_modes.is_empty() {
                "Off, Static".to_string()
            } else {
                lighting.supported_modes.join(", ")
            };
            format!(
                "Current: {}. Supported by this backend: {}.",
                lighting_mode_label(lighting),
                supported_modes
            )
        } else {
            feature_control_subtitle(access, "Choose a lighting mode and apply")
        }
    } else {
        feature_control_subtitle(access, "Choose a lighting mode and apply")
    }
}

fn lighting_apply_subtitle(
    lighting: Option<&LightingInfo>,
    access: &FeatureAvailability,
) -> String {
    if matches!(lighting, Some(lighting) if lighting.authorization_required) {
        "Apply the staged brightness and open the normal administrator authorization flow."
            .to_string()
    } else if matches!(lighting, Some(lighting) if lighting.can_set) {
        "Apply the staged lighting change.".to_string()
    } else {
        feature_control_subtitle(access, "Apply the staged lighting change.")
    }
}

fn lighting_rgb_subtitle(lighting: Option<&LightingInfo>, access: &FeatureAvailability) -> String {
    if let Some(lighting) = lighting {
        if lighting.supports_rgb {
            if lighting.can_set {
                "Choose an RGB colour to stage and apply.".to_string()
            } else if let Some(warning) = lighting.permission_warning.as_deref() {
                warning.to_string()
            } else if access.reason.is_empty() {
                "RGB colour is available, but writes are blocked for the current user.".to_string()
            } else {
                access.reason.clone()
            }
        } else {
            if lighting.backend.eq_ignore_ascii_case("sysfs-led") {
                lighting.fallback_reason.clone().unwrap_or_else(|| {
                    "RGB colour requires ASUS Aura support. Current backend only supports keyboard brightness."
                        .to_string()
                })
            } else if lighting.status == "rgb_not_exposed" {
                "asusd is available, but it does not expose a writable Aura RGB colour control."
                    .to_string()
            } else if let Some(error) = lighting.last_error.as_deref() {
                format!("RGB colour is blocked by a backend error: {error}")
            } else if let Some(reason) = lighting.unavailable_reason.as_deref() {
                reason.to_string()
            } else {
                "RGB colour is not supported by the current lighting backend.".to_string()
            }
        }
    } else {
        match access.status {
            FeatureAccessState::MissingBackend => {
                "No lighting backend is available right now.".to_string()
            }
            FeatureAccessState::PermissionDenied => {
                if access.reason.is_empty() {
                    "Lighting is detected, but writes are blocked for the current user."
                        .to_string()
                } else {
                    access.reason.clone()
                }
            }
            FeatureAccessState::TemporarilyUnavailable => {
                "Lighting backend is temporarily unavailable. RGB colour cannot be changed right now."
                    .to_string()
            }
            FeatureAccessState::Unsupported => {
                "Keyboard lighting is not supported on this machine.".to_string()
            }
            FeatureAccessState::Available => {
                "Waiting for lighting backend details before enabling RGB colour."
                    .to_string()
            }
            FeatureAccessState::Unknown => {
                "Checking whether RGB colour control is available.".to_string()
            }
        }
    }
}

fn lighting_last_action_text(
    lighting: Option<&LightingInfo>,
    access: &FeatureAvailability,
) -> String {
    if let Some(lighting) = lighting {
        if let Some(outcome) = lighting.apply_outcome.as_deref() {
            return match outcome {
                "verified" => "Applied and verified by hardware readback.".to_string(),
                "accepted_no_readback" => {
                    "Applied; this backend does not provide reliable readback.".to_string()
                }
                value if value.starts_with("failed:") => {
                    format!(
                        "Apply failed: {}",
                        value.trim_start_matches("failed:").trim()
                    )
                }
                value => value.to_string(),
            };
        }
        if lighting.can_set {
            "No lighting change applied yet.".to_string()
        } else if access.reason.is_empty() {
            "Lighting is detected, but writes are blocked for the current user.".to_string()
        } else {
            access.reason.clone()
        }
    } else if access.reason.is_empty() {
        feature_status_label(access.status).to_string()
    } else {
        access.reason.clone()
    }
}

fn combined_asusd_issue(caps: &DeviceCaps) -> Option<String> {
    let profile = &caps.profile_access;
    let charge = &caps.charge_limit_access;
    if profile.status != charge.status || profile.is_available() || charge.is_available() {
        return None;
    }

    match profile.status {
        FeatureAccessState::MissingBackend => Some(
            "Install and start asusd to enable performance profiles and charge-limit control."
                .to_string(),
        ),
        FeatureAccessState::Unsupported => Some(
            "This machine does not expose ASUS performance profiles or charge-limit control."
                .to_string(),
        ),
        FeatureAccessState::TemporarilyUnavailable => Some(
            "asusd is present, but profile and charge-limit support are temporarily unavailable."
                .to_string(),
        ),
        _ => None,
    }
}

fn support_messages(caps: &DeviceCaps, cpu_caps: &CpuCaps) -> Vec<String> {
    let mut messages = Vec::new();

    if let Some(message) = combined_asusd_issue(caps) {
        messages.push(message);
    } else {
        if !caps.profile_access.is_available() {
            messages.push(caps.profile_access.reason.clone());
        }
        if !caps.charge_limit_access.is_available() {
            messages.push(caps.charge_limit_access.reason.clone());
        }
    }

    if !caps.gpu_mode_access.is_available() {
        messages.push(caps.gpu_mode_access.reason.clone());
    }

    if caps.kbd_backlight_access.status == FeatureAccessState::PermissionDenied {
        messages.push(caps.kbd_backlight_access.reason.clone());
    }

    if cpu_caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::AuthorizationRequired)
    {
        messages.push(
            "Administrator access is required for some CPU controls. Authentication starts only when Apply is used."
                .to_string(),
        );
    } else if cpu_caps.control_access.iter().any(|control| {
        matches!(
            control.status,
            CpuAccessState::PermissionDenied | CpuAccessState::AuthorizationDenied
        )
    }) {
        messages.push("A CPU write was denied. Telemetry remains available.".to_string());
    } else if cpu_caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::HelperMissing)
    {
        messages.push("Some supported CPU controls need the privileged helper.".to_string());
    } else if cpu_caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::MissingBackend)
    {
        messages.push("Some CPU controls are unsupported by the current backend.".to_string());
    } else if cpu_caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::TemporarilyUnavailable)
    {
        messages.push("Some CPU controls are temporarily unavailable right now.".to_string());
    }

    messages.retain(|message| !message.trim().is_empty());
    messages
}

fn gpu_status_summary(caps: &DeviceCaps, warnings: &[String]) -> String {
    if let Some(detail) = warning_detail(warnings, "GPU switch hint: ") {
        return detail.to_string();
    }
    if let Some(_detail) = warning_detail(warnings, "GPU mode read failed: ") {
        return "GPU mode is detected, but the current value could not be read right now."
            .to_string();
    }
    if let Some(_detail) = warning_detail(warnings, "profile read failed: ") {
        return "Performance profile support is detected, but the current value could not be read right now."
            .to_string();
    }
    if !caps.profile_access.is_available() {
        return caps.profile_access.reason.clone();
    }
    if !caps.gpu_mode_access.is_available() {
        return caps.gpu_mode_access.reason.clone();
    }
    if !caps.gpu_switch_hint.trim().is_empty()
        && !matches!(
            caps.gpu_switch_state,
            GpuSwitchState::Ready | GpuSwitchState::Unknown
        )
    {
        return caps.gpu_switch_hint.clone();
    }
    "Ready".to_string()
}

fn gpu_switch_hint_text(caps: &DeviceCaps, warnings: &[String]) -> String {
    if let Some(detail) = warning_detail(warnings, "GPU switch hint: ") {
        detail.to_string()
    } else if !caps.gpu_switch_hint.trim().is_empty()
        && !matches!(
            caps.gpu_switch_state,
            GpuSwitchState::Ready | GpuSwitchState::Unknown
        )
    {
        caps.gpu_switch_hint.clone()
    } else if !caps.gpu_mode_access.is_available() {
        caps.gpu_mode_access.reason.clone()
    } else if caps.requires_logout_for_gpu_switch {
        "Logout will be required to complete the GPU transition.".to_string()
    } else if caps.requires_reboot_for_gpu_switch {
        "supergfxd reports that this configuration may require a reboot after switching."
            .to_string()
    } else {
        "supergfxd controls switch safety. Close applications using the discrete GPU if the external service asks, then follow any logout or reboot instruction.".to_string()
    }
}

fn troubleshooting_summary_text(
    caps: &DeviceCaps,
    cpu_caps: &CpuCaps,
    warnings: &[String],
) -> String {
    let mut lines = Vec::new();
    lines.push("Troubleshooting Summary".to_string());
    lines.push("=======================".to_string());

    let mut issues = Vec::new();
    for (label, access) in [
        ("Performance Profile", &caps.profile_access),
        ("Charge Limit", &caps.charge_limit_access),
        ("GPU Mode", &caps.gpu_mode_access),
        ("Keyboard Lighting", &caps.kbd_backlight_access),
    ] {
        if !access.is_available() && access.status != FeatureAccessState::Unknown {
            issues.push(format!(
                "{label}: {} ({})",
                access.status.as_str(),
                access.reason
            ));
        }
    }

    if cpu_caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::AuthorizationRequired)
    {
        issues.push(
            "CPU Writes: authorization_required (Use Apply to request administrator access.)"
                .to_string(),
        );
    } else if cpu_caps.control_access.iter().any(|control| {
        matches!(
            control.status,
            CpuAccessState::PermissionDenied | CpuAccessState::AuthorizationDenied
        )
    }) {
        issues.push("CPU Writes: authorization_denied (Telemetry remains available.)".to_string());
    } else if cpu_caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::MissingBackend)
    {
        issues.push(
            "CPU Writes: missing_backend (Some CPU controls are not available through the current backend.)"
                .to_string(),
        );
    } else if cpu_caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::TemporarilyUnavailable)
    {
        issues.push(
            "CPU Writes: temporarily_unavailable (Some CPU controls could not be confirmed right now.)"
                .to_string(),
        );
    }

    if let Some(_detail) = warning_detail(warnings, "profile read failed: ") {
        issues.push(
            "Profile state: temporarily_unavailable (The current ASUS profile could not be read right now.)"
                .to_string(),
        );
    }
    if let Some(_detail) = warning_detail(warnings, "GPU mode read failed: ") {
        issues.push(
            "GPU mode state: temporarily_unavailable (The current GPU mode could not be read right now.)"
                .to_string(),
        );
    }
    if let Some(detail) = warning_detail(warnings, "GPU switch hint: ") {
        issues.push(format!("GPU switch status: {detail}"));
    }

    if issues.is_empty() {
        lines.push("No blocking service or permission issues detected.".to_string());
    } else {
        for issue in issues {
            lines.push(format!("- {issue}"));
        }
    }

    lines.join("\n")
}

fn friendly_action_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("authentication was cancelled") || lower.contains("authorization cancelled") {
        "Authentication was cancelled. No changes were applied; telemetry remains available."
            .to_string()
    } else if lower.contains("keyboard lighting")
        && (lower.contains("authorization was denied") || lower.contains("not_authorized"))
    {
        "Administrator authorization was denied. Keyboard brightness remains readable.".to_string()
    } else if lower.contains("privileged lighting helper is unavailable") {
        "ROG Helper privileged service is not installed or is unavailable. Keyboard brightness remains readable."
            .to_string()
    } else if lower.contains("fan control")
        && (lower.contains("authorization was denied") || lower.contains("not_authorized"))
    {
        "Administrator authorization was denied. Fan telemetry is still available.".to_string()
    } else if lower.contains("privileged fan helper is unavailable") {
        "ROG Helper privileged service is not installed or is unavailable. Fan telemetry is still available."
            .to_string()
    } else if lower.contains("battery control")
        && (lower.contains("authorization was denied") || lower.contains("not_authorized"))
    {
        "Administrator authorization was denied. Battery telemetry remains available.".to_string()
    } else if lower.contains("privileged battery helper is unavailable") {
        "ROG Helper privileged service is not installed or is unavailable. Battery telemetry remains available."
            .to_string()
    } else if lower.contains("not_authorized") || lower.contains("authorization was denied") {
        "Administrator authorization was denied. CPU telemetry is still available.".to_string()
    } else if lower.contains("privileged cpu helper is unavailable") {
        "ROG Helper privileged service is not installed or is unavailable. CPU telemetry is still available."
            .to_string()
    } else if lower.contains("profile control requires asusd") {
        "Performance profiles require asusd. Install or start asusd, then retry.".to_string()
    } else if lower.contains("battery limit control requires asusd") {
        "Charge-limit control requires asusd. Install or start asusd, then retry.".to_string()
    } else if lower.contains("gpu mode control requires supergfxd") {
        "GPU mode switching requires supergfxd. Install or start supergfxd, then retry.".to_string()
    } else if lower.contains("set supergfx mode")
        && (lower.contains("accessdenied")
            || lower.contains("access denied")
            || lower.contains("notauthorized")
            || lower.contains("not authorized"))
    {
        "supergfxd or its system policy denied the GPU mode switch. Root access to ROG Helper is not a fallback; review the external service authorization.".to_string()
    } else if lower.contains("logout required") && lower.contains("gpu switch") {
        "Logout is required to complete the pending GPU switch. Another switch is unavailable until that transition completes."
            .to_string()
    } else if lower.contains("reboot required") && lower.contains("gpu switch") {
        "A reboot is required to complete the pending GPU switch. Another switch is unavailable until that transition completes."
            .to_string()
    } else if lower.contains("pending gpu")
        || lower.contains("gpu is busy")
        || lower.contains("gpu is in use")
    {
        "supergfxd reports that the GPU transition cannot proceed yet. Close applications using the discrete GPU and follow any logout or reboot instruction."
            .to_string()
    } else if lower.contains("lighting not supported on this system") {
        "No lighting backend is available on this machine.".to_string()
    } else if lower.contains("rgb colour requires asus aura support")
        || lower.contains("rgb color is not supported by the sysfs led backend")
    {
        "RGB colour requires ASUS Aura support. Current backend only supports keyboard brightness."
            .to_string()
    } else if lower.contains("rgb colour is not exposed")
        || lower.contains("rgb color is not exposed")
    {
        "asusd is available, but it does not expose a writable Aura RGB colour control.".to_string()
    } else if lower.contains("lighting mode")
        && lower.contains("not supported by the sysfs led backend")
    {
        "The current lighting backend supports only Off and Static.".to_string()
    } else if lower.contains("session dbus unavailable")
        || lower.contains("failed to connect to daemon proxy")
    {
        "rog-helperd is not reachable on the session bus.".to_string()
    } else {
        match ErrorCategory::classify_message(error) {
            ErrorCategory::PermissionDenied => {
                "This write was blocked by the active backend. Review Setup & Access to see whether administrator authorization or an external service is required."
                    .to_string()
            }
            ErrorCategory::ReadOnly => {
                "This control is read-only for the current backend or user.".to_string()
            }
            ErrorCategory::MissingDependency => {
                "A required service is missing or not running. Open Diagnostics for setup details."
                    .to_string()
            }
            ErrorCategory::Unsupported => {
                "This control is not supported by the active hardware backend.".to_string()
            }
            ErrorCategory::TransientFailure | ErrorCategory::Unavailable => {
                "The control is temporarily unavailable. Check the daemon and backend status, then retry."
                    .to_string()
            }
            ErrorCategory::InvalidInput | ErrorCategory::Unknown => error
                .split_once(": ")
                .map_or_else(|| error.to_string(), |(_, suffix)| suffix.to_string()),
        }
    }
}

fn cpu_has_access_issue(caps: &CpuCaps) -> bool {
    caps.control_access.iter().any(|control| {
        matches!(
            control.status,
            CpuAccessState::PermissionDenied
                | CpuAccessState::AuthorizationRequired
                | CpuAccessState::AuthorizationDenied
                | CpuAccessState::HelperMissing
                | CpuAccessState::ReadOnly
                | CpuAccessState::MissingBackend
                | CpuAccessState::TemporarilyUnavailable
        )
    })
}

fn cpu_banner_title(caps: &CpuCaps) -> String {
    if caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::AuthorizationRequired)
    {
        "Administrator access required for some CPU controls".to_string()
    } else if caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::AuthorizationDenied)
    {
        "Administrator authorization was denied".to_string()
    } else if caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::HelperMissing)
    {
        "ROG Helper privileged service is unavailable or not installed".to_string()
    } else if caps.control_access.iter().any(|control| {
        matches!(
            control.status,
            CpuAccessState::PermissionDenied | CpuAccessState::ReadOnly
        )
    }) {
        "CPU controls are visible, but writes are blocked".to_string()
    } else if caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::MissingBackend)
    {
        "Some CPU controls need backend support".to_string()
    } else if caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::TemporarilyUnavailable)
    {
        "Some CPU controls could not be confirmed right now".to_string()
    } else {
        "Some CPU controls are not supported here".to_string()
    }
}

fn cpu_access_report_text(caps: &CpuCaps) -> String {
    let mut out = String::new();
    out.push_str("CPU Write Access\n");
    out.push_str("----------------\n");
    if caps.control_access.is_empty() {
        out.push_str("No CPU write access data available.\n");
        return out;
    }

    for control in &caps.control_access {
        let supported = !matches!(
            control.status,
            CpuAccessState::Unknown | CpuAccessState::Unsupported | CpuAccessState::MissingBackend
        );
        out.push_str(&format!("{}:\n", control.kind.as_str()));
        out.push_str(&format!("  supported: {supported}\n"));
        out.push_str(&format!("  direct_write: {}\n", control.direct_write));
        out.push_str(&format!(
            "  privileged_write: {}\n",
            control.privileged_write
        ));
        out.push_str(&format!(
            "  authorization: {}\n",
            control.authorization.as_str()
        ));
        out.push_str(&format!("  status: {}", control.status.as_str()));
        if !control.reason.is_empty() {
            out.push_str(&format!(" ({})", control.reason));
        }
        out.push('\n');
        if control.paths.is_empty() {
            out.push_str("  (no paths detected)\n");
            continue;
        }
        for path in &control.paths {
            out.push_str(&format!(
                "  [{}{}] {}\n",
                if path.readable { "r" } else { "-" },
                if path.writable { "w" } else { "-" },
                path.path
            ));
        }
    }

    out.push_str("\nSuggested checks\n");
    out.push_str("----------------\n");
    out.push_str("  ls -l /sys/devices/system/cpu/cpufreq/policy*/scaling_governor\n");
    out.push_str("  ls -l /sys/devices/system/cpu/cpufreq/policy*/energy_performance_preference\n");
    out.push_str("  ls -l /sys/devices/system/cpu/cpufreq/policy*/scaling_{min,max}_freq\n");
    out.push_str("  ls -l /sys/devices/system/cpu/intel_pstate/no_turbo\n");
    out.push_str("  ls -l /sys/devices/system/cpu/cpu*/online\n");
    out.push_str("  systemctl --user restart rog-helperd\n");
    out.push_str("  systemctl status rog-helper-privileged.service\n");
    out.push_str("\nDirect writes are preferred. Supported permission-blocked controls use the typed privileged helper only after Apply is selected.\n");

    out
}

fn cpu_diagnostics_text(caps: &CpuCaps, cpu: Option<&CpuTelemetry>) -> String {
    let mut out = String::new();
    out.push_str("CPU Diagnostics\n");
    out.push_str("================\n");
    out.push_str(&format!("has_cpufreq: {}\n", caps.has_cpufreq));
    out.push_str(&format!("has_governor: {}\n", caps.has_governor));
    out.push_str(&format!("has_epp: {}\n", caps.has_epp));
    out.push_str(&format!("has_boost_toggle: {}\n", caps.has_boost_toggle));
    out.push_str(&format!("has_package_power: {}\n", caps.has_package_power));
    out.push_str(&format!("policy_writable: {}\n", caps.policy_writable));
    out.push_str(&format!("cpu_count (physical cores): {}\n", caps.cpu_count));
    out.push_str(&format!(
        "thread_count (logical threads): {}\n",
        caps.thread_count
    ));
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
    if !caps.sysfs_paths.is_empty() {
        out.push_str("sysfs_paths:\n");
        for path in &caps.sysfs_paths {
            out.push_str(&format!("  {path}\n"));
        }
    }
    out.push('\n');
    out.push_str(&cpu_access_report_text(caps));

    out.push_str("\nCurrent\n");
    out.push_str("-------\n");
    if let Some(cpu) = cpu {
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

        out.push_str("\nLogical CPUs / Threads\n");
        out.push_str("----------------------\n");
        out.push_str("cpu  pcore  thr   pol   online  usage%  cur_mhz  min_mhz  max_mhz\n");
        for core in &cpu.per_core {
            let thread_label = format_cpu_thread_position(core.thread_index, core.thread_count);
            out.push_str(&format!(
                "{:<3}  {:<5}  {:<4}  {:<4}  {:<6}  {:<6}  {:<7}  {:<7}  {:<7}\n",
                core.logical_cpu_id,
                core.physical_core_index
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                thread_label,
                core.policy_id
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
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
    } else {
        out.push_str("CPU telemetry unavailable.\n");
    }

    out
}

fn normalize_label(v: &str) -> String {
    v.to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect()
}

fn is_placeholder_text(v: &str) -> bool {
    let trimmed = v.trim();
    trimmed.is_empty() || trimmed == "(n/a)"
}

fn profile_name_is(v: &str, expected: &str) -> bool {
    normalize_label(v) == normalize_label(expected)
}

fn tray_profile_index_from_text(v: &str) -> usize {
    let key = normalize_label(v);
    match key.as_str() {
        "silent" | "quiet" | "lowpower" => 0,
        "balanced" => 1,
        "turbo" | "performance" => 2,
        _ => 1,
    }
}

fn gpu_mode_index_in_supported(current: &str, supported: &[String]) -> Option<usize> {
    let current = normalize_label(current);
    supported.iter().position(|candidate| {
        let candidate = normalize_label(candidate);
        current == candidate
            || matches!(
                (current.as_str(), candidate.as_str()),
                ("dedicated", "asusmuxdgpu") | ("discrete", "asusmuxdgpu")
            )
    })
}

fn pref_value_row(group: &adw::PreferencesGroup, title: &str, monospace: bool) -> gtk::Label {
    let row = adw::ActionRow::builder().title(title).build();
    let value = gtk::Label::new(Some("(n/a)"));
    value.set_xalign(1.0);
    value.set_halign(gtk::Align::End);
    value.set_wrap(true);
    value.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    value.set_single_line_mode(false);
    value.set_ellipsize(gtk::pango::EllipsizeMode::None);
    value.set_justify(gtk::Justification::Right);
    value.set_width_chars(14);
    value.set_max_width_chars(48);
    value.set_size_request(150, -1);
    if monospace {
        value.add_css_class("monospace");
    }
    row.add_suffix(&value);
    row.set_activatable(false);
    group.add(&row);
    value
}

fn settings_switch_row(
    group: &adw::PreferencesGroup,
    title: &str,
    subtitle: &str,
    active: bool,
) -> gtk::Switch {
    let switch = gtk::Switch::new();
    switch.set_active(active);
    switch.set_valign(gtk::Align::Center);
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    row.add_suffix(&switch);
    row.set_activatable(false);
    group.add(&row);
    switch
}

fn setup_pref_row(group: &adw::PreferencesGroup, title: &str) -> (adw::ActionRow, gtk::Label) {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle("Checking readiness…")
        .build();
    let value = gtk::Label::new(Some("Checking"));
    value.add_css_class("setup-status-value");
    value.set_halign(gtk::Align::End);
    value.set_valign(gtk::Align::Center);
    row.add_suffix(&value);
    row.set_activatable(false);
    group.add(&row);
    (row, value)
}

fn setup_access_row(group: &adw::PreferencesGroup, title: &str) -> SetupAccessRow {
    let (row, value) = setup_pref_row(group, title);
    SetupAccessRow { row, value }
}

fn set_setup_access(row: &SetupAccessRow, state: AccessUiState, subtitle: &str) {
    set_setup_access_label(row, state, state.label(), subtitle);
}

fn set_setup_access_label(row: &SetupAccessRow, state: AccessUiState, label: &str, subtitle: &str) {
    row.value.set_text(label);
    set_setup_value_status(&row.value, state.css_class());
    row.row.set_subtitle(subtitle);
}

fn feature_access_ui_state(access: &FeatureAvailability) -> AccessUiState {
    match access.status {
        FeatureAccessState::Unknown => AccessUiState::Checking,
        FeatureAccessState::Available => AccessUiState::Available,
        FeatureAccessState::Unsupported => AccessUiState::UnsupportedHardware,
        FeatureAccessState::MissingBackend => AccessUiState::BackendMissing,
        FeatureAccessState::PermissionDenied => AccessUiState::ReadOnly,
        FeatureAccessState::TemporarilyUnavailable => AccessUiState::Unavailable,
    }
}

fn update_setup_feature_row(
    row: &adw::ActionRow,
    value: &gtk::Label,
    access: &FeatureAvailability,
) {
    let state = feature_access_ui_state(access);
    value.set_text(state.label());
    set_setup_value_status(value, state.css_class());
    row.set_subtitle(&access.reason);
}

fn helper_access_state(status: Option<&PrivilegedStatus>) -> AccessUiState {
    let Some(status) = status else {
        return AccessUiState::Checking;
    };
    if !status.system_bus_connected {
        AccessUiState::Unavailable
    } else if !status.privileged_helper_installed {
        AccessUiState::PrivilegedHelperMissing
    } else if !status.privileged_helper_reachable || !status.privileged_helper_compatible {
        AccessUiState::SetupRequired
    } else {
        AccessUiState::Available
    }
}

fn cpu_privileged_access_state(
    status: Option<&PrivilegedStatus>,
    cpu_caps: &CpuCaps,
) -> AccessUiState {
    let hardware_supported = cpu_caps.control_access.iter().any(|control| {
        !matches!(
            control.status,
            CpuAccessState::Unknown | CpuAccessState::Unsupported | CpuAccessState::MissingBackend
        )
    });
    if !hardware_supported {
        return AccessUiState::UnsupportedHardware;
    }
    if cpu_caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::AuthorizationDenied)
    {
        return AccessUiState::AuthorizationDenied;
    }
    if cpu_caps
        .control_access
        .iter()
        .any(|control| control.authorization == CpuAuthorization::Authorized)
    {
        return AccessUiState::Authorized;
    }
    if cpu_caps.control_access.iter().any(|control| {
        control.status == CpuAccessState::AuthorizationRequired
            || control.authorization == CpuAuthorization::Required
    }) {
        return AccessUiState::AvailableWithAdministrator;
    }
    if cpu_caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::HelperMissing)
    {
        return AccessUiState::PrivilegedHelperMissing;
    }
    match status {
        Some(status)
            if status
                .privileged_categories_available
                .contains(&PrivilegedCategory::Cpu) =>
        {
            AccessUiState::Available
        }
        Some(_) => AccessUiState::PrivilegedHelperMissing,
        None => AccessUiState::Checking,
    }
}

fn fan_privileged_access_state(
    status: Option<&PrivilegedStatus>,
    fan_state: &FanState,
) -> AccessUiState {
    if fan_state.fans.is_empty() {
        return AccessUiState::UnsupportedHardware;
    }
    let safe_mapping = fan_state.fans.iter().any(|fan| {
        fan.pwm_endpoint_verified && (fan.supports_auto || fan.supports_curve || fan.controllable)
    });
    if !safe_mapping {
        return AccessUiState::UnsafeMapping;
    }
    if fan_state
        .fans
        .iter()
        .any(|fan| fan.authorization == "denied")
    {
        return AccessUiState::AuthorizationDenied;
    }
    if fan_state
        .fans
        .iter()
        .any(|fan| fan.authorization == "authorized")
    {
        return AccessUiState::Authorized;
    }
    if fan_state
        .fans
        .iter()
        .any(|fan| fan.access_state == "authorization_required")
    {
        return AccessUiState::AvailableWithAdministrator;
    }
    if fan_state.fans.iter().any(|fan| fan.direct_write) {
        return AccessUiState::NotRequired;
    }
    match status {
        Some(status)
            if status
                .privileged_categories_available
                .contains(&PrivilegedCategory::Fans) =>
        {
            AccessUiState::ReadOnly
        }
        Some(_) => AccessUiState::PrivilegedHelperMissing,
        None => AccessUiState::Checking,
    }
}

fn lighting_privileged_access_state(
    status: Option<&PrivilegedStatus>,
    lighting: Option<&LightingInfo>,
    caps: &DeviceCaps,
) -> AccessUiState {
    if !caps.has_kbd_backlight && lighting.is_none() {
        return AccessUiState::UnsupportedHardware;
    }
    if let Some(lighting) = lighting {
        if lighting.direct_writable {
            return AccessUiState::NotRequired;
        }
        if lighting.authorization == "denied" {
            return AccessUiState::AuthorizationDenied;
        }
        if lighting.authorization == "authorized" {
            return AccessUiState::Authorized;
        }
        if lighting.privileged_writable && lighting.authorization_required {
            return AccessUiState::AvailableWithAdministrator;
        }
        if lighting.privileged_writable {
            return AccessUiState::Available;
        }
    }
    match status {
        Some(status)
            if status
                .privileged_categories_available
                .contains(&PrivilegedCategory::Lighting) =>
        {
            AccessUiState::ReadOnly
        }
        Some(_) => AccessUiState::PrivilegedHelperMissing,
        None => AccessUiState::Checking,
    }
}

fn update_administrator_access_rows(
    status: Option<&PrivilegedStatus>,
    cpu_caps: &CpuCaps,
    fan_state: &FanState,
    lighting: Option<&LightingInfo>,
    caps: &DeviceCaps,
    rows: &AdministratorAccessRows,
) {
    let helper_state = helper_access_state(status);
    let helper_label = match helper_state {
        AccessUiState::Available => "Installed",
        AccessUiState::PrivilegedHelperMissing => "Missing",
        _ => helper_state.label(),
    };
    let helper_detail = match status {
        Some(status) if status.privileged_helper_reachable && status.privileged_helper_compatible => {
            "The on-demand service is compatible. It is used only for validated per-operation fallbacks."
        }
        Some(status) if status.privileged_helper_installed => {
            "The service is installed but could not be reached or its API is incompatible."
        }
        Some(_) => {
            "Install the packaged rog-helper-privileged service to unlock supported permission-blocked controls."
        }
        None => "Refresh checks to inspect the optional privileged service.",
    };
    set_setup_access_label(&rows.helper, helper_state, helper_label, helper_detail);

    let system_bus_state = match status {
        Some(status) if status.system_bus_connected => AccessUiState::Available,
        Some(_) => AccessUiState::Unavailable,
        None => AccessUiState::Checking,
    };
    set_setup_access_label(
        &rows.system_bus,
        system_bus_state,
        if system_bus_state == AccessUiState::Available {
            "Connected"
        } else {
            system_bus_state.label()
        },
        "System services and the privileged helper communicate over the system bus.",
    );

    let polkit_state = match status {
        Some(status) if status.polkit_available => AccessUiState::Available,
        Some(_) => AccessUiState::Unavailable,
        None => AccessUiState::Checking,
    };
    set_setup_access(
        &rows.polkit,
        polkit_state,
        "PolicyKit authorizes the original DBus caller only when a privileged action is attempted.",
    );

    let cpu_state = cpu_privileged_access_state(status, cpu_caps);
    set_setup_access(
        &rows.cpu,
        cpu_state,
        match cpu_state {
            AccessUiState::AvailableWithAdministrator => {
                "Supported CPU controls stay enabled. Use Apply to authenticate for the selected change."
            }
            AccessUiState::Authorized => "A CPU control was authorized during this session.",
            AccessUiState::AuthorizationDenied => {
                "Administrator authorization was denied. Apply again to retry."
            }
            AccessUiState::PrivilegedHelperMissing => {
                "Supported CPU controls need rog-helper-privileged because direct sysfs writes are blocked."
            }
            AccessUiState::UnsupportedHardware => {
                "No supported CPU write endpoint was detected; CPU telemetry remains available."
            }
            _ => "Direct CPU writes remain preferred; the helper is only a permission fallback.",
        },
    );

    let fan_state_value = fan_privileged_access_state(status, fan_state);
    set_setup_access(
        &rows.fans,
        fan_state_value,
        match fan_state_value {
            AccessUiState::UnsafeMapping => {
                "Fan RPM telemetry is available, but no safe writable fan interface was confirmed."
            }
            AccessUiState::ReadOnly => {
                "A safe fan interface is detected, but it is not currently writable."
            }
            AccessUiState::AvailableWithAdministrator => {
                "Use a fan Apply action to authenticate; simply opening Cooling never prompts."
            }
            AccessUiState::PrivilegedHelperMissing => {
                "A verified fan interface needs rog-helper-privileged for writes."
            }
            _ => "Only verified fan mappings can cross the privileged boundary.",
        },
    );

    let lighting_state = lighting_privileged_access_state(status, lighting, caps);
    set_setup_access(
        &rows.lighting,
        lighting_state,
        match lighting_state {
            AccessUiState::NotRequired => {
                "The verified asusd or directly writable LED backend handles lighting."
            }
            AccessUiState::AvailableWithAdministrator => {
                "Use Apply lighting to authenticate for the approved keyboard-brightness fallback."
            }
            AccessUiState::PrivilegedHelperMissing => {
                "Keyboard brightness is readable, but the optional lighting helper is unavailable."
            }
            AccessUiState::UnsupportedHardware => {
                "No supported keyboard-lighting interface was detected."
            }
            _ => "RGB and Aura modes never use a generic root hardware API.",
        },
    );

    let gpu_state = if caps.gpu_backend == "supergfxd" {
        AccessUiState::Available
    } else {
        AccessUiState::ExternalServiceRequired
    };
    set_setup_access_label(
        &rows.gpu,
        gpu_state,
        "Handled by supergfxd",
        if caps.gpu_backend == "supergfxd" {
            "supergfxd owns GPU mode validation, switching, authorization, and transition safety."
        } else {
            "supergfxd is required for GPU mode switching; root access to ROG Helper is not a substitute."
        },
    );

    let (battery_state, battery_label, battery_detail) = match caps.battery_limit_backend.as_str() {
        "asusd" => (
            AccessUiState::Available,
            "Handled by asusd",
            "asusd remains the preferred battery charge-limit backend and owns its service authorization.",
        ),
        "power_supply_sysfs" if caps.battery_limit_direct_write => (
            AccessUiState::NotRequired,
            "Direct kernel interface",
            "asusd does not expose this feature; the validated standard battery threshold is directly writable.",
        ),
        "power_supply_sysfs" if caps.battery_limit_privileged_write => (
            AccessUiState::AvailableWithAdministrator,
            "Administrator access required",
            "asusd does not expose this feature; Apply may use the validated standard battery-threshold fallback.",
        ),
        _ if caps.charge_limit_access.status == FeatureAccessState::MissingBackend => (
            AccessUiState::ExternalServiceRequired,
            "asusd required",
            "Install or start asusd when no safe standard kernel fallback is available.",
        ),
        _ => (
            AccessUiState::UnsupportedHardware,
            "Unsupported hardware",
            "No maintainable battery charge-limit backend was detected.",
        ),
    };
    set_setup_access_label(&rows.battery, battery_state, battery_label, battery_detail);
}

fn privilege_diagnostics_text(
    status: Option<&PrivilegedStatus>,
    cpu_caps: &CpuCaps,
    fan_state: &FanState,
    lighting: Option<&LightingInfo>,
    caps: &DeviceCaps,
) -> String {
    let mut lines = vec![
        "ROG Helper Privilege Diagnostics".to_string(),
        "================================".to_string(),
        String::new(),
    ];
    if let Some(status) = status {
        lines.extend([
            format!("system_bus_connected: {}", status.system_bus_connected),
            format!(
                "privileged_helper_installed: {}",
                status.privileged_helper_installed
            ),
            format!(
                "privileged_helper_reachable: {}",
                status.privileged_helper_reachable
            ),
            format!(
                "privileged_helper_compatible: {}",
                status.privileged_helper_compatible
            ),
            format!("polkit_available: {}", status.polkit_available),
            format!(
                "authorization_state: {}",
                status.authorization_state.as_str()
            ),
            format!(
                "categories: {}",
                status
                    .privileged_categories_available
                    .iter()
                    .map(|category| category.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ]);
    } else {
        lines.push("privileged_status: not received".to_string());
    }

    lines.push(String::new());
    lines.push("CPU controls:".to_string());
    if cpu_caps.control_access.is_empty() {
        lines.push("  no structured access rows".to_string());
    } else {
        for control in &cpu_caps.control_access {
            lines.push(format!(
                "  {}: status={}, direct={}, privileged={}, authorization={}",
                control.kind.as_str(),
                control.status.as_str(),
                control.direct_write,
                control.privileged_write,
                control.authorization.as_str()
            ));
        }
    }

    lines.push(String::new());
    lines.push("Fan controls:".to_string());
    if fan_state.fans.is_empty() {
        lines.push("  unsupported: no fan interface detected".to_string());
    } else {
        for fan in &fan_state.fans {
            lines.push(format!(
                "  {}: safe_mapping={}, direct={}, privileged={}, authorization={}, access={}",
                fan.id,
                fan.pwm_endpoint_verified,
                fan.direct_write,
                fan.privileged_write,
                fan.authorization,
                fan.access_state
            ));
        }
    }

    lines.push(String::new());
    lines.push("Lighting controls:".to_string());
    if let Some(lighting) = lighting {
        lines.push(format!(
            "  backend={}, direct={}, privileged={}, authorization={}, status={}",
            lighting.backend,
            lighting.direct_writable,
            lighting.privileged_writable,
            lighting.authorization,
            lighting.status
        ));
    } else {
        lines.push("  no lighting state received".to_string());
    }

    lines.push(String::new());
    lines.push(format!(
        "GPU mode: backend={}, access=external-service",
        caps.gpu_backend
    ));
    lines.push(format!(
        "Battery limit: backend={}, direct={}, privileged={}, authorization={}",
        caps.battery_limit_backend,
        caps.battery_limit_direct_write,
        caps.battery_limit_privileged_write,
        caps.battery_limit_authorization
    ));
    lines.push(String::new());
    lines.push(
        "Authorization is per operation. This report does not perform an authorization check."
            .to_string(),
    );
    lines.join("\n")
}

fn update_setup_dependency_row(
    status: &SetupStatus,
    kind: DependencyKind,
    row: &adw::ActionRow,
    value: &gtk::Label,
) {
    if let Some(dependency) = status.dependency(kind) {
        value.set_text(dependency.state.label());
        set_setup_value_status(
            value,
            match dependency.state {
                DependencyState::Connected | DependencyState::Ready => "status-ok",
                DependencyState::NotRelevant | DependencyState::Unknown => "status-info",
                DependencyState::NotAvailable | DependencyState::Inactive => "status-warning",
                DependencyState::Unreachable => "status-error",
            },
        );
        let required_for = if dependency.required_for.is_empty() {
            String::new()
        } else {
            format!(" Required for: {}.", dependency.required_for.join(", "))
        };
        row.set_subtitle(&format!("{}{required_for}", dependency.summary));
    } else {
        value.set_text("Checking");
        row.set_subtitle("No readiness result has been received yet.");
    }
}

fn update_setup_permission_row(
    status: &SetupStatus,
    kind: PermissionKind,
    row: &adw::ActionRow,
    value: &gtk::Label,
) {
    if let Some(permission) = status.permission(kind) {
        value.set_text(permission.state.label());
        set_setup_value_status(
            value,
            match permission.state {
                PermissionState::Writable => "status-ok",
                PermissionState::Unknown | PermissionState::Unsupported => "status-info",
                PermissionState::ReadOnly => "status-warning",
                PermissionState::Unavailable => "status-error",
            },
        );
        row.set_subtitle(&permission.summary);
    } else {
        value.set_text("Checking");
        row.set_subtitle("No permission result has been received yet.");
    }
}

fn set_setup_value_status(value: &gtk::Label, class_name: &str) {
    for class in ["status-ok", "status-info", "status-warning", "status-error"] {
        value.remove_css_class(class);
    }
    value.add_css_class(class_name);
}

fn dashboard_nav_card(
    card: &MetricCard,
    stack: &adw::ViewStack,
    page_name: &'static str,
    accessible_label: &'static str,
) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("dashboard-card-button");
    button.set_hexpand(true);
    button.set_tooltip_text(Some(accessible_label));
    button.update_property(&[gtk::accessible::Property::Label(accessible_label)]);
    button.set_child(Some(card.widget()));
    let stack = stack.clone();
    button.connect_clicked(move |_| stack.set_visible_child_name(page_name));
    button
}

fn dashboard_panel(title: &str, icon_name: &str) -> (gtk::Box, gtk::Box) {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.add_css_class("surface-card");
    root.add_css_class("dashboard-panel");
    root.set_hexpand(true);
    root.set_vexpand(false);
    root.set_valign(gtk::Align::Start);

    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(17);
    icon.add_css_class("dashboard-panel-icon");
    icon.set_accessible_role(gtk::AccessibleRole::Img);
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.add_css_class("dashboard-panel-title");
    heading.append(&icon);
    heading.append(&label);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.append(&heading);
    root.append(&body);
    (root, body)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DashboardLayout {
    Narrow,
    Medium,
    Wide,
}

fn dashboard_layout(width: i32) -> DashboardLayout {
    match width {
        ..640 => DashboardLayout::Narrow,
        640..1160 => DashboardLayout::Medium,
        _ => DashboardLayout::Wide,
    }
}

#[allow(clippy::too_many_arguments)]
fn update_dashboard_flow_layout(
    width: i32,
    primary_metrics: &gtk::FlowBox,
    secondary_metrics: &gtk::FlowBox,
    current_modes: &gtk::FlowBox,
    quick_performance: &gtk::FlowBox,
    cockpit_middle: &gtk::FlowBox,
    live_performance: &gtk::FlowBox,
    cockpit_lower: &gtk::FlowBox,
    previous: &std::cell::Cell<Option<DashboardLayout>>,
) {
    if width <= 0 {
        return;
    }

    let layout = dashboard_layout(width);
    if previous.replace(Some(layout)) == Some(layout) {
        return;
    }

    let secondary_columns = dashboard_secondary_columns(width);
    let mode_columns = dashboard_mode_columns(width);
    let columns = match layout {
        DashboardLayout::Narrow => [1, secondary_columns, mode_columns, 1, 1, 1, 1],
        DashboardLayout::Medium => [2, secondary_columns, mode_columns, 2, 1, 2, 1],
        DashboardLayout::Wide => [3, secondary_columns, mode_columns, 2, 2, 2, 2],
    };
    for (flow, max) in [
        (primary_metrics, columns[0]),
        (secondary_metrics, columns[1]),
        (current_modes, columns[2]),
        (quick_performance, columns[3]),
        (cockpit_middle, columns[4]),
        (live_performance, columns[5]),
        (cockpit_lower, columns[6]),
    ] {
        flow.set_max_children_per_line(max);
    }
}

fn dashboard_secondary_columns(width: i32) -> u32 {
    match dashboard_layout(width) {
        DashboardLayout::Narrow => 1,
        DashboardLayout::Medium => 3,
        DashboardLayout::Wide => 5,
    }
}

fn dashboard_mode_columns(width: i32) -> u32 {
    dashboard_secondary_columns(width)
}

fn dashboard_mode_item(title: &str) -> (gtk::Box, gtk::Label) {
    let item = gtk::Box::new(gtk::Orientation::Vertical, 2);
    item.add_css_class("dashboard-mode-item");
    item.set_hexpand(true);

    let title = gtk::Label::new(Some(title));
    title.set_xalign(0.0);
    title.add_css_class("dashboard-mode-title");
    let value = gtk::Label::new(Some("—"));
    value.set_xalign(0.0);
    value.set_ellipsize(gtk::pango::EllipsizeMode::End);
    value.add_css_class("dashboard-mode-value");
    item.append(&title);
    item.append(&value);
    (item, value)
}

fn dashboard_status_row(title: &str) -> (gtk::Box, gtk::Label, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("dashboard-status-row");
    let indicator = gtk::Label::new(Some("●"));
    indicator.add_css_class("dashboard-status-indicator");
    let title = gtk::Label::new(Some(title));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("dashboard-status-name");
    let value = gtk::Label::new(Some("—"));
    value.set_xalign(1.0);
    value.set_halign(gtk::Align::End);
    value.add_css_class("dashboard-status-value");
    row.append(&indicator);
    row.append(&title);
    row.append(&value);
    (row, indicator, value)
}

fn set_dashboard_status(indicator: &gtk::Label, label: &gtk::Label, text: &str, class: &str) {
    for old_class in ["status-ok", "status-info", "status-warning", "status-error"] {
        indicator.remove_css_class(old_class);
        label.remove_css_class(old_class);
    }
    label.set_text(text);
    indicator.add_css_class(class);
    label.add_css_class(class);
}

fn style_linked_toggle_row(row: &gtk::Box) {
    row.add_css_class("linked");
    row.set_spacing(0);
    row.set_halign(gtk::Align::End);
    row.set_valign(gtk::Align::Center);
}

fn style_dropdown_control(dropdown: &gtk::DropDown) {
    dropdown.set_halign(gtk::Align::End);
    dropdown.set_valign(gtk::Align::Center);
}

fn style_apply_button(button: &gtk::Button) {
    button.set_halign(gtk::Align::End);
    button.set_valign(gtk::Align::Center);
    button.set_size_request(96, -1);
}

fn style_inline_control_box(control_box: &gtk::Box) {
    control_box.set_spacing(10);
    control_box.set_halign(gtk::Align::End);
    control_box.set_valign(gtk::Align::Center);
}

fn style_spin_control(spin: &gtk::SpinButton, width_chars: i32) {
    spin.set_width_chars(width_chars);
    spin.set_alignment(1.0);
    spin.set_halign(gtk::Align::End);
    spin.set_valign(gtk::Align::Center);
}

fn style_scale_control(scale: &gtk::Scale) {
    scale.set_digits(0);
    scale.set_hexpand(true);
    scale.set_valign(gtk::Align::Center);
}

fn style_combo_control(combo: &gtk::ComboBoxText) {
    combo.set_halign(gtk::Align::End);
    combo.set_valign(gtk::Align::Center);
}

fn format_cpu_freq_value(mhz: Option<u32>) -> String {
    mhz.map(|value| format!("{value} MHz"))
        .unwrap_or_else(|| "unavailable".to_string())
}

fn cpu_dashboard_summary(cpu: &CpuTelemetry) -> String {
    let mut parts = Vec::new();
    if let Some(usage) = cpu.usage_percent {
        parts.push(format!("{usage:.0}% utilisation"));
    }
    if let Some(clock) = cpu.avg_freq_mhz {
        parts.push(format!("{:.2} GHz", clock / 1000.0));
    }
    if parts.is_empty() {
        "Telemetry available".to_string()
    } else {
        parts.join(" · ")
    }
}

fn gpu_dashboard_summary(telemetry: &TelemetrySnapshot, mode: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(usage) = telemetry.gpu_usage_percent {
        parts.push(format!("{usage:.0}% load"));
    }
    if let (Some(used), Some(total)) = (
        telemetry.gpu_vram_used_bytes,
        telemetry.gpu_vram_total_bytes,
    ) {
        parts.push(format!(
            "{:.1} / {:.1} GiB VRAM",
            used as f64 / 1024_f64.powi(3),
            total as f64 / 1024_f64.powi(3)
        ));
    }
    if let Some(mode) = mode {
        parts.push(format!("Mode {mode}"));
    }
    if parts.is_empty() {
        if telemetry.gpu_temp_c.is_some() {
            "Telemetry available".to_string()
        } else {
            "Telemetry unavailable".to_string()
        }
    } else {
        parts.join(" · ")
    }
}

fn cpu_power_preset_summary(cpu: &CpuTelemetry) -> String {
    let mut parts = Vec::new();
    if let Some(governor) = cpu.governor.as_deref() {
        parts.push(format!("governor {governor}"));
    }
    if let Some(epp) = cpu.epp.as_deref() {
        parts.push(format!("EPP {epp}"));
    }

    if parts.is_empty() {
        "Choose Quiet, Balanced, or Performance, then apply.".to_string()
    } else {
        format!(
            "Current hint: {}. Choose a preset, then apply.",
            parts.join(" / ")
        )
    }
}

fn cpu_quick_apply_subtitle(caps: &CpuCaps) -> String {
    let writable_controls = [
        CpuControlKind::Boost,
        CpuControlKind::PowerMode,
        CpuControlKind::FreqLimits,
    ]
    .into_iter()
    .filter(|kind| caps.control_actionable(*kind))
    .count();

    match writable_controls {
        0 => "Quick controls stay visible for diagnostics, but no quick CPU writes are available right now.".to_string(),
        1 => "Apply the writable quick-control change from this section.".to_string(),
        _ => "Apply staged turbo, preset, and frequency changes together.".to_string(),
    }
}

fn cpu_controls_require_authorization(caps: &CpuCaps, kinds: &[CpuControlKind]) -> bool {
    caps.control_access.iter().any(|control| {
        kinds.contains(&control.kind)
            && (control.status == CpuAccessState::AuthorizationRequired
                || control.authorization == CpuAuthorization::Required)
    })
}

fn cpu_policy_apply_subtitle(caps: &CpuCaps) -> String {
    let governor_writable = caps.control_actionable(CpuControlKind::Governor);
    let epp_writable = caps.control_actionable(CpuControlKind::Epp);

    match (governor_writable, epp_writable) {
        (true, true) => "Apply governor and EPP together.".to_string(),
        (true, false) => "Apply the selected governor.".to_string(),
        (false, true) => "Apply the selected EPP.".to_string(),
        (false, false) => {
            "Policy controls stay visible for diagnostics, but writes are not available right now."
                .to_string()
        }
    }
}

fn cpu_control_subtitle(
    caps: &CpuCaps,
    kind: CpuControlKind,
    available_text: &str,
    unsupported_text: &str,
) -> String {
    let access_reason = caps
        .control_access(kind)
        .map(|entry| entry.reason.trim())
        .filter(|reason| !reason.is_empty());

    match caps.control_state(kind) {
        CpuAccessState::Available => available_text.to_string(),
        CpuAccessState::AuthorizationRequired => access_reason
            .unwrap_or("Administrator access required. Apply to authenticate.")
            .to_string(),
        CpuAccessState::AuthorizationDenied => access_reason
            .unwrap_or("Administrator authorization was denied. Apply to retry.")
            .to_string(),
        CpuAccessState::HelperMissing => access_reason
            .unwrap_or("The privileged CPU helper is unavailable.")
            .to_string(),
        CpuAccessState::ReadOnly => access_reason
            .unwrap_or("This supported CPU control is read-only.")
            .to_string(),
        CpuAccessState::PermissionDenied => access_reason
            .unwrap_or("Readable, but not writable by the current user.")
            .to_string(),
        CpuAccessState::MissingBackend => access_reason
            .unwrap_or("Required CPU policy backend is not available.")
            .to_string(),
        CpuAccessState::TemporarilyUnavailable => access_reason
            .unwrap_or("Temporarily unavailable; inspect diagnostics.")
            .to_string(),
        CpuAccessState::Unsupported | CpuAccessState::Unknown => unsupported_text.to_string(),
    }
}

fn cpu_toggle_row_subtitle(
    core: &CpuCoreTelemetry,
    caps: &CpuCaps,
    core_online_writable: bool,
) -> String {
    let mut parts = vec![
        format_cpu_toggle_summary(
            core.physical_core_index,
            core.thread_index,
            core.thread_count,
        ),
        if core.online {
            "currently online".to_string()
        } else {
            "currently offline".to_string()
        },
    ];

    if core.logical_cpu_id == 0 {
        parts.push("CPU 0 stays online".to_string());
    } else if !caps.has_core_online {
        parts.push("Online/offline control is not supported".to_string());
    } else if !core_online_writable {
        parts.push("Read-only for the current user".to_string());
    } else {
        parts.push("Use the switch to stage a change".to_string());
    }

    parts.join("  |  ")
}

fn cpu_toggle_dialog_title(desired_online: bool) -> &'static str {
    if desired_online {
        "Bring Logical CPU Online?"
    } else {
        "Take Logical CPU Offline?"
    }
}

fn cpu_toggle_dialog_body(core: &CpuCoreTelemetry, desired_online: bool) -> String {
    let action = if desired_online {
        "bring it back online"
    } else {
        "take it offline"
    };
    format!(
        "Logical CPU {} maps to {}. Changing its online state can affect responsiveness, thermals, and scheduler behavior. Continue and {action}?",
        core.logical_cpu_id,
        format_cpu_toggle_summary(
            core.physical_core_index,
            core.thread_index,
            core.thread_count,
        ),
    )
}

fn build_detail_rows(group: &adw::PreferencesGroup, count: usize) -> Vec<DetailRow> {
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        rows.push(append_detail_row(group));
    }
    rows
}

fn update_detail_rows(rows: &[DetailRow], entries: &[(String, String)]) {
    for (idx, detail_row) in rows.iter().enumerate() {
        if let Some((title, value)) = entries.get(idx) {
            detail_row.row.set_title(title);
            detail_row.value_label.set_text(value);
            detail_row.row.set_visible(true);
        } else {
            detail_row.row.set_title("");
            detail_row.value_label.set_text("");
            detail_row.row.set_visible(false);
        }
    }
}

fn append_detail_row(group: &adw::PreferencesGroup) -> DetailRow {
    let row = adw::ActionRow::builder().title("").build();
    let value = gtk::Label::new(Some(""));
    value.set_xalign(1.0);
    value.set_halign(gtk::Align::End);
    value.add_css_class("monospace");
    row.add_suffix(&value);
    row.set_activatable(false);
    row.set_visible(false);
    group.add(&row);
    DetailRow {
        row,
        value_label: value,
    }
}

fn update_detail_rows_dynamic(
    group: &adw::PreferencesGroup,
    rows: &mut Vec<DetailRow>,
    entries: &[(String, String)],
) {
    while rows.len() < entries.len() {
        rows.push(append_detail_row(group));
    }
    update_detail_rows(rows, entries);
}

fn lighting_from_dbus(map: HashMap<String, OwnedValue>) -> Option<LightingInfo> {
    let supported_modes =
        dbus_decode::strings(&map, dbus_keys::lighting::SUPPORTED_MODES).unwrap_or_default();
    let supported_zones =
        dbus_decode::strings(&map, dbus_keys::lighting::SUPPORTED_ZONES).unwrap_or_default();
    let max_brightness =
        dbus_decode::unsigned(&map, dbus_keys::lighting::MAX_BRIGHTNESS).unwrap_or(0);
    let supports_brightness = dbus_decode::boolean(&map, dbus_keys::lighting::SUPPORTS_BRIGHTNESS)
        .unwrap_or(max_brightness > 0);
    let supports_modes = dbus_decode::boolean(&map, dbus_keys::lighting::SUPPORTS_MODES)
        .unwrap_or(!supported_modes.is_empty());

    Some(LightingInfo {
        backend: dbus_decode::string(&map, dbus_keys::lighting::BACKEND)
            .unwrap_or_else(|| "Unknown backend".to_string()),
        backend_kind: dbus_decode::string(&map, dbus_keys::lighting::BACKEND_KIND)
            .or_else(|| dbus_decode::string(&map, dbus_keys::lighting::BACKEND))
            .unwrap_or_else(|| "none".to_string()),
        device: dbus_decode::string(&map, dbus_keys::lighting::DEVICE)
            .unwrap_or_else(|| "Unknown device".to_string()),
        brightness: dbus_decode::unsigned(&map, dbus_keys::lighting::BRIGHTNESS).unwrap_or(0),
        max_brightness,
        can_set: dbus_decode::boolean(&map, dbus_keys::lighting::CAN_SET).unwrap_or(false),
        direct_writable: dbus_decode::boolean(&map, dbus_keys::lighting::DIRECT_WRITABLE)
            .unwrap_or(false),
        privileged_writable: dbus_decode::boolean(&map, dbus_keys::lighting::PRIVILEGED_WRITABLE)
            .unwrap_or(false),
        authorization_required: dbus_decode::boolean(
            &map,
            dbus_keys::lighting::AUTHORIZATION_REQUIRED,
        )
        .unwrap_or(false),
        authorization: dbus_decode::string(&map, dbus_keys::lighting::AUTHORIZATION)
            .unwrap_or_else(|| "not_applicable".to_string()),
        mode: dbus_decode::string(&map, dbus_keys::lighting::MODE)
            .unwrap_or_else(|| "Current mode not reported".to_string()),
        supports_brightness,
        supports_modes,
        supports_rgb: dbus_decode::boolean(&map, dbus_keys::lighting::SUPPORTS_RGB)
            .unwrap_or(false),
        supports_argb: dbus_decode::boolean(&map, dbus_keys::lighting::SUPPORTS_ARGB)
            .unwrap_or(false),
        supports_zones: dbus_decode::boolean(&map, dbus_keys::lighting::SUPPORTS_ZONES)
            .unwrap_or(!supported_zones.is_empty()),
        supports_per_key: dbus_decode::boolean(&map, dbus_keys::lighting::SUPPORTS_PER_KEY)
            .unwrap_or(false),
        rgb_hex: dbus_decode::string(&map, dbus_keys::lighting::RGB_HEX),
        secondary_rgb_hex: dbus_decode::string(&map, dbus_keys::lighting::SECONDARY_RGB_HEX),
        supported_modes,
        supports_speed: dbus_decode::boolean(&map, dbus_keys::lighting::SUPPORTS_SPEED)
            .unwrap_or(false),
        supported_speeds: dbus_decode::strings(&map, dbus_keys::lighting::SUPPORTED_SPEEDS)
            .unwrap_or_default(),
        speed: dbus_decode::string(&map, dbus_keys::lighting::SPEED),
        supported_directions: dbus_decode::strings(&map, dbus_keys::lighting::SUPPORTED_DIRECTIONS)
            .unwrap_or_default(),
        direction: dbus_decode::string(&map, dbus_keys::lighting::DIRECTION),
        supported_zones,
        active_zone: dbus_decode::string(&map, dbus_keys::lighting::ACTIVE_ZONE),
        apply_outcome: dbus_decode::string(&map, dbus_keys::lighting::APPLY_OUTCOME),
        status: dbus_decode::string(&map, dbus_keys::lighting::STATUS)
            .unwrap_or_else(|| "unknown".to_string()),
        last_error: dbus_decode::string(&map, dbus_keys::lighting::LAST_ERROR),
        diagnostics_summary: dbus_decode::string(&map, dbus_keys::lighting::DIAGNOSTICS_SUMMARY),
        diagnostics_details: dbus_decode::string(&map, dbus_keys::lighting::DIAGNOSTICS_DETAILS),
        fallback_reason: dbus_decode::string(&map, dbus_keys::lighting::FALLBACK_REASON),
        unavailable_reason: dbus_decode::string(&map, dbus_keys::lighting::UNAVAILABLE_REASON),
        permission_warning: dbus_decode::string(&map, dbus_keys::lighting::PERMISSION_WARNING),
    })
}

fn fan_state_from_dbus(map: HashMap<String, OwnedValue>) -> FanState {
    let caps = map
        .get(dbus_keys::FAN_CAPS_KEY)
        .cloned()
        .and_then(|value| HashMap::<String, OwnedValue>::try_from(value).ok())
        .map(fan_caps_from_dbus)
        .unwrap_or_else(|| FanCaps::from_fans(&[]));
    let fans = map
        .get(dbus_keys::FAN_INFOS_KEY)
        .cloned()
        .and_then(|value| Vec::<HashMap<String, OwnedValue>>::try_from(value).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(fan_info_from_dbus)
        .collect::<Vec<_>>();
    let mode = map
        .get("mode")
        .and_then(|value| <&str>::try_from(value).ok())
        .map(FanControlMode::parse)
        .unwrap_or(FanControlMode::ReadOnly);
    let sync_enabled = map
        .get("sync_enabled")
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false);
    let last_action = map
        .get("last_action")
        .and_then(|value| <&str>::try_from(value).ok())
        .map(|value| value.to_string());
    let active_boost_fan_id = map
        .get("active_boost_fan_id")
        .and_then(|value| <&str>::try_from(value).ok())
        .map(|value| value.to_string());
    let active_boost_until_ms = map.get("active_boost_until_ms").and_then(u64_from_value);
    let active_curve_summary = map
        .get("active_curve_summary")
        .and_then(|value| <&str>::try_from(value).ok())
        .map(|value| value.to_string());
    let warnings = map
        .get("warnings")
        .cloned()
        .and_then(|value| Vec::<String>::try_from(value).ok())
        .unwrap_or_default();

    FanState {
        fans,
        caps,
        mode,
        sync_enabled,
        last_action,
        active_boost_fan_id,
        active_boost_until_ms,
        active_curve_summary,
        warnings,
    }
}

fn fan_caps_from_dbus(map: HashMap<String, OwnedValue>) -> FanCaps {
    FanCaps {
        has_fan_reading: dbus_decode::boolean(&map, dbus_keys::caps::HAS_FAN_READING)
            .unwrap_or(false),
        has_fan_curves: dbus_decode::boolean(&map, dbus_keys::caps::HAS_FAN_CURVES)
            .unwrap_or(false),
        fan_curve_readable: dbus_decode::boolean(&map, dbus_keys::fan_caps::FAN_CURVE_READABLE)
            .unwrap_or(false),
        fan_curve_writable: dbus_decode::boolean(&map, dbus_keys::fan_caps::FAN_CURVE_WRITABLE)
            .unwrap_or(false),
        has_fan_manual_percent: dbus_decode::boolean(&map, dbus_keys::caps::HAS_FAN_MANUAL_PERCENT)
            .unwrap_or(false),
        has_fan_manual_rpm_target: dbus_decode::boolean(
            &map,
            dbus_keys::caps::HAS_FAN_MANUAL_RPM_TARGET,
        )
        .unwrap_or(false),
        has_individual_fan_control: dbus_decode::boolean(
            &map,
            dbus_keys::caps::HAS_INDIVIDUAL_FAN_CONTROL,
        )
        .unwrap_or(false),
        has_fan_sync_control: dbus_decode::boolean(&map, dbus_keys::caps::HAS_FAN_SYNC_CONTROL)
            .unwrap_or(false),
        has_fan_boost: dbus_decode::boolean(&map, dbus_keys::caps::HAS_FAN_BOOST).unwrap_or(false),
        fan_count: dbus_decode::unsigned_u32(&map, dbus_keys::caps::FAN_COUNT).unwrap_or(0),
        fan_mapping_confidence: dbus_decode::string(
            &map,
            dbus_keys::fan_caps::FAN_MAPPING_CONFIDENCE,
        )
        .as_deref()
        .map(FanMappingConfidence::parse)
        .unwrap_or(FanMappingConfidence::Unknown),
        fan_backend: dbus_decode::string(&map, dbus_keys::caps::FAN_BACKEND).unwrap_or_default(),
        endpoints: dbus_decode::strings(&map, dbus_keys::caps::ENDPOINTS).unwrap_or_default(),
        notes: dbus_decode::strings(&map, dbus_keys::caps::NOTES).unwrap_or_default(),
        warnings: dbus_decode::strings(&map, dbus_keys::fan_state::WARNINGS).unwrap_or_default(),
    }
}

fn fan_info_from_dbus(map: HashMap<String, OwnedValue>) -> Option<FanInfo> {
    fn s(map: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
        map.get(key)
            .and_then(|value| <&str>::try_from(value).ok())
            .map(|value| value.to_string())
    }
    fn b(map: &HashMap<String, OwnedValue>, key: &str) -> bool {
        map.get(key)
            .and_then(|value| bool::try_from(value).ok())
            .unwrap_or(false)
    }
    fn u32v(map: &HashMap<String, OwnedValue>, key: &str) -> Option<u32> {
        map.get(key)
            .and_then(u64_from_value)
            .and_then(|value| u32::try_from(value).ok())
    }
    fn u8v(map: &HashMap<String, OwnedValue>, key: &str) -> Option<u8> {
        map.get(key)
            .and_then(u64_from_value)
            .and_then(|value| u8::try_from(value).ok())
    }
    fn vec_string(map: &HashMap<String, OwnedValue>, key: &str) -> Vec<String> {
        map.get(key)
            .cloned()
            .and_then(|value| Vec::<String>::try_from(value).ok())
            .unwrap_or_default()
    }

    Some(FanInfo {
        id: s(&map, dbus_keys::FAN_INFO_ID_KEY)?,
        index: u32v(&map, dbus_keys::FAN_INFO_INDEX_KEY).unwrap_or(0),
        label: s(&map, dbus_keys::FAN_INFO_LABEL_KEY)?,
        mapping_confidence: s(&map, dbus_keys::FAN_INFO_MAPPING_CONFIDENCE_KEY)
            .map(|value| FanMappingConfidence::parse(&value))
            .unwrap_or(FanMappingConfidence::Unknown),
        current_rpm: u32v(&map, dbus_keys::FAN_INFO_CURRENT_RPM_KEY),
        min_rpm: u32v(&map, dbus_keys::FAN_INFO_MIN_RPM_KEY),
        max_rpm: u32v(&map, dbus_keys::FAN_INFO_MAX_RPM_KEY),
        current_percent: u8v(&map, dbus_keys::FAN_INFO_CURRENT_PERCENT_KEY),
        rpm_readable: b(&map, dbus_keys::FAN_INFO_RPM_READABLE_KEY),
        pwm_endpoint_verified: b(&map, dbus_keys::FAN_INFO_PWM_ENDPOINT_VERIFIED_KEY),
        direct_write: b(&map, dbus_keys::FAN_INFO_DIRECT_WRITE_KEY),
        privileged_write: b(&map, dbus_keys::FAN_INFO_PRIVILEGED_WRITE_KEY),
        authorization: s(&map, dbus_keys::FAN_INFO_AUTHORIZATION_KEY)
            .unwrap_or_else(|| "not_checked".to_string()),
        access_state: s(&map, dbus_keys::FAN_INFO_ACCESS_STATE_KEY)
            .unwrap_or_else(|| "telemetry_only".to_string()),
        controllable: b(&map, dbus_keys::FAN_INFO_CONTROLLABLE_KEY),
        supports_manual_percent: b(&map, dbus_keys::FAN_INFO_SUPPORTS_MANUAL_PERCENT_KEY),
        supports_manual_rpm_target: b(&map, dbus_keys::FAN_INFO_SUPPORTS_MANUAL_RPM_TARGET_KEY),
        supports_curve: b(&map, dbus_keys::FAN_INFO_SUPPORTS_CURVE_KEY),
        supports_auto: b(&map, dbus_keys::FAN_INFO_SUPPORTS_AUTO_KEY),
        backend: s(&map, dbus_keys::FAN_INFO_BACKEND_KEY).unwrap_or_else(|| "unknown".to_string()),
        endpoints: vec_string(&map, dbus_keys::FAN_INFO_ENDPOINTS_KEY),
        notes: vec_string(&map, dbus_keys::FAN_INFO_NOTES_KEY),
        warnings: vec_string(&map, dbus_keys::FAN_INFO_WARNINGS_KEY),
    })
}

fn fan_row_from_dbus(map: &HashMap<String, OwnedValue>) -> Option<FanTelemetry> {
    let hwmon_device = map
        .get(dbus_keys::FAN_ROW_HWMON_DEVICE_KEY)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(|value| value.to_string())?;
    let hwmon_path = map
        .get(dbus_keys::FAN_ROW_HWMON_PATH_KEY)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(|value| value.to_string())?;
    let input_path = map
        .get(dbus_keys::FAN_ROW_INPUT_PATH_KEY)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(|value| value.to_string())?;
    let display_label = map
        .get(dbus_keys::FAN_ROW_DISPLAY_LABEL_KEY)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(|value| value.to_string())?;

    Some(FanTelemetry {
        hwmon_device,
        hwmon_path,
        input_path,
        raw_label: map
            .get(dbus_keys::FAN_ROW_RAW_LABEL_KEY)
            .and_then(|value| <&str>::try_from(value).ok())
            .map(|value| value.to_string()),
        display_label,
        rpm: map
            .get(dbus_keys::FAN_ROW_RPM_KEY)
            .and_then(u64_from_value)
            .and_then(|value| u32::try_from(value).ok()),
    })
}

fn clamped_scroller(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(1260);
    clamp.set_tightening_threshold(900);
    clamp.set_child(Some(child));

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_vexpand(true);
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_child(Some(&clamp));
    scroller
}

fn update_diagnostics_buffer(
    buffer: &gtk::TextBuffer,
    scroller: &gtk::ScrolledWindow,
    next_text: &str,
) {
    let current_text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
    if current_text.as_str() == next_text {
        return;
    }

    let adjustment = scroller.vadjustment();
    let scroll_value = adjustment.value();
    let selection = buffer
        .selection_bounds()
        .map(|(start, end)| (start.offset(), end.offset()));
    let cursor_offset = buffer.iter_at_mark(&buffer.get_insert()).offset();
    let max_offset = next_text.chars().count().min(i32::MAX as usize) as i32;

    buffer.set_text(next_text);

    if let Some((start_offset, end_offset)) = selection {
        let start_iter = buffer.iter_at_offset(start_offset.clamp(0, max_offset));
        let end_iter = buffer.iter_at_offset(end_offset.clamp(0, max_offset));
        buffer.select_range(&start_iter, &end_iter);
    } else {
        let cursor_iter = buffer.iter_at_offset(cursor_offset.clamp(0, max_offset));
        buffer.place_cursor(&cursor_iter);
    }

    restore_adjustment_value(&adjustment, scroll_value);
    glib::idle_add_local_once(move || {
        restore_adjustment_value(&adjustment, scroll_value);
    });
}

fn restore_adjustment_value(adjustment: &gtk::Adjustment, value: f64) {
    let lower = adjustment.lower();
    let upper = (adjustment.upper() - adjustment.page_size()).max(lower);
    adjustment.set_value(value.clamp(lower, upper));
}

fn rgba_to_hex(rgba: &gtk::gdk::RGBA) -> String {
    fn to_u8(v: f32) -> u8 {
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    }
    let r = to_u8(rgba.red());
    let g = to_u8(rgba.green());
    let b = to_u8(rgba.blue());
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn rgb_hex_to_rgba(value: &str) -> Option<gtk::gdk::RGBA> {
    let rgb = RgbColor::parse_hex(value).ok()?;
    Some(gtk::gdk::RGBA::new(
        rgb.r as f32 / 255.0,
        rgb.g as f32 / 255.0,
        rgb.b as f32 / 255.0,
        1.0,
    ))
}

fn cpu_path_access_from_dbus(map: &HashMap<String, OwnedValue>) -> Option<CpuPathAccess> {
    let path = map
        .get(dbus_keys::CPU_PATH_PATH_KEY)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(|value| value.to_string())?;
    let readable = map
        .get(dbus_keys::CPU_PATH_READABLE_KEY)
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false);
    let writable = map
        .get(dbus_keys::CPU_PATH_WRITABLE_KEY)
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false);

    Some(CpuPathAccess {
        path,
        readable,
        writable,
    })
}

fn cpu_control_access_from_dbus(map: &HashMap<String, OwnedValue>) -> Option<CpuControlAccess> {
    let kind = map
        .get(dbus_keys::CPU_CONTROL_KIND_KEY)
        .and_then(|value| <&str>::try_from(value).ok())
        .and_then(CpuControlKind::parse)?;
    let status = map
        .get(dbus_keys::CPU_CONTROL_STATUS_KEY)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(CpuAccessState::parse)
        .unwrap_or(CpuAccessState::Unknown);
    let reason = map
        .get(dbus_keys::CPU_CONTROL_REASON_KEY)
        .and_then(|value| <&str>::try_from(value).ok())
        .unwrap_or_default()
        .to_string();
    let direct_write = map
        .get(dbus_keys::CPU_CONTROL_DIRECT_WRITE_KEY)
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(status == CpuAccessState::Available);
    let privileged_write = map
        .get(dbus_keys::CPU_CONTROL_PRIVILEGED_WRITE_KEY)
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false);
    let authorization = map
        .get(dbus_keys::CPU_CONTROL_AUTHORIZATION_KEY)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(CpuAuthorization::parse)
        .unwrap_or_default();
    let paths = map
        .get(dbus_keys::CPU_CONTROL_PATHS_KEY)
        .cloned()
        .and_then(|value| Vec::<HashMap<String, OwnedValue>>::try_from(value).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| cpu_path_access_from_dbus(&entry))
        .collect();

    Some(CpuControlAccess {
        kind,
        status,
        reason,
        direct_write,
        privileged_write,
        authorization,
        paths,
    })
}

fn cpu_caps_from_dbus(map: &HashMap<String, OwnedValue>) -> CpuCaps {
    fn b(map: &HashMap<String, OwnedValue>, key: &str) -> bool {
        map.get(key)
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false)
    }
    fn u32v(map: &HashMap<String, OwnedValue>, key: &str) -> Option<u32> {
        map.get(key)
            .and_then(u64_from_value)
            .and_then(|v| u32::try_from(v).ok())
    }
    fn s(map: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
        map.get(key)
            .and_then(|v| <&str>::try_from(v).ok())
            .map(|v| v.to_string())
    }
    fn vec_string(map: &HashMap<String, OwnedValue>, key: &str) -> Vec<String> {
        map.get(key)
            .cloned()
            .and_then(|v| Vec::<String>::try_from(v).ok())
            .unwrap_or_default()
    }

    CpuCaps {
        has_cpufreq: b(map, dbus_keys::cpu_caps::HAS_CPUFREQ),
        has_epp: b(map, dbus_keys::cpu_caps::HAS_EPP),
        has_boost_toggle: b(map, dbus_keys::cpu_caps::HAS_BOOST_TOGGLE),
        has_package_power: b(map, dbus_keys::cpu_caps::HAS_PACKAGE_POWER),
        policy_writable: b(map, dbus_keys::cpu_caps::POLICY_WRITABLE),
        has_min_freq_limit: b(map, dbus_keys::cpu_caps::HAS_MIN_FREQ_LIMIT),
        has_max_freq_limit: b(map, dbus_keys::cpu_caps::HAS_MAX_FREQ_LIMIT),
        has_governor: b(map, dbus_keys::cpu_caps::HAS_GOVERNOR),
        has_core_online: b(map, dbus_keys::cpu_caps::HAS_CORE_ONLINE),
        scaling_driver: s(map, dbus_keys::cpu_caps::SCALING_DRIVER),
        cpu_count: u32v(map, dbus_keys::cpu_caps::CPU_COUNT).unwrap_or(0),
        thread_count: u32v(map, dbus_keys::cpu_caps::THREAD_COUNT).unwrap_or(0),
        governor_choices: vec_string(map, dbus_keys::cpu_caps::GOVERNOR_CHOICES),
        epp_choices: vec_string(map, dbus_keys::cpu_caps::EPP_CHOICES),
        sysfs_paths: vec_string(map, dbus_keys::cpu_caps::SYSFS_PATHS),
        control_access: map
            .get(dbus_keys::CPU_CONTROL_ACCESS_KEY)
            .cloned()
            .and_then(|value| Vec::<HashMap<String, OwnedValue>>::try_from(value).ok())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|entry| cpu_control_access_from_dbus(&entry))
            .collect(),
    }
}

fn setup_status_from_dbus(map: &HashMap<String, OwnedValue>) -> SetupStatus {
    fn string(map: &HashMap<String, OwnedValue>, key: &str) -> String {
        map.get(key)
            .and_then(|value| <&str>::try_from(value).ok())
            .unwrap_or_default()
            .to_string()
    }
    fn strings(map: &HashMap<String, OwnedValue>, key: &str) -> Vec<String> {
        map.get(key)
            .cloned()
            .and_then(|value| Vec::<String>::try_from(value).ok())
            .unwrap_or_default()
    }

    let dependencies = map
        .get(dbus_keys::SETUP_DEPENDENCIES_KEY)
        .cloned()
        .and_then(|value| Vec::<HashMap<String, OwnedValue>>::try_from(value).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| {
            Some(DependencyStatus {
                kind: DependencyKind::parse(&string(&entry, dbus_keys::SETUP_KIND_KEY))?,
                state: DependencyState::parse(&string(&entry, dbus_keys::SETUP_STATE_KEY)),
                summary: string(&entry, dbus_keys::SETUP_SUMMARY_KEY),
                required_for: strings(&entry, dbus_keys::SETUP_REQUIRED_FOR_KEY),
                evidence: strings(&entry, dbus_keys::SETUP_EVIDENCE_KEY),
            })
        })
        .collect();
    let permissions = map
        .get(dbus_keys::SETUP_PERMISSIONS_KEY)
        .cloned()
        .and_then(|value| Vec::<HashMap<String, OwnedValue>>::try_from(value).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| {
            Some(PermissionStatus {
                kind: PermissionKind::parse(&string(&entry, dbus_keys::SETUP_KIND_KEY))?,
                state: PermissionState::parse(&string(&entry, dbus_keys::SETUP_STATE_KEY)),
                summary: string(&entry, dbus_keys::SETUP_SUMMARY_KEY),
                paths: strings(&entry, dbus_keys::SETUP_PATHS_KEY),
            })
        })
        .collect();
    let issues = map
        .get(dbus_keys::SETUP_ISSUES_KEY)
        .cloned()
        .and_then(|value| Vec::<HashMap<String, OwnedValue>>::try_from(value).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|entry| SetupIssue {
            severity: SetupSeverity::parse(&string(&entry, dbus_keys::SETUP_SEVERITY_KEY)),
            title: string(&entry, dbus_keys::SETUP_TITLE_KEY),
            summary: string(&entry, dbus_keys::SETUP_SUMMARY_KEY),
            guidance: string(&entry, dbus_keys::SETUP_GUIDANCE_KEY),
        })
        .collect();

    SetupStatus {
        checked_at_ms: map
            .get(dbus_keys::setup::CHECKED_AT_MS)
            .and_then(u64_from_value)
            .unwrap_or_default(),
        dependencies,
        permissions,
        issues,
    }
}

fn privileged_status_from_dbus(map: &HashMap<String, OwnedValue>) -> PrivilegedStatus {
    use dbus_keys::privileged_status as keys;

    let boolean = |key| dbus_decode::boolean(map, key).unwrap_or(false);
    let text = |key| dbus_decode::string(map, key).unwrap_or_default();
    let categories = dbus_decode::strings(map, keys::CATEGORIES_AVAILABLE)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| PrivilegedCategory::parse(&value).ok())
        .collect();
    let version = text(keys::HELPER_VERSION);

    PrivilegedStatus {
        system_bus_connected: boolean(keys::SYSTEM_BUS_CONNECTED),
        privileged_helper_installed: boolean(keys::HELPER_INSTALLED),
        privileged_helper_reachable: boolean(keys::HELPER_REACHABLE),
        privileged_helper_compatible: boolean(keys::HELPER_COMPATIBLE),
        privileged_helper_version: (!version.is_empty()).then_some(version),
        polkit_available: boolean(keys::POLKIT_AVAILABLE),
        authorization_backend: text(keys::AUTHORIZATION_BACKEND),
        authorization_state: AuthorizationState::parse(&text(keys::AUTHORIZATION_STATE)),
        privileged_categories_available: categories,
    }
}

fn caps_from_dbus(map: &HashMap<String, OwnedValue>) -> DeviceCaps {
    let b = |key| dbus_decode::boolean(map, key).unwrap_or(false);

    DeviceCaps {
        has_profiles: b(dbus_keys::caps::HAS_PROFILES),
        has_fan_curves: b(dbus_keys::caps::HAS_FAN_CURVES),
        has_fan_reading: b(dbus_keys::caps::HAS_FAN_READING),
        has_charge_limit: b(dbus_keys::caps::HAS_CHARGE_LIMIT),
        has_gpu_modes: b(dbus_keys::caps::HAS_GPU_MODES),
        has_aura: b(dbus_keys::caps::HAS_AURA),
        has_kbd_backlight: b(dbus_keys::caps::HAS_KBD_BACKLIGHT),
        has_fan_manual_percent: b(dbus_keys::caps::HAS_FAN_MANUAL_PERCENT),
        has_fan_manual_rpm_target: b(dbus_keys::caps::HAS_FAN_MANUAL_RPM_TARGET),
        has_individual_fan_control: b(dbus_keys::caps::HAS_INDIVIDUAL_FAN_CONTROL),
        has_fan_sync_control: b(dbus_keys::caps::HAS_FAN_SYNC_CONTROL),
        has_fan_boost: b(dbus_keys::caps::HAS_FAN_BOOST),
        fan_count: dbus_decode::unsigned_u32(map, dbus_keys::caps::FAN_COUNT).unwrap_or(0),
        fan_backend: dbus_decode::string(map, dbus_keys::caps::FAN_BACKEND).unwrap_or_default(),
        gpu_backend: dbus_decode::string(map, dbus_keys::caps::GPU_BACKEND)
            .unwrap_or_else(|| "none".to_string()),
        battery_limit_backend: dbus_decode::string(map, dbus_keys::caps::BATTERY_LIMIT_BACKEND)
            .unwrap_or_else(|| "none".to_string()),
        battery_limit_direct_write: b(dbus_keys::caps::BATTERY_LIMIT_DIRECT_WRITE),
        battery_limit_privileged_write: b(dbus_keys::caps::BATTERY_LIMIT_PRIVILEGED_WRITE),
        battery_limit_authorization: dbus_decode::string(
            map,
            dbus_keys::caps::BATTERY_LIMIT_AUTHORIZATION,
        )
        .unwrap_or_else(|| "not_required".to_string()),
        gpu_supported_modes: dbus_decode::strings(map, dbus_keys::caps::GPU_SUPPORTED_MODES)
            .unwrap_or_default(),
        gpu_external_authorization: dbus_decode::string(
            map,
            dbus_keys::caps::GPU_EXTERNAL_AUTHORIZATION,
        )
        .unwrap_or_else(|| "unavailable".to_string()),
        gpu_switch_state: dbus_decode::string(map, dbus_keys::caps::GPU_SWITCH_STATE)
            .map(|value| GpuSwitchState::parse(&value))
            .unwrap_or_default(),
        gpu_switch_hint: dbus_decode::string(map, dbus_keys::caps::GPU_SWITCH_HINT)
            .unwrap_or_default(),
        requires_logout_for_gpu_switch: b(dbus_keys::caps::REQUIRES_LOGOUT_FOR_GPU_SWITCH),
        requires_reboot_for_gpu_switch: b(dbus_keys::caps::REQUIRES_REBOOT_FOR_GPU_SWITCH),
        profile_access: feature_access_from_dbus(map, dbus_keys::PROFILE_ACCESS_PREFIX),
        charge_limit_access: feature_access_from_dbus(map, dbus_keys::CHARGE_LIMIT_ACCESS_PREFIX),
        gpu_mode_access: feature_access_from_dbus(map, dbus_keys::GPU_MODE_ACCESS_PREFIX),
        kbd_backlight_access: feature_access_from_dbus(map, dbus_keys::KBD_BACKLIGHT_ACCESS_PREFIX),
        endpoints: dbus_decode::strings(map, dbus_keys::caps::ENDPOINTS).unwrap_or_default(),
        notes: dbus_decode::strings(map, dbus_keys::caps::NOTES).unwrap_or_default(),
    }
}

fn feature_access_from_dbus(
    map: &HashMap<String, OwnedValue>,
    prefix: &str,
) -> FeatureAvailability {
    let status_key = dbus_keys::feature_access_status_key(prefix);
    let reason_key = dbus_keys::feature_access_reason_key(prefix);

    FeatureAvailability {
        status: map
            .get(&status_key)
            .and_then(|value| <&str>::try_from(value).ok())
            .map(FeatureAccessState::parse)
            .unwrap_or(FeatureAccessState::Unknown),
        reason: map
            .get(&reason_key)
            .and_then(|value| <&str>::try_from(value).ok())
            .unwrap_or_default()
            .to_string(),
    }
}

fn caps_text_from_dbus(map: HashMap<String, OwnedValue>) -> String {
    fn b(map: &HashMap<String, OwnedValue>, k: &str) -> Option<bool> {
        map.get(k).and_then(|v| bool::try_from(v).ok())
    }
    fn s(map: &HashMap<String, OwnedValue>, k: &str) -> Option<String> {
        map.get(k)
            .and_then(|v| <&str>::try_from(v).ok())
            .map(|v| v.to_string())
    }
    fn vec_string(map: &HashMap<String, OwnedValue>, k: &str) -> Option<Vec<String>> {
        map.get(k)
            .cloned()
            .and_then(|v| Vec::<String>::try_from(v).ok())
    }

    let mut lines = Vec::new();
    lines.push("Diagnostics".to_string());
    lines.push("".to_string());
    lines.push(format!(
        "Daemon API: {DAEMON_DBUS_NAME} {DAEMON_DBUS_PATH} ({DAEMON_DBUS_IFACE})"
    ));
    lines.push("".to_string());
    lines.push("DeviceCaps:".to_string());
    for (k, v) in [
        ("has_profiles", b(&map, "has_profiles")),
        ("has_fan_curves", b(&map, "has_fan_curves")),
        ("has_fan_reading", b(&map, "has_fan_reading")),
        ("has_fan_manual_percent", b(&map, "has_fan_manual_percent")),
        (
            "has_fan_manual_rpm_target",
            b(&map, "has_fan_manual_rpm_target"),
        ),
        (
            "has_individual_fan_control",
            b(&map, "has_individual_fan_control"),
        ),
        ("has_fan_sync_control", b(&map, "has_fan_sync_control")),
        ("has_fan_boost", b(&map, "has_fan_boost")),
        ("has_charge_limit", b(&map, "has_charge_limit")),
        ("has_gpu_modes", b(&map, "has_gpu_modes")),
        ("has_aura", b(&map, "has_aura")),
        ("has_kbd_backlight", b(&map, "has_kbd_backlight")),
        (
            "requires_logout_for_gpu_switch",
            b(&map, "requires_logout_for_gpu_switch"),
        ),
        (
            "requires_reboot_for_gpu_switch",
            b(&map, "requires_reboot_for_gpu_switch"),
        ),
    ] {
        let s = v
            .map(|v| v.to_string())
            .unwrap_or_else(|| "(n/a)".to_string());
        lines.push(format!("  {k}: {s}"));
    }
    if let Some(backend) = s(&map, "fan_backend") {
        lines.push(format!("  fan_backend: {backend}"));
    }
    for key in [
        "battery_limit_backend",
        "battery_limit_authorization",
        "gpu_backend",
        "gpu_external_authorization",
        "gpu_switch_state",
        "gpu_switch_hint",
    ] {
        if let Some(value) = s(&map, key) {
            lines.push(format!("  {key}: {value}"));
        }
    }
    for key in [
        "battery_limit_direct_write",
        "battery_limit_privileged_write",
    ] {
        if let Some(value) = b(&map, key) {
            lines.push(format!("  {key}: {value}"));
        }
    }
    if let Some(modes) = vec_string(&map, "gpu_supported_modes") {
        lines.push(format!("  gpu_supported_modes: {}", modes.join(", ")));
    }

    lines.push("".to_string());
    lines.push("Feature access:".to_string());
    for (label, prefix) in [
        ("profile", dbus_keys::PROFILE_ACCESS_PREFIX),
        ("charge_limit", dbus_keys::CHARGE_LIMIT_ACCESS_PREFIX),
        ("gpu_mode", dbus_keys::GPU_MODE_ACCESS_PREFIX),
        ("kbd_backlight", dbus_keys::KBD_BACKLIGHT_ACCESS_PREFIX),
    ] {
        let status = s(&map, &dbus_keys::feature_access_status_key(prefix))
            .unwrap_or_else(|| "unknown".to_string());
        let reason = s(&map, &dbus_keys::feature_access_reason_key(prefix))
            .unwrap_or_else(|| "(n/a)".to_string());
        lines.push(format!("  {label}: {status} ({reason})"));
    }

    if let Some(rows) = map
        .get(dbus_keys::caps::CONTROL_PRIVILEGE_MATRIX)
        .cloned()
        .and_then(|value| Vec::<HashMap<String, OwnedValue>>::try_from(value).ok())
    {
        lines.push("".to_string());
        lines.push("Control / privilege matrix:".to_string());
        for row in rows {
            let operation = s(&row, "operation").unwrap_or_else(|| "unknown".to_string());
            let backend = s(&row, "current_backend").unwrap_or_else(|| "none".to_string());
            let access = s(&row, "access").unwrap_or_else(|| "unknown".to_string());
            let supported = b(&row, "supported").unwrap_or(false);
            let decision = s(&row, "implementation_decision").unwrap_or_default();
            lines.push(format!(
                "  {operation}: supported={supported}, backend={backend}, access={access}"
            ));
            if !decision.is_empty() {
                lines.push(format!("    decision: {decision}"));
            }
        }
    }

    if let Some(endpoints) = vec_string(&map, "endpoints") {
        lines.push("".to_string());
        lines.push("Endpoints:".to_string());
        for e in endpoints {
            lines.push(format!("  {e}"));
        }
    }

    if let Some(notes) = vec_string(&map, "notes") {
        lines.push("".to_string());
        lines.push("Notes:".to_string());
        for n in notes {
            lines.push(format!("  {n}"));
        }
    }

    lines.join("\n")
}

fn fan_diagnostics_text(telemetry: &TelemetrySnapshot) -> String {
    let mut lines = Vec::new();
    lines.push("Fan Diagnostics".to_string());
    lines.push("===============".to_string());

    let detected = telemetry.fan_rows.len();
    let reporting = telemetry
        .fan_rows
        .iter()
        .filter(|fan| fan.rpm.is_some())
        .count();
    lines.push(format!("fan_inputs_detected: {detected}"));
    lines.push(format!("fan_inputs_reporting_rpm: {reporting}"));

    if telemetry.fan_rows.is_empty() {
        lines.push("".to_string());
        lines.push("No hwmon fan inputs are currently exposed on this platform.".to_string());
        return lines.join("\n");
    }

    lines.push("".to_string());
    lines.push("Detected fan mappings".to_string());
    lines.push("---------------------".to_string());
    for fan in &telemetry.fan_rows {
        lines.push(format!(
            "{}: {}",
            fan.display_label,
            fan.rpm
                .map(|rpm| format!("{rpm} rpm"))
                .unwrap_or_else(|| "(unavailable)".to_string())
        ));
        lines.push(format!(
            "  hwmon_device: {}",
            text_or_unknown(&fan.hwmon_device)
        ));
        lines.push(format!(
            "  hwmon_path: {}",
            text_or_unknown(&fan.hwmon_path)
        ));
        lines.push(format!(
            "  input_path: {}",
            text_or_unknown(&fan.input_path)
        ));
        lines.push(format!(
            "  raw_label: {}",
            fan.raw_label.as_deref().unwrap_or("(none)")
        ));
    }

    lines.join("\n")
}

fn fan_state_diagnostics_text(state: &FanState) -> String {
    let mut lines = Vec::new();
    lines.push("Fan Control Diagnostics".to_string());
    lines.push("=======================".to_string());
    lines.push(format!("backend: {}", state.caps.fan_backend));
    lines.push(format!("fan_count: {}", state.caps.fan_count));
    lines.push(format!(
        "fan_mapping_confidence: {}",
        state.caps.fan_mapping_confidence.as_str()
    ));
    lines.push(format!("mode: {}", fan_mode_label_text(state.mode)));
    lines.push(format!("sync_enabled: {}", state.sync_enabled));
    lines.push(format!(
        "manual_percent: {}",
        state.caps.has_fan_manual_percent
    ));
    lines.push(format!(
        "manual_rpm_target: {}",
        state.caps.has_fan_manual_rpm_target
    ));
    lines.push(format!("curves: {}", state.caps.has_fan_curves));
    lines.push(format!(
        "fan_curve_readable: {}",
        state.caps.fan_curve_readable
    ));
    lines.push(format!(
        "fan_curve_writable: {}",
        state.caps.fan_curve_writable
    ));
    lines.push(format!("boost: {}", state.caps.has_fan_boost));
    if let Some(until) = state.active_boost_until_ms {
        let remaining = until.saturating_sub(now_ms()) / 1000;
        lines.push(format!("boost_remaining_seconds: {remaining}"));
    }
    if let Some(action) = &state.last_action {
        lines.push(format!("last_action: {action}"));
    }

    lines.push(String::new());
    lines.push("Fans".to_string());
    lines.push("----".to_string());
    if state.fans.is_empty() {
        lines.push("No fan sensors are exposed by the active backend.".to_string());
    }
    for fan in &state.fans {
        lines.push(format!(
            "{} [{}]: {}",
            fan.label,
            fan.id,
            fan.current_rpm
                .map(|rpm| format!("{rpm} rpm"))
                .unwrap_or_else(|| "RPM unavailable".to_string())
        ));
        lines.push(format!(
            "  percent: {}",
            fan.current_percent
                .map(|percent| format!("{percent}%"))
                .unwrap_or_else(|| "(not reported)".to_string())
        ));
        lines.push(format!(
            "  backend: {}; mapping: {}; controllable: {}; manual_percent: {}; rpm_target: {}; curve: {}; auto: {}",
            fan.backend,
            fan.mapping_confidence.as_str(),
            fan.controllable,
            fan.supports_manual_percent,
            fan.supports_manual_rpm_target,
            fan.supports_curve,
            fan.supports_auto
        ));
        lines.push(format!(
            "  rpm_readable: {}; pwm_endpoint_verified: {}; direct_write: {}; privileged_write: {}; authorization: {}; access: {}",
            fan.rpm_readable,
            fan.pwm_endpoint_verified,
            fan.direct_write,
            fan.privileged_write,
            fan.authorization,
            fan.access_state,
        ));
        if !fan.endpoints.is_empty() {
            lines.push(format!("  endpoints: {}", fan.endpoints.join(", ")));
        }
        for note in &fan.notes {
            lines.push(format!("  note: {note}"));
        }
        for warning in &fan.warnings {
            lines.push(format!("  warning: {warning}"));
        }
    }
    if !state.caps.endpoints.is_empty() {
        lines.push(String::new());
        lines.push("Candidate endpoints".to_string());
        lines.push("-------------------".to_string());
        for endpoint in &state.caps.endpoints {
            lines.push(format!("- {endpoint}"));
        }
    }
    if !state.caps.notes.is_empty() {
        lines.push(String::new());
        lines.push("Capability notes".to_string());
        lines.push("----------------".to_string());
        for note in &state.caps.notes {
            lines.push(format!("- {note}"));
        }
    }
    if !state.caps.warnings.is_empty() {
        lines.push(String::new());
        lines.push("Warnings".to_string());
        lines.push("--------".to_string());
        for warning in &state.caps.warnings {
            lines.push(format!("- {warning}"));
        }
    }
    lines.join("\n")
}

fn fan_mode_label_text(mode: FanControlMode) -> &'static str {
    match mode {
        FanControlMode::Auto => "Auto / BIOS Default",
        FanControlMode::ManualPercent => "Manual Percentage",
        FanControlMode::ManualRpmTarget => "Manual RPM Target",
        FanControlMode::Curve => "Custom Curve",
        FanControlMode::FullSpeedBoost => "Full Speed Boost",
        FanControlMode::Unsupported => "Unsupported",
        FanControlMode::ReadOnly => "Read-only",
    }
}

fn fan_banner_title(state: &FanState) -> String {
    if state.fans.is_empty() {
        "No fan telemetry exposed by the current backend".to_string()
    } else if !state.caps.has_individual_fan_control {
        "Fan RPM telemetry is available, but controls are read-only".to_string()
    } else if !state.caps.warnings.is_empty() {
        "Some fan endpoints are read-only or uncertain".to_string()
    } else {
        "Fan controls available for supported hardware".to_string()
    }
}

fn fan_sync_subtitle(state: &FanState) -> String {
    if state.caps.has_fan_sync_control {
        "One manual percentage or boost action applies to all controllable fans; read-only fans stay visible.".to_string()
    } else if state.caps.fan_count <= 1 {
        "Sync is useful only when more than one controllable fan is detected.".to_string()
    } else {
        "No multiple writable fan controls were confirmed by the daemon.".to_string()
    }
}

fn fan_manual_subtitle(state: &FanState) -> String {
    if state.caps.has_fan_manual_percent {
        "Set a percentage duty value. Current RPM is telemetry, not a promised target.".to_string()
    } else if state.caps.has_fan_reading {
        "RPM telemetry is available, but no writable PWM control was confirmed.".to_string()
    } else {
        "No fan RPM or writable PWM endpoint is exposed by the active backend.".to_string()
    }
}

fn fan_boost_subtitle(state: &FanState) -> String {
    if let Some(until) = state.active_boost_until_ms {
        let remaining = until.saturating_sub(now_ms()) / 1000;
        return format!("Boost active; about {remaining}s remaining before Auto restore.");
    }
    if state.caps.has_fan_boost {
        "Temporarily set controllable fans to 100%, then restore Auto/BIOS mode.".to_string()
    } else {
        "Boost needs writable manual percentage fan control.".to_string()
    }
}

fn fans_status_pill(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("fans-status-pill");
    label.set_xalign(0.5);
    label.set_wrap(false);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label
}

fn build_fans_gauge_card(gauge: &TempGauge) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("fans-gauge-card");
    root.add_css_class("fans-gauge-card-large");
    root.set_hexpand(true);
    root.append(gauge.widget());
    root
}

fn build_fan_visual_slot() -> FanVisualSlot {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
    root.add_css_class("fan-visual-card");
    root.add_css_class("fan-rotor-card");
    root.set_hexpand(true);

    let rotor = FanRotor::new(false);
    let label = gtk::Label::new(Some("Fan"));
    label.add_css_class("fan-rpm-label");
    label.set_xalign(0.5);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let rpm = gtk::Label::new(Some("-- RPM"));
    rpm.add_css_class("fan-rpm-large");
    rpm.set_xalign(0.5);
    let status = gtk::Label::new(Some("Telemetry"));
    status.add_css_class("fans-status-pill");
    status.set_xalign(0.5);

    root.append(rotor.widget());
    root.append(&rpm);
    root.append(&label);
    root.append(&status);

    FanVisualSlot {
        root,
        rotor,
        label,
        rpm,
        status,
    }
}

fn build_fan_card_slot() -> FanCardSlot {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.add_css_class("fan-card");
    root.set_hexpand(true);

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let rotor = FanRotor::new(true);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 5);
    text.set_hexpand(true);

    let title = gtk::Label::new(Some("Fan"));
    title.add_css_class("title-3");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let id = gtk::Label::new(Some("hwmon"));
    id.add_css_class("dim-label");
    id.add_css_class("monospace");
    id.set_xalign(0.0);
    id.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let rpm = gtk::Label::new(Some("-- RPM"));
    rpm.add_css_class("fan-rpm-large");
    rpm.set_xalign(0.0);
    let percent = gtk::Label::new(Some("Fan speed percentage not reported by backend"));
    percent.add_css_class("dim-label");
    percent.set_xalign(0.0);
    percent.set_wrap(true);
    let status = gtk::Label::new(Some("Telemetry only"));
    status.add_css_class("fans-status-pill");
    status.set_xalign(0.0);
    let backend = gtk::Label::new(Some("backend"));
    backend.add_css_class("dim-label");
    backend.set_xalign(0.0);
    backend.set_wrap(true);

    text.append(&title);
    text.append(&id);
    text.append(&rpm);
    text.append(&percent);
    text.append(&status);
    text.append(&backend);
    top.append(rotor.widget());
    top.append(&text);

    let details = gtk::Label::new(Some(""));
    details.add_css_class("dim-label");
    details.add_css_class("monospace");
    details.set_xalign(0.0);
    details.set_wrap(true);
    let details_expander = gtk::Expander::new(Some("Endpoint details"));
    details_expander.set_expanded(false);
    details_expander.set_child(Some(&details));

    let warning = gtk::Label::new(Some(""));
    warning.add_css_class("fan-warning-text");
    warning.set_xalign(0.0);
    warning.set_wrap(true);
    warning.set_visible(false);

    root.append(&top);
    root.append(&warning);
    root.append(&details_expander);
    root.set_visible(false);

    FanCardSlot {
        root,
        rotor,
        title,
        id,
        rpm,
        percent,
        status,
        backend,
        details,
        warning,
    }
}

fn update_fan_visual_slots(slots: &[FanVisualSlot], fans: &[FanInfo]) {
    for (idx, slot) in slots.iter().enumerate() {
        if let Some(fan) = fans.get(idx) {
            slot.root.set_visible(true);
            slot.label.set_text(&fan.label);
            slot.rpm.set_text(&fan_rpm_text(fan.current_rpm));
            slot.status.set_text(fan_status_text(fan));
            update_status_pill_class(&slot.status, fan);
            slot.rotor.set_state(fan.current_rpm, rotor_status(fan));
            slot.root.set_tooltip_text(Some(&fan_card_tooltip(fan)));
        } else {
            slot.root.set_visible(false);
            slot.rotor.set_state(None, RotorStatus::ReadOnly);
        }
    }
}

fn update_fan_card_slots(slots: &[FanCardSlot], fans: &[FanInfo]) {
    for (idx, slot) in slots.iter().enumerate() {
        if let Some(fan) = fans.get(idx) {
            slot.root.set_visible(true);
            slot.title.set_text(&fan.label);
            slot.id.set_text(&fan.id);
            slot.rpm.set_text(&fan_rpm_text(fan.current_rpm));
            slot.percent.set_text(
                &fan.current_percent
                    .map(|percent| format!("Backend-reported speed: {percent}%"))
                    .unwrap_or_else(|| "Fan speed percentage not reported by backend".to_string()),
            );
            slot.status.set_text(fan_status_text(fan));
            update_status_pill_class(&slot.status, fan);
            slot.backend.set_text(&format!(
                "{} | {}",
                fan.backend,
                if fan.controllable {
                    "control endpoint writable"
                } else {
                    "control endpoint not writable"
                }
            ));
            slot.details.set_text(&fan_endpoint_details(fan));
            let warning_text = fan_warning_text(fan);
            slot.warning.set_text(&warning_text);
            slot.warning.set_visible(!warning_text.is_empty());
            slot.rotor.set_state(fan.current_rpm, rotor_status(fan));
            slot.root.set_tooltip_text(Some(&fan_card_tooltip(fan)));
            slot.root.remove_css_class("fan-card-read-only");
            slot.root.remove_css_class("fan-card-controllable");
            if fan.controllable {
                slot.root.add_css_class("fan-card-controllable");
            } else {
                slot.root.add_css_class("fan-card-read-only");
            }
        } else {
            slot.root.set_visible(false);
            slot.rotor.set_state(None, RotorStatus::ReadOnly);
        }
    }
}

fn fan_rpm_text(rpm: Option<u32>) -> String {
    rpm.map(|rpm| format!("{rpm} RPM"))
        .unwrap_or_else(|| "-- RPM".to_string())
}

fn fan_status_text(fan: &FanInfo) -> &'static str {
    if fan_mapping_uncertain(fan) {
        "Mapping uncertain"
    } else if fan.controllable {
        "Controllable"
    } else if fan.current_rpm.is_some() {
        "Telemetry only"
    } else {
        "Unavailable"
    }
}

fn rotor_status(fan: &FanInfo) -> RotorStatus {
    if fan.current_rpm.is_none() {
        RotorStatus::Error
    } else if fan_mapping_uncertain(fan) {
        RotorStatus::Warning
    } else if fan.controllable {
        RotorStatus::Controllable
    } else if fan.backend.contains("read-only") {
        RotorStatus::ReadOnly
    } else {
        RotorStatus::Telemetry
    }
}

fn fan_mapping_uncertain(fan: &FanInfo) -> bool {
    fan.mapping_confidence != FanMappingConfidence::HardwareLabel
}

fn fan_warning_text(fan: &FanInfo) -> String {
    fan.warnings
        .iter()
        .chain(fan.notes.iter())
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

fn fan_endpoint_details(fan: &FanInfo) -> String {
    let mut lines = Vec::new();
    lines.push(format!("id: {}", fan.id));
    lines.push(format!("backend: {}", fan.backend));
    lines.push(format!(
        "manual_percent: {}; rpm_target: {}; curve: {}; auto: {}",
        fan.supports_manual_percent,
        fan.supports_manual_rpm_target,
        fan.supports_curve,
        fan.supports_auto
    ));
    lines.push(format!(
        "percent: {}",
        fan.current_percent
            .map(|percent| format!("{percent}%"))
            .unwrap_or_else(|| "not reported".to_string())
    ));
    if !fan.endpoints.is_empty() {
        lines.push("endpoints:".to_string());
        lines.extend(fan.endpoints.iter().map(|endpoint| format!("  {endpoint}")));
    }
    lines.join("\n")
}

fn fan_card_tooltip(fan: &FanInfo) -> String {
    if fan.controllable {
        format!("{} is controllable through {}.", fan.label, fan.backend)
    } else {
        format!(
            "{} is read-only. Fan RPM telemetry is available, but no writable fan-control endpoint was confirmed.",
            fan.label
        )
    }
}

fn update_status_pill_class(label: &gtk::Label, fan: &FanInfo) {
    label.remove_css_class("fans-status-pill-read-only");
    label.remove_css_class("fans-status-pill-controllable");
    label.remove_css_class("fans-status-pill-warning");
    label.remove_css_class("fans-status-pill-error");
    if fan.current_rpm.is_none() {
        label.add_css_class("fans-status-pill-error");
    } else if fan_mapping_uncertain(fan) {
        label.add_css_class("fans-status-pill-warning");
    } else if fan.controllable {
        label.add_css_class("fans-status-pill-controllable");
    } else {
        label.add_css_class("fans-status-pill-read-only");
    }
}

fn update_backend_pill_class(label: &gtk::Label, backend: &str) {
    label.remove_css_class("fans-status-pill-read-only");
    label.remove_css_class("fans-status-pill-controllable");
    label.remove_css_class("fans-status-pill-error");
    let backend = backend.to_ascii_lowercase();
    if backend.contains("read") {
        label.add_css_class("fans-status-pill-read-only");
    } else if backend.contains("unsupported") {
        label.add_css_class("fans-status-pill-error");
    } else {
        label.add_css_class("fans-status-pill-controllable");
    }
}

fn fan_controls_hint(state: &FanState) -> String {
    if state
        .fans
        .iter()
        .any(|fan| fan.access_state == "authorization_required")
    {
        "Administrator access required for fan control. Authentication starts only when Apply is used. Auto/BIOS restore remains available.".to_string()
    } else if state
        .fans
        .iter()
        .any(|fan| fan.access_state == "authorization_denied")
    {
        "Administrator authorization was denied. RPM telemetry remains available.".to_string()
    } else if state.caps.has_fan_manual_percent
        || state.caps.has_fan_boost
        || state.caps.fan_curve_writable
    {
        "Writable fan-control support was detected. Use these controls carefully; Auto/BIOS restore remains available.".to_string()
    } else if state.caps.has_fan_reading {
        "Fan RPM telemetry is available, but no writable fan-control endpoint was confirmed. Controls will be enabled automatically when a supported backend is detected.".to_string()
    } else {
        "No fan telemetry or writable fan-control backend is exposed by the current system."
            .to_string()
    }
}

fn lighting_diagnostics_text(lighting: &LightingInfo) -> String {
    if let Some(details) = lighting.diagnostics_details.as_deref() {
        if !details.trim().is_empty() {
            return details.to_string();
        }
    }

    let mut lines = Vec::new();
    lines.push("Lighting Diagnostics".to_string());
    lines.push("====================".to_string());
    lines.push(format!("backend: {}", lighting.backend));
    lines.push(format!("backend_kind: {}", lighting.backend_kind));
    lines.push(format!("device: {}", lighting.device));
    lines.push(format!(
        "brightness: {}/{}",
        lighting.brightness, lighting.max_brightness
    ));
    lines.push(format!("mode: {}", lighting.mode));
    lines.push(format!(
        "supported_modes: {}",
        if lighting.supported_modes.is_empty() {
            "(not reported)".to_string()
        } else {
            lighting.supported_modes.join(", ")
        }
    ));
    lines.push(format!(
        "supports_brightness: {}",
        lighting.supports_brightness
    ));
    lines.push(format!("supports_modes: {}", lighting.supports_modes));
    lines.push(format!("supports_rgb: {}", lighting.supports_rgb));
    lines.push(format!("supports_argb: {}", lighting.supports_argb));
    lines.push(format!("supports_zones: {}", lighting.supports_zones));
    lines.push(format!("supports_per_key: {}", lighting.supports_per_key));
    lines.push(format!("supports_speed: {}", lighting.supports_speed));
    lines.push(format!(
        "supported_speeds: {}",
        if lighting.supported_speeds.is_empty() {
            "(not reported)".to_string()
        } else {
            lighting.supported_speeds.join(", ")
        }
    ));
    lines.push(format!(
        "speed: {}",
        lighting.speed.as_deref().unwrap_or("(not reported)")
    ));
    lines.push(format!(
        "supported_directions: {}",
        if lighting.supported_directions.is_empty() {
            "(not reported)".to_string()
        } else {
            lighting.supported_directions.join(", ")
        }
    ));
    lines.push(format!(
        "direction: {}",
        lighting.direction.as_deref().unwrap_or("(not reported)")
    ));
    lines.push(format!(
        "supported_zones: {}",
        if lighting.supported_zones.is_empty() {
            "(not reported)".to_string()
        } else {
            lighting.supported_zones.join(", ")
        }
    ));
    lines.push(format!(
        "rgb_hex: {}",
        lighting.rgb_hex.as_deref().unwrap_or("(not reported)")
    ));
    lines.push(format!(
        "secondary_rgb_hex: {}",
        lighting
            .secondary_rgb_hex
            .as_deref()
            .unwrap_or("(not reported)")
    ));
    lines.push(format!(
        "active_zone: {}",
        lighting.active_zone.as_deref().unwrap_or("(not reported)")
    ));
    lines.push(format!(
        "apply_outcome: {}",
        lighting.apply_outcome.as_deref().unwrap_or("(none)")
    ));
    lines.push(format!("writable: {}", lighting.can_set));
    lines.push(format!("direct_writable: {}", lighting.direct_writable));
    lines.push(format!(
        "privileged_writable: {}",
        lighting.privileged_writable
    ));
    lines.push(format!(
        "authorization_required: {}",
        lighting.authorization_required
    ));
    lines.push(format!("authorization: {}", lighting.authorization));
    lines.push(format!(
        "fallback_reason: {}",
        lighting.fallback_reason.as_deref().unwrap_or("(none)")
    ));
    lines.push(format!("status: {}", lighting.status));
    if let Some(error) = &lighting.last_error {
        lines.push(format!("last_error: {error}"));
    }
    lines.join("\n")
}

fn text_or_unknown(value: &str) -> &str {
    if value.is_empty() {
        "(not provided by daemon)"
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ov<T>(value: T) -> OwnedValue
    where
        T: Into<Value<'static>>,
    {
        OwnedValue::try_from(value.into()).expect("OwnedValue conversion should succeed")
    }

    #[test]
    fn privileged_status_decoder_preserves_access_infrastructure_state() {
        let mut map = HashMap::new();
        map.insert(
            dbus_keys::privileged_status::SYSTEM_BUS_CONNECTED.to_string(),
            OwnedValue::from(true),
        );
        map.insert(
            dbus_keys::privileged_status::HELPER_INSTALLED.to_string(),
            OwnedValue::from(true),
        );
        map.insert(
            dbus_keys::privileged_status::HELPER_REACHABLE.to_string(),
            OwnedValue::from(true),
        );
        map.insert(
            dbus_keys::privileged_status::HELPER_COMPATIBLE.to_string(),
            OwnedValue::from(true),
        );
        map.insert(
            dbus_keys::privileged_status::POLKIT_AVAILABLE.to_string(),
            OwnedValue::from(true),
        );
        map.insert(
            dbus_keys::privileged_status::AUTHORIZATION_STATE.to_string(),
            ov("denied".to_string()),
        );
        map.insert(
            dbus_keys::privileged_status::CATEGORIES_AVAILABLE.to_string(),
            ov(vec!["cpu".to_string(), "fans".to_string()]),
        );

        let status = privileged_status_from_dbus(&map);
        assert!(status.system_bus_connected);
        assert!(status.privileged_helper_installed);
        assert!(status.privileged_helper_reachable);
        assert!(status.privileged_helper_compatible);
        assert!(status.polkit_available);
        assert_eq!(status.authorization_state, AuthorizationState::Denied);
        assert_eq!(
            status.privileged_categories_available,
            [PrivilegedCategory::Cpu, PrivilegedCategory::Fans]
        );
    }

    #[test]
    fn reusable_access_states_distinguish_backend_permission_and_hardware() {
        assert_eq!(
            feature_access_ui_state(&FeatureAvailability::new(
                FeatureAccessState::MissingBackend,
                "supergfxd is required",
            )),
            AccessUiState::BackendMissing
        );
        assert_eq!(
            feature_access_ui_state(&FeatureAvailability::new(
                FeatureAccessState::PermissionDenied,
                "read only",
            )),
            AccessUiState::ReadOnly
        );
        assert_eq!(
            feature_access_ui_state(&FeatureAvailability::new(
                FeatureAccessState::Unsupported,
                "unsupported",
            )),
            AccessUiState::UnsupportedHardware
        );
    }

    #[test]
    fn cpu_unlock_label_is_limited_to_controls_that_need_authorization() {
        let mut caps = CpuCaps::unknown();
        caps.control_access.push(CpuControlAccess {
            kind: CpuControlKind::Governor,
            status: CpuAccessState::AuthorizationRequired,
            reason: "Administrator access required".to_string(),
            direct_write: false,
            privileged_write: true,
            authorization: CpuAuthorization::Required,
            paths: Vec::new(),
        });

        assert!(cpu_controls_require_authorization(
            &caps,
            &[CpuControlKind::Governor]
        ));
        assert!(!cpu_controls_require_authorization(
            &caps,
            &[CpuControlKind::Boost]
        ));
    }

    #[test]
    fn privilege_failures_have_distinct_user_messages() {
        assert!(friendly_action_error("Authentication was cancelled")
            .starts_with("Authentication was cancelled."));
        assert!(
            friendly_action_error("keyboard lighting: Administrator authorization was denied")
                .starts_with("Administrator authorization was denied.")
        );
        assert!(
            friendly_action_error("the privileged fan helper is unavailable")
                .contains("not installed or is unavailable")
        );
        assert!(friendly_action_error("GPU mode control requires supergfxd")
            .starts_with("GPU mode switching requires supergfxd."));
        assert!(
            friendly_action_error("battery limit control requires asusd")
                .starts_with("Charge-limit control requires asusd.")
        );
    }

    #[test]
    fn caps_from_dbus_parses_feature_access_status_and_reason_keys() {
        let mut map = HashMap::new();
        map.insert(
            dbus_keys::feature_access_status_key(dbus_keys::PROFILE_ACCESS_PREFIX),
            ov("available".to_string()),
        );
        map.insert(
            dbus_keys::feature_access_reason_key(dbus_keys::PROFILE_ACCESS_PREFIX),
            ov("Performance profiles are available through asusd.".to_string()),
        );
        map.insert(
            dbus_keys::feature_access_status_key(dbus_keys::CHARGE_LIMIT_ACCESS_PREFIX),
            ov("missing_backend".to_string()),
        );
        map.insert(
            dbus_keys::feature_access_reason_key(dbus_keys::CHARGE_LIMIT_ACCESS_PREFIX),
            ov("Install and start asusd to enable charge-limit control.".to_string()),
        );
        map.insert(
            dbus_keys::feature_access_status_key(dbus_keys::GPU_MODE_ACCESS_PREFIX),
            ov("temporarily_unavailable".to_string()),
        );
        map.insert(
            dbus_keys::feature_access_reason_key(dbus_keys::GPU_MODE_ACCESS_PREFIX),
            ov("GPU mode switching needs supergfxd, but the system backend could not be reached right now.".to_string()),
        );
        map.insert(
            dbus_keys::caps::GPU_BACKEND.to_string(),
            ov("supergfxd".to_string()),
        );
        map.insert(
            dbus_keys::caps::GPU_SUPPORTED_MODES.to_string(),
            ov(vec!["Hybrid".to_string(), "Vfio".to_string()]),
        );
        map.insert(
            dbus_keys::caps::GPU_EXTERNAL_AUTHORIZATION.to_string(),
            ov("external_service".to_string()),
        );
        map.insert(
            dbus_keys::caps::GPU_SWITCH_STATE.to_string(),
            ov("reboot_required".to_string()),
        );
        map.insert(
            dbus_keys::caps::GPU_SWITCH_HINT.to_string(),
            ov("Reboot required to complete the pending GPU switch.".to_string()),
        );
        map.insert(
            dbus_keys::caps::REQUIRES_REBOOT_FOR_GPU_SWITCH.to_string(),
            OwnedValue::from(true),
        );
        map.insert(
            dbus_keys::feature_access_status_key(dbus_keys::KBD_BACKLIGHT_ACCESS_PREFIX),
            ov("permission_denied".to_string()),
        );
        map.insert(
            dbus_keys::feature_access_reason_key(dbus_keys::KBD_BACKLIGHT_ACCESS_PREFIX),
            ov(
                "Keyboard backlight is detected, but writes are blocked for the current user."
                    .to_string(),
            ),
        );

        let caps = caps_from_dbus(&map);

        assert_eq!(caps.profile_access.status, FeatureAccessState::Available);
        assert_eq!(
            caps.charge_limit_access.status,
            FeatureAccessState::MissingBackend
        );
        assert_eq!(
            caps.gpu_mode_access.status,
            FeatureAccessState::TemporarilyUnavailable
        );
        assert_eq!(
            caps.kbd_backlight_access.status,
            FeatureAccessState::PermissionDenied
        );
        assert_eq!(
            caps.gpu_mode_access.reason,
            "GPU mode switching needs supergfxd, but the system backend could not be reached right now."
        );
        assert_eq!(caps.gpu_backend, "supergfxd");
        assert_eq!(caps.gpu_supported_modes, vec!["Hybrid", "Vfio"]);
        assert_eq!(caps.gpu_external_authorization, "external_service");
        assert_eq!(caps.gpu_switch_state, GpuSwitchState::RebootRequired);
        assert!(caps.requires_reboot_for_gpu_switch);
    }

    #[test]
    fn gpu_mode_selection_uses_only_backend_reported_modes() {
        let modes = vec![
            "Integrated".to_string(),
            "Hybrid".to_string(),
            "AsusMuxDgpu".to_string(),
        ];
        assert_eq!(gpu_mode_index_in_supported("Hybrid", &modes), Some(1));
        assert_eq!(gpu_mode_index_in_supported("Dedicated", &modes), Some(2));
        assert_eq!(gpu_mode_index_in_supported("Vfio", &modes), None);
    }

    #[test]
    fn lighting_from_dbus_parses_explicit_backend_capabilities() {
        let mut map = HashMap::new();
        map.insert("backend".to_string(), ov("verified-aura".to_string()));
        map.insert(
            "backend_kind".to_string(),
            ov("native-aura-hid".to_string()),
        );
        map.insert("device".to_string(), ov("test-endpoint".to_string()));
        map.insert("can_set".to_string(), OwnedValue::from(true));
        map.insert("direct_writable".to_string(), OwnedValue::from(false));
        map.insert("privileged_writable".to_string(), OwnedValue::from(true));
        map.insert("authorization_required".to_string(), OwnedValue::from(true));
        map.insert("authorization".to_string(), ov("not_checked".to_string()));
        map.insert("supports_brightness".to_string(), OwnedValue::from(false));
        map.insert("supports_modes".to_string(), OwnedValue::from(true));
        map.insert("supports_rgb".to_string(), OwnedValue::from(true));
        map.insert("supports_argb".to_string(), OwnedValue::from(false));
        map.insert("supports_zones".to_string(), OwnedValue::from(true));
        map.insert("supports_per_key".to_string(), OwnedValue::from(false));
        map.insert("supports_speed".to_string(), OwnedValue::from(true));
        map.insert("secondary_rgb_hex".to_string(), ov("#001122".to_string()));
        map.insert(
            "supported_modes".to_string(),
            ov(vec!["Static".to_string(), "Breathe".to_string()]),
        );
        map.insert(
            "supported_speeds".to_string(),
            ov(vec!["Medium".to_string()]),
        );
        map.insert("speed".to_string(), ov("Medium".to_string()));
        map.insert(
            "supported_directions".to_string(),
            ov(vec!["Left".to_string(), "Right".to_string()]),
        );
        map.insert("direction".to_string(), ov("Left".to_string()));
        map.insert(
            "supported_zones".to_string(),
            ov(vec!["Keyboard".to_string()]),
        );
        map.insert("active_zone".to_string(), ov("Keyboard".to_string()));
        map.insert(
            "apply_outcome".to_string(),
            ov("accepted_no_readback".to_string()),
        );

        let lighting = lighting_from_dbus(map).expect("lighting state should parse");
        assert!(!lighting.supports_brightness);
        assert!(lighting.supports_modes);
        assert!(lighting.supports_rgb);
        assert!(!lighting.supports_argb);
        assert!(lighting.supports_zones);
        assert!(!lighting.supports_per_key);
        assert!(lighting.supports_speed);
        assert_eq!(lighting.backend_kind, "native-aura-hid");
        assert!(!lighting.direct_writable);
        assert!(lighting.privileged_writable);
        assert!(lighting.authorization_required);
        assert_eq!(lighting.authorization, "not_checked");
        assert_eq!(lighting.supported_modes, ["Static", "Breathe"]);
        assert_eq!(lighting.supported_speeds, ["Medium"]);
        assert_eq!(lighting.speed.as_deref(), Some("Medium"));
        assert_eq!(lighting.supported_directions, ["Left", "Right"]);
        assert_eq!(lighting.direction.as_deref(), Some("Left"));
        assert_eq!(lighting.supported_zones, ["Keyboard"]);
        assert_eq!(lighting.active_zone.as_deref(), Some("Keyboard"));
        assert_eq!(lighting.secondary_rgb_hex.as_deref(), Some("#001122"));
        assert_eq!(
            lighting.apply_outcome.as_deref(),
            Some("accepted_no_readback")
        );
    }

    #[test]
    fn lighting_request_encoder_keeps_effect_fields_optional() {
        let brightness_only = lighting_request_to_dbus(PendingLighting {
            brightness: Some(2),
            mode: None,
            rgb_hex: None,
            secondary_rgb_hex: None,
            speed: None,
            direction: None,
            zone: None,
        });
        assert_eq!(brightness_only.len(), 1);
        assert_eq!(
            dbus_decode::unsigned(&brightness_only, dbus_keys::lighting::BRIGHTNESS),
            Some(2)
        );

        let effect = lighting_request_to_dbus(PendingLighting {
            brightness: None,
            mode: Some("Breathe".to_string()),
            rgb_hex: Some("#FF0000".to_string()),
            secondary_rgb_hex: Some("#0000FF".to_string()),
            speed: Some("Fast".to_string()),
            direction: Some("Left".to_string()),
            zone: Some("Keyboard".to_string()),
        });
        assert_eq!(
            dbus_decode::string(&effect, dbus_keys::lighting::SECONDARY_RGB_HEX).as_deref(),
            Some("#0000FF")
        );
        assert_eq!(
            dbus_decode::string(&effect, dbus_keys::lighting::SPEED).as_deref(),
            Some("Fast")
        );
        assert_eq!(
            dbus_decode::string(&effect, dbus_keys::lighting::DIRECTION).as_deref(),
            Some("Left")
        );
        assert_eq!(
            dbus_decode::string(&effect, "zone").as_deref(),
            Some("Keyboard")
        );
    }

    #[test]
    fn effect_specific_controls_are_conservative() {
        assert!(lighting_mode_supports_primary_colour("Static"));
        assert!(!lighting_mode_supports_primary_colour("Rainbow Cycle"));
        assert!(lighting_mode_supports_secondary_colour("Breathe"));
        assert!(!lighting_mode_supports_secondary_colour("Static"));
        assert!(lighting_mode_supports_speed("Rainbow Cycle"));
        assert!(!lighting_mode_supports_speed("Static"));
        assert!(lighting_mode_supports_direction("Rainbow Wave"));
        assert!(!lighting_mode_supports_direction("Rainbow Cycle"));
    }

    #[test]
    fn missing_capability_fields_remain_disabled() {
        let caps = caps_from_dbus(&HashMap::new());
        let lighting = lighting_from_dbus(HashMap::new()).expect("map should decode");

        assert!(!caps.has_profiles);
        assert!(!caps.has_aura);
        assert_eq!(caps.profile_access.status, FeatureAccessState::Unknown);
        assert!(!lighting.can_set);
        assert!(!lighting.supports_brightness);
        assert!(!lighting.supports_rgb);
        assert_eq!(lighting.max_brightness, 0);
    }

    #[test]
    fn telemetry_from_dbus_parses_structured_fan_rows() {
        let mut fan_row = HashMap::new();
        fan_row.insert(
            dbus_keys::FAN_ROW_HWMON_DEVICE_KEY.to_string(),
            ov("hwmon4".to_string()),
        );
        fan_row.insert(
            dbus_keys::FAN_ROW_HWMON_PATH_KEY.to_string(),
            ov("/sys/class/hwmon/hwmon4".to_string()),
        );
        fan_row.insert(
            dbus_keys::FAN_ROW_INPUT_PATH_KEY.to_string(),
            ov("/sys/class/hwmon/hwmon4/fan2_input".to_string()),
        );
        fan_row.insert(
            dbus_keys::FAN_ROW_RAW_LABEL_KEY.to_string(),
            ov("gpu".to_string()),
        );
        fan_row.insert(
            dbus_keys::FAN_ROW_DISPLAY_LABEL_KEY.to_string(),
            ov("GPU Fan".to_string()),
        );
        fan_row.insert(
            dbus_keys::FAN_ROW_RPM_KEY.to_string(),
            OwnedValue::from(2875_u32),
        );

        let mut map = HashMap::new();
        map.insert("timestamp_ms".to_string(), OwnedValue::from(10_u64));
        map.insert("gpu_core_clock_mhz".to_string(), OwnedValue::from(2100_u64));
        map.insert("gpu_usage_percent".to_string(), OwnedValue::from(32.0_f64));
        map.insert(
            "gpu_vram_total_bytes".to_string(),
            OwnedValue::from(8_589_934_592_u64),
        );
        map.insert("gpu_name".to_string(), ov("NVIDIA Test GPU".to_string()));
        map.insert(
            "gpu_memory_clock_mhz".to_string(),
            OwnedValue::from(7000_u64),
        );
        map.insert(
            dbus_keys::TELEMETRY_FAN_ROWS_KEY.to_string(),
            ov(vec![fan_row]),
        );

        let telemetry = telemetry_from_dbus(map);
        let fan = telemetry.fan_rows.first().expect("fan row should parse");

        assert_eq!(fan.hwmon_device, "hwmon4");
        assert_eq!(fan.hwmon_path, "/sys/class/hwmon/hwmon4");
        assert_eq!(fan.input_path, "/sys/class/hwmon/hwmon4/fan2_input");
        assert_eq!(fan.raw_label.as_deref(), Some("gpu"));
        assert_eq!(fan.display_label, "GPU Fan");
        assert_eq!(fan.rpm, Some(2875));
        assert_eq!(telemetry.gpu_core_clock_mhz, Some(2100));
        assert_eq!(telemetry.gpu_memory_clock_mhz, Some(7000));
        assert_eq!(telemetry.gpu_usage_percent, Some(32.0));
        assert_eq!(telemetry.gpu_vram_total_bytes, Some(8_589_934_592));
        assert_eq!(telemetry.gpu_name.as_deref(), Some("NVIDIA Test GPU"));
    }

    #[test]
    fn telemetry_from_dbus_preserves_missing_optional_fields() {
        let telemetry = telemetry_from_dbus(HashMap::new());

        assert_eq!(telemetry.gpu_temp_c, None);
        assert_eq!(telemetry.gpu_usage_percent, None);
        assert_eq!(telemetry.gpu_vram_total_bytes, None);
        assert_eq!(telemetry.power_source, None);
        assert!(telemetry.fan_rows.is_empty());
    }

    #[test]
    fn telemetry_from_dbus_does_not_fabricate_unknown_power_source() {
        let mut map = HashMap::new();
        map.insert(
            dbus_keys::telemetry::POWER_SOURCE.to_string(),
            ov("future-wire-value".to_string()),
        );

        assert_eq!(telemetry_from_dbus(map).power_source, None);
    }

    #[test]
    fn cpu_caps_from_dbus_parses_structured_control_access_rows() {
        let mut path_row = HashMap::new();
        path_row.insert(
            dbus_keys::CPU_PATH_PATH_KEY.to_string(),
            ov("/sys/devices/system/cpu/cpufreq/policy0/energy_performance_preference".to_string()),
        );
        path_row.insert(
            dbus_keys::CPU_PATH_READABLE_KEY.to_string(),
            OwnedValue::from(true),
        );
        path_row.insert(
            dbus_keys::CPU_PATH_WRITABLE_KEY.to_string(),
            OwnedValue::from(false),
        );

        let mut control_row = HashMap::new();
        control_row.insert(
            dbus_keys::CPU_CONTROL_KIND_KEY.to_string(),
            ov(rog_core::CpuControlKind::Epp.as_str().to_string()),
        );
        control_row.insert(
            dbus_keys::CPU_CONTROL_STATUS_KEY.to_string(),
            ov(CpuAccessState::PermissionDenied.as_str().to_string()),
        );
        control_row.insert(
            dbus_keys::CPU_CONTROL_REASON_KEY.to_string(),
            ov("Energy Performance Preference paths are readable but not writable by the current user.".to_string()),
        );
        control_row.insert(
            dbus_keys::CPU_CONTROL_PATHS_KEY.to_string(),
            ov(vec![path_row]),
        );

        let mut map = HashMap::new();
        map.insert(
            dbus_keys::CPU_CONTROL_ACCESS_KEY.to_string(),
            ov(vec![control_row]),
        );

        let caps = cpu_caps_from_dbus(&map);
        let control = caps
            .control_access
            .first()
            .expect("control access row should parse");

        assert_eq!(control.kind, rog_core::CpuControlKind::Epp);
        assert_eq!(control.status, CpuAccessState::PermissionDenied);
        assert_eq!(
            control.reason,
            "Energy Performance Preference paths are readable but not writable by the current user."
        );
        assert_eq!(control.paths.len(), 1);
        assert_eq!(
            control.paths[0].path,
            "/sys/devices/system/cpu/cpufreq/policy0/energy_performance_preference"
        );
        assert!(control.paths[0].readable);
        assert!(!control.paths[0].writable);
    }

    #[test]
    fn cpu_telemetry_from_dbus_supports_logical_cpu_id_and_core_id_alias() {
        let mut logical_row = HashMap::new();
        logical_row.insert(
            dbus_keys::CPU_LOGICAL_CPU_ID_KEY.to_string(),
            OwnedValue::from(3_u64),
        );
        logical_row.insert("online".to_string(), OwnedValue::from(true));

        let mut compat_row = HashMap::new();
        compat_row.insert(
            dbus_keys::CPU_CORE_ID_COMPAT_KEY.to_string(),
            OwnedValue::from(7_u64),
        );
        compat_row.insert("online".to_string(), OwnedValue::from(false));

        let mut map = HashMap::new();
        map.insert("timestamp_ms".to_string(), OwnedValue::from(99_u64));
        map.insert(
            dbus_keys::CPU_TELEMETRY_PER_CORE_KEY.to_string(),
            ov(vec![logical_row, compat_row]),
        );

        let cpu = cpu_telemetry_from_dbus(map);

        assert_eq!(cpu.per_core.len(), 2);
        assert_eq!(cpu.per_core[0].logical_cpu_id, 3);
        assert!(cpu.per_core[0].online);
        assert_eq!(cpu.per_core[1].logical_cpu_id, 7);
        assert!(!cpu.per_core[1].online);
    }

    #[test]
    fn parse_version_for_compare_handles_v_prefix_and_missing_patch() {
        let parsed = parse_version_for_compare("v0.16").expect("version should parse");
        assert_eq!(parsed.major, 0);
        assert_eq!(parsed.minor, 16);
        assert_eq!(parsed.patch, 0);

        let comparison = compare_versions("0.16.0", "v0.16");
        assert_eq!(comparison, Some(std::cmp::Ordering::Equal));
    }

    #[test]
    fn github_repo_from_url_supports_https_and_ssh_forms() {
        assert_eq!(
            github_repo_from_url("https://github.com/UdayaSri0/g-helper-linux"),
            Some(("UdayaSri0".to_string(), "g-helper-linux".to_string()))
        );
        assert_eq!(
            github_repo_from_url("git@github.com:UdayaSri0/g-helper-linux.git"),
            Some(("UdayaSri0".to_string(), "g-helper-linux".to_string()))
        );
    }

    #[test]
    fn summary_from_state_includes_tray_hover_fields() {
        let mut telemetry = TelemetrySnapshot::empty_now(0);
        telemetry.power_source = Some(PowerSource::Ac);
        telemetry.battery_percent = Some(87.0);
        telemetry.cpu_temp_c = Some(61.3);
        telemetry.gpu_temp_c = Some(50.2);

        assert_eq!(
            summary_from_state(&telemetry, Some("Turbo"), Some("Hybrid")),
            "AC | 87% | CPU 61C | GPU 50C | Profile Turbo | Mode Hybrid"
        );
    }

    #[test]
    fn dashboard_summaries_only_include_reported_telemetry() {
        let mut cpu = CpuTelemetry::empty_now(0);
        cpu.usage_percent = Some(18.0);
        assert_eq!(cpu_dashboard_summary(&cpu), "18% utilisation");
        cpu.avg_freq_mhz = Some(1170.0);
        assert_eq!(cpu_dashboard_summary(&cpu), "18% utilisation · 1.17 GHz");

        let mut telemetry = TelemetrySnapshot::empty_now(0);
        telemetry.gpu_temp_c = Some(46.0);
        telemetry.gpu_core_clock_mhz = Some(2100);
        assert_eq!(
            gpu_dashboard_summary(&telemetry, None),
            "Telemetry available"
        );
        telemetry.gpu_usage_percent = Some(32.0);
        telemetry.gpu_vram_used_bytes = Some(2 * 1024 * 1024 * 1024);
        telemetry.gpu_vram_total_bytes = Some(8 * 1024 * 1024 * 1024);
        assert_eq!(
            gpu_dashboard_summary(&telemetry, Some("Hybrid")),
            "32% load · 2.0 / 8.0 GiB VRAM · Mode Hybrid"
        );
    }

    #[test]
    fn dashboard_missing_backend_points_to_setup_and_access() {
        let access = FeatureAvailability::new(
            FeatureAccessState::MissingBackend,
            "Install and start asusd to enable this control.",
        );

        assert_eq!(
            dashboard_control_subtitle(&access, "Apply control"),
            "Unavailable · review Setup & Access"
        );
    }

    #[test]
    fn dashboard_breakpoints_avoid_four_plus_one_packing() {
        assert_eq!(dashboard_secondary_columns(1260), 5);
        assert_eq!(dashboard_secondary_columns(1160), 5);
        assert_eq!(dashboard_secondary_columns(1159), 3);
        assert_eq!(dashboard_secondary_columns(641), 3);
        assert_eq!(dashboard_secondary_columns(640), 3);
        assert_eq!(dashboard_secondary_columns(639), 1);
        assert_eq!(dashboard_secondary_columns(780), 3);

        assert_eq!(dashboard_mode_columns(1161), 5);
        assert_eq!(dashboard_mode_columns(1160), 5);
        assert_eq!(dashboard_mode_columns(1159), 3);
        assert_eq!(dashboard_mode_columns(641), 3);
        assert_eq!(dashboard_mode_columns(640), 3);
        assert_eq!(dashboard_mode_columns(639), 1);
    }

    #[test]
    fn autostart_entry_supports_minimized_launch() {
        let settings = rog_core::UiPreferences {
            launch_on_login: true,
            start_minimized_to_tray: true,
            ..rog_core::UiPreferences::default()
        };

        let entry = autostart_desktop_entry(&settings);

        assert!(entry.contains("Type=Application"));
        assert!(entry.contains(&format!("Exec={APP_BINARY_NAME} {START_MINIMIZED_ARG}")));
    }
}
