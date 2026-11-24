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
            self.row_controls(label.1),
        )
    }

    pub(super) fn parameter<T: egui::emath::Numeric>(
        self,
        ui: &mut egui::Ui,
        pointer: impl Into<String>,
        label: (&str, &str),
        value: &mut T,
        range: RangeInclusive<T>,
        default: T,
    ) -> bool {
        crate::multi_edit_controls::field(ui, pointer, |ui| {
            self.adjustment(ui, label, value, range, default)
        })
    }
    pub(super) fn saturation_parameter(
        self,
        ui: &mut egui::Ui,
        pointer: impl Into<String>,
        label: (&str, &str),
        amount: &mut f32,
    ) -> bool {
        crate::multi_edit_controls::mapped_field(
            ui,
            pointer,
            crate::multi_edit_controls::Mapping::Saturation,
            |ui| self.saturation_adjustment(ui, label, amount),
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
    adjustment_row(ui, label, value, range, default, RowControls::default())
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
        RowControls {
            step: Some(step),
            ..Default::default()
        },
    )
}

/// Up/Down nudges the shown digit before the last, which Option+Up/Down nudges.
const COARSE_NUDGE_FACTOR: f64 = 10.;

/// A focused value box's nudge from `value`: floats move by their shown
/// digits and land on them, stepped rows by whole steps, integers by one.
pub(super) fn nudged(
    value: f64,
    coarse_nudges: f64,
    fine_nudges: f64,
    step: Option<f64>,
    integral: bool,
    bounds: [f64; 2],
) -> f64 {
    let next = match step {
        Some(step) => value + (coarse_nudges + fine_nudges) * step,
        None if integral => value + coarse_nudges + fine_nudges,
        None => {
            // Start from the shown value, so digits the box hides stay hidden.
            let last = crate::widgets::last_shown_digit(value, false);
            let shown = (value / last).round() * last;
            let moved = shown + coarse_nudges * last * COARSE_NUDGE_FACTOR + fine_nudges * last;
            let grid = crate::widgets::last_shown_digit(moved, false);
            (moved / grid).round() * grid
        }
    };
    next.clamp(bounds[0], bounds[1])
}

/// Auto follows the photo's automatic value and stays editable: moving the
/// value makes it manual, and Auto returns to the automatic value.
pub(super) struct Automatic<'a> {
    active: &'a mut bool,
    calculated: Option<f64>,
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
        RowControls {
            automatic: Some(Automatic {
                active: &mut automatic,
                calculated: calculated.map(|v| v.to_f64()),
            }),
            ..controls
        },
    );
    *manual = (!automatic).then_some(value);
    if before.is_none() != automatic {
        crate::multi_edit_controls::optional_current(ui.ctx(), !automatic);
    }
    changed || before != manual.map(|v| v.to_f64())
}

