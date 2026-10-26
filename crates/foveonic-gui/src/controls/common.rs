//! Shares editing rows and photographic controls used by family panels.

use crate::help;
use eframe::egui;
use foveonic::{
    render::{Rendering, RepairThreshold, Settings},
    source::Metadata,
};
use std::ops::RangeInclusive;

// General editing rows share this font-sized column. Compact sections supply
// their own LabelColumn rather than reserving space for unrelated controls.
const COLUMN_LABELS: &[&str] = &[
    "Rendering",
    "White balance",
    "Smooth / Crisp",
    "ColorDQ gain",
];
const ROW_HEIGHT: f32 = 20.;

fn label_width(ui: &egui::Ui) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let width = |text: &str| {
        ui.painter()
            .layout_no_wrap(text.into(), font.clone(), egui::Color32::PLACEHOLDER)
            .size()
            .x
    };
    COLUMN_LABELS
        .iter()
        .map(|s| width(s))
        .chain([width("Highlights") + ui.spacing().icon_width + ui.spacing().icon_spacing])
        .fold(0., f32::max)
        .ceil()
}

/// Reserve the full column while centering the lowercase body on its control.
/// Keep glyphs and their natural spacing intact. Only the label's layout box
/// changes, so native labels/checkboxes retain their interaction and help.
fn aligned_label(
    ui: &mut egui::Ui,
    text: &str,
    indent: Option<f32>,
    enabled: Option<&mut bool>,
    width: Option<f32>,
) -> egui::Response {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let width = width.unwrap_or_else(|| label_width(ui));
    let prefix = if enabled.is_some() {
        ui.spacing().icon_width + ui.spacing().icon_spacing
    } else {
        0.
    };
    let mut job = egui::text::LayoutJob::simple_singleline(
        text.into(),
        font.clone(),
        egui::Color32::PLACEHOLDER,
    );
    if indent.is_some() {
        job.sections[0].leading_space = ui.spacing().indent;
    }
    let mut galley = ui.painter().layout_job(job);
    let reference = ui
        .painter()
        .layout_no_wrap("x".into(), font, egui::Color32::PLACEHOLDER);
    let body_offset = reference
        .rows
        .first()
        .and_then(|row| row.glyphs.first())
        .map_or(0., |glyph| {
            glyph.uv_rect.offset.y + glyph.uv_rect.size.y * 0.5
        });
    let baseline = galley
        .rows
        .first()
        .and_then(|row| row.glyphs.first())
        .map_or(galley.size().y * 0.5, |g| g.pos.y);
    let ink_left = galley
        .rows
        .first()
        .map_or(0., |row| row.visuals.mesh_bounds.left());
    let column = std::sync::Arc::make_mut(&mut galley);
    // Native widgets center this box. Putting its midpoint at baseline minus
    // half the x-height aligns the visible letter body, independent of g/p/h.
    column.rect.max.x = column.rect.min.x + (width - prefix).max(column.size().x);
    column.rect.max.y = column.rect.min.y + 2. * (baseline + body_offset);
    let response = if let Some(value) = enabled {
        ui.add_sized([width, ROW_HEIGHT], |ui: &mut egui::Ui| {
            crate::widgets::checkbox(ui, value, galley)
        })
    } else {
        ui.add_sized([width, ROW_HEIGHT], egui::Label::new(galley))
    };
    if let Some(outer_padding) = indent {
        // Center on the actual visible glyph edge, including the group's
        // padding to the left of this row. Font side bearings count too.
        let x = response.rect.left() + (ink_left - outer_padding) * 0.5;
        ui.painter().line_segment(
            [
                egui::pos2(x, response.rect.top()),
                egui::pos2(x, response.rect.bottom()),
            ],
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
    }
    response
}

pub(super) fn label(ui: &mut egui::Ui, text: &str) -> egui::Response {
    aligned_label(ui, text, None, None, None)
}

pub(super) fn label_toggle(ui: &mut egui::Ui, text: &str, enabled: &mut bool) -> egui::Response {
    aligned_label(ui, text, None, Some(enabled), None)
}

/// A section's labels share only the space needed by that section.
#[derive(Clone, Copy)]
pub(super) struct LabelColumn {
    width: f32,
    indent: bool,
    outer_padding: f32,
}
impl LabelColumn {
    pub(super) fn new(ui: &egui::Ui, labels: &[&str], children: &[&str]) -> Self {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let width = |text: &str| {
            ui.painter()
                .layout_no_wrap(text.into(), font.clone(), egui::Color32::PLACEHOLDER)
                .size()
                .x
        };
        Self {
            width: labels
                .iter()
                .map(|s| width(s))
                .chain(children.iter().map(|s| width(s) + ui.spacing().indent))
                .fold(0., f32::max)
                .ceil(),
            indent: false,
            outer_padding: 0.,
        }
    }

    pub(super) fn with_outer_padding(self, outer_padding: f32) -> Self {
        Self {
            outer_padding,
            ..self
        }
    }

    pub(super) fn indented(self) -> Self {
        Self {
            indent: true,
            ..self
        }
    }

    pub(super) fn row_controls(self, label: &str) -> RowControls<'_> {
        RowControls {
            indent: self.indent.then_some(self.outer_padding),
            label: Some((label, self.width)),
            ..Default::default()
        }
    }

    pub(super) fn adjustment<T: egui::emath::Numeric>(
        self,
        ui: &mut egui::Ui,
        label: (&str, &str),
        value: &mut T,
        range: RangeInclusive<T>,
        default: T,
    ) -> bool {
        adjustment_row(
            ui,
            label.0,
            value,
            range,
            default,
            if T::INTEGRAL { 1. } else { 0.1 },
            self.row_controls(label.1),
        )
    }
}

