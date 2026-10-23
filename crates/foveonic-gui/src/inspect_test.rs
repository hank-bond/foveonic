//! Exercises inspection menus, preview preferences, and display comparisons.

use super::*;
#[test]
fn undocumented_inspection_disclosures_remain_available() {
    assert!(help::inspection("future.stage").is_none());
    let context = egui::Context::default();
    let _ = context.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            assert!(!group_row(ui, "future.stage", false));
            let response = inspection_help(
                widgets::menu_row(ui, "future.scalar", false),
                "future.scalar",
            );
            assert!(response.enabled());
        });
    });
}
#[test]
fn quattro_layer_popup_opens_without_requiring_hover_help() {
    use foveonic::source::{Matrix, MatrixEncoding};
    let metadata = Metadata {
        width: 5888,
        height: 3672,
        rotation: 0,
        processing_version: Some([3, 1, 2, 0]),
        properties: [("CAMMODEL".into(), "SIGMA dp2 Quattro".into())].into(),
        matrices: [(
            "CAMERAID".into(),
            Matrix {
                encoding: MatrixEncoding::Float32,
                representation: "native",
                dimensions: vec![1],
                values: vec![81.],
            },
        )]
        .into(),
        lists: Default::default(),
    };
    let context = egui::Context::default();
    let settings = Settings::default();
    let mut output = "sensor.rgb".to_owned();
    let mut channel = 0;
    let mut draw = |events| {
        context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 1200.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.spacing_mut().combo_height = 1000.;
                    assert!(!show(
                        ui,
                        &settings,
                        &mut output,
                        &mut channel,
                        Some(&metadata),
                        None
                    ));
                });
            },
        )
    };
    let closed = draw(vec![]);
    let point = closed
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "sensor.rgb · Layer 1 (R)" => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .unwrap();
    for pressed in [true, false] {
        draw(vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            },
        ]);
    }
    let opened = draw(vec![]);
    assert!(opened.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text() == "blue-refresh.rgb")));
    assert_eq!(output, "sensor.rgb");
    assert_eq!(channel, 0);
}
#[test]
fn inspect_width_follows_available_labels_and_preserves_long_selected_components() {
    for em in [13., 20.] {
        for rendering in [
            foveonic::render::Rendering::Color,
            foveonic::render::Rendering::Monochrome,
        ] {
            for (name, component, selected) in [
                ("final", -1, "final · RGB"),
                ("calibration.layers", 0, "calibration.layers · Layer 1 (R)"),
            ] {
                let ctx = egui::Context::default();
                crate::widgets::install_style(&ctx);
                ctx.style_mut(|style| {
                    style.override_font_id = Some(egui::FontId::proportional(em))
                });
                let mut settings = Settings::new("Sunlight", "Standard");
                settings.rendering = rendering;
                let mut output = name.to_owned();
                let mut channel = component;
                let mut expected = 0.;
                let rendered = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        expected =
                            menu_width(ui, &inspection::image_outputs(&settings, None), selected);
                        assert!(!show(ui, &settings, &mut output, &mut channel, None, None));
                    });
                });
                let text = rendered
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == selected => Some(text),
                        _ => None,
                    })
                    .unwrap();
                let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
                let header = rendered
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect)
                            if rect.stroke.width > 0. && rect.rect.contains_rect(bounds) =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .min_by(|a, b| a.area().total_cmp(&b.area()))
                    .unwrap();
                assert!((header.width() - expected).abs() < 1.);
                if em == 13. && name == "final" {
                    assert!(
                        header.width() < 205.,
                        "default Inspect still reserves its old fixed width"
                    );
                }
                assert_eq!(output, name);
                assert_eq!(channel, component);
            }
        }
    }
}

