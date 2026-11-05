//! Shares editing rows and photographic controls used by family panels.

use crate::{help, preferences::EditingSection};
use eframe::egui;
use foveonic::{
    render::{Rendering, RepairThreshold, Settings},
    source::Metadata,
};
use std::ops::RangeInclusive;

const ROW_HEIGHT: f32 = 20.;

/// Keep the section title and optional bypass on one stable row.
pub(super) fn section_header(
    ui: &mut egui::Ui,
    section: EditingSection,
    title: &str,
    enabled: Option<&mut bool>,
) -> bool {
    section_header_with(ui, section, title, enabled, |_, _| {})
}

pub(super) fn section_header_with(
    ui: &mut egui::Ui,
    section: EditingSection,
    title: &str,
    enabled: Option<&mut bool>,
    contents: impl FnOnce(&mut egui::Ui, bool),
) -> bool {
    let spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = crate::widgets::section_spacing(ui);
    let changed = ui
        .horizontal(|ui| {
            ui.set_min_height(crate::widgets::ControlSize::Medium.group_height());
            let text = crate::widgets::section_title(ui, title);
            let (response, enabled) = if let Some(enabled) = enabled {
                let response = crate::widgets::checkbox_sized(
                    ui,
                    enabled,
                    text,
                    crate::widgets::ControlSize::Medium,
                );
                (response, *enabled)
            } else {
                (ui.label(text), true)
            };
            let changed = help::control(response, title).changed();
            contents(ui, enabled);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                crate::sections::editing_handle(ui, section);
            });
            changed
        })
        .inner;
    ui.spacing_mut().item_spacing.y = spacing;
    changed
}

/// Reserve the full column while centering the lowercase body on its control.
/// Keep glyphs and their natural spacing intact. Only the label's layout box
/// changes, so native labels/checkboxes retain their interaction and help.
fn aligned_label(
    ui: &mut egui::Ui,
    text: &str,
    enabled: Option<&mut bool>,
    width: Option<f32>,
) -> egui::Response {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let width = width.unwrap_or_else(|| {
        crate::widgets::form_label_width(ui, &[text], enabled.as_ref().map(|_| text))
    });
    let prefix = if enabled.is_some() {
        crate::widgets::ControlSize::Small.side() + ui.spacing().icon_spacing
    } else {
        0.
    };
    let job = egui::text::LayoutJob::simple_singleline(
        text.into(),
        font.clone(),
        egui::Color32::PLACEHOLDER,
    );
    let mut galley = ui.painter().layout_job(job);
    let width = width.max(galley.size().x + prefix);
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
    let column = std::sync::Arc::make_mut(&mut galley);
    // Native widgets center this box. Putting its midpoint at baseline minus
    // half the x-height aligns the visible letter body, independent of g/p/h.
    column.rect.max.x = column.rect.min.x + (width - prefix).max(column.size().x);
    column.rect.max.y = column.rect.min.y + 2. * (baseline + body_offset);
    if let Some(value) = enabled {
        ui.add_sized([width, ROW_HEIGHT], |ui: &mut egui::Ui| {
            crate::widgets::checkbox(ui, value, galley)
        })
    } else {
        ui.add_sized([width, ROW_HEIGHT], egui::Label::new(galley))
    }
}

pub(super) fn label(ui: &mut egui::Ui, text: &str) -> egui::Response {
    aligned_label(ui, text, None, None)
}

/// A section's labels share only the space needed by that section.
#[derive(Clone, Copy)]
pub(super) struct LabelColumn {
    width: f32,
    origin: f32,
}
impl LabelColumn {
    pub(super) fn new(ui: &egui::Ui) -> Self {
        Self {
            width: crate::widgets::form_label_width(ui, &[], None),
            origin: ui.next_widget_position().x,
        }
    }

    // Disclosure labels stay indented while their controls retain the parent's grid.
    fn width_at(self, ui: &egui::Ui) -> f32 {
        (self.width + self.origin - ui.next_widget_position().x).max(0.)
    }

