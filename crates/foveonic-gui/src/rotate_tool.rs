//! Owns line and freehand rotation gestures within the shared photograph-tool lifecycle.

use crate::{
    photo_tools::{self, Tool},
    widgets::{self, ControlSize},
};
use eframe::egui;
use foveonic::{crop::Crop, rotation::Rotation};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Mode {
    #[default]
    Straighten,
    Freehand,
    Left,
    Right,
}
impl Mode {
    const ALL: [Self; 4] = [Self::Straighten, Self::Freehand, Self::Left, Self::Right];
    fn label(self) -> &'static str {
        match self {
            Self::Straighten => "Straighten",
            Self::Freehand => "Rotate Freehand",
            Self::Left => "Rotate Left",
            Self::Right => "Rotate Right",
        }
    }
    fn gesture(self) -> bool {
        matches!(self, Self::Straighten | Self::Freehand)
    }
    fn next(self) -> Self {
        Self::ALL[(Self::ALL.iter().position(|m| *m == self).unwrap() + 1) % Self::ALL.len()]
    }
    fn icon(self, painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
        let clockwise = matches!(self, Self::Freehand | Self::Right);
        let p = |x: f32, y: f32| {
            let x = if clockwise { x } else { 34. - x };
            rect.min + egui::vec2(x / 36., y / 36.) * rect.size()
        };
        let ring = egui::Stroke::new(rect.width() * 3.7 / 36., color);
        let detail = egui::Stroke::new(rect.width() * 1.8 / 36., color);
        let mut arc: Vec<_> = (0..=30)
            .map(|i| {
                let angle = (i as f32 * 9.).to_radians();
                p(17. + angle.cos() * 12.5, 20. + angle.sin() * 12.5)
            })
            .collect();
        arc.push(p(18.5, 7.5));
        painter.add(egui::Shape::line(arc, ring));
        painter.add(egui::Shape::convex_polygon(
            vec![p(24.5, 7.5), p(18.5, 1.), p(18.5, 14.)],
            color,
            egui::Stroke::NONE,
        ));
        match self {
            Self::Straighten => {
                painter.line_segment([p(11., 20.), p(23., 20.)], detail);
                painter.line_segment([p(17., 14.), p(17., 26.)], detail);
            }
            Self::Left | Self::Right => {
                painter.line_segment([p(27., 2.5), p(27., 12.5)], detail);
            }
            Self::Freehand => {}
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    FirstPress,
    AwaitingEnd,
    SecondPress,
}
#[derive(Clone)]
struct Gesture {
    phase: Phase,
    original: Rotation,
    draft: Rotation,
    start: egui::Pos2,
    end: egui::Pos2,
}
#[derive(Clone, Default)]
struct State {
    mode: Mode,
    source: Option<PathBuf>,
    gesture: Option<Gesture>,
    menu: bool,
}
fn state(ctx: &egui::Context) -> State {
    ctx.data_mut(|d| d.get_temp(egui::Id::new("Rotate tool")))
        .unwrap_or_default()
}
fn store(ctx: &egui::Context, s: State) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("Rotate tool"), s));
}
pub fn active(ctx: &egui::Context) -> bool {
    photo_tools::selected(ctx) == Tool::Rotate
}
pub fn editing(ctx: &egui::Context) -> bool {
    let s = state(ctx);
    active(ctx) && s.mode.gesture()
}
pub fn menu_open(ctx: &egui::Context) -> bool {
    state(ctx).menu
}
pub fn source(ctx: &egui::Context, path: Option<&Path>) {
    let mut s = state(ctx);
    if s.source.as_deref() != path {
        s.source = path.map(Path::to_owned);
        s.gesture = None;
        store(ctx, s);
    }
}
pub(crate) fn cancel(ctx: &egui::Context) {
    let mut s = state(ctx);
    s.gesture = None;
    s.menu = false;
    store(ctx, s);
}
pub fn exit(ctx: &egui::Context) {
    cancel(ctx);
    photo_tools::release(ctx, Tool::Rotate);
}
pub fn escape(ctx: &egui::Context) {
    let mut s = state(ctx);
    if s.gesture.take().is_none() {
        exit(ctx);
    } else {
        store(ctx, s);
    }
}
pub fn invoke(ctx: &egui::Context, rotation: &mut Rotation, crop: &mut Crop) {
    let s = state(ctx);
    if s.mode.gesture() && active(ctx) {
        exit(ctx);
    } else {
        activate(ctx, s.mode);
        match s.mode {
            Mode::Left => rotation.turn(false, crop),
            Mode::Right => rotation.turn(true, crop),
            _ => {}
        }
    }
}
pub fn cycle(ctx: &egui::Context) {
    activate(ctx, state(ctx).mode.next());
}
pub(crate) fn activate(ctx: &egui::Context, mode: Mode) {
    photo_tools::select(ctx, Tool::Rotate);
    let mut s = state(ctx);
    s.mode = mode;
    store(ctx, s);
    ctx.request_repaint();
}
pub fn displayed(ctx: &egui::Context, rotation: Rotation) -> Rotation {
    let s = state(ctx);
    if s.mode == Mode::Freehand {
        s.gesture.map_or(rotation, |d| d.draft)
    } else {
        rotation
    }
}

