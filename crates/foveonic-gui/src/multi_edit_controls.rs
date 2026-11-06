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
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Mapping {
    #[default]
    Identity,
    Saturation,
}
impl Mapping {
    fn control(self, value: f64) -> f64 {
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
impl Edit {
    pub fn pointer(&self) -> &str {
        match self {
            Self::Number { pointer, .. }
            | Self::Assign { pointer, .. }
            | Self::Optional { pointer, .. }
            | Self::Remove { pointer } => pointer,
        }
    }
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
        if settings.len() > 1 {
            settings
                .iter()
                .filter_map(|s| serde_json::to_value(s).ok())
                .collect::<Vec<_>>()
                .into()
        } else {
            Arc::default()
        }
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
        .is_some_and(|s| s.values.len() > 1)
}
fn property(ctx: &egui::Context) -> Option<Property> {
    ctx.data_mut(|d| d.get_temp::<State>(id()))?.property
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
    (numbers.len() > 1 && numbers.len() == state.values.len()).then_some((p, numbers))
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

/// A mixed slider moves the whole interval. Saturated photos remain at their own bound.
pub fn slider<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut T,
    range: RangeInclusive<T>,
    default: T,
    step: Option<f64>,
    nudge: f64,
) -> Option<egui::Response> {
    let (_, numbers) = numeric_values(ui.ctx())?;
    let fallback = [range.start().to_f64(), range.end().to_f64()];
    let bounds = property(ui.ctx())
        .and_then(|p| {
            ui.ctx()
                .data_mut(|d| d.get_temp::<State>(id()))
                .and_then(|s| s.ranges.get(&p.pointer).copied())
        })
        .unwrap_or(fallback);
    let low = numbers
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min)
        .clamp(bounds[0], bounds[1]);
    let high = numbers
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max)
        .clamp(bounds[0], bounds[1]);
    let (rect, mut response) = ui.allocate_exact_size(
        egui::vec2(ui.spacing().slider_width, ui.spacing().interact_size.y),
        egui::Sense::click_and_drag(),
    );
    let track =
        egui::Rect::from_center_size(rect.center(), egui::vec2((rect.width() - 12.).max(1.), 6.));
    let x =
        |v: f64| track.left() + ((v - bounds[0]) / (bounds[1] - bounds[0])) as f32 * track.width();
    let color = ui.style().interact(&response).fg_stroke.color;
    ui.painter()
        .rect_filled(track, 3., crate::widgets::slider_track_fill(ui.visuals()));
    ui.painter().rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(x(low), track.top()),
            egui::pos2(x(high), track.bottom()),
        ),
        3.,
        ui.visuals().selection.bg_fill,
    );
    for v in [low, high] {
        ui.painter().circle(
            egui::pos2(x(v), track.center().y),
            6.,
            ui.visuals().widgets.inactive.bg_fill,
            egui::Stroke::new(1_f32, color),
        );
    }
    let mut change = None;
    let delta_id = response.id.with("relative travel");
    if response.double_clicked() {
        change = Some(NumberChange::Reset(default.to_f64()));
    } else if response.dragged() {
        let previous = ui.data_mut(|d| d.get_temp::<f32>(delta_id)).unwrap_or(0.);
        let travel = response.drag_delta().x;
        let mut delta = f64::from((travel - previous) / track.width()) * (bounds[1] - bounds[0]);
        if let Some(step) = step.or(T::INTEGRAL.then_some(1.)) {
            delta = (delta / step).round() * step;
        }
        if delta != 0. {
            ui.data_mut(|d| d.insert_temp(delta_id, travel));
            change = Some(NumberChange::Relative(delta));
        }
    } else if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let target = bounds[0]
                + f64::from((pos.x - track.left()) / track.width()).clamp(0., 1.)
                    * (bounds[1] - bounds[0]);
            change = Some(NumberChange::Relative(target - (low + high) * 0.5));
        }
    }
    if response.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
        ui.data_mut(|d| d.remove::<f32>(delta_id));
    }
    if response.has_focus() {
        let direction = ui.input(|i| {
            i.num_presses(egui::Key::ArrowRight) as f64 - i.num_presses(egui::Key::ArrowLeft) as f64
        });
        if direction != 0. {
            change = Some(NumberChange::Relative(direction * nudge));
        }
    }
    if let Some(change) = change {
        let change = match (change, step.or(T::INTEGRAL.then_some(1.))) {
            (NumberChange::Relative(delta), Some(step)) => {
                NumberChange::Relative((delta / step).round() * step)
            }
            (change, _) => change,
        };
        let next = match change {
            NumberChange::Relative(delta) => value.to_f64() + delta,
            NumberChange::Set(v) | NumberChange::Reset(v) => v,
        }
        .clamp(bounds[0], bounds[1]);
        *value = T::from_f64(next);
        number(ui.ctx(), change, bounds);
        response.mark_changed();
    }
    Some(response)
}

#[cfg(test)]
mod tests {
    use super::*;
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
                            slider(ui, &mut value, -4. ..=4., 0., None, 0.1).unwrap();
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