pub(super) fn adjustment_row<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    range: RangeInclusive<T>,
    default: T,
    controls: RowControls<'_>,
) -> bool {
    let RowControls {
        mut automatic,
        enabled,
        step,
        step_origin,
        label: label_override,
    } = controls;
    let range = crate::multi_edit_controls::control_range(ui.ctx(), range);
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
                let value_width = crate::widgets::numeric_field_width(ui, &range);
                ui.spacing_mut().slider_width =
                    (ui.available_width() - value_width - ui.spacing().item_spacing.x - auto_width)
                        .max(35.);
                let slider_response = ui
                    .add_enabled_ui(crate::multi_edit_controls::numeric_ready(ui.ctx()), |ui| {
                        // Apply to every editing slider while leaving adjacent
                        // value fields, buttons and group backgrounds unchanged.
                        ui.visuals_mut().widgets.inactive.bg_fill =
                            crate::widgets::slider_track_fill(ui.visuals());
                        let before_slider = value.to_f64();
                        let response = crate::multi_edit_controls::slider(
                            ui,
                            value,
                            range.clone(),
                            default,
                            step,
                        );
                        if response.changed() || before_slider != value.to_f64() {
                            if let Some(auto) = automatic.as_mut() {
                                *auto.active = false;
                            }
                        }
                        help::control(response, label)
                    })
                    .inner;
                if let Some(auto) = &mut automatic {
                    let response = ui
                        .add_enabled_ui(
                            !*auto.active
                                || crate::multi_edit_controls::optional_current_ready(ui.ctx()),
                            |ui| crate::widgets::auto_button(ui, *auto.active),
                        )
                        .inner;
                    if help::attach(response, help::automatic(label).unwrap()).clicked() {
                        // Leaving Auto keeps the shown value as the manual one.
                        *auto.active = !*auto.active;
                        if let Some(calculated) = auto.calculated.filter(|_| *auto.active) {
                            *value = T::from_f64(calculated);
                        }
                    }
                }
                let unresolved = automatic
                    .as_ref()
                    .is_some_and(|a| *a.active && a.calculated.is_none());
                // DragValue handles keys while drawing. Find its stable ID first so our
                // focused nudge can run without changing the row's layout.
                let mixed = crate::multi_edit_controls::number_mixed(ui.ctx());
                let response = ui
                    .add_enabled_ui(crate::multi_edit_controls::numeric_ready(ui.ctx()), |ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(value_width, ROW_HEIGHT),
                            egui::Layout::centered_and_justified(ui.layout().main_dir()),
                            |ui| {
                                if unresolved {
                                    return ui.add(egui::Label::new("Auto"));
                                }
                                let id = ui.next_auto_id();
                                let focused = ui.memory(|memory| memory.has_focus(id));
                                // Taking the arrows before egui's value box replaces
                                // its fixed nudge with one that follows the shown digits.
                                let (coarse_nudges, fine_nudges) = if focused {
                                    ui.input_mut(|input| {
                                        let mut count = |modifiers| {
                                            input.count_and_consume_key(
                                                modifiers,
                                                egui::Key::ArrowUp,
                                            ) as f64
                                                - input.count_and_consume_key(
                                                    modifiers,
                                                    egui::Key::ArrowDown,
                                                )
                                                    as f64
                                        };
                                        // Unmodified keys match Option too, so count it first.
                                        let fine = count(egui::Modifiers::ALT);
                                        (count(egui::Modifiers::NONE), fine)
                                    })
                                } else {
                                    (0., 0.)
                                };
                                let before_nudge = value.to_f64();
                                if coarse_nudges != 0. || fine_nudges != 0. {
                                    *value = T::from_f64(nudged(
                                        before_nudge,
                                        coarse_nudges,
                                        fine_nudges,
                                        step,
                                        T::INTEGRAL,
                                        [range.start().to_f64(), range.end().to_f64()],
                                    ));
                                }
                                let nudge_changed = value.to_f64() != before_nudge;
                                if nudge_changed {
                                    // Show the new value instead of the text being edited.
                                    ui.data_mut(|data| data.remove::<String>(id));
                                }
                                let mut response = ui.add(
                                    crate::widgets::mixed_drag_value(value, mixed && !focused)
                                        .range(range.clone())
                                        // Drawing a stored value must never change it.
                                        .clamp_existing_to_range(false)
                                        .speed(if T::INTEGRAL { 1. } else { 0.01 })
                                        // Shift-drags add a decimal; keep them to the shown ones.
                                        .max_decimals(crate::widgets::VALUE_DECIMAL_PLACES),
                                );
                                if nudge_changed {
                                    response.mark_changed();
                                }
                                response
                            },
                        )
                        .inner
                    })
                    .inner;
                if response.changed() {
                    if let Some(auto) = automatic.as_mut() {
                        *auto.active = false;
                    }
                }
                if response.changed() {
                    // Refresh the cached text after snapping so the value box does not
                    // show the typed overshoot.
                    let mut accepted = value.to_f64();
                    if let Some(step) = step {
                        accepted = ((accepted - step_origin) / step).round() * step + step_origin;
                    }
                    let accepted = accepted.clamp(range.start().to_f64(), range.end().to_f64());
                    if value.to_f64() != accepted {
                        *value = T::from_f64(accepted);
                        ui.data_mut(|data| data.remove::<String>(response.id));
                        ui.ctx().request_repaint();
                    }
                    crate::multi_edit_controls::number(
                        ui.ctx(),
                        crate::multi_edit_controls::NumberChange::Set(value.to_f64()),
                        [range.start().to_f64(), range.end().to_f64()],
                    );
                }
                if crate::automation::enabled() {
                    let property = crate::multi_edit_controls::observed_property(ui.ctx());
                    crate::automation::control(
                        ui,
                        crate::automation::ControlInput {
                            label,
                            pointer: property.as_ref().map(|(pointer, _)| pointer.clone()),
                            mapping: property.map_or(
                                crate::multi_edit_controls::Mapping::default(),
                                |(_, mapping)| mapping,
                            ),
                            slider: &slider_response,
                            readout: &response,
                            value: value.to_f64(),
                            range: [range.start().to_f64(), range.end().to_f64()],
                            step: step.or(T::INTEGRAL.then_some(1.)),
                            mixed,
                        },
                    );
                }
                help::control(response, label);
            });
        });
    });
    // The double-click response can hide the slider's changed flag.
    // Compare values so a real edit still requests a render.
    toggled || before != value.to_f64()
}

