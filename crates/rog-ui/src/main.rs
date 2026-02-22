use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use ksni::menu::{MenuItem, StandardItem};
use ksni::{ToolTip, Tray, TrayMethods};
use rog_core::{BatteryState, DeviceCaps, PowerSource, TelemetrySnapshot, TopProcessMem};
use tracing::{info, warn};
use zbus::zvariant::{OwnedValue, Value};

const DAEMON_DBUS_NAME: &str = "io.github.roghelper.Daemon";
const DAEMON_DBUS_PATH: &str = "/io/github/roghelper/Daemon";
const DAEMON_DBUS_IFACE: &str = "io.github.roghelper.Daemon1";

#[zbus::proxy(
    interface = "io.github.roghelper.Daemon1",
    default_service = "io.github.roghelper.Daemon",
    default_path = "/io/github/roghelper/Daemon"
)]
trait Daemon1 {
    fn get_caps(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_state(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_telemetry(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn set_lighting(&self, state: HashMap<String, OwnedValue>) -> zbus::Result<()>;
}

#[derive(Debug)]
struct SharedUiState {
    telemetry: Option<TelemetrySnapshot>,
    caps: DeviceCaps,
    caps_text: String,
    warnings: Vec<String>,
    lighting: Option<LightingInfo>,
    pending_lighting: Option<PendingLighting>,
    lighting_error: Option<String>,
    daemon_error: Option<String>,
    show_window: bool,
    quit: bool,
}

impl Default for SharedUiState {
    fn default() -> Self {
        Self {
            telemetry: None,
            caps: DeviceCaps::unknown(),
            caps_text: String::new(),
            warnings: Vec::new(),
            lighting: None,
            pending_lighting: None,
            lighting_error: None,
            daemon_error: None,
            show_window: false,
            quit: false,
        }
    }
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

    let warning_banner = adw::Banner::new("Keyboard backlight is read-only");
    warning_banner.set_button_label(Some("Fix permissions"));
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
        .subtitle("Not supported")
        .build();
    profile_row.add_suffix(&profile_buttons);
    profile_row.set_activatable(false);
    quick_actions_group.add(&profile_row);

    let gpu_mode_dropdown = gtk::DropDown::from_strings(&["Integrated", "Hybrid", "Discrete"]);
    gpu_mode_dropdown.set_sensitive(false);
    let gpu_mode_row = adw::ActionRow::builder()
        .title("GPU Mode")
        .subtitle("Unavailable")
        .build();
    gpu_mode_row.add_suffix(&gpu_mode_dropdown);
    gpu_mode_row.set_activatable(false);
    quick_actions_group.add(&gpu_mode_row);

    let charge_limit_spin = gtk::SpinButton::with_range(50.0, 100.0, 5.0);
    charge_limit_spin.set_value(80.0);
    charge_limit_spin.set_sensitive(false);
    let charge_limit_row = adw::ActionRow::builder()
        .title("Charge Limit")
        .subtitle("Unavailable")
        .build();
    charge_limit_row.add_suffix(&charge_limit_spin);
    charge_limit_row.set_activatable(false);
    quick_actions_group.add(&charge_limit_row);

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
        .subtitle("Unavailable")
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
    let detail_fan_rows = build_detail_rows(&details_fan_group, 6);
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
    stack.add_titled(&battery, Some("battery"), "Battery");
    stack.add_titled(&ram, Some("ram"), "RAM");
    stack.add_titled(&lighting, Some("lighting"), "Lighting");
    stack.add_titled(&diag, Some("diagnostics"), "Diagnostics");

    let switcher = adw::ViewSwitcher::new();
    switcher.set_stack(Some(&stack));

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&switcher));

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&stack));

    let win = adw::ApplicationWindow::builder()
        .application(app)
        .title("rog-helper")
        .default_width(560)
        .default_height(380)
        .build();
    win.set_content(Some(&view));
    win.present();

    {
        let win = win.clone();
        warning_banner.connect_button_clicked(move |_| {
            let dialog = adw::MessageDialog::new(
                Some(&win),
                Some("Fix keyboard backlight permissions"),
                Some(
                    "1. Install and start asusd/asusctl.\n2. Restart the user daemon.\n3. Or add a udev rule to grant write access to /sys/class/leds/asus::kbd_backlight/brightness.",
                ),
            );
            dialog.add_response("close", "Close");
            dialog.set_close_response("close");
            dialog.set_default_response(Some("close"));
            dialog.present();
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
    let last_supported_modes = std::rc::Rc::new(std::cell::RefCell::<Vec<String>>::new(Vec::new()));
    glib::timeout_add_local(Duration::from_millis(250), move || {
        let (
            telemetry,
            caps,
            caps_text,
            warnings,
            lighting,
            lighting_error_txt,
            daemon_error,
            show_window,
            quit,
        ) = {
            let mut st = match shared_clone.lock() {
                Ok(st) => st,
                Err(_) => return glib::ControlFlow::Continue,
            };
            let show_window = st.show_window;
            st.show_window = false;
            (
                st.telemetry.clone(),
                st.caps.clone(),
                st.caps_text.clone(),
                st.warnings.clone(),
                st.lighting.clone(),
                st.lighting_error.clone(),
                st.daemon_error.clone(),
                show_window,
                st.quit,
            )
        };

        if show_window {
            win_clone.present();
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

            if t.fans_rpm.is_empty() {
                fans_card.set_value("--");
                fans_card.set_unit(None);
                fans_card.set_subtitle(None);
            } else {
                let max_rpm = t.fans_rpm.values().copied().max().unwrap_or(0);
                fans_card.set_value(max_rpm.to_string());
                fans_card.set_unit(Some("rpm"));
                let count = t.fans_rpm.len();
                let fan_word = if count == 1 { "fan" } else { "fans" };
                fans_card.set_subtitle(Some(&format!("{count} {fan_word} detected")));
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

            let mut fan_entries: Vec<(String, String)> = t
                .fans_rpm
                .iter()
                .map(|(k, v)| (k.clone(), format!("{v} rpm")))
                .collect();
            fan_entries.sort_by(|a, b| a.0.cmp(&b.0));
            update_detail_rows(&detail_fan_rows, &fan_entries);
            details_fan_group.set_visible(!fan_entries.is_empty());
        }

        diag_buffer.set_text(if caps_text.is_empty() {
            "Loading diagnostics..."
        } else {
            &caps_text
        });

        if let Some(e) = daemon_error {
            status_label.set_text(&format!(
                "Daemon not reachable on session DBus ({DAEMON_DBUS_NAME} {DAEMON_DBUS_PATH} {DAEMON_DBUS_IFACE}). {e}"
            ));
        } else {
            status_label.set_text("");
        }

        let read_only_kbd = lighting.as_ref().map(|l| !l.can_set).unwrap_or(false)
            || warnings
                .iter()
                .any(|w| is_kbd_backlight_read_only_warning(w));

        if read_only_kbd {
            warning_banner.set_title("Keyboard backlight is read-only");
            warning_banner.set_button_label(Some("Fix permissions"));
            warning_subtitle
                .set_text("Install asusd/asusctl or add a udev rule to allow write access.");
            warning_subtitle.set_visible(true);
            warning_banner.set_revealed(true);
            warning_area.set_visible(true);
            open_diagnostics_button.set_visible(true);
        } else if !warnings.is_empty() {
            warning_banner.set_title("Hardware warnings detected");
            warning_banner.set_button_label(None);
            warning_subtitle.set_text(&warnings.join("\n"));
            warning_subtitle.set_visible(true);
            warning_banner.set_revealed(true);
            warning_area.set_visible(true);
            open_diagnostics_button.set_visible(true);
        } else {
            warning_banner.set_revealed(false);
            warning_subtitle.set_visible(false);
            warning_subtitle.set_text("");
            warning_area.set_visible(false);
            open_diagnostics_button.set_visible(false);
        }

        profile_quiet.set_sensitive(caps.has_profiles);
        profile_balanced.set_sensitive(caps.has_profiles);
        profile_turbo.set_sensitive(caps.has_profiles);
        if caps.has_profiles {
            profile_row.set_subtitle("Quiet / Balanced / Turbo");
        } else {
            profile_row.set_subtitle("Not supported");
        }

        gpu_mode_row.set_visible(caps.has_gpu_modes);
        if caps.has_gpu_modes {
            gpu_mode_dropdown.set_sensitive(false);
            if caps.requires_reboot_for_gpu_switch {
                gpu_mode_row.set_subtitle("Requires reboot for switch");
            } else {
                gpu_mode_row.set_subtitle("Control path pending daemon write API");
            }
        }

        charge_limit_row.set_visible(caps.has_charge_limit);
        if caps.has_charge_limit {
            charge_limit_spin.set_sensitive(false);
            charge_limit_row.set_subtitle("Control path pending daemon write API");
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
                    kbd_backlight_row.set_subtitle("Read-only for this user");
                }
            } else {
                kbd_brightness_spin.set_sensitive(false);
                kbd_apply_button.set_sensitive(false);
                kbd_backlight_row.set_subtitle("Unavailable");
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
                    Ok((t, caps, caps_text, warnings, lighting)) => {
                        let summary = summary_from_telemetry(&t);
                        if let Some(h) = &tray_handle {
                            let _ = h.update(|tray| tray.summary = summary.clone()).await;
                        }
                        if let Ok(mut st) = shared.lock() {
                            st.telemetry = Some(t);
                            st.caps = caps;
                            st.caps_text = caps_text;
                            st.warnings = warnings;
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
        DeviceCaps,
        String,
        Vec<String>,
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
    let warnings = state
        .get("warnings")
        .cloned()
        .and_then(|v| Vec::<String>::try_from(v).ok())
        .unwrap_or_default();
    let lighting = state
        .get("lighting")
        .cloned()
        .and_then(|v| HashMap::<String, OwnedValue>::try_from(v).ok())
        .and_then(lighting_from_dbus);

    let caps = caps_from_dbus(&caps_map);
    let mut caps_text = caps_text_from_dbus(caps_map);
    if !warnings.is_empty() {
        caps_text.push_str("\n\nWarnings:");
        for w in &warnings {
            caps_text.push_str(&format!("\n  {w}"));
        }
    }

    Ok((
        telemetry_from_dbus(telemetry_map),
        caps,
        caps_text,
        warnings,
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
        let row = adw::ActionRow::builder().title("").build();
        let value = gtk::Label::new(Some(""));
        value.set_xalign(1.0);
        value.set_halign(gtk::Align::End);
        value.add_css_class("monospace");
        row.add_suffix(&value);
        row.set_activatable(false);
        row.set_visible(false);
        group.add(&row);
        rows.push(DetailRow {
            row,
            value_label: value,
        });
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

fn is_kbd_backlight_read_only_warning(warning: &str) -> bool {
    let lower = warning.to_ascii_lowercase();
    lower.contains("kbd_backlight")
        || lower.contains("keyboard backlight")
        || (lower.contains("read-only") && lower.contains("backlight"))
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
        endpoints: vec_string(map, "endpoints"),
        notes: vec_string(map, "notes"),
    }
}

fn caps_text_from_dbus(map: HashMap<String, OwnedValue>) -> String {
    fn b(map: &HashMap<String, OwnedValue>, k: &str) -> Option<bool> {
        map.get(k).and_then(|v| bool::try_from(v).ok())
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
