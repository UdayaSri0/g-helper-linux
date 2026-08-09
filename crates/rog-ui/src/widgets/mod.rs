use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::cairo::Context;
use gtk4 as gtk;

#[derive(Debug, Clone)]
pub struct MetricCard {
    root: gtk::Box,
    value_label: gtk::Label,
    unit_label: gtk::Label,
    subtitle_label: gtk::Label,
    chip_label: gtk::Label,
}

impl MetricCard {
    pub fn new(title: &str) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.add_css_class("metric-card");
        root.set_hexpand(true);
        root.set_size_request(210, -1);

        let title_label = gtk::Label::new(Some(title));
        title_label.set_xalign(0.0);
        title_label.add_css_class("metric-card-title");

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
        subtitle_label.add_css_class("metric-subtitle");
        subtitle_label.set_visible(false);

        let chip_label = gtk::Label::new(None);
        chip_label.set_xalign(0.0);
        chip_label.set_halign(gtk::Align::Start);
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

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn add_css_class(&self, class: &str) {
        self.root.add_css_class(class);
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

    pub fn set_status_chip(&self, chip_text: Option<&str>) {
        if let Some(chip_text) = chip_text {
            self.chip_label.set_text(chip_text);
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
    let page = gtk::Box::new(gtk::Orientation::Vertical, 24);
    page.set_margin_top(24);
    page.set_margin_bottom(32);
    page.set_margin_start(24);
    page.set_margin_end(24);
    page.set_hexpand(true);
    page
}

#[derive(Debug)]
struct HistoryGraphState {
    title: String,
    samples: Vec<f32>,
    maximum: f32,
    unit: String,
    color: (f64, f64, f64),
}

#[derive(Clone)]
pub struct HistoryGraph {
    area: gtk::DrawingArea,
    state: Rc<RefCell<HistoryGraphState>>,
}

impl HistoryGraph {
    pub fn new(title: &str, maximum: f32, unit: &str, color: (f64, f64, f64)) -> Self {
        let area = gtk::DrawingArea::new();
        area.set_content_width(420);
        area.set_content_height(180);
        area.set_hexpand(true);
        area.add_css_class("history-graph");

        let state = Rc::new(RefCell::new(HistoryGraphState {
            title: title.to_string(),
            samples: Vec::new(),
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
            self.area.queue_draw();
        }
    }
}

fn draw_history_graph(ctx: &Context, width: f64, height: f64, state: &HistoryGraphState) {
    let left = 16.0;
    let right = 16.0;
    let top = 34.0;
    let bottom = 30.0;
    let plot_width = (width - left - right).max(1.0);
    let plot_height = (height - top - bottom).max(1.0);

    ctx.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Bold,
    );
    ctx.set_font_size(12.0);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.76);
    ctx.move_to(left, 20.0);
    ctx.show_text(&state.title).ok();

    ctx.set_line_width(1.0);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.08);
    for line in 0..=4 {
        let y = top + plot_height * line as f64 / 4.0;
        ctx.move_to(left, y);
        ctx.line_to(left + plot_width, y);
    }
    ctx.stroke().ok();

    if state.samples.len() > 1 {
        let max = state.maximum.max(1.0) as f64;
        for (index, sample) in state.samples.iter().enumerate() {
            let x = left + plot_width * index as f64 / (state.samples.len() - 1) as f64;
            let y = top + plot_height * (1.0 - (*sample as f64 / max).clamp(0.0, 1.0));
            if index == 0 {
                ctx.move_to(x, y);
            } else {
                ctx.line_to(x, y);
            }
        }
        let (red, green, blue) = state.color;
        ctx.set_source_rgba(red, green, blue, 0.92);
        ctx.set_line_width(2.5);
        ctx.stroke().ok();
    }

    let latest = state
        .samples
        .last()
        .map(|value| format!("{value:.0} {}", state.unit))
        .unwrap_or_else(|| "Waiting for telemetry".to_string());
    ctx.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Normal,
    );
    ctx.set_font_size(11.0);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.62);
    ctx.move_to(left, height - 9.0);
    ctx.show_text(&latest).ok();
}
