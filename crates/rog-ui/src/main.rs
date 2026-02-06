use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use ksni::menu::{MenuItem, StandardItem};
use ksni::Tray;
use rog_core::TelemetrySnapshot;
use tracing::{info, warn};
use zbus::zvariant::OwnedValue;

const DAEMON_DBUS_NAME: &str = "io.github.roghelper.Daemon";
const DAEMON_DBUS_PATH: &str = "/io/github/roghelper/Daemon";
const DAEMON_DBUS_IFACE: &str = "io.github.roghelper.Daemon1";

#[derive(Debug, Clone)]
enum UiMessage {
    Telemetry(TelemetrySnapshot),
    CapsText(String),
    DaemonError(String),
    ShowWindow,
    Quit,
}

#[zbus::proxy(
    interface = "io.github.roghelper.Daemon1",
    default_service = "io.github.roghelper.Daemon",
    default_path = "/io/github/roghelper/Daemon"
)]
trait Daemon1 {
    fn get_caps(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_telemetry(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
}

#[derive(Debug, Default)]
struct TrayState {
    last_summary: String,
}

struct RogTray {
    tx: glib::Sender<UiMessage>,
    state: Arc<Mutex<TrayState>>,
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

    fn tool_tip(&self) -> String {
        self.state
            .lock()
            .ok()
            .map(|s| s.last_summary.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "rog-helper".to_string())
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let summary = self
            .state
            .lock()
            .ok()
            .map(|s| s.last_summary.clone())
            .unwrap_or_default();

        vec![
            StandardItem {
                label: if summary.is_empty() {
                    "rog-helper".to_string()
                } else {
                    summary
                },
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Open Main Window".to_string(),
                activate: Box::new(|this: &mut RogTray| {
                    let _ = this.tx.send(UiMessage::ShowWindow);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Quit".to_string(),
                activate: Box::new(|this: &mut RogTray| {
                    let _ = this.tx.send(UiMessage::Quit);
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

    // Required for libadwaita styling.
    adw::init();

    let app = adw::Application::builder()
        .application_id("io.github.roghelper.UI")
        .build();

    app.connect_activate(build_ui);
    app.run();
    Ok(())
}

fn build_ui(app: &adw::Application) {
    let (tx, rx) = glib::MainContext::channel::<UiMessage>(glib::Priority::default());

    let tray_state = Arc::new(Mutex::new(TrayState::default()));
    spawn_tray(tx.clone(), tray_state.clone());
    spawn_telemetry_loop(tx.clone(), tray_state.clone());

    let cpu_label = gtk::Label::new(Some("CPU: (n/a)"));
    cpu_label.set_xalign(0.0);
    let gpu_label = gtk::Label::new(Some("GPU: (n/a)"));
    gpu_label.set_xalign(0.0);
    let batt_label = gtk::Label::new(Some("Battery: (n/a)"));
    batt_label.set_xalign(0.0);
    let power_label = gtk::Label::new(Some("Power: (n/a)"));
    power_label.set_xalign(0.0);
    let status_label = gtk::Label::new(Some(""));
    status_label.set_xalign(0.0);
    status_label.add_css_class("dim-label");

    let dash = gtk::Box::new(gtk::Orientation::Vertical, 12);
    dash.set_margin_top(16);
    dash.set_margin_bottom(16);
    dash.set_margin_start(16);
    dash.set_margin_end(16);
    dash.append(&cpu_label);
    dash.append(&gpu_label);
    dash.append(&batt_label);
    dash.append(&power_label);
    dash.append(&status_label);

    let diag_label = gtk::Label::new(Some("Loading diagnostics..."));
    diag_label.set_xalign(0.0);
    diag_label.set_wrap(true);
    let diag = gtk::Box::new(gtk::Orientation::Vertical, 12);
    diag.set_margin_top(16);
    diag.set_margin_bottom(16);
    diag.set_margin_start(16);
    diag.set_margin_end(16);
    diag.append(&diag_label);

    let stack = adw::ViewStack::new();
    stack.add_titled(&dash, Some("dashboard"), "Dashboard");
    stack.add_titled(&diag, Some("diagnostics"), "Diagnostics");

    let switcher = adw::ViewSwitcher::new();
    switcher.set_stack(Some(&stack));

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&switcher));

    let win = adw::ApplicationWindow::builder()
        .application(app)
        .title("rog-helper")
        .default_width(520)
        .default_height(360)
        .build();
    win.set_titlebar(Some(&header));
    win.set_content(Some(&stack));

    // Wire updates.
    let win_clone = win.clone();
    let app_clone = app.clone();
    rx.attach(None, move |msg| {
        match msg {
            UiMessage::Telemetry(t) => {
                let cpu = t
                    .cpu_temp_c
                    .map(|v| format!("{v:.1} C"))
                    .unwrap_or_else(|| "(n/a)".to_string());
                cpu_label.set_text(&format!("CPU: {cpu}"));

                let gpu = t
                    .gpu_temp_c
                    .map(|v| format!("{v:.1} C"))
                    .unwrap_or_else(|| "(n/a)".to_string());
                gpu_label.set_text(&format!("GPU: {gpu}"));

                let batt = t
                    .battery_percent
                    .map(|v| format!("{v:.0}%"))
                    .unwrap_or_else(|| "(n/a)".to_string());
                batt_label.set_text(&format!("Battery: {batt}"));

                let power = t
                    .power_source
                    .map(|p| format!("{p:?}"))
                    .unwrap_or_else(|| "(n/a)".to_string());
                power_label.set_text(&format!("Power: {power}"));

                status_label.set_text("");
            }
            UiMessage::CapsText(t) => {
                diag_label.set_text(&t);
            }
            UiMessage::DaemonError(e) => {
                status_label.set_text(&format!(
                    "Daemon not reachable on session DBus ({DAEMON_DBUS_NAME} {DAEMON_DBUS_PATH} {DAEMON_DBUS_IFACE}). {e}"
                ));
            }
            UiMessage::ShowWindow => {
                win_clone.present();
            }
            UiMessage::Quit => {
                app_clone.quit();
                return glib::ControlFlow::Break;
            }
        }
        glib::ControlFlow::Continue
    });

    win.present();
}

fn spawn_tray(tx: glib::Sender<UiMessage>, tray_state: Arc<Mutex<TrayState>>) {
    std::thread::spawn(move || {
        let tray = RogTray {
            tx,
            state: tray_state,
        };
        let service = ksni::TrayService::new(tray);
        if let Err(e) = service.run() {
            eprintln!("tray service error: {e:?}");
        }
    });
}

fn spawn_telemetry_loop(tx: glib::Sender<UiMessage>, tray_state: Arc<Mutex<TrayState>>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.send(UiMessage::DaemonError(format!(
                    "failed to start tokio runtime: {e}"
                )));
                return;
            }
        };

        rt.block_on(async move {
            loop {
                match fetch_telemetry().await {
                    Ok(t) => {
                        update_tray_state(&tray_state, &t);
                        let _ = tx.send(UiMessage::Telemetry(t));
                    }
                    Err(e) => {
                        warn!("{e}");
                        let _ = tx.send(UiMessage::DaemonError(e));
                    }
                }
                if let Ok(text) = fetch_caps_text().await {
                    let _ = tx.send(UiMessage::CapsText(text));
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    });
}

async fn fetch_telemetry() -> Result<TelemetrySnapshot, String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| format!("session DBus unavailable: {e}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|e| format!("failed to connect to daemon proxy: {e}"))?;
    let map = proxy
        .get_telemetry()
        .await
        .map_err(|e| format!("daemon GetTelemetry failed: {e}"))?;

    Ok(telemetry_from_dbus(map))
}

fn update_tray_state(state: &Arc<Mutex<TrayState>>, t: &TelemetrySnapshot) {
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

    let summary = if parts.is_empty() {
        "rog-helper".to_string()
    } else {
        parts.join(" | ")
    };

    if let Ok(mut guard) = state.lock() {
        guard.last_summary = summary;
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
            "Ac" => rog_core::PowerSource::Ac,
            "Battery" => rog_core::PowerSource::Battery,
            _ => rog_core::PowerSource::Ac,
        });
    }

    t
}

async fn fetch_caps_text() -> Result<String, String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| format!("session DBus unavailable: {e}"))?;
    let proxy = Daemon1Proxy::new(&conn)
        .await
        .map_err(|e| format!("failed to connect to daemon proxy: {e}"))?;
    let map = proxy
        .get_caps()
        .await
        .map_err(|e| format!("daemon GetCaps failed: {e}"))?;

    Ok(caps_text_from_dbus(map))
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