#[test]
fn scrolling_inspect_popup_keeps_disclosure_arrows_clear_of_long_labels() {
    for em in [13., 20.] {
        let ctx = egui::Context::default();
        crate::widgets::install_style(&ctx);
        ctx.style_mut(|style| style.override_font_id = Some(egui::FontId::proportional(em)));
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        settings.sharpen_amount = Some(1.);
        let mut output = "final".to_owned();
        let mut channel = -1;
        let mut draw = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.spacing_mut().combo_height = 180.;
                        assert!(!show(ui, &settings, &mut output, &mut channel, None, None));
                    });
                },
            )
        };
        let closed = draw(vec![]);
        let point = closed
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "final · RGB" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        for pressed in [true, false] {
            draw(vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ]);
        }
        let opened = draw(vec![]);
        let text = opened
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "calibration.layers" => {
                    Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                }
                _ => None,
            })
            .unwrap();
        let arrow = opened
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Path(path) if path.closed && path.points.len() == 3 => {
                    let bounds = egui::Rect::from_points(&path.points);
                    ((bounds.center().y - text.center().y).abs() < 1.).then_some(bounds)
                }
                _ => None,
            })
            .unwrap();
        assert!(
            arrow.left() - text.right() >= 4.,
            "{em}pt label overlaps disclosure: text={text:?}, arrow={arrow:?}"
        );
    }
}

#[test]
fn popup_disclosure_stays_open_but_component_scalar_and_outside_clicks_close() {
    fn text_rect(shapes: &[egui::epaint::ClippedShape], label: &str) -> egui::Rect {
        shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing visible text: {label}"))
    }
    let ctx = egui::Context::default();
    let settings = Settings::new("Sunlight", "Standard");
    let mut output = "final".to_owned();
    let mut channel = -1;
    let draw = |output: &mut String, channel: &mut i32, events: Vec<egui::Event>| {
        let mut id = egui::Id::NULL;
        let mut changed = false;
        let result = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 1200.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.spacing_mut().combo_height = 1000.;
                    id = ui.make_persistent_id(egui::Id::new("artifact"));
                    changed = show(ui, &settings, output, channel, None, None);
                });
            },
        );
        (changed, egui::ComboBox::is_open(&ctx, id), result.shapes)
    };
    let click = |output: &mut String, channel: &mut i32, point| {
        draw(
            output,
            channel,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        draw(
            output,
            channel,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        )
    };
    let (_, _, shapes) = draw(&mut output, &mut channel, vec![]);
    let combo = text_rect(&shapes, "final · RGB").center();
    assert!(click(&mut output, &mut channel, combo).1);
    let (_, _, shapes) = draw(&mut output, &mut channel, vec![]);
    let group = text_rect(&shapes, "sensor.layers").center();
    let (changed, open, _) = click(&mut output, &mut channel, group);
    assert!(!changed);
    assert!(open, "expanding a group must not dismiss Inspect");
    assert_eq!((&*output, channel), ("final", -1));
    let (_, open, shapes) = draw(&mut output, &mut channel, vec![]);
    assert!(open);
    assert!(shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Path(path) if path.closed && path.points.len() == 3)));
    let component = text_rect(&shapes, "Layer 2 (G)").center();
    let (_, open, _) = click(&mut output, &mut channel, group);
    assert!(open, "collapsing a group must not dismiss Inspect");
    click(&mut output, &mut channel, group);
    let (changed, open, _) = click(&mut output, &mut channel, component);
    assert!(changed);
    assert!(!open, "choosing a component must dismiss Inspect");
    assert_eq!((&*output, channel), ("sensor.layers", 1));
    let (_, _, shapes) = draw(&mut output, &mut channel, vec![]);
    let combo = text_rect(&shapes, "sensor.layers · Layer 2 (G)").center();
    click(&mut output, &mut channel, combo);
    let (_, _, shapes) = draw(&mut output, &mut channel, vec![]);
    let scalar = text_rect(&shapes, "highlight.density").center();
    let (changed, open, _) = click(&mut output, &mut channel, scalar);
    assert!(changed);
    assert!(!open);
    assert_eq!((&*output, channel), ("highlight.density", -1));
    let (_, _, shapes) = draw(&mut output, &mut channel, vec![]);
    let combo = text_rect(&shapes, "highlight.density").center();
    click(&mut output, &mut channel, combo);
    assert!(draw(&mut output, &mut channel, vec![]).1);
    assert!(!click(&mut output, &mut channel, egui::pos2(550., 1100.)).1);
}

