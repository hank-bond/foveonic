//! Builds aligned sensor/tone editing rows with reset, focused value nudges, and sample-based rerender signals.

use crate::help;
use eframe::egui;
use foveonic::{
    render::{ColorFieldApproach, HighlightMode, Rendering, RepairThreshold, Settings},
    source::Metadata,
};
use std::ops::RangeInclusive;

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

struct Automatic<'a> {
    active: &'a mut bool,
    resolved: bool,
}

#[derive(Default)]
struct RowControls<'a> {
    automatic: Option<Automatic<'a>>,
    enabled: Option<&'a mut bool>,
}

/// Keep automatic selection on the value row rather than in a separate checkbox.
pub fn automatic_adjustment<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    manual: &mut Option<T>,
    range: RangeInclusive<T>,
    default: T,
    calculated: Option<T>,
    enabled: Option<&mut bool>,
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
            enabled,
        },
    );
    *manual = (!automatic).then_some(value);
    changed || before != manual.map(|v| v.to_f64())
}

fn adjustment_row<T: egui::emath::Numeric>(
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
    } = controls;
    // The two Highlights rows need separate help and widget IDs.
    let display_label = match label {
        "Highlight correction" | "Color-field highlights" => "Highlights",
        _ => label,
    };
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
                toggled =
                    help::control(crate::widgets::checkbox(ui, enabled, display_label), label)
                        .changed();
                *enabled
            } else {
                help::control(
                    ui.add_sized([92., 20.], egui::Label::new(display_label)),
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
                ui.spacing_mut().slider_width = (ui.available_width() - 62. - auto_width).max(35.);
                ui.add_enabled_ui(!automatic.as_ref().is_some_and(|a| *a.active), |ui| {
                    let response = ui
                        .add(egui::Slider::new(value, range.clone()).show_value(false))
                        .interact(egui::Sense::click_and_drag());
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
                            egui::vec2(54., 20.),
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
                let accepted = value
                    .to_f64()
                    .clamp(range.start().to_f64(), range.end().to_f64());
                if !active && value.to_f64() != accepted {
                    *value = T::from_f64(accepted);
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

const HIGHLIGHT_METHODS: [((HighlightMode, ColorFieldApproach), &str); 3] = [
    (
        (HighlightMode::Classic, ColorFieldApproach::Faithful),
        "Classic",
    ),
    (
        (HighlightMode::ColorField, ColorFieldApproach::LocalGradient),
        "Color Field: Local",
    ),
    (
        (HighlightMode::ColorField, ColorFieldApproach::Faithful),
        "Color Field: Global",
    ),
];

fn highlight_choice(settings: &Settings) -> (HighlightMode, ColorFieldApproach) {
    (
        settings.highlight_mode,
        if settings.highlight_mode == HighlightMode::Classic {
            ColorFieldApproach::Faithful
        } else {
            settings.color_field.approach
        },
    )
}

fn select_highlight(settings: &mut Settings, choice: (HighlightMode, ColorFieldApproach)) -> bool {
    let before = (settings.highlight_mode, settings.color_field);
    settings.highlight_mode = choice.0;
    if choice.0 == HighlightMode::ColorField {
        settings.color_field.approach = choice.1;
    }
    before != (settings.highlight_mode, settings.color_field)
}

pub fn processing(ui: &mut egui::Ui, settings: &mut Settings, metadata: Option<&Metadata>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        help::control(
            ui.add_sized([92., 20.], egui::Label::new("Rendering")),
            "Rendering",
        );
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
    let mono = settings.is_monochrome();
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
        if mono && title == "Color mode" {
            continue;
        }
        let choices = metadata.and_then(|metadata| metadata.lists.get(key));
        // Keep the same rows while metadata is loading so later controls do not
        // jump when the first processed frame supplies the camera's choices.
        ui.horizontal(|ui| {
            help::control(ui.add_sized([92., 20.], egui::Label::new(title)), title);
            let response = ui
                .add_enabled_ui(choices.is_some_and(|list| !list.is_empty()), |ui| {
                    let combo = egui::ComboBox::from_id_salt(key)
                        .width(ui.available_width() - 8.)
                        .selected_text(if value.is_empty() {
                            "Choose…"
                        } else {
                            value.as_str()
                        });
                    crate::widgets::dropdown(ui, combo, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.;
                        if let Some(list) = choices {
                            for name in list.names() {
                                changed |=
                                    crate::widgets::menu_value(ui, value, name.to_owned(), name)
                                        .changed();
                            }
                        }
                    })
                    .response
                })
                .inner;
            help::control(response, title);
        });
    }
    ui.separator();
    if mono {
        changed |= crate::mixer::show(ui, &mut settings.monochrome.mix);
        ui.separator();
    }
    ui.weak(if mono { "Tone" } else { "Tone and Color" });
    for (label, value, range, default) in [
        ("Exposure", &mut settings.exposure, -4. ..=4., 0.),
        ("Saturation", &mut settings.saturation, 0. ..=2., 1.),
    ] {
        if !mono || label != "Saturation" {
            changed |= adjustment(ui, label, value, range, default);
        }
    }
    for (label, value) in [
        ("Contrast", &mut settings.tone.contrast),
        ("Shadows", &mut settings.tone.shadows),
        ("Highlights", &mut settings.tone.highlights),
    ] {
        changed |= adjustment(ui, label, value, -2. ..=2., 0.);
    }
    ui.separator();
    if !mono {
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
        if let Some(controls) = &mut settings.reconstruction {
            changed |= adjustment(ui, "Chroma noise", &mut controls.chroma, 0..=4, 2);
            changed |= adjustment(ui, "Luma noise", &mut controls.luminance, 0..=4, 2);
            changed |= adjustment(ui, "Banding noise", &mut controls.banding, 0..=4, 2);
            changed |= adjustment(ui, "Detail", &mut controls.detail, 0. ..=2., 1.);
            changed |= crate::advanced::show(
                ui,
                controls,
                &mut settings.color_dq_gain,
                metadata,
                &settings.white_balance,
            );
        }
    } else {
        changed |= adjustment(ui, "Mono noise", &mut settings.monochrome.noise, 0..=4, 2);
        changed |= adjustment(
            ui,
            "Mono banding",
            &mut settings.monochrome.banding,
            0..=4,
            2,
        );
    }
    ui.separator();
    ui.weak("Refinement");
    let calculated = metadata.and_then(|metadata| {
        let mut automatic = settings.clone();
        automatic.sharpen_amount = None;
        automatic.effective_sharpen_amount(metadata).ok()
    });
    changed |= automatic_adjustment(
        ui,
        "Sharpen",
        &mut settings.sharpen_amount,
        0. ..=2_f32.max(calculated.unwrap_or(0.)),
        0.,
        calculated,
        None,
    );
    ui.separator();
    ui.weak("Corrections");
    ui.horizontal(|ui| {
        help::control(
            ui.add_sized([92., 20.], egui::Label::new("Method")),
            "Highlight method",
        );
        let mut choice = highlight_choice(settings);
        let label = HIGHLIGHT_METHODS
            .iter()
            .find(|(value, _)| *value == choice)
            .unwrap()
            .1;
        let combo = egui::ComboBox::from_id_salt("highlight-method")
            .width(ui.available_width() - 8.)
            .selected_text(label);
        let response = crate::widgets::dropdown(ui, combo, |ui| {
            for (value, label) in HIGHLIGHT_METHODS {
                if crate::widgets::menu_value(ui, &mut choice, value, label).clicked() {
                    changed |= select_highlight(settings, choice);
                }
            }
        })
        .response;
        help::control(response, "Highlight method");
    });
    if settings.highlight_mode == HighlightMode::Classic {
        changed |= automatic_adjustment(
            ui,
            "Highlight correction",
            &mut settings.highlight_threshold_ev,
            -2. ..=2.,
            0.,
            Some(0.),
            Some(&mut settings.highlight_restoration),
        );
    } else {
        changed |= adjustment_row(
            ui,
            "Color-field highlights",
            &mut settings.color_field.selectivity,
            0. ..=1.,
            0.,
            0.1,
            RowControls {
                enabled: Some(&mut settings.highlight_restoration),
                ..Default::default()
            },
        );
    }
    let advanced = egui::CollapsingHeader::new("Advanced")
        .id_salt("advanced-corrections")
        .show(ui, |ui| {
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
            changed |= help::control(
                crate::widgets::checkbox(ui, &mut settings.row_correction, "Row banding"),
                "Row banding",
            )
            .changed();
        });
    help::control(advanced.header_response, "Advanced corrections");
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn processing_expanded(
        ui: &mut egui::Ui,
        settings: &mut Settings,
        metadata: Option<&Metadata>,
    ) -> bool {
        // Use egui's test visibility switch for expanded control fixtures.
        // Disclosure clicks are checked separately with ordinary visibility.
        ui.ctx()
            .memory_mut(|memory| memory.set_everything_is_visible(true));
        processing(ui, settings, metadata)
    }
    fn text_centers(output: &egui::FullOutput, label: &str) -> Vec<egui::Pos2> {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .collect()
    }

    fn pointer_button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    #[test]
    fn editing_hides_export_controls_and_preserves_the_selected_tiff_space() {
        let ctx = egui::Context::default();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.tiff_color_space = Some(foveonic::render::TiffColorSpace::ProPhotoRgb);
        let before = settings.clone();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400., 1600.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert!(!processing(ui, &mut settings, None));
                });
            },
        );
        assert_eq!(settings, before);
        for label in ["Export", "TIFF space", "Camera", "ProPhoto RGB"] {
            assert!(text_centers(&output, label).is_empty());
        }
    }

    #[test]
    fn rendering_toggle_keeps_both_modes_and_hides_irrelevant_controls() {
        let ctx = egui::Context::default();
        let mut settings = Settings::new("Sunlight", "Vivid");
        settings.saturation = 1.4;
        settings.reconstruction = Some(foveonic::noise::Controls {
            chroma: 4,
            luminance: 1,
            detail: 1.3,
            ..Default::default()
        });
        let saved = settings.clone();
        let mut frame = |time, events| {
            let mut changed = false;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 2000.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        changed = processing(ui, &mut settings, None);
                    });
                },
            );
            (changed, output, settings.clone())
        };
        frame(0., vec![]);
        let (_, output, _) = frame(0.1, vec![]);
        let mono = text_centers(&output, "Monochrome")[0];
        frame(
            1.,
            vec![egui::Event::PointerMoved(mono), pointer_button(mono, true)],
        );
        let (changed, _, current) = frame(1.1, vec![pointer_button(mono, false)]);
        assert!(changed && current.is_monochrome());
        let (_, output, current) = frame(1.5, vec![]);
        assert_eq!(current.reconstruction, saved.reconstruction);
        assert_eq!(current.saturation, saved.saturation);
        for label in ["Color Mix", "Mono noise", "Mono banding"] {
            assert_eq!(text_centers(&output, label).len(), 1);
        }
        for label in [
            "Color mode",
            "Saturation",
            "Noise and Detail",
            "Chroma noise",
            "Luma noise",
            "Banding noise",
            "Detail",
            "ColorDQ gain",
        ] {
            assert!(
                text_centers(&output, label).is_empty(),
                "{label} must be hidden in monochrome"
            );
        }
        let blue = text_centers(&output, "B")[0] - egui::vec2(12., 0.);
        frame(
            4.,
            vec![egui::Event::PointerMoved(blue), pointer_button(blue, true)],
        );
        let (changed, _, after) = frame(4.1, vec![pointer_button(blue, false)]);
        assert!(changed);
        assert_eq!(after.monochrome.mix, [0., 0., 1.]);
        let color = text_centers(&output, "Color")[0];
        frame(
            5.,
            vec![
                egui::Event::PointerMoved(color),
                pointer_button(color, true),
            ],
        );
        let (_, output, after) = frame(5.1, vec![pointer_button(color, false)]);
        assert!(!after.is_monochrome());
        for label in ["Color mode", "Saturation", "Noise and Detail"] {
            assert_eq!(text_centers(&output, label).len(), 1);
        }
        for label in ["Color Mix", "Mono noise", "Mono banding"] {
            assert!(text_centers(&output, label).is_empty());
        }
        assert_eq!(after.monochrome.mix, [0., 0., 1.]);
        assert_eq!(after.reconstruction, saved.reconstruction);
        assert_eq!(after.color_mode, saved.color_mode);
        assert_eq!(after.saturation, saved.saturation);
        frame(
            6.,
            vec![egui::Event::PointerMoved(mono), pointer_button(mono, true)],
        );
        let (_, _, after) = frame(6.1, vec![pointer_button(mono, false)]);
        assert!(after.is_monochrome());
        assert_eq!(after.monochrome.mix, [0., 0., 1.]);
    }

    #[test]
    fn expanded_editing_shows_only_the_active_modes_controls_without_resetting_values() {
        for rendering in [Rendering::Color, Rendering::Monochrome] {
            let ctx = egui::Context::default();
            let mut settings = Settings::new("Sunlight", "Vivid");
            settings.rendering = rendering;
            settings.saturation = 1.4;
            settings.reconstruction = Some(Default::default());
            settings
                .reconstruction
                .as_mut()
                .unwrap()
                .overrides
                .set(foveonic::noise::Parameter::BaseBlend, Some(0.2));
            settings.color_dq_gain = Some(1.2);
            settings.monochrome.mix = [0.2, 0.3, 0.5];
            settings.monochrome.noise = 4;
            settings.monochrome.banding = 1;
            let before = settings.clone();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 2500.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        assert!(!processing_expanded(ui, &mut settings, None));
                    });
                },
            );
            assert_eq!(settings, before);
            for label in [
                "Color mode",
                "Saturation",
                "Noise and Detail",
                "Chroma noise",
                "Luma noise",
                "Banding noise",
                "Detail",
                "ColorDQ gain",
                "Base filter blend",
            ] {
                assert_eq!(
                    text_centers(&output, label).len(),
                    usize::from(rendering == Rendering::Color),
                    "{rendering:?}: {label}"
                );
            }
            for label in ["Color Mix", "Mono noise", "Mono banding"] {
                assert_eq!(
                    text_centers(&output, label).len(),
                    usize::from(rendering == Rendering::Monochrome),
                    "{rendering:?}: {label}"
                );
            }
            for label in [
                "White balance",
                "Exposure",
                "Contrast",
                "Shadows",
                "Sharpen",
                "Speckles",
                "Bad pixels",
                "Row banding",
            ] {
                assert_eq!(
                    text_centers(&output, label).len(),
                    1,
                    "{rendering:?}: {label}"
                );
            }
            assert_eq!(text_centers(&output, "Highlights").len(), 2);
        }
    }

    #[test]
    fn advanced_disclosures_are_independent_and_preserve_corrections() {
        let ctx = egui::Context::default();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        let defaults = settings.clone();
        let mut frame = |time, events| {
            let mut changed = false;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 3000.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        changed = processing(ui, &mut settings, None);
                    });
                },
            );
            (changed, output, settings.clone())
        };
        frame(0., vec![]);
        let (changed, output, initial) = frame(0.1, vec![]);
        assert!(!changed);
        assert_eq!(initial, defaults);
        assert!(initial.despeckle && initial.repair.is_some() && initial.row_correction);
        let headers = text_centers(&output, "Advanced");
        assert_eq!(headers.len(), 2);
        assert!(text_centers(&output, "Speckles").is_empty());
        let noise_parameter = foveonic::noise::Parameter::MeanDecay.label();
        assert!(text_centers(&output, noise_parameter).is_empty());
        assert!(text_centers(&output, "ColorDQ gain").is_empty());
        assert_eq!(text_centers(&output, "Tone and Color").len(), 1);
        assert_eq!(text_centers(&output, "Noise and Detail").len(), 1);
        for pressed in [true, false] {
            assert!(
                !frame(
                    if pressed { 1. } else { 1.1 },
                    vec![
                        egui::Event::PointerMoved(headers[0]),
                        pointer_button(headers[0], pressed)
                    ]
                )
                .0
            );
        }
        let (_, output, current) = frame(1.5, vec![egui::Event::PointerMoved(egui::Pos2::ZERO)]);
        assert_eq!(current, defaults);
        assert!(!text_centers(&output, noise_parameter).is_empty());
        assert_eq!(text_centers(&output, "ColorDQ gain").len(), 1);
        assert!(
            text_centers(&output, "ColorDQ gain")[0].y
                < text_centers(&output, foveonic::noise::Parameter::BaseBlend.label())[0].y
        );
        assert!(text_centers(&output, "Speckles").is_empty());
        let corrections = text_centers(&output, "Advanced")[1];
        for pressed in [true, false] {
            assert!(
                !frame(
                    if pressed { 2. } else { 2.1 },
                    vec![
                        egui::Event::PointerMoved(corrections),
                        pointer_button(corrections, pressed)
                    ]
                )
                .0
            );
        }
        let (_, output, current) = frame(2.5, vec![egui::Event::PointerMoved(egui::Pos2::ZERO)]);
        assert_eq!(current, defaults);
        assert!(!text_centers(&output, noise_parameter).is_empty());
        for label in ["Speckles", "Bad pixels", "Row banding"] {
            assert_eq!(text_centers(&output, label).len(), 1);
        }
        let noise = text_centers(&output, "Advanced")[0];
        for pressed in [true, false] {
            assert!(
                !frame(
                    if pressed { 3. } else { 3.1 },
                    vec![
                        egui::Event::PointerMoved(noise),
                        pointer_button(noise, pressed)
                    ]
                )
                .0
            );
        }
        let (_, output, current) = frame(3.5, vec![egui::Event::PointerMoved(egui::Pos2::ZERO)]);
        assert_eq!(current, defaults);
        assert!(text_centers(&output, noise_parameter).is_empty());
        assert!(text_centers(&output, "ColorDQ gain").is_empty());
        let speckles = text_centers(&output, "Speckles")[0];
        frame(
            4.,
            vec![
                egui::Event::PointerMoved(speckles),
                pointer_button(speckles, true),
            ],
        );
        let (changed, _, edited) = frame(4.1, vec![pointer_button(speckles, false)]);
        assert!(changed && !edited.despeckle);
        assert!(edited.repair.is_some() && edited.row_correction && edited.highlight_restoration);
        let corrections = text_centers(&output, "Advanced")[1];
        for pressed in [true, false] {
            assert!(
                !frame(
                    if pressed { 5. } else { 5.1 },
                    vec![
                        egui::Event::PointerMoved(corrections),
                        pointer_button(corrections, pressed)
                    ]
                )
                .0
            );
        }
        let (_, output, closed) = frame(5.5, vec![egui::Event::PointerMoved(egui::Pos2::ZERO)]);
        assert_eq!(closed, edited);
        assert!(text_centers(&output, "Speckles").is_empty());
        let corrections = text_centers(&output, "Advanced")[1];
        for pressed in [true, false] {
            assert!(
                !frame(
                    if pressed { 6. } else { 6.1 },
                    vec![
                        egui::Event::PointerMoved(corrections),
                        pointer_button(corrections, pressed)
                    ]
                )
                .0
            );
        }
        let (_, output, reopened) = frame(6.5, vec![egui::Event::PointerMoved(egui::Pos2::ZERO)]);
        assert_eq!(reopened, edited);
        assert_eq!(text_centers(&output, "Speckles").len(), 1);
        let mut saved =
            foveonic::sidecar::decode(&foveonic::sidecar::encode(&edited).unwrap()).unwrap();
        let other = egui::Context::default();
        let _ = other.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!processing(ui, &mut saved, None));
            });
        });
        assert_eq!(saved, edited, "collapsed controls reset a saved choice");
    }

    #[test]
    fn camera_choices_keep_loading_layout_and_saved_values_until_metadata_arrives() {
        use foveonic::source::PropertyList;
        let metadata = Metadata {
            width: 1,
            height: 1,
            rotation: 0,
            properties: Default::default(),
            matrices: Default::default(),
            lists: [
                (
                    "WhiteBalanceGains".into(),
                    PropertyList::new(vec![
                        ("Sunlight".into(), "sun".into()),
                        ("Shade".into(), "shade".into()),
                    ])
                    .unwrap(),
                ),
                (
                    "ColorModeCompensations".into(),
                    PropertyList::new(vec![
                        ("Standard".into(), "standard".into()),
                        ("Vivid".into(), "vivid".into()),
                    ])
                    .unwrap(),
                ),
            ]
            .into_iter()
            .collect(),
        };
        let mut empty_metadata = metadata.clone();
        empty_metadata.lists.clear();
        for initial in [
            Settings::new("", ""),
            Settings::new("Sunlight", "Standard"),
            Settings {
                rendering: Rendering::Monochrome,
                ..Settings::new("", "")
            },
            Settings {
                rendering: Rendering::Monochrome,
                ..Settings::new("Sunlight", "Standard")
            },
        ] {
            let ctx = egui::Context::default();
            let mut settings = initial.clone();
            let mut time = 0.;
            let mut frame = |metadata: Option<&Metadata>, events| {
                time += 0.05;
                let mut changed = false;
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 1500.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            changed = processing(ui, &mut settings, metadata);
                        });
                    },
                );
                (changed, output, settings.clone())
            };
            let mut anchors = None;
            for available in [None, Some(&empty_metadata), Some(&metadata), None] {
                let (changed, output, current) = frame(available, vec![]);
                assert!(!changed);
                assert_eq!(current, initial);
                let labels: &[&str] = if initial.is_monochrome() {
                    assert!(text_centers(&output, "Color mode").is_empty());
                    &["White balance", "Exposure", "Refinement"]
                } else {
                    &["White balance", "Color mode", "Exposure", "Refinement"]
                };
                let positions: Vec<_> = labels
                    .iter()
                    .map(|label| text_centers(&output, label)[0])
                    .collect();
                assert_eq!(*anchors.get_or_insert(positions.clone()), positions);
            }
            let (_, output, _) = frame(None, vec![]);
            let value = if initial.white_balance.is_empty() {
                "Choose…"
            } else {
                "Sunlight"
            };
            let point = text_centers(&output, value)[0];
            for pressed in [true, false] {
                let (changed, output, current) = frame(
                    None,
                    vec![
                        egui::Event::PointerMoved(point),
                        pointer_button(point, pressed),
                    ],
                );
                assert!(!changed);
                assert_eq!(current, initial);
                assert!(text_centers(&output, "Shade").is_empty());
            }
            frame(Some(&metadata), vec![]);
            for pressed in [true, false] {
                frame(
                    Some(&metadata),
                    vec![
                        egui::Event::PointerMoved(point),
                        pointer_button(point, pressed),
                    ],
                );
            }
            let (_, output, _) = frame(Some(&metadata), vec![]);
            let shade = text_centers(&output, "Shade")[0];
            frame(
                Some(&metadata),
                vec![
                    egui::Event::PointerMoved(shade),
                    pointer_button(shade, true),
                ],
            );
            let (changed, _, current) = frame(Some(&metadata), vec![pointer_button(shade, false)]);
            assert!(changed);
            assert_eq!(current.white_balance, "Shade");
            assert_eq!(current.color_mode, initial.color_mode);
        }
    }

    #[test]
    fn all_visible_editing_controls_have_explanations() {
        let ctx = egui::Context::default();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        settings.color_dq_gain = Some(1.);
        settings.repair = Some(RepairThreshold::Camera);
        let metadata = Metadata {
            width: 1,
            height: 1,
            rotation: 0,
            properties: Default::default(),
            lists: Default::default(),
            matrices: Default::default(),
        };
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 2000.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    processing_expanded(ui, &mut settings, Some(&metadata));
                });
            },
        );
        assert!(text_centers(&output, "Sensor").is_empty());
        assert!(text_centers(&output, "Sensor gain").is_empty());
        for label in [
            "White balance",
            "Color mode",
            "Exposure",
            "Saturation",
            "Contrast",
            "Shadows",
            "Highlights",
            "Noise and Detail",
            "Chroma noise",
            "Luma noise",
            "Banding noise",
            "Detail",
            "ColorDQ gain",
            "Sharpen",
            "Bad pixels",
            "Speckles",
            "Row banding",
        ] {
            assert!(
                output.shapes.iter().any(|shape| matches!(
                    &shape.shape, egui::Shape::Text(text) if text.galley.job.text == label
                )),
                "control missing: {label}"
            );
            assert!(help::editing(label).is_some(), "help missing: {label}");
        }
        assert_eq!(
            output
                .shapes
                .iter()
                .filter(|shape| matches!(
                    &shape.shape, egui::Shape::Text(text) if text.galley.job.text == "Highlights"
                ))
                .count(),
            2
        );
        assert!(help::editing("Highlight correction").is_some());
    }

    #[test]
    fn three_highlight_choices_preserve_selectivity_threshold_and_enable_state() {
        assert_eq!(
            HIGHLIGHT_METHODS.map(|(_, label)| label),
            ["Classic", "Color Field: Local", "Color Field: Global"]
        );
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.highlight_restoration = false;
        settings.highlight_threshold_ev = Some(1.25);
        settings.color_field.selectivity = 0.5;
        for index in [2, 1, 0, 1, 2] {
            let choice = HIGHLIGHT_METHODS[index].0;
            select_highlight(&mut settings, choice);
            assert_eq!(highlight_choice(&settings), choice);
            assert_eq!(settings.highlight_threshold_ev, Some(1.25));
            assert!(!settings.highlight_restoration);
            assert_eq!(settings.color_field.selectivity, 0.5);
            if choice.0 == HighlightMode::ColorField {
                assert_eq!(settings.color_field.approach, choice.1);
            }
            assert!(!select_highlight(&mut settings, choice));
        }
    }

    #[test]
    fn color_field_controls_preserve_edits_and_remain_distinct_from_stock_threshold() {
        for mode in [HighlightMode::Classic, HighlightMode::ColorField] {
            for enabled in [false, true] {
                let ctx = egui::Context::default();
                let mut settings = Settings::new("Sunlight", "Standard");
                settings.highlight_mode = mode;
                settings.highlight_restoration = enabled;
                settings.highlight_threshold_ev = Some(1.25);
                settings.color_field.selectivity = 0.54321;
                let before = settings.clone();
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 2000.),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            assert!(!processing(ui, &mut settings, None));
                        });
                    },
                );
                assert_eq!(settings, before);
                let shown = |label: &str| {
                    output.shapes.iter().any(|shape| matches!(&shape.shape,egui::Shape::Text(text) if text.galley.job.text==label))
                };
                assert!(!shown("Color donation"));
                assert!(!shown("Color agreement"));
                assert!(!shown("Boundary EV"));
                assert!(!shown("Color-field tuning"));
                assert!(!shown("Color restraint"));
                assert!(shown("Highlights"));
                let y = text_centers(&output, "Highlights").last().unwrap().y;
                assert_eq!(
                    text_centers(&output, "A")
                        .iter()
                        .filter(|position| (position.y - y).abs() < 2.)
                        .count(),
                    if mode == HighlightMode::Classic { 1 } else { 0 }
                );
                assert!(
                    text_centers(&output, "Method")[0].y
                        < text_centers(&output, "Highlights").last().unwrap().y
                );
                assert!(shown("Method"));
                for label in [
                    "Highlight method",
                    "Color-field highlights",
                    "Highlight correction",
                ] {
                    assert!(help::editing(label).is_some());
                }
            }
        }
    }

    #[test]
    fn all_highlight_methods_share_manual_reset_and_enabled_pointer_behavior() {
        for ((mode, approach), label) in HIGHLIGHT_METHODS {
            let ctx = egui::Context::default();
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.highlight_mode = mode;
            settings.color_field.approach = approach;
            settings.highlight_threshold_ev = (mode != HighlightMode::Classic).then_some(1.25);
            settings.color_field.selectivity = if mode == HighlightMode::Classic {
                0.5
            } else {
                0.
            };
            let amount = |s: &Settings| {
                if mode == HighlightMode::Classic {
                    s.highlight_threshold_ev
                } else {
                    Some(s.color_field.selectivity)
                }
            };
            let reset = 0.;
            let frame = |time: f64, events: Vec<egui::Event>, settings: &mut Settings| {
                let mut changed = false;
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 2000.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            changed |= processing(ui, settings, None);
                        });
                    },
                );
                (output, changed)
            };
            let (output, changed) = frame(0., Vec::new(), &mut settings);
            assert!(!changed);
            let y = text_centers(&output, "Highlights").last().unwrap().y;
            let auto = text_centers(&output, "A")
                .into_iter()
                .find(|position| (position.y - y).abs() < 2.);
            if mode == HighlightMode::Classic {
                assert_eq!(amount(&settings), None, "Classic starts in A");
                let auto = auto.expect("Classic retains A");
                let mut requested = false;
                for (time, pressed) in [(0.1, true), (0.2, false)] {
                    requested |= frame(
                        time,
                        vec![
                            egui::Event::PointerMoved(auto),
                            pointer_button(auto, pressed),
                        ],
                        &mut settings,
                    )
                    .1;
                }
                assert!(requested);
                assert_eq!(amount(&settings), Some(reset));
            } else {
                assert!(auto.is_none(), "{label} has no redundant A button");
                assert_eq!(amount(&settings), Some(0.), "Color Field starts at zero");
            }
            frame(0.6, Vec::new(), &mut settings);
            for (time, pos, pressed) in [
                (1., egui::pos2(420., y), Some(true)),
                (1.1, egui::pos2(250., y), None),
                (1.2, egui::pos2(250., y), Some(false)),
            ] {
                let before = settings.clone();
                let mut events = vec![egui::Event::PointerMoved(pos)];
                if let Some(pressed) = pressed {
                    events.push(pointer_button(pos, pressed));
                }
                let (_, changed) = frame(time, events, &mut settings);
                if pressed != Some(false) {
                    assert_ne!(
                        amount(&before),
                        amount(&settings),
                        "{label} must accept slider edits"
                    );
                    assert!(changed, "{label} must request rendering");
                }
                let mut expected = before;
                if mode == HighlightMode::Classic {
                    expected.highlight_threshold_ev = settings.highlight_threshold_ev;
                } else {
                    expected.color_field.selectivity = settings.color_field.selectivity;
                }
                assert_eq!(
                    settings, expected,
                    "only the current method's amount may change"
                );
            }
            let pos = egui::pos2(350., y);
            let mut reset_requested = false;
            for (time, pressed) in [(2., true), (2.1, false), (2.2, true), (2.3, false)] {
                let (_, changed) = frame(
                    time,
                    vec![egui::Event::PointerMoved(pos), pointer_button(pos, pressed)],
                    &mut settings,
                );
                reset_requested |= changed && amount(&settings) == Some(reset);
            }
            assert_eq!(
                amount(&settings),
                Some(reset),
                "double-click resets the manual value"
            );
            assert!(reset_requested);
            if let Some(auto) = auto {
                for (time, pressed) in [(3., true), (3.1, false)] {
                    frame(
                        time,
                        vec![
                            egui::Event::PointerMoved(auto),
                            pointer_button(auto, pressed),
                        ],
                        &mut settings,
                    );
                }
                assert_eq!(
                    amount(&settings),
                    None,
                    "Classic A restores automatic behavior"
                );
            }
            settings.highlight_restoration = false;
            if mode == HighlightMode::Classic {
                settings.highlight_threshold_ev = Some(0.5);
            } else {
                settings.color_field.selectivity = 0.5;
            }
            // Present disabled hit regions before the next pointer interaction.
            assert!(!frame(3.7, Vec::new(), &mut settings).1);
            let disabled = settings.clone();
            for (time, position, pressed) in [
                (4., pos, true),
                (4.1, pos, false),
                (5., auto.unwrap_or(pos), true),
                (5.1, auto.unwrap_or(pos), false),
            ] {
                assert!(
                    !frame(
                        time,
                        vec![
                            egui::Event::PointerMoved(position),
                            pointer_button(position, pressed)
                        ],
                        &mut settings
                    )
                    .1
                );
                assert_eq!(settings, disabled);
            }
            let (output, _) = frame(5.5, Vec::new(), &mut settings);
            let checkbox = *text_centers(&output, "Highlights").last().unwrap();
            let mut requested = false;
            for (time, pressed) in [(6., true), (6.1, false)] {
                requested |= frame(
                    time,
                    vec![
                        egui::Event::PointerMoved(checkbox),
                        pointer_button(checkbox, pressed),
                    ],
                    &mut settings,
                )
                .1;
            }
            let mut enabled = disabled;
            enabled.highlight_restoration = true;
            assert_eq!(settings, enabled, "checkbox enables only reconstruction");
            assert!(requested, "checkbox must request rendering");
        }
    }

    #[test]
    fn colordq_stays_inside_collapsed_noise_advanced_and_repair_has_no_parameter_row() {
        for reconstruction in [false, true] {
            for manual_gain in [None, Some(1.2)] {
                let ctx = egui::Context::default();
                let mut settings = Settings::new("Sunlight", "Standard");
                settings.reconstruction = reconstruction.then(Default::default);
                settings.color_dq_gain = manual_gain;
                let before = settings.clone();
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 2000.),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            assert!(!processing(ui, &mut settings, None));
                        });
                    },
                );
                assert_eq!(settings, before);
                let position = |label: &str| {
                    output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == label => {
                            Some(text.pos.y)
                        }
                        _ => None,
                    })
                };
                assert!(position("Bad pixels").is_none());
                assert_eq!(
                    output
                        .shapes
                        .iter()
                        .filter(|shape| matches!(
                            &shape.shape,
                            egui::Shape::Text(text) if text.galley.job.text == "Noise and Detail"
                        ))
                        .count(),
                    1,
                    "the section heading and enable toggle must share one label"
                );
                assert!(position("Reconstruction").is_none());
                assert!(position("Advanced reconstruction").is_none());
                assert!(
                    position("Classic").is_some(),
                    "the selected highlight method is explicit"
                );
                for removed in [
                    "Camera-record repair",
                    "Min. badness",
                    "Waiting for camera cutoff",
                    "ColorDQ inactive without reconstruction",
                ] {
                    assert!(position(removed).is_none(), "obsolete control: {removed}");
                }
                assert!(position("ColorDQ gain").is_none());
                let auto_buttons = output
                    .shapes
                    .iter()
                    .filter(|shape| {
                        matches!(
                            &shape.shape,
                            egui::Shape::Text(text) if text.galley.job.text == "A"
                        )
                    })
                    .count();
                assert_eq!(auto_buttons, 2);
                let advanced_headers = output.shapes.iter().filter(|shape| matches!(
                    &shape.shape, egui::Shape::Text(text) if text.galley.job.text == "Advanced"
                )).count();
                assert_eq!(advanced_headers, 1 + usize::from(reconstruction));
                if reconstruction {
                    let y = position("Advanced").unwrap();
                    assert!(y > position("Detail").unwrap());
                    assert!(y < position("Refinement").unwrap());
                }
            }
        }
    }

    #[test]
    fn automatic_buttons_replace_checkboxes_and_noise_explanation_is_not_inline() {
        for manual in [false, true] {
            let ctx = egui::Context::default();
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.reconstruction = Some(Default::default());
            if manual {
                settings.color_dq_gain = Some(1.);
            }
            let before = settings.clone();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 2000.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        assert!(!processing_expanded(ui, &mut settings, None));
                    });
                },
            );
            assert_eq!(settings, before);
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(texts.iter().filter(|text| **text == "A").count(), 3);
            assert!(texts.contains(&"ColorDQ gain"));
            assert!(help::automatic("ColorDQ gain").is_some());
            assert!(texts.contains(&"Bad pixels"));
            assert!(!texts.contains(&"Min. badness"));
            assert!(!texts.iter().any(|text| text.contains("Noise preset 0")
                || *text == "Auto ColorDQ"
                || *text == "Manual cutoff"));
        }
    }

    #[test]
    fn automatic_row_disables_editing_and_toggles_to_resolved_or_fallback_value() {
        for (label, calculated) in ["ColorDQ gain", "Sharpen"].into_iter().flat_map(|label| {
            [Some(1.4_f32), None]
                .into_iter()
                .map(move |calculated| (label, calculated))
        }) {
            let ctx = egui::Context::default();
            let mut manual = None;
            let mut render = |time, events| {
                let mut changed = false;
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(400., 100.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            changed = automatic_adjustment(
                                ui,
                                label,
                                &mut manual,
                                0. ..=2.,
                                1.,
                                calculated,
                                None,
                            );
                        });
                    },
                );
                (changed, output, manual)
            };
            render(0., vec![]);
            let (_, output, _) = render(0.1, vec![]);
            let a = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "A" => {
                        Some(text.pos + text.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap();
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            // Neither dragging/double-clicking the slider nor typing/nudging the value edits Auto.
            for pos in [egui::pos2(220., 18.), egui::pos2(362., 18.)] {
                for (time, pressed) in [(1., true), (1.05, false), (1.1, true), (1.15, false)] {
                    let (changed, _, manual) = render(
                        time,
                        vec![egui::Event::PointerMoved(pos), button(pos, pressed)],
                    );
                    assert!(!changed);
                    assert_eq!(manual, None);
                }
            }
            assert!(
                !render(
                    1.2,
                    vec![
                        egui::Event::Text("2".into()),
                        arrow(egui::Key::ArrowUp, false)
                    ]
                )
                .0
            );
            render(2., vec![egui::Event::PointerMoved(a), button(a, true)]);
            let (changed, _, manual) = render(2.1, vec![button(a, false)]);
            assert!(changed);
            assert_eq!(manual, Some(calculated.unwrap_or(1.)));
            render(2.2, vec![]); // Register the now-enabled slider for hit testing.
            let pos = egui::pos2(150., 18.);
            let (changed, _, value) =
                render(3., vec![egui::Event::PointerMoved(pos), button(pos, true)]);
            assert!(changed);
            assert_ne!(value, manual);
            render(3.1, vec![button(pos, false)]);
            render(4., vec![egui::Event::PointerMoved(a), button(a, true)]);
            let (changed, _, manual) = render(4.1, vec![button(a, false)]);
            assert!(changed);
            assert_eq!(manual, None);
        }
    }

    #[test]
    fn correction_groups_align_checkboxes_and_keep_highlights_inline() {
        for width in [320., 600.] {
            let ctx = egui::Context::default();
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.highlight_threshold_ev = Some(1.25);
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 1200.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        assert!(!processing_expanded(ui, &mut settings, None));
                    });
                },
            );
            let bounds = |label: &str| {
                output
                    .shapes
                    .iter()
                    .rev()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == label => {
                            Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                        }
                        _ => None,
                    })
                    .unwrap()
            };
            let label = bounds("Highlights");
            assert!(label.left() < bounds("Bad pixels").left());
            assert_eq!(bounds("Bad pixels").left(), bounds("Speckles").left());
            assert_eq!(bounds("Bad pixels").left(), bounds("Row banding").left());
            let ordered = [
                "Tone and Color",
                "Noise and Detail",
                "Refinement",
                "Sharpen",
                "Corrections",
                "Highlights",
                "Advanced",
                "Speckles",
                "Bad pixels",
                "Row banding",
            ];
            for pair in ordered.windows(2) {
                assert!(
                    bounds(pair[0]).top() < bounds(pair[1]).top(),
                    "incorrect group order: {pair:?}"
                );
            }
            for item in [bounds("A"), bounds("1.25")] {
                assert!(
                    (item.center().y - label.center().y).abs() < 2.,
                    "control is not inline at width {width}"
                );
                assert!(
                    item.left() > label.right(),
                    "control overlaps label at width {width}"
                );
                assert!(
                    item.right() <= width - 8.,
                    "control exceeds pane at width {width}"
                );
            }
        }
    }

    #[test]
    fn tone_and_correction_highlights_have_independent_controls() {
        let ctx = egui::Context::default();
        let mut settings = Settings::new("Sunlight", "Standard");
        let mut frame = |time, events| {
            let mut changed = false;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 1200.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        changed = processing(ui, &mut settings, None);
                    });
                },
            );
            (changed, output, settings.clone())
        };
        frame(0., vec![]);
        let (_, output, _) = frame(0.1, vec![]);
        let highlights: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Highlights" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .collect();
        assert_eq!(highlights.len(), 2);
        let auto = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.job.text == "A"
                        && (text.pos.y + text.galley.size().y * 0.5 - highlights[1].y).abs()
                            < 2. =>
                {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        let event = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let tone_slider = egui::pos2(280., highlights[0].y);
        frame(
            1.,
            vec![
                egui::Event::PointerMoved(tone_slider),
                event(tone_slider, true),
            ],
        );
        let (_, _, tone) = frame(1.1, vec![event(tone_slider, false)]);
        assert_ne!(tone.tone.highlights, 0.);
        assert_eq!(tone.highlight_threshold_ev, None);
        assert!(tone.highlight_restoration);
        frame(2., vec![egui::Event::PointerMoved(auto), event(auto, true)]);
        let (changed, _, manual) = frame(2.1, vec![event(auto, false)]);
        assert!(changed);
        assert_eq!(manual.highlight_threshold_ev, Some(0.));
        assert_eq!(manual.tone, tone.tone);
        frame(2.2, vec![]);
        let correction_slider = egui::pos2(280., highlights[1].y);
        frame(
            3.,
            vec![
                egui::Event::PointerMoved(correction_slider),
                event(correction_slider, true),
            ],
        );
        let (_, _, edited) = frame(3.1, vec![event(correction_slider, false)]);
        assert!(edited.highlight_threshold_ev.unwrap() > 0.);
        assert_eq!(edited.tone, tone.tone);
        frame(
            4.,
            vec![
                egui::Event::PointerMoved(highlights[1]),
                event(highlights[1], true),
            ],
        );
        let (changed, _, disabled) = frame(4.1, vec![event(highlights[1], false)]);
        assert!(changed && !disabled.highlight_restoration);
        assert_eq!(disabled.tone, tone.tone);
        assert_eq!(
            disabled.highlight_threshold_ev,
            edited.highlight_threshold_ev
        );
    }

    #[test]
    fn highlight_row_keeps_inline_enable_auto_manual_value_and_reset() {
        for initial in [None, Some(1_f32)] {
            let ctx = egui::Context::default();
            let mut manual = initial;
            let mut enabled = true;
            let mut frame = |time, events| {
                let mut changed = false;
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(400., 100.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            changed = automatic_adjustment(
                                ui,
                                "Highlight correction",
                                &mut manual,
                                -2. ..=2.,
                                0.,
                                Some(0.),
                                Some(&mut enabled),
                            );
                        });
                    },
                );
                (changed, output, enabled, manual)
            };
            frame(0., vec![]);
            let (_, output, _, _) = frame(0.1, vec![]);
            let center = |label: &str| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == label => {
                            Some(text.pos + text.galley.size() * 0.5)
                        }
                        _ => None,
                    })
                    .unwrap()
            };
            let checkbox = center("Highlights");
            let auto = center("A");
            assert!(
                (checkbox.y - auto.y).abs() < 2.,
                "enable and Auto must be inline"
            );
            let event = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(
                1.,
                vec![egui::Event::PointerMoved(checkbox), event(checkbox, true)],
            );
            let (changed, _, enabled, value) = frame(1.1, vec![event(checkbox, false)]);
            assert!(changed && !enabled);
            assert_eq!(value, initial);
            for (i, pos) in [auto, egui::pos2(220., 18.), egui::pos2(362., 18.)]
                .into_iter()
                .enumerate()
            {
                let time = 2. + i as f64;
                assert!(!frame(time, vec![egui::Event::PointerMoved(pos), event(pos, true)]).0);
                let (changed, _, enabled, value) = frame(time + 0.1, vec![event(pos, false)]);
                assert!(!changed && !enabled);
                assert_eq!(value, initial);
            }
            frame(
                5.,
                vec![egui::Event::PointerMoved(checkbox), event(checkbox, true)],
            );
            let (changed, _, enabled, value) = frame(5.1, vec![event(checkbox, false)]);
            assert!(changed && enabled);
            assert_eq!(value, initial);
            frame(6., vec![egui::Event::PointerMoved(auto), event(auto, true)]);
            let (changed, _, _, value) = frame(6.1, vec![event(auto, false)]);
            assert!(changed);
            assert_eq!(value, if initial.is_none() { Some(0.) } else { None });
            if initial.is_some() {
                frame(7., vec![egui::Event::PointerMoved(auto), event(auto, true)]);
                assert_eq!(frame(7.1, vec![event(auto, false)]).3, Some(0.));
            }
            frame(8., vec![]);
            let slider = egui::pos2(290., 18.);
            let (changed, _, _, value) = frame(
                9.,
                vec![egui::Event::PointerMoved(slider), event(slider, true)],
            );
            assert!(changed);
            assert!(value.unwrap() > 0.);
            frame(9.1, vec![event(slider, false)]);
            let mut value = None;
            for (time, pressed) in [(10., true), (10.05, false), (10.1, true), (10.15, false)] {
                value = frame(
                    time,
                    vec![egui::Event::PointerMoved(slider), event(slider, pressed)],
                )
                .3;
            }
            assert_eq!(
                value,
                Some(0.),
                "double-click resets manual EV, not the enable switch"
            );
            let field = egui::pos2(362., 18.);
            frame(
                11.,
                vec![egui::Event::PointerMoved(field), event(field, true)],
            );
            frame(11.1, vec![event(field, false)]);
            frame(11.2, vec![egui::Event::Text("1.25".into())]);
            let enter = egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            };
            assert_eq!(frame(11.3, vec![enter]).3, Some(1.25));
            frame(
                12.,
                vec![egui::Event::PointerMoved(field), event(field, true)],
            );
            frame(12.1, vec![event(field, false)]);
            let (changed, _, _, value) = frame(12.2, vec![arrow(egui::Key::ArrowUp, false)]);
            assert!(changed);
            assert!((value.unwrap() - 1.35).abs() < 0.000001);
        }
    }

    #[test]
    fn bad_pixel_repair_checkbox_disables_and_restores_camera_selection() {
        let ctx = egui::Context::default();
        let mut settings = Settings::new("Sunlight", "Standard");
        let mut time = 0.;
        let mut render = |events| {
            time += 0.25;
            let mut changed = false;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 2000.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        changed = processing(ui, &mut settings, None);
                    });
                },
            );
            (changed, output, settings.repair, settings.color_dq_gain)
        };
        render(vec![]);
        let (_, output, repair, _) = render(vec![]);
        assert_eq!(repair, Some(RepairThreshold::Camera));
        let advanced = text_centers(&output, "Advanced")[0];
        for pressed in [true, false] {
            assert!(
                !render(vec![
                    egui::Event::PointerMoved(advanced),
                    pointer_button(advanced, pressed)
                ])
                .0
            );
        }
        let (_, output, _, _) = render(vec![egui::Event::PointerMoved(egui::Pos2::ZERO)]);
        let point = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Bad pixels" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        for expected in [None, Some(RepairThreshold::Camera)] {
            let event = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            render(vec![egui::Event::PointerMoved(point), event(true)]);
            let (changed, _, repair, color_dq) = render(vec![event(false)]);
            assert!(changed);
            assert_eq!(repair, expected);
            assert_eq!(color_dq, None);
        }
    }

    #[test]
    fn exposure_help_opens_on_label_slider_and_value_without_changing_settings() {
        for point in [
            egui::pos2(45., 18.),
            egui::pos2(220., 18.),
            egui::pos2(362., 18.),
        ] {
            let ctx = egui::Context::default();
            let mut value = 0_f32;
            let mut output = None;
            for time in [0., 1., 2.] {
                output = Some(ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 400.),
                        )),
                        time: Some(time),
                        events: vec![egui::Event::PointerMoved(point)],
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.set_width(384.);
                            assert!(!adjustment(ui, "Exposure", &mut value, -4. ..=4., 0.));
                        });
                    },
                ));
            }
            assert_eq!(value, 0.);
            assert!(output.unwrap().shapes.iter().any(|shape| matches!(
                &shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains("Makes the photo brighter or darker")
            )), "help did not open at {point:?}");
        }
    }

    #[test]
    fn highlight_popover_opens_on_all_row_parts_and_switches_without_editing() {
        for initial in [None, Some(0.8_f32)] {
            for initially_enabled in [true, false] {
                for point in [
                    egui::pos2(45., 18.),
                    egui::pos2(220., 18.),
                    egui::pos2(362., 18.),
                ] {
                    let ctx = egui::Context::default();
                    let mut manual = initial;
                    let mut enabled = initially_enabled;
                    let mut time = 0.;
                    let mut frame = |events| {
                        time += 0.5;
                        ctx.run(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(800., 1000.),
                                )),
                                time: Some(time),
                                events,
                                ..Default::default()
                            },
                            |ctx| {
                                egui::CentralPanel::default().show(ctx, |ui| {
                                    ui.allocate_ui(egui::vec2(384., 20.), |ui| {
                                        assert!(!automatic_adjustment(
                                            ui,
                                            "Highlight correction",
                                            &mut manual,
                                            -2_f32..=2.,
                                            0.,
                                            Some(0.),
                                            Some(&mut enabled)
                                        ));
                                    });
                                });
                                assert_eq!(manual, initial);
                                assert_eq!(enabled, initially_enabled);
                            },
                        )
                    };
                    let mut output = frame(vec![]);
                    for _ in 0..4 {
                        output = frame(vec![egui::Event::PointerMoved(point)]);
                    }
                    let button = output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "+0.8" =>
                            Some(text.pos + text.galley.size() * 0.5),
                        _ => None,
                    }).unwrap_or_else(|| panic!("No comparison at {point:?}, manual {initial:?}, enabled {initially_enabled}"));
                    for pressed in [true, false] {
                        frame(vec![
                            egui::Event::PointerMoved(button),
                            egui::Event::PointerButton {
                                pos: button,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            },
                        ]);
                    }
                    let output = frame(vec![egui::Event::PointerMoved(button)]);
                    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                        egui::Shape::Text(text) if text.galley.text().starts_with("Flower color is largely preserved"))),
                        "popover lost selection at {point:?}, manual {initial:?}, enabled {initially_enabled}");
                }
            }
        }
    }

    #[test]
    fn correction_comparisons_open_and_switch_without_editing_even_on_disabled_controls() {
        for (label, caption) in [
            ("Bad pixels", "Bad pixels removes the magenta patch."),
            ("Speckles", "Speckles removes the magenta dot."),
            ("Row banding", "The stripes are reduced. The grain remains."),
        ] {
            for initially_enabled in [true, false] {
                for interactive in [true, false] {
                    let ctx = egui::Context::default();
                    let mut settings = Settings::new("Sunlight", "Standard");
                    match label {
                        "Bad pixels" => {
                            settings.repair = initially_enabled.then_some(RepairThreshold::Camera)
                        }
                        "Speckles" => settings.despeckle = initially_enabled,
                        "Row banding" => settings.row_correction = initially_enabled,
                        _ => unreachable!(),
                    }
                    let initial = settings.clone();
                    let mut time = 0.;
                    let mut frame = |events, interactive| {
                        // egui needs closely spaced samples to recognize movement
                        // after the disclosure click before allowing hover help.
                        time += 0.025;
                        ctx.run(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(1000., 1600.),
                                )),
                                time: Some(time),
                                events,
                                ..Default::default()
                            },
                            |ctx| {
                                egui::CentralPanel::default().show(ctx, |ui| {
                                    ui.set_width(384.);
                                    ui.add_enabled_ui(interactive, |ui| {
                                        assert!(!processing(ui, &mut settings, None));
                                    });
                                });
                                assert_eq!(settings, initial);
                            },
                        )
                    };
                    frame(vec![], true);
                    let output = frame(vec![], true);
                    let advanced = text_centers(&output, "Advanced")[0];
                    for pressed in [true, false] {
                        frame(
                            vec![
                                egui::Event::PointerMoved(advanced),
                                pointer_button(advanced, pressed),
                            ],
                            true,
                        );
                    }
                    let mut output = frame(vec![], true);
                    for _ in 0..8 {
                        output = frame(vec![egui::Event::PointerMoved(egui::Pos2::ZERO)], true);
                    }
                    let point = text_centers(&output, label)[0];
                    for _ in 0..32 {
                        output = frame(vec![egui::Event::PointerMoved(point)], interactive);
                    }
                    let button = *text_centers(&output, "On").first().unwrap_or_else(|| {
                        panic!("No {label} comparison, enabled {initially_enabled}, interactive {interactive}")
                    });
                    for pressed in [true, false] {
                        frame(
                            vec![
                                egui::Event::PointerMoved(button),
                                pointer_button(button, pressed),
                            ],
                            interactive,
                        );
                    }
                    let output = frame(vec![egui::Event::PointerMoved(button)], interactive);
                    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                        egui::Shape::Text(text) if text.galley.text() == caption)),
                        "{label} comparison lost selection, enabled {initially_enabled}, interactive {interactive}");
                }
            }
        }
    }

    #[test]
    fn correction_checkboxes_start_enabled_and_toggle_independently() {
        for label in ["Bad pixels", "Highlights", "Speckles", "Row banding"] {
            let ctx = egui::Context::default();
            let mut settings = Settings::new("Sunlight", "Standard");
            let mut time = 0.;
            let mut render = |events| {
                time += 0.25;
                let mut changed = false;
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 2000.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            changed = processing(ui, &mut settings, None);
                        });
                    },
                );
                (
                    changed,
                    output,
                    settings.repair.is_some(),
                    settings.highlight_restoration,
                    settings.despeckle,
                    settings.row_correction,
                )
            };
            let (_, output, repair, highlights, despeckle, rows) = render(vec![]);
            assert!(repair && highlights && despeckle && rows);
            if label != "Highlights" {
                let advanced = text_centers(&output, "Advanced")[0];
                for pressed in [true, false] {
                    assert!(
                        !render(vec![
                            egui::Event::PointerMoved(advanced),
                            pointer_button(advanced, pressed)
                        ])
                        .0
                    );
                }
            }
            let (_, output, _, _, _, _) = render(vec![egui::Event::PointerMoved(egui::Pos2::ZERO)]);
            let point = output
                .shapes
                .iter()
                .rev()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => {
                        Some(t.pos + t.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap();
            let event = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            render(vec![egui::Event::PointerMoved(point), event(true)]);
            let (changed, _, repair, highlights, despeckle, rows) = render(vec![event(false)]);
            assert!(changed);
            assert_eq!(
                (repair, highlights, despeckle, rows),
                match label {
                    "Speckles" => (true, true, false, true),
                    "Highlights" => (true, false, true, true),
                    "Row banding" => (true, true, true, false),
                    _ => (false, true, true, true),
                }
            );
        }
    }

    #[test]
    fn noise_and_detail_checkbox_enables_shared_defaults_and_requests_rerender() {
        let ctx = egui::Context::default();
        let mut settings = Settings::new("Sunlight", "Standard");
        let mut frame = |time, events| {
            let mut changed = false;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 1000.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        changed = processing(ui, &mut settings, None);
                    });
                },
            );
            (changed, output, settings.reconstruction)
        };
        let (changed, output, reconstruction) = frame(0., vec![]);
        assert!(reconstruction.is_none());
        assert!(!changed);
        let point = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Noise and Detail" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        for (time, enabled) in [(1., true), (2., false)] {
            let event = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            assert!(!frame(time, vec![egui::Event::PointerMoved(point), event(true)]).0);
            let (changed, _, controls) = frame(time + 0.1, vec![event(false)]);
            assert!(changed);
            assert_eq!(controls, enabled.then(Default::default));
        }
        assert!(settings.reconstruction.is_none());
        assert_eq!(settings.exposure, 0.);
    }

    fn nudge_frame<T: egui::emath::Numeric>(
        ctx: &egui::Context,
        value: &mut T,
        range: RangeInclusive<T>,
        step: Option<f64>,
        time: f64,
        events: Vec<egui::Event>,
    ) -> bool {
        let mut changed = false;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400., 100.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    changed = if let Some(step) = step {
                        adjustment_with_nudge(
                            ui,
                            "Value",
                            value,
                            range.clone(),
                            T::from_f64(1.),
                            step,
                        )
                    } else {
                        adjustment(ui, "Value", value, range.clone(), T::from_f64(1.))
                    };
                });
            },
        );
        changed
    }
    fn arrow(key: egui::Key, repeat: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers: Default::default(),
        }
    }
    fn focus_nudge<T: egui::emath::Numeric>(
        ctx: &egui::Context,
        value: &mut T,
        range: RangeInclusive<T>,
        step: Option<f64>,
    ) {
        nudge_frame(ctx, value, range.clone(), step, 0., vec![]);
        let pos = egui::pos2(362., 18.);
        for (time, pressed) in [(0.1, true), (0.2, false)] {
            nudge_frame(
                ctx,
                value,
                range.clone(),
                step,
                time,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }
    #[test]
    fn focused_float_values_nudge_by_a_tenth_preserve_precision_and_stop_at_bounds() {
        let ctx = egui::Context::default();
        let mut value = 1.05_f32;
        assert!(!nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.,
            vec![arrow(egui::Key::ArrowUp, false)]
        ));
        assert_eq!(value, 1.05); // unfocused value stays unchanged
        focus_nudge(&ctx, &mut value, 0. ..=2., None);
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.3,
            vec![arrow(egui::Key::ArrowUp, false)]
        ));
        assert!((value - 1.15).abs() < 0.00001);
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.4,
            vec![arrow(egui::Key::ArrowDown, false)]
        ));
        assert!((value - 1.05).abs() < 0.00001);
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.5,
            vec![arrow(egui::Key::ArrowDown, true)]
        ));
        assert!((value - 0.95).abs() < 0.00001);
        value = 2.;
        assert!(!nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.6,
            vec![arrow(egui::Key::ArrowUp, true)]
        ));
        assert_eq!(value, 2.);
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.7,
            vec![arrow(egui::Key::ArrowDown, false)]
        ));
        assert_eq!(value, 1.9);
        value = 0.;
        assert!(!nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.8,
            vec![arrow(egui::Key::ArrowDown, true)]
        ));
        assert_eq!(value, 0.);
    }
    #[test]
    fn parameter_nudge_overrides_and_integer_steps_are_respected() {
        let ctx = egui::Context::default();
        let mut value = 1_f32;
        focus_nudge(&ctx, &mut value, 0. ..=2., Some(0.25));
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            Some(0.25),
            0.3,
            vec![arrow(egui::Key::ArrowUp, false)]
        ));
        assert_eq!(value, 1.25);
        let ctx = egui::Context::default();
        let mut value = 1_f32;
        focus_nudge(&ctx, &mut value, 0. ..=2., Some(0.001));
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            Some(0.001),
            0.3,
            vec![arrow(egui::Key::ArrowUp, false)]
        ));
        assert!((value - 1.001).abs() < 0.000001);
        let ctx = egui::Context::default();
        let mut value = 8_u32;
        focus_nudge(&ctx, &mut value, 0..=15, None);
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0..=15,
            None,
            0.3,
            vec![arrow(egui::Key::ArrowUp, false)]
        ));
        assert_eq!(value, 9);
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0..=15,
            None,
            0.4,
            vec![arrow(egui::Key::ArrowDown, false)]
        ));
        assert_eq!(value, 8);
    }

    #[test]
    fn banding_slider_snaps_to_five_choices_and_double_click_resets() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400., 100.),
            )),
            ..Default::default()
        };
        let mut value = 2_u32;
        let mut frame = |time, events| {
            let before = value;
            let mut event = input.clone();
            event.time = Some(time);
            event.events = events;
            let mut changed = false;
            let _ = ctx.run(event, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    changed = adjustment(ui, "Banding noise", &mut value, 0..=4, 2);
                });
            });
            assert!(value <= 4);
            assert_eq!(changed, value != before);
            value
        };
        frame(0., vec![]);
        let mut seen = std::collections::BTreeSet::new();
        for (i, x) in [112., 140., 170., 195., 215., 240., 260., 285., 310.]
            .into_iter()
            .enumerate()
        {
            let point = egui::pos2(x, 18.);
            for (offset, pressed) in [(0., true), (0.1, false)] {
                let current = frame(
                    1. + i as f64 * 0.5 + offset,
                    vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                );
                seen.insert(current);
            }
        }
        assert_eq!(seen, (0..=4).collect());
        let point = egui::pos2(310., 18.);
        let mut reset = 4;
        for (time, pressed) in [(10., true), (10.05, false), (10.1, true), (10.15, false)] {
            reset = frame(
                time,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert_eq!(reset, 2);
    }

    #[test]
    fn slider_drag_reports_value_changes_for_rerender() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400., 100.),
            )),
            ..Default::default()
        };
        let mut value = 0_f32;
        let _ = ctx.run(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                adjustment(ui, "Exposure", &mut value, -4. ..=4., 0.);
            });
        });
        for (time, events) in [
            (
                1.,
                vec![
                    egui::Event::PointerMoved(egui::pos2(290., 18.)),
                    egui::Event::PointerButton {
                        pos: egui::pos2(290., 18.),
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
            ),
            (1.1, vec![egui::Event::PointerMoved(egui::pos2(260., 18.))]),
        ] {
            let before = value;
            let mut input = input.clone();
            input.time = Some(time);
            input.events = events;
            let mut changed = false;
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    changed |= adjustment(ui, "Exposure", &mut value, -4. ..=4., 0.);
                });
            });
            assert_ne!(value, before, "pointer input must actually move the slider");
            assert!(changed, "a changed value must request a rerender");
        }
    }

    #[test]
    fn double_click_resets_the_slider_and_value_remains_editable() {
        let ctx = egui::Context::default();
        let mut value = 0.7_f32;
        let mut slider = egui::Rect::NOTHING;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400., 100.),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                adjustment(ui, "Exposure", &mut value, -4. ..=4., 0.);
            });
        });
        // The slider follows the 92-point label in this row, before the value field.
        slider.min = egui::pos2(108., 8.);
        slider.max = egui::pos2(320., 28.);
        let point = slider.center();
        for (time, pressed) in [(1., true), (1.05, false), (1.1, true), (1.15, false)] {
            let mut event = input.clone();
            event.time = Some(time);
            event.events = vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ];
            let _ = ctx.run(event, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    adjustment(ui, "Exposure", &mut value, -4. ..=4., 0.);
                });
            });
        }
        assert_eq!(value, 0.);
        // Click the separate value field, then type a numeric value and accept it.
        let field = egui::pos2(362., 18.);
        for (time, events) in [
            (
                2.,
                vec![
                    egui::Event::PointerMoved(field),
                    egui::Event::PointerButton {
                        pos: field,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
            ),
            (
                2.1,
                vec![egui::Event::PointerButton {
                    pos: field,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            ),
            (2.2, vec![egui::Event::Text("1.25".into())]),
            (
                2.3,
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            ),
        ] {
            let mut event = input.clone();
            event.time = Some(time);
            event.events = events;
            let _ = ctx.run(event, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    adjustment(ui, "Exposure", &mut value, -4. ..=4., 0.);
                });
            });
        }
        assert_eq!(value, 1.25);
    }
}
