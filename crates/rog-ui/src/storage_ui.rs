use adw::prelude::*;
use gtk4 as gtk;
use rog_core::dbus_keys::storage as key;
use std::cell::RefCell;
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
    summary: MetricCard,
    status: gtk::Label,
    drives: gtk::Box,
    details: gtk::Box,
    selected_id: Rc<RefCell<Option<String>>>,
    request_diagnostics: Rc<dyn Fn()>,
}

impl StoragePage {
    pub(crate) fn new(request_diagnostics: impl Fn() + 'static) -> Self {
        let body = page_container();
        body.append(&page_header(
            "Storage",
            "Physical drives, live I/O, volumes, and available health data.",
        ));
        let summary = MetricCard::new("Physical drives");
        summary.set_icon_name("rog-storage-symbolic");
        body.append(summary.widget());
        let status = gtk::Label::new(Some("Detecting physical drives…"));
        status.set_xalign(0.0);
        status.set_wrap(true);
        body.append(&status);
        let drives = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.append(&drives);
        let details = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.append(&details);
        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(1260);
        clamp.set_tightening_threshold(900);
        clamp.set_child(Some(&body));
        let root = gtk::ScrolledWindow::new();
        root.set_vexpand(true);
        root.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        root.set_child(Some(&clamp));
        Self {
            root,
            summary,
            status,
            drives,
            details,
            selected_id: Rc::new(RefCell::new(None)),
            request_diagnostics: Rc::new(request_diagnostics),
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
        let diagnostics = diagnostics.filter(|state| {
            state
                .sampled_at_ms
                .is_some_and(|sampled| now_ms.saturating_sub(sampled) <= 15 * 60_000)
        });
        let Some(snapshot) = snapshot else {
            self.summary.set_value("—");
            self.status.set_text("Waiting for the storage service…");
            clear(&self.drives);
            clear(&self.details);
            return;
        };
        let fresh = snapshot
            .sampled_at_ms
            .is_some_and(|sampled| now_ms.saturating_sub(sampled) <= 10_000);
        let available = snapshot.status.eq_ignore_ascii_case("available") && fresh;
        if !available {
            self.summary.set_value("—");
            self.status.set_text(
                if !fresh && snapshot.status.eq_ignore_ascii_case("available") {
                    "Last storage reading is stale."
                } else {
                    &snapshot.status
                },
            );
            clear(&self.drives);
            clear(&self.details);
            return;
        }
        let count = snapshot.drives.len();
        self.summary.set_value(count.to_string());
        self.summary.set_subtitle(Some(&format!(
            "{} raw physical capacity",
            byte_size(
                snapshot
                    .drives
                    .iter()
                    .filter_map(|drive| drive.capacity_bytes)
                    .fold(0_u64, u64::saturating_add)
            )
        )));
        if count == 0 {
            self.status.set_text("No physical drives detected.");
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
            self.status.set_text(&format!(
                "Monitoring available · {warnings} health warning(s) · Read {} · Write {}",
                rate(total_read),
                rate(total_write)
            ));
        }
        let selection_missing = self
            .selected_id
            .borrow()
            .as_ref()
            .is_none_or(|id| !snapshot.drives.iter().any(|drive| &drive.id == id));
        if selection_missing {
            *self.selected_id.borrow_mut() = snapshot.drives.first().map(|drive| drive.id.clone());
        }
        clear(&self.drives);
        for drive in &snapshot.drives {
            let button = gtk::Button::new();
            button.add_css_class("flat");
            button.update_property(&[gtk::accessible::Property::Label(&format!(
                "Show details for {}",
                drive.model
            ))]);
            let card = MetricCard::new(if drive.model.is_empty() {
                &drive.name
            } else {
                &drive.model
            });
            card.set_icon_name("rog-storage-symbolic");
            card.set_value(
                drive
                    .capacity_bytes
                    .map(byte_size)
                    .unwrap_or_else(|| "Unavailable".into()),
            );
            let temperature = drive.temperature_c.filter(|_| {
                drive
                    .temperature_sampled_at_ms
                    .is_some_and(|sampled| now_ms.saturating_sub(sampled) <= 10_000)
            });
            card.set_subtitle(Some(&format!(
                "{} · {} · {} · Read {} · Write {}",
                drive.kind,
                drive.transport,
                temperature
                    .map(|value| format!("{value:.1} °C"))
                    .unwrap_or_else(|| "Temperature unavailable".into()),
                rate(drive.read_bytes_per_sec),
                rate(drive.write_bytes_per_sec)
            )));
            card.set_status_chip(Some(&drive.health));
            button.set_child(Some(card.widget()));
            let id = drive.id.clone();
            let details = self.details.clone();
            let selected = self.selected_id.clone();
            let selected_drive = drive.clone();
            let diagnostic_drive = diagnostics
                .and_then(|snapshot| snapshot.drives.iter().find(|item| item.id == id))
                .cloned();
            let request_diagnostics = self.request_diagnostics.clone();
            button.connect_clicked(move |_| {
                *selected.borrow_mut() = Some(id.clone());
                clear(&details);
                render_details(
                    &details,
                    &selected_drive,
                    diagnostic_drive.as_ref(),
                    now_ms,
                    request_diagnostics.clone(),
                );
            });
            self.drives.append(&button);
        }
        clear(&self.details);
        if let Some(drive) = snapshot
            .drives
            .iter()
            .find(|drive| Some(&drive.id) == self.selected_id.borrow().as_ref())
        {
            let diagnostic_drive = diagnostics
                .and_then(|snapshot| snapshot.drives.iter().find(|item| item.id == drive.id));
            render_details(
                &self.details,
                drive,
                diagnostic_drive,
                now_ms,
                self.request_diagnostics.clone(),
            );
        }
    }
}

fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn rate(bytes_per_sec: Option<u64>) -> String {
    bytes_per_sec
        .map(|value| format!("{:.1} MB/s", value as f64 / 1_000_000.0))
        .unwrap_or_else(|| "Unavailable".into())
}

fn redacted_diagnostics(drive: &StorageDrive, now_ms: u64) -> String {
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
        rate(drive.read_bytes_per_sec),
        rate(drive.write_bytes_per_sec),
        temperature.map(|value| format!("{value:.1} °C")).unwrap_or_else(|| "Unavailable".into()),
    )
}

