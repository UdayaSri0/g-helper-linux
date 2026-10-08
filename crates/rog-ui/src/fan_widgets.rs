use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

use gtk::cairo::{Context, FontSlant, FontWeight};
use gtk::prelude::*;
use gtk4 as gtk;

type Color = (f64, f64, f64);
type CurvePoints = Vec<(u8, u8)>;
type CurveChangeHandler = Rc<RefCell<Option<Box<dyn Fn(CurvePoints)>>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotorStatus {
    Telemetry,
    ReadOnly,
    Controllable,
    Warning,
    Error,
}

#[derive(Debug)]
struct FanRotorState {
    rpm: Option<u32>,
    angle: f64,
    status: RotorStatus,
    compact: bool,
}

#[derive(Clone)]
pub struct FanRotor {
    area: gtk::DrawingArea,
    state: Rc<RefCell<FanRotorState>>,
}

impl FanRotor {
    pub fn new(compact: bool) -> Self {
        let area = gtk::DrawingArea::new();
        area.set_content_width(if compact { 112 } else { 164 });
        area.set_content_height(if compact { 112 } else { 164 });
        area.add_css_class("fan-rotor-widget");
        area.set_tooltip_text(Some(
            "Fan rotor animation is visually scaled from RPM telemetry.",
        ));

        let state = Rc::new(RefCell::new(FanRotorState {
            rpm: None,
            angle: 0.0,
            status: RotorStatus::ReadOnly,
            compact,
        }));
        let draw_state = state.clone();
        area.set_draw_func(move |_, ctx, width, height| {
            let state = draw_state.borrow();
            draw_fan_rotor(ctx, width as f64, height as f64, &state);
        });

        Self { area, state }
    }

    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    pub fn set_state(&self, rpm: Option<u32>, status: RotorStatus) {
        let mut state = self.state.borrow_mut();
        if state.rpm == rpm && state.status == status {
            return;
        }
        state.rpm = rpm;
        state.status = status;
        if rpm.unwrap_or(0) == 0 {
            state.angle = 0.0;
        }
        drop(state);
        if self.area.is_mapped() {
            self.area.queue_draw();
        }
    }

    pub fn tick(&self, delta_seconds: f64, reduced_motion: bool) {
        if !self.area.is_mapped() {
            return;
        }
        let mut state = self.state.borrow_mut();
        let rpm = state.rpm.unwrap_or(0) as f64;
        if rpm <= 0.0 {
            return;
        }
        let visual_rps = if reduced_motion {
            (rpm / 4500.0).clamp(0.0, 0.8)
        } else {
            (rpm / 1500.0).clamp(0.0, 4.0)
        };
        state.angle = (state.angle + visual_rps * 2.0 * PI * delta_seconds) % (2.0 * PI);
        drop(state);
        self.area.queue_draw();
    }
}

#[derive(Debug)]
struct TempGaugeState {
    temp_c: Option<f32>,
    label: String,
    primary_speed_label: String,
    primary_speed_mhz: Option<u64>,
    secondary_speed_label: Option<String>,
    secondary_speed_mhz: Option<u64>,
    accent: GaugeAccent,
}

#[derive(Debug, Clone, Copy)]
pub enum GaugeAccent {
    Blue,
    Magenta,
}

#[derive(Clone)]
pub struct TempGauge {
    area: gtk::DrawingArea,
    state: Rc<RefCell<TempGaugeState>>,
}

