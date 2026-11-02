//! Routes on-image color gestures into existing range sliders using asynchronous guide picks.

use crate::worker::{ColorPickReply, ColorPickRequest};
use eframe::egui;
use foveonic::render::{ColorEditorControls, Settings};
use std::path::{Path, PathBuf};

#[path = "direct_color_settings.rs"]
mod settings;
pub use settings::Configuration;
use settings::Gesture;

const POINTS_PER_UNIT: f32 = 300.;
const DRAG_THRESHOLD: f32 = 3.;

#[derive(Clone, Copy, Debug)]
enum Channel {
    Hue,
    Saturation,
    Lightness,
}
impl Channel {
    const ALL: [Self; 3] = [Self::Hue, Self::Saturation, Self::Lightness];
    fn index(self) -> usize {
        match self {
            Self::Hue => 0,
            Self::Saturation => 1,
            Self::Lightness => 2,
        }
    }
}

#[derive(Clone)]
struct Drag {
    token: u64,
    path: PathBuf,
    anchor: egui::Pos2,
    position: egui::Pos2,
    configuration: Configuration,
    moved: bool,
    amounts: [f32; 3],
    original: ColorEditorControls,
    expected: Settings,
    weights: Option<[f32; 8]>,
}

#[derive(Clone, Default)]
struct State {
    active: bool,
    configuration_menu: bool,
    context_menu: bool,
    configuration: Configuration,
    generation: u64,
    selected: usize,
    weights: [f32; 8],
    drag: Option<Drag>,
    notice: String,
}
fn id() -> egui::Id {
    egui::Id::new("direct color tool")
}
fn state(ctx: &egui::Context) -> State {
    ctx.data_mut(|d| d.get_temp(id())).unwrap_or_default()
}
fn store(ctx: &egui::Context, state: State) {
    ctx.data_mut(|d| d.insert_temp(id(), state));
}

pub fn active(ctx: &egui::Context) -> bool {
    state(ctx).active
}
pub fn menu_open(ctx: &egui::Context) -> bool {
    let s = state(ctx);
    s.configuration_menu || s.context_menu
}
pub fn toggle(ctx: &egui::Context) {
    let mut s = state(ctx);
    s.active = !s.active;
    s.drag = None;
    s.generation += 1;
    store(ctx, s);
}
pub fn exit(ctx: &egui::Context) {
    let mut s = state(ctx);
    s.active = false;
    s.drag = None;
    s.generation += 1;
    store(ctx, s);
}
pub fn selection(ctx: &egui::Context) -> (usize, [f32; 8]) {
    let s = state(ctx);
    (s.selected, s.weights)
}
pub fn select(ctx: &egui::Context, selected: usize) {
    let mut s = state(ctx);
    s.selected = selected;
    s.weights = [0.; 8];
    s.drag = None;
    s.generation += 1;
    store(ctx, s);
}
pub fn configuration(ctx: &egui::Context) -> Configuration {
    state(ctx).configuration
}
pub fn configure(ctx: &egui::Context, mut configuration: Configuration) {
    configuration.normalize();
    let mut s = state(ctx);
    s.configuration = configuration;
    store(ctx, s);
}
pub fn dragging(ctx: &egui::Context) -> bool {
    state(ctx).drag.is_some()
}
pub fn button(ui: &mut egui::Ui) {
    let mut s = state(ui.ctx());
    let (toggle, opened) = settings::tools(ui, s.active, &mut s.configuration);
    s.configuration_menu = opened;
    if toggle {
        s.active = !s.active;
        s.drag = None;
        s.generation += 1;
    }
    store(ui.ctx(), s);
}

pub fn receive(ctx: &egui::Context, reply: ColorPickReply) {
    let mut s = state(ctx);
    if let Some(drag) = &mut s.drag {
        if drag.token == reply.token && drag.path == reply.path {
            match reply.weights {
                Ok(weights) => {
                    drag.weights = Some(weights);
                    s.weights = weights;
                    if let Some((index, _)) = weights
                        .iter()
                        .enumerate()
                        .filter(|(_, w)| **w > 0.)
                        .max_by(|a, b| a.1.total_cmp(b.1))
                    {
                        s.selected = index;
                        s.notice.clear();
                    } else {
                        s.notice = "No reliable color at this point".into();
                    }
                }
                Err(error) => {
                    s.notice = error;
                    s.drag = None;
                }
            }
        }
    }
    store(ctx, s);
}

