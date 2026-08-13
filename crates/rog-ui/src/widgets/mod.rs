use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::cairo::Context;
use gtk4 as gtk;

#[derive(Debug, Clone)]
pub struct MetricCard {
    root: gtk::Box,
    icon: gtk::Image,
    value_label: gtk::Label,
    unit_label: gtk::Label,
    subtitle_label: gtk::Label,
    chip_label: gtk::Label,
}

impl MetricCard {
    pub fn new(title: &str) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.add_css_class("metric-card");
        root.set_hexpand(true);

        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        let icon = gtk::Image::new();
        icon.set_pixel_size(16);
        icon.add_css_class("metric-card-icon");
        icon.set_visible(false);

        let title_label = gtk::Label::new(Some(title));
        title_label.set_xalign(0.0);
        title_label.set_hexpand(true);
        title_label.add_css_class("metric-card-title");
        heading.append(&icon);
        heading.append(&title_label);

        let value_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        value_row.set_halign(gtk::Align::Start);
        value_row.set_valign(gtk::Align::End);

        let value_label = gtk::Label::new(Some("—"));
        value_label.set_xalign(0.0);
        value_label.add_css_class("metric-value");

        let unit_label = gtk::Label::new(None);
        unit_label.set_xalign(0.0);
        unit_label.add_css_class("metric-unit");
        unit_label.set_visible(false);

        value_row.append(&value_label);
        value_row.append(&unit_label);

        let subtitle_label = gtk::Label::new(None);
        subtitle_label.set_xalign(0.0);
        subtitle_label.set_wrap(true);
        subtitle_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        subtitle_label.set_max_width_chars(32);
        subtitle_label.add_css_class("metric-subtitle");
        subtitle_label.set_visible(false);

        let chip_label = gtk::Label::new(None);
        chip_label.set_xalign(0.0);
        chip_label.set_halign(gtk::Align::Start);
        chip_label.add_css_class("status-chip");
        chip_label.set_visible(false);

        root.append(&heading);
        root.append(&value_row);
        root.append(&subtitle_label);
        root.append(&chip_label);

        Self {
            root,
            icon,
            value_label,
            unit_label,
            subtitle_label,
            chip_label,
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn add_css_class(&self, class: &str) {
        self.root.add_css_class(class);
    }

    pub fn set_icon_name(&self, icon_name: &str) {
        self.icon.set_icon_name(Some(icon_name));
        self.icon.set_visible(true);
    }

    pub fn set_value(&self, value: impl AsRef<str>) {
        self.value_label.set_text(value.as_ref());
    }

    pub fn set_unit(&self, unit: Option<&str>) {
        if let Some(unit) = unit {
            self.unit_label.set_text(unit);
            self.unit_label.set_visible(true);
        } else {
            self.unit_label.set_visible(false);
            self.unit_label.set_text("");
        }
    }

    pub fn set_subtitle(&self, subtitle: Option<&str>) {
        if let Some(subtitle) = subtitle {
            self.subtitle_label.set_text(subtitle);
            self.subtitle_label.set_visible(true);
        } else {
            self.subtitle_label.set_text("");
            self.subtitle_label.set_visible(false);
        }
    }

    pub fn set_subtitle_width_chars(&self, width_chars: i32) {
        self.subtitle_label.set_max_width_chars(width_chars);
    }

    pub fn set_status_chip(&self, chip_text: Option<&str>) {
        for class in [
            "status-chip-success",
            "status-chip-info",
            "status-chip-warning",
            "status-chip-error",
        ] {
            self.chip_label.remove_css_class(class);
        }
        if let Some(chip_text) = chip_text {
            self.chip_label.set_text(chip_text);
            let normalized = chip_text.to_ascii_lowercase();
            let class = if normalized.contains("unavailable")
                || normalized.contains("error")
                || normalized.contains("disconnected")
            {
                "status-chip-error"
            } else if normalized.contains("warning") || normalized.contains("attention") {
                "status-chip-warning"
            } else if normalized.contains("available")
                || normalized.contains("connected")
                || normalized.contains("full")
            {
                "status-chip-success"
            } else {
                "status-chip-info"
            };
            self.chip_label.add_css_class(class);
            self.chip_label.set_visible(true);
        } else {
            self.chip_label.set_text("");
            self.chip_label.set_visible(false);
        }
    }
}

pub fn page_header(title: &str, subtitle: &str) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
    root.add_css_class("page-header");