impl TempGauge {
    pub fn new(label: &str, accent: GaugeAccent) -> Self {
        let area = gtk::DrawingArea::new();
        area.set_content_height(230);
        area.set_hexpand(true);
        area.add_css_class("fan-gauge-widget");
        area.set_tooltip_text(Some("Temperature gauge from daemon telemetry."));

        let state = Rc::new(RefCell::new(TempGaugeState {
            temp_c: None,
            label: label.to_string(),
            primary_speed_label: "Clock".to_string(),
            primary_speed_mhz: None,
            secondary_speed_label: None,
            secondary_speed_mhz: None,
            accent,
        }));
        let draw_state = state.clone();
        area.set_draw_func(move |_, ctx, width, height| {
            draw_temperature_gauge(ctx, width as f64, height as f64, &draw_state.borrow());
        });

        Self { area, state }
    }

    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    pub fn set_temp(&self, temp_c: Option<f32>) {
        let mut state = self.state.borrow_mut();
        if state.temp_c == temp_c {
            return;
        }
        state.temp_c = temp_c;
        drop(state);
        if self.area.is_mapped() {
            self.area.queue_draw();
        }
    }

    pub fn set_speed_metrics(
        &self,
        primary_label: &str,
        primary_mhz: Option<u64>,
        secondary_label: Option<&str>,
        secondary_mhz: Option<u64>,
    ) {
        let mut state = self.state.borrow_mut();
        let secondary_label = secondary_label.map(ToString::to_string);
        if state.primary_speed_label == primary_label
            && state.primary_speed_mhz == primary_mhz
            && state.secondary_speed_label == secondary_label
            && state.secondary_speed_mhz == secondary_mhz
        {
            return;
        }
        state.primary_speed_label = primary_label.to_string();
        state.primary_speed_mhz = primary_mhz;
        state.secondary_speed_label = secondary_label;
        state.secondary_speed_mhz = secondary_mhz;
        drop(state);
        if self.area.is_mapped() {
            self.area.queue_draw();
        }
    }
}

#[derive(Debug)]
struct CurvePreviewState {
    points: Vec<(u8, u8)>,
    enabled: bool,
    current_temp_c: Option<f32>,
    selected_point: Option<usize>,
    drag_origin: Option<(f64, f64, u8, u8)>,
}

#[derive(Clone)]
pub struct CurvePreview {
    area: gtk::DrawingArea,
    state: Rc<RefCell<CurvePreviewState>>,
    points_changed: CurveChangeHandler,
}

