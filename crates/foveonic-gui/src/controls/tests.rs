//! Checks family panel composition, shared widgets, and editing interactions.

use super::common::{adjustment, adjustment_with_nudge, automatic_adjustment};
use super::merrill::{highlight_choice, select_highlight, HIGHLIGHT_METHODS};
use super::*;
use crate::help;
use foveonic::render::{HighlightMode, Rendering, RepairThreshold};
use std::ops::RangeInclusive;

// Existing widget interaction fixtures exercise the Merrill panel while full
// metadata is absent. Capture-only and unidentified states are checked separately.
fn processing(ui: &mut egui::Ui, settings: &mut Settings, metadata: Option<&Metadata>) -> bool {
    let capture = [("CAMMODEL".into(), "SIGMA DP2 Merrill".into())].into();
    super::processing(ui, settings, metadata, Some(&capture))
}

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
        "Chroma",
        "Luma",
        "Banding",
        "Fine",
        "Size",
        "Coarse",
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
    for label in ["Color mode", "Saturation", "Noise", "Detail"] {
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
        settings.reconstruction.as_mut().unwrap().texture.fine = 0.2;
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
            "Chroma",
            "Luma",
            "Banding",
            "Fine",
            "Size",
            "Coarse",
            "ColorDQ gain",
        ] {
            assert_eq!(
                text_centers(&output, label).len(),
                usize::from(rendering == Rendering::Color) * if label == "Size" { 2 } else { 1 },
                "{rendering:?}: {label}"
            );
        }
        assert_eq!(text_centers(&output, "Noise").len(), 1);
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
            "Amount",
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
    let noise_parameter = "Mean decay";
    assert!(text_centers(&output, noise_parameter).is_empty());
    assert!(text_centers(&output, "ColorDQ gain").is_empty());
    assert_eq!(text_centers(&output, "Tone and Color").len(), 1);
    assert_eq!(text_centers(&output, "Noise").len(), 1);
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
    assert!(text_centers(&output, noise_parameter).is_empty());
    assert_eq!(text_centers(&output, "ColorDQ gain").len(), 1);
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
    assert!(text_centers(&output, noise_parameter).is_empty());
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
fn classic_choices_use_profile_catalogues_and_fixed_standard_mode() {
    use foveonic::source::PropertyList;
    let metadata = Metadata {
        processing_version: None,
        width: 2688,
        height: 1792,
        rotation: 0,
        properties: [("CAMMODEL".into(), "SIGMA DP1S".into())].into(),
        matrices: Default::default(),
        lists: [(
            "DP1_WhiteBalanceGains".into(),
            PropertyList::new(vec![
                ("Sunlight".into(), "sun".into()),
                ("Shade".into(), "shade".into()),
            ])
            .unwrap(),
        )]
        .into(),
    };
    assert_eq!(metadata.white_balance_presets(), vec!["Sunlight", "Shade"]);
    assert_eq!(metadata.color_mode_presets(), vec!["Standard"]);
    let ctx = egui::Context::default();
    let mut settings = Settings::new("Sunlight", "Standard");
    let before = settings.clone();
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            assert!(!processing(ui, &mut settings, Some(&metadata)));
        });
    });
    assert_eq!(settings, before);
    assert_eq!(text_centers(&output, "Sunlight").len(), 1);
    assert_eq!(text_centers(&output, "Standard").len(), 1);
}

#[test]
fn classic_metadata_restores_intrinsic_correction_contract() {
    for model in [
        "SIGMA SD9",
        "SIGMA SD10",
        "SIGMA SD14",
        "SIGMA SD15",
        "SIGMA DP1",
        "SIGMA DP1S",
        "SIGMA DP1X",
        "SIGMA DP2",
        "SIGMA DP2S",
        "SIGMA DP2X",
    ] {
        let metadata = Metadata {
            processing_version: None,
            width: 1,
            height: 1,
            rotation: 0,
            properties: [("CAMMODEL".into(), model.into())].into(),
            matrices: Default::default(),
            lists: Default::default(),
        };
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.row_correction = false;
        settings.highlight_restoration = false;
        settings.reconstruction = None;
        assert!(normalize_incompatible(&mut settings, Some(&metadata)));
        assert!(settings.row_correction);
        assert!(!settings.highlight_restoration);
        assert!(settings.reconstruction.is_none());
        assert!(!normalize_incompatible(&mut settings, Some(&metadata)));
    }
}