#[test]
fn disclosure_rows_share_option_padding_at_each_font_size() {
    for em in [14., 20.] {
        let ctx = egui::Context::default();
        crate::widgets::install_style(&ctx);
        let mut rect = egui::Rect::NOTHING;
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.style_mut().override_font_id = Some(egui::FontId::proportional(em));
                rect = ui
                    .scope(|ui| group_row(ui, "noise.layers", false))
                    .response
                    .rect;
            });
        });
        let text = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "noise.layers" => Some(text),
                _ => None,
            })
            .unwrap();
        let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
        assert!((bounds.left() - rect.left() - 0.5 * em).abs() < 1.);
        for gap in [bounds.top() - rect.top(), rect.bottom() - bounds.bottom()] {
            assert!((gap - 0.25 * em).abs() < 1.);
        }
    }
}

#[test]
fn disclosure_opens_and_closes_from_both_edges_of_the_row() {
    let ctx = egui::Context::default();
    let draw = |events: Vec<egui::Event>| {
        let mut open = false;
        let mut rect = egui::Rect::NOTHING;
        let rendered = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300., 300.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let origin = ui.cursor().min;
                    let width = ui.available_width();
                    let height = ui.spacing().interact_size.y;
                    open = group_row(ui, "repair.layers", false);
                    rect = egui::Rect::from_min_size(origin, egui::vec2(width, height));
                });
            },
        );
        assert_eq!(rendered.shapes.iter().filter(|shape| {
            matches!(&shape.shape, egui::Shape::Path(path) if path.closed && path.points.len() == 3)
        }).count(), 1, "the disclosure must be a drawn triangle, not a font glyph");
        (open, rect)
    };
    let (open, rect) = draw(vec![]);
    assert!(!open);
    let click = |point| {
        draw(vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ]);
        draw(vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }])
        .0
    };
    assert!(click(rect.left_center() + egui::vec2(5., 0.)));
    assert!(draw(vec![]).0); // state survives subsequent frames
    assert!(!click(rect.right_center() - egui::vec2(5., 0.)));
}

#[test]
fn comparison_buttons_reuse_pairs_restore_the_side_and_support_rgb() {
    let ctx = egui::Context::default();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.saturation = 1.2;
    let mut comparison = Comparison::default();
    let draw = |comparison: &mut Comparison, output: &str, channel: i32, events| {
        let mut requested = false;
        let result = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 400.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        requested = comparison.show(ui, &settings, output, channel, None);
                    });
                });
            },
        );
        (requested, result)
    };
    let click = |comparison: &mut Comparison, output, channel, label| {
        let (_, result) = draw(comparison, output, channel, vec![]);
        let point = result
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        let mut requested = false;
        for pressed in [true, false] {
            requested |= draw(
                comparison,
                output,
                channel,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            )
            .0;
        }
        requested
    };
    assert!(!comparison.requested());
    assert!(!comparison.input);
    assert!(click(&mut comparison, "repair.layers", 0, "Input"));
    assert!(comparison.input);
    assert!(!draw(&mut comparison, "repair.layers", 1, vec![]).0);
    assert!(!click(&mut comparison, "repair.layers", 1, "Δ Difference"));
    assert!(comparison.difference && comparison.input);
    assert!(!click(&mut comparison, "repair.layers", 1, "Output"));
    assert!(comparison.input, "Difference must disable the side buttons");
    assert!(!click(&mut comparison, "repair.layers", 1, "Δ Difference"));
    assert!(!comparison.difference && comparison.input);
    assert!(comparison.mapping(1, true).input);
    assert!(!click(&mut comparison, "repair.layers", 1, "Output"));
    assert!(!comparison.input);
    assert!(click(&mut comparison, "repair.layers", 1, "Δ Difference"));
    assert!(!draw(&mut comparison, "repair.layers", 1, vec![]).0);
    draw(&mut comparison, "color.rgb", 0, vec![]);
    assert!(!comparison.requested());
    assert!(click(&mut comparison, "saturation.rgb", -1, "Input"));
    assert!(comparison.input);
    assert!(!click(
        &mut comparison,
        "saturation.rgb",
        -1,
        "Δ Difference"
    ));
    assert!(!comparison.difference);
    assert!(comparison.mapping(-1, true).input);
    draw(&mut comparison, "sensor.layers", 0, vec![]);
    assert!(!comparison.requested());
    assert!(!click(&mut comparison, "sensor.layers", 0, "Input"));
    assert!(!comparison.requested());
}

