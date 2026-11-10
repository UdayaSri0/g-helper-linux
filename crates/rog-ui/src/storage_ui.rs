use adw::prelude::*;
use gtk4 as gtk;
use rog_core::dbus_keys::storage as key;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::dbus_decode::{self, WireMap};
use crate::widgets::{page_container, page_header, HistoryGraph, MetricCard};

#[derive(Clone, Debug, Default)]
pub(crate) struct StorageSnapshot {
    pub status: String,
    pub sampled_at_ms: Option<u64>,
    pub drives: Vec<StorageDrive>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct StorageDrive {
    pub id: String,
    pub name: String,
    pub model: String,
    pub manufacturer: Option<String>,
    pub kind: String,
    pub transport: String,
    pub capacity_bytes: Option<u64>,
    pub temperature_c: Option<f64>,
    pub temperature_sampled_at_ms: Option<u64>,
    pub health: String,
    pub read_bytes_per_sec: Option<u64>,
    pub write_bytes_per_sec: Option<u64>,
    pub read_iops: Option<u64>,
    pub write_iops: Option<u64>,
    pub volumes: Vec<WireMap>,
    pub temperatures: Vec<WireMap>,
    pub health_reason: String,
    pub health_source: String,
    pub health_attributes: Vec<WireMap>,
    pub firmware: Option<String>,
    pub logical_block_size: Option<u64>,
    pub physical_block_size: Option<u64>,
    pub history: Vec<WireMap>,
}

pub(crate) fn decode(map: WireMap) -> StorageSnapshot {
    let drives = dbus_decode::rows(&map, key::DRIVES)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|drive| {
            let id = dbus_decode::string(&drive, key::ID)?;
            if id.is_empty() {
                return None;
            }
            let io = dbus_decode::nested_map(&drive, key::IO).unwrap_or_default();
            let health = dbus_decode::nested_map(&drive, key::HEALTH).unwrap_or_default();
            let temperatures = dbus_decode::rows(&drive, key::TEMPERATURES).unwrap_or_default();
            let primary_temperature = temperatures
                .iter()
                .find(|sensor| dbus_decode::float(sensor, "celsius").is_some());
            Some(StorageDrive {
                id,
                name: dbus_decode::string(&drive, key::NAME).unwrap_or_default(),
                model: dbus_decode::string(&drive, key::MODEL).unwrap_or_default(),
                manufacturer: dbus_decode::string(&drive, key::MANUFACTURER),
                kind: dbus_decode::string(&drive, key::KIND).unwrap_or_default(),
                transport: dbus_decode::string(&drive, key::TRANSPORT).unwrap_or_default(),
                capacity_bytes: dbus_decode::unsigned(&drive, key::CAPACITY_BYTES),
                temperature_c: primary_temperature
                    .and_then(|sensor| dbus_decode::float(sensor, "celsius")),
                temperature_sampled_at_ms: primary_temperature
                    .and_then(|sensor| dbus_decode::unsigned(sensor, "sampled_at_ms")),
                health: dbus_decode::string(&health, "state")
                    .unwrap_or_else(|| "Unknown".to_string()),
                read_bytes_per_sec: dbus_decode::unsigned(&io, "read_bytes_per_sec"),
                write_bytes_per_sec: dbus_decode::unsigned(&io, "write_bytes_per_sec"),
                read_iops: dbus_decode::unsigned(&io, "read_iops"),
                write_iops: dbus_decode::unsigned(&io, "write_iops"),
                volumes: dbus_decode::rows(&drive, key::VOLUMES).unwrap_or_default(),
                temperatures,
                health_reason: dbus_decode::string(&health, "reason").unwrap_or_default(),
                health_source: dbus_decode::string(&health, "source").unwrap_or_default(),
                health_attributes: dbus_decode::rows(&health, "attributes").unwrap_or_default(),
                firmware: dbus_decode::string(&drive, key::FIRMWARE),
                logical_block_size: dbus_decode::unsigned(&drive, key::LOGICAL_BLOCK_SIZE),
                physical_block_size: dbus_decode::unsigned(&drive, key::PHYSICAL_BLOCK_SIZE),
                history: dbus_decode::rows(&drive, key::HISTORY).unwrap_or_default(),
            })
        })
        .collect();
    StorageSnapshot {
        status: dbus_decode::string(&map, key::STATUS).unwrap_or_else(|| "Unavailable".to_string()),
        sampled_at_ms: dbus_decode::unsigned(&map, key::SAMPLED_AT_MS),
        drives,
    }
}