#[test]
fn camera_choices_keep_loading_layout_and_saved_values_until_metadata_arrives() {
    use foveonic::source::PropertyList;
    let metadata = Metadata {
        processing_version: None,
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
                &["White balance", "Exposure", "Sharpening"]
            } else {
                &["White balance", "Color mode", "Exposure", "Sharpening"]
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
        processing_version: None,
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
    for key in [
        "White balance",
        "Color mode",
        "Exposure",
        "Saturation",
        "Contrast",
        "Shadows",
        "Highlights",
        "Noise",
        "Detail",
        "Chroma noise",
        "Luma noise",
        "Banding noise",
        "Fine detail",
        "Fine size",
        "Coarse detail",
        "Coarse size",
        "ColorDQ gain",
        "Sharpen",
        "Bad pixels",
        "Speckles",
        "Row banding",
    ] {
        let label = match key {
            "Chroma noise" => "Chroma",
            "Luma noise" => "Luma",
            "Banding noise" => "Banding",
            "Fine detail" => "Fine",
            "Coarse detail" => "Coarse",
            "Fine size" | "Coarse size" => "Size",
            "Sharpen" => "Amount",
            _ => key,
        };
        assert!(
            output.shapes.iter().any(|shape| matches!(
                &shape.shape, egui::Shape::Text(text) if text.galley.job.text == label
            )),
            "control missing: {label}"
        );
        assert!(help::editing(key).is_some(), "help missing: {label}");
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
            let y = text_centers(&output, "Threshold")[0].y;
            assert_eq!(
                text_centers(&output, "A")
                    .iter()
                    .filter(|position| (position.y - y).abs() < 2.)
                    .count(),
                if mode == HighlightMode::Classic { 1 } else { 0 }
            );
            assert!(
                text_centers(&output, "Highlights").last().unwrap().y
                    < text_centers(&output, "Threshold")[0].y
            );
            assert!(shown("Threshold"));
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
        let y = text_centers(&output, "Threshold")[0].y;
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
                    egui::Shape::Text(text) if text.galley.job.text == label => Some(text.pos.y),
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
                        egui::Shape::Text(text) if text.galley.job.text == "Noise"
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
            let advanced_headers = output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(
                        &shape.shape, egui::Shape::Text(text) if text.galley.job.text == "Advanced"
                    )
                })
                .count();
            assert_eq!(advanced_headers, 2);
            if reconstruction {
                let y = position("Advanced").unwrap();
                assert!(y > position("Banding").unwrap());
                assert!(y < position("Detail").unwrap());
                assert!(y < position("Sharpening").unwrap());
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
                            super::common::RowControls::default(),
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
        let label = bounds("Threshold");
        assert!(label.left() < bounds("Bad pixels").left());
        assert_eq!(bounds("Bad pixels").left(), bounds("Speckles").left());
        assert_eq!(bounds("Bad pixels").left(), bounds("Row banding").left());
        let ordered = [
            "Tone and Color",
            "Noise",
            "Sharpening",
            "Amount",
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
    settings.photographic.tone_method = foveonic::render::PhotographicMethod::Universal;
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
                    && (text.pos.y + text.galley.size().y * 0.5
                        - text_centers(&output, "Threshold")[0].y)
                        .abs()
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
    assert_ne!(tone.tonal_ranges.highlights, 0.);
    assert_eq!(tone.tone.highlights, 0.);
    assert_eq!(tone.highlight_threshold_ev, None);
    assert!(tone.highlight_restoration);
    frame(2., vec![egui::Event::PointerMoved(auto), event(auto, true)]);
    let (changed, _, manual) = frame(2.1, vec![event(auto, false)]);
    assert!(changed);
    assert_eq!(manual.highlight_threshold_ev, Some(0.));
    assert_eq!(manual.tone, tone.tone);
    frame(2.2, vec![]);
    let correction_slider = egui::pos2(280., text_centers(&output, "Threshold")[0].y);
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
    assert_eq!(edited.tonal_ranges, tone.tonal_ranges);
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
fn tone_methods_show_only_their_active_controls_and_preserve_edits() {
    use foveonic::render::PhotographicMethod;
    let ctx = egui::Context::default();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.tone.shadows = 0.4;
    settings.tone.highlights = -0.3;
    settings.tonal_ranges.shadows = 0.6;
    settings.photographic.local_contrast = 0.2;
    for method in [
        PhotographicMethod::Reference,
        PhotographicMethod::Universal,
        PhotographicMethod::Reference,
    ] {
        settings.photographic.tone_method = method;
        let before = settings.clone();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400., 900.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert!(!common::tone(ui, &mut settings, false));
                });
            },
        );
        for label in [
            "Exposure",
            "Saturation",
            "Contrast",
            "Shadows",
            "Highlights",
        ] {
            assert_eq!(text_centers(&output, label).len(), 1, "{method:?}: {label}");
        }
        for label in ["Blacks", "Whites", "Local contrast"] {
            assert_eq!(
                !text_centers(&output, label).is_empty(),
                method == PhotographicMethod::Universal,
                "{method:?}: {label}"
            );
        }
        assert!(text_centers(&output, "Camera curve").is_empty());
        let badge = if method == PhotographicMethod::Reference {
            "DNG | TIFF"
        } else {
            "TIFF"
        };
        let header = text_centers(&output, "Tone and Color")[0];
        let badge = text_centers(&output, badge);
        assert_eq!(badge.len(), 1);
        assert!(badge[0].x > header.x);
        assert!((badge[0].y - header.y).abs() < 2.);
        assert_eq!(settings, before);
    }
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
                            super::common::RowControls {
                                enabled: Some(&mut enabled),
                                ..Default::default()
                            },
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
        let checkbox = center("Threshold");
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
    let advanced = *text_centers(&output, "Advanced").last().unwrap();
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
                                        super::common::RowControls {
                                            enabled: Some(&mut enabled),
                                            ..Default::default()
                                        }
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
                let advanced = *text_centers(&output, "Advanced").last().unwrap();
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
                assert!(
                    output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.text() == caption)),
                    "{label} comparison lost selection, enabled {initially_enabled}, interactive {interactive}"
                );
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
            let advanced = *text_centers(&output, "Advanced").last().unwrap();
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
fn noise_checkbox_enables_only_noise_and_preserves_inactive_settings() {
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
            egui::Shape::Text(text) if text.galley.job.text == "Noise" => {
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
        assert_eq!(
            controls,
            Some(foveonic::noise::Controls {
                noise_enabled: enabled,
                detail_enabled: false,
                ..Default::default()
            })
        );
    }
    assert!(settings.reconstruction.is_some());
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
                    adjustment_with_nudge(ui, "Value", value, range.clone(), T::from_f64(1.), step)
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
    for (i, x) in [112., 140., 170., 195., 215., 240., 260., 285., 310., 330.]
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
    let point = egui::pos2(330., 18.);
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
    // Click inside the slider, between the font-sized label and the value field.
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

fn family_metadata(model: &str) -> Metadata {
    Metadata {
        processing_version: None,
        width: 2688,
        height: 1792,
        rotation: 0,
        properties: [("CAMMODEL".into(), model.into())].into(),
        matrices: Default::default(),
        lists: Default::default(),
    }
}

#[test]
fn family_panels_show_supported_controls_without_mutating_edits() {
    for model in [
        "SIGMA SD14",
        "SIGMA DP1S",
        "SIGMA DP2X",
        "SIGMA DP2 Merrill",
    ] {
        let metadata = family_metadata(model);
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        settings.exposure = 0.7;
        let before = settings.clone();
        let context = egui::Context::default();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 2400.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert!(!processing_expanded(ui, &mut settings, Some(&metadata)));
                });
            },
        );
        assert_eq!(settings, before);
        let classic = metadata.processing_family().unwrap() == ProcessingFamily::Classic;
        for label in [
            "White balance",
            "Color mode",
            "Exposure",
            if classic { "Noise and Detail" } else { "Noise" },
            "Amount",
            "Speckles",
            "Bad pixels",
        ] {
            assert!(
                !text_centers(&output, label).is_empty(),
                "{model}: missing {label}"
            );
        }
        for label in [
            "Chroma",
            "Luma",
            "Banding",
            "Fine",
            "Size",
            "Coarse",
            "Size",
            "Threshold",
            "Row banding",
        ] {
            assert_eq!(
                text_centers(&output, label).is_empty(),
                classic,
                "{model}: {label}"
            );
        }
        assert_eq!(
            text_centers(&output, "Highlights").len(),
            2,
            "tone and correction remain separate"
        );
    }
}