#[test]
fn comparison_mapping_keeps_pending_inputs_on_output_and_difference_overrides_the_side() {
    let mut comparison = Comparison::default();
    assert_eq!(
        comparison.mapping(2, true),
        crate::viewport::Mapping::normal(2)
    );
    comparison.input = true;
    assert_eq!(
        comparison.mapping(2, false),
        crate::viewport::Mapping::normal(2)
    );
    let input = comparison.mapping(2, true);
    assert!(input.input);
    assert_eq!(input.channel, 2);
    assert_eq!(input.difference_gain, 0.);
    comparison.difference = true;
    let difference = comparison.mapping(2, true);
    assert!(!difference.input);
    assert_eq!(difference.difference_gain, 32.);
    assert_eq!(
        comparison.mapping(2, false),
        crate::viewport::Mapping::normal(2)
    );
    comparison.difference = false;
    assert_eq!(comparison.mapping(2, true), input);
}

#[test]
fn component_selection_reuses_the_root_and_scalar_views_have_no_layers() {
    let mut output = "final".to_owned();
    let mut channel = -1;
    choose(&mut output, &mut channel, "final", 2);
    assert_eq!((&*output, channel), ("final", 2));
    choose(&mut output, &mut channel, "sensor.layers", 1);
    assert_eq!((&*output, channel), ("sensor.layers", 1));
    assert_eq!(components(Channels::Scalar), &[(-1, "Scalar")]);
    assert_eq!(components(Channels::Layers).len(), 3);
}

#[test]
fn classic_selection_normalizes_unavailable_stages_and_layer_components() {
    let mut metadata = Metadata {
        processing_version: None,
        width: 2688,
        height: 1792,
        rotation: 0,
        properties: [
            ("CAMMODEL".into(), "SIGMA SD15".into()),
            ("ISO".into(), "100".into()),
        ]
        .into(),
        matrices: [(
            "ChromaBlur".into(),
            foveonic::source::Matrix {
                encoding: foveonic::source::MatrixEncoding::Float32,
                representation: "native",
                dimensions: vec![1],
                values: vec![1.],
            },
        )]
        .into(),
        lists: Default::default(),
    };
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    let mut output = "noise.layers".into();
    let mut channel = 1;
    normalize(&settings, &mut output, &mut channel, Some(&metadata), None);
    assert_eq!((&*output, channel), ("final", -1));
    output = "chroma.layers".into();
    normalize(&settings, &mut output, &mut channel, Some(&metadata), None);
    assert_eq!((&*output, channel), ("chroma.layers", 0));
    metadata.properties.insert("ISO".into(), "400".into());
    normalize(&settings, &mut output, &mut channel, Some(&metadata), None);
    assert_eq!((&*output, channel), ("final", -1));
}