fn detail_row(parent: &gtk::Box, title: &str, value: &str) {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(value)
        .build();
    parent.append(&row);
}

fn render_details(
    parent: &gtk::Box,
    drive: &StorageDrive,
    diagnostics: Option<&StorageDrive>,
    now_ms: u64,
    request_diagnostics: Rc<dyn Fn()>,
) {
    let title = gtk::Label::new(Some(if drive.model.is_empty() {
        &drive.name
    } else {
        &drive.model
    }));
    title.set_xalign(0.0);
    title.add_css_class("page-title");
    title.set_wrap(true);
    parent.append(&title);
    for (heading, rows) in [
        (
            "Overview",
            vec![
                ("Type", drive.kind.clone()),
                ("Transport", drive.transport.clone()),
                (
                    "Capacity",
                    drive
                        .capacity_bytes
                        .map(byte_size)
                        .unwrap_or_else(|| "Unavailable".into()),
                ),
                (
                    "Temperature",
                    drive
                        .temperature_c
                        .filter(|_| {
                            drive
                                .temperature_sampled_at_ms
                                .is_some_and(|sampled| now_ms.saturating_sub(sampled) <= 10_000)
                        })
                        .map(|value| format!("{value:.1} °C"))
                        .unwrap_or_else(|| "Unavailable".into()),
                ),
            ],
        ),
        (
            "Performance",
            vec![
                ("Read", rate(drive.read_bytes_per_sec)),
                ("Write", rate(drive.write_bytes_per_sec)),
                (
                    "Read IOPS",
                    drive
                        .read_iops
                        .map(|value| format!("{value:.0}"))
                        .unwrap_or_else(|| "Unavailable".into()),
                ),
                (
                    "Write IOPS",
                    drive
                        .write_iops
                        .map(|value| format!("{value:.0}"))
                        .unwrap_or_else(|| "Unavailable".into()),
                ),
            ],
        ),
        (
            "SMART Health",
            vec![
                ("Assessment", drive.health.clone()),
                (
                    "Evidence",
                    if drive.health_reason.is_empty() {
                        "Unavailable".into()
                    } else {
                        drive.health_reason.clone()
                    },
                ),
            ],
        ),
    ] {
        let group = adw::PreferencesGroup::builder().title(heading).build();
        for (title, value) in rows {
            let row = adw::ActionRow::builder()
                .title(title)
                .subtitle(value)
                .build();
            group.add(&row);
        }
        parent.append(&group);
    }
    let read_history = HistoryGraph::new("Read throughput", 0.0, 1.0, "MB/s", (0.30, 0.64, 1.0));
    let write_history = HistoryGraph::new("Write throughput", 0.0, 1.0, "MB/s", (0.49, 0.88, 0.70));
    read_history.set_samples(&history_rates(&drive.history, "read_bytes_per_sec"));
    write_history.set_samples(&history_rates(&drive.history, "write_bytes_per_sec"));
    parent.append(read_history.widget());
    parent.append(write_history.widget());
    let temperature_history = drive
        .history
        .iter()
        .map(|point| {
            dbus_decode::float(point, "temperature_c")
                .filter(|value| value.is_finite())
                .map(|value| value as f32)
                .unwrap_or(f32::NAN)
        })
        .collect::<Vec<_>>();
    if temperature_history
        .iter()
        .filter(|value| value.is_finite())
        .count()
        >= 2
    {
        let temperature_graph =
            HistoryGraph::new("Drive temperature", 0.0, 50.0, "°C", (0.96, 0.68, 0.38));
        temperature_graph.set_samples(&temperature_history);
        parent.append(temperature_graph.widget());
    }
    let diagnostics_button = gtk::Button::with_label("Refresh SMART details");
    diagnostics_button.set_halign(gtk::Align::Start);
    diagnostics_button.update_property(&[gtk::accessible::Property::Label(
        "Read available SMART details for physical drives",
    )]);
    diagnostics_button.connect_clicked(move |_| request_diagnostics());
    parent.append(&diagnostics_button);
    let copy_button = gtk::Button::with_label("Copy redacted diagnostics");
    copy_button.set_halign(gtk::Align::Start);
    copy_button.update_property(&[gtk::accessible::Property::Label(
        "Copy drive diagnostics without serial numbers or mount paths",
    )]);
    let copied_text = redacted_diagnostics(drive, now_ms);
    copy_button.connect_clicked(move |_| {
        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(&copied_text);
        }
    });
    parent.append(&copy_button);
    if let Some(diagnostics) = diagnostics {
        if !diagnostics.health_source.is_empty() {
            detail_row(parent, "SMART source", &diagnostics.health_source);
        }
        let group = adw::PreferencesGroup::builder()
            .title("SMART attributes")
            .build();
        if diagnostics.health_attributes.is_empty() {
            group.add(
                &adw::ActionRow::builder()
                    .title("No detailed attributes available")
                    .build(),
            );
        } else {
            for attribute in &diagnostics.health_attributes {
                let key =
                    dbus_decode::string(attribute, "key").unwrap_or_else(|| "Attribute".into());
                let value =
                    dbus_decode::string(attribute, "value").unwrap_or_else(|| "Unavailable".into());
                let unit = dbus_decode::string(attribute, "unit").unwrap_or_default();
                let warning = dbus_decode::boolean(attribute, "warning").unwrap_or(false);
                let row = adw::ActionRow::builder()
                    .title(key)
                    .subtitle(format!("{value} {unit}"))
                    .build();
                if warning {
                    row.add_css_class("status-chip-warning");
                }
                group.add(&row);
            }
        }
        parent.append(&group);
    }
    let volumes = adw::PreferencesGroup::builder()
        .title("Partitions & Volumes")
        .build();
    if drive.volumes.is_empty() {
        let row = adw::ActionRow::builder()
            .title("No attributable volumes reported")
            .build();
        volumes.add(&row);
    } else {
        for volume in &drive.volumes {
            let name = dbus_decode::string(volume, "name").unwrap_or_else(|| "Volume".into());
            let used = dbus_decode::unsigned(volume, "used_bytes");
            let available = dbus_decode::unsigned(volume, "available_bytes");
            let mounts = dbus_decode::strings(volume, "mount_points").unwrap_or_default();
            let subtitle = format!(
                "{} · {} · {} · Used {} · Available {}{}{}",
                dbus_decode::unsigned(volume, "size_bytes")
                    .map(byte_size)
                    .unwrap_or_else(|| "Size unavailable".into()),
                dbus_decode::string(volume, "filesystem")
                    .unwrap_or_else(|| "Filesystem unavailable".into()),
                dbus_decode::string(volume, "status").unwrap_or_else(|| "Unknown".into()),
                used.map(byte_size).unwrap_or_else(|| "Unavailable".into()),
                available
                    .map(byte_size)
                    .unwrap_or_else(|| "Unavailable".into()),
                dbus_decode::string(volume, "label")
                    .map(|label| format!(" · Label {label}"))
                    .unwrap_or_default(),
                if mounts.is_empty() {
                    String::new()
                } else {
                    format!(" · Mounted at {}", mounts.join(", "))
                },
            );
            let row = adw::ActionRow::builder()
                .title(name)
                .subtitle(subtitle)
                .build();
            if let (Some(used), Some(available)) = (used, available) {
                let denominator = used.saturating_add(available);
                if denominator > 0 {
                    let progress = gtk::ProgressBar::new();
                    progress.set_size_request(100, -1);
                    progress.set_fraction(used as f64 / denominator as f64);
                    progress.update_property(&[gtk::accessible::Property::Label(
                        "Filesystem space used",
                    )]);
                    row.add_suffix(&progress);
                }
            }
            volumes.add(&row);
        }
    }
    parent.append(&volumes);
    let technical = gtk::Expander::new(Some("Technical details"));
    let data = gtk::Box::new(gtk::Orientation::Vertical, 4);
    for (key, value) in [
        ("Device", drive.name.as_str()),
        ("Model", drive.model.as_str()),
        (
            "Manufacturer",
            drive.manufacturer.as_deref().unwrap_or("Unavailable"),
        ),
        ("Type", drive.kind.as_str()),
        ("Transport", drive.transport.as_str()),
        (
            "Firmware",
            drive.firmware.as_deref().unwrap_or("Unavailable"),
        ),
    ] {
        detail_row(&data, key, value);
    }
    detail_row(
        &data,
        "Logical block size",
        &drive
            .logical_block_size
            .map(byte_size)
            .unwrap_or_else(|| "Unavailable".into()),
    );
    detail_row(
        &data,
        "Physical block size",
        &drive
            .physical_block_size
            .map(byte_size)
            .unwrap_or_else(|| "Unavailable".into()),
    );
    for sensor in &drive.temperatures {
        if let Some(value) = dbus_decode::float(sensor, "celsius").filter(|_| {
            dbus_decode::unsigned(sensor, "sampled_at_ms")
                .is_some_and(|sampled| now_ms.saturating_sub(sampled) <= 10_000)
        }) {
            let label =
                dbus_decode::string(sensor, "label").unwrap_or_else(|| "Temperature".into());
            let source =
                dbus_decode::string(sensor, "source").unwrap_or_else(|| "Unknown source".into());
            detail_row(&data, &label, &format!("{value:.1} °C · {source}"));
        }
    }
    technical.set_child(Some(&data));
    parent.append(&technical);
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
        let report = redacted_diagnostics(&drive, 0);
        assert!(report.contains("Example SSD"));
        assert!(!report.contains("SERIAL-SECRET"));
        assert!(!report.contains("/dev/nvme0n1"));
        assert!(!report.contains("private/path"));
    }
}