#[test]
fn classic_reconstruction_toggle_preserves_other_edits() {
    let metadata = family_metadata("SIGMA SD14");
    let context = egui::Context::default();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    settings.exposure = 0.7;
    settings.sharpen_amount = Some(0.4);
    let initial = settings.clone();
    let mut frame = |time, events| {
        let mut changed = false;
        let output = context.run(
            egui::RawInput {
                time: Some(time),
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 1600.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    changed = processing(ui, &mut settings, Some(&metadata));
                });
            },
        );
        (changed, output, settings.clone())
    };
    let (_, output, _) = frame(0., vec![]);
    let position = text_centers(&output, "Noise and Detail")[0];
    frame(
        1.,
        vec![
            egui::Event::PointerMoved(position),
            pointer_button(position, true),
        ],
    );
    let (changed, _, disabled) = frame(1.1, vec![pointer_button(position, false)]);
    assert!(changed);
    let mut expected = initial.clone();
    expected.reconstruction = None;
    assert_eq!(disabled, expected);
    frame(
        2.,
        vec![
            egui::Event::PointerMoved(position),
            pointer_button(position, true),
        ],
    );
    let (changed, _, enabled) = frame(2.1, vec![pointer_button(position, false)]);
    assert!(changed);
    assert_eq!(enabled, initial);
}

#[test]
fn family_metadata_changes_normalize_only_incompatible_settings() {
    let classic = family_metadata("SIGMA SD14");
    let merrill = family_metadata("SIGMA DP2 Merrill");
    let mut settings = Settings::new("Shade", "Standard");
    settings.rendering = Rendering::Monochrome;
    settings.exposure = 1.2;
    settings.highlight_restoration = false;
    settings.reconstruction = None;
    let saved = settings.clone();
    assert!(!normalize_incompatible(&mut settings, None));
    assert_eq!(settings, saved);
    assert!(!normalize_incompatible(&mut settings, Some(&merrill)));
    assert!(!normalize_incompatible(&mut settings, Some(&classic)));
    assert_eq!(settings.rendering, Rendering::Monochrome);
    assert_eq!(settings.exposure, 1.2);
    assert_eq!(settings.white_balance, "Shade");
    assert!(!settings.highlight_restoration && settings.reconstruction.is_none());
    let compatible = settings.clone();
    assert!(!normalize_incompatible(&mut settings, Some(&merrill)));
    assert_eq!(settings, compatible);
}