pub(super) fn rendering(ui: &mut egui::Ui, settings: &Settings) -> bool {
    crate::multi_edit_controls::choice(ui, "/rendering", settings.rendering, |ui, rendering| {
        let mut changed = false;
        ui.horizontal(|ui| {
            help::control(label(ui, "Rendering"), "Rendering");
            let mixed = crate::multi_edit_controls::mixed(ui.ctx());
            for (value, label) in [
                (Rendering::Color, "Color"),
                (Rendering::Monochrome, "Monochrome"),
            ] {
                if help::control(
                    crate::widgets::button(
                        ui,
                        label,
                        !mixed && *rendering == value,
                        egui::vec2(if value == Rendering::Color { 55. } else { 100. }, 22.),
                    ),
                    "Rendering",
                )
                .clicked()
                {
                    crate::multi_edit_controls::choice_clicked(ui.ctx());
                    changed |= *rendering != value || mixed;
                    *rendering = value;
                }
            }
        });
        changed
    })
}

pub(super) fn color_mode(
    ui: &mut egui::Ui,
    settings: &Settings,
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
            settings.color_mode.clone(),
            true,
        )
}

pub(super) fn white_balance(
    ui: &mut egui::Ui,
    settings: &Settings,
    metadata: Option<&Metadata>,
    enabled: bool,
) -> bool {
    section_header(ui, EditingSection::WhiteBalance, "White Balance", None);
    let column = LabelColumn::new(ui);
    let choice = if settings.white_balance_bias.neutral.is_some() {
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
        choice,
        enabled,
    );
    if changed {
        crate::multi_edit_controls::set(
            ui.ctx(),
            crate::multi_edit_controls::field::WHITE_BALANCE_BIAS,
            &Default::default(),
        );
        crate::editor::white_balance::exit(ui.ctx());
    }
    let mut bias = settings.white_balance_bias;
    ui.add_enabled_ui(!settings.is_monochrome(), |ui| {
        changed |= crate::multi_edit_controls::field(ui, "/white_balance_bias/warmth", |ui| {
            column.adjustment(ui, ("Warmth", "Warmth"), &mut bias.warmth, -1. ..=1., 0.)
        });
        changed |= crate::multi_edit_controls::field(ui, "/white_balance_bias/tint", |ui| {
            column.adjustment(ui, ("Tint", "Tint"), &mut bias.tint, -1. ..=1., 0.)
        });
    });
    changed
}

fn camera_choice(
    ui: &mut egui::Ui,
    column: LabelColumn,
    metadata: Option<&Metadata>,
    key: &str,
    labels: (&str, &str, bool),
    value: String,
    enabled: bool,
) -> bool {
    let pointer = if key == "WhiteBalanceGains" {
        "/white_balance"
    } else {
        "/color_mode"
    };
    crate::multi_edit_controls::choice(ui, pointer, value, |ui, value| {
        camera_choice_widget(ui, column, metadata, key, labels, value, enabled)
    })
}
fn camera_choice_widget(
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
                                if !crate::multi_edit_controls::camera_choice_available(
                                    ui.ctx(),
                                    white_balance,
                                    name,
                                ) {
                                    continue;
                                }
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
    settings: &Settings,
    compensation: Option<foveonic::source::CaptureExposureCompensation>,
) -> bool {
    tone_with_exposure_range(ui, settings, -4. ..=4., compensation)
}

