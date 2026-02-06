use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use ksni::menu::{MenuItem, StandardItem};
use ksni::{ToolTip, Tray, TrayMethods};
use rog_core::{PowerSource, TelemetrySnapshot};
use tracing::{info, warn};
use zbus::zvariant::OwnedValue;

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
}

#[derive(Debug, Default)]
struct SharedUiState {
    telemetry: Option<TelemetrySnapshot>,
    caps_text: String,
    warnings: Vec<String>,
    daemon_error: Option<String>,
    show_window: bool,
    quit: bool,
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

    // Small amount of app-specific CSS so the Dashboard reads like a dashboard instead of a list
    // of labels.
    install_css();

    let (cpu_card, cpu_value) = metric_card("CPU Temperature");
    let (gpu_card, gpu_value) = metric_card("GPU Temperature");
    let (batt_card, batt_value) = metric_card("Battery");
    let (power_card, power_value) = metric_card("Power Source");

    let tiles = gtk::FlowBox::new();
    tiles.set_selection_mode(gtk::SelectionMode::None);
    tiles.set_min_children_per_line(1);
    tiles.set_max_children_per_line(2);
    tiles.set_row_spacing(12);
    tiles.set_column_spacing(12);
    tiles.set_halign(gtk::Align::Fill);
    tiles.insert(&cpu_card, -1);
    tiles.insert(&gpu_card, -1);
    tiles.insert(&batt_card, -1);
    tiles.insert(&power_card, -1);

    let fans_label = gtk::Label::new(Some(""));
    fans_label.set_xalign(0.0);
    fans_label.set_wrap(true);
    fans_label.add_css_class("dim-label");

    let warnings_label = gtk::Label::new(Some(""));
    warnings_label.set_xalign(0.0);
    warnings_label.set_wrap(true);
    warnings_label.add_css_class("dim-label");
    warnings_label.set_visible(false);

    let sensors_details = gtk::Label::new(Some(""));
    sensors_details.set_xalign(0.0);
    sensors_details.set_wrap(true);
    sensors_details.set_selectable(true);
    sensors_details.add_css_class("monospace");

    let sensors_expander = gtk::Expander::new(Some("Sensors"));
    sensors_expander.set_expanded(false);
    sensors_expander.set_child(Some(&sensors_details));
    sensors_expander.set_visible(false);

    let status_label = gtk::Label::new(Some(""));
    status_label.set_xalign(0.0);
    status_label.set_wrap(true);
    status_label.add_css_class("dim-label");

    let dash_root = gtk::Box::new(gtk::Orientation::Vertical, 18);
    dash_root.set_margin_top(16);
    dash_root.set_margin_bottom(16);
    dash_root.set_margin_start(16);
    dash_root.set_margin_end(16);
    dash_root.append(&tiles);
    dash_root.append(&fans_label);
    dash_root.append(&warnings_label);
    dash_root.append(&sensors_expander);
    dash_root.append(&status_label);

    let dash = adw::ClampScrollable::new();
    dash.set_child(Some(&dash_root));

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
    diag_root.set_margin_top(16);
    diag_root.set_margin_bottom(16);
    diag_root.set_margin_start(16);
    diag_root.set_margin_end(16);
    diag_root.append(&copy_diag_button);
    diag_root.append(&diag_scroller);

    let diag = adw::ClampScrollable::new();
    diag.set_child(Some(&diag_root));

    let stack = adw::ViewStack::new();
    stack.add_titled(&dash, Some("dashboard"), "Dashboard");
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