    let title_label = gtk::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.add_css_class("page-title");

    let subtitle_label = gtk::Label::new(Some(subtitle));
    subtitle_label.set_xalign(0.0);
    subtitle_label.set_wrap(true);
    subtitle_label.add_css_class("page-subtitle");

    root.append(&title_label);
    root.append(&subtitle_label);
    root
}

pub fn page_header_group(title: &str, subtitle: &str) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.add(&page_header(title, subtitle));
    group
}

pub fn page_container() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 20);
    page.set_margin_top(20);
    page.set_margin_bottom(28);
    page.set_margin_start(24);
    page.set_margin_end(24);
    page.set_hexpand(true);
    page
}

#[derive(Debug)]
struct HistoryGraphState {
    title: String,
    samples: Vec<f32>,
    minimum: f32,
    maximum: f32,
    unit: String,
    color: (f64, f64, f64),
}

#[derive(Clone)]
pub struct HistoryGraph {
    area: gtk::DrawingArea,
    state: Rc<RefCell<HistoryGraphState>>,
}

#[derive(Clone)]
pub struct Sparkline {
    area: gtk::DrawingArea,
    samples: Rc<RefCell<Vec<f32>>>,
}

impl Sparkline {
    pub fn new(color: (f64, f64, f64)) -> Self {
        let area = gtk::DrawingArea::new();
        area.set_content_height(58);
        area.set_hexpand(true);
        area.add_css_class("metric-sparkline");
        let samples = Rc::new(RefCell::new(Vec::<f32>::new()));
        let draw_samples = samples.clone();
        area.set_draw_func(move |_, ctx, width, height| {
            let samples = draw_samples.borrow();
            if samples.len() < 2 {
                return;
            }
            let width = width as f64;
            let height = height as f64;
            let bottom = height - 3.0;
            let points = samples
                .iter()
                .enumerate()
                .map(|(index, sample)| {
                    let x = width * index as f64 / (samples.len() - 1) as f64;
                    let y = bottom - (bottom - 3.0) * (*sample as f64 / 100.0).clamp(0.0, 1.0);
                    (x, y)
                })
                .collect::<Vec<_>>();
            let (red, green, blue) = color;
            ctx.move_to(points[0].0, bottom);
            for (x, y) in &points {
                ctx.line_to(*x, *y);
            }
            ctx.line_to(points[points.len() - 1].0, bottom);
            ctx.close_path();
            ctx.set_source_rgba(red, green, blue, 0.13);
            ctx.fill().ok();
            ctx.move_to(points[0].0, points[0].1);
            for (x, y) in points.iter().skip(1) {
                ctx.line_to(*x, *y);
            }
            ctx.set_source_rgba(red, green, blue, 0.82);
            ctx.set_line_width(2.0);
            ctx.stroke().ok();
        });
        Self { area, samples }
    }

    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    pub fn set_samples(&self, samples: &[f32]) {
        let changed = {
            let current = self.samples.borrow();
            current.len() != samples.len() || current.last().copied() != samples.last().copied()
        };
        if changed {
            *self.samples.borrow_mut() = samples.to_vec();
            if self.area.is_mapped() {
                self.area.queue_draw();
            }
        }
    }
}