pub(super) fn tone_with_exposure_range(
    ui: &mut egui::Ui,
    settings: &Settings,
    exposure_range: RangeInclusive<f32>,
    compensation: Option<foveonic::source::CaptureExposureCompensation>,
) -> bool {
    tone_rows(ui, settings, exposure_range, compensation)
}

/// Draw a section's bypass switch and push its change.
pub(super) fn section_switch(
    ui: &mut egui::Ui,
    section: EditingSection,
    title: &str,
    field: crate::multi_edit_controls::Field<bool>,
    enabled: bool,
) -> bool {
    section_switch_with(ui, section, title, field, enabled, |_, _| {})
}

pub(super) fn section_switch_with(
    ui: &mut egui::Ui,
    section: EditingSection,
    title: &str,
    field: crate::multi_edit_controls::Field<bool>,
    mut enabled: bool,
    contents: impl FnOnce(&mut egui::Ui, bool),
) -> bool {
    let changed = section_header_with(ui, section, title, Some(&mut enabled), contents);
    if changed {
        crate::multi_edit_controls::set(ui.ctx(), field, &enabled);
    }
    changed
}

/// Draw a checkbox for `field` and push its change.
pub(super) fn checkbox(
    ui: &mut egui::Ui,
    label: &str,
    field: crate::multi_edit_controls::Field<bool>,
    mut value: bool,
) -> egui::Response {
    let response = crate::widgets::checkbox(ui, &mut value, label);
    if response.changed() {
        crate::multi_edit_controls::set(ui.ctx(), field, &value);
    }
    response
}

impl LabelColumn {
    /// A checkbox in the label column for `field`, which pushes its change.
    pub(super) fn switch(
        self,
        ui: &mut egui::Ui,
        label: &str,
        field: crate::multi_edit_controls::Field<bool>,
        mut value: bool,
    ) -> egui::Response {
        let response = self.toggle(ui, label, &mut value);
        if response.changed() {
            crate::multi_edit_controls::set(ui.ctx(), field, &value);
        }
        response
    }
}

fn exposure_row(
    ui: &mut egui::Ui,
    settings: &Settings,
    column: LabelColumn,
    range: RangeInclusive<f32>,
    compensation: Option<foveonic::source::CaptureExposureCompensation>,
) -> bool {
    let calculated = compensation.and_then(|c| settings.exposure_auto_compensation.inverse(c));
    let mut automatic = settings.exposure_auto;
    let mut value = if automatic {
        calculated.unwrap_or(0.)
    } else {
        settings.exposure
    };
    let range = range.start().min(calculated.unwrap_or(*range.start()))
        ..=range.end().max(calculated.unwrap_or(*range.end()));
    let changed = crate::multi_edit_controls::field(ui, "/exposure", |ui| {
        adjustment_row(
            ui,
            "Exposure",
            &mut value,
            range,
            0.,
            RowControls {
                automatic: Some(Automatic {
                    active: &mut automatic,
                    calculated: calculated.map(f64::from),
                }),
                ..column.row_controls("Exposure")
            },
        )
    });
    // The store gives each photo leaving Auto its own calculated exposure.
    let toggled = automatic != settings.exposure_auto;
    if toggled {
        crate::multi_edit_controls::set(
            ui.ctx(),
            crate::multi_edit_controls::field::EXPOSURE_AUTO,
            &automatic,
        );
    }
    changed || toggled
}