pub(crate) fn byte_size(value: u64) -> String {
    const UNITS: &[&str] = &["B", "kB", "MB", "GB", "TB", "PB"];
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1000.0 && unit + 1 < UNITS.len() {
        size /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

pub(crate) fn dashboard_summary(snapshot: Option<&StorageSnapshot>) -> (String, String) {
    let Some(snapshot) = snapshot else {
        return ("—".into(), "Waiting for storage service".into());
    };
    if !snapshot.status.eq_ignore_ascii_case("available") {
        return ("—".into(), snapshot.status.clone());
    }
    let count = snapshot.drives.len();
    let warnings = snapshot
        .drives
        .iter()
        .filter(|drive| {
            matches!(
                drive.health.to_ascii_lowercase().as_str(),
                "attention" | "critical"
            )
        })
        .count();
    let subtitle = if warnings > 0 {
        format!("{warnings} drive health warning(s)")
    } else {
        "Open storage details".to_string()
    };
    (count.to_string(), subtitle)
}

pub(crate) struct StoragePage {
    root: gtk::ScrolledWindow,
    summary: [MetricCard; 4],
    status: gtk::Label,
    drive_list: gtk::Box,
    drive_rows: RefCell<HashMap<String, DriveRow>>,
    details_title: gtk::Label,
    details_meta: gtk::Label,
    details_health: gtk::Label,
    details_temp: gtk::Label,
    overview: gtk::Label,
    health: gtk::Label,
    volumes: gtk::Box,
    volume_rows: RefCell<HashMap<String, VolumeWidgets>>,
    advanced: gtk::Label,
    tabs: gtk::Stack,
    active_tab: Rc<RefCell<String>>,
    selected_id: Rc<RefCell<Option<String>>>,
    latest: Rc<RefCell<Option<StorageSnapshot>>>,
    copy_now: Rc<Cell<u64>>,
    read_graph: HistoryGraph,
    write_graph: HistoryGraph,
    temperature_graph: HistoryGraph,
    request_render: Rc<dyn Fn()>,
}

struct DriveRow {
    button: gtk::Button,
    title: gtk::Label,
    subtitle: gtk::Label,
    health: gtk::Label,
}

struct VolumeWidgets {
    root: gtk::Box,
    label: gtk::Label,
    usage: gtk::ProgressBar,
}

impl StoragePage {
    pub(crate) fn new(
        request_diagnostics: impl Fn() + 'static,
        request_render: impl Fn() + 'static,
    ) -> Self {
        let request_diagnostics = Rc::new(request_diagnostics);
        let request_render = Rc::new(request_render);
        let body = page_container();
        body.append(&page_header(
            "Storage",
            "Physical drives, live I/O, volumes, and available health data.",
        ));
        let summary = [
            MetricCard::new("Physical drives"),
            MetricCard::new("Total capacity"),
            MetricCard::new("Storage health"),
            MetricCard::new("Live activity"),
        ];
        let summary_grid = gtk::FlowBox::new();
        summary_grid.set_selection_mode(gtk::SelectionMode::None);
        summary_grid.set_max_children_per_line(4);
        summary_grid.set_min_children_per_line(1);
        summary_grid.set_column_spacing(12);
        summary_grid.set_row_spacing(12);
        for (card, icon) in summary.iter().zip([
            "rog-storage-symbolic",
            "drive-harddisk-symbolic",
            "emblem-ok-symbolic",
            "utilities-system-monitor-symbolic",
        ]) {
            card.set_icon_name(icon);
            summary_grid.insert(card.widget(), -1);
        }
        body.append(&summary_grid);
        let status = gtk::Label::new(Some("Detecting physical drives…"));
        status.set_xalign(0.0);
        status.set_wrap(true);
        body.append(&status);
        let columns = gtk::Paned::new(gtk::Orientation::Horizontal);
        columns.set_wide_handle(true);
        columns.set_position(340);
        let drive_list = gtk::Box::new(gtk::Orientation::Vertical, 8);
        drive_list.set_size_request(240, -1);
        columns.set_start_child(Some(&drive_list));
        let detail = gtk::Box::new(gtk::Orientation::Vertical, 12);
        detail.set_margin_start(16);
        columns.set_end_child(Some(&detail));
        columns.set_resize_start_child(false);
        columns.set_shrink_start_child(true);
        body.append(&columns);
        let root = gtk::ScrolledWindow::new();
        root.set_vexpand(true);
        root.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(1260);
        clamp.set_tightening_threshold(900);
        clamp.set_child(Some(&body));
        root.set_child(Some(&clamp));
        let responsive_columns = columns.clone();
        root.connect_notify_local(Some("width"), move |root, _| {
            let narrow = root.width() < 900;
            responsive_columns.set_orientation(if narrow {
                gtk::Orientation::Vertical
            } else {
                gtk::Orientation::Horizontal
            });
            responsive_columns.set_position(if narrow { 180 } else { 340 });
        });
        let details_title = gtk::Label::new(Some("Select a drive"));
        details_title.set_xalign(0.0);
        details_title.add_css_class("page-title");
        detail.append(&details_title);
        let details_meta =
            gtk::Label::new(Some("Drive performance, capacity, temperature and health"));
        details_meta.set_xalign(0.0);
        detail.append(&details_meta);
        let metric_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let details_health = gtk::Label::new(Some("Health unavailable"));
        let details_temp = gtk::Label::new(Some("Temperature unavailable"));
        details_health.add_css_class("status-chip");
        details_temp.add_css_class("status-chip");
        metric_row.append(&details_health);
        metric_row.append(&details_temp);
        detail.append(&metric_row);
        let tabs = gtk::Stack::new();
        tabs.set_transition_type(gtk::StackTransitionType::None);
        let overview = gtk::Label::new(Some("Select a physical drive to view its details."));
        overview.set_xalign(0.0);
        overview.set_wrap(true);
        let performance = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let read_graph = HistoryGraph::new("Read throughput", 0.0, 1.0, "MB/s", (0.30, 0.64, 1.0));
        let write_graph =
            HistoryGraph::new("Write throughput", 0.0, 1.0, "MB/s", (0.49, 0.88, 0.70));
        let temperature_graph =
            HistoryGraph::new("Drive temperature", 0.0, 50.0, "°C", (0.96, 0.68, 0.38));
        performance.append(read_graph.widget());
        performance.append(write_graph.widget());
        performance.append(temperature_graph.widget());
        let health = gtk::Label::new(Some("SMART details are not available."));
        health.set_xalign(0.0);
        health.set_wrap(true);
        health.set_selectable(true);
        let volumes = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let volume_placeholder = gtk::Label::new(Some("No attributable volumes reported."));
        volume_placeholder.set_xalign(0.0);
        volume_placeholder.add_css_class("volume-placeholder");
        volumes.append(&volume_placeholder);
        tabs.add_titled(&overview, Some("Overview"), "Overview");
        tabs.add_titled(&performance, Some("Performance"), "Performance");
        tabs.add_titled(&health, Some("Health & SMART"), "Health & SMART");
        tabs.add_titled(
            &volumes,
            Some("Partitions & Volumes"),
            "Partitions & Volumes",
        );
        let switcher = gtk::StackSwitcher::new();
        switcher.set_stack(Some(&tabs));
        let active_tab = Rc::new(RefCell::new("Overview".to_string()));
        let active_tab_changed = active_tab.clone();
        tabs.connect_visible_child_notify(move |stack| {
            if let Some(name) = stack.visible_child_name() {
                *active_tab_changed.borrow_mut() = name.to_string();
            }
        });
        detail.append(&switcher);
        detail.append(&tabs);
        let refresh = gtk::Button::with_label("Refresh SMART details");
        refresh.set_halign(gtk::Align::Start);
        let diagnostics_busy = Rc::new(Cell::new(false));
        let refresh_weak = refresh.downgrade();
        refresh.connect_clicked({
            let request = request_diagnostics.clone();
            let busy = diagnostics_busy.clone();
            move |button| {
                if busy.replace(true) {
                    return;
                }
                button.set_label("Loading SMART details…");
                button.set_sensitive(false);
                request();
                let busy = busy.clone();
                let button = refresh_weak.clone();
                gtk::glib::timeout_add_seconds_local_once(8, move || {
                    busy.set(false);
                    if let Some(button) = button.upgrade() {
                        button.set_label("Refresh SMART details");
                        button.set_sensitive(true);
                    }
                });
            }
        });
        detail.append(&refresh);
        let copy = gtk::Button::with_label("Copy redacted diagnostics");
        copy.set_halign(gtk::Align::Start);
        let selected_id = Rc::new(RefCell::new(None::<String>));
        let latest = Rc::new(RefCell::new(None::<StorageSnapshot>));
        let copy_now = Rc::new(Cell::new(0));
        copy.connect_clicked({
            let selected = selected_id.clone();
            let latest = latest.clone();
            let now = copy_now.clone();
            move |_| {
                if let (Some(id), Some(snapshot)) =
                    (selected.borrow().as_ref(), latest.borrow().as_ref())
                {
                    if let Some(drive) = snapshot.drives.iter().find(|drive| &drive.id == id) {
                        if let Some(display) = gtk::gdk::Display::default() {
                            display.clipboard().set_text(&redacted_diagnostics(
                                drive,
                                snapshot.sampled_at_ms,
                                now.get(),
                            ));
                        }
                    }
                }
            }
        });
        detail.append(&copy);
        let advanced = gtk::Label::new(Some("Technical details unavailable."));
        advanced.set_xalign(0.0);
        advanced.set_wrap(true);
        let expander = gtk::Expander::new(Some("Technical details"));
        expander.set_child(Some(&advanced));
        detail.append(&expander);
        Self {
            root,
            summary,
            status,
            drive_list,
            drive_rows: RefCell::new(HashMap::new()),
            details_title,
            details_meta,
            details_health,
            details_temp,
            overview,
            health,
            volumes,
            volume_rows: RefCell::new(HashMap::new()),
            advanced,
            tabs,
            active_tab,
            selected_id,
            latest,
            copy_now,
            read_graph,
            write_graph,
            temperature_graph,
            request_render,
        }
    }

    pub(crate) fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }

    pub(crate) fn update(
        &self,
        snapshot: Option<&StorageSnapshot>,
        diagnostics: Option<&StorageSnapshot>,
        now_ms: u64,
    ) {
        self.copy_now.set(now_ms);
        let diagnostics = diagnostics.filter(|state| {
            state
                .sampled_at_ms
                .is_some_and(|sampled| now_ms.saturating_sub(sampled) <= 15 * 60_000)
        });
        let Some(snapshot) = snapshot else {
            self.status.set_text("Waiting for the storage service…");
            self.mark_live_metrics_unavailable();
            return;
        };
        let fresh = snapshot
            .sampled_at_ms
            .is_some_and(|sampled| now_ms.saturating_sub(sampled) <= 10_000);
        let available = snapshot.status.eq_ignore_ascii_case("available") && fresh;
        if !available {
            self.status.set_text(
                if !fresh && snapshot.status.eq_ignore_ascii_case("available") {
                    "Last storage reading is stale."
                } else {
                    &snapshot.status
                },
            );
            // Keep the last valid inventory and selection visible while the service recovers.
            self.mark_live_metrics_unavailable();
            return;
        }
        *self.latest.borrow_mut() = Some(snapshot.clone());
        let count = snapshot.drives.len();
        self.summary[0].set_value(count.to_string());
        self.summary[0].set_subtitle(Some("Detected physical devices"));
        self.summary[1].set_value(byte_size(
            snapshot
                .drives
                .iter()
                .filter_map(|drive| drive.capacity_bytes)
                .fold(0_u64, u64::saturating_add),
        ));
        if count == 0 {
            self.status.set_text("No physical drives detected.");
            self.summary[2].set_value("Unknown");
            self.summary[2].set_subtitle(Some("No health evidence"));
            self.summary[3].set_value("Unavailable");
        } else {
            let warnings = snapshot
                .drives
                .iter()
                .filter(|drive| matches!(drive.health.as_str(), "attention" | "critical"))
                .count();
            let total_read = snapshot
                .drives
                .iter()
                .filter_map(|drive| drive.read_bytes_per_sec)
                .reduce(u64::saturating_add);
            let total_write = snapshot
                .drives
                .iter()
                .filter_map(|drive| drive.write_bytes_per_sec)
                .reduce(u64::saturating_add);
            let critical = snapshot
                .drives
                .iter()
                .filter(|d| d.health.eq_ignore_ascii_case("critical"))
                .count();
            let attention = snapshot
                .drives
                .iter()
                .filter(|d| d.health.eq_ignore_ascii_case("attention"))
                .count();
            let unknown = snapshot
                .drives
                .iter()
                .filter(|d| {
                    !d.health.eq_ignore_ascii_case("healthy")
                        && !d.health.eq_ignore_ascii_case("critical")
                        && !d.health.eq_ignore_ascii_case("attention")
                })
                .count();
            let health = if critical > 0 {
                "Critical"
            } else if attention > 0 {
                "Attention"
            } else if unknown > 0 {
                "Unknown / partial"
            } else {
                "Healthy"
            };
            self.summary[2].set_value(health);
            self.summary[2].set_status_chip(Some(health));
            let healthy = snapshot
                .drives
                .iter()
                .filter(|d| d.health.eq_ignore_ascii_case("healthy"))
                .count();
            self.summary[2].set_subtitle(Some(&format!(
                "{healthy} healthy · {attention} attention · {unknown} unknown"
            )));
            self.summary[3].set_value(format!("R {} · W {}", rate(total_read), rate(total_write)));
            self.summary[3].set_subtitle(Some("Current combined throughput"));
            self.status.set_text(&format!(
                "Monitoring available · {warnings} health warning(s) · Read {} · Write {}",
                rate(total_read),
                rate(total_write)
            ));
        }
        let previous_selection = self.selected_id.borrow().clone();
        let selection = reconcile_selection(previous_selection.as_deref(), &snapshot.drives);
        *self.selected_id.borrow_mut() = selection;
        let ids = snapshot
            .drives
            .iter()
            .map(|drive| drive.id.clone())
            .collect::<std::collections::HashSet<_>>();
        let removed = self
            .drive_rows
            .borrow()
            .keys()
            .filter(|id| !ids.contains(*id))
            .cloned()
            .collect::<Vec<_>>();
        for id in removed {
            if let Some(row) = self.drive_rows.borrow_mut().remove(&id) {
                self.drive_list.remove(&row.button);
            }
        }
        for drive in &snapshot.drives {
            let temperature = drive.temperature_c.filter(|_| {
                drive
                    .temperature_sampled_at_ms
                    .is_some_and(|sampled| now_ms.saturating_sub(sampled) <= 10_000)
            });
            let mut rows = self.drive_rows.borrow_mut();
            if !rows.contains_key(&drive.id) {
                let button = gtk::Button::new();
                button.add_css_class("flat");
                button.set_hexpand(true);
                button.set_halign(gtk::Align::Fill);
                let box_ = gtk::Box::new(gtk::Orientation::Vertical, 4);
                let title = gtk::Label::new(None);
                title.set_xalign(0.0);
                title.set_ellipsize(gtk::pango::EllipsizeMode::End);
                let subtitle = gtk::Label::new(None);
                subtitle.set_xalign(0.0);
                subtitle.set_single_line_mode(true);
                subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
                let health = gtk::Label::new(None);
                health.set_xalign(0.0);
                health.add_css_class("status-chip");
                box_.append(&title);
                box_.append(&subtitle);
                box_.append(&health);
                button.set_child(Some(&box_));
                let id = drive.id.clone();
                let selected = self.selected_id.clone();
                let render = self.request_render.clone();
                button.update_property(&[gtk::accessible::Property::Label("Select storage drive")]);
                button.connect_clicked(move |_| {
                    *selected.borrow_mut() = Some(id.clone());
                    render();
                });
                self.drive_list.append(&button);
                rows.insert(
                    drive.id.clone(),
                    DriveRow {
                        button,
                        title,
                        subtitle,
                        health,
                    },
                );
            }
            if let Some(row) = rows.get(&drive.id) {
                row.title.set_text(if drive.model.is_empty() {
                    &drive.name
                } else {
                    &drive.model
                });
                row.subtitle.set_text(&format!(
                    "{} · {} · {} · R {} · W {}",
                    drive.kind,
                    drive
                        .capacity_bytes
                        .map(byte_size)
                        .unwrap_or_else(|| "Unavailable".into()),
                    temperature
                        .map(|v| format!("{v:.1} °C"))
                        .unwrap_or_else(|| "Temp unavailable".into()),
                    rate(drive.read_bytes_per_sec),
                    rate(drive.write_bytes_per_sec)
                ));
                row.health.set_text(&drive.health);
                set_health_style(&row.health, &drive.health);
                let accessible_name = format!(
                    "Select {}",
                    if drive.model.is_empty() {
                        &drive.name
                    } else {
                        &drive.model
                    }
                );
                row.button
                    .update_property(&[gtk::accessible::Property::Label(&accessible_name)]);
                row.button
                    .set_tooltip_text(Some(&format!("{} — {}", drive.kind, drive.transport)));
                if Some(&drive.id) == self.selected_id.borrow().as_ref() {
                    row.button.add_css_class("suggested-action");
                } else {
                    row.button.remove_css_class("suggested-action");
                }
            }
        }
        if let Some(drive) = snapshot
            .drives
            .iter()
            .find(|drive| Some(&drive.id) == self.selected_id.borrow().as_ref())
        {
            let diagnostic_drive = diagnostics
                .and_then(|snapshot| snapshot.drives.iter().find(|item| item.id == drive.id));
            self.update_details(drive, diagnostic_drive, now_ms);
        } else {
            self.details_title.set_text("No physical drive selected");
            self.details_meta
                .set_text("Connect a supported drive to view its details.");
            self.overview
                .set_text("No physical drive is currently available.");
            self.health.set_text("SMART details are unavailable.");
            self.update_volumes(&[]);
        }
    }

    fn update_details(
        &self,
        drive: &StorageDrive,
        diagnostics: Option<&StorageDrive>,
        now_ms: u64,
    ) {
        let name = if drive.model.is_empty() {
            &drive.name
        } else {
            &drive.model
        };
        self.details_title.set_text(name);
        self.details_meta.set_text(&format!(
            "{} · {} · {}",
            drive.kind,
            drive.transport,
            drive
                .capacity_bytes
                .map(byte_size)
                .unwrap_or_else(|| "Capacity unavailable".into())
        ));
        self.details_health
            .set_text(&format!("Health: {}", drive.health));
        set_health_style(&self.details_health, &drive.health);
        let temp = drive.temperature_c.filter(|_| {
            drive
                .temperature_sampled_at_ms
                .is_some_and(|sampled| now_ms.saturating_sub(sampled) <= 10_000)
        });
        self.details_temp.set_text(
            &temp
                .map(|v| format!("Temperature: {v:.1} °C"))
                .unwrap_or_else(|| "Temperature unavailable".into()),
        );
        self.overview.set_text(&format!("Model: {}\nManufacturer: {}\nType: {}\nTransport: {}\nCapacity: {}\nTemperature: {}\nRead: {}\nWrite: {}\nRead IOPS: {}\nWrite IOPS: {}", name, drive.manufacturer.as_deref().unwrap_or("Unavailable"), drive.kind, drive.transport, drive.capacity_bytes.map(byte_size).unwrap_or_else(|| "Unavailable".into()), temp.map(|v| format!("{v:.1} °C")).unwrap_or_else(|| "Unavailable".into()), rate(drive.read_bytes_per_sec), rate(drive.write_bytes_per_sec), drive.read_iops.map(|v| v.to_string()).unwrap_or_else(|| "Unavailable".into()), drive.write_iops.map(|v| v.to_string()).unwrap_or_else(|| "Unavailable".into())));
        self.read_graph
            .set_samples(&history_rates(&drive.history, "read_bytes_per_sec"));
        self.write_graph
            .set_samples(&history_rates(&drive.history, "write_bytes_per_sec"));
        self.temperature_graph.set_samples(
            &drive
                .history
                .iter()
                .map(|p| {
                    dbus_decode::float(p, "temperature_c")
                        .map(|v| v as f32)
                        .unwrap_or(f32::NAN)
                })
                .collect::<Vec<_>>(),
        );
        self.health.set_text(&format!(
            "Assessment: {}\nEvidence: {}\nSource: {}\n\n{}",
            drive.health,
            if drive.health_reason.is_empty() {
                "Unavailable"
            } else {
                &drive.health_reason
            },
            diagnostics
                .map(|d| d.health_source.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("Unavailable"),
            diagnostics
                .map(|d| d
                    .health_attributes
                    .iter()
                    .map(|a| format!(
                        "{}: {} {}{}",
                        dbus_decode::string(a, "key").unwrap_or_else(|| "Attribute".into()),
                        dbus_decode::string(a, "value").unwrap_or_else(|| "Unavailable".into()),
                        dbus_decode::string(a, "unit").unwrap_or_default(),
                        if dbus_decode::boolean(a, "warning").unwrap_or(false) {
                            " · Warning"
                        } else {
                            ""
                        }
                    ))
                    .collect::<Vec<_>>()
                    .join("\n"))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Detailed attributes unavailable".into())
        ));
        self.update_volumes(&drive.volumes);
        let temperatures = drive
            .temperatures
            .iter()
            .filter_map(|sensor| {
                let value = dbus_decode::float(sensor, "celsius")?;
                let age = dbus_decode::unsigned(sensor, "sampled_at_ms")?;
                (now_ms.saturating_sub(age) <= 10_000).then(|| {
                    format!(
                        "{}: {value:.1} °C · {}",
                        dbus_decode::string(sensor, "label")
                            .unwrap_or_else(|| "Temperature".into()),
                        dbus_decode::string(sensor, "source")
                            .unwrap_or_else(|| "Unknown source".into())
                    )
                })
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.advanced.set_text(&format!(
            "Device: {}\nFirmware: {}\nLogical block size: {}\nPhysical block size: {}{}{}",
            drive.name,
            drive.firmware.as_deref().unwrap_or("Unavailable"),
            drive
                .logical_block_size
                .map(byte_size)
                .unwrap_or_else(|| "Unavailable".into()),
            drive
                .physical_block_size
                .map(byte_size)
                .unwrap_or_else(|| "Unavailable".into()),
            if temperatures.is_empty() { "" } else { "\n" },
            temperatures
        ));
        self.tabs.set_visible_child_name(&self.active_tab.borrow());
    }

    fn mark_live_metrics_unavailable(&self) {
        self.summary[3].set_value("Unavailable");
        self.summary[3].set_subtitle(Some("Telemetry is stale or disconnected"));
        if let Some(snapshot) = self.latest.borrow().as_ref() {
            for drive in &snapshot.drives {
                if let Some(row) = self.drive_rows.borrow().get(&drive.id) {
                    row.subtitle.set_text(&format!(
                        "{} · {} · Live telemetry unavailable",
                        drive.kind,
                        drive
                            .capacity_bytes
                            .map(byte_size)
                            .unwrap_or_else(|| "Capacity unavailable".into())
                    ));
                }
            }
            if let Some(drive) = snapshot
                .drives
                .iter()
                .find(|drive| Some(&drive.id) == self.selected_id.borrow().as_ref())
            {
                self.overview.set_text(&format!(
                    "Model: {}\nType: {}\nTransport: {}\nCapacity: {}\nTemperature: Unavailable\nRead: Unavailable\nWrite: Unavailable\nRead IOPS: Unavailable\nWrite IOPS: Unavailable",
                    if drive.model.is_empty() { &drive.name } else { &drive.model },
                    drive.kind,
                    drive.transport,
                    drive.capacity_bytes.map(byte_size).unwrap_or_else(|| "Unavailable".into())
                ));
            }
        }
        self.read_graph.set_samples(&[]);
        self.write_graph.set_samples(&[]);
        self.temperature_graph.set_samples(&[]);
        for row in self.volume_rows.borrow().values() {
            row.usage.set_text(Some("Usage unavailable"));
        }
    }

    fn update_volumes(&self, volumes: &[WireMap]) {
        if volumes.is_empty() && self.volumes.first_child().is_none() {
            let empty = gtk::Label::new(Some("No attributable volumes reported."));
            empty.set_xalign(0.0);
            empty.add_css_class("volume-placeholder");
            self.volumes.append(&empty);
        } else if !volumes.is_empty()
            && self
                .volumes
                .first_child()
                .is_some_and(|child| child.has_css_class("volume-placeholder"))
        {
            if let Some(child) = self.volumes.first_child() {
                self.volumes.remove(&child);
            }
        }
        let names = volumes
            .iter()
            .filter_map(|volume| dbus_decode::string(volume, "name"))
            .collect::<std::collections::HashSet<_>>();
        let removed = self
            .volume_rows
            .borrow()
            .keys()
            .filter(|name| !names.contains(*name))
            .cloned()
            .collect::<Vec<_>>();
        for name in removed {
            if let Some(row) = self.volume_rows.borrow_mut().remove(&name) {
                self.volumes.remove(&row.root);
            }
        }
        if volumes.is_empty()
            && self.volume_rows.borrow().is_empty()
            && self.volumes.first_child().is_none()
        {
            let empty = gtk::Label::new(Some("No attributable volumes reported."));
            empty.set_xalign(0.0);
            empty.add_css_class("volume-placeholder");
            self.volumes.append(&empty);
        }
        for volume in volumes {
            let name = dbus_decode::string(volume, "name").unwrap_or_else(|| "Volume".into());
            let mut rows = self.volume_rows.borrow_mut();
            if !rows.contains_key(&name) {
                let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
                root.add_css_class("metric-card");
                let label = gtk::Label::new(None);
                label.set_xalign(0.0);
                label.set_wrap(true);
                label.set_selectable(true);
                let usage = gtk::ProgressBar::new();
                usage.set_show_text(true);
                usage.update_property(&[gtk::accessible::Property::Label("Filesystem space used")]);
                root.append(&label);
                root.append(&usage);
                self.volumes.append(&root);
                rows.insert(name.clone(), VolumeWidgets { root, label, usage });
            }
            if let Some(row) = rows.get(&name) {
                let used = dbus_decode::unsigned(volume, "used_bytes");
                let available = dbus_decode::unsigned(volume, "available_bytes");
                let total = used
                    .zip(available)
                    .and_then(|(u, a)| u.checked_add(a).filter(|total| *total > 0));
                let mounts = dbus_decode::strings(volume, "mount_points").unwrap_or_default();
                row.label.set_text(&format!(
                    "{} · {} · {} · Size {} · Used {} · Available {} · {}",
                    name,
                    dbus_decode::string(volume, "filesystem")
                        .unwrap_or_else(|| "Filesystem unavailable".into()),
                    dbus_decode::string(volume, "status").unwrap_or_else(|| "Unknown".into()),
                    dbus_decode::unsigned(volume, "size_bytes")
                        .map(byte_size)
                        .unwrap_or_else(|| "Unavailable".into()),
                    used.map(byte_size).unwrap_or_else(|| "Unavailable".into()),
                    available
                        .map(byte_size)
                        .unwrap_or_else(|| "Unavailable".into()),
                    if mounts.is_empty() {
                        "Not mounted".into()
                    } else {
                        format!("Mounted at {}", mounts.join(", "))
                    }
                ));
                let fraction = volume_usage_fraction(used, available);
                row.usage.set_visible(fraction.is_some());
                row.usage.set_fraction(fraction.unwrap_or_default());
                let usage_text = total
                    .zip(used)
                    .map(|(total, used)| format!("{:.0}% used", used as f64 * 100.0 / total as f64))
                    .unwrap_or_else(|| "Usage unavailable".into());
                row.usage.set_text(Some(&usage_text));
            }
        }
    }
}

fn reconcile_selection(selected: Option<&str>, drives: &[StorageDrive]) -> Option<String> {
    selected
        .filter(|id| drives.iter().any(|drive| drive.id == *id))
        .map(str::to_string)
        .or_else(|| drives.iter().map(|drive| drive.id.clone()).min())
}

fn set_health_style(label: &gtk::Label, health: &str) {
    for class in [
        "status-chip-success",
        "status-chip-warning",
        "status-chip-error",
        "status-chip-info",
    ] {
        label.remove_css_class(class);
    }
    let class = match health.to_ascii_lowercase().as_str() {
        "healthy" => "status-chip-success",
        "attention" => "status-chip-warning",
        "critical" => "status-chip-error",
        _ => "status-chip-info",
    };
    label.add_css_class(class);
}

fn volume_usage_fraction(used: Option<u64>, available: Option<u64>) -> Option<f64> {
    let (used, available) = (used?, available?);
    let total = used.checked_add(available).filter(|total| *total > 0)?;
    Some((used as f64 / total as f64).clamp(0.0, 1.0))
}

fn rate(bytes_per_sec: Option<u64>) -> String {
    bytes_per_sec
        .map(|value| format!("{:.1} MB/s", value as f64 / 1_000_000.0))
        .unwrap_or_else(|| "Unavailable".into())
}

fn redacted_diagnostics(
    drive: &StorageDrive,
    snapshot_sampled_at_ms: Option<u64>,
    now_ms: u64,
) -> String {
    let live =
        snapshot_sampled_at_ms.is_some_and(|sampled| now_ms.saturating_sub(sampled) <= 10_000);
    let temperature = drive.temperature_c.filter(|_| {
        drive
            .temperature_sampled_at_ms
            .is_some_and(|sampled| now_ms.saturating_sub(sampled) <= 10_000)
    });
    format!(
        "Storage drive\nModel: {}\nType: {}\nTransport: {}\nCapacity: {}\nHealth: {}\nRead: {}\nWrite: {}\nTemperature: {}\nDevice IDs, serial numbers, mount paths, and raw SMART attributes are omitted.",
        drive.model,
        drive.kind,
        drive.transport,
        drive.capacity_bytes.map(byte_size).unwrap_or_else(|| "Unavailable".into()),
        drive.health,
        if live { rate(drive.read_bytes_per_sec) } else { "Unavailable".into() },
        if live { rate(drive.write_bytes_per_sec) } else { "Unavailable".into() },
        temperature.map(|value| format!("{value:.1} °C")).unwrap_or_else(|| "Unavailable".into()),
    )
}

fn history_rates(history: &[WireMap], key: &str) -> Vec<f32> {
    history
        .iter()
        .map(|point| {
            dbus_decode::unsigned(point, key)
                .map(|value| value as f32 / 1_000_000.0)
                .unwrap_or(f32::NAN)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::OwnedValue;

    #[test]
    fn wrong_wire_types_do_not_create_drives() {
        let mut map = WireMap::new();
        map.insert("drives".into(), OwnedValue::from(1_u64));
        assert!(decode(map).drives.is_empty());
    }

    #[test]
    fn dashboard_does_not_claim_unknown_health_is_healthy() {
        let snapshot = StorageSnapshot {
            status: "available".into(),
            sampled_at_ms: Some(1),
            drives: vec![],
        };
        assert_eq!(dashboard_summary(Some(&snapshot)).0, "0");
        assert_eq!(dashboard_summary(None).0, "—");
    }

    #[test]
    fn selection_survives_telemetry_and_reordering_and_falls_back_on_removal() {
        let drives = vec![
            StorageDrive {
                id: "b".into(),
                ..StorageDrive::default()
            },
            StorageDrive {
                id: "a".into(),
                ..StorageDrive::default()
            },
        ];
        assert_eq!(
            reconcile_selection(Some("b"), &drives).as_deref(),
            Some("b")
        );
        assert_eq!(
            reconcile_selection(Some("removed"), &drives).as_deref(),
            Some("a")
        );
        assert_eq!(reconcile_selection(Some("removed"), &[]), None);
    }

    #[test]
    fn unavailable_snapshot_does_not_overwrite_valid_temperature() {
        let drive = StorageDrive {
            temperature_c: Some(41.5),
            temperature_sampled_at_ms: Some(1_000),
            ..StorageDrive::default()
        };
        assert!(redacted_diagnostics(&drive, Some(1_000), 2_000).contains("41.5 °C"));
        assert!(
            redacted_diagnostics(&drive, Some(1_000), 20_000).contains("Temperature: Unavailable")
        );
    }

    #[test]
    fn volume_usage_is_unavailable_without_both_counters() {
        assert_eq!(volume_usage_fraction(Some(5), None), None);
        assert_eq!(volume_usage_fraction(Some(5), Some(5)), Some(0.5));
    }

    #[test]
    fn missing_history_samples_create_chart_gaps() {
        let mut measured = WireMap::new();
        measured.insert("read_bytes_per_sec".into(), OwnedValue::from(2_000_000_u64));
        let samples = history_rates(&[measured, WireMap::new()], "read_bytes_per_sec");
        assert_eq!(samples[0], 2.0);
        assert!(samples[1].is_nan());
    }

    #[test]
    fn copied_diagnostics_exclude_identifiers_and_mount_paths() {
        let drive = StorageDrive {
            id: "SERIAL-SECRET".into(),
            name: "/dev/nvme0n1".into(),
            model: "Example SSD".into(),
            health_reason: "private/path".into(),
            ..StorageDrive::default()
        };
        let report = redacted_diagnostics(&drive, Some(0), 0);
        assert!(report.contains("Example SSD"));
        assert!(!report.contains("SERIAL-SECRET"));
        assert!(!report.contains("/dev/nvme0n1"));
        assert!(!report.contains("private/path"));
    }
}