#[test]
fn capture_properties_select_classic_before_a_render_completes() {
    let capture = [("CAMMODEL".into(), "SIGMA SD14".into())].into();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.rendering = Rendering::Monochrome;
    assert!(!normalize_with_capture(&mut settings, None, Some(&capture)));
    assert_eq!(settings.rendering, Rendering::Monochrome);
    for capture in [None, Some(&capture)] {
        let ctx = egui::Context::default();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!super::processing(ui, &mut settings, None, capture));
            });
        });
        assert_eq!(
            text_centers(&output, "Rendering").len(),
            usize::from(capture.is_some())
        );
        assert_eq!(
            text_centers(&output, "Monochrome").len(),
            usize::from(capture.is_some())
        );
        assert_eq!(
            text_centers(&output, "Noise and Detail").len(),
            usize::from(capture.is_some())
        );
    }
}

#[test]
fn classic_monochrome_keeps_color_modes_and_preserves_disabled_preferences() {
    use foveonic::source::PropertyList;
    let mut metadata = family_metadata("SIGMA DP2");
    metadata.lists.insert(
        "WhiteBalanceGains".into(),
        PropertyList::new(vec![
            ("Sunlight".into(), "sun".into()),
            ("Shade".into(), "shade".into()),
        ])
        .unwrap(),
    );
    metadata.lists.insert(
        "ColorModeCompensations".into(),
        PropertyList::new(vec![
            ("Standard".into(), "standard".into()),
            ("Vivid".into(), "vivid".into()),
        ])
        .unwrap(),
    );
    let ctx = egui::Context::default();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.rendering = Rendering::Monochrome;
    let initial = settings.clone();
    let mut frame = |events| {
        let mut changed = false;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 2000.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    changed = processing(ui, &mut settings, Some(&metadata));
                });
            },
        );
        (changed, output, settings.clone())
    };
    let (_, output, _) = frame(vec![]);
    for hidden in ["Saturation", "Mono noise", "Mono banding", "Sensor mix"] {
        assert!(text_centers(&output, hidden).is_empty());
    }
    for visible in [
        "Monochrome",
        "White balance",
        "Color mode",
        "Noise and Detail",
    ] {
        assert_eq!(text_centers(&output, visible).len(), 1);
    }
    let wb = text_centers(&output, "Sunlight")[0];
    let highlight = *text_centers(&output, "Highlights").last().unwrap();
    for position in [wb, highlight] {
        frame(vec![
            egui::Event::PointerMoved(position),
            pointer_button(position, true),
        ]);
        let (changed, output, current) = frame(vec![pointer_button(position, false)]);
        assert!(!changed);
        assert_eq!(current, initial);
        assert!(text_centers(&output, "Shade").is_empty());
    }
    let (_, output, _) = frame(vec![]);
    let mode = text_centers(&output, "Standard")[0];
    frame(vec![
        egui::Event::PointerMoved(mode),
        pointer_button(mode, true),
    ]);
    frame(vec![pointer_button(mode, false)]);
    let (_, output, _) = frame(vec![]);
    let vivid = text_centers(&output, "Vivid")[0];
    frame(vec![
        egui::Event::PointerMoved(vivid),
        pointer_button(vivid, true),
    ]);
    let (changed, _, current) = frame(vec![pointer_button(vivid, false)]);
    assert!(changed);
    let mut expected = initial;
    expected.color_mode = "Vivid".into();
    assert_eq!(current, expected);
}

#[test]
fn unsupported_tiger_geometry_keeps_inspection_without_photographic_controls() {
    let ctx = egui::Context::default();
    let mut settings = Settings::new("Sunlight", "Standard");
    let expected = settings.clone();
    let capture: std::collections::BTreeMap<String, String> =
        [("CAMMODEL".into(), "SIGMA sd Quattro H".into())].into();
    let metadata = Metadata {
        processing_version: Some([4, 0, 1, 0]),
        width: 0,
        height: 0,
        rotation: 0,
        properties: capture.clone(),
        matrices: Default::default(),
        lists: Default::default(),
    };
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            assert!(!super::processing(
                ui,
                &mut settings,
                Some(&metadata),
                Some(&capture)
            ));
        });
    });
    assert_eq!(settings, expected);
    assert_eq!(
        text_centers(&output, "Rendering for this camera is not available yet.").len(),
        1
    );
    for label in [
        "White balance",
        "Color mode",
        "Exposure",
        "Noise and Detail",
        "Monochrome",
    ] {
        assert!(text_centers(&output, label).is_empty());
    }
}

#[test]
fn ordinary_quattro_controls_arrive_with_capture_before_processing_metadata() {
    for model in [
        "SIGMA dp0 Quattro",
        "SIGMA dp1 Quattro",
        "SIGMA dp2 Quattro",
        "SIGMA dp3 Quattro",
        "SIGMA sd Quattro",
        "SIGMA sd Quattro H",
    ] {
        for rendering in [Rendering::Color, Rendering::Monochrome] {
            let ctx = egui::Context::default();
            let capture = [("CAMMODEL".into(), model.into())].into();
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.rendering = rendering;
            let before = settings.clone();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 1800.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        assert!(!super::processing(ui, &mut settings, None, Some(&capture)));
                    });
                },
            );
            assert_eq!(settings, before);
            for label in [
                "Color",
                "Monochrome",
                "Exposure",
                "Smooth / Crisp",
                "Amount",
                "Highlight recovery",
            ] {
                assert_eq!(text_centers(&output, label).len(), 1, "{model}: {label}");
            }
            assert!(
                text_centers(&output, "Rendering for this camera is not available yet.").is_empty()
            );
            let labels: &[&str] = if rendering == Rendering::Monochrome {
                &["Color Mix", "Mono noise"]
            } else {
                &["Chroma noise", "Luma noise"]
            };
            for label in labels {
                assert_eq!(text_centers(&output, label).len(), 1, "{model}: {label}");
            }
        }
    }
}