pub fn adjustment<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    range: RangeInclusive<T>,
    default: T,
) -> bool {
    adjustment_with_nudge(
        ui,
        label,
        value,
        range,
        default,
        if T::INTEGRAL { 1. } else { 0.1 },
    )
}

/// Set the focused value box's arrow-key step independently of mouse-drag sensitivity.
pub fn adjustment_with_nudge<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    range: RangeInclusive<T>,
    default: T,
    nudge: f64,
) -> bool {
    adjustment_row(
        ui,
        label,
        value,
        range,
        default,
        nudge,
        RowControls::default(),
    )
}

/// Snap the slider and value box to the same discrete parameter positions.
pub fn stepped_adjustment(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: RangeInclusive<f32>,
    default: f32,
    step: f64,
) -> bool {
    adjustment_row(
        ui,
        label,
        value,
        range,
        default,
        step,
        RowControls {
            step: Some(step),
            ..Default::default()
        },
    )
}

pub(super) struct Automatic<'a> {
    active: &'a mut bool,
    resolved: bool,
}

#[derive(Default)]
pub(super) struct RowControls<'a> {
    pub(super) automatic: Option<Automatic<'a>>,
    pub(super) enabled: Option<&'a mut bool>,
    pub(super) step: Option<f64>,
    /// Some includes the containing group's left padding when centering the pipe.
    pub(super) indent: Option<f32>,
    pub(super) label: Option<(&'a str, f32)>,
}

/// Keep automatic selection on the value row rather than in a separate checkbox.
pub fn automatic_adjustment<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    manual: &mut Option<T>,
    range: RangeInclusive<T>,
    default: T,
    calculated: Option<T>,
    controls: RowControls<'_>,
) -> bool {
    let before = manual.map(|v| v.to_f64());
    let mut automatic = manual.is_none();
    let mut value = manual.or(calculated).unwrap_or(default);
    let changed = adjustment_row(
        ui,
        label,
        &mut value,
        range,
        default,
        if T::INTEGRAL { 1. } else { 0.1 },
        RowControls {
            automatic: Some(Automatic {
                active: &mut automatic,
                resolved: calculated.is_some(),
            }),
            ..controls
        },
    );
    *manual = (!automatic).then_some(value);
    changed || before != manual.map(|v| v.to_f64())
}

