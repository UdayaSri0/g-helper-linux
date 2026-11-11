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
    pub health_sampled_at_ms: Option<u64>,
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
                health_sampled_at_ms: dbus_decode::unsigned(&health, "sampled_at_ms"),
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
    dashboards_box: gtk::Box,
    dashboards: RefCell<HashMap<String, DriveDashboard>>,
    order: RefCell<Vec<String>>,
    latest: Rc<RefCell<Option<StorageSnapshot>>>,
    copy_now: Rc<Cell<u64>>,
    request_diagnostics: Rc<dyn Fn()>,
    diagnostics_busy: Rc<Cell<bool>>,
}

struct DriveDashboard {
    root: gtk::Box,
    details_title: gtk::Label,
    details_meta: gtk::Label,
    details_health: gtk::Label,
    details_temp: gtk::Label,
    overview_metrics: [MetricCard; 4],
    health_assessment: gtk::Label,
    health_meta: gtk::Label,
    health_attributes: gtk::Box,
    health_advanced_attributes: gtk::Box,
    health_attribute_placeholder: gtk::Label,
    health_attribute_rows: RefCell<HashMap<String, HealthAttributeWidgets>>,
    volumes: gtk::Box,
    volume_rows: RefCell<HashMap<String, VolumeWidgets>>,
    advanced: gtk::Label,
    read_graph: HistoryGraph,
    write_graph: HistoryGraph,
    temperature_graph: HistoryGraph,
    performance_read_iops: MetricCard,
    performance_write_iops: MetricCard,
    performance_freshness: gtk::Label,
}

struct VolumeWidgets {
    root: gtk::Box,
    title: gtk::Label,
    metadata: gtk::Label,
    usage: gtk::ProgressBar,
}

struct HealthAttributeWidgets {
    parent: gtk::Box,
    row: gtk::Box,
    label: gtk::Label,
}

