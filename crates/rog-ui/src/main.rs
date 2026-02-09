use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use ksni::menu::{MenuItem, StandardItem};
use ksni::{ToolTip, Tray, TrayMethods};
use rog_core::{BatteryState, PowerSource, TelemetrySnapshot};
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

#[derive(Debug, Default)]
struct SharedUiState {
    telemetry: Option<TelemetrySnapshot>,
    caps_text: String,
    warnings: Vec<String>,
    lighting: Option<LightingInfo>,
    pending_lighting: Option<PendingLighting>,
    lighting_error: Option<String>,
    daemon_error: Option<String>,
    show_window: bool,
    quit: bool,
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
    let (batt_state_card, batt_state_value) = metric_card("Charging Status");
    let (batt_health_card, batt_health_value) = metric_card("Battery Health");
    let (batt_cycles_card, batt_cycles_value) = metric_card("Cycle Count");
    let (pwr_in_card, pwr_in_value) = metric_card("Power Input");
    let (pwr_draw_card, pwr_draw_value) = metric_card("Power Draw");
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
    tiles.insert(&batt_state_card, -1);
    tiles.insert(&batt_health_card, -1);
    tiles.insert(&batt_cycles_card, -1);
    tiles.insert(&pwr_in_card, -1);
    tiles.insert(&pwr_draw_card, -1);
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

    let dash = clamped_scroller(&dash_root);

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

    let diag = adw::Clamp::new();
    diag.set_child(Some(&diag_root));

    // Lighting page (Milestone 1: keyboard backlight brightness via sysfs when available).
    let lighting_title = gtk::Label::new(Some("Keyboard Lighting"));
    lighting_title.set_xalign(0.0);
    lighting_title.add_css_class("title-3");

    let lighting_backend = gtk::Label::new(Some("Backend: (n/a)"));
    lighting_backend.set_xalign(0.0);
    lighting_backend.set_wrap(true);
    lighting_backend.add_css_class("dim-label");

    let lighting_current = gtk::Label::new(Some("Brightness: (n/a)"));
    lighting_current.set_xalign(0.0);
    lighting_current.add_css_class("dim-label");

    let lighting_mode = gtk::Label::new(Some("Mode: (n/a)"));
    lighting_mode.set_xalign(0.0);
    lighting_mode.add_css_class("dim-label");

    let mode_combo = gtk::ComboBoxText::new();
    mode_combo.set_sensitive(false);

    let brightness_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 3.0, 1.0);
    brightness_scale.set_hexpand(true);
    brightness_scale.set_draw_value(true);
    brightness_scale.set_value_pos(gtk::PositionType::Right);

    let rgb_label = gtk::Label::new(Some("RGB Color"));
    rgb_label.set_xalign(0.0);
    rgb_label.add_css_class("title-4");

    let rgb_button = gtk::ColorButton::new();
    rgb_button.set_use_alpha(false);
    rgb_button.set_title("Keyboard RGB");
    rgb_button.set_sensitive(false);

    let rgb_hint = gtk::Label::new(Some(
        "RGB/effects require asusd/Aura support. This device currently exposes keyboard brightness only.",
    ));
    rgb_hint.set_xalign(0.0);
    rgb_hint.set_wrap(true);
    rgb_hint.add_css_class("dim-label");

    let apply_lighting = gtk::Button::with_label("Apply lighting");
    apply_lighting.add_css_class("suggested-action");

    let lighting_error = gtk::Label::new(None);
    lighting_error.set_xalign(0.0);
    lighting_error.set_wrap(true);
    lighting_error.add_css_class("dim-label");
    lighting_error.set_visible(false);

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

    let lighting_root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    lighting_root.set_margin_top(16);
    lighting_root.set_margin_bottom(16);
    lighting_root.set_margin_start(16);
    lighting_root.set_margin_end(16);
    lighting_root.append(&lighting_title);
    lighting_root.append(&lighting_backend);
    lighting_root.append(&lighting_current);
    lighting_root.append(&lighting_mode);
    lighting_root.append(&mode_combo);
    lighting_root.append(&brightness_scale);
    lighting_root.append(&rgb_label);
    lighting_root.append(&rgb_button);
    lighting_root.append(&rgb_hint);
    lighting_root.append(&apply_lighting);
    lighting_root.append(&lighting_error);

    let lighting = clamped_scroller(&lighting_root);

    let stack = adw::ViewStack::new();
    stack.add_titled(&dash, Some("dashboard"), "Dashboard");
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

    // Periodically refresh UI from the background thread state.
    let shared_clone = shared.clone();
    let win_clone = win.clone();
    let app_clone = app.clone();
    let last_supported_modes = std::rc::Rc::new(std::cell::RefCell::<Vec<String>>::new(Vec::new()));
    glib::timeout_add_local(Duration::from_millis(250), move || {
        let (
            telemetry,
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
            batt_state_value.set_text(&format_battery_state(&t));
            batt_health_value.set_text(
                &t.battery_health_percent
                    .map(|v| format!("{v:.0}%"))
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            batt_cycles_value.set_text(
                &t.battery_cycle_count
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            pwr_in_value.set_text(
                &t.battery_charge_power_w
                    .map(|v| format!("{v:.1} W"))
                    .unwrap_or_else(|| "(n/a)".to_string()),
            );
            pwr_draw_value.set_text(
                &t.battery_discharge_power_w
                    .map(|v| format!("{v:.1} W"))
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

        // Lighting page (capability-driven).
        if let Some(ref l) = lighting {
            lighting_backend.set_text(&format!("Backend: {} ({})", l.backend, l.device));
            lighting_current.set_text(&format!(
                "Brightness: {}/{}",
                l.brightness, l.max_brightness
            ));
            lighting_mode.set_text(&format!("Mode: {}", l.mode));

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
            rgb_hint.set_visible(!l.supports_rgb);

            if !can_set && lighting_error_txt.is_none() {
                lighting_error.set_visible(true);
                lighting_error.set_text(
                    "Keyboard backlight is read-only for this user. Install asusd/asusctl or add a udev rule to allow writing the LED brightness.",
                );
            }
        } else {
            lighting_backend.set_text("Backend: (n/a)");
            lighting_current.set_text("Brightness: (n/a)");
            lighting_mode.set_text("Mode: (n/a)");
            brightness_scale.set_sensitive(false);
            mode_combo.set_sensitive(false);
            apply_lighting.set_sensitive(false);
            rgb_button.set_sensitive(false);
            rgb_hint.set_visible(true);
        }

        if let Some(msg) = lighting_error_txt {
            lighting_error.set_visible(true);
            lighting_error.set_text(&msg);
        } else if lighting.is_some() {
            // keep whatever message we set above for read-only hint
        } else {
            lighting_error.set_visible(false);
            lighting_error.set_text("");
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
                    Ok((t, caps_text, warnings, lighting)) => {
                        let summary = summary_from_telemetry(&t);
                        if let Some(h) = &tray_handle {
                            let _ = h.update(|tray| tray.summary = summary.clone()).await;
                        }
                        if let Ok(mut st) = shared.lock() {
                            st.telemetry = Some(t);
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

async fn fetch_state(
) -> Result<(TelemetrySnapshot, String, Vec<String>, Option<LightingInfo>), String> {
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

    let mut caps_text = caps_text_from_dbus(caps_map);
    if !warnings.is_empty() {
        caps_text.push_str("\n\nWarnings:");
        for w in &warnings {
            caps_text.push_str(&format!("\n  {w}"));
        }
    }

    Ok((
        telemetry_from_dbus(telemetry_map),
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

fn clamped_scroller(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let clamp = adw::Clamp::new();
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