impl CurvePreview {
    pub fn new() -> Self {
        let area = gtk::DrawingArea::new();
        area.set_content_height(220);
        area.set_hexpand(true);
        area.add_css_class("fan-curve-widget");
        area.set_tooltip_text(Some(
            "Eight-point draft editor. Drag a point or focus the chart and use arrow keys; edits remain local until Apply.",
        ));
        area.set_focusable(true);

        let state = Rc::new(RefCell::new(CurvePreviewState {
            points: vec![
                (35, 15),
                (45, 25),
                (55, 35),
                (65, 50),
                (75, 70),
                (85, 90),
                (90, 100),
                (95, 100),
            ],
            enabled: false,
            current_temp_c: None,
            selected_point: None,
            drag_origin: None,
        }));
        let points_changed = Rc::new(RefCell::new(None::<Box<dyn Fn(CurvePoints)>>));
        let draw_state = state.clone();
        area.set_draw_func(move |_, ctx, width, height| {
            draw_curve_preview(ctx, width as f64, height as f64, &draw_state.borrow());
        });

        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        let drag_state = state.clone();
        let drag_area = area.clone();
        drag.connect_drag_begin(move |_, x, y| {
            let width = drag_area.width() as f64;
            let height = drag_area.height() as f64;
            let mut state = drag_state.borrow_mut();
            if !state.enabled {
                return;
            }
            let Some(index) = nearest_curve_point(&state.points, width, height, x, y, 18.0) else {
                state.selected_point = None;
                state.drag_origin = None;
                return;
            };
            state.selected_point = Some(index);
            let (temp, duty) = state.points[index];
            state.drag_origin = Some((x, y, temp, duty));
            drop(state);
            drag_area.grab_focus();
            drag_area.queue_draw();
        });
        let drag_state = state.clone();
        let drag_area = area.clone();
        let drag_changed = points_changed.clone();
        drag.connect_drag_update(move |_, offset_x, offset_y| {
            let width = drag_area.width() as f64;
            let height = drag_area.height() as f64;
            let changed = {
                let mut state = drag_state.borrow_mut();
                if !state.enabled {
                    return;
                }
                let (Some(index), Some((start_x, start_y, start_temp, start_duty))) =
                    (state.selected_point, state.drag_origin)
                else {
                    return;
                };
                let (_, _, plot_w, plot_h) = curve_plot_bounds(width, height);
                let temp = (start_temp as f64 + (offset_x / plot_w * 70.0).round())
                    .clamp(30.0, 100.0) as u8;
                let duty = (start_duty as f64 - (offset_y / plot_h * 100.0).round())
                    .clamp(0.0, 100.0) as u8;
                let _ = (start_x, start_y); // GestureDrag offsets are relative to the drag origin.
                move_curve_point(&mut state.points, index, temp, duty)
            };
            if changed {
                let points = drag_state.borrow().points.clone();
                if let Some(callback) = drag_changed.borrow().as_ref() {
                    callback(points);
                }
                drag_area.queue_draw();
            }
        });
        let drag_state = state.clone();
        drag.connect_drag_end(move |_, _, _| {
            drag_state.borrow_mut().drag_origin = None;
        });
        area.add_controller(drag);

        let keys = gtk::EventControllerKey::new();
        let key_state = state.clone();
        let key_area = area.clone();
        let key_changed = points_changed.clone();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let mut state = key_state.borrow_mut();
            if !state.enabled || state.points.len() != 8 {
                return gtk::glib::Propagation::Proceed;
            }
            let selected = state.selected_point.unwrap_or(0).min(7);
            let step = if modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                5
            } else {
                1
            };
            match key {
                gtk::gdk::Key::Left => {
                    let temp = state.points[selected].0.saturating_sub(step);
                    let duty = state.points[selected].1;
                    move_curve_point(&mut state.points, selected, temp, duty);
                }
                gtk::gdk::Key::Right => {
                    let temp = state.points[selected].0.saturating_add(step);
                    let duty = state.points[selected].1;
                    move_curve_point(&mut state.points, selected, temp, duty);
                }
                gtk::gdk::Key::Up => {
                    let temp = state.points[selected].0;
                    let duty = state.points[selected].1.saturating_add(step).min(100);
                    move_curve_point(&mut state.points, selected, temp, duty);
                }
                gtk::gdk::Key::Down => {
                    let temp = state.points[selected].0;
                    let duty = state.points[selected].1.saturating_sub(step);
                    move_curve_point(&mut state.points, selected, temp, duty);
                }
                _ => return gtk::glib::Propagation::Proceed,
            }
            state.selected_point = Some(selected);
            let points = state.points.clone();
            drop(state);
            if let Some(callback) = key_changed.borrow().as_ref() {
                callback(points);
            }
            key_area.queue_draw();
            gtk::glib::Propagation::Stop
        });
        area.add_controller(keys);

        Self {
            area,
            state,
            points_changed,
        }
    }

    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    pub fn set_enabled(&self, enabled: bool) {
        let mut state = self.state.borrow_mut();
        if state.enabled == enabled {
            return;
        }
        state.enabled = enabled;
        drop(state);
        if self.area.is_mapped() {
            self.area.queue_draw();
        }
    }

    pub fn points(&self) -> Vec<(u8, u8)> {
        self.state.borrow().points.clone()
    }

    pub fn set_points(&self, points: Vec<(u8, u8)>) {
        let mut state = self.state.borrow_mut();
        if state.points == points {
            return;
        }
        state.points = points;
        state.selected_point = None;
        state.drag_origin = None;
        drop(state);
        if self.area.is_mapped() {
            self.area.queue_draw();
        }
    }

    pub fn connect_points_changed(&self, callback: impl Fn(Vec<(u8, u8)>) + 'static) {
        *self.points_changed.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_current_temp(&self, temp_c: Option<f32>) {
        let mut state = self.state.borrow_mut();
        if state.current_temp_c == temp_c {
            return;
        }
        state.current_temp_c = temp_c;
        drop(state);
        if self.area.is_mapped() {
            self.area.queue_draw();
        }
    }
}