impl StoragePage {
    pub(crate) fn new(request_diagnostics: impl Fn() + 'static) -> Self {
        let request_diagnostics = Rc::new(request_diagnostics) as Rc<dyn Fn()>;
        let body = page_container();
        body.append(&page_header(
            "Storage",
            "Physical drive monitoring, performance and health",
        ));
        let summary = [
            MetricCard::new("Physical drives"),
            MetricCard::new("Total capacity"),
            MetricCard::new("Overall health"),
            MetricCard::new("Live activity"),
        ];
        let summary_grid = gtk::FlowBox::new();
        summary_grid.set_selection_mode(gtk::SelectionMode::None);
        summary_grid.set_min_children_per_line(1);
        summary_grid.set_max_children_per_line(4);
        summary_grid.set_column_spacing(12);
        summary_grid.set_row_spacing(12);
        for (card, icon) in summary.iter().zip([
            "rog-storage-symbolic",
            "drive-harddisk-symbolic",
            "emblem-ok-symbolic",
            "utilities-system-monitor-symbolic",
        ]) {
            card.add_css_class("metric-card-compact");
            card.set_subtitle_width_chars(24);
            card.set_icon_name(icon);
            summary_grid.insert(card.widget(), -1);
        }
        body.append(&summary_grid);
        let status = gtk::Label::new(Some("Detecting physical drives…"));
        status.set_xalign(0.0);
        status.add_css_class("storage-status-caption");
        body.append(&status);
        let dashboards_box = gtk::Box::new(gtk::Orientation::Vertical, 20);
        dashboards_box.set_hexpand(true);
        body.append(&dashboards_box);
        let root = gtk::ScrolledWindow::new();
        root.set_vexpand(true);
        root.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(1260);
        clamp.set_tightening_threshold(900);
        clamp.set_child(Some(&body));
        root.set_child(Some(&clamp));
        let latest = Rc::new(RefCell::new(None));
        let copy_now = Rc::new(Cell::new(0));
        Self {
            root,
            summary,
            status,
            dashboards_box,
            dashboards: RefCell::new(HashMap::new()),
            order: RefCell::new(Vec::new()),
            latest,
            copy_now,
            request_diagnostics,
            diagnostics_busy: Rc::new(Cell::new(false)),
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
            // Keep the last valid inventory visible while the service recovers.
            self.mark_live_metrics_unavailable();
            return;
        }
        self.status.set_text(&format!(
            "Updated {} · {} physical drive(s)",
            age_label(snapshot.sampled_at_ms, now_ms),
            snapshot.drives.len()
        ));
        *self.latest.borrow_mut() = Some(snapshot.clone());
        let count = snapshot.drives.len();
        self.summary[0].set_value(count.to_string());
        self.summary[0].set_subtitle(Some("Detected physical devices"));
        self.summary[1].set_value(
            total_capacity(&snapshot.drives)
                .map(byte_size)
                .unwrap_or_else(|| "Unavailable".into()),
        );
        if count == 0 {
            self.status.set_text("No physical drives detected.");
            self.summary[2].set_value("Unknown");
            self.summary[2].set_status_chip(None);
            self.summary[2].set_subtitle(Some("No health evidence"));
            self.summary[3].set_value("Unavailable");
            self.summary[3].set_subtitle(Some("No physical drive activity"));
        } else {
            let total_read = snapshot
                .drives
                .iter()
                .map(|drive| drive.read_bytes_per_sec)
                .try_fold(0_u64, |total, rate| Some(total.saturating_add(rate?)));
            let total_write = snapshot
                .drives
                .iter()
                .map(|drive| drive.write_bytes_per_sec)
                .try_fold(0_u64, |total, rate| Some(total.saturating_add(rate?)));
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
        }
        let incoming = snapshot
            .drives
            .iter()
            .map(|d| d.id.clone())
            .collect::<Vec<_>>();
        let next_order = reconcile_order(&self.order.borrow(), &incoming);
        let mut dashboards = self.dashboards.borrow_mut();
        for id in self
            .order
            .borrow()
            .iter()
            .filter(|id| !incoming.contains(id))
        {
            if let Some(dashboard) = dashboards.remove(id) {
                self.dashboards_box.remove(&dashboard.root);
            }
        }
        for id in &next_order {
            if !dashboards.contains_key(id) {
                let dashboard = DriveDashboard::new(
                    id.clone(),
                    self.request_diagnostics.clone(),
                    self.diagnostics_busy.clone(),
                    self.latest.clone(),
                    self.copy_now.clone(),
                );
                self.dashboards_box.append(&dashboard.root);
                dashboards.insert(id.clone(), dashboard);
            }
            if let Some(drive) = snapshot.drives.iter().find(|d| &d.id == id) {
                let diagnostic_drive = diagnostics.and_then(|state| diagnostic_drive(state, id));
                dashboards[id].update_details(
                    drive,
                    diagnostic_drive,
                    snapshot.sampled_at_ms,
                    now_ms,
                );
            }
        }
        *self.order.borrow_mut() = next_order;
    }

    fn mark_live_metrics_unavailable(&self) {
        self.summary[3].set_value("Unavailable");
        self.summary[3].set_subtitle(Some("Telemetry is stale or disconnected"));
        for dashboard in self.dashboards.borrow().values() {
            dashboard.mark_live_metrics_unavailable();
        }
    }
}

fn reconcile_order(previous: &[String], current: &[String]) -> Vec<String> {
    let mut result = previous
        .iter()
        .filter(|id| current.contains(id))
        .cloned()
        .collect::<Vec<_>>();
    for id in current {
        if !result.contains(id) {
            result.push(id.clone());
        }
    }
    result
}

fn diagnostic_drive<'a>(snapshot: &'a StorageSnapshot, id: &str) -> Option<&'a StorageDrive> {
    snapshot.drives.iter().find(|drive| drive.id == id)
}