#[test]
fn quattro_unavailable_final_preserves_selection_and_plane_views_use_their_components() {
    let metadata = Metadata {
        processing_version: Some([3, 1, 2, 0]),
        width: 5888,
        height: 3672,
        rotation: 0,
        properties: [("CAMMODEL".into(), "SIGMA dp2 Quattro".into())].into(),
        matrices: Default::default(),
        lists: Default::default(),
    };
    let settings = Settings::new("Sunlight", "Standard");
    let outputs = inspection::image_outputs(&settings, Some(&metadata));
    assert!(!outputs.iter().any(|o| o.name == "final"));
    for image in &outputs {
        assert!(help::inspection(image.name).is_some(), "{}", image.name);
    }
    let ctx = egui::Context::default();
    let mut output = "final".to_owned();
    let mut channel = -1;
    let rendered = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            assert!(!show(
                ui,
                &settings,
                &mut output,
                &mut channel,
                Some(&metadata),
                None
            ));
        });
    });
    assert_eq!(output, "final");
    assert_eq!(channel, -1);
    assert!(rendered.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.job.text=="final (unavailable)")));
    output = "sensor.rgb".into();
    normalize(&settings, &mut output, &mut channel, Some(&metadata), None);
    assert_eq!(channel, 0);
    output = "sensor.blue".into();
    normalize(&settings, &mut output, &mut channel, Some(&metadata), None);
    assert_eq!(channel, -1);
}

#[test]
fn early_quattro_hint_keeps_final_unavailable_without_merrill_menu_choices() {
    let settings = Settings::new("", "");
    let mut output = "final".to_owned();
    let mut channel = -1;
    let ctx = egui::Context::default();
    let rendered = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            assert!(!show(
                ui,
                &settings,
                &mut output,
                &mut channel,
                None,
                Some(ProcessingFamily::Quattro)
            ));
        });
    });
    assert_eq!(output, "final");
    assert!(rendered.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.job.text=="final (unavailable)")));
    output = "sensor.blue".into();
    channel = 2;
    normalize(
        &settings,
        &mut output,
        &mut channel,
        None,
        Some(ProcessingFamily::Quattro),
    );
    assert_eq!(output, "sensor.blue");
    assert_eq!(channel, -1);
}

#[cfg(test)]
mod preview_tests {
    #[test]
    fn capture_support_is_resolved_before_render_without_changing_the_preference() {
        let settings = Settings::new("", "");
        assert_eq!(preview_support(&settings, None, None), None);
        for (model, supported) in [
            ("SIGMA dp0 Quattro", true),
            ("SIGMA sd Quattro", true),
            ("SIGMA sd Quattro H", false),
            ("SIGMA DP1 Merrill", false),
            ("SIGMA SD15", false),
        ] {
            let properties = [("CAMMODEL".to_string(), model.to_string())].into();
            assert_eq!(
                preview_support(&settings, None, Some(&properties)),
                Some(supported)
            );
        }
    }

    use super::*;
    #[test]
    fn fast_button_changes_workspace_quality_and_unavailable_choices_stay_disabled() {
        use crate::preferences::PreviewMode;
        let ctx = egui::Context::default();
        widgets::install_style(&ctx);
        let mut quality = PreviewMode::Full;
        let mut draw = |events, available| {
            let mut changed = false;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 200.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        changed = preview_quality(ui, true, available, &mut quality);
                    });
                },
            );
            (changed, output)
        };
        draw(vec![], Some(true));
        let (_, output) = draw(vec![], Some(true));
        let point = output
            .shapes
            .iter()
            .find_map(|s| {
                if let egui::Shape::Text(t) = &s.shape {
                    (t.galley.job.text == "Fast").then(|| t.pos + t.galley.size() * 0.5)
                } else {
                    None
                }
            })
            .unwrap();
        let button = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        draw(
            vec![egui::Event::PointerMoved(point), button(true)],
            Some(true),
        );
        assert!(draw(vec![button(false)], Some(true)).0);
        assert!(
            !draw(
                vec![egui::Event::PointerMoved(point), button(true)],
                Some(false)
            )
            .0
        );
        assert!(!draw(vec![button(false)], Some(false)).0);
        assert_eq!(quality, PreviewMode::Fast);
    }
}