fn curve_plot_bounds(width: f64, height: f64) -> (f64, f64, f64, f64) {
    let left = 42.0;
    let right = 18.0;
    let top = 22.0;
    let bottom = 34.0;
    (
        left,
        top,
        (width - left - right).max(40.0),
        (height - top - bottom).max(40.0),
    )
}

fn curve_xy(width: f64, height: f64, temp: u8, duty: u8) -> (f64, f64) {
    let (left, top, plot_w, plot_h) = curve_plot_bounds(width, height);
    (
        left + ((temp.saturating_sub(30)) as f64 / 70.0).clamp(0.0, 1.0) * plot_w,
        top + (1.0 - (duty as f64 / 100.0).clamp(0.0, 1.0)) * plot_h,
    )
}

fn nearest_curve_point(
    points: &[(u8, u8)],
    width: f64,
    height: f64,
    x: f64,
    y: f64,
    radius: f64,
) -> Option<usize> {
    points
        .iter()
        .enumerate()
        .map(|(index, (temp, duty))| {
            let (point_x, point_y) = curve_xy(width, height, *temp, *duty);
            (index, (point_x - x).hypot(point_y - y))
        })
        .filter(|(_, distance)| *distance <= radius)
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(index, _)| index)
}

/// Move one point without allowing temperature or duty ordering to cross neighbors.
pub fn move_curve_point(points: &mut [(u8, u8)], index: usize, temp: u8, duty: u8) -> bool {
    if points.len() != 8 || index >= points.len() {
        return false;
    }
    let min_temp = if index == 0 {
        30
    } else {
        points[index - 1].0.saturating_add(1)
    };
    let max_temp = if index + 1 == points.len() {
        100
    } else {
        points[index + 1].0.saturating_sub(1)
    };
    if min_temp > max_temp {
        return false;
    }
    let min_duty = if index == 0 { 0 } else { points[index - 1].1 };
    let max_duty = if index + 1 == points.len() {
        100
    } else {
        points[index + 1].1
    };
    let next = (
        temp.clamp(min_temp, max_temp),
        duty.clamp(min_duty, max_duty),
    );
    let changed = points[index] != next;
    points[index] = next;
    changed
}

fn draw_fan_rotor(ctx: &Context, width: f64, height: f64, state: &FanRotorState) {
    let size = width.min(height);
    let cx = width / 2.0;
    let cy = height / 2.0;
    let radius = size * 0.39;
    let (r, g, b) = rotor_color(state.status);
    let alpha = if state.rpm.unwrap_or(0) == 0 {
        0.45
    } else {
        1.0
    };

    ctx.save().ok();
    radial_glow(ctx, cx, cy, radius * 1.28, (r, g, b), 0.045 * alpha);

    ctx.set_line_width((size * 0.035).max(3.0));
    ctx.set_source_rgba(r, g, b, 0.85 * alpha);
    ctx.arc(cx, cy, radius, 0.0, 2.0 * PI);
    ctx.stroke().ok();

    ctx.set_line_width(1.2);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.16);
    ctx.arc(cx, cy, radius * 0.76, 0.0, 2.0 * PI);
    ctx.stroke().ok();

    let blades = if state.compact { 3 } else { 5 };
    for blade in 0..blades {
        let angle = state.angle + blade as f64 * (2.0 * PI / blades as f64);
        draw_blade(ctx, cx, cy, radius, angle, state.compact, alpha);
    }

    ctx.set_source_rgba(0.05, 0.06, 0.08, 0.86);
    ctx.arc(cx, cy, radius * 0.18, 0.0, 2.0 * PI);
    ctx.fill().ok();
    ctx.set_line_width(2.0);
    ctx.set_source_rgba(r, g, b, 0.85);
    ctx.arc(cx, cy, radius * 0.18, 0.0, 2.0 * PI);
    ctx.stroke().ok();

    ctx.restore().ok();
}