impl DriveDashboard {
    fn new(
        id: String,
        request_diagnostics: Rc<dyn Fn()>,
        busy: Rc<Cell<bool>>,
        latest: Rc<RefCell<Option<StorageSnapshot>>>,
        copy_now: Rc<Cell<u64>>,
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.add_css_class("surface-card");
        root.set_hexpand(true);
        let details_title = gtk::Label::new(Some("Drive"));
        details_title.set_xalign(0.0);
        details_title.set_wrap(true);
        details_title.add_css_class("page-title");
        root.append(&details_title);
        let details_meta = gtk::Label::new(None);
        details_meta.set_xalign(0.0);
        details_meta.set_wrap(true);
        root.append(&details_meta);
        let header_chips = gtk::FlowBox::new();
        header_chips.set_selection_mode(gtk::SelectionMode::None);
        header_chips.set_column_spacing(8);
        header_chips.set_row_spacing(8);
        let details_health = gtk::Label::new(Some("Health unavailable"));
        let details_temp = gtk::Label::new(Some("Temperature unavailable"));
        for chip in [&details_health, &details_temp] {
            chip.add_css_class("status-chip");
            header_chips.insert(chip, -1);
        }
        root.append(&header_chips);
        let overview_metrics = [
            MetricCard::new("Temperature"),
            MetricCard::new("SMART health"),
            MetricCard::new("Read throughput"),
            MetricCard::new("Write throughput"),
        ];
        let metrics = gtk::FlowBox::new();
        metrics.set_selection_mode(gtk::SelectionMode::None);
        metrics.set_min_children_per_line(1);
        metrics.set_max_children_per_line(4);
        metrics.set_column_spacing(10);
        metrics.set_row_spacing(10);
        for (card, icon) in overview_metrics.iter().zip([
            "weather-clear-symbolic",
            "emblem-ok-symbolic",
            "go-down-symbolic",
            "go-up-symbolic",
        ]) {
            card.add_css_class("metric-card-compact");
            card.set_icon_name(icon);
            metrics.insert(card.widget(), -1);
        }
        root.append(&metrics);
        let performance = gtk::Box::new(gtk::Orientation::Vertical, 10);
        performance.append(&section_title("Performance"));
        let performance_metrics = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let performance_read_iops = MetricCard::new("Read IOPS");
        let performance_write_iops = MetricCard::new("Write IOPS");
        for card in [&performance_read_iops, &performance_write_iops] {
            card.add_css_class("metric-card-compact");
            card.set_value("Unavailable");
            performance_metrics.append(card.widget());
        }
        performance.append(&performance_metrics);
        let performance_freshness = gtk::Label::new(Some("Waiting for telemetry"));
        performance_freshness.set_xalign(0.0);
        performance_freshness.add_css_class("storage-status-caption");
        performance.append(&performance_freshness);
        let read_graph = HistoryGraph::new("Read throughput", 0.0, 1.0, "MB/s", (0.30, 0.64, 1.0));
        let write_graph =
            HistoryGraph::new("Write throughput", 0.0, 1.0, "MB/s", (0.49, 0.88, 0.70));
        let temperature_graph =
            HistoryGraph::new("Drive temperature", 0.0, 50.0, "°C", (0.96, 0.68, 0.38));
        read_graph.set_content_height(170);
        write_graph.set_content_height(170);
        temperature_graph.set_content_height(155);
        let io_graphs = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        io_graphs.append(read_graph.widget());
        io_graphs.append(write_graph.widget());
        performance.append(&io_graphs);
        performance.append(temperature_graph.widget());
        root.append(&performance);
        root.connect_notify_local(Some("width"), move |root, _| {
            io_graphs.set_orientation(if root.width() < 640 {
                gtk::Orientation::Vertical
            } else {
                gtk::Orientation::Horizontal
            });
        });
        let health = gtk::Box::new(gtk::Orientation::Vertical, 8);
        health.append(&section_title("SMART health"));
        let health_assessment = gtk::Label::new(Some("SMART data unavailable"));
        health_assessment.set_xalign(0.0);
        health_assessment.add_css_class("status-chip");
        health.append(&health_assessment);
        let health_meta =
            gtk::Label::new(Some("Refresh SMART details to request a read-only sample."));
        health_meta.set_xalign(0.0);
        health_meta.set_wrap(true);
        health.append(&health_meta);
        let health_attributes = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let health_attribute_placeholder = gtk::Label::new(Some(
            "Detailed SMART attributes are unavailable. Refresh to request a read-only sample.",
        ));
        health_attribute_placeholder.set_xalign(0.0);
        health_attribute_placeholder.set_wrap(true);
        health_attributes.append(&health_attribute_placeholder);
        health.append(&health_attributes);
        let smart_expander = gtk::Expander::new(Some("Advanced SMART attributes"));
        let health_advanced_attributes = gtk::Box::new(gtk::Orientation::Vertical, 8);
        smart_expander.set_child(Some(&health_advanced_attributes));
        health.append(&smart_expander);
        root.append(&health);
        let volumes = gtk::Box::new(gtk::Orientation::Vertical, 12);
        volumes.set_hexpand(true);
        let volume_placeholder = gtk::Label::new(Some("No attributable volumes reported."));
        volume_placeholder.set_xalign(0.0);
        volume_placeholder.add_css_class("volume-placeholder");
        volumes.append(&volume_placeholder);
        root.append(&section_title("Partitions & Volumes"));
        root.append(&volumes);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let refresh = gtk::Button::with_label("Refresh SMART details for all drives");
        let refresh_weak = refresh.downgrade();
        refresh.connect_clicked(move |button| {
            if busy.replace(true) {
                return;
            }
            button.set_label("Loading SMART details…");
            button.set_sensitive(false);
            request_diagnostics();
            let button = refresh_weak.clone();
            let busy = busy.clone();
            gtk::glib::timeout_add_seconds_local_once(8, move || {
                busy.set(false);
                if let Some(button) = button.upgrade() {
                    button.set_label("Refresh SMART details for all drives");
                    button.set_sensitive(true);
                }
            });
        });
        actions.append(&refresh);
        let copy = gtk::Button::with_label("Copy redacted diagnostics");
        copy.connect_clicked(move |_| {
            if let Some(snapshot) = latest.borrow().as_ref() {
                if let Some(drive) = snapshot.drives.iter().find(|drive| drive.id == id) {
                    if let Some(display) = gtk::gdk::Display::default() {
                        display.clipboard().set_text(&redacted_diagnostics(
                            drive,
                            snapshot.sampled_at_ms,
                            copy_now.get(),
                        ));
                    }
                }
            }
        });
        actions.append(&copy);
        root.append(&actions);
        let advanced = gtk::Label::new(Some("Technical details unavailable."));
        advanced.set_xalign(0.0);
        advanced.set_wrap(true);
        advanced.set_selectable(true);
        let technical = gtk::Expander::new(Some("Technical details"));
        technical.set_child(Some(&advanced));
        root.append(&technical);
        Self {
            root,
            details_title,
            details_meta,
            details_health,
            details_temp,
            overview_metrics,
            health_assessment,
            health_meta,
            health_attributes,
            health_advanced_attributes,
            health_attribute_placeholder,
            health_attribute_rows: RefCell::new(HashMap::new()),
            volumes,
            volume_rows: RefCell::new(HashMap::new()),
            advanced,
            read_graph,
            write_graph,
            temperature_graph,
            performance_read_iops,
            performance_write_iops,
            performance_freshness,
        }
    }