impl HistoryGraph {
    pub fn new(
        title: &str,
        minimum: f32,
        maximum: f32,
        unit: &str,
        color: (f64, f64, f64),
    ) -> Self {
        let area = gtk::DrawingArea::new();
        area.set_content_height(190);
        area.set_hexpand(true);
        area.add_css_class("history-graph");

        let state = Rc::new(RefCell::new(HistoryGraphState {
            title: title.to_string(),
            samples: Vec::new(),
            minimum,
            maximum,
            unit: unit.to_string(),
            color,
        }));
        let draw_state = state.clone();
        area.set_draw_func(move |_, ctx, width, height| {
            draw_history_graph(ctx, width as f64, height as f64, &draw_state.borrow());
        });

        Self { area, state }
    }

    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    pub fn set_samples(&self, samples: &[f32]) {
        let changed = {
            let current = self.state.borrow();
            current.samples.len() != samples.len()
                || current.samples.last().copied() != samples.last().copied()
        };
        if changed {
            self.state.borrow_mut().samples = samples.to_vec();
            if self.area.is_mapped() {
                self.area.queue_draw();
            }
        }
    }
}

fn draw_history_graph(ctx: &Context, width: f64, height: f64, state: &HistoryGraphState) {
    let left = 18.0;
    let right = 18.0;
    let top = 48.0;
    let bottom = 28.0;
    let plot_width = (width - left - right).max(1.0);
    let plot_height = (height - top - bottom).max(1.0);

    ctx.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Bold,
    );
    ctx.set_font_size(12.5);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.76);
    ctx.move_to(left, 21.0);
    ctx.show_text(&state.title).ok();

    let current = state.samples.last().copied();
    let minimum_sample = state.samples.iter().copied().reduce(f32::min);
    let maximum_sample = state.samples.iter().copied().reduce(f32::max);
    let summary = match (current, minimum_sample, maximum_sample) {
        (Some(current), Some(minimum), Some(maximum)) => format!(
            "Now {current:.0} {}   Min {minimum:.0}   Max {maximum:.0}",
            state.unit
        ),
        _ => "Waiting for telemetry".to_string(),
    };
    ctx.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Normal,
    );
    ctx.set_font_size(10.5);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.56);
    ctx.move_to(left, 38.0);
    ctx.show_text(&summary).ok();

    ctx.set_line_width(1.0);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.08);
    for line in 0..=4 {
        let y = top + plot_height * line as f64 / 4.0;
        ctx.move_to(left, y);
        ctx.line_to(left + plot_width, y);
    }
    ctx.stroke().ok();

    if state.samples.len() > 1 {
        let observed_max = state
            .samples
            .iter()
            .copied()
            .reduce(f32::max)
            .unwrap_or(state.maximum);
        let maximum = if state.minimum > 0.0 {
            state.maximum.max(observed_max + 5.0)
        } else {
            state.maximum
        };
        let range = (maximum - state.minimum).max(1.0) as f64;
        let mut points = Vec::with_capacity(state.samples.len());
        for (index, sample) in state.samples.iter().enumerate() {
            let x = left + plot_width * index as f64 / (state.samples.len() - 1) as f64;
            let normalized = ((*sample - state.minimum) as f64 / range).clamp(0.0, 1.0);
            points.push((x, top + plot_height * (1.0 - normalized)));
        }
        let (red, green, blue) = state.color;
        ctx.move_to(points[0].0, top + plot_height);
        for (x, y) in &points {
            ctx.line_to(*x, *y);
        }
        ctx.line_to(points[points.len() - 1].0, top + plot_height);
        ctx.close_path();
        ctx.set_source_rgba(red, green, blue, 0.16);
        ctx.fill().ok();

        ctx.move_to(points[0].0, points[0].1);
        for (x, y) in points.iter().skip(1) {
            ctx.line_to(*x, *y);
        }
        ctx.set_source_rgba(red, green, blue, 0.92);
        ctx.set_line_width(2.5);
        ctx.stroke().ok();
    }

    ctx.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Normal,
    );
    ctx.set_font_size(11.0);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.62);
    ctx.move_to(left, height - 9.0);
    ctx.show_text("60 seconds ago").ok();
    let end_label = "Now";
    if let Ok(extents) = ctx.text_extents(end_label) {
        ctx.move_to(left + plot_width - extents.width(), height - 9.0);
        ctx.show_text(end_label).ok();
    }
}