fn draw_blade(ctx: &Context, cx: f64, cy: f64, radius: f64, angle: f64, compact: bool, alpha: f64) {
    let inner = radius * 0.22;
    let outer = radius * if compact { 0.88 } else { 0.92 };
    let sweep = if compact { 0.34 } else { 0.40 };
    let skew = if compact { 0.18 } else { 0.24 };

    let p1 = polar(cx, cy, inner, angle - sweep);
    let p2 = polar(cx, cy, outer, angle - skew);
    let p3 = polar(cx, cy, outer * 0.84, angle + sweep);
    let p4 = polar(cx, cy, inner * 0.85, angle + skew);

    ctx.move_to(p1.0, p1.1);
    ctx.curve_to(p2.0, p2.1, p2.0, p2.1, p3.0, p3.1);
    ctx.curve_to(p4.0, p4.1, p4.0, p4.1, p1.0, p1.1);
    ctx.close_path();
    ctx.set_source_rgba(0.90, 0.96, 1.0, 0.68 * alpha);
    ctx.fill_preserve().ok();
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.22 * alpha);
    ctx.set_line_width(1.0);
    ctx.stroke().ok();
}

fn draw_temperature_gauge(ctx: &Context, width: f64, height: f64, state: &TempGaugeState) {
    let size = width.min(height);
    let cx = width / 2.0;
    let cy = height * 0.43;
    let radius = size * 0.36;
    let temp = state.temp_c.map(|value| value.clamp(0.0, 100.0));
    let (r, g, b) = gauge_color(state.accent, temp);
    let start = 0.78 * PI;
    let end = 2.22 * PI;
    let range = end - start;

    radial_glow(ctx, cx, cy, radius * 1.30, (r, g, b), 0.045);

    ctx.set_line_width((size * 0.042).max(5.0));
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.12);
    ctx.arc(cx, cy, radius, start, end);
    ctx.stroke().ok();

    if let Some(temp) = temp {
        ctx.set_source_rgba(r, g, b, 0.90);
        ctx.arc(cx, cy, radius, start, start + range * (temp as f64 / 100.0));
        ctx.stroke().ok();
    }

    ctx.set_line_width(1.0);
    for tick in 0..=20 {
        let a = start + range * (tick as f64 / 20.0);
        let inner = if tick % 2 == 0 {
            radius * 0.84
        } else {
            radius * 0.91
        };
        let outer = radius * 1.03;
        let p1 = polar(cx, cy, inner, a);
        let p2 = polar(cx, cy, outer, a);
        ctx.move_to(p1.0, p1.1);
        ctx.line_to(p2.0, p2.1);
    }
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.45);
    ctx.stroke().ok();

    let value = state
        .temp_c
        .map(|temp| format!("{temp:.0}"))
        .unwrap_or_else(|| "--".to_string());
    draw_center_text(
        ctx,
        cx,
        cy - size * 0.02,
        &value,
        "C",
        size * 0.23,
        (r, g, b),
    );
    draw_label(
        ctx,
        cx,
        cy + radius * 0.60,
        &state.label,
        size * 0.095,
        (1.0, 1.0, 1.0),
        0.82,
    );

    let primary_speed = speed_line(&state.primary_speed_label, state.primary_speed_mhz);
    draw_label(
        ctx,
        cx,
        height - size * 0.205,
        &primary_speed,
        size * 0.067,
        (1.0, 1.0, 1.0),
        0.72,
    );
    if let Some(label) = state.secondary_speed_label.as_deref() {
        let secondary_speed = speed_line(label, state.secondary_speed_mhz);
        draw_label(
            ctx,
            cx,
            height - size * 0.105,
            &secondary_speed,
            size * 0.058,
            (1.0, 1.0, 1.0),
            0.54,
        );
    }
}