    fn update_details(
        &self,
        drive: &StorageDrive,
        diagnostics: Option<&StorageDrive>,
        snapshot_sampled_at_ms: Option<u64>,
        now_ms: u64,
    ) {
        let name = if drive.model.is_empty() {
            &drive.name
        } else {
            &drive.model
        };
        self.details_title.set_text(name);
        self.details_meta.set_text(&format!(
            "{} · {} · {} · {}",
            drive
                .manufacturer
                .as_deref()
                .unwrap_or("Manufacturer unavailable"),
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
        self.overview_metrics[0].set_value(
            temp.map(|v| format!("{v:.1} °C"))
                .unwrap_or_else(|| "Unavailable".into()),
        );
        self.overview_metrics[0].set_subtitle(Some("Current drive sensor"));
        self.overview_metrics[1].set_value(&drive.health);
        self.overview_metrics[1].set_status_chip(Some(&drive.health));
        self.overview_metrics[1].set_subtitle(Some("SMART assessment"));
        self.overview_metrics[2].set_value(rate(drive.read_bytes_per_sec));
        self.overview_metrics[2].set_subtitle(Some("Current read rate"));
        self.overview_metrics[3].set_value(rate(drive.write_bytes_per_sec));
        self.overview_metrics[3].set_subtitle(Some("Current write rate"));
        self.performance_read_iops.set_value(
            drive
                .read_iops
                .map(|n| n.to_string())
                .unwrap_or_else(|| "Unavailable".into()),
        );
        self.performance_write_iops.set_value(
            drive
                .write_iops
                .map(|n| n.to_string())
                .unwrap_or_else(|| "Unavailable".into()),
        );
        self.performance_freshness.set_text(&format!(
            "Live sample · {}",
            age_label(snapshot_sampled_at_ms, now_ms)
        ));
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
        let health_source = diagnostics
            .map(|d| d.health_source.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if drive.health_source.is_empty() {
                    "Unavailable"
                } else {
                    &drive.health_source
                }
            });
        let warning_count = diagnostics
            .map(|d| {
                d.health_attributes
                    .iter()
                    .filter(|a| dbus_decode::boolean(a, "warning").unwrap_or(false))
                    .count()
            })
            .unwrap_or(0);
        let health_sampled = diagnostics
            .and_then(|d| d.health_sampled_at_ms)
            .or(drive.health_sampled_at_ms);
        let health_note = if drive.health_reason.is_empty() {
            "No additional health warning reported"
        } else {
            &drive.health_reason
        };
        self.health_assessment
            .set_text(&format!("SMART assessment: {}", drive.health));
        set_health_style(&self.health_assessment, &drive.health);
        self.health_meta.set_text(&format!("Provider: {health_source} · Last successful sample: {} · {warning_count} warning attribute(s)\n{health_note}", age_label(health_sampled, now_ms)));
        self.update_health_attributes(
            diagnostics
                .map(|d| d.health_attributes.as_slice())
                .unwrap_or(&[]),
        );
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
    }

    fn mark_live_metrics_unavailable(&self) {
        self.details_temp.set_text("Temperature unavailable");
        self.overview_metrics[0].set_value("Unavailable");
        self.overview_metrics[2].set_value("Unavailable");
        self.overview_metrics[3].set_value("Unavailable");
        self.performance_read_iops.set_value("Unavailable");
        self.performance_write_iops.set_value("Unavailable");
        self.performance_freshness
            .set_text("Telemetry is stale or disconnected");
        self.read_graph.set_samples(&[]);
        self.write_graph.set_samples(&[]);
        self.temperature_graph.set_samples(&[]);
        for row in self.volume_rows.borrow().values() {
            row.usage.set_visible(false);
            row.metadata
                .set_text("Storage telemetry is stale; volume usage unavailable.");
        }
    }

    fn update_health_attributes(&self, attributes: &[WireMap]) {
        let keys = attributes
            .iter()
            .filter_map(|attribute| dbus_decode::string(attribute, "key"))
            .collect::<std::collections::HashSet<_>>();
        let removed = self
            .health_attribute_rows
            .borrow()
            .keys()
            .filter(|key| !keys.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        for key in removed {
            if let Some(row) = self.health_attribute_rows.borrow_mut().remove(&key) {
                row.parent.remove(&row.row);
            }
        }
        if attributes.is_empty() {
            if self.health_attributes.first_child().is_none() {
                self.health_attributes
                    .append(&self.health_attribute_placeholder);
            }
            return;
        }
        if self
            .health_attributes
            .first_child()
            .is_some_and(|child| child.is::<gtk::Label>())
        {
            if let Some(child) = self.health_attributes.first_child() {
                self.health_attributes.remove(&child);
            }
        }
        for attribute in attributes {
            let key = dbus_decode::string(attribute, "key").unwrap_or_else(|| "Attribute".into());
            let mut rows = self.health_attribute_rows.borrow_mut();
            if !rows.contains_key(&key) {
                let parent = if is_advanced_smart_attribute(&key) {
                    self.health_advanced_attributes.clone()
                } else {
                    self.health_attributes.clone()
                };
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                row.add_css_class("smart-attribute-row");
                let label = gtk::Label::new(None);
                label.set_xalign(0.0);
                label.set_hexpand(true);
                label.set_wrap(true);
                row.append(&label);
                parent.append(&row);
                rows.insert(key.clone(), HealthAttributeWidgets { parent, row, label });
            }
            if let Some(widgets) = rows.get(&key) {
                let value =
                    dbus_decode::string(attribute, "value").unwrap_or_else(|| "Unavailable".into());
                let unit = dbus_decode::string(attribute, "unit").unwrap_or_default();
                let warning = dbus_decode::boolean(attribute, "warning").unwrap_or(false);
                widgets.label.set_text(&format!(
                    "{}\n{} {}{}",
                    smart_attribute_label(&key),
                    value,
                    unit,
                    if warning { " · Warning" } else { "" }
                ));
                widgets.label.set_tooltip_text(Some(&key));
                if warning {
                    widgets.row.add_css_class("smart-attribute-warning");
                } else {
                    widgets.row.remove_css_class("smart-attribute-warning");
                }
            }
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
                let title = gtk::Label::new(None);
                title.set_xalign(0.0);
                title.set_ellipsize(gtk::pango::EllipsizeMode::End);
                title.set_tooltip_text(Some(&name));
                let metadata = gtk::Label::new(None);
                metadata.set_xalign(0.0);
                metadata.set_wrap(true);
                metadata.set_selectable(true);
                let usage = gtk::ProgressBar::new();
                usage.add_css_class("volume-usage");
                usage.set_show_text(true);
                usage.update_property(&[gtk::accessible::Property::Label("Filesystem space used")]);
                root.append(&title);
                root.append(&metadata);
                root.append(&usage);
                self.volumes.append(&root);
                rows.insert(
                    name.clone(),
                    VolumeWidgets {
                        root,
                        title,
                        metadata,
                        usage,
                    },
                );
            }
            if let Some(row) = rows.get(&name) {
                let used = dbus_decode::unsigned(volume, "used_bytes");
                let available = dbus_decode::unsigned(volume, "available_bytes");
                let total = dbus_decode::unsigned(volume, "filesystem_size_bytes")
                    .filter(|total| *total > 0);
                let mounts = dbus_decode::strings(volume, "mount_points").unwrap_or_default();
                row.title
                    .set_text(&dbus_decode::string(volume, "label").unwrap_or(name));
                row.metadata.set_text(&format!(
                    "{} · {}\nPartition size: {} · Filesystem: {} total, {} used, {} available\n{}",
                    dbus_decode::string(volume, "filesystem")
                        .unwrap_or_else(|| "Filesystem unavailable".into()),
                    if mounts.is_empty() {
                        "Not mounted".into()
                    } else {
                        format!("Mounted at {}", mounts.join(", "))
                    },
                    dbus_decode::unsigned(volume, "size_bytes")
                        .map(byte_size)
                        .unwrap_or_else(|| "Unavailable".into()),
                    total.map(byte_size).unwrap_or_else(|| "Unavailable".into()),
                    used.map(byte_size).unwrap_or_else(|| "Unavailable".into()),
                    available
                        .map(byte_size)
                        .unwrap_or_else(|| "Unavailable".into()),
                    dbus_decode::string(volume, "status").unwrap_or_else(|| "Unknown".into())
                ));
                let fraction = volume_usage_fraction(used, available, total);
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

fn section_title(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("dashboard-panel-title");
    label
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

fn volume_usage_fraction(
    used: Option<u64>,
    available: Option<u64>,
    total: Option<u64>,
) -> Option<f64> {
    let (used, available, total) = (used?, available?, total?);
    let total = (total > 0).then_some(total)?;
    if used.checked_add(available)? > total {
        return None;
    }
    Some((used as f64 / total as f64).clamp(0.0, 1.0))
}

fn total_capacity(drives: &[StorageDrive]) -> Option<u64> {
    drives
        .iter()
        .map(|drive| drive.capacity_bytes)
        .try_fold(0_u64, |total, capacity| {
            Some(total.saturating_add(capacity?))
        })
}

fn age_label(sampled_at_ms: Option<u64>, now_ms: u64) -> String {
    sampled_at_ms
        .filter(|sampled| *sampled <= now_ms)
        .map(|sampled| match now_ms.saturating_sub(sampled) / 1000 {
            0 => "just now".to_string(),
            seconds => format!("{seconds}s ago"),
        })
        .unwrap_or_else(|| "time unavailable".into())
}

fn smart_attribute_label(key: &str) -> String {
    match key {
        "critical_warning" => "Critical warning flags".into(),
        "available_spare" => "Available spare".into(),
        "avail_spare" => "Available spare".into(),
        "spare_thresh" | "available_spare_threshold" => "Available spare threshold".into(),
        "percentage_used" => "Percentage used".into(),
        "percent_used" => "Percentage used".into(),
        "data_units_read" => "Data units read".into(),
        "data_units_written" => "Data units written".into(),
        "total_data_read" => "Total data read".into(),
        "total_data_written" => "Total data written".into(),
        "power_on_hours" => "Power-on hours".into(),
        "power_cycles" => "Power cycles".into(),
        "unsafe_shutdowns" => "Unsafe shutdowns".into(),
        "media_errors" => "Media/data integrity errors".into(),
        "error_log_entries" => "Error-information log entries".into(),
        "num_err_log_entries" => "Error-information log entries".into(),
        _ => key.replace('_', " "),
    }
}

fn is_advanced_smart_attribute(key: &str) -> bool {
    matches!(
        key,
        "vendor_attribute"
            | "controller_busy_time"
            | "ctrl_busy_time"
            | "warning_temp_time"
            | "critical_comp_time"
            | "host_reads"
            | "host_writes"
    ) || key.starts_with("ata_attribute_")
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
    fn drive_order_survives_telemetry_and_reordering_and_removal() {
        let initial = vec!["b".to_string(), "a".to_string()];
        assert_eq!(
            reconcile_order(&initial, &["a".into(), "b".into(), "c".into()]),
            vec!["b", "a", "c"]
        );
        assert_eq!(reconcile_order(&initial, &["a".into()]), vec!["a"]);
    }

    #[test]
    fn diagnostics_are_attributed_by_stable_drive_id() {
        let snapshot = StorageSnapshot {
            drives: vec![
                StorageDrive {
                    id: "other".into(),
                    health_source: "wrong".into(),
                    ..StorageDrive::default()
                },
                StorageDrive {
                    id: "target".into(),
                    health_source: "correct".into(),
                    ..StorageDrive::default()
                },
            ],
            ..StorageSnapshot::default()
        };
        assert_eq!(
            diagnostic_drive(&snapshot, "target").map(|d| d.health_source.as_str()),
            Some("correct")
        );
        assert!(diagnostic_drive(&snapshot, "missing").is_none());
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
        assert_eq!(volume_usage_fraction(Some(5), None, Some(10)), None);
        assert_eq!(volume_usage_fraction(Some(5), Some(5), Some(10)), Some(0.5));
        assert_eq!(volume_usage_fraction(Some(11), Some(0), Some(10)), None);
        assert_eq!(volume_usage_fraction(Some(8), Some(5), Some(10)), None);
    }

    #[test]
    fn capacity_summary_does_not_turn_missing_data_into_zero() {
        assert_eq!(total_capacity(&[]), Some(0));
        assert_eq!(
            total_capacity(&[
                StorageDrive {
                    capacity_bytes: Some(100),
                    ..StorageDrive::default()
                },
                StorageDrive {
                    capacity_bytes: None,
                    ..StorageDrive::default()
                },
            ]),
            None
        );
        assert_eq!(
            total_capacity(&[
                StorageDrive {
                    capacity_bytes: Some(100),
                    ..StorageDrive::default()
                },
                StorageDrive {
                    capacity_bytes: Some(200),
                    ..StorageDrive::default()
                },
            ]),
            Some(300)
        );
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

    #[test]
    fn smart_labels_do_not_infer_drive_lifetime() {
        assert_eq!(smart_attribute_label("percentage_used"), "Percentage used");
        assert_eq!(
            smart_attribute_label("error_log_entries"),
            "Error-information log entries"
        );
        assert!(!smart_attribute_label("percentage_used").contains("remaining"));
        assert!(is_advanced_smart_attribute("ata_attribute_241"));
        assert!(!is_advanced_smart_attribute("power_on_hours"));
    }

    #[test]
    fn freshness_label_handles_missing_and_future_samples() {
        assert_eq!(age_label(None, 1000), "time unavailable");
        assert_eq!(age_label(Some(2000), 1000), "time unavailable");
        assert_eq!(age_label(Some(500), 1000), "just now");
    }
}