#[test]
fn quattro_panel_shows_white_balance_and_tone_without_unsupported_controls() {
    use foveonic::source::{Matrix, MatrixEncoding, PropertyList};
    let mut metadata = family_metadata("SIGMA sd Quattro");
    metadata.processing_version = Some([3, 1, 2, 0]);
    metadata.width = 5888;
    metadata.height = 3776;
    for (name, value) in [("CAMERAID", 40.), ("CaptureISO", 100.)] {
        metadata.matrices.insert(
            name.into(),
            Matrix {
                encoding: MatrixEncoding::Float32,
                representation: "test",
                dimensions: vec![1],
                values: vec![value],
            },
        );
    }
    metadata.lists.insert(
        "WhiteBalanceGains".into(),
        PropertyList::new(vec![
            ("Sunlight".into(), "sun".into()),
            ("Shade".into(), "shade".into()),
        ])
        .unwrap(),
    );
    let ctx = egui::Context::default();
    let mut settings = Settings::new("Shade", "Standard");
    settings.exposure = 0.7;
    settings.tone.shadows = -0.4;
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
                assert!(!super::processing(ui, &mut settings, Some(&metadata), None));
            });
        },
    );
    assert_eq!(settings, before);
    for label in [
        "White balance",
        "Color mode",
        "Exposure",
        "Contrast",
        "Shadows",
        "Highlights",
        "Saturation",
        "Noise and Detail",
        "Chroma noise",
        "Luma noise",
        "Smooth / Crisp",
        "Amount",
        "Highlight recovery",
        "Monochrome",
    ] {
        assert_eq!(text_centers(&output, label).len(), 1, "{label}");
    }
    assert!(text_centers(&output, "Banding noise").is_empty());

    settings.rendering = Rendering::Monochrome;
    settings.monochrome_smoothness = Some(1. / 6.);
    settings.reconstruction = Some(foveonic::noise::Controls {
        chroma: 4,
        detail: 1.,
        ..Default::default()
    });
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
                assert!(!super::processing(ui, &mut settings, Some(&metadata), None));
            });
        },
    );
    assert_eq!(settings, before);
    for label in ["Color Mix", "Mono noise", "Smooth / Crisp", "Amount"] {
        assert_eq!(text_centers(&output, label).len(), 1, "{label}");
    }
    for label in ["Saturation", "Chroma noise", "Luma noise", "Mono banding"] {
        assert!(text_centers(&output, label).is_empty(), "{label}");
    }
}