fn draw_curve_preview(ctx: &Context, width: f64, height: f64, state: &CurvePreviewState) {
    let (pad_left, pad_top, plot_w, plot_h) = curve_plot_bounds(width, height);
    let alpha = if state.enabled { 0.95 } else { 0.48 };

    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.06);
    rounded_rect(ctx, 8.0, 8.0, width - 16.0, height - 16.0, 12.0);
    ctx.fill().ok();

    for i in 0..=5 {
        let x = pad_left + plot_w * i as f64 / 5.0;
        ctx.move_to(x, pad_top);
        ctx.line_to(x, pad_top + plot_h);
        let y = pad_top + plot_h * i as f64 / 5.0;
        ctx.move_to(pad_left, y);
        ctx.line_to(pad_left + plot_w, y);
    }
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.14);
    ctx.set_line_width(1.0);
    ctx.stroke().ok();

    let to_xy = |temp: u8, speed: u8| curve_xy(width, height, temp, speed);

    if let Some((first_temp, first_speed)) = state.points.first().copied() {
        let first = to_xy(first_temp, first_speed);
        ctx.move_to(first.0, first.1);
        for (temp, speed) in state.points.iter().copied().skip(1) {
            let point = to_xy(temp, speed);
            ctx.line_to(point.0, point.1);
        }
        ctx.set_line_width(3.0);
        ctx.set_source_rgba(0.30, 0.64, 1.0, 0.35 * alpha);
        ctx.stroke_preserve().ok();
        ctx.set_source_rgba(0.30, 0.64, 1.0, 0.85 * alpha);
        ctx.stroke().ok();

        for (index, (temp, speed)) in state.points.iter().enumerate() {
            let (x, y) = to_xy(*temp, *speed);
            if state.selected_point == Some(index) {
                ctx.set_source_rgba(1.0, 1.0, 1.0, 0.85);
                ctx.arc(x, y, 8.0, 0.0, 2.0 * PI);
                ctx.stroke().ok();
            }
            ctx.arc(x, y, 4.0, 0.0, 2.0 * PI);
            ctx.set_source_rgba(0.30, 0.64, 1.0, alpha);
            ctx.fill().ok();
        }
    }

    if let Some(temp_c) = state.current_temp_c.filter(|value| value.is_finite()) {
        let x = pad_left + ((f64::from(temp_c) - 30.0) / 70.0).clamp(0.0, 1.0) * plot_w;
        ctx.set_dash(&[4.0, 4.0], 0.0);
        ctx.set_source_rgba(1.0, 0.74, 0.22, 0.9);
        ctx.set_line_width(1.5);
        ctx.move_to(x, pad_top);
        ctx.line_to(x, pad_top + plot_h);
        ctx.stroke().ok();
        ctx.set_dash(&[], 0.0);
        draw_label(
            ctx,
            x.clamp(pad_left + 34.0, pad_left + plot_w - 34.0),
            14.0,
            &format!("Now {:.0}°C", temp_c),
            10.0,
            (1.0, 0.74, 0.22),
            0.95,
        );
    }

    draw_label(
        ctx,
        pad_left + plot_w / 2.0,
        height - 5.0,
        "Temperature (°C) · straight segments are visual only",
        10.0,
        (1.0, 1.0, 1.0),
        0.72,
    );
    draw_label(
        ctx,
        13.0,
        pad_top + plot_h / 2.0,
        "Fan %",
        10.0,
        (1.0, 1.0, 1.0),
        0.72,
    );

    draw_label(
        ctx,
        pad_left + plot_w / 2.0,
        height - 10.0,
        "Temperature (C)",
        11.0,
        (1.0, 1.0, 1.0),
        0.58,
    );
    draw_label(
        ctx,
        20.0,
        pad_top + plot_h / 2.0,
        "Fan %",
        11.0,
        (1.0, 1.0, 1.0),
        0.58,
    );
}

fn rotor_color(status: RotorStatus) -> Color {
    match status {
        RotorStatus::Telemetry => (0.0, 0.70, 1.0),
        RotorStatus::ReadOnly => (0.42, 0.58, 0.72),
        RotorStatus::Controllable => (0.0, 0.95, 0.72),
        RotorStatus::Warning => (1.0, 0.66, 0.16),
        RotorStatus::Error => (1.0, 0.22, 0.28),
    }
}