fn tone_rows(
    ui: &mut egui::Ui,
    settings: &Settings,
    exposure_range: RangeInclusive<f32>,
    compensation: Option<foveonic::source::CaptureExposureCompensation>,
) -> bool {
    let mut changed = section_switch(
        ui,
        EditingSection::Tone,
        "Tone",
        crate::multi_edit_controls::field::TONE_ENABLED,
        settings.tone_enabled,
    );
    ui.add_enabled_ui(settings.tone_enabled, |ui| {
        let column = LabelColumn::new(ui);
        changed |= column.method_choice(
            ui,
            "Tone method",
            "Method",
            settings.photographic.tone_method,
        );
        let universal =
            settings.photographic.tone_method == foveonic::render::PhotographicMethod::Universal;
        if universal {
            use foveonic::render::BaseResponse;
            crate::multi_edit_controls::choice(
                ui,
                "/photographic/base_response",
                settings.photographic.base_response,
                |ui, selected| {
                    ui.horizontal(|ui| {
                        help::control(
                            aligned_label(ui, "Adaptive", None, Some(column.width_at(ui))),
                            "Adaptive",
                        );
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
                                changed |= crate::widgets::menu_value(ui, selected, choice, name)
                                    .changed();
                            }
                        })
                        .response;
                        help::control(response, "Adaptive");
                    })
                },
            );
        }
        changed |= exposure_row(ui, settings, column, exposure_range, compensation);
        let mut photographic = settings.photographic;
        let mut tone = settings.tone;
        if universal {
            ui.weak("Dynamic range");
            let mut tonal_ranges = settings.tonal_ranges;
            for (label, pointer, value) in [
                (
                    "Shadows",
                    "/tonal_ranges/shadows",
                    &mut tonal_ranges.shadows,
                ),
                (
                    "Highlights",
                    "/tonal_ranges/highlights",
                    &mut tonal_ranges.highlights,
                ),
                ("Blacks", "/tonal_ranges/blacks", &mut tonal_ranges.blacks),
                ("Whites", "/tonal_ranges/whites", &mut tonal_ranges.whites),
            ] {
                changed |= ui
                    .push_id(("tonal-ranges", label), |ui| {
                        column.parameter(
                            ui,
                            pointer,
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
            changed |= crate::multi_edit_controls::field(ui, "/photographic/brightness", |ui| {
                column.adjustment(
                    ui,
                    ("Brightness", "Brightness"),
                    &mut photographic.brightness,
                    -1. ..=1.,
                    0.,
                )
            });
        }
        changed |= crate::multi_edit_controls::field(ui, "/tone/contrast", |ui| {
            column.adjustment(
                ui,
                ("Contrast", "Contrast"),
                &mut tone.contrast,
                -2. ..=2.,
                0.,
            )
        });
        if universal {
            changed |=
                crate::multi_edit_controls::field(ui, "/photographic/local_contrast", |ui| {
                    column.adjustment(
                        ui,
                        ("Local contrast", "Clarity"),
                        &mut photographic.local_contrast,
                        -1. ..=1.,
                        0.,
                    )
                });
        }
        if !universal {
            for (label, pointer, value) in [
                ("Shadows", "/tone/shadows", &mut tone.shadows),
                ("Highlights", "/tone/highlights", &mut tone.highlights),
            ] {
                changed |= column.parameter(ui, pointer, (label, label), value, -2. ..=2., 0.);
            }
        }
    });
    changed
}

pub(super) fn sharpening(
    ui: &mut egui::Ui,
    settings: &Settings,
    metadata: Option<&Metadata>,
) -> bool {
    sharpening_with_range(ui, settings, metadata, 0. ..=2.)
}

pub(super) fn sharpening_with_range(
    ui: &mut egui::Ui,
    settings: &Settings,
    metadata: Option<&Metadata>,
    reference_range: RangeInclusive<f32>,
) -> bool {
    let mut changed = section_switch(
        ui,
        EditingSection::Sharpening,
        "Sharpening",
        crate::multi_edit_controls::field::SHARPENING_ENABLED,
        settings.sharpening_enabled,
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
            settings.photographic.sharpening.method,
        );
        if settings.photographic.sharpening.method
            == foveonic::render::PhotographicMethod::Universal
        {
            let mut sharp = settings.photographic.sharpening;
            let column = LabelColumn::new(ui);
            changed |=
                crate::multi_edit_controls::field(ui, "/photographic/sharpening/amount", |ui| {
                    column.adjustment(
                        ui,
                        ("Universal amount", "Amount"),
                        &mut sharp.amount,
                        0. ..=4.,
                        0.,
                    )
                });
            changed |=
                crate::multi_edit_controls::field(ui, "/photographic/sharpening/radius", |ui| {
                    column.adjustment(
                        ui,
                        ("Universal radius", "Radius"),
                        &mut sharp.radius,
                        0.3..=4.,
                        0.8,
                    )
                });
            crate::widgets::disclosure(
                ui,
                egui::CollapsingHeader::new("Advanced").id_salt("universal-sharpening"),
                |ui| {
                    let column = LabelColumn::new(ui);
                    changed |= crate::multi_edit_controls::field(
                        ui,
                        "/photographic/sharpening/threshold",
                        |ui| {
                            adjustment_row(
                                ui,
                                "Sharpen threshold",
                                &mut sharp.threshold,
                                0. ..=0.2,
                                0.01,
                                column.row_controls("Threshold"),
                            )
                        },
                    );
                    changed |= crate::multi_edit_controls::field(
                        ui,
                        "/photographic/sharpening/halo_suppression",
                        |ui| {
                            column.adjustment(
                                ui,
                                ("Halo suppression", "Halos"),
                                &mut sharp.halo_suppression,
                                0. ..=1.,
                                0.5,
                            )
                        },
                    );
                },
            );
            return;
        }
        let column = LabelColumn::new(ui);
        let mut amount = settings.sharpen_amount;
        changed |= crate::multi_edit_controls::field(ui, "/sharpen_amount", |ui| {
            automatic_adjustment(
                ui,
                "Sharpen",
                &mut amount,
                *reference_range.start()..=reference_range.end().max(calculated.unwrap_or(0.)),
                0.,
                calculated,
                column.row_controls("Amount"),
            )
        });
    });
    changed
}