    pub(super) fn label(self, ui: &mut egui::Ui, label: &str) -> egui::Response {
        aligned_label(ui, label, None, Some(self.width_at(ui)))
    }

    pub(super) fn toggle(
        self,
        ui: &mut egui::Ui,
        label: &str,
        enabled: &mut bool,
    ) -> egui::Response {
        aligned_label(ui, label, Some(enabled), Some(self.width_at(ui)))
    }

    pub(super) fn row_controls(self, label: &str) -> RowControls<'_> {
        RowControls {
            label: Some((label, self)),
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

    pub(super) fn saturation_adjustment(
        self,
        ui: &mut egui::Ui,
        label: (&str, &str),
        amount: &mut f32,
    ) -> bool {
        use foveonic::render::PerceptualControls;
        let mut control = PerceptualControls::saturation_to_control(*amount);
        let changed = self.adjustment(
            ui,
            label,
            &mut control,
            PerceptualControls::SATURATION_CONTROL_RANGE,
            0.,
        );
        // Drawing and formatting must never round or rewrite an existing recipe.
        if changed {
            *amount = PerceptualControls::saturation_from_control(control);
        }
        changed
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
    calculated: Option<f64>,
    editable: bool,
}

#[derive(Default)]
pub(super) struct RowControls<'a> {
    pub(super) automatic: Option<Automatic<'a>>,
    pub(super) enabled: Option<&'a mut bool>,
    pub(super) step: Option<f64>,
    /// Grid origin for stepped values. Zero preserves the ordinary slider grid.
    pub(super) step_origin: f64,
    pub(super) label: Option<(&'a str, LabelColumn)>,
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
                calculated: calculated.map(|v| v.to_f64()),
                editable: false,
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
        step_origin,
        label: label_override,
    } = controls;
    // The two Highlights rows need separate help and widget IDs.
    let display_label = label_override.map_or_else(
        || match label {
            "Highlight correction" | "Color-field highlights" | "Speckle threshold" => "Threshold",
            "Speckle size" => "Size",
            _ => label,
        },
        |(text, _)| text,
    );
    let label_width = label_override.map(|(_, column)| column.width_at(ui));
    let before = value.to_f64();
    let mut toggled = false;
    let decimals = if T::INTEGRAL {
        0
    } else {
        (-nudge.abs().log10()).ceil().clamp(2., 15.) as usize
    };
    let format_value = |value| crate::widgets::format_numeric_value(value, T::INTEGRAL);
    ui.push_id(label, |ui| {
        ui.horizontal(|ui| {
            let enabled = if let Some(enabled) = enabled {
                toggled = help::control(
                    aligned_label(ui, display_label, Some(&mut *enabled), label_width),
                    label,
                )
                .changed();
                *enabled
            } else {
                help::control(aligned_label(ui, display_label, None, label_width), label);
                true
            };
            ui.add_enabled_ui(enabled, |ui| {
                let auto_width = if automatic.is_some() {
                    crate::widgets::ControlSize::Large.side() + ui.spacing().item_spacing.x
                } else {
                    0.
                };
                // Measure the same formatted values and font as DragValue.
                // Reserve any extra space from the slider, so a numeric readout
                // never expands the enclosing resizable panel.
                let value_font = ui.style().drag_value_text_style.resolve(ui.style());
                let text_width = [range.start().to_f64(), range.end().to_f64(), value.to_f64()]
                    .into_iter()
                    .map(|v| {
                        let text = format_value(v);
                        ui.painter()
                            .layout_no_wrap(text, value_font.clone(), egui::Color32::PLACEHOLDER)
                            .size()
                            .x
                    })
                    .fold(0., f32::max);
                // Integer and decimal rows share a field wide enough for a signed
                // neutral value. Wider formatted values can still remain readable.
                let standard_width = ui
                    .painter()
                    .layout_no_wrap("-0.00".into(), value_font, egui::Color32::PLACEHOLDER)
                    .size()
                    .x;
                let value_width = (text_width.max(standard_width)
                    + 2. * ui.spacing().button_padding.x)
                    .max(ui.spacing().interact_size.x);
                ui.spacing_mut().slider_width =
                    (ui.available_width() - value_width - ui.spacing().item_spacing.x - auto_width)
                        .max(35.);
                ui.add_enabled_ui(
                    !automatic.as_ref().is_some_and(|a| *a.active && !a.editable),
                    |ui| {
                        // Apply to every editing slider while leaving adjacent
                        // value fields, buttons and group backgrounds unchanged.
                        ui.visuals_mut().widgets.inactive.bg_fill =
                            crate::widgets::slider_track_fill(ui.visuals());
                        let before_slider = value.to_f64();
                        let mut slider =
                            crate::widgets::slider(value, range.clone()).show_value(false);
                        // Shifted grids snap below so odd endpoints remain reachable.
                        if let Some(step) = step.filter(|_| step_origin == 0.) {
                            slider = slider.step_by(step);
                        }
                        let response = ui.add(slider).interact(egui::Sense::click_and_drag());
                        if response.double_clicked() {
                            *value = default;
                        }
                        if response.changed()
                            || response.double_clicked()
                            || before_slider != value.to_f64()
                        {
                            if let Some(auto) = automatic.as_mut().filter(|a| a.editable) {
                                *auto.active = false;
                            }
                        }
                        help::control(response, label);
                    },
                );
                if let Some(auto) = &mut automatic {
                    let response = crate::widgets::auto_button(ui, *auto.active);
                    if help::attach(response, help::automatic(label).unwrap()).clicked() {
                        if auto.editable {
                            if *auto.active {
                                *auto.active = false;
                                *value = default;
                            } else if let Some(calculated) = auto.calculated {
                                *auto.active = true;
                                *value = T::from_f64(calculated);
                            }
                        } else {
                            *auto.active = !*auto.active;
                        }
                    }
                }
                let locked = automatic.as_ref().is_some_and(|a| *a.active && !a.editable);
                let unresolved =
                    locked && automatic.as_ref().is_some_and(|a| a.calculated.is_none());
                // DragValue handles keys while drawing. Find its stable ID first so our
                // focused nudge can run without changing the row's layout.
                let response = ui
                    .add_enabled_ui(!locked, |ui| {
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
                                    crate::widgets::drag_value(value)
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
                if response.changed() {
                    if let Some(auto) = automatic.as_mut().filter(|a| a.editable) {
                        *auto.active = false;
                    }
                }
                // Refresh the cached text after clamping so the value box does not show
                // the overshoot even though the stored setting is already in range.
                let mut accepted = value.to_f64();
                if let Some(step) = step {
                    accepted = ((accepted - step_origin) / step).round() * step + step_origin;
                }
                let accepted = accepted.clamp(range.start().to_f64(), range.end().to_f64());
                let accepted = T::from_f64(accepted);
                if !locked && value.to_f64() != accepted.to_f64() {
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

pub(super) fn color_mode(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
    active: bool,
) -> bool {
    let column = LabelColumn::new(ui);
    active
        && camera_choice(
            ui,
            column,
            metadata,
            "ColorModeCompensations",
            ("Color mode", "Color mode", false),
            &mut settings.color_mode,
            true,
        )
}

pub(super) fn white_balance(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
    enabled: bool,
) -> bool {
    section_header(ui, EditingSection::WhiteBalance, "White Balance", None);
    let column = LabelColumn::new(ui);
    let mut choice = if settings.white_balance_bias.neutral.is_some() {
        "Custom".to_owned()
    } else {
        settings.white_balance.clone()
    };
    let mut changed = camera_choice(
        ui,
        column,
        metadata,
        "WhiteBalanceGains",
        ("Mode", "White balance", !settings.is_monochrome()),
        &mut choice,
        enabled,
    );
    if changed {
        settings.white_balance = choice;
        settings.white_balance_bias = Default::default();
        crate::editor::white_balance::exit(ui.ctx());
    }
    ui.add_enabled_ui(!settings.is_monochrome(), |ui| {
        changed |= column.adjustment(
            ui,
            ("Warmth", "Warmth"),
            &mut settings.white_balance_bias.warmth,
            -1. ..=1.,
            0.,
        );
        changed |= column.adjustment(
            ui,
            ("Tint", "Tint"),
            &mut settings.white_balance_bias.tint,
            -1. ..=1.,
            0.,
        );
    });
    changed
}

fn camera_choice(
    ui: &mut egui::Ui,
    column: LabelColumn,
    metadata: Option<&Metadata>,
    key: &str,
    (label, help_key, picker_enabled): (&str, &str, bool),
    value: &mut String,
    enabled: bool,
) -> bool {
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
    let mut changed = false;
    ui.horizontal(|ui| {
        let white_balance = key == "WhiteBalanceGains";
        if white_balance {
            ui.spacing_mut().item_spacing.x = crate::widgets::MEDIUM_CONTROL_GAP;
        }
        help::control(
            aligned_label(ui, label, None, Some(column.width_at(ui))),
            help_key,
        );
        let response = ui
            .add_enabled_ui(
                enabled && choices.as_ref().is_some_and(|list| !list.is_empty()),
                |ui| {
                    let combo = egui::ComboBox::from_id_salt(key)
                        .width(
                            (ui.available_width()
                                - if white_balance {
                                    crate::widgets::control_height(ui) + ui.spacing().item_spacing.x
                                } else {
                                    0.
                                })
                            .max(1.),
                        )
                        .truncate()
                        .selected_text(if value.is_empty() {
                            "As Shot"
                        } else {
                            value.as_str()
                        });
                    crate::widgets::dropdown(ui, combo, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.;
                        changed |= crate::widgets::menu_value(ui, value, String::new(), "As Shot")
                            .changed();
                        if let Some(list) = &choices {
                            for name in list.iter().copied() {
                                changed |=
                                    crate::widgets::menu_value(ui, value, name.to_owned(), name)
                                        .changed();
                            }
                        }
                    })
                    .response
                },
            )
            .inner;
        let height = response.rect.height();
        help::control(response, help_key);
        if white_balance {
            ui.add_enabled_ui(enabled && metadata.is_some() && picker_enabled, |ui| {
                crate::editor::white_balance::button(ui, height)
            });
        }
    });
    changed
}

pub(super) fn tone(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    compensation: Option<foveonic::source::CaptureExposureCompensation>,
) -> bool {
    tone_with_exposure_range(ui, settings, -4. ..=4., compensation)
}

pub(super) fn tone_with_exposure_range(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    exposure_range: RangeInclusive<f32>,
    compensation: Option<foveonic::source::CaptureExposureCompensation>,
) -> bool {
    tone_rows(ui, settings, exposure_range, compensation)
}

fn exposure_row(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    column: LabelColumn,
    range: RangeInclusive<f32>,
    compensation: Option<foveonic::source::CaptureExposureCompensation>,
) -> bool {
    let calculated = compensation.and_then(|c| settings.exposure_auto_compensation.inverse(c));
    let was_auto = settings.exposure_auto;
    let mut value = if was_auto {
        calculated.unwrap_or(0.)
    } else {
        settings.exposure
    };
    let range = range.start().min(calculated.unwrap_or(*range.start()))
        ..=range.end().max(calculated.unwrap_or(*range.end()));
    let changed = adjustment_row(
        ui,
        "Exposure",
        &mut value,
        range,
        0.,
        0.1,
        RowControls {
            automatic: Some(Automatic {
                active: &mut settings.exposure_auto,
                calculated: calculated.map(f64::from),
                editable: true,
            }),
            ..column.row_controls("Exposure")
        },
    );
    if !settings.exposure_auto {
        settings.exposure = value;
    }
    changed || was_auto != settings.exposure_auto
}

fn tone_rows(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    exposure_range: RangeInclusive<f32>,
    compensation: Option<foveonic::source::CaptureExposureCompensation>,
) -> bool {
    let mut changed = false;
    changed |= section_header(
        ui,
        EditingSection::Tone,
        "Tone",
        Some(&mut settings.tone_enabled),
    );
    ui.add_enabled_ui(settings.tone_enabled, |ui| {
        let column = LabelColumn::new(ui);
        changed |= column.method_choice(
            ui,
            "Tone method",
            "Method",
            &mut settings.photographic.tone_method,
        );
        let universal =
            settings.photographic.tone_method == foveonic::render::PhotographicMethod::Universal;
        if universal {
            use foveonic::render::BaseResponse;
            ui.horizontal(|ui| {
                help::control(
                    aligned_label(ui, "Adaptive", None, Some(column.width_at(ui))),
                    "Adaptive",
                );
                let selected = &mut settings.photographic.base_response;
                let combo = egui::ComboBox::from_id_salt("base-response")
                    .width(ui.available_width())
                    .truncate()
                    .selected_text(selected.label());
                let response = crate::widgets::dropdown(ui, combo, |ui| {
                    for (choice, name) in [
                        (BaseResponse::Camera, "Disabled"),
                        (BaseResponse::Natural, "Natural"),
                        (BaseResponse::Strong, "Strong"),
                    ] {
                        changed |= crate::widgets::menu_value(ui, selected, choice, name).changed();
                    }
                })
                .response;
                help::control(response, "Adaptive");
            });
        }
        changed |= exposure_row(ui, settings, column, exposure_range, compensation);
        if universal {
            ui.weak("Dynamic range");
            for (label, value) in [
                ("Shadows", &mut settings.tonal_ranges.shadows),
                ("Highlights", &mut settings.tonal_ranges.highlights),
                ("Blacks", &mut settings.tonal_ranges.blacks),
                ("Whites", &mut settings.tonal_ranges.whites),
            ] {
                changed |= ui
                    .push_id(("tonal-ranges", label), |ui| {
                        column.adjustment(
                            ui,
                            ("Tonal ranges", label),
                            value,
                            -foveonic::render::TonalRanges::MAX_AMOUNT
                                ..=foveonic::render::TonalRanges::MAX_AMOUNT,
                            0.,
                        )
                    })
                    .inner;
            }
            ui.weak("Tone shaping");
            changed |= column.adjustment(
                ui,
                ("Brightness", "Brightness"),
                &mut settings.photographic.brightness,
                -1. ..=1.,
                0.,
            );
        }
        changed |= column.adjustment(
            ui,
            ("Contrast", "Contrast"),
            &mut settings.tone.contrast,
            -2. ..=2.,
            0.,
        );
        if universal {
            changed |= column.adjustment(
                ui,
                ("Local contrast", "Clarity"),
                &mut settings.photographic.local_contrast,
                -1. ..=1.,
                0.,
            );
        }
        if !universal {
            for (label, value) in [
                ("Shadows", &mut settings.tone.shadows),
                ("Highlights", &mut settings.tone.highlights),
            ] {
                changed |= column.adjustment(ui, (label, label), value, -2. ..=2., 0.);
            }
        }
    });
    changed
}

pub(super) fn sharpening(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
) -> bool {
    sharpening_with_range(ui, settings, metadata, 0. ..=2.)
}

pub(super) fn sharpening_with_range(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
    reference_range: RangeInclusive<f32>,
) -> bool {
    let mut changed = false;
    changed |= section_header(
        ui,
        EditingSection::Sharpening,
        "Sharpening",
        Some(&mut settings.sharpening_enabled),
    );
    let calculated = metadata.and_then(|metadata| {
        let mut automatic = settings.clone();
        automatic.sharpen_amount = None;
        automatic.sharpening_enabled = true;
        automatic.effective_sharpen_amount(metadata).ok()
    });
    ui.add_enabled_ui(settings.sharpening_enabled, |ui| {
        changed |= method_choice(
            ui,
            "Sharpening method",
            "Method",
            &mut settings.photographic.sharpening.method,
        );
        if settings.photographic.sharpening.method
            == foveonic::render::PhotographicMethod::Universal
        {
            let sharp = &mut settings.photographic.sharpening;
            let column = LabelColumn::new(ui);
            changed |= column.adjustment(
                ui,
                ("Universal amount", "Amount"),
                &mut sharp.amount,
                0. ..=4.,
                0.,
            );
            changed |= column.adjustment(
                ui,
                ("Universal radius", "Radius"),
                &mut sharp.radius,
                0.3..=4.,
                0.8,
            );
            crate::widgets::disclosure(
                ui,
                egui::CollapsingHeader::new("Advanced").id_salt("universal-sharpening"),
                |ui| {
                    let column = LabelColumn::new(ui);
                    changed |= adjustment_row(
                        ui,
                        "Sharpen threshold",
                        &mut sharp.threshold,
                        0. ..=0.2,
                        0.01,
                        0.001,
                        column.row_controls("Threshold"),
                    );
                    changed |= column.adjustment(
                        ui,
                        ("Halo suppression", "Halos"),
                        &mut sharp.halo_suppression,
                        0. ..=1.,
                        0.5,
                    );
                },
            );
            return;
        }
        let column = LabelColumn::new(ui);
        changed |= automatic_adjustment(
            ui,
            "Sharpen",
            &mut settings.sharpen_amount,
            *reference_range.start()..=reference_range.end().max(calculated.unwrap_or(0.)),
            0.,
            calculated,
            column.row_controls("Amount"),
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
    changed | bad_pixels(ui, settings)
}

pub(super) fn bad_pixels(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mut changed = false;
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
    if section_header(
        ui,
        EditingSection::Noise,
        "Noise and Detail",
        Some(&mut reconstruction),
    ) {
        settings.reconstruction = reconstruction.then(Default::default);
        changed = true;
    }
    changed
}

/// Shared method selection keeps family interpretation out of editing widgets.
pub(super) fn method_choice(
    ui: &mut egui::Ui,
    key: &str,
    label: &str,
    value: &mut foveonic::render::PhotographicMethod,
) -> bool {
    LabelColumn::new(ui).method_choice(ui, key, label, value)
}

impl LabelColumn {
    pub(super) fn method_choice(
        self,
        ui: &mut egui::Ui,
        key: &str,
        label: &str,
        value: &mut foveonic::render::PhotographicMethod,
    ) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            help::control(aligned_label(ui, label, None, Some(self.width_at(ui))), key);
            let combo = egui::ComboBox::from_id_salt(key)
                .width(ui.available_width())
                .truncate()
                .selected_text(value.label());
            let response = crate::widgets::dropdown(ui, combo, |ui| {
                for method in [
                    foveonic::render::PhotographicMethod::Reference,
                    foveonic::render::PhotographicMethod::Universal,
                ] {
                    changed |=
                        crate::widgets::menu_value(ui, value, method, method.label()).changed();
                }
            });
            help::control(response.response, key);
        });
        changed
    }
}

/// Shared application tuning uses the same form grid as photographic controls.
pub fn adaptive_limits(ui: &mut egui::Ui, tuning: &mut foveonic::render::AdaptiveToneTuning) {
    let defaults = foveonic::render::AdaptiveToneTuning::default();
    for (profile, limits, initial) in [
        ("Natural", &mut tuning.natural, defaults.natural),
        ("Strong", &mut tuning.strong, defaults.strong),
    ] {
        ui.push_id(profile, |ui| {
            ui.strong(profile).on_hover_text("Strong targets greater visibility. Its caps must allow the Natural correction to preserve that relationship.");
            let column = LabelColumn::new(ui);
            for (name, value, default) in [
                ("Black limit", &mut limits.blacks, initial.blacks),
                ("Shadow limit", &mut limits.shadows, initial.shadows),
                ("Highlight limit", &mut limits.highlights, initial.highlights),
                ("White limit", &mut limits.whites, initial.whites),
            ] {
                column.adjustment(ui, (name, name), value, 0. ..=1., default);
            }
        });
    }
}