fn gauge_color(accent: GaugeAccent, temp: Option<f32>) -> Color {
    if let Some(temp) = temp {
        if temp >= 85.0 {
            return (1.0, 0.22, 0.28);
        }
        if temp >= 70.0 {
            return (1.0, 0.66, 0.16);
        }
    }
    match accent {
        GaugeAccent::Blue => (0.30, 0.64, 1.0),
        GaugeAccent::Magenta => (0.24, 0.76, 0.67),
    }
}

fn speed_line(label: &str, mhz: Option<u64>) -> String {
    mhz.map(|value| format!("{label}: {value} MHz"))
        .unwrap_or_else(|| format!("{label}: -- MHz"))
}

fn draw_center_text(
    ctx: &Context,
    cx: f64,
    cy: f64,
    value: &str,
    unit: &str,
    size: f64,
    color: Color,
) {
    draw_label(ctx, cx, cy, value, size, (1.0, 1.0, 1.0), 0.92);
    draw_label(
        ctx,
        cx + size * 0.80,
        cy + size * 0.06,
        unit,
        size * 0.36,
        color,
        0.82,
    );
}

fn draw_label(ctx: &Context, cx: f64, cy: f64, text: &str, size: f64, color: Color, alpha: f64) {
    ctx.save().ok();
    ctx.select_font_face("Sans", FontSlant::Normal, FontWeight::Bold);
    ctx.set_font_size(size);
    let ext = ctx.text_extents(text).ok();
    let x = ext
        .as_ref()
        .map(|ext| cx - ext.width() / 2.0 - ext.x_bearing())
        .unwrap_or(cx);
    let y = ext
        .as_ref()
        .map(|ext| cy - ext.height() / 2.0 - ext.y_bearing())
        .unwrap_or(cy);
    ctx.move_to(x, y);
    let (r, g, b) = color;
    ctx.set_source_rgba(r, g, b, alpha);
    ctx.show_text(text).ok();
    ctx.restore().ok();
}

fn radial_glow(ctx: &Context, cx: f64, cy: f64, radius: f64, color: Color, alpha: f64) {
    let (r, g, b) = color;
    let gradient = gtk::cairo::RadialGradient::new(cx, cy, radius * 0.15, cx, cy, radius);
    gradient.add_color_stop_rgba(0.0, r, g, b, alpha);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
    ctx.set_source(&gradient).ok();
    ctx.arc(cx, cy, radius, 0.0, 2.0 * PI);
    ctx.fill().ok();
}

fn rounded_rect(ctx: &Context, x: f64, y: f64, w: f64, h: f64, radius: f64) {
    let r = radius.min(w / 2.0).min(h / 2.0);
    ctx.new_sub_path();
    ctx.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    ctx.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    ctx.arc(x + r, y + h - r, r, PI / 2.0, PI);
    ctx.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0);
    ctx.close_path();
}

fn polar(cx: f64, cy: f64, radius: f64, angle: f64) -> (f64, f64) {
    (cx + radius * angle.cos(), cy + radius * angle.sin())
}

#[cfg(test)]
mod curve_editor_tests {
    use super::move_curve_point;

    fn points() -> Vec<(u8, u8)> {
        vec![
            (35, 15),
            (45, 25),
            (55, 35),
            (65, 50),
            (75, 70),
            (85, 90),
            (90, 100),
            (95, 100),
        ]
    }

    #[test]
    fn graph_point_drag_clamps_to_neighbor_order() {
        let mut curve = points();
        assert!(move_curve_point(&mut curve, 3, 99, 100));
        assert_eq!(curve[3], (74, 70));
        assert!(curve.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(curve.windows(2).all(|pair| pair[0].1 <= pair[1].1));
    }

    #[test]
    fn graph_point_edit_rejects_wrong_point_count() {
        let mut curve = points();
        curve.pop();
        assert!(!move_curve_point(&mut curve, 1, 50, 30));
    }
}