pub fn button(ui: &mut egui::Ui, rotation: &mut Rotation, crop: &mut Crop) {
    let mut s = state(ui.ctx());
    let mut chosen = None;
    let (button, menu) =
        widgets::split_icon_button(ui, "Rotate", active(ui.ctx()), ControlSize::Large, |ui| {
            for mode in Mode::ALL {
                if widgets::menu_icon_row(
                    ui,
                    mode.label(),
                    mode == s.mode,
                    |painter, rect, color| mode.icon(painter, rect, color),
                )
                .clicked()
                {
                    chosen = Some(mode);
                    ui.close_menu();
                }
            }
            ui.separator();
            if widgets::menu_row(ui, "Reset rotation", false).clicked() {
                rotation.reset(crop);
                ui.close_menu();
            }
        });
    s.mode.icon(
        ui.painter(),
        widgets::icon_rect(ui, &button),
        widgets::visuals(ui, &button, active(ui.ctx())).text_color(),
    );
    if button
        .on_hover_text(format!(
            "{} (R) · Shift+R cycles rotation tools",
            s.mode.label()
        ))
        .clicked()
    {
        invoke(ui.ctx(), rotation, crop);
        s = state(ui.ctx());
    }
    if let Some(mode) = chosen {
        activate(ui.ctx(), mode);
        match mode {
            Mode::Left => rotation.turn(false, crop),
            Mode::Right => rotation.turn(true, crop),
            _ => {}
        }
        s = state(ui.ctx());
    }
    s.menu = menu.inner.is_some();
    store(ui.ctx(), s);
}

fn angle(mode: Mode, original: f64, start: egui::Pos2, end: egui::Pos2, center: egui::Pos2) -> f64 {
    let delta = if mode == Mode::Straighten {
        let d = end - start;
        let line = f64::from(d.y.atan2(d.x).to_degrees());
        -((line + 45.).rem_euclid(90.) - 45.)
    } else {
        let a = start - center;
        let b = end - center;
        (f64::from(b.y.atan2(b.x) - a.y.atan2(a.x)).to_degrees() + 180.).rem_euclid(360.) - 180.
    };
    (original + delta).clamp(-45., 45.)
}

pub fn photo(
    ui: &mut egui::Ui,
    response: &egui::Response,
    image: egui::Rect,
    rotation: &mut Rotation,
) -> bool {
    let mut s = state(ui.ctx());
    if !active(ui.ctx()) || !s.mode.gesture() {
        return false;
    }
    let pan = ui.input(|i| i.key_down(egui::Key::Space));
    if pan || s.gesture.as_ref().is_some_and(|d| d.original != *rotation) {
        s.gesture = None;
    }
    if !pan {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            if response.hovered()
                && image.contains(pos)
                && ui.input(|i| i.pointer.primary_pressed())
            {
                if let Some(drag) = s
                    .gesture
                    .as_mut()
                    .filter(|d| s.mode == Mode::Straighten && d.phase == Phase::AwaitingEnd)
                {
                    drag.phase = Phase::SecondPress;
                } else {
                    s.gesture = Some(Gesture {
                        phase: Phase::FirstPress,
                        original: *rotation,
                        draft: *rotation,
                        start: pos,
                        end: pos,
                    });
                }
            }
            if ui.input(|i| i.pointer.primary_pressed())
                && (!response.hovered() || !image.contains(pos))
                && s.gesture
                    .as_ref()
                    .is_some_and(|d| d.phase == Phase::AwaitingEnd)
            {
                s.gesture = None;
            }
            if let Some(drag) = &mut s.gesture {
                drag.end = pos;
                let previous = drag.draft;
                drag.draft.degrees = angle(
                    s.mode,
                    drag.original.degrees,
                    drag.start,
                    pos,
                    image.center(),
                );
                if previous != drag.draft {
                    ui.ctx().request_repaint();
                }
            }
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }
    let mut changed = false;
    let completing_line = s.mode == Mode::Straighten
        && s.gesture
            .as_ref()
            .is_some_and(|d| d.phase == Phase::SecondPress && d.start.distance(d.end) > 3.);
    if !pan && response.double_clicked() && !completing_line {
        s.gesture = None;
        changed = rotation.degrees != 0.;
        rotation.degrees = 0.;
    } else if ui.input(|i| i.pointer.primary_released()) {
        if let Some(drag) = &mut s.gesture {
            if s.mode == Mode::Straighten
                && drag.phase == Phase::FirstPress
                && drag.start.distance(drag.end) <= 3.
            {
                drag.phase = Phase::AwaitingEnd;
            } else if drag.phase != Phase::AwaitingEnd {
                let drag = s.gesture.take().unwrap();
                if drag.start.distance(drag.end) > 3. {
                    changed = *rotation != drag.draft;
                    *rotation = drag.draft;
                }
            }
        }
    }
    store(ui.ctx(), s);
    changed
}

pub fn paint_guides(ui: &egui::Ui, canvas: egui::Rect, rotation: Rotation) {
    let s = state(ui.ctx());
    let proposed = s.gesture.as_ref().map_or(rotation, |d| d.draft);
    let painter = ui.painter().with_clip_rect(canvas);
    if let Some(drag) = &s.gesture {
        if s.mode == Mode::Straighten {
            painter.line_segment(
                [drag.start, drag.end],
                egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
            );
        } else {
            for i in 1..12 {
                let f = i as f32 / 12.;
                painter.line_segment(
                    [
                        egui::pos2(canvas.left() + canvas.width() * f, canvas.top()),
                        egui::pos2(canvas.left() + canvas.width() * f, canvas.bottom()),
                    ],
                    egui::Stroke::new(1_f32, egui::Color32::from_white_alpha(90)),
                );
                painter.line_segment(
                    [
                        egui::pos2(canvas.left(), canvas.top() + canvas.height() * f),
                        egui::pos2(canvas.right(), canvas.top() + canvas.height() * f),
                    ],
                    egui::Stroke::new(1_f32, egui::Color32::from_white_alpha(90)),
                );
            }
        }
    }
    widgets::overlay_badge(ui, canvas, format!("Rotation {:+.2}°", proposed.degrees));
}

#[cfg(test)]
#[path = "rotate_tool_test.rs"]
mod tests;
