//! Applies explicit property edits and draws mixed-value controls for viewport-selected photographs.

use eframe::egui;
use foveonic::render::Settings;
use serde::Serialize;
use serde_json::Value;
use std::{ops::RangeInclusive, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumberChange {
    Relative(f64),
    Set(f64),
    Reset(f64),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub enum Mapping {
    #[default]
    Identity,
    Saturation,
}
impl Mapping {
    pub(crate) fn control(self, value: f64) -> f64 {
        match self {
            Self::Identity => value,
            Self::Saturation => f64::from(
                foveonic::render::PerceptualControls::saturation_to_control(value as f32),
            ),
        }
    }
    fn stored(self, value: f64) -> f64 {
        match self {
            Self::Identity => value,
            Self::Saturation => f64::from(
                foveonic::render::PerceptualControls::saturation_from_control(value as f32),
            ),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Remove {
        pointer: String,
    },
    Optional {
        pointer: String,
        enabled: bool,
    },
    Number {
        pointer: String,
        change: NumberChange,
        range: [f64; 2],
        mapping: Mapping,
    },
    Assign {
        pointer: String,
        value: Value,
    },
}
/// A recipe field and the type of its value, so a writer names what it changes.
pub struct Field<T>(&'static str, std::marker::PhantomData<fn() -> T>);
impl<T> Clone for Field<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Field<T> {}
impl<T> Field<T> {
    const fn new(pointer: &'static str) -> Self {
        Self(pointer, std::marker::PhantomData)
    }
}
impl Edit {
    /// Set `field` to `value`.
    pub fn set<T: Serialize>(field: Field<T>, value: &T) -> Self {
        Self::Assign {
            pointer: field.0.into(),
            value: serde_json::to_value(value).expect("recipe fields serialize"),
        }
    }
    pub fn pointer(&self) -> &str {
        match self {
            Self::Number { pointer, .. }
            | Self::Assign { pointer, .. }
            | Self::Optional { pointer, .. }
            | Self::Remove { pointer } => pointer,
        }
    }
}

/// Fields that tools and actions outside the panel set.
pub mod field {
    use super::Field;
    use foveonic::render::{AutoExposureCompensation, ColorEditorControls, WhiteBalanceBias};
    pub const CROP: Field<foveonic::crop::Crop> = Field::new("/crop");
    pub const ROTATION: Field<foveonic::rotation::Rotation> = Field::new("/rotation");
    pub const WHITE_BALANCE_BIAS: Field<WhiteBalanceBias> = Field::new("/white_balance_bias");
    pub const DEHAZE_SHADOW: Field<Option<[f32; 3]>> = Field::new("/dehaze/shadow");
    pub const COLOR_EDITOR: Field<ColorEditorControls> = Field::new("/photographic/color_editor");
    pub const PREVENT_COLOR_CLIPPING: Field<bool> = Field::new("/prevent_color_clipping");
    pub const EXPOSURE_AUTO_COMPENSATION: Field<AutoExposureCompensation> =
        Field::new("/exposure_auto_compensation");
}
/// Each descriptor changes one authored property. Transient render policy stays on its owner.
pub fn apply(settings: &mut Settings, edits: &[Edit]) -> bool {
    if edits.is_empty() {
        return false;
    }
    let Ok(mut encoded) = serde_json::to_value(&*settings) else {
        return false;
    };
    let mut changed = false;
    for edit in edits {
        if let Edit::Remove { pointer } = edit {
            if let Some((parent, key)) = pointer.rsplit_once('/') {
                if let Some(object) = encoded.pointer_mut(parent).and_then(Value::as_object_mut) {
                    changed |= object
                        .remove(&key.replace("~1", "/").replace("~0", "~"))
                        .is_some();
                }
            }
            continue;
        }
        if let Edit::Assign { pointer, value } = edit {
            if encoded.pointer(pointer).is_none() {
                if let Some((parent, key)) = pointer.rsplit_once('/') {
                    if let Some(object) = encoded.pointer_mut(parent).and_then(Value::as_object_mut)
                    {
                        object.insert(key.replace("~1", "/").replace("~0", "~"), value.clone());
                        changed = true;
                    }
                }
            }
        }
        let Some(slot) = encoded.pointer_mut(edit.pointer()) else {
            continue;
        };
        let next = match edit {
            Edit::Remove { .. } => unreachable!("removal handled above"),
            Edit::Optional { enabled: false, .. } => Value::Null,
            Edit::Optional { enabled: true, .. } => continue,
            Edit::Assign { value, .. } => value.clone(),
            Edit::Number {
                change,
                range,
                mapping,
                ..
            } => {
                let Some(old) = slot.as_f64() else { continue };
                let value = match change {
                    NumberChange::Relative(delta) => mapping.control(old) + delta,
                    NumberChange::Set(value) | NumberChange::Reset(value) => *value,
                }
                .clamp(range[0], range[1]);
                let value = mapping.stored(value);
                if !value.is_finite() {
                    continue;
                }
                if slot.is_u64() {
                    Value::from(value.round().max(0.) as u64)
                } else if slot.is_i64() {
                    Value::from(value.round() as i64)
                } else {
                    Value::from(value)
                }
            }
        };
        changed |= *slot != next;
        *slot = next;
    }
    if !changed {
        return false;
    }
    let Ok(mut result) = serde_json::from_value::<Settings>(encoded) else {
        return false;
    };
    result.working_scale = settings.working_scale;
    result.preview_region = settings.preview_region;
    result.dehaze_estimate = settings.dehaze_estimate;
    result.processing_sections = settings.processing_sections;
    result.color_range_preview = settings.color_range_preview;
    result.adaptive_tone_tuning = settings.adaptive_tone_tuning;
    if result.validate_values().is_err() {
        return false;
    }
    *settings = result;
    true
}

/// Explicit widget descriptors own relative edits. Remaining changed editor fields are assignments.
pub fn assignments(before: &Settings, after: &Settings, explicit: &[Edit]) -> Vec<Edit> {
    fn visit(path: String, a: &Value, b: &Value, explicit: &[Edit], edits: &mut Vec<Edit>) {
        if a == b
            || explicit
                .iter()
                .any(|e| path == e.pointer() || path.starts_with(&format!("{}/", e.pointer())))
        {
            return;
        }
        match (a, b) {
            (Value::Object(a), Value::Object(b)) => {
                for key in a.keys().filter(|key| !b.contains_key(*key)) {
                    let pointer = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                    if !explicit.iter().any(|e| e.pointer() == pointer) {
                        edits.push(Edit::Remove { pointer });
                    }
                }
                for (key, value) in b {
                    visit(
                        format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
                        a.get(key).unwrap_or(&Value::Null),
                        value,
                        explicit,
                        edits,
                    );
                }
            }
            (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
                for (index, (a, b)) in a.iter().zip(b).enumerate() {
                    visit(format!("{path}/{index}"), a, b, explicit, edits);
                }
            }
            _ if explicit
                .iter()
                .any(|edit| edit.pointer().starts_with(&format!("{path}/"))) => {}
            _ => edits.push(Edit::Assign {
                pointer: path,
                value: b.clone(),
            }),
        }
    }
    let mut edits = Vec::new();
    if let (Ok(a), Ok(b)) = (serde_json::to_value(before), serde_json::to_value(after)) {
        visit(String::new(), &a, &b, explicit, &mut edits);
    }
    edits
}

#[derive(Clone)]
struct Property {
    pointer: String,
    mapping: Mapping,
}
#[derive(Clone, Default)]
struct State {
    values: Arc<[Value]>,
    numeric: Arc<[Value]>,
    mixed_override: Option<bool>,
    metadata: Arc<[Arc<foveonic::source::Metadata>]>,
    ranges: std::collections::BTreeMap<String, [f64; 2]>,
    optional_ready: std::collections::BTreeMap<String, bool>,
    edits: Vec<Edit>,
    property: Option<Property>,
    choice_clicked: bool,
}
fn id() -> egui::Id {
    egui::Id::new("viewport editing controls")
}
pub fn begin_with_numeric(ctx: &egui::Context, authored: Vec<Settings>, numeric: Vec<Settings>) {
    let encode = |settings: Vec<Settings>| -> Arc<[Value]> {
        settings
            .iter()
            .filter_map(|s| serde_json::to_value(s).ok())
            .collect::<Vec<_>>()
            .into()
    };
    let values = encode(authored);
    let numeric = encode(numeric);
    ctx.data_mut(|data| {
        data.insert_temp(
            id(),
            State {
                values,
                numeric,
                ..Default::default()
            },
        )
    });
}
pub fn set_metadata(ctx: &egui::Context, metadata: Vec<Arc<foveonic::source::Metadata>>) {
    if !active(ctx) {
        return;
    }
    ctx.data_mut(|d| {
        let mut s = d.get_temp::<State>(id()).unwrap();
        s.metadata = metadata.into();
        d.insert_temp(id(), s);
    });
}
pub fn set_range(ctx: &egui::Context, pointer: &str, range: [f64; 2]) {
    if !active(ctx) {
        return;
    }
    ctx.data_mut(|d| {
        let mut s = d.get_temp::<State>(id()).unwrap();
        s.ranges.insert(pointer.into(), range);
        d.insert_temp(id(), s);
    });
}
pub fn control_range<T: egui::emath::Numeric>(
    ctx: &egui::Context,
    fallback: RangeInclusive<T>,
) -> RangeInclusive<T> {
    property(ctx)
        .and_then(|p| {
            ctx.data_mut(|d| d.get_temp::<State>(id()))
                .and_then(|s| s.ranges.get(&p.pointer).copied())
        })
        .map_or(fallback, |r| T::from_f64(r[0])..=T::from_f64(r[1]))
}
fn available(ctx: &egui::Context, pointer: &str) -> bool {
    if !active(ctx) || pointer.is_empty() {
        return true;
    }
    ctx.data_mut(|d| d.get_temp::<State>(id())).is_none_or(|s| {
        let exists = s.values.iter().enumerate().all(|(i, v)| {
            v.pointer(pointer).is_some()
                || s.numeric
                    .get(i)
                    .is_some_and(|v| v.pointer(pointer).is_some())
        });
        exists
            && s.metadata.iter().all(|m| {
                let Ok(c) = foveonic::render::ControlCapabilities::from_metadata(m) else {
                    return false;
                };
                if pointer.starts_with("/merrill_")
                    || pointer.starts_with("/color_field")
                    || matches!(pointer, "/highlight_mode" | "/highlight_threshold_ev")
                {
                    return c.family == foveonic::source::ProcessingFamily::Merrill;
                }
                if pointer == "/color_dq_gain" {
                    return c.color_dq_adjustable;
                }
                if pointer == "/monochrome_smoothness" {
                    return c.family == foveonic::source::ProcessingFamily::Quattro;
                }
                if pointer.starts_with("/reconstruction/") {
                    return c.noise_adjustments;
                }
                true
            })
    })
}
pub fn camera_choice_available(ctx: &egui::Context, white_balance: bool, name: &str) -> bool {
    if name.is_empty() || !active(ctx) {
        return true;
    }
    ctx.data_mut(|d| d.get_temp::<State>(id())).is_none_or(|s| {
        s.metadata.iter().all(|m| {
            if white_balance {
                m.white_balance_presets().contains(&name)
            } else {
                foveonic::render::ControlCapabilities::from_metadata(m)
                    .map(|c| c.color_mode_presets(m).contains(&name))
                    .unwrap_or(false)
            }
        })
    })
}
pub fn finish(ctx: &egui::Context) -> Vec<Edit> {
    ctx.data_mut(|data| {
        data.remove_temp::<State>(id())
            .map_or_else(Vec::new, |s| s.edits)
    })
}
pub fn active(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.get_temp::<State>(id()))
        .is_some_and(|s| !s.values.is_empty())
}
/// More than one photo is being edited, so per-photo display choices such as
/// orientation cannot apply to the controls.
pub fn several(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.get_temp::<State>(id()))
        .is_some_and(|s| s.values.len() > 1)
}
fn property(ctx: &egui::Context) -> Option<Property> {
    ctx.data_mut(|d| d.get_temp::<State>(id()))?.property
}
pub(crate) fn observed_property(ctx: &egui::Context) -> Option<(String, Mapping)> {
    property(ctx).map(|property| (property.pointer, property.mapping))
}
pub fn different(ctx: &egui::Context, pointer: &str) -> bool {
    ctx.data_mut(|d| d.get_temp::<State>(id()))
        .is_some_and(|s| {
            let mut values = s.values.iter().map(|v| v.pointer(pointer));
            let first = values.next();
            first.is_some() && values.any(|v| Some(v) != first)
        })
}
pub fn mixed(ctx: &egui::Context) -> bool {
    if let Some(value) = ctx
        .data_mut(|d| d.get_temp::<State>(id()))
        .and_then(|s| s.mixed_override)
    {
        return value;
    }
    property(ctx).is_some_and(|p| different(ctx, &p.pointer))
}
pub fn compound<R>(
    ui: &mut egui::Ui,
    pointers: &[&str],
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let Some(first) = pointers.first() else {
        return contents(ui);
    };
    field(ui, *first, |ui| {
        let previous = ui
            .ctx()
            .data_mut(|d| d.get_temp::<State>(id()))
            .and_then(|s| s.mixed_override);
        let mixed = pointers.iter().any(|p| different(ui.ctx(), p));
        if active(ui.ctx()) {
            ui.ctx().data_mut(|d| {
                let mut s = d.get_temp::<State>(id()).unwrap();
                s.mixed_override = Some(mixed);
                d.insert_temp(id(), s);
            });
        }
        let result = contents(ui);
        if active(ui.ctx()) {
            ui.ctx().data_mut(|d| {
                let mut s = d.get_temp::<State>(id()).unwrap();
                s.mixed_override = previous;
                d.insert_temp(id(), s);
            });
        }
        result
    })
}
pub fn choice_clicked(ctx: &egui::Context) {
    if active(ctx) && property(ctx).is_some() {
        ctx.data_mut(|d| {
            let mut s = d.get_temp::<State>(id()).unwrap();
            s.choice_clicked = true;
            d.insert_temp(id(), s);
        });
    }
}
fn push(ctx: &egui::Context, edit: Edit) {
    if !active(ctx) {
        return;
    }
    ctx.data_mut(|d| {
        let mut s = d.get_temp::<State>(id()).unwrap();
        s.edits.push(edit);
        d.insert_temp(id(), s);
    });
}
pub fn field<R>(
    ui: &mut egui::Ui,
    pointer: impl Into<String>,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    mapped_field(ui, pointer, Mapping::Identity, contents)
}
pub fn mapped_field<R>(
    ui: &mut egui::Ui,
    pointer: impl Into<String>,
    mapping: Mapping,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    if !active(ui.ctx()) {
        return contents(ui);
    }
    let pointer = pointer.into();
    let enabled = available(ui.ctx(), &pointer);
    let previous = property(ui.ctx());
    if active(ui.ctx()) {
        ui.ctx().data_mut(|d| {
            let mut s = d.get_temp::<State>(id()).unwrap();
            s.property = Some(Property {
                pointer: pointer.clone(),
                mapping,
            });
            d.insert_temp(id(), s);
        });
    }
    let response = ui.add_enabled_ui(enabled, contents);
    if !enabled {
        response
            .response
            .on_disabled_hover_text("This adjustment is not shared by every selected photo.");
    }
    let result = response.inner;
    if active(ui.ctx()) {
        ui.ctx().data_mut(|d| {
            let mut s = d.get_temp::<State>(id()).unwrap();
            s.property = previous;
            d.insert_temp(id(), s);
        });
    }
    result
}
pub fn choice<T: Serialize, R>(
    ui: &mut egui::Ui,
    pointer: impl Into<String>,
    value: &mut T,
    contents: impl FnOnce(&mut egui::Ui, &mut T) -> R,
) -> R {
    if !active(ui.ctx()) {
        return contents(ui, value);
    }
    field(ui, pointer, |ui| {
        let before = serde_json::to_value(&*value).ok();
        if active(ui.ctx()) {
            ui.ctx().data_mut(|d| {
                let mut s = d.get_temp::<State>(id()).unwrap();
                s.choice_clicked = false;
                d.insert_temp(id(), s);
            });
        }
        let result = contents(ui, value);
        let after = serde_json::to_value(&*value).ok();
        let clicked = ui
            .ctx()
            .data_mut(|d| d.get_temp::<State>(id()))
            .is_some_and(|s| s.choice_clicked);
        if before != after || clicked {
            if let (Some(value), Some(p)) = (after, property(ui.ctx())) {
                push(
                    ui.ctx(),
                    Edit::Assign {
                        pointer: p.pointer,
                        value,
                    },
                );
            }
        }
        result
    })
}
pub fn optional_current(ctx: &egui::Context, enabled: bool) {
    if let Some(p) = property(ctx) {
        optional(ctx, p.pointer, enabled);
    }
}
pub fn set_optional_ready(ctx: &egui::Context, pointer: &str, ready: bool) {
    ctx.data_mut(|data| {
        if let Some(mut state) = data.get_temp::<State>(id()) {
            state.optional_ready.insert(pointer.into(), ready);
            data.insert_temp(id(), state);
        }
    });
}
pub fn optional_ready(ctx: &egui::Context, pointer: &str) -> bool {
    if !active(ctx) {
        return true;
    }
    ctx.data_mut(|data| data.get_temp::<State>(id()))
        .and_then(|state| state.optional_ready.get(pointer).copied())
        .unwrap_or(true)
}
pub fn optional_current_ready(ctx: &egui::Context) -> bool {
    property(ctx).is_none_or(|property| optional_ready(ctx, &property.pointer))
}
pub fn optional(ctx: &egui::Context, pointer: impl Into<String>, enabled: bool) {
    push(
        ctx,
        Edit::Optional {
            pointer: pointer.into(),
            enabled,
        },
    );
}
pub fn assign(ctx: &egui::Context, pointer: impl Into<String>, value: impl Serialize) {
    if let Ok(value) = serde_json::to_value(value) {
        push(
            ctx,
            Edit::Assign {
                pointer: pointer.into(),
                value,
            },
        );
    }
}
pub fn numeric_ready(ctx: &egui::Context) -> bool {
    if !active(ctx) || property(ctx).is_none() {
        return true;
    }
    numeric_values(ctx).is_some()
}

fn numeric_values(ctx: &egui::Context) -> Option<(Property, Vec<f64>)> {
    if !active(ctx) {
        return None;
    }
    let p = property(ctx)?;
    let state = ctx.data_mut(|d| d.get_temp::<State>(id()))?;
    let numbers = state
        .numeric
        .iter()
        .filter_map(|v| v.pointer(&p.pointer).and_then(Value::as_f64))
        .map(|v| p.mapping.control(v))
        .collect::<Vec<_>>();
    (numbers.len() == state.values.len()).then_some((p, numbers))
}
pub fn number_mixed(ctx: &egui::Context) -> bool {
    numeric_values(ctx).is_some_and(|(_, values)| values[1..].iter().any(|v| *v != values[0]))
}

pub fn number(ctx: &egui::Context, change: NumberChange, range: [f64; 2]) {
    if let Some(p) = property(ctx) {
        push(
            ctx,
            Edit::Number {
                pointer: p.pointer,
                change,
                range,
                mapping: p.mapping,
            },
        );
    }
}

/// The editing slider for one photo or several. The change is recorded as an
/// edit for each editing photo; saturated photos stay at their bound.
pub fn slider<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut T,
    range: RangeInclusive<T>,
    default: T,
    step: Option<f64>,
) -> egui::Response {
    let values =
        numeric_values(ui.ctx()).map_or_else(|| vec![value.to_f64()], |(_, numbers)| numbers);
    let bounds = [range.start().to_f64(), range.end().to_f64()];
    let step = step.or(T::INTEGRAL.then_some(1.));
    let (mut response, change) = selection_slider(ui, &values, bounds, step, default.to_f64());
    if let Some(change) = change {
        *value = T::from_f64(change.applied(value.to_f64(), bounds));
        number(ui.ctx(), change, bounds);
        response.mark_changed();
    }
    response
}

impl NumberChange {
    /// The value after this change, within `bounds`.
    fn applied(self, value: f64, bounds: [f64; 2]) -> f64 {
        match self {
            Self::Relative(delta) => value + delta,
            Self::Set(value) | Self::Reset(value) => value,
        }
        .clamp(bounds[0], bounds[1])
    }
}

/// A slider handle's radius. It insets the track at both ends, and presses
/// within it of a handle grab the handle instead of moving it.
pub(crate) const HANDLE_RADIUS: f32 = 6.;
const TRACK_HEIGHT: f32 = 6.;

/// Where a drag began and how far it has moved the photos so far.
#[derive(Clone, Copy)]
struct Drag {
    start_x: f32,
    applied: f64,
}

/// Shows the photos' values as a range that collapses to one handle when they
/// agree. A press outside the range moves the nearer handle there and every
/// photo by the same offset, so with one photo it is an ordinary slider; a
/// press inside the range or on a handle grabs it. A drag then shifts every
/// photo by the pointer's travel, and a double-click resets. Changes snap to
/// `step`, or else to round values within a pixel, like egui's slider.
fn selection_slider(
    ui: &mut egui::Ui,
    values: &[f64],
    bounds: [f64; 2],
    step: Option<f64>,
    default: f64,
) -> (egui::Response, Option<NumberChange>) {
    let span = bounds[1] - bounds[0];
    let low = values
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min)
        .clamp(bounds[0], bounds[1]);
    let high = values
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max)
        .clamp(bounds[0], bounds[1]);
    // Sensing only drags lets a press move the value at once; sensing clicks too
    // would make egui wait to tell a click from a drag.
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.spacing().slider_width, ui.spacing().interact_size.y),
        egui::Sense::drag(),
    );
    response.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), low, ""));
    let track = egui::Rect::from_center_size(
        rect.center(),
        egui::vec2((rect.width() - 2. * HANDLE_RADIUS).max(1.), TRACK_HEIGHT),
    );
    let x = |v: f64| track.left() + ((v - bounds[0]) / span) as f32 * track.width();
    let value_at = |pos_x: f32| f64::from((pos_x - track.left()) / track.width()) * span;
    let half_pixel = span / f64::from(track.width()) * 0.5;
    let snap = |delta: f64| match step {
        Some(step) => (delta / step).round() * step,
        None => egui::emath::smart_aim::best_in_range_f64(delta - half_pixel, delta + half_pixel),
    };
    let color = ui.style().interact(&response).fg_stroke.color;
    let rounding = TRACK_HEIGHT / 2.;
    ui.painter().rect_filled(
        track,
        rounding,
        crate::widgets::slider_track_fill(ui.visuals()),
    );
    ui.painter().rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(x(low), track.top()),
            egui::pos2(x(high), track.bottom()),
        ),
        rounding,
        ui.visuals().selection.bg_fill,
    );
    let handles: &[f64] = if low == high { &[low] } else { &[low, high] };
    for &v in handles {
        ui.painter().circle(
            egui::pos2(x(v), track.center().y),
            HANDLE_RADIUS,
            ui.visuals().widgets.inactive.bg_fill,
            egui::Stroke::new(1_f32, color),
        );
    }
    let drag_id = response.id.with("drag");
    let double_clicked = response.hovered()
        && ui.input(|i| {
            i.pointer
                .button_double_clicked(egui::PointerButton::Primary)
        });
    let change = if double_clicked {
        Some(NumberChange::Reset(default))
    } else if response.drag_started() {
        let pos = response.interact_pointer_pos();
        if let Some(pos) = pos {
            ui.data_mut(|d| {
                d.insert_temp(
                    drag_id,
                    Drag {
                        start_x: pos.x,
                        applied: 0.,
                    },
                )
            });
        }
        pos.filter(|pos| pos.x < x(low) - HANDLE_RADIUS || pos.x > x(high) + HANDLE_RADIUS)
            .map(|pos| {
                let target = (bounds[0] + value_at(pos.x)).clamp(bounds[0], bounds[1]);
                let nearest = if target < low { low } else { high };
                NumberChange::Relative(snap(target - nearest))
            })
    } else if response.dragged() {
        let drag = ui.data_mut(|d| d.get_temp::<Drag>(drag_id));
        drag.zip(response.interact_pointer_pos())
            .map(|(drag, pos)| {
                // Measuring from the press, not frame to frame, keeps snapping from
                // adding up and leaves the photos under the pointer.
                let total = snap(value_at(pos.x) - value_at(drag.start_x));
                ui.data_mut(|d| {
                    d.insert_temp(
                        drag_id,
                        Drag {
                            applied: total,
                            ..drag
                        },
                    )
                });
                NumberChange::Relative(total - drag.applied)
            })
    } else {
        None
    };
    if response.drag_stopped() {
        ui.data_mut(|d| d.remove::<Drag>(drag_id));
    }
    let change = change.filter(|change| !matches!(change, NumberChange::Relative(0.)));
    (response, change)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_field_sets_its_recipe_value() {
        let mut settings = Settings::default();
        settings.crop.bounds = [0.1, 0.2, 0.9, 0.8];
        settings.rotation.turns = 1;
        settings.white_balance_bias.warmth = 0.3;
        settings.dehaze.shadow = Some([0.1, 0.2, 0.3]);
        settings.photographic.color_editor.bands[0].saturation = -0.2;
        settings.prevent_color_clipping = !settings.prevent_color_clipping;
        settings.exposure_auto_compensation = foveonic::render::AutoExposureCompensation::Dial;
        let edits = [
            Edit::set(field::CROP, &settings.crop),
            Edit::set(field::ROTATION, &settings.rotation),
            Edit::set(field::WHITE_BALANCE_BIAS, &settings.white_balance_bias),
            Edit::set(field::DEHAZE_SHADOW, &settings.dehaze.shadow),
            Edit::set(field::COLOR_EDITOR, &settings.photographic.color_editor),
            Edit::set(
                field::PREVENT_COLOR_CLIPPING,
                &settings.prevent_color_clipping,
            ),
            Edit::set(
                field::EXPOSURE_AUTO_COMPENSATION,
                &settings.exposure_auto_compensation,
            ),
        ];
        for edit in &edits {
            let mut edited = Settings::default();
            assert!(
                apply(&mut edited, std::slice::from_ref(edit)),
                "{}",
                edit.pointer()
            );
        }
        let mut edited = Settings::default();
        assert!(apply(&mut edited, &edits));
        assert_eq!(edited, settings);
    }

    /// Draw a slider for `values` in [0, 1] with these events and return its rect and change.
    fn slider_frame(
        ctx: &egui::Context,
        values: &[f64],
        events: Vec<egui::Event>,
    ) -> (egui::Rect, Option<NumberChange>) {
        stepped_slider_frame(ctx, values, None, events)
    }
    fn stepped_slider_frame(
        ctx: &egui::Context,
        values: &[f64],
        step: Option<f64>,
        events: Vec<egui::Event>,
    ) -> (egui::Rect, Option<NumberChange>) {
        let mut out = (egui::Rect::NOTHING, None);
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (response, change) = selection_slider(ui, values, [0., 1.], step, 0.5);
                    out = (response.rect, change);
                });
            },
        );
        out
    }
    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }
    /// The track position of `value`, matching the slider's 6-point inset.
    fn at(rect: egui::Rect, value: f32) -> egui::Pos2 {
        egui::pos2(
            rect.left() + 6. + (rect.width() - 12.) * value,
            rect.center().y,
        )
    }
    fn relative(change: Option<NumberChange>) -> f64 {
        match change {
            Some(NumberChange::Relative(delta)) => delta,
            other => panic!("expected a relative change, got {other:?}"),
        }
    }

    #[test]
    fn a_press_moves_the_nearer_handle_at_once_and_every_photo_by_that_offset() {
        let ctx = egui::Context::default();
        let (rect, _) = slider_frame(&ctx, &[0.5], vec![]);
        let press = at(rect, 0.9);
        let (_, change) = slider_frame(
            &ctx,
            &[0.5],
            vec![egui::Event::PointerMoved(press), button(press, true)],
        );
        assert!(
            (relative(change) - 0.4).abs() < 1e-3,
            "one photo jumps on press"
        );
        slider_frame(&ctx, &[0.9], vec![button(press, false)]);

        let range = [0.2, 0.6];
        let low_press = at(rect, 0.1);
        let (_, change) = slider_frame(
            &ctx,
            &range,
            vec![
                egui::Event::PointerMoved(low_press),
                button(low_press, true),
            ],
        );
        assert!(
            (relative(change) + 0.1).abs() < 1e-3,
            "the low handle is nearer, so every photo moves down by its offset"
        );
    }

    #[test]
    fn a_stepped_press_closer_than_half_a_step_changes_nothing() {
        let ctx = egui::Context::default();
        let slider = |events| stepped_slider_frame(&ctx, &[0.5], Some(0.25), events);
        let (rect, _) = slider(vec![]);
        let press = at(rect, 0.6);
        let (_, change) = slider(vec![egui::Event::PointerMoved(press), button(press, true)]);
        assert!(change.is_none(), "{change:?}");
    }

    #[test]
    fn a_press_inside_the_range_or_on_a_handle_does_not_move_it() {
        // A fresh context per case, so the two presses are not a double-click.
        for (values, point) in [(&[0.2, 0.6][..], 0.4), (&[0.5][..], 0.5)] {
            let ctx = egui::Context::default();
            let (rect, _) = slider_frame(&ctx, values, vec![]);
            let press = at(rect, point);
            let (_, change) = slider_frame(
                &ctx,
                values,
                vec![egui::Event::PointerMoved(press), button(press, true)],
            );
            assert!(change.is_none(), "{values:?} pressed at {point}");
            let (_, change) = slider_frame(&ctx, values, vec![button(press, false)]);
            assert!(change.is_none(), "{values:?} released at {point}");
        }
    }

    #[test]
    fn a_drag_shifts_every_photo_from_the_press_and_a_double_click_resets() {
        let ctx = egui::Context::default();
        let (rect, _) = slider_frame(&ctx, &[0.5], vec![]);
        let start = at(rect, 0.5);
        slider_frame(
            &ctx,
            &[0.5],
            vec![egui::Event::PointerMoved(start), button(start, true)],
        );
        let (_, change) =
            slider_frame(&ctx, &[0.5], vec![egui::Event::PointerMoved(at(rect, 0.7))]);
        assert!(
            (relative(change) - 0.2).abs() < 1e-3,
            "the drag moves by its travel"
        );
        slider_frame(&ctx, &[0.7], vec![button(at(rect, 0.7), false)]);
        let point = at(rect, 0.7);
        slider_frame(&ctx, &[0.7], vec![button(point, true)]);
        slider_frame(&ctx, &[0.7], vec![button(point, false)]);
        slider_frame(&ctx, &[0.7], vec![button(point, true)]);
        let (_, change) = slider_frame(&ctx, &[0.7], vec![button(point, false)]);
        assert!(matches!(change, Some(NumberChange::Reset(v)) if v == 0.5));
    }

    #[test]
    fn relative_edits_compress_at_limits_and_preserve_unrelated_settings() {
        let mut a = Settings::new("Sunlight", "Standard");
        let mut b = a.clone();
        a.exposure = 3.;
        b.exposure = 1.;
        b.saturation = 0.4;
        let edit = Edit::Number {
            pointer: "/exposure".into(),
            change: NumberChange::Relative(1.),
            range: [-3., 3.],
            mapping: Mapping::Identity,
        };
        assert!(!apply(&mut a, std::slice::from_ref(&edit)));
        assert!(apply(&mut b, std::slice::from_ref(&edit)));
        assert_eq!((a.exposure, b.exposure), (3., 2.));
        assert_eq!(b.saturation, 0.4);
        apply(
            &mut b,
            &[Edit::Number {
                pointer: "/exposure".into(),
                change: NumberChange::Reset(0.),
                range: [-3., 3.],
                mapping: Mapping::Identity,
            }],
        );
        assert_eq!(b.exposure, 0.);
    }
    #[test]
    fn assign_preserves_workspace_policy() {
        let mut s = Settings::new("Sunlight", "Standard");
        s.working_scale = foveonic::render::Scale::new(8).unwrap();
        s.preview_region = Some([1, 2, 30, 40]);
        s.dehaze_estimate = true;
        assert!(apply(
            &mut s,
            &[Edit::Assign {
                pointer: "/color_mode".into(),
                value: Value::from("Vivid")
            }]
        ));
        assert_eq!(s.color_mode, "Vivid");
        assert_eq!(s.working_scale.divisor(), 8);
        assert_eq!(s.preview_region, Some([1, 2, 30, 40]));
        assert!(s.dehaze_estimate);
    }
    #[test]
    fn live_group_drag_moves_other_photos_when_active_is_already_at_limit() {
        let ctx = egui::Context::default();
        let mut a = Settings::new("Sunlight", "Standard");
        a.exposure = 4.;
        let mut b = a.clone();
        b.exposure = 1.;
        b.saturation = 0.3;
        let draw = |time: f64, events: Vec<egui::Event>, a: &Settings, b: &Settings| {
            let mut edits = Vec::new();
            let mut value = a.exposure;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(420., 90.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    begin_with_numeric(ctx, vec![a.clone(), b.clone()], vec![a.clone(), b.clone()]);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.spacing_mut().slider_width = 300.;
                        field(ui, "/exposure", |ui| {
                            slider(ui, &mut value, -4. ..=4., 0., None);
                        });
                    });
                    edits = finish(ctx);
                },
            );
            edits
        };
        assert!(draw(0., vec![], &a, &b).is_empty());
        let at = egui::pos2(160., 18.);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        draw(
            1.,
            vec![egui::Event::PointerMoved(at), button(true)],
            &a,
            &b,
        );
        let edits = draw(
            1.1,
            vec![egui::Event::PointerMoved(at + egui::vec2(30., 0.))],
            &a,
            &b,
        );
        assert!(
            matches!(edits.as_slice(),[Edit::Number {change:NumberChange::Relative(delta),..}] if *delta>0.)
        );
        assert!(!apply(&mut a, &edits));
        assert!(apply(&mut b, &edits));
        assert_eq!(a.exposure, 4.);
        assert!(b.exposure > 1. && b.exposure < 4.);
        assert_eq!(b.saturation, 0.3);
        let previous = b.exposure;
        let edits = draw(
            1.2,
            vec![egui::Event::PointerMoved(at + egui::vec2(60., 0.))],
            &a,
            &b,
        );
        assert!(
            matches!(edits.as_slice(),[Edit::Number {change:NumberChange::Relative(delta),..}] if *delta>0.),
            "each frame of a sustained drag must contribute its pointer movement"
        );
        assert!(apply(&mut b, &edits));
        assert!(b.exposure > previous);
        let previous = b.exposure;
        let edits = draw(
            1.3,
            vec![egui::Event::PointerMoved(at + egui::vec2(30., 0.))],
            &a,
            &b,
        );
        assert!(
            matches!(edits.as_slice(),[Edit::Number {change:NumberChange::Relative(delta),..}] if *delta<0.),
            "reversing the pointer must reverse the relative edit"
        );
        assert!(apply(&mut a, &edits));
        assert!(apply(&mut b, &edits));
        assert!(b.exposure < previous);
    }

    #[test]
    fn stepped_group_drag_accumulates_small_movements_and_reverses() {
        let ctx = egui::Context::default();
        let mut photos = [
            Settings::new("Sunlight", "Standard"),
            Settings::new("Sunlight", "Standard"),
        ];
        photos[1].exposure = -1.;
        let frame = |time: f64, events: Vec<egui::Event>, photos: &mut [Settings; 2]| {
            let mut edits = Vec::new();
            let mut value = photos[0].exposure;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(420., 90.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    begin_with_numeric(ctx, photos.to_vec(), photos.to_vec());
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.spacing_mut().slider_width = 300.;
                        field(ui, "/exposure", |ui| {
                            slider(ui, &mut value, -4. ..=4., 0., Some(1.));
                        });
                    });
                    edits = finish(ctx);
                },
            );
            for photo in photos {
                apply(photo, &edits);
            }
        };
        frame(0., vec![], &mut photos);
        let start = egui::pos2(160., 18.);
        frame(
            1.,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
            &mut photos,
        );
        for index in 1..=6 {
            frame(
                1. + f64::from(index) * 0.1,
                vec![egui::Event::PointerMoved(
                    start + egui::vec2(index as f32 * 4., 0.),
                )],
                &mut photos,
            );
        }
        assert_eq!([photos[0].exposure, photos[1].exposure], [1., 0.]);
        for index in (0..6).rev() {
            frame(
                2. + f64::from(6 - index) * 0.1,
                vec![egui::Event::PointerMoved(
                    start + egui::vec2(index as f32 * 4., 0.),
                )],
                &mut photos,
            );
        }
        assert_eq!([photos[0].exposure, photos[1].exposure], [0., -1.]);
    }
    #[test]
    fn explicit_numeric_child_survives_optional_parent_normalization() {
        let mut before = Settings::new("Sunlight", "Standard");
        before.reconstruction = Some(Default::default());
        let mut after = before.clone();
        after.reconstruction = None;
        after.exposure_auto = true;
        let edits = [Edit::Number {
            pointer: "/reconstruction/chroma".into(),
            change: NumberChange::Relative(1.),
            range: [0., 10.],
            mapping: Mapping::Identity,
        }];
        let fallback = assignments(&before, &after, &edits);
        assert!(fallback
            .iter()
            .all(|e| !e.pointer().starts_with("/reconstruction")));
        assert!(fallback.iter().any(|e| e.pointer() == "/exposure_auto"));
    }
    #[test]
    fn group_reset_and_explicit_assignment_share_values_without_overwriting_other_edits() {
        let mut a = Settings::new("Sunlight", "Standard");
        let mut b = a.clone();
        a.exposure = -2.;
        b.exposure = 3.;
        b.tone.contrast = 0.4;
        let reset = Edit::Number {
            pointer: "/exposure".into(),
            change: NumberChange::Reset(0.),
            range: [-4., 4.],
            mapping: Mapping::Identity,
        };
        for s in [&mut a, &mut b] {
            assert!(apply(s, std::slice::from_ref(&reset)));
            assert_eq!(s.exposure, 0.);
        }
        let set = Edit::Number {
            pointer: "/exposure".into(),
            change: NumberChange::Set(1.25),
            range: [-4., 4.],
            mapping: Mapping::Identity,
        };
        for s in [&mut a, &mut b] {
            assert!(apply(s, std::slice::from_ref(&set)));
            assert_eq!(s.exposure, 1.25);
        }
        assert_eq!(b.tone.contrast, 0.4);
    }
    #[test]
    fn optional_range_names_insert_and_clear_without_copying_adjustments() {
        let mut source = Settings::new("Sunlight", "Standard");
        source
            .photographic
            .color_editor
            .advanced
            .push(Default::default())
            .unwrap();
        let mut target = source.clone();
        target.photographic.color_editor.advanced.ranges[0]
            .adjustment
            .lightness = 0.3;
        let name = "/photographic/color_editor/advanced/0/name";
        let rename = Edit::Assign {
            pointer: name.into(),
            value: Value::from("Sky"),
        };
        assert!(apply(&mut source, std::slice::from_ref(&rename)));
        assert!(apply(&mut target, std::slice::from_ref(&rename)));
        assert_eq!(
            target.photographic.color_editor.advanced.ranges[0]
                .name
                .as_str(),
            "Sky"
        );
        assert_eq!(
            target.photographic.color_editor.advanced.ranges[0]
                .adjustment
                .lightness,
            0.3
        );
        let before = source.clone();
        source.photographic.color_editor.advanced.ranges[0]
            .name
            .clear();
        let clear = assignments(&before, &source, &[]);
        assert!(clear
            .iter()
            .any(|e| matches!(e,Edit::Remove {pointer} if pointer==name)));
        assert!(apply(&mut target, &clear));
        assert!(target.photographic.color_editor.advanced.ranges[0]
            .name
            .is_empty());
        assert_eq!(
            target.photographic.color_editor.advanced.ranges[0]
                .adjustment
                .lightness,
            0.3
        );
    }
}