fn adjusted(
    original: ColorEditorControls,
    weights: [f32; 8],
    channel: Channel,
    amount: f32,
) -> ColorEditorControls {
    let mut result = original;
    for (band, weight) in result.bands.iter_mut().zip(weights) {
        let value = match channel {
            Channel::Hue => &mut band.hue,
            Channel::Lightness => &mut band.lightness,
            Channel::Saturation => &mut band.saturation,
        };
        *value = (*value + amount * weight).clamp(-1., 1.);
    }
    result
}

/// The pointer response and normalized position use the viewport's actual painted geometry.
/// Pending picks retain the gesture baseline, including when the mouse is released first.
pub fn photo(
    ui: &egui::Ui,
    response: &egui::Response,
    uv: Option<[f32; 2]>,
    path: &Path,
    settings: &mut Settings,
    allowed: bool,
) -> (bool, Option<ColorPickRequest>) {
    let mut s = state(ui.ctx());
    if !s.active || !allowed {
        s.drag = None;
        s.context_menu = false;
        store(ui.ctx(), s);
        return (false, None);
    }
    let (down, position, alt, pan) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.interact_pos(),
            i.modifiers.alt,
            i.key_down(egui::Key::Space),
        )
    });
    response.context_menu(|ui| settings::controls(ui, &mut s.configuration));
    s.context_menu = response.context_menu_opened();
    if response.hovered() && !pan {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if pan {
        s.drag = None;
        store(ui.ctx(), s);
        return (false, None);
    }
    let mut request = None;
    if s.drag
        .as_ref()
        .is_some_and(|d| d.path != path || d.expected != *settings)
    {
        s.drag = None;
    }
    // A fresh press is required after a recipe/photo change or while Space pans.
    let pressed = ui.input(|i| i.pointer.primary_pressed());
    if pressed && !response.is_pointer_button_down_on() {
        s.drag = None;
    }
    if pressed && response.is_pointer_button_down_on() && !pan {
        if let (Some(uv), Some(anchor)) = (uv, position) {
            s.generation += 1;
            s.drag = Some(Drag {
                token: s.generation,
                path: path.to_owned(),
                anchor,
                position: anchor,
                configuration: s.configuration,
                moved: false,
                amounts: [0.; 3],
                original: settings.photographic.color_editor,
                expected: settings.clone(),
                weights: None,
            });
            s.notice = "Selecting color…".into();
            request = Some(ColorPickRequest {
                token: s.generation,
                path: path.to_owned(),
                settings: settings.clone(),
                uv,
            });
        }
    }
    let mut changed = false;
    let mono = settings.is_monochrome();
    if let Some(drag) = &mut s.drag {
        let previous = drag.position;
        if down || ui.input(|i| i.pointer.primary_released()) {
            if let Some(position) = position {
                drag.position = position;
            }
        }
        let total = drag.position - drag.anchor;
        let delta = if !drag.moved && total.length() > DRAG_THRESHOLD {
            drag.moved = true;
            total
        } else if drag.moved {
            drag.position - previous
        } else {
            egui::Vec2::ZERO
        };
        // Both axes contribute to the same gesture. Keep accumulated amounts
        // separate so Option can change assignments without losing earlier edits.
        for (horizontal, distance) in [(true, delta.x), (false, -delta.y)] {
            if let Some(channel) = drag
                .configuration
                .channel(Gesture::from_axis(horizontal, alt))
            {
                drag.amounts[channel.index()] +=
                    distance / POINTS_PER_UNIT * (drag.configuration.sensitivity / 50.);
            }
        }
        if let Some(weights) = drag.weights.filter(|_| drag.moved) {
            let mut controls = drag.original;
            for channel in Channel::ALL {
                if !mono || matches!(channel, Channel::Lightness) {
                    controls = adjusted(controls, weights, channel, drag.amounts[channel.index()]);
                }
            }
            changed = settings.photographic.color_editor != controls;
            settings.photographic.color_editor = controls;
            drag.expected = settings.clone();
        }
        if !down && drag.weights.is_some() {
            s.drag = None;
        }
    }
    if response.hovered() && !s.notice.is_empty() {
        let painter = ui.painter();
        let pos = response.rect.left_top() + egui::vec2(12., 12.);
        let galley = painter.layout_no_wrap(
            s.notice.clone(),
            egui::TextStyle::Body.resolve(ui.style()),
            egui::Color32::WHITE,
        );
        painter.rect_filled(
            egui::Rect::from_min_size(
                pos - egui::vec2(5., 3.),
                galley.size() + egui::vec2(10., 6.),
            ),
            3.,
            egui::Color32::from_black_alpha(210),
        );
        painter.galley(pos, galley, egui::Color32::WHITE);
    }
    store(ui.ctx(), s);
    (changed, request)
}

#[cfg(test)]
#[path = "direct_color_test.rs"]
mod tests;
