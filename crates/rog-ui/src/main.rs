use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use ksni::menu::{MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{ToolTip, Tray, TrayMethods};
use rog_core::{
    BatteryState, CpuAccessState, CpuCaps, CpuControlAccess, CpuControlKind, CpuPathAccess,
    CpuTelemetry, DeviceCaps, FanTelemetry, FeatureAccessState, FeatureAvailability, PowerSource,
    TelemetrySnapshot, TopProcessMem,
};
use tracing::{info, warn};
use zbus::zvariant::{OwnedValue, Value};

const DAEMON_DBUS_NAME: &str = "io.github.roghelper.Daemon";
const DAEMON_DBUS_PATH: &str = "/io/github/roghelper/Daemon";
const DAEMON_DBUS_IFACE: &str = "io.github.roghelper.Daemon1";
const APP_DISPLAY_NAME: &str = "rog-helper";
const APP_BINARY_NAME: &str = "rog-helper-ui";
const APP_DEVELOPER_FALLBACK: &str = "rog-helper contributors";
const APP_SOURCE_FALLBACK_URL: &str = "https://github.com/UdayaSri0/g-helper-linux";

#[zbus::proxy(
    interface = "io.github.roghelper.Daemon1",
    default_service = "io.github.roghelper.Daemon",
    default_path = "/io/github/roghelper/Daemon"
)]
trait Daemon1 {
    fn get_caps(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_state(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_telemetry(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_cpu_caps(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_cpu_telemetry(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_cpu_diagnostics(&self) -> zbus::Result<String>;
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

#[derive(Debug)]
struct SharedUiState {
    telemetry: Option<TelemetrySnapshot>,
    cpu: Option<CpuTelemetry>,
    cpu_caps: CpuCaps,
    cpu_usage_history: Vec<f32>,
    cpu_temp_history: Vec<f32>,
    cpu_power_history: Vec<f32>,
    caps: DeviceCaps,
    caps_text: String,
    warnings: Vec<String>,
    profile: Option<String>,
    gpu_mode: Option<String>,
    battery_limit: Option<u8>,
    lighting: Option<LightingInfo>,
    pending_profile: Option<String>,
    pending_gpu_mode: Option<String>,
    pending_battery_limit: Option<u8>,
    pending_cpu_actions: Vec<PendingCpuAction>,
    pending_lighting: Option<PendingLighting>,
    lighting_error: Option<String>,
    action_error: Option<String>,
    pending_toast: Option<(String, bool)>,
    daemon_error: Option<String>,
    show_window: bool,
    show_about: bool,
    show_gpu_page: bool,
    quit: bool,
}

impl Default for SharedUiState {
    fn default() -> Self {
        Self {
            telemetry: None,
            cpu: None,
            cpu_caps: CpuCaps::unknown(),
            cpu_usage_history: Vec::new(),
            cpu_temp_history: Vec::new(),
            cpu_power_history: Vec::new(),
            caps: DeviceCaps::unknown(),
            caps_text: String::new(),
            warnings: Vec::new(),
            profile: None,
            gpu_mode: None,
            battery_limit: None,
            lighting: None,
            pending_profile: None,
            pending_gpu_mode: None,
            pending_battery_limit: None,
            pending_cpu_actions: Vec::new(),
            pending_lighting: None,
            lighting_error: None,
            action_error: None,
            pending_toast: None,
            daemon_error: None,
            show_window: false,
            show_about: false,
            show_gpu_page: false,
            quit: false,
        }
    }
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
    device: String,
    brightness: u64,
    max_brightness: u64,
    can_set: bool,
    mode: String,
    supports_rgb: bool,
    supported_modes: Vec<String>,
}

#[derive(Debug, Clone)]
struct PendingLighting {
    brightness: u64,
    mode: String,
    rgb_hex: Option<String>,
}

#[derive(Debug, Clone)]
struct MetricCard {
    root: gtk::Box,
    value_label: gtk::Label,
    unit_label: gtk::Label,
    subtitle_label: gtk::Label,
    chip_label: gtk::Label,
}

impl MetricCard {
    fn new(title: &str) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.add_css_class("card");
        root.add_css_class("metric-card");
        root.set_hexpand(true);
        root.set_size_request(240, -1);

        let title_label = gtk::Label::new(Some(title));
        title_label.set_xalign(0.0);
        title_label.add_css_class("caption");
        title_label.add_css_class("dim-label");

        let value_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        value_row.set_halign(gtk::Align::Start);

        let value_label = gtk::Label::new(Some("--"));
        value_label.set_xalign(0.0);
        value_label.add_css_class("title-1");

        let unit_label = gtk::Label::new(None);
        unit_label.set_xalign(0.0);
        unit_label.add_css_class("title-4");
        unit_label.add_css_class("dim-label");
        unit_label.set_visible(false);

        value_row.append(&value_label);
        value_row.append(&unit_label);

        let subtitle_label = gtk::Label::new(None);
        subtitle_label.set_xalign(0.0);
        subtitle_label.add_css_class("dim-label");
        subtitle_label.set_visible(false);

        let chip_label = gtk::Label::new(None);
        chip_label.set_xalign(0.0);
        chip_label.add_css_class("status-chip");
        chip_label.set_visible(false);

        root.append(&title_label);
        root.append(&value_row);
        root.append(&subtitle_label);
        root.append(&chip_label);

        Self {
            root,
            value_label,
            unit_label,
            subtitle_label,
            chip_label,
        }
    }

    fn widget(&self) -> &gtk::Box {
        &self.root
    }

    fn set_value(&self, value: impl AsRef<str>) {
        self.value_label.set_text(value.as_ref());
    }

    fn set_unit(&self, unit: Option<&str>) {
        if let Some(unit) = unit {
            self.unit_label.set_text(unit);
            self.unit_label.set_visible(true);
        } else {
            self.unit_label.set_visible(false);
            self.unit_label.set_text("");
        }
    }

    fn set_subtitle(&self, subtitle: Option<&str>) {
        if let Some(subtitle) = subtitle {
            self.subtitle_label.set_text(subtitle);
            self.subtitle_label.set_visible(true);
        } else {
            self.subtitle_label.set_text("");
            self.subtitle_label.set_visible(false);
        }
    }

    fn set_status_chip(&self, chip_text: Option<&str>) {
        if let Some(chip_text) = chip_text {
            self.chip_label.set_text(chip_text);
            self.chip_label.set_visible(true);
        } else {
            self.chip_label.set_text("");
            self.chip_label.set_visible(false);
        }
    }
}

#[derive(Debug, Clone)]
struct DetailRow {
    row: adw::ActionRow,
    value_label: gtk::Label,
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
        "computer".to_string()
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

        let (has_profiles, has_gpu_modes, current_profile, current_gpu_mode, reboot_hint) = self
            .shared
            .lock()
            .map(|st| {
                (
                    st.caps.has_profiles,
                    st.caps.has_gpu_modes,
                    st.profile.clone(),
                    st.gpu_mode.clone(),
                    st.caps.requires_reboot_for_gpu_switch,
                )
            })
            .unwrap_or((false, false, None, None, false));

        let profile_selected = current_profile
            .as_deref()
            .map(tray_profile_index_from_text)
            .unwrap_or(1);
        let gpu_selected = current_gpu_mode
            .as_deref()
            .map(|v| gpu_mode_to_dropdown_index(v) as usize)
            .unwrap_or(1);

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
                enabled: has_gpu_modes,
                submenu: vec![
                    RadioGroup {
                        selected: gpu_selected,
                        select: Box::new(|this: &mut RogTray, index: usize| {
                            let mode = match index {
                                0 => "Integrated",
                                1 => "Hybrid",
                                _ => "Dedicated",
                            };
                            if let Ok(mut st) = this.shared.lock() {
                                st.pending_gpu_mode = Some(mode.to_string());
                                st.action_error = None;
                            }
                        }),
                        options: vec![
                            RadioItem {
                                label: "Integrated".to_string(),
                                ..Default::default()
                            },
                            RadioItem {
                                label: "Hybrid".to_string(),
                                ..Default::default()
                            },
                            RadioItem {
                                label: "Dedicated".to_string(),
                                ..Default::default()
                            },
                        ],
                    }
                    .into(),
                    StandardItem {
                        label: if reboot_hint {
                            "Switch may require reboot/logout".to_string()
                        } else {
                            "Switch applies immediately or with logout".to_string()
                        },
                        enabled: false,
                        ..Default::default()
                    }
                    .into(),
                ],
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

    let app = adw::Application::builder()
        .application_id("io.github.roghelper.UI")
        .build();

    app.connect_activate(build_ui);
    app.run();
    Ok(())
}

fn build_ui(app: &adw::Application) {
    let shared = Arc::new(Mutex::new(SharedUiState::default()));
    spawn_background(shared.clone());

    install_css();

    let stack = adw::ViewStack::new();
    stack.set_vexpand(true);

    let cpu_card = MetricCard::new("CPU Temperature");
    let gpu_card = MetricCard::new("GPU Temperature");
    let batt_card = MetricCard::new("Battery");
    let power_card = MetricCard::new("Power Source");
    let fans_card = MetricCard::new("Fans");
    let nvme_card = MetricCard::new("NVMe Temperature");

    let metrics_grid = gtk::FlowBox::new();
    metrics_grid.set_selection_mode(gtk::SelectionMode::None);
    metrics_grid.set_min_children_per_line(1);
    metrics_grid.set_max_children_per_line(2);
    metrics_grid.set_row_spacing(12);
    metrics_grid.set_column_spacing(12);
    metrics_grid.insert(cpu_card.widget(), -1);
    metrics_grid.insert(gpu_card.widget(), -1);
    metrics_grid.insert(batt_card.widget(), -1);
    metrics_grid.insert(power_card.widget(), -1);
    metrics_grid.insert(fans_card.widget(), -1);
    metrics_grid.insert(nvme_card.widget(), -1);

    let warning_banner = adw::Banner::new("Some controls need attention");
    warning_banner.set_button_label(Some("Open Diagnostics"));
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

    let quick_actions_title = gtk::Label::new(Some("Quick Actions"));
    quick_actions_title.set_xalign(0.0);
    quick_actions_title.add_css_class("title-3");

    let quick_actions_group = adw::PreferencesGroup::builder()
        .description("Controls are shown or disabled based on discovered device capabilities.")
        .build();

    let profile_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    profile_buttons.add_css_class("linked");
    let profile_quiet = gtk::ToggleButton::with_label("Quiet");
    let profile_balanced = gtk::ToggleButton::with_label("Balanced");
    let profile_turbo = gtk::ToggleButton::with_label("Turbo");
    profile_balanced.set_group(Some(&profile_quiet));
    profile_turbo.set_group(Some(&profile_quiet));
    profile_balanced.set_active(true);
    profile_buttons.append(&profile_quiet);
    profile_buttons.append(&profile_balanced);
    profile_buttons.append(&profile_turbo);

    let profile_row = adw::ActionRow::builder()
        .title("Performance Profile")
        .subtitle("Checking support...")
        .build();
    profile_row.add_suffix(&profile_buttons);
    profile_row.set_activatable(false);
    quick_actions_group.add(&profile_row);

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
    let gpu_mode_dropdown = gtk::DropDown::from_strings(&["Integrated", "Hybrid", "Dedicated"]);
    gpu_mode_dropdown.set_sensitive(false);
    let gpu_apply_button = gtk::Button::with_label("Apply");
    gpu_apply_button.set_sensitive(false);
    gpu_control_box.append(&gpu_mode_dropdown);
    gpu_control_box.append(&gpu_apply_button);
    let gpu_mode_row = adw::ActionRow::builder()
        .title("GPU Mode")
        .subtitle("Checking support...")
        .build();
    gpu_mode_row.add_suffix(&gpu_control_box);
    gpu_mode_row.set_activatable(false);
    quick_actions_group.add(&gpu_mode_row);
    {
        let shared = shared.clone();
        let gpu_mode_dropdown = gpu_mode_dropdown.clone();
        gpu_apply_button.connect_clicked(move |_| {
            let mode = match gpu_mode_dropdown.selected() {
                0 => "Integrated",
                1 => "Hybrid",
                _ => "Dedicated",
            };
            if let Ok(mut st) = shared.lock() {
                st.pending_gpu_mode = Some(mode.to_string());
                st.action_error = None;
            }
        });
    }

    let charge_limit_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let charge_limit_spin = gtk::SpinButton::with_range(50.0, 100.0, 5.0);
    charge_limit_spin.set_value(80.0);
    charge_limit_spin.set_sensitive(false);
    charge_limit_spin.set_numeric(true);
    let charge_apply_button = gtk::Button::with_label("Apply");
    charge_apply_button.set_sensitive(false);
    charge_limit_box.append(&charge_limit_spin);
    charge_limit_box.append(&charge_apply_button);
    let charge_limit_row = adw::ActionRow::builder()
        .title("Charge Limit")
        .subtitle("Checking support...")
        .build();
    charge_limit_row.add_suffix(&charge_limit_box);
    charge_limit_row.set_activatable(false);
    quick_actions_group.add(&charge_limit_row);
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
    kbd_brightness_spin.set_sensitive(false);
    kbd_brightness_spin.set_numeric(true);
    let kbd_apply_button = gtk::Button::with_label("Apply");
    kbd_apply_button.set_sensitive(false);
    kbd_control_box.append(&kbd_brightness_spin);
    kbd_control_box.append(&kbd_apply_button);

    let kbd_backlight_row = adw::ActionRow::builder()
        .title("Keyboard Backlight")
        .subtitle("Checking support...")
        .build();
    kbd_backlight_row.add_suffix(&kbd_control_box);
    kbd_backlight_row.set_activatable(false);
    quick_actions_group.add(&kbd_backlight_row);

    {
        let shared = shared.clone();
        let kbd_brightness_spin = kbd_brightness_spin.clone();
        kbd_apply_button.connect_clicked(move |_| {
            let desired_brightness = kbd_brightness_spin.value().round().max(0.0) as u64;
            let desired_mode = shared
                .lock()
                .ok()
                .and_then(|st| st.lighting.as_ref().map(|l| l.mode.clone()))
                .unwrap_or_else(|| "Static".to_string());
            if let Ok(mut st) = shared.lock() {
                st.pending_lighting = Some(PendingLighting {
                    brightness: desired_brightness,
                    mode: desired_mode,
                    rgb_hex: None,
                });
                st.lighting_error = None;
            }
        });
    }

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

    let details_expander = gtk::Expander::new(Some("Details"));
    details_expander.set_expanded(false);
    details_expander.set_child(Some(&details_box));

    let status_label = gtk::Label::new(Some(""));
    status_label.set_xalign(0.0);
    status_label.set_wrap(true);
    status_label.add_css_class("dim-label");

    let dash_root = gtk::Box::new(gtk::Orientation::Vertical, 18);
    dash_root.set_margin_top(18);
    dash_root.set_margin_bottom(18);
    dash_root.set_margin_start(18);
    dash_root.set_margin_end(18);
    dash_root.append(&warning_area);
    dash_root.append(&metrics_grid);
    dash_root.append(&quick_actions_title);
    dash_root.append(&quick_actions_group);
    dash_root.append(&details_expander);
    dash_root.append(&status_label);

    let dash = clamped_scroller(&dash_root);

    let cpu_page = adw::PreferencesPage::new();

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
    cpu_page.add(&cpu_overview_group);

    let cpu_banner_group = adw::PreferencesGroup::new();
    let cpu_read_only_banner = adw::Banner::new("Some CPU controls are unavailable");
    cpu_read_only_banner.set_button_label(Some("Open Diagnostics"));
    cpu_read_only_banner.set_revealed(false);
    cpu_banner_group.add(&cpu_read_only_banner);
    cpu_page.add(&cpu_banner_group);

    {
        let stack = stack.clone();
        cpu_read_only_banner.connect_button_clicked(move |_| {
            stack.set_visible_child_name("diagnostics");
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
        .description("Safe controls applied through daemon policy endpoints.")
        .build();

    let cpu_turbo_switch = gtk::Switch::new();
    let cpu_turbo_row = adw::ActionRow::builder().title("Turbo Boost").build();
    cpu_turbo_row.add_suffix(&cpu_turbo_switch);
    cpu_turbo_row.set_activatable(false);
    cpu_quick_group.add(&cpu_turbo_row);

    let cpu_power_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    cpu_power_buttons.add_css_class("linked");
    let cpu_power_quiet = gtk::ToggleButton::with_label("Quiet");
    let cpu_power_balanced = gtk::ToggleButton::with_label("Balanced");
    let cpu_power_perf = gtk::ToggleButton::with_label("Performance");
    cpu_power_balanced.set_group(Some(&cpu_power_quiet));
    cpu_power_perf.set_group(Some(&cpu_power_quiet));
    cpu_power_balanced.set_active(true);
    cpu_power_buttons.append(&cpu_power_quiet);
    cpu_power_buttons.append(&cpu_power_balanced);
    cpu_power_buttons.append(&cpu_power_perf);
    let cpu_power_row = adw::ActionRow::builder().title("Power Mode").build();
    cpu_power_row.add_suffix(&cpu_power_buttons);
    cpu_power_row.set_activatable(false);
    cpu_quick_group.add(&cpu_power_row);

    let cpu_min_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 400.0, 6000.0, 50.0);
    cpu_min_scale.set_draw_value(true);
    cpu_min_scale.set_value_pos(gtk::PositionType::Right);
    cpu_min_scale.set_hexpand(true);
    let cpu_min_row = adw::ActionRow::builder().title("Min Frequency").build();
    cpu_min_row.add_suffix(&cpu_min_scale);
    cpu_min_row.set_activatable(false);
    cpu_quick_group.add(&cpu_min_row);

    let cpu_max_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 400.0, 6000.0, 50.0);
    cpu_max_scale.set_draw_value(true);
    cpu_max_scale.set_value_pos(gtk::PositionType::Right);
    cpu_max_scale.set_hexpand(true);
    let cpu_max_row = adw::ActionRow::builder().title("Max Frequency").build();
    cpu_max_row.add_suffix(&cpu_max_scale);
    cpu_max_row.set_activatable(false);
    cpu_quick_group.add(&cpu_max_row);

    let cpu_apply_quick = gtk::Button::with_label("Apply CPU Quick Controls");
    cpu_apply_quick.add_css_class("suggested-action");
    let cpu_apply_row = adw::ActionRow::builder().title("Apply").build();
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

            if caps.control_writable(CpuControlKind::Boost) {
                actions.push(PendingCpuAction::Turbo(cpu_turbo_switch.is_active()));
            }

            let mode = if cpu_power_quiet.is_active() {
                "Quiet"
            } else if cpu_power_balanced.is_active() {
                "Balanced"
            } else {
                "Performance"
            };
            if caps.control_writable(CpuControlKind::PowerMode) {
                actions.push(PendingCpuAction::PowerMode(mode.to_string()));
            }

            if caps.control_writable(CpuControlKind::FreqLimits)
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

    cpu_page.add(&cpu_quick_group);

    let cpu_policy_group = adw::PreferencesGroup::builder()
        .title("Policy")
        .description("EPP controls how aggressively CPU frequency/power is tuned.")
        .build();
    let cpu_scaling_driver = pref_value_row(&cpu_policy_group, "Scaling driver", false);
    let cpu_cpu_count = pref_value_row(&cpu_policy_group, "Physical cores", false);
    let cpu_thread_count = pref_value_row(&cpu_policy_group, "Logical threads", false);

    let cpu_governor_combo = gtk::ComboBoxText::new();
    cpu_governor_combo.set_sensitive(false);
    let cpu_governor_row = adw::ActionRow::builder().title("Governor").build();
    cpu_governor_row.add_suffix(&cpu_governor_combo);
    cpu_governor_row.set_activatable(false);
    cpu_policy_group.add(&cpu_governor_row);

    let cpu_epp_combo = gtk::ComboBoxText::new();
    cpu_epp_combo.set_sensitive(false);
    let cpu_epp_row = adw::ActionRow::builder()
        .title("Energy Performance Preference")
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

    let cpu_apply_policy = gtk::Button::with_label("Apply Policy");
    let cpu_apply_policy_row = adw::ActionRow::builder().title("Apply").build();
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
                if caps.control_writable(CpuControlKind::Governor) {
                    if let Some(gov) = cpu_governor_combo.active_id() {
                        st.pending_cpu_actions
                            .push(PendingCpuAction::Governor(gov.to_string()));
                    }
                }
                if caps.control_writable(CpuControlKind::Epp) {
                    if let Some(epp) = cpu_epp_combo.active_id() {
                        st.pending_cpu_actions
                            .push(PendingCpuAction::Epp(epp.to_string()));
                    }
                }
                st.action_error = None;
            }
        });
    }

    cpu_page.add(&cpu_policy_group);

    let cpu_per_core_group = adw::PreferencesGroup::builder()
        .title("Logical CPUs / Threads")
        .description(
            "Each row is one logical CPU/thread. Physical core count is shown above, and cpufreq policy ids are shown when the kernel exposes them.",
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
    cpu_page.add(&cpu_per_core_group);

    let cpu_adv_group = adw::PreferencesGroup::builder().title("Advanced").build();
    let cpu_advanced_expander = gtk::Expander::new(Some("Advanced CPU Controls"));
    cpu_advanced_expander.set_expanded(false);
    let cpu_adv_box = gtk::Box::new(gtk::Orientation::Vertical, 8);

    let cpu_core_toggle_title = gtk::Label::new(Some("Logical CPU Online/Offline"));
    cpu_core_toggle_title.set_xalign(0.0);
    cpu_core_toggle_title.add_css_class("heading");
    let cpu_core_toggle_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    cpu_adv_box.append(&cpu_core_toggle_title);
    cpu_adv_box.append(&cpu_core_toggle_box);

    cpu_adv_box.append(&cpu_access_title);
    cpu_adv_box.append(&cpu_access_hint);
    cpu_adv_box.append(&cpu_access_report_scroll);

    let cpu_copy_diag = gtk::Button::with_label("Copy CPU diagnostics");
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
    cpu_page.add(&cpu_adv_group);

    let cpu_view = clamped_scroller(&cpu_page);

    let battery_page = adw::PreferencesPage::new();
    let battery_group_status = adw::PreferencesGroup::builder()
        .title("Status")
        .description("Battery telemetry is best-effort and depends on available providers.")
        .build();
    let batt_tbl_percent = pref_value_row(&battery_group_status, "Charge", false);
    let batt_tbl_state = pref_value_row(&battery_group_status, "Status", false);
    let batt_tbl_source = pref_value_row(&battery_group_status, "Power Source", false);
    let batt_tbl_ac_online = pref_value_row(&battery_group_status, "AC Online", false);
    battery_page.add(&battery_group_status);

    let battery_group_power = adw::PreferencesGroup::builder().title("Power").build();
    let batt_tbl_charge_power = pref_value_row(&battery_group_power, "Power Input", false);
    let batt_tbl_discharge_power = pref_value_row(&battery_group_power, "Power Draw", false);
    let batt_tbl_ttf = pref_value_row(&battery_group_power, "Time to Full", false);
    let batt_tbl_tte = pref_value_row(&battery_group_power, "Time to Empty", false);
    battery_page.add(&battery_group_power);

    let battery_group_health = adw::PreferencesGroup::builder().title("Health").build();
    let batt_tbl_health = pref_value_row(&battery_group_health, "Health", false);
    let batt_tbl_cycles = pref_value_row(&battery_group_health, "Cycle Count", true);
    battery_page.add(&battery_group_health);

    let battery = clamped_scroller(&battery_page);

    let ram_page = adw::PreferencesPage::new();
    let ram_group_core = adw::PreferencesGroup::builder()
        .title("Core Memory")
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
    ram_page.add(&ram_group_core);

    let ram_group_swap = adw::PreferencesGroup::builder().title("Swap").build();
    let ram_swap_total = pref_value_row(&ram_group_swap, "Swap Total", false);
    let ram_swap_used = pref_value_row(&ram_group_swap, "Swap Used", false);
    let ram_swap_free = pref_value_row(&ram_group_swap, "Swap Free", false);
    let ram_swap_in = pref_value_row(&ram_group_swap, "Swap In", false);
    let ram_swap_out = pref_value_row(&ram_group_swap, "Swap Out", false);
    let ram_zram = pref_value_row(&ram_group_swap, "zram", false);
    let ram_zswap = pref_value_row(&ram_group_swap, "zswap", false);
    ram_page.add(&ram_group_swap);

    let ram_group_pressure = adw::PreferencesGroup::builder()
        .title("Memory Pressure (PSI)")
        .build();
    let ram_psi_some = pref_value_row(&ram_group_pressure, "some (10/60/300)", true);
    let ram_psi_full = pref_value_row(&ram_group_pressure, "full (10/60/300)", true);
    ram_page.add(&ram_group_pressure);

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
    ram_page.add(&ram_group_adv);

    let copy_top_button = gtk::Button::with_label("Copy");
    let ram_group_top = adw::PreferencesGroup::builder()
        .title("Top Memory Users")
        .description("Sorted by RSS (top 10).")
        .header_suffix(&copy_top_button)
        .build();
    let mut ram_top_rows: Vec<(adw::ActionRow, gtk::Label)> = Vec::new();
    for _ in 0..10 {
        let row = adw::ActionRow::builder().title("(n/a)").build();
        let value = gtk::Label::new(Some(""));
        value.set_xalign(1.0);
        value.add_css_class("monospace");
        row.add_suffix(&value);
        row.set_activatable(false);
        ram_group_top.add(&row);
        ram_top_rows.push((row, value));
    }
    ram_page.add(&ram_group_top);

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

    let gpu_page = adw::PreferencesPage::new();

    let gpu_state_group = adw::PreferencesGroup::builder()
        .title("Current State")
        .description("Read from daemon-backed asusd/supergfx providers when available.")
        .build();
    let gpu_page_current_profile = pref_value_row(&gpu_state_group, "Profile", false);
    let gpu_page_current_mode = pref_value_row(&gpu_state_group, "GPU Mode", false);
    let gpu_page_switch_hint = pref_value_row(&gpu_state_group, "Switch Hint", false);
    gpu_page_current_profile.set_text("Checking support...");
    gpu_page_current_mode.set_text("Checking support...");
    gpu_page_switch_hint.set_text("Checking support...");
    gpu_page.add(&gpu_state_group);

    let gpu_controls_group = adw::PreferencesGroup::builder()
        .title("Controls")
        .description("Changes are applied through rog-helperd session DBus API.")
        .build();

    let gpu_page_profile_combo = gtk::ComboBoxText::new();
    for label in ["Silent", "Balanced", "Turbo"] {
        gpu_page_profile_combo.append(Some(label), label);
    }
    gpu_page_profile_combo.set_active_id(Some("Balanced"));
    gpu_page_profile_combo.set_sensitive(false);
    let gpu_page_profile_apply = gtk::Button::with_label("Apply");
    gpu_page_profile_apply.set_sensitive(false);
    let gpu_page_profile_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
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
    for label in ["Integrated", "Hybrid", "Dedicated"] {
        gpu_page_mode_combo.append(Some(label), label);
    }
    gpu_page_mode_combo.set_active_id(Some("Hybrid"));
    gpu_page_mode_combo.set_sensitive(false);
    let gpu_page_mode_apply = gtk::Button::with_label("Apply");
    gpu_page_mode_apply.set_sensitive(false);
    let gpu_page_mode_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
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

    gpu_page.add(&gpu_controls_group);

    let gpu_status_group = adw::PreferencesGroup::builder().title("Status").build();
    let gpu_page_last_action = pref_value_row(&gpu_status_group, "Status", false);
    gpu_page_last_action.set_text("Checking support...");
    gpu_page.add(&gpu_status_group);

    let gpu = clamped_scroller(&gpu_page);

    let diag_buffer = gtk::TextBuffer::new(None);
    diag_buffer.set_text("Loading diagnostics...");
    let diag_view = gtk::TextView::with_buffer(&diag_buffer);
    diag_view.set_editable(false);
    diag_view.set_cursor_visible(false);
    diag_view.set_wrap_mode(gtk::WrapMode::WordChar);
    diag_view.add_css_class("monospace");

    let diag_scroller = gtk::ScrolledWindow::new();
    diag_scroller.set_vexpand(true);
    diag_scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    diag_scroller.set_child(Some(&diag_view));

    let copy_diag_button = gtk::Button::with_label("Copy diagnostics");
    {
        let shared = shared.clone();
        copy_diag_button.connect_clicked(move |_| {
            let text = shared
                .lock()
                .map(|st| st.caps_text.clone())
                .unwrap_or_default();
            let Some(display) = gtk::gdk::Display::default() else {
                return;
            };
            display.clipboard().set_text(&text);
        });
    }

    let diag_root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    diag_root.set_margin_top(18);
    diag_root.set_margin_bottom(18);
    diag_root.set_margin_start(18);
    diag_root.set_margin_end(18);
    diag_root.append(&copy_diag_button);
    diag_root.append(&diag_scroller);

    let diag = clamped_scroller(&diag_root);

    let about_page = adw::PreferencesPage::new();

    let about_app_group = adw::PreferencesGroup::builder()
        .title("App")
        .description("Linux-native control app for ASUS ROG laptops.")
        .build();
    let about_name = pref_value_row(&about_app_group, "Name", false);
    let about_binary = pref_value_row(&about_app_group, "Binary", false);
    let about_version = pref_value_row(&about_app_group, "Version", false);
    let about_license = pref_value_row(&about_app_group, "License", false);
    let about_dbus = pref_value_row(&about_app_group, "Session DBus API", true);
    about_page.add(&about_app_group);

    let about_dev_group = adw::PreferencesGroup::builder().title("Developer").build();
    let about_developer = pref_value_row(&about_dev_group, "Maintainer", false);
    let about_source = pref_value_row(&about_dev_group, "Source", false);
    about_page.add(&about_dev_group);

    let source_url = if env!("CARGO_PKG_REPOSITORY").trim().is_empty() {
        APP_SOURCE_FALLBACK_URL
    } else {
        env!("CARGO_PKG_REPOSITORY")
    };
    let developers = if env!("CARGO_PKG_AUTHORS").trim().is_empty() {
        APP_DEVELOPER_FALLBACK.to_string()
    } else {
        env!("CARGO_PKG_AUTHORS").replace(':', ", ")
    };

    about_name.set_text(APP_DISPLAY_NAME);
    about_binary.set_text(APP_BINARY_NAME);
    about_version.set_text(env!("CARGO_PKG_VERSION"));
    about_license.set_text(env!("CARGO_PKG_LICENSE"));
    about_dbus.set_text(&format!(
        "{DAEMON_DBUS_NAME} {DAEMON_DBUS_PATH} ({DAEMON_DBUS_IFACE})"
    ));
    about_developer.set_text(&developers);
    about_source.set_text(source_url);

    let about = clamped_scroller(&about_page);

    let lighting_page = adw::PreferencesPage::new();
    let lighting_overview_group = adw::PreferencesGroup::builder()
        .title("Keyboard Lighting")
        .description("Controls are capability-driven and depend on writable backends.")
        .build();
    let lighting_backend = pref_value_row(&lighting_overview_group, "Backend", false);
    let lighting_current = pref_value_row(&lighting_overview_group, "Brightness", false);
    let lighting_mode = pref_value_row(&lighting_overview_group, "Mode", false);
    lighting_page.add(&lighting_overview_group);

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
        .title("RGB Color")
        .subtitle("Requires Aura/asusd support")
        .build();
    rgb_row.add_suffix(&rgb_button);
    rgb_row.set_activatable(false);
    lighting_controls_group.add(&rgb_row);

    let apply_lighting = gtk::Button::with_label("Apply lighting");
    apply_lighting.add_css_class("suggested-action");
    let apply_row = adw::ActionRow::builder().title("Apply Changes").build();
    apply_row.add_suffix(&apply_lighting);
    apply_row.set_activatable(false);
    lighting_controls_group.add(&apply_row);
    lighting_page.add(&lighting_controls_group);

    let lighting_status_group = adw::PreferencesGroup::builder().title("Status").build();
    let lighting_error = pref_value_row(&lighting_status_group, "Last action", false);
    lighting_page.add(&lighting_status_group);

    {
        let shared = shared.clone();
        let brightness_scale = brightness_scale.clone();
        let mode_combo = mode_combo.clone();
        let rgb_button = rgb_button.clone();
        apply_lighting.connect_clicked(move |_| {
            let desired_brightness = brightness_scale.value().round().max(0.0) as u64;
            let desired_mode = mode_combo
                .active_id()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Static".to_string());

            let mut desired_rgb_hex = None;
            if let Ok(st) = shared.lock() {
                if st
                    .lighting
                    .as_ref()
                    .map(|l| l.supports_rgb)
                    .unwrap_or(false)
                {
                    desired_rgb_hex = Some(rgba_to_hex(&rgb_button.rgba()));
                }
            }

            if let Ok(mut st) = shared.lock() {
                st.pending_lighting = Some(PendingLighting {
                    brightness: desired_brightness,
                    mode: desired_mode,
                    rgb_hex: desired_rgb_hex,
                });
                st.lighting_error = None;
            }
        });
    }

    let lighting = clamped_scroller(&lighting_page);

    stack.add_titled(&dash, Some("dashboard"), "Dashboard");
    stack.add_titled(&cpu_view, Some("cpu"), "CPU");
    stack.add_titled(&gpu, Some("gpu"), "GPU");
    stack.add_titled(&battery, Some("battery"), "Battery");
    stack.add_titled(&ram, Some("ram"), "RAM");
    stack.add_titled(&lighting, Some("lighting"), "Lighting");
    stack.add_titled(&diag, Some("diagnostics"), "Diagnostics");
    stack.add_titled(&about, Some("about"), "About");

    let switcher = adw::ViewSwitcher::new();
    switcher.set_stack(Some(&stack));

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&switcher));

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&stack));

    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&view));

    let win = adw::ApplicationWindow::builder()
        .application(app)
        .title("rog-helper")
        .default_width(560)
        .default_height(380)
        .build();
    win.set_content(Some(&toast_overlay));
    win.present();

    {
        let stack = stack.clone();
        warning_banner.connect_button_clicked(move |_| {
            stack.set_visible_child_name("diagnostics");
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
    let last_supported_modes = std::rc::Rc::new(std::cell::RefCell::<Vec<String>>::new(Vec::new()));
    let last_cpu_governors = std::rc::Rc::new(std::cell::RefCell::<Vec<String>>::new(Vec::new()));
    let last_cpu_epp = std::rc::Rc::new(std::cell::RefCell::<Vec<String>>::new(Vec::new()));
    let detail_fan_rows_ref = detail_fan_rows.clone();
    let details_fan_group_ref = details_fan_group.clone();
    glib::timeout_add_local(Duration::from_millis(250), move || {
        let (
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
            lighting_error_txt,
            action_error_txt,
            daemon_error,
            show_window,
            show_about,
            show_gpu_page,
            pending_toast,
            quit,
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
                st.caps.clone(),
                st.caps_text.clone(),
                st.warnings.clone(),
                st.profile.clone(),
                st.gpu_mode.clone(),
                st.battery_limit,
                st.lighting.clone(),
                st.lighting_error.clone(),
                st.action_error.clone(),
                st.daemon_error.clone(),
                show_window,
                show_about,
                show_gpu_page,
                pending_toast,
                st.quit,
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

        if quit {
            app_clone.quit();
            return glib::ControlFlow::Break;
        }

        if let Some(t) = telemetry {
            if let Some(v) = t.cpu_temp_c {
                cpu_card.set_value(format!("{v:.1}"));
                cpu_card.set_unit(Some("°C"));
            } else {
                cpu_card.set_value("--");
                cpu_card.set_unit(None);
            }

            if let Some(v) = t.gpu_temp_c {
                gpu_card.set_value(format!("{v:.1}"));
                gpu_card.set_unit(Some("°C"));
            } else {
                gpu_card.set_value("--");
                gpu_card.set_unit(None);
            }

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
            batt_card.set_status_chip(battery_chip.as_deref());

            power_card.set_value(match t.power_source {
                Some(PowerSource::Ac) => "AC".to_string(),
                Some(PowerSource::Battery) => "Battery".to_string(),
                None => "--".to_string(),
            });
            power_card.set_unit(None);

            if t.fan_rows.is_empty() {
                fans_card.set_value("--");
                fans_card.set_unit(None);
                fans_card.set_subtitle(Some("No fan RPM sensors exposed"));
            } else {
                let reporting_count = t.fan_rows.iter().filter(|fan| fan.rpm.is_some()).count();
                let max_rpm = t.fan_rows.iter().filter_map(|fan| fan.rpm).max();
                if let Some(max_rpm) = max_rpm {
                    fans_card.set_value(max_rpm.to_string());
                    fans_card.set_unit(Some("rpm"));
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
                nvme_card.widget().set_visible(true);
                nvme_card.set_value(format!("{v:.1}"));
                nvme_card.set_unit(Some("°C"));
            } else {
                nvme_card.widget().set_visible(false);
                nvme_card.set_value("--");
                nvme_card.set_unit(None);
            }

            // Battery table.
            batt_tbl_percent.set_text(
                &t.battery_percent
                    .map(|v| format!("{v:.0}%"))
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            batt_tbl_state.set_text(&format_battery_state(&t));
            batt_tbl_source.set_text(
                &t.power_source
                    .map(|p| match p {
                        PowerSource::Ac => "AC".to_string(),
                        PowerSource::Battery => "Battery".to_string(),
                    })
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            batt_tbl_ac_online.set_text(
                &t.ac_online
                    .map(|v| if v { "Yes" } else { "No" }.to_string())
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            batt_tbl_charge_power.set_text(
                &t.battery_charge_power_w
                    .map(|v| format!("{v:.1} W"))
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            batt_tbl_discharge_power.set_text(
                &t.battery_discharge_power_w
                    .map(|v| format!("{v:.1} W"))
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            batt_tbl_health.set_text(
                &t.battery_health_percent
                    .map(|v| format!("{v:.0}%"))
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            batt_tbl_cycles.set_text(
                &t.battery_cycle_count
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            batt_tbl_ttf.set_text(
                &t.battery_time_to_full_s
                    .map(format_duration_short)
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            batt_tbl_tte.set_text(
                &t.battery_time_to_empty_s
                    .map(format_duration_short)
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );

            // RAM.
            ram_total.set_text(&format_bytes_gib_opt(t.mem_total_bytes));
            ram_used.set_text(&format_used_ram(t.mem_used_bytes, t.mem_used_percent));
            ram_avail.set_text(&format_bytes_gib_opt(t.mem_available_bytes));
            ram_free.set_text(&format_bytes_gib_opt(t.mem_free_bytes));
            ram_cached.set_text(&format_bytes_gib_opt(t.mem_cached_bytes));
            ram_buffers.set_text(&format_bytes_gib_opt(t.mem_buffers_bytes));
            ram_shared.set_text(&format_bytes_gib_opt(t.mem_shared_bytes));
            ram_anon.set_text(&format_bytes_gib_opt(t.mem_anon_bytes));

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
            for (idx, (row, value)) in ram_top_rows.iter().enumerate() {
                if let Some(p) = rows.get(idx) {
                    row.set_title(&p.name);
                    value.set_text(&format!(
                        "{} RSS | {} SWAP | pid {} | {}",
                        format_bytes_human(p.rss_bytes),
                        format_bytes_human(p.swap_bytes),
                        p.pid,
                        p.user
                    ));
                } else if idx == 0 {
                    row.set_title("(n/a)");
                    value.set_text("");
                } else {
                    row.set_title("");
                    value.set_text("");
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
            } else {
                cpu_card_usage.set_value("--");
                cpu_card_usage.set_unit(None);
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
            cpu_card_temp.set_status_chip(cpu_data.status.as_deref());

            cpu_scaling_driver.set_text(
                &cpu_caps
                    .scaling_driver
                    .clone()
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            cpu_cpu_count.set_text(&cpu_caps.cpu_count.to_string());
            cpu_thread_count.set_text(&cpu_caps.thread_count.to_string());

            let turbo_writable = cpu_caps.control_writable(CpuControlKind::Boost);
            let power_mode_writable = cpu_caps.control_writable(CpuControlKind::PowerMode);
            let governor_writable = cpu_caps.control_writable(CpuControlKind::Governor);
            let epp_writable = cpu_caps.control_writable(CpuControlKind::Epp);
            let freq_limits_writable = cpu_caps.control_writable(CpuControlKind::FreqLimits);
            let core_online_writable = cpu_caps.control_writable(CpuControlKind::CoreOnline);
            let quick_sensitive = turbo_writable || power_mode_writable || freq_limits_writable;

            cpu_turbo_switch.set_sensitive(turbo_writable);
            if let Some(v) = cpu_data.turbo_boost_enabled {
                if !cpu_turbo_switch.has_focus() {
                    cpu_turbo_switch.set_active(v);
                }
            }
            cpu_turbo_row.set_subtitle(cpu_control_subtitle(
                &cpu_caps,
                CpuControlKind::Boost,
                "Enable/disable CPU turbo boost",
                "Turbo boost control is not supported on this system.",
            ));

            cpu_power_quiet.set_sensitive(quick_sensitive);
            cpu_power_balanced.set_sensitive(quick_sensitive);
            cpu_power_perf.set_sensitive(quick_sensitive);
            cpu_apply_quick.set_sensitive(quick_sensitive);
            cpu_power_row.set_subtitle(cpu_control_subtitle(
                &cpu_caps,
                CpuControlKind::PowerMode,
                "Apply a high-level CPU power preset through governor/EPP controls.",
                "Power mode presets are not supported on this system.",
            ));
            cpu_min_scale.set_sensitive(freq_limits_writable && cpu_caps.has_min_freq_limit);
            cpu_max_scale.set_sensitive(freq_limits_writable && cpu_caps.has_max_freq_limit);
            cpu_min_row.set_subtitle(cpu_control_subtitle(
                &cpu_caps,
                CpuControlKind::FreqLimits,
                "Adjust the minimum CPU frequency limit.",
                "Minimum frequency limits are not supported on this system.",
            ));
            cpu_max_row.set_subtitle(cpu_control_subtitle(
                &cpu_caps,
                CpuControlKind::FreqLimits,
                "Adjust the maximum CPU frequency limit.",
                "Maximum frequency limits are not supported on this system.",
            ));
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
            cpu_governor_row.set_subtitle(cpu_control_subtitle(
                &cpu_caps,
                CpuControlKind::Governor,
                "Select the active CPU governor.",
                "Governor control is not supported on this system.",
            ));
            cpu_epp_row.set_subtitle(cpu_control_subtitle(
                &cpu_caps,
                CpuControlKind::Epp,
                "Select the energy performance preference.",
                "Energy Performance Preference is not supported on this system.",
            ));
            cpu_apply_policy.set_sensitive(governor_writable || epp_writable);

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
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let label = gtk::Label::new(Some(&format!(
                    "CPU {} ({})",
                    core.logical_cpu_id,
                    format_cpu_toggle_summary(
                        core.physical_core_index,
                        core.thread_index,
                        core.thread_count
                    )
                )));
                label.set_xalign(0.0);
                label.set_wrap(true);
                label.set_hexpand(true);
                let switch = gtk::Switch::new();
                switch.set_active(core.online);
                let allow_toggle =
                    cpu_caps.has_core_online && core_online_writable && core.logical_cpu_id != 0;
                switch.set_sensitive(allow_toggle);
                if allow_toggle {
                    let shared = shared_clone.clone();
                    let win = win_clone.clone();
                    let core_id = core.logical_cpu_id;
                    let original_online = core.online;
                    switch.connect_active_notify(move |sw| {
                        let desired_online = sw.is_active();
                        if desired_online == original_online {
                            return;
                        }
                        let dialog = adw::MessageDialog::new(
                            Some(&win),
                            Some("Confirm Logical CPU Toggle"),
                            Some(
                                "Changing a logical CPU online/offline state can impact stability and thermals. Continue?",
                            ),
                        );
                        dialog.add_responses(&[("cancel", "Cancel"), ("apply", "Apply")]);
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
                row.append(&label);
                row.append(&switch);
                cpu_core_toggle_box.append(&row);
            }

            cpu_access_report.set_text(&cpu_access_report_text(&cpu_caps));
        } else {
            cpu_card_temp.set_value("--");
            cpu_card_usage.set_value("--");
            cpu_card_power.set_value("--");
            cpu_card_clock.set_value("--");
            cpu_read_only_banner.set_revealed(false);
            cpu_per_core_buffer.set_text("Logical CPU / thread telemetry unavailable.");
            cpu_access_report.set_text(&cpu_access_report_text(&cpu_caps));
        }

        diag_buffer.set_text(if caps_text.is_empty() {
            "Loading diagnostics..."
        } else {
            &caps_text
        });

        if let Some(ref e) = daemon_error {
            status_label.set_text(&format!(
                "Daemon not reachable on session DBus ({DAEMON_DBUS_NAME} {DAEMON_DBUS_PATH} {DAEMON_DBUS_IFACE}). {e}"
            ));
        } else if let Some(ref e) = action_error_txt {
            status_label.set_text(&friendly_action_error(e));
        } else {
            status_label.set_text("");
        }

        let support_messages = support_messages(&caps, &cpu_caps);
        let profile_read_failed = warning_detail(&warnings, "profile read failed: ");
        let gpu_mode_read_failed = warning_detail(&warnings, "GPU mode read failed: ");
        let charge_limit_read_failed = warning_detail(&warnings, "charge limit read failed: ");

        if !support_messages.is_empty() {
            warning_banner.set_title(&support_banner_title(&caps, &cpu_caps));
            warning_banner.set_button_label(Some("Open Diagnostics"));
            warning_subtitle.set_text(&support_messages.join("\n"));
            warning_subtitle.set_visible(true);
            warning_banner.set_revealed(true);
            warning_area.set_visible(true);
            open_diagnostics_button.set_visible(false);
        } else if !warnings.is_empty() {
            warning_banner.set_title("Hardware warnings detected");
            warning_banner.set_button_label(Some("Open Diagnostics"));
            warning_subtitle.set_text(&warnings.join("\n"));
            warning_subtitle.set_visible(true);
            warning_banner.set_revealed(true);
            warning_area.set_visible(true);
            open_diagnostics_button.set_visible(false);
        } else {
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
            gpu_page_profile_row.set_subtitle("Apply profile through daemon");
        } else {
            profile_row.set_subtitle(&feature_control_subtitle(
                &caps.profile_access,
                "Quiet / Balanced / Turbo",
            ));
            gpu_page_current_profile.set_text(&feature_state_value(
                None,
                &caps.profile_access,
                profile_read_failed,
            ));
            gpu_page_profile_row.set_subtitle(&feature_control_subtitle(
                &caps.profile_access,
                "Apply profile through daemon",
            ));
        }

        gpu_mode_row.set_visible(true);
        gpu_page_mode_combo.set_sensitive(caps.gpu_mode_access.is_available());
        gpu_page_mode_apply.set_sensitive(caps.gpu_mode_access.is_available());
        if caps.gpu_mode_access.is_available() {
            gpu_mode_dropdown.set_sensitive(true);
            gpu_apply_button.set_sensitive(true);
            if let Some(current) = gpu_mode.as_deref() {
                gpu_mode_dropdown.set_selected(gpu_mode_to_dropdown_index(current));
                gpu_mode_row.set_subtitle(&format!("Current: {current}"));
                if !gpu_page_mode_combo.has_focus() {
                    match gpu_mode_to_dropdown_index(current) {
                        0 => gpu_page_mode_combo.set_active_id(Some("Integrated")),
                        1 => gpu_page_mode_combo.set_active_id(Some("Hybrid")),
                        _ => gpu_page_mode_combo.set_active_id(Some("Dedicated")),
                    };
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
            gpu_page_switch_hint.set_text(&gpu_switch_hint_text(&caps, &warnings));
            gpu_page_mode_row.set_subtitle("Apply GPU mode through daemon");
        } else {
            gpu_mode_dropdown.set_sensitive(false);
            gpu_apply_button.set_sensitive(false);
            gpu_mode_row.set_subtitle(&feature_control_subtitle(
                &caps.gpu_mode_access,
                "Select mode and apply",
            ));
            gpu_page_current_mode.set_text(&feature_state_value(
                None,
                &caps.gpu_mode_access,
                gpu_mode_read_failed,
            ));
            gpu_page_switch_hint.set_text(&gpu_switch_hint_text(&caps, &warnings));
            gpu_page_mode_row.set_subtitle(&feature_control_subtitle(
                &caps.gpu_mode_access,
                "Apply GPU mode through daemon",
            ));
        }

        charge_limit_row.set_visible(true);
        if caps.charge_limit_access.is_available() {
            charge_limit_spin.set_sensitive(true);
            charge_apply_button.set_sensitive(true);
            if let Some(limit) = battery_limit {
                if !charge_limit_spin.has_focus() {
                    charge_limit_spin.set_value(limit as f64);
                }
                charge_limit_row.set_subtitle(&format!("Current: {limit}%"));
            } else if charge_limit_read_failed.is_some() {
                charge_limit_row.set_subtitle(
                    "Charge limit is detected, but the current value could not be read.",
                );
            } else {
                charge_limit_row.set_subtitle("Set desired limit and apply");
            }
        } else {
            charge_limit_spin.set_sensitive(false);
            charge_apply_button.set_sensitive(false);
            charge_limit_row.set_subtitle(&feature_control_subtitle(
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

        let has_kbd_backlight = caps.has_kbd_backlight || lighting.is_some();
        kbd_backlight_row.set_visible(has_kbd_backlight);
        if has_kbd_backlight {
            if let Some(ref l) = lighting {
                kbd_brightness_spin.set_range(0.0, l.max_brightness as f64);
                if !kbd_brightness_spin.has_focus() {
                    kbd_brightness_spin.set_value(l.brightness as f64);
                }
                kbd_brightness_spin.set_sensitive(l.can_set);
                kbd_apply_button.set_sensitive(l.can_set);
                if l.can_set {
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
        if let Some(ref l) = lighting {
            lighting_backend.set_text(&format!("{} ({})", l.backend, l.device));
            lighting_current.set_text(&format!("{}/{}", l.brightness, l.max_brightness));
            lighting_mode.set_text(&l.mode);

            brightness_scale.set_range(0.0, l.max_brightness as f64);
            if !brightness_scale.is_focus() {
                brightness_scale.set_value(l.brightness as f64);
            }
            let can_set = l.can_set;
            brightness_scale.set_sensitive(can_set);
            mode_combo.set_sensitive(can_set);
            apply_lighting.set_sensitive(can_set);

            let mut modes = l.supported_modes.clone();
            if modes.is_empty() {
                modes = vec!["Off".to_string(), "Static".to_string()];
            }
            if !mode_combo.is_focus() {
                let mut last = last_supported_modes.borrow_mut();
                if *last != modes {
                    mode_combo.remove_all();
                    for m in &modes {
                        mode_combo.append(Some(m), m);
                    }
                    *last = modes.clone();
                }

                let desired = modes
                    .iter()
                    .find(|m| m.as_str().eq_ignore_ascii_case(&l.mode))
                    .cloned()
                    .or_else(|| modes.first().cloned())
                    .unwrap_or_else(|| "Static".to_string());
                mode_combo.set_active_id(Some(&desired));
            }

            rgb_button.set_sensitive(can_set && l.supports_rgb);
            rgb_row.set_subtitle(if l.supports_rgb {
                "Supported by backend"
            } else {
                "Requires Aura/asusd support"
            });
        } else {
            lighting_backend.set_text("(n/a)");
            lighting_current.set_text("(n/a)");
            lighting_mode.set_text("(n/a)");
            brightness_scale.set_sensitive(false);
            mode_combo.set_sensitive(false);
            apply_lighting.set_sensitive(false);
            rgb_button.set_sensitive(false);
            rgb_row.set_subtitle("Requires Aura/asusd support");
        }

        if let Some(msg) = lighting_error_txt {
            lighting_error.set_text(&msg);
        } else if lighting.is_some() {
            lighting_error.set_text("");
        } else {
            lighting_error.set_text("(n/a)");
        }

        glib::ControlFlow::Continue
    });
}

fn spawn_background(shared: Arc<Mutex<SharedUiState>>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                if let Ok(mut st) = shared.lock() {
                    st.daemon_error = Some(format!("failed to start tokio runtime: {e}"));
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

            loop {
                if shared.lock().map(|st| st.quit).unwrap_or(true) {
                    break;
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
                    if let Err(e) = apply_gpu_mode(mode).await {
                        if let Ok(mut st) = shared.lock() {
                            st.action_error = Some(e);
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
                    )) => {
                        let summary = summary_from_telemetry(&t);
                        if let Some(h) = &tray_handle {
                            let _ = h.update(|tray| tray.summary = summary.clone()).await;
                        }
                        if let Ok(mut st) = shared.lock() {
                            st.telemetry = Some(t);
                            st.cpu = cpu.clone();
                            st.cpu_caps = cpu_caps;
                            push_history(
                                &mut st.cpu_usage_history,
                                cpu.as_ref().and_then(|c| c.usage_percent),
                                600,
                            );
                            push_history(
                                &mut st.cpu_temp_history,
                                cpu.as_ref().and_then(|c| c.temp_c),
                                600,
                            );
                            push_history(
                                &mut st.cpu_power_history,
                                cpu.as_ref().and_then(|c| c.package_power_w),
                                600,
                            );
                            st.caps = caps;
                            st.caps_text = caps_text;
                            st.warnings = warnings;
                            st.profile = profile;
                            st.gpu_mode = gpu_mode;
                            st.battery_limit = battery_limit;
                            st.lighting = lighting;
                            st.daemon_error = None;
                        }
                    }
                    Err(e) => {
                        warn!("{e}");
                        if let Ok(mut st) = shared.lock() {
                            st.daemon_error = Some(e);
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

    let caps_map = state
        .get("caps")
        .cloned()
        .and_then(|v| HashMap::<String, OwnedValue>::try_from(v).ok())
        .unwrap_or_default();
    let telemetry_map = state
        .get("telemetry")
        .cloned()
        .and_then(|v| HashMap::<String, OwnedValue>::try_from(v).ok())
        .unwrap_or_default();
    let cpu_map = state
        .get("cpu")
        .cloned()
        .and_then(|v| HashMap::<String, OwnedValue>::try_from(v).ok())
        .unwrap_or_default();
    let cpu_caps_map = state
        .get("cpu_caps")
        .cloned()
        .and_then(|v| HashMap::<String, OwnedValue>::try_from(v).ok())
        .unwrap_or_default();
    let warnings = state
        .get("warnings")
        .cloned()
        .and_then(|v| Vec::<String>::try_from(v).ok())
        .unwrap_or_default();
    let profile = state
        .get("profile")
        .and_then(|v| <&str>::try_from(v).ok())
        .map(|v| v.to_string());
    let gpu_mode = state
        .get("gpu_mode")
        .and_then(|v| <&str>::try_from(v).ok())
        .map(|v| v.to_string());
    let battery_limit = state
        .get("battery_limit")
        .and_then(u64_from_value)
        .and_then(|v| u8::try_from(v).ok());
    let lighting = state
        .get("lighting")
        .cloned()
        .and_then(|v| HashMap::<String, OwnedValue>::try_from(v).ok())
        .and_then(lighting_from_dbus);

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
    ))
}

async fn apply_lighting(p: PendingLighting) -> Result<(), String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| format!("session DBus unavailable: {e}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|e| format!("failed to connect to daemon proxy: {e}"))?;

    let mut m = HashMap::new();
    m.insert("brightness".to_string(), OwnedValue::from(p.brightness));
    m.insert("mode".to_string(), ov(p.mode));
    if let Some(rgb) = p.rgb_hex {
        m.insert("rgb_hex".to_string(), ov(rgb));
    }
    proxy
        .set_lighting(m)
        .await
        .map_err(|e| format!("daemon SetLighting failed: {e}"))?;
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

fn ov<T>(v: T) -> OwnedValue
where
    T: Into<Value<'static>>,
{
    OwnedValue::try_from(v.into()).expect("OwnedValue conversion should succeed")
}

fn summary_from_telemetry(t: &TelemetrySnapshot) -> String {
    let mut parts = Vec::new();
    if let Some(p) = t.power_source {
        parts.push(format!("{p:?}"));
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
    if parts.is_empty() {
        "rog-helper".to_string()
    } else {
        parts.join(" | ")
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
    let ts = map
        .get("timestamp_ms")
        .and_then(|v| u64::try_from(v).ok())
        .unwrap_or(0);

    let mut t = TelemetrySnapshot::empty_now(ts);

    if let Some(v) = map.get("cpu_temp_c").and_then(|v| f64::try_from(v).ok()) {
        t.cpu_temp_c = Some(v as f32);
    }
    if let Some(v) = map.get("gpu_temp_c").and_then(|v| f64::try_from(v).ok()) {
        t.gpu_temp_c = Some(v as f32);
    }
    if let Some(temps) = map
        .get("temps_c")
        .cloned()
        .and_then(|v| HashMap::<String, f64>::try_from(v).ok())
    {
        for (k, v) in temps {
            t.temps_c.insert(k, v as f32);
        }
    }
    if let Some(fans) = map
        .get("fans_rpm")
        .cloned()
        .and_then(|v| HashMap::<String, u32>::try_from(v).ok())
    {
        for (k, v) in fans {
            t.fans_rpm.insert(k, v);
        }
    }
    if let Some(rows) = map
        .get("fan_rows")
        .cloned()
        .and_then(|v| Vec::<HashMap<String, OwnedValue>>::try_from(v).ok())
    {
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
    if let Some(v) = map
        .get("battery_percent")
        .and_then(|v| f64::try_from(v).ok())
    {
        t.battery_percent = Some(v as f32);
    }
    if let Some(v) = map.get("ac_online").and_then(|v| bool::try_from(v).ok()) {
        t.ac_online = Some(v);
    }
    if let Some(src) = map
        .get("power_source")
        .and_then(|v| <&str>::try_from(v).ok())
    {
        t.power_source = Some(match src {
            "Ac" => PowerSource::Ac,
            "Battery" => PowerSource::Battery,
            _ => PowerSource::Ac,
        });
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
        .get("per_core")
        .cloned()
        .and_then(|v| Vec::<HashMap<String, OwnedValue>>::try_from(v).ok())
    {
        let mut per_core = Vec::with_capacity(rows.len());
        for row in rows {
            let logical_cpu_id = row
                .get("logical_cpu_id")
                .or_else(|| row.get("core_id"))
                .and_then(u64_from_value)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or_default();
            per_core.push(rog_core::CpuCoreTelemetry {
                logical_cpu_id,
                physical_core_index: row
                    .get("physical_core_index")
                    .and_then(u64_from_value)
                    .and_then(|v| u32::try_from(v).ok()),
                policy_id: row
                    .get("policy_id")
                    .and_then(u64_from_value)
                    .and_then(|v| u32::try_from(v).ok()),
                thread_index: row
                    .get("thread_index")
                    .and_then(u64_from_value)
                    .and_then(|v| u32::try_from(v).ok()),
                thread_count: row
                    .get("thread_count")
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
    u64::try_from(v)
        .ok()
        .or_else(|| u32::try_from(v).ok().map(|v| v as u64))
}

fn format_battery_state(t: &TelemetrySnapshot) -> String {
    let Some(s) = t.battery_state else {
        return "(n/a)".to_string();
    };

    let label = match s {
        BatteryState::Unknown => "Unknown",
        BatteryState::Charging => "Charging",
        BatteryState::Discharging => "Discharging",
        BatteryState::Empty => "Empty",
        BatteryState::Full => "Full",
        BatteryState::PendingCharge => "Pending",
        BatteryState::PendingDischarge => "Pending",
        BatteryState::NotCharging => "Not charging",
    };

    match s {
        BatteryState::Charging | BatteryState::PendingCharge => {
            if let Some(secs) = t.battery_time_to_full_s {
                format!("{label} ({})", format_duration_short(secs))
            } else {
                label.to_string()
            }
        }
        BatteryState::Discharging | BatteryState::PendingDischarge => {
            if let Some(secs) = t.battery_time_to_empty_s {
                format!("{label} ({})", format_duration_short(secs))
            } else {
                label.to_string()
            }
        }
        _ => label.to_string(),
    }
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
    match status {
        FeatureAccessState::Available => "Available",
        FeatureAccessState::Unsupported => "Not supported on this machine",
        FeatureAccessState::MissingBackend => "Missing required service",
        FeatureAccessState::PermissionDenied => "Permission denied",
        FeatureAccessState::TemporarilyUnavailable => "Temporarily unavailable",
        FeatureAccessState::Unknown => "Checking support",
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
        .any(|control| control.status == CpuAccessState::PermissionDenied)
    {
        messages.push(
            "CPU telemetry is available, but CPU writes are blocked by sysfs permissions."
                .to_string(),
        );
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

fn support_banner_title(caps: &DeviceCaps, cpu_caps: &CpuCaps) -> String {
    let has_missing_backend = [
        caps.profile_access.status,
        caps.charge_limit_access.status,
        caps.gpu_mode_access.status,
        caps.kbd_backlight_access.status,
    ]
    .into_iter()
    .any(|status| status == FeatureAccessState::MissingBackend);
    let has_permission_denied = [
        caps.profile_access.status,
        caps.charge_limit_access.status,
        caps.gpu_mode_access.status,
        caps.kbd_backlight_access.status,
    ]
    .into_iter()
    .any(|status| status == FeatureAccessState::PermissionDenied)
        || cpu_caps
            .control_access
            .iter()
            .any(|control| control.status == CpuAccessState::PermissionDenied);
    let has_temporarily_unavailable = [
        caps.profile_access.status,
        caps.charge_limit_access.status,
        caps.gpu_mode_access.status,
        caps.kbd_backlight_access.status,
    ]
    .into_iter()
    .any(|status| status == FeatureAccessState::TemporarilyUnavailable)
        || cpu_caps
            .control_access
            .iter()
            .any(|control| control.status == CpuAccessState::TemporarilyUnavailable);
    let has_unsupported = [
        caps.profile_access.status,
        caps.charge_limit_access.status,
        caps.gpu_mode_access.status,
        caps.kbd_backlight_access.status,
    ]
    .into_iter()
    .any(|status| status == FeatureAccessState::Unsupported)
        || cpu_caps
            .control_access
            .iter()
            .any(|control| control.status == CpuAccessState::MissingBackend);

    let category_count = [
        has_missing_backend,
        has_permission_denied,
        has_temporarily_unavailable,
        has_unsupported,
    ]
    .into_iter()
    .filter(|value| *value)
    .count();

    if category_count > 1 {
        "Some controls need attention".to_string()
    } else if has_missing_backend {
        "Some controls need system services".to_string()
    } else if has_permission_denied {
        "Some controls are read-only".to_string()
    } else if has_temporarily_unavailable {
        "Some controls are temporarily unavailable".to_string()
    } else {
        "Some controls are unsupported on this machine".to_string()
    }
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
    "Ready".to_string()
}

fn gpu_switch_hint_text(caps: &DeviceCaps, warnings: &[String]) -> String {
    if let Some(detail) = warning_detail(warnings, "GPU switch hint: ") {
        detail.to_string()
    } else if !caps.gpu_mode_access.is_available() {
        caps.gpu_mode_access.reason.clone()
    } else if caps.requires_reboot_for_gpu_switch {
        "Switch may require reboot or logout.".to_string()
    } else {
        "Switch applies immediately or after logout.".to_string()
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
        ("Keyboard Backlight", &caps.kbd_backlight_access),
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
        .any(|control| control.status == CpuAccessState::PermissionDenied)
    {
        issues.push(
            "CPU Writes: permission_denied (CPU telemetry works, but sysfs control files are not writable by the current user.)"
                .to_string(),
        );
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
    if lower.contains("profile control requires asusd") {
        "Performance profiles require asusd. Install or start asusd, then retry.".to_string()
    } else if lower.contains("battery limit control requires asusd") {
        "Charge-limit control requires asusd. Install or start asusd, then retry.".to_string()
    } else if lower.contains("gpu mode control requires supergfxd") {
        "GPU mode switching requires supergfxd. Install or start supergfxd, then retry.".to_string()
    } else if lower.contains("permission denied") {
        "Permission denied. Open Diagnostics for the affected paths and backend details."
            .to_string()
    } else if lower.contains("session dbus unavailable")
        || lower.contains("failed to connect to daemon proxy")
    {
        "rog-helperd is not reachable on the session bus.".to_string()
    } else if let Some((_, suffix)) = error.split_once(": ") {
        suffix.to_string()
    } else {
        error.to_string()
    }
}

fn cpu_control_subtitle<'a>(
    caps: &'a CpuCaps,
    kind: CpuControlKind,
    available_text: &'a str,
    unsupported_text: &'a str,
) -> &'a str {
    match caps.control_state(kind) {
        CpuAccessState::Available => available_text,
        CpuAccessState::PermissionDenied => "Readable, but not writable by the current user",
        CpuAccessState::MissingBackend => "Required CPU policy backend is not available",
        CpuAccessState::TemporarilyUnavailable => "Temporarily unavailable; inspect diagnostics",
        CpuAccessState::Unsupported | CpuAccessState::Unknown => unsupported_text,
    }
}

fn cpu_has_access_issue(caps: &CpuCaps) -> bool {
    caps.control_access.iter().any(|control| {
        matches!(
            control.status,
            CpuAccessState::PermissionDenied
                | CpuAccessState::MissingBackend
                | CpuAccessState::TemporarilyUnavailable
        )
    })
}

fn cpu_banner_title(caps: &CpuCaps) -> String {
    if caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::PermissionDenied)
    {
        "CPU writes are blocked by sysfs permissions".to_string()
    } else if caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::MissingBackend)
    {
        "Some CPU controls are unavailable".to_string()
    } else if caps
        .control_access
        .iter()
        .any(|control| control.status == CpuAccessState::TemporarilyUnavailable)
    {
        "Some CPU controls are temporarily unavailable".to_string()
    } else {
        "Some CPU controls are unavailable".to_string()
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
        out.push_str(&format!(
            "{}: {}",
            control.kind.label(),
            control.status.as_str()
        ));
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
    out.push_str(
        "\nrog-helper does not bundle an automatic privilege escalation path for CPU sysfs writes. To make these controls writable in the app, grant the daemon write access only to the specific CPU sysfs files above, then restart the user daemon.\n",
    );

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

fn gpu_mode_to_dropdown_index(v: &str) -> u32 {
    let key = normalize_label(v);
    match key.as_str() {
        "integrated" => 0,
        "hybrid" => 1,
        "dedicated" | "discrete" | "asusmuxdgpu" | "asusegpu" | "vfio" => 2,
        _ => 1,
    }
}

fn pref_value_row(group: &adw::PreferencesGroup, title: &str, monospace: bool) -> gtk::Label {
    let row = adw::ActionRow::builder().title(title).build();
    let value = gtk::Label::new(Some("(n/a)"));
    value.set_xalign(1.0);
    value.set_halign(gtk::Align::End);
    if monospace {
        value.add_css_class("monospace");
    }
    row.add_suffix(&value);
    row.set_activatable(false);
    group.add(&row);
    value
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
    fn s(map: &HashMap<String, OwnedValue>, k: &str) -> Option<String> {
        map.get(k)
            .and_then(|v| <&str>::try_from(v).ok())
            .map(|v| v.to_string())
    }
    fn b(map: &HashMap<String, OwnedValue>, k: &str) -> Option<bool> {
        map.get(k).and_then(|v| bool::try_from(v).ok())
    }
    fn u(map: &HashMap<String, OwnedValue>, k: &str) -> Option<u64> {
        map.get(k).and_then(|v| {
            u64::try_from(v)
                .ok()
                .or_else(|| u32::try_from(v).ok().map(|v| v as u64))
        })
    }
    fn vec_string(map: &HashMap<String, OwnedValue>, k: &str) -> Option<Vec<String>> {
        map.get(k)
            .cloned()
            .and_then(|v| Vec::<String>::try_from(v).ok())
    }

    Some(LightingInfo {
        backend: s(&map, "backend").unwrap_or_else(|| "(n/a)".to_string()),
        device: s(&map, "device").unwrap_or_else(|| "(n/a)".to_string()),
        brightness: u(&map, "brightness").unwrap_or(0),
        max_brightness: u(&map, "max_brightness").unwrap_or(3),
        can_set: b(&map, "can_set").unwrap_or(false),
        mode: s(&map, "mode").unwrap_or_else(|| "(n/a)".to_string()),
        supports_rgb: b(&map, "supports_rgb").unwrap_or(false),
        supported_modes: vec_string(&map, "supported_modes").unwrap_or_default(),
    })
}

fn fan_row_from_dbus(map: &HashMap<String, OwnedValue>) -> Option<FanTelemetry> {
    let hwmon_device = map
        .get("hwmon_device")
        .and_then(|value| <&str>::try_from(value).ok())
        .map(|value| value.to_string())?;
    let hwmon_path = map
        .get("hwmon_path")
        .and_then(|value| <&str>::try_from(value).ok())
        .map(|value| value.to_string())?;
    let input_path = map
        .get("input_path")
        .and_then(|value| <&str>::try_from(value).ok())
        .map(|value| value.to_string())?;
    let display_label = map
        .get("display_label")
        .and_then(|value| <&str>::try_from(value).ok())
        .map(|value| value.to_string())?;

    Some(FanTelemetry {
        hwmon_device,
        hwmon_path,
        input_path,
        raw_label: map
            .get("raw_label")
            .and_then(|value| <&str>::try_from(value).ok())
            .map(|value| value.to_string()),
        display_label,
        rpm: map
            .get("rpm")
            .and_then(u64_from_value)
            .and_then(|value| u32::try_from(value).ok()),
    })
}

fn install_css() {
    let css = r#"
.metric-card { padding: 14px; border-radius: 12px; }
.status-chip {
  padding: 2px 8px;
  border-radius: 999px;
  background-color: alpha(currentColor, 0.12);
}
"#;

    let provider = gtk::CssProvider::new();
    provider.load_from_data(css);
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn clamped_scroller(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(980);
    clamp.set_tightening_threshold(720);
    clamp.set_child(Some(child));

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_vexpand(true);
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_child(Some(&clamp));
    scroller
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

fn cpu_path_access_from_dbus(map: &HashMap<String, OwnedValue>) -> Option<CpuPathAccess> {
    let path = map
        .get("path")
        .and_then(|value| <&str>::try_from(value).ok())
        .map(|value| value.to_string())?;
    let readable = map
        .get("readable")
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false);
    let writable = map
        .get("writable")
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
        .get("kind")
        .and_then(|value| <&str>::try_from(value).ok())
        .and_then(CpuControlKind::parse)?;
    let status = map
        .get("status")
        .and_then(|value| <&str>::try_from(value).ok())
        .map(CpuAccessState::parse)
        .unwrap_or(CpuAccessState::Unknown);
    let reason = map
        .get("reason")
        .and_then(|value| <&str>::try_from(value).ok())
        .unwrap_or_default()
        .to_string();
    let paths = map
        .get("paths")
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
        has_cpufreq: b(map, "has_cpufreq"),
        has_epp: b(map, "has_epp"),
        has_boost_toggle: b(map, "has_boost_toggle"),
        has_package_power: b(map, "has_package_power"),
        policy_writable: b(map, "policy_writable"),
        has_min_freq_limit: b(map, "has_min_freq_limit"),
        has_max_freq_limit: b(map, "has_max_freq_limit"),
        has_governor: b(map, "has_governor"),
        has_core_online: b(map, "has_core_online"),
        scaling_driver: s(map, "scaling_driver"),
        cpu_count: u32v(map, "cpu_count").unwrap_or(0),
        thread_count: u32v(map, "thread_count").unwrap_or(0),
        governor_choices: vec_string(map, "governor_choices"),
        epp_choices: vec_string(map, "epp_choices"),
        sysfs_paths: vec_string(map, "sysfs_paths"),
        control_access: map
            .get("control_access")
            .cloned()
            .and_then(|value| Vec::<HashMap<String, OwnedValue>>::try_from(value).ok())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|entry| cpu_control_access_from_dbus(&entry))
            .collect(),
    }
}

fn caps_from_dbus(map: &HashMap<String, OwnedValue>) -> DeviceCaps {
    fn b(map: &HashMap<String, OwnedValue>, key: &str) -> bool {
        map.get(key)
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false)
    }
    fn vec_string(map: &HashMap<String, OwnedValue>, key: &str) -> Vec<String> {
        map.get(key)
            .cloned()
            .and_then(|v| Vec::<String>::try_from(v).ok())
            .unwrap_or_default()
    }

    DeviceCaps {
        has_profiles: b(map, "has_profiles"),
        has_fan_curves: b(map, "has_fan_curves"),
        has_fan_reading: b(map, "has_fan_reading"),
        has_charge_limit: b(map, "has_charge_limit"),
        has_gpu_modes: b(map, "has_gpu_modes"),
        has_aura: b(map, "has_aura"),
        has_kbd_backlight: b(map, "has_kbd_backlight"),
        requires_reboot_for_gpu_switch: b(map, "requires_reboot_for_gpu_switch"),
        profile_access: feature_access_from_dbus(map, "profile_access"),
        charge_limit_access: feature_access_from_dbus(map, "charge_limit_access"),
        gpu_mode_access: feature_access_from_dbus(map, "gpu_mode_access"),
        kbd_backlight_access: feature_access_from_dbus(map, "kbd_backlight_access"),
        endpoints: vec_string(map, "endpoints"),
        notes: vec_string(map, "notes"),
    }
}

fn feature_access_from_dbus(
    map: &HashMap<String, OwnedValue>,
    prefix: &str,
) -> FeatureAvailability {
    let status_key = format!("{prefix}_status");
    let reason_key = format!("{prefix}_reason");

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
        ("has_charge_limit", b(&map, "has_charge_limit")),
        ("has_gpu_modes", b(&map, "has_gpu_modes")),
        ("has_aura", b(&map, "has_aura")),
        ("has_kbd_backlight", b(&map, "has_kbd_backlight")),
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

    lines.push("".to_string());
    lines.push("Feature access:".to_string());
    for (label, prefix) in [
        ("profile", "profile_access"),
        ("charge_limit", "charge_limit_access"),
        ("gpu_mode", "gpu_mode_access"),
        ("kbd_backlight", "kbd_backlight_access"),
    ] {
        let status = s(&map, &format!("{prefix}_status")).unwrap_or_else(|| "unknown".to_string());
        let reason = s(&map, &format!("{prefix}_reason")).unwrap_or_else(|| "(n/a)".to_string());
        lines.push(format!("  {label}: {status} ({reason})"));
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

fn text_or_unknown(value: &str) -> &str {
    if value.is_empty() {
        "(not provided by daemon)"
    } else {
        value
    }
}