pub(super) fn adjustment_row<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    range: RangeInclusive<T>,
    default: T,
    nudge: f64,
    controls: RowControls<'_>,
) -> bool {
    let RowControls {
        mut automatic,
        enabled,
        step,
        indent,
        label: label_override,
    } = controls;
    // The two Highlights rows need separate help and widget IDs.
    let display_label = label_override.map_or_else(
        || match label {
            "Highlight correction" | "Color-field highlights" => "Threshold",
            _ => label,
        },
        |(text, _)| text,
    );
    let label_width = label_override.map(|(_, width)| width);
    let before = value.to_f64();
    let mut toggled = false;
    let decimals = if T::INTEGRAL {
        0
    } else {
        (-nudge.abs().log10()).ceil().clamp(2., 15.) as usize
    };
    ui.push_id(label, |ui| {
        ui.horizontal(|ui| {
            let enabled = if let Some(enabled) = enabled {
                toggled = help::control(
                    aligned_label(ui, display_label, indent, Some(&mut *enabled), label_width),
                    label,
                )
                .changed();
                *enabled
            } else {
                help::control(
                    aligned_label(ui, display_label, indent, None, label_width),
                    label,
                );
                true
            };
            ui.add_enabled_ui(enabled, |ui| {
                let auto_width = if automatic.is_some() {
                    22. + ui.spacing().item_spacing.x
                } else {
                    0.
                };
                // Fit the widest normal signed value, rather than reserving an
                // extra em in every numeric field. Return that space to the track.
                let value_width = (ui
                    .painter()
                    .layout_no_wrap(
                        "-2.00".into(),
                        egui::TextStyle::Button.resolve(ui.style()),
                        egui::Color32::PLACEHOLDER,
                    )
                    .size()
                    .x
                    + 2. * ui.spacing().button_padding.x)
                    .max(ui.spacing().interact_size.x);
                ui.spacing_mut().slider_width =
                    (ui.available_width() - value_width - ui.spacing().item_spacing.x - auto_width)
                        .max(35.);
                ui.add_enabled_ui(!automatic.as_ref().is_some_and(|a| *a.active), |ui| {
                    // Apply to every editing slider while leaving adjacent
                    // value fields, buttons and group backgrounds unchanged.
                    ui.visuals_mut().widgets.inactive.bg_fill = crate::widgets::SLIDER_TRACK_FILL;
                    let mut slider = egui::Slider::new(value, range.clone()).show_value(false);
                    if let Some(step) = step {
                        slider = slider.step_by(step);
                    }
                    let response = ui.add(slider).interact(egui::Sense::click_and_drag());
                    if response.double_clicked() {
                        *value = default;
                    }
                    help::control(response, label);
                });
                if let Some(auto) = &mut automatic {
                    let response =
                        crate::widgets::button(ui, "A", *auto.active, egui::vec2(22., 20.));
                    if help::attach(response, help::automatic(label).unwrap()).clicked() {
                        *auto.active = !*auto.active;
                    }
                }
                let active = automatic.as_ref().is_some_and(|a| *a.active);
                let unresolved = automatic.as_ref().is_some_and(|a| *a.active && !a.resolved);
                // DragValue handles keys while drawing. Find its stable ID first so our
                // focused nudge can run without changing the row's layout.
                let response = ui
                    .add_enabled_ui(!active, |ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(value_width, ROW_HEIGHT),
                            egui::Layout::centered_and_justified(ui.layout().main_dir()),
                            |ui| {
                                if unresolved {
                                    return ui.add(egui::Label::new("Auto"));
                                }
                                let focused =
                                    ui.memory(|memory| memory.has_focus(ui.next_auto_id()));
                                ui.add(
                                    egui::DragValue::new(value)
                                        .range(range.clone())
                                        .speed(if focused {
                                            nudge
                                        } else if T::INTEGRAL {
                                            1.
                                        } else {
                                            0.01
                                        })
                                        .fixed_decimals(decimals),
                                )
                            },
                        )
                        .inner
                    })
                    .inner;
                // Refresh the cached text after clamping so the value box does not show
                // the overshoot even though the stored setting is already in range.
                let mut accepted = value.to_f64();
                if let Some(step) = step {
                    accepted = (accepted / step).round() * step;
                }
                let accepted = accepted.clamp(range.start().to_f64(), range.end().to_f64());
                let accepted = T::from_f64(accepted);
                if !active && value.to_f64() != accepted.to_f64() {
                    *value = accepted;
                    ui.data_mut(|data| data.remove::<String>(response.id));
                    ui.ctx().request_repaint();
                }
                help::control(response, label);
            });
        });
    });
    // The double-click response can hide the slider's changed flag.
    // Compare values so a real edit still requests a render.
    toggled || before != value.to_f64()
}

pub(super) fn rendering(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        help::control(label(ui, "Rendering"), "Rendering");
        for (value, label) in [
            (Rendering::Color, "Color"),
            (Rendering::Monochrome, "Monochrome"),
        ] {
            if help::control(
                crate::widgets::button(
                    ui,
                    label,
                    settings.rendering == value,
                    egui::vec2(if value == Rendering::Color { 55. } else { 100. }, 22.),
                ),
                "Rendering",
            )
            .clicked()
            {
                changed |= settings.rendering != value;
                settings.rendering = value;
            }
        }
    });
    changed
}

/// Families choose which preset controls apply without changing stored selections.
#[derive(Clone, Copy)]
pub(super) struct PresetControls {
    pub white_balance: bool,
    pub color_mode: bool,
}