    // Periodically refresh UI from the background thread state.
    let shared_clone = shared.clone();
    let win_clone = win.clone();
    let app_clone = app.clone();
    glib::timeout_add_local(Duration::from_millis(250), move || {
        let (telemetry, caps_text, warnings, daemon_error, show_window, quit) = {
            let mut st = match shared_clone.lock() {
                Ok(st) => st,
                Err(_) => return glib::ControlFlow::Continue,
            };
            let show_window = st.show_window;
            st.show_window = false;
            (
                st.telemetry.clone(),
                st.caps_text.clone(),
                st.warnings.clone(),
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
            cpu_value.set_text(
                &t.cpu_temp_c
                    .map(|v| format!("{v:.1} C"))
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            gpu_value.set_text(
                &t.gpu_temp_c
                    .map(|v| format!("{v:.1} C"))
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            batt_value.set_text(
                &t.battery_percent
                    .map(|v| format!("{v:.0}%"))
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            power_value.set_text(
                &t.power_source
                    .map(|p| match p {
                        PowerSource::Ac => "AC".to_string(),
                        PowerSource::Battery => "Battery".to_string(),
                    })
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );

            let fans_summary = format_fans_summary(&t);
            fans_label.set_visible(!fans_summary.is_empty());
            fans_label.set_text(&fans_summary);

            let sensors_text = format_sensors_details(&t);
            sensors_expander.set_visible(!sensors_text.is_empty());
            sensors_details.set_text(&sensors_text);
        }

        diag_buffer.set_text(if caps_text.is_empty() {
            "Loading diagnostics..."
        } else {
            &caps_text
        });

        if let Some(e) = daemon_error {
            warnings_label.set_visible(false);
            status_label.set_text(&format!(
                "Daemon not reachable on session DBus ({DAEMON_DBUS_NAME} {DAEMON_DBUS_PATH} {DAEMON_DBUS_IFACE}). {e}"
            ));
        } else {
            status_label.set_text("");
            let warn_text = format_warnings(&warnings);
            warnings_label.set_visible(!warn_text.is_empty());
            warnings_label.set_text(&warn_text);
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

                match fetch_state().await {
                    Ok((t, caps_text, warnings)) => {
                        let summary = summary_from_telemetry(&t);
                        if let Some(h) = &tray_handle {
                            let _ = h.update(|tray| tray.summary = summary.clone()).await;
                        }
                        if let Ok(mut st) = shared.lock() {
                            st.telemetry = Some(t);
                            st.caps_text = caps_text;
                            st.warnings = warnings;
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

async fn fetch_state() -> Result<(TelemetrySnapshot, String, Vec<String>), String> {
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

    let mut caps_text = caps_text_from_dbus(caps_map);
    if !warnings.is_empty() {
        caps_text.push_str("\n\nWarnings:");
        for w in &warnings {
            caps_text.push_str(&format!("\n  {w}"));
        }
    }

    Ok((telemetry_from_dbus(telemetry_map), caps_text, warnings))
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

    t
}

fn metric_card(title: &str) -> (gtk::Box, gtk::Label) {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.add_css_class("card");
    card.add_css_class("metric-card");
    card.set_hexpand(true);
    card.set_size_request(220, -1);

    let title_label = gtk::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.add_css_class("metric-title");

    let value = gtk::Label::new(Some("(n/a)"));
    value.set_xalign(0.0);
    value.add_css_class("metric-value");

    card.append(&title_label);
    card.append(&value);
    (card, value)
}

fn install_css() {
    let css = r#"
.metric-card { padding: 12px; }
.metric-title { opacity: 0.85; }
.metric-value { font-weight: 700; font-size: 26px; }
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

fn format_fans_summary(t: &TelemetrySnapshot) -> String {
    if t.fans_rpm.is_empty() {
        return String::new();
    }

    let mut parts = Vec::new();
    for (k, v) in &t.fans_rpm {
        parts.push(format!("{k}: {v} rpm"));
    }
    format!("Fans: {}", parts.join(" | "))
}

fn format_sensors_details(t: &TelemetrySnapshot) -> String {
    if t.temps_c.is_empty() && t.fans_rpm.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    if !t.temps_c.is_empty() {
        lines.push("Temperatures".to_string());
        for (k, v) in &t.temps_c {
            lines.push(format!("  {k}: {v:.1} C"));
        }
    }
    if !t.fans_rpm.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push("Fans".to_string());
        for (k, v) in &t.fans_rpm {
            lines.push(format!("  {k}: {v} rpm"));
        }
    }
    lines.join("\n")
}

fn format_warnings(warnings: &[String]) -> String {
    if warnings.is_empty() {
        return String::new();
    }
    let mut out = String::from("Warnings:\n");
    for (idx, w) in warnings.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        out.push_str("  - ");
        out.push_str(w);
    }
    out
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
