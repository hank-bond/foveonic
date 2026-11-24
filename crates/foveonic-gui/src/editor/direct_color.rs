//! Routes on-image color gestures into existing range sliders using asynchronous guide picks.

use crate::{
    multi_edit_controls::{field, Edit, Mapping, NumberChange},
    photo_tools::{self, Tool},
    rendering::worker::{ColorPickReply, ColorPickRequest, ColorPickTool},
};
use eframe::egui;
use foveonic::render::{AdvancedColorControls, PerceptualControls, Settings};
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
    /// The band field this channel edits, its control range and the mapping to that range.
    fn control(self) -> (&'static str, [f64; 2], Mapping) {
        let saturation = PerceptualControls::SATURATION_CONTROL_RANGE;
        match self {
            Self::Hue => ("hue", [-1., 1.], Mapping::Identity),
            Self::Saturation => (
                "saturation",
                [f64::from(*saturation.start()), f64::from(*saturation.end())],
                Mapping::Saturation,
            ),
            Self::Lightness => ("lightness", [-1., 1.], Mapping::Identity),
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Target {
    #[default]
    Basic,
    Advanced,
}
impl Target {
    fn tool(self) -> ColorPickTool {
        match self {
            Self::Basic => ColorPickTool::Basic,
            Self::Advanced => ColorPickTool::Advanced,
        }
    }
}

#[derive(Clone)]
struct Drag {
    target: Target,
    range: Option<usize>,
    token: u64,
    path: PathBuf,
    anchor: egui::Pos2,
    position: egui::Pos2,
    configuration: Configuration,
    moved: bool,
    released: bool,
    amounts: [f32; 3],
    /// The amounts already sent as edits, once the first edit is sent.
    applied: Option<[f32; 3]>,
    /// The pressed photo's range list, which an Advanced pick adds its range to.
    ranges: AdvancedColorControls,
    expected: Settings,
    weights: Option<[f32; 8]>,
    feedback: Option<crate::activity::Event>,
}

#[derive(Clone, Default)]
struct State {
    target: Target,
    configuration_menu: bool,
    /// The photo whose right-click settings menu is open.
    context_menu: Option<PathBuf>,
    configuration: Configuration,
    generation: u64,
    selected: usize,
    weights: [f32; 8],
    drag: Option<Drag>,
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
    photo_tools::selected(ctx) == Tool::ColorRange
}
pub fn menu_open(ctx: &egui::Context) -> bool {
    let s = state(ctx);
    s.configuration_menu || s.context_menu.is_some()
}
pub fn toggle(ctx: &egui::Context) {
    toggle_target(ctx, Target::Basic);
}
pub fn toggle_advanced(ctx: &egui::Context) {
    toggle_target(ctx, Target::Advanced);
}
pub fn advanced_active(ctx: &egui::Context) -> bool {
    let s = state(ctx);
    active(ctx) && s.target == Target::Advanced
}

fn toggle_target(ctx: &egui::Context, target: Target) {
    let on = active(ctx) && state(ctx).target == target;
    photo_tools::select(ctx, if on { Tool::Pointer } else { Tool::ColorRange });
    let mut s = state(ctx);
    s.target = target;
    store(ctx, s);
}
pub(crate) fn cancel(ctx: &egui::Context) {
    super::advanced_color::cancel_rename(ctx);
    let mut s = state(ctx);
    s.drag = None;
    s.configuration_menu = false;
    s.context_menu = None;
    s.generation += 1;
    store(ctx, s);
}
pub fn exit(ctx: &egui::Context) {
    cancel(ctx);
    photo_tools::release(ctx, Tool::ColorRange);
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
/// The photo whose drag or settings menu is in progress.
pub fn held_photo(ctx: &egui::Context) -> Option<PathBuf> {
    let s = state(ctx);
    s.drag.map(|drag| drag.path).or(s.context_menu)
}
/// Record the dragged photo's recipe after the drag's edits, so later frames
/// recognize it as the drag's own change.
pub fn edited(ctx: &egui::Context, settings: &Settings) {
    let mut s = state(ctx);
    if let Some(drag) = &mut s.drag {
        drag.expected = settings.clone();
        store(ctx, s);
    }
}
pub fn button(ui: &mut egui::Ui) {
    let mut s = state(ui.ctx());
    let (toggle, opened) = settings::tools(ui, active(ui.ctx()), &mut s.configuration);
    s.configuration_menu = opened;
    store(ui.ctx(), s);
    if toggle {
        toggle_target(ui.ctx(), Target::Basic);
    }
}

pub fn receive(ctx: &egui::Context, reply: ColorPickReply) {
    let mut s = state(ctx);
    if let Some(drag) = &mut s.drag {
        if drag.token == reply.token && drag.path == reply.path && drag.target.tool() == reply.tool
        {
            match reply.sample {
                Ok(sample) => {
                    let weights = sample.weights;
                    drag.weights = Some(weights);
                    s.weights = weights;
                    if drag.target == Target::Advanced {
                        if sample.color[2] > 0.5 {
                            let range = foveonic::render::AdvancedColorRange {
                                hue: sample.color[0],
                                saturation: [
                                    (sample.color[1] - 0.25).max(0.),
                                    (sample.color[1] + 0.25).min(1.),
                                ],
                                ..Default::default()
                            };
                            drag.range = drag.ranges.push(range);
                            if let Some(index) = drag.range {
                                super::advanced_color::select(ctx, index);
                                drag.feedback = Some(crate::activity::Event::Operation(
                                    "Color selected.".into(),
                                ));
                            } else {
                                drag.feedback = Some(crate::activity::Event::Error(
                                    "The range list is full".into(),
                                ));
                            }
                        } else {
                            drag.feedback = Some(crate::activity::Event::Error(
                                "Choose a more colorful area".into(),
                            ));
                        }
                    } else if let Some((index, _)) = weights
                        .iter()
                        .enumerate()
                        .filter(|(_, w)| **w > 0.)
                        .max_by(|a, b| a.1.total_cmp(b.1))
                    {
                        s.selected = index;
                        drag.feedback =
                            Some(crate::activity::Event::Operation("Color selected.".into()));
                    } else {
                        drag.feedback = Some(crate::activity::Event::Error(
                            "No reliable color at this point".into(),
                        ));
                    }
                }
                Err(error) => {
                    drag.feedback = Some(crate::activity::Event::Error(error));
                    drag.weights = Some([0.; 8]);
                }
            }
        }
    }
    store(ctx, s);
}

/// Relative edits that move each weighted band by its share of `amount`, so
/// every photo moves by the same amount and clamps at its own limits.
fn band_edits(weights: [f32; 8], channel: Channel, amount: f32) -> impl Iterator<Item = Edit> {
    let (name, range, mapping) = channel.control();
    weights
        .into_iter()
        .enumerate()
        .filter(move |(_, weight)| *weight != 0. && amount != 0.)
        .map(move |(band, weight)| Edit::Number {
            pointer: format!("/photographic/color_editor/bands/{band}/{name}"),
            change: NumberChange::Relative(f64::from(amount * weight)),
            range,
            mapping,
        })
}

fn adjusted_amount(original: f32, channel: Channel, amount: f32) -> f32 {
    if amount == 0. {
        return original;
    }
    if matches!(channel, Channel::Saturation) {
        let range = PerceptualControls::SATURATION_CONTROL_RANGE;
        PerceptualControls::saturation_from_control(
            (PerceptualControls::saturation_to_control(original) + amount)
                .clamp(*range.start(), *range.end()),
        )
    } else {
        (original + amount).clamp(-1., 1.)
    }
}

/// The pointer response and normalized position use the viewport's actual painted geometry.
/// Pending picks retain the gesture baseline, including when the mouse is released first.
/// Returns the gesture's edits for every editing photo. An Advanced pick adds
/// a range only when every editing photo has the same range list (`shared_ranges`).
pub fn photo(
    ui: &egui::Ui,
    response: &egui::Response,
    uv: Option<[f32; 2]>,
    path: &Path,
    settings: &Settings,
    allowed: bool,
    shared_ranges: bool,
) -> (Vec<Edit>, Option<ColorPickRequest>) {
    let mut s = state(ui.ctx());
    if (!active(ui.ctx()) && s.drag.is_none()) || !allowed || !settings.color_editor_enabled {
        s.drag = None;
        s.context_menu = None;
        store(ui.ctx(), s);
        return (Vec::new(), None);
    }
    let (down, position, alt, pan) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.interact_pos(),
            i.modifiers.alt,
            i.key_down(egui::Key::Space),
        )
    });
    if active(ui.ctx()) {
        response.context_menu(|ui| settings::controls(ui, &mut s.configuration));
    }
    s.context_menu = (active(ui.ctx()) && response.context_menu_opened()).then(|| path.to_owned());
    if active(ui.ctx()) && response.hovered() && !pan {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if pan && active(ui.ctx()) {
        s.drag = None;
        store(ui.ctx(), s);
        return (Vec::new(), None);
    }
    let mut request = None;
    if s.drag
        .as_ref()
        .is_some_and(|d| d.path != path || d.expected != *settings)
    {
        s.drag = None;
    }
    if let Some(event) = s.drag.as_mut().and_then(|drag| drag.feedback.take()) {
        crate::activity::communicate(ui.ctx(), event);
    }
    // A fresh press is required after a recipe/photo change or while Space pans.
    let pressed = ui.input(|i| i.pointer.primary_pressed());
    if pressed && !response.is_pointer_button_down_on() {
        s.drag = None;
    }
    if active(ui.ctx()) && pressed && response.is_pointer_button_down_on() && !pan {
        if s.target == Target::Advanced && !shared_ranges {
            crate::activity::communicate(
                ui.ctx(),
                crate::activity::Event::Error(
                    "Pick a color range when the editing photographs have the same range list"
                        .into(),
                ),
            );
        } else if let (Some(uv), Some(anchor)) = (uv, position) {
            s.generation += 1;
            s.drag = Some(Drag {
                token: s.generation,
                target: s.target,
                range: None,
                path: path.to_owned(),
                anchor,
                position: anchor,
                configuration: s.configuration,
                moved: false,
                released: false,
                amounts: [0.; 3],
                applied: None,
                ranges: settings.photographic.color_editor.advanced,
                expected: settings.clone(),
                weights: None,
                feedback: None,
            });
            crate::activity::communicate(
                ui.ctx(),
                crate::activity::Event::Operation("Selecting color…".into()),
            );
            request = Some(ColorPickRequest {
                tool: s.target.tool(),
                token: s.generation,
                path: path.to_owned(),
                settings: settings.clone(),
                uv,
            });
        }
    }
    let mut edits = Vec::new();
    let mono = settings.is_monochrome();
    if let Some(drag) = &mut s.drag {
        let previous = drag.position;
        if !drag.released && (down || ui.input(|i| i.pointer.primary_released())) {
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
        if let Some(weights) = drag.weights {
            let applied = drag.applied.unwrap_or_default();
            match drag.target {
                Target::Basic if drag.moved => {
                    for channel in Channel::ALL {
                        if !mono || matches!(channel, Channel::Lightness) {
                            let i = channel.index();
                            edits.extend(band_edits(
                                weights,
                                channel,
                                drag.amounts[i] - applied[i],
                            ));
                        }
                    }
                    drag.applied = Some(drag.amounts);
                }
                // The range list is shared, so the whole list is set on every photo.
                Target::Advanced if drag.applied != Some(drag.amounts) => {
                    if let Some(index) = drag.range {
                        let mut ranges = drag.ranges;
                        let adjustment = &mut ranges.ranges[index].adjustment;
                        for (channel, value) in [
                            (Channel::Hue, &mut adjustment.hue),
                            (Channel::Saturation, &mut adjustment.saturation),
                            (Channel::Lightness, &mut adjustment.lightness),
                        ] {
                            if !mono || matches!(channel, Channel::Lightness) {
                                *value =
                                    adjusted_amount(*value, channel, drag.amounts[channel.index()]);
                            }
                        }
                        edits.push(Edit::set(field::COLOR_RANGES, &ranges));
                        drag.applied = Some(drag.amounts);
                    }
                }
                _ => {}
            }
        }
        drag.released |= !down;
        if drag.released {
            photo_tools::release(ui.ctx(), Tool::ColorRange);
        }
        if drag.released && drag.weights.is_some() {
            s.drag = None;
        }
    }
    store(ui.ctx(), s);
    (edits, request)
}

#[cfg(test)]
#[path = "direct_color_test.rs"]
mod tests;