#[test]
fn quattro_saved_slider_values_are_stable_when_full_metadata_arrives() {
    use foveonic::source::{Matrix, MatrixEncoding};
    let capture: std::collections::BTreeMap<String, String> = [
        ("CAMMODEL", "SIGMA sd Quattro"),
        ("X3F_ADJUSTMENT_CHROMA_NOISE", "0.75"),
        ("X3F_ADJUSTMENT_LUMINANCE_NOISE", "0.25"),
        ("X3F_ADJUSTMENT_SMOOTHNESS", "0.6666667"),
        ("X3F_ADJUSTMENT_MONO_SMOOTHNESS", "0.16666667"),
        ("X3F_ADJUSTMENT_SHARPNESS", "-1"),
        ("X3F_ADJUSTMENT_MONO_SHARPNESS", "0.8"),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value.into()))
    .collect();
    let mut metadata = crate::capture::processing_metadata(capture.clone());
    metadata.processing_version = Some([3, 1, 2, 0]);
    metadata.width = 5888;
    metadata.height = 3776;
    for (name, value) in [("CAMERAID", 40.), ("CaptureISO", 100.)] {
        metadata.matrices.insert(
            name.into(),
            Matrix {
                encoding: MatrixEncoding::Float32,
                representation: "test",
                dimensions: vec![1],
                values: vec![value],
            },
        );
    }
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.rendering = rendering;
        settings.sharpen_amount = None;
        let before = settings.clone();
        let mut values = Vec::new();
        for full in [None, Some(&metadata)] {
            let ctx = egui::Context::default();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 1800.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        assert!(!super::processing(ui, &mut settings, full, Some(&capture)));
                    });
                },
            );
            values.push(
                output
                    .shapes
                    .iter()
                    .filter_map(|shape| {
                        if let egui::Shape::Text(text) = &shape.shape {
                            let label = &text.galley.job.text;
                            label.parse::<f64>().ok().map(|_| label.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>(),
            );
            assert_eq!(settings, before);
        }
        assert!(!values[0].is_empty());
        assert_eq!(values[0], values[1], "{rendering:?}");
    }
}

#[test]
fn merrill_texture_controls_are_always_visible_and_keep_their_values() {
    let ctx = egui::Context::default();
    crate::widgets::install_style(&ctx);
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(foveonic::noise::Controls {
        texture: foveonic::noise::TextureControls {
            fine: 0.8,
            coarse: 0.2,
            fine_size: 0.75,
            coarse_size: 1.75,
        },
        ..Default::default()
    });
    let before = settings.clone();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(440., 1400.),
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
    for label in ["Fine", "Size", "Coarse"] {
        assert!(!text_centers(&output, label).is_empty(), "{label}");
    }
    for label in [
        "Separate texture (experimental)",
        "Mean decay",
        "Base filter blend",
    ] {
        assert!(text_centers(&output, label).is_empty(), "{label}");
    }
}

#[test]
fn label_alignment_preserves_controls_across_fonts_and_scales() {
    for font_size in [11., 13.] {
        for density in [1., 2.] {
            let ctx = egui::Context::default();
            crate::widgets::install_style(&ctx);
            ctx.set_pixels_per_point(density);
            ctx.style_mut(|style| {
                style
                    .text_styles
                    .insert(egui::TextStyle::Body, egui::FontId::proportional(font_size));
            });
            let mut output = None;
            for time in [0., 0.1] {
                output = Some(ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 300.),
                        )),
                        time: Some(time),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            for row in 0..3 {
                                ui.push_id(row, |ui| {
                                    let mut value = 1_f32;
                                    let changed = if row == 1 {
                                        super::common::LabelColumn::new(ui, &["xxx"], &["xxx"])
                                            .indented()
                                            .adjustment(
                                                ui,
                                                ("xxx", "xxx"),
                                                &mut value,
                                                0. ..=2.,
                                                1.,
                                            )
                                    } else {
                                        super::common::LabelColumn::new(ui, &["xxx"], &["xxx"])
                                            .adjustment(
                                                ui,
                                                if row == 2 {
                                                    ("ggg", "ggg")
                                                } else {
                                                    ("xxx", "xxx")
                                                },
                                                &mut value,
                                                0. ..=2.,
                                                1.,
                                            )
                                    };
                                    assert!(!changed);
                                    assert_eq!(value, 1.);
                                });
                            }
                        });
                    },
                ));
            }
            let output = output.unwrap();
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) if matches!(t.galley.text(), "xxx" | "ggg") => Some(t),
                    _ => None,
                })
                .collect();
            let knobs: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Circle(c) => Some(c.center),
                    _ => None,
                })
                .collect();
            let values: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == "1.00" => Some(t),
                    _ => None,
                })
                .collect();
            assert_eq!(labels.len(), 3);
            assert_eq!(knobs.len(), 3);
            assert_eq!(values.len(), 3);
            let ink_left =
                |t: &egui::epaint::TextShape| t.pos.x + t.galley.rows[0].visuals.mesh_bounds.left();
            let pipe = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::LineSegment { points, .. } if points[0].x == points[1].x => {
                        Some(points[0].x)
                    }
                    _ => None,
                })
                .unwrap();
            let original_child = ink_left(labels[0]) + ctx.style().spacing.indent;
            let expected = pipe + (original_child - pipe) * 0.5;
            assert!((ink_left(labels[1]) - expected).abs() <= 1. / density);
            for row in 0..3 {
                assert!(
                    (knobs[row].x - knobs[0].x).abs() < 0.1,
                    "child shifted its slider: font {font_size}, density {density}, knobs {knobs:?}"
                );
                let text = values[row];
                let middle = text.pos + text.galley.size() * 0.5;
                let input = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Rect(r)
                            if r.rect.contains(middle) && r.rect.width() < 100. =>
                        {
                            Some(r.rect)
                        }
                        _ => None,
                    })
                    .min_by(|a, b| a.area().total_cmp(&b.area()))
                    .unwrap();
                assert!(
                    (input.center().y - knobs[row].y).abs() < 0.1,
                    "font {font_size} density {density}: input {input:?}, slider {:?}",
                    knobs[row]
                );
                let baseline = labels[row].pos.y + labels[row].galley.rows[0].glyphs[0].pos.y;
                let reference_baseline = labels[0].pos.y + labels[0].galley.rows[0].glyphs[0].pos.y;
                assert!(
                    ((baseline - knobs[row].y) - (reference_baseline - knobs[0].y)).abs() < 0.1,
                    "descender changed the baseline"
                );
                if row < 2 {
                    let body_center = labels[row].pos.y
                        + labels[row].galley.rows[0].visuals.mesh_bounds.center().y;
                    assert!(
                        (body_center - knobs[row].y).abs() <= 0.5 / density,
                        "font {font_size} density {density}: lowercase body {body_center}, slider {}",
                        knobs[row].y
                    );
                }
            }
        }
    }
}