pub(super) fn presets(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
    controls: PresetControls,
) -> bool {
    let mut changed = false;
    for (title, key, value) in [
        (
            "White balance",
            "WhiteBalanceGains",
            &mut settings.white_balance,
        ),
        (
            "Color mode",
            "ColorModeCompensations",
            &mut settings.color_mode,
        ),
    ] {
        if !controls.color_mode && title == "Color mode" {
            continue;
        }
        let choices = metadata.map(|metadata| {
            if key == "WhiteBalanceGains" {
                metadata.white_balance_presets()
            } else {
                foveonic::render::ControlCapabilities::from_metadata(metadata).map_or_else(
                    |_| metadata.color_mode_presets(),
                    |c| c.color_mode_presets(metadata),
                )
            }
        });
        // Keep the same rows while metadata is loading so later controls do not
        // jump when the first processed frame supplies the camera's choices.
        ui.horizontal(|ui| {
            help::control(label(ui, title), title);
            let response = ui
                .add_enabled_ui(
                    (title != "White balance" || controls.white_balance)
                        && choices.as_ref().is_some_and(|list| !list.is_empty()),
                    |ui| {
                        let combo = egui::ComboBox::from_id_salt(key)
                            .width(ui.available_width() - 8.)
                            .selected_text(if value.is_empty() {
                                "Choose…"
                            } else {
                                value.as_str()
                            });
                        crate::widgets::dropdown(ui, combo, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.;
                            if let Some(list) = &choices {
                                for name in list.iter().copied() {
                                    changed |= crate::widgets::menu_value(
                                        ui,
                                        value,
                                        name.to_owned(),
                                        name,
                                    )
                                    .changed();
                                }
                            }
                        })
                        .response
                    },
                )
                .inner;
            help::control(response, title);
        });
    }
    changed
}

pub(super) fn tone(ui: &mut egui::Ui, settings: &mut Settings, mono: bool) -> bool {
    tone_with_exposure_range(ui, settings, mono, -4. ..=4.)
}

pub(super) fn tone_with_exposure_range(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    mono: bool,
    exposure_range: RangeInclusive<f32>,
) -> bool {
    let mut changed = false;
    let title = if mono { "Tone" } else { "Tone and Color" };
    changed |= help::control(
        crate::widgets::checkbox(
            ui,
            &mut settings.tone_enabled,
            egui::RichText::new(title).weak(),
        ),
        title,
    )
    .changed();
    ui.add_enabled_ui(settings.tone_enabled, |ui| {
        let column = LabelColumn::new(
            ui,
            &[
                "Exposure",
                "Saturation",
                "Contrast",
                "Shadows",
                "Highlights",
            ],
            &[],
        );
        for (label, value, range, default) in [
            ("Exposure", &mut settings.exposure, exposure_range, 0.),
            ("Saturation", &mut settings.saturation, 0. ..=2., 1.),
        ] {
            if !mono || label != "Saturation" {
                changed |= column.adjustment(ui, (label, label), value, range, default);
            }
        }
        for (label, value) in [
            ("Contrast", &mut settings.tone.contrast),
            ("Shadows", &mut settings.tone.shadows),
            ("Highlights", &mut settings.tone.highlights),
        ] {
            changed |= column.adjustment(ui, (label, label), value, -2. ..=2., 0.);
        }
    });
    changed
}

pub(super) fn refinement(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
) -> bool {
    let mut changed = false;
    changed |= help::control(
        crate::widgets::checkbox(
            ui,
            &mut settings.refinement_enabled,
            egui::RichText::new("Refinement").weak(),
        ),
        "Refinement",
    )
    .changed();
    let calculated = metadata.and_then(|metadata| {
        let mut automatic = settings.clone();
        automatic.sharpen_amount = None;
        automatic.refinement_enabled = true;
        automatic.effective_sharpen_amount(metadata).ok()
    });
    ui.add_enabled_ui(settings.refinement_enabled, |ui| {
        let column = LabelColumn::new(ui, &["Sharpen"], &[]);
        changed |= automatic_adjustment(
            ui,
            "Sharpen",
            &mut settings.sharpen_amount,
            0. ..=2_f32.max(calculated.unwrap_or(0.)),
            0.,
            calculated,
            column.row_controls("Sharpen"),
        );
    });
    changed
}

pub(super) fn pixel_corrections(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mut changed = false;
    changed |= help::control(
        crate::widgets::checkbox(ui, &mut settings.despeckle, "Speckles"),
        "Speckles",
    )
    .changed();
    let mut repair = settings.repair.is_some();
    if help::control(
        crate::widgets::checkbox(ui, &mut repair, "Bad pixels"),
        "Bad pixels",
    )
    .changed()
    {
        settings.repair = repair.then_some(RepairThreshold::Camera);
        changed = true;
    }
    changed
}

pub(super) fn reconstruction(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mut changed = false;
    let mut reconstruction = settings.reconstruction.is_some();
    if help::control(
        crate::widgets::checkbox(ui, &mut reconstruction, "Noise and Detail"),
        "Noise and Detail",
    )
    .changed()
    {
        settings.reconstruction = reconstruction.then(Default::default);
        changed = true;
    }
    changed
}