pub(super) fn pixel_corrections(ui: &mut egui::Ui, settings: &Settings) -> bool {
    let changed = help::control(
        checkbox(
            ui,
            "Speckles",
            crate::multi_edit_controls::field::DESPECKLE,
            settings.despeckle,
        ),
        "Speckles",
    )
    .changed();
    changed | bad_pixels(ui, settings)
}

pub(super) fn bad_pixels(ui: &mut egui::Ui, settings: &Settings) -> bool {
    let mut repair = settings.repair.is_some();
    let changed = help::control(
        crate::widgets::checkbox(ui, &mut repair, "Bad pixels"),
        "Bad pixels",
    )
    .changed();
    if changed {
        crate::multi_edit_controls::set(
            ui.ctx(),
            crate::multi_edit_controls::field::REPAIR,
            &repair.then_some(RepairThreshold::Camera),
        );
    }
    changed
}

pub(super) fn reconstruction(ui: &mut egui::Ui, settings: &Settings) -> bool {
    let mut reconstruction = settings.reconstruction.is_some();
    let changed = section_header(
        ui,
        EditingSection::Noise,
        "Noise and Detail",
        Some(&mut reconstruction),
    );
    if changed {
        crate::multi_edit_controls::optional(ui.ctx(), "/reconstruction", reconstruction);
    }
    changed
}

/// Shared method selection keeps family interpretation out of editing widgets.
pub(super) fn method_choice(
    ui: &mut egui::Ui,
    key: &str,
    label: &str,
    value: foveonic::render::PhotographicMethod,
) -> bool {
    LabelColumn::new(ui).method_choice(ui, key, label, value)
}

impl LabelColumn {
    pub(super) fn method_choice(
        self,
        ui: &mut egui::Ui,
        key: &str,
        label: &str,
        value: foveonic::render::PhotographicMethod,
    ) -> bool {
        let pointer = match key {
            "Tone method" => "/photographic/tone_method",
            "Saturation method" => "/photographic/saturation_method",
            "Sharpening method" => "/photographic/sharpening/method",
            _ => "",
        };
        crate::multi_edit_controls::choice(ui, pointer, value, |ui, value| {
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
        })
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