#[test]
fn independent_noise_and_detail_toggles_preserve_all_inactive_values() {
    let ctx = egui::Context::default();
    crate::widgets::install_style(&ctx);
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(foveonic::noise::Controls {
        chroma: 4,
        luminance: 1,
        banding: 3,
        texture: foveonic::noise::TextureControls {
            fine: 1.3,
            coarse: 0.4,
            fine_size: 0.8,
            coarse_size: 1.7,
        },
        ..Default::default()
    });
    let original = settings.clone();
    let mut frame = |time, events| {
        let mut changed = false;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400., 1800.),
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
    let (_, mut output, _) = frame(0., vec![]);
    for (index, label) in ["Detail", "Noise"].into_iter().enumerate() {
        let point = text_centers(&output, label)[0];
        for (iteration, enabled) in [false, true].into_iter().enumerate() {
            let time = 1. + (index * 4 + iteration * 2) as f64;
            frame(
                time,
                vec![
                    egui::Event::PointerMoved(point),
                    pointer_button(point, true),
                ],
            );
            let (changed, next, current) = frame(time + 0.1, vec![pointer_button(point, false)]);
            assert!(changed);
            let mut expected = original.clone();
            let controls = expected.reconstruction.as_mut().unwrap();
            if label == "Detail" {
                controls.detail_enabled = enabled;
            } else {
                controls.noise_enabled = enabled;
            }
            assert_eq!(current, expected);
            assert_eq!(current.color_dq_selected(), label == "Detail" || enabled);
            output = next;
        }
    }
}

#[test]
fn section_bypasses_keep_controls_visible_fixed_and_disabled_without_losing_edits() {
    let ctx = egui::Context::default();
    crate::widgets::install_style(&ctx);
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.exposure = 0.7;
    settings.tone.contrast = 0.3;
    settings.saturation = 1.2;
    settings.sharpen_amount = Some(0.4);
    settings.reconstruction = Some(Default::default());
    let original = settings.clone();
    let mut frame = |time, events| {
        let mut changed = false;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400., 2000.),
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
    let (_, mut output, _) = frame(0., vec![]);
    let labels = [
        "Tone and Color",
        "Exposure",
        "Noise",
        "Chroma",
        "Detail",
        "Fine",
        "Size",
        "Coarse",
        "Sharpening",
        "Amount",
        "Corrections",
        "Threshold",
    ];
    let positions = |out: &egui::FullOutput| {
        labels
            .iter()
            .map(|label| text_centers(out, label))
            .collect::<Vec<_>>()
    };
    let baseline = positions(&output);
    for (index, (label, parameter)) in [
        ("Tone and Color", "Exposure"),
        ("Noise", "Chroma"),
        ("Detail", "Fine"),
        ("Sharpening", "Amount"),
    ]
    .into_iter()
    .enumerate()
    {
        let time = 1. + 4. * index as f64;
        let point = text_centers(&output, label)[0];
        frame(
            time,
            vec![
                egui::Event::PointerMoved(point),
                pointer_button(point, true),
            ],
        );
        let (changed, off, current) = frame(time + 0.1, vec![pointer_button(point, false)]);
        assert!(changed);
        assert_eq!(positions(&off), baseline, "{label} shifted controls");
        let mut expected = original.clone();
        match label {
            "Tone and Color" => expected.tone_enabled = false,
            "Sharpening" => expected.sharpening_enabled = false,
            "Noise" => expected.reconstruction.as_mut().unwrap().noise_enabled = false,
            "Detail" => expected.reconstruction.as_mut().unwrap().detail_enabled = false,
            _ => unreachable!(),
        }
        assert_eq!(current, expected);
        let slider = egui::pos2(240., text_centers(&off, parameter)[0].y);
        frame(
            time + 1.,
            vec![
                egui::Event::PointerMoved(slider),
                pointer_button(slider, true),
            ],
        );
        let (changed, _, current) = frame(time + 1.1, vec![pointer_button(slider, false)]);
        assert!(!changed, "disabled {parameter} changed");
        assert_eq!(current, expected);
        frame(
            time + 2.,
            vec![
                egui::Event::PointerMoved(point),
                pointer_button(point, true),
            ],
        );
        let (changed, next, current) = frame(time + 2.1, vec![pointer_button(point, false)]);
        assert!(changed);
        assert_eq!(current, original);
        assert_eq!(positions(&next), baseline);
        output = next;
    }
}

#[test]
fn parameter_group_pipe_keeps_left_spacing_and_halves_text_gap() {
    let ctx = egui::Context::default();
    crate::widgets::install_style(&ctx);
    let mut group = egui::Rect::NOTHING;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400., 180.),
            )),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let padding = f32::from(crate::widgets::PARAMETER_GROUP_HORIZONTAL_PADDING);
                let column = super::common::LabelColumn::new(ui, &["Coarse"], &["Size"])
                    .with_outer_padding(padding);
                group = crate::widgets::parameter_group(ui, |ui| {
                    column.adjustment(ui, ("Coarse detail", "Coarse"), &mut 1_f32, 0. ..=2., 1.);
                    column.indented().adjustment(
                        ui,
                        ("Coarse size", "Size"),
                        &mut 1_f32,
                        0.5..=2.,
                        1.,
                    );
                })
                .response
                .rect;
            });
        },
    );
    let text = |label: &str| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == label => Some(t),
                _ => None,
            })
            .unwrap()
    };
    let child = text("Size");
    let glyph_left = child.pos.x + child.galley.rows[0].visuals.mesh_bounds.left();
    let pipe = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::LineSegment { points, .. } if points[0].x == points[1].x => {
                Some(points[0].x)
            }
            _ => None,
        })
        .unwrap();
    assert!(
        ((pipe - group.left()) - 2. * (glyph_left - pipe)).abs() <= 1. / ctx.pixels_per_point(),
        "group {}, pipe {pipe}, glyph {glyph_left}",
        group.left()
    );
    let padding = f32::from(crate::widgets::PARAMETER_GROUP_HORIZONTAL_PADDING);
    assert_eq!(text("Coarse").pos.x - group.left(), padding);
    for value in output.shapes.iter().filter_map(|shape| match &shape.shape {
        egui::Shape::Text(t) if t.galley.text() == "1.00" => Some(t),
        _ => None,
    }) {
        let center = value.pos + value.galley.size() * 0.5;
        let field = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(r) if r.rect.contains(center) => Some(r.rect),
                _ => None,
            })
            .min_by(|a, b| a.area().total_cmp(&b.area()))
            .unwrap();
        assert_eq!(group.right() - field.right(), padding);
    }
}

#[test]
fn hdr_edits_and_pointer_hover_keep_the_editor_panel_width_stable() {
    let ctx = egui::Context::default();
    crate::widgets::install_style(&ctx);
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.photographic.tone_method = foveonic::render::PhotographicMethod::Universal;
    let mut frame = |time, amount, pointer| {
        settings.tonal_ranges.highlights = amount;
        let mut width = 0.;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400., 800.),
                )),
                time: Some(time),
                events: vec![egui::Event::PointerMoved(pointer)],
                ..Default::default()
            },
            |ctx| {
                egui::SidePanel::left("organizer")
                    .default_width(300.)
                    .show(ctx, |ui| {
                        ui.label("Photos");
                    });
                width = egui::SidePanel::right("settings")
                    .default_width(330.)
                    .width_range(300. ..=600.)
                    .resizable(true)
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            processing(ui, &mut settings, None);
                        });
                    })
                    .response
                    .rect
                    .width();
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("Image");
                });
            },
        );
        width
    };
    for i in 0..5 {
        frame(i as f64 * 0.1, 0., egui::pos2(500., 100.));
    }
    let expected = frame(0.5, 0., egui::pos2(500., 100.));
    for (i, amount) in [0.1, -0.1, 0.5, -0.5, 1., -1., 0.].into_iter().enumerate() {
        for j in 0..12 {
            let pointer = egui::pos2(1100. + j as f32 * 23., 180. + (j % 4) as f32 * 100.);
            let width = frame(1. + (i * 12 + j) as f64 * 0.1, amount, pointer);
            assert!(
                (width - expected).abs() < 1.,
                "amount {amount}, hover {pointer:?}: editor width {width}, expected {expected}"
            );
        }
    }
}

#[test]
fn universal_sharpening_values_fit_the_reserved_field_and_keep_panel_width_stable() {
    let ctx = egui::Context::default();
    crate::widgets::install_style(&ctx);
    ctx.memory_mut(|m| m.set_everything_is_visible(true));
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.photographic.sharpening.method = foveonic::render::PhotographicMethod::Universal;
    for (i, threshold) in [0.01, 0.123, 0.2, 0.001, 0.00123456, 1.23456e-10, 0.]
        .into_iter()
        .enumerate()
    {
        settings.photographic.sharpening.threshold = threshold;
        settings.photographic.sharpening.radius = 0.3123456;
        let mut width = 0.;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400., 800.),
                )),
                time: Some(i as f64 * 0.1),
                events: vec![egui::Event::PointerMoved(egui::pos2(
                    1100. + i as f32 * 20.,
                    110.,
                ))],
                ..Default::default()
            },
            |ctx| {
                width = egui::SidePanel::right("settings")
                    .default_width(330.)
                    .width_range(300. ..=600.)
                    .show(ctx, |ui| {
                        common::sharpening(ui, &mut settings, None);
                    })
                    .response
                    .rect
                    .width();
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("Image");
                });
            },
        );
        assert!(
            (width - 330.).abs() < 1.,
            "threshold {threshold}: panel grew to {width}"
        );
        assert_eq!(
            text_centers(&output, "0.312").len(),
            1,
            "radius readout should use three significant digits"
        );
        assert_eq!(
            settings.photographic.sharpening.radius, 0.3123456,
            "formatting should preserve the stored setting"
        );
    }
}

#[test]
fn photographic_value_readouts_limit_significant_digits_and_round_carries() {
    for (value, expected) in [
        (0., "0.00"),
        (-0., "0.00"),
        (1., "1.00"),
        (1.23456, "1.23"),
        (-1.23456, "-1.23"),
        (12.3456, "12.3"),
        (123.456, "123"),
        (0.123456, "0.123"),
        (0.0123456, "0.0123"),
        (0.00123456, "0.00123"),
        (0.01, "0.01"),
        (0.8, "0.80"),
        (9.999, "10.0"),
        (0.9999, "1.00"),
        (999.9, "1e3"),
        (1.23456e-10, "1.23e-10"),
        (-1.23456e10, "-1.23e10"),
    ] {
        assert_eq!(common::format_parameter_value(value), expected, "{value}");
    }
}
