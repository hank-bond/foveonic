//! Checks the shared numeric display policy across widget types, themes and production call sites.

use super::*;
use std::path::Path;

fn numeric_text(output: &egui::FullOutput, expected: &str) -> usize {
    output
        .shapes
        .iter()
        .filter(|s| {
            matches!(&s.shape,
        egui::Shape::Text(t) if t.galley.job.text == expected)
        })
        .count()
}

#[test]
fn numeric_widgets_limit_readouts_without_call_site_formatting_or_value_rounding() {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let ctx = egui::Context::default();
        install_style(&ctx);
        ctx.set_theme(theme);
        for original in [
            0.123456789,
            -0.23456789,
            0.0123456789,
            1.23456789,
            12.3456789,
            123.456789,
            999.9,
            1.23456789e-8,
        ] {
            let mut value = original;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add(drag_value(&mut value));
                    ui.add(slider(&mut value, -1e5..=1e5));
                });
            });
            assert_eq!(value, original, "formatting changed stored precision");
            let expected = format_parameter_value(original);
            assert_eq!(
                numeric_text(&output, &expected),
                2,
                "{theme:?} {original}: {expected}"
            );
            assert!(!ctx.style().explanation_tooltips);
            assert_eq!(
                ctx.style().number_formatter.format(original, 0..=15),
                expected
            );
        }
    }
}

#[test]
fn integer_readouts_follow_the_same_significant_digit_limit() {
    assert_eq!(format_numeric_value(15., true), "15");
    assert_eq!(format_numeric_value(12345., true), "1.23e4");
    let ctx = egui::Context::default();
    install_style(&ctx);
    let mut value = 12345_u32;
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add(drag_value(&mut value));
        });
    });
    assert_eq!(numeric_text(&output, "1.23e4"), 1);
    assert_eq!(value, 12345);
}

#[test]
fn production_numeric_controls_cannot_bypass_shared_widgets() {
    fn check(directory: &Path) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                check(&path);
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let name = path.file_name().unwrap().to_str().unwrap();
            if name == "widgets.rs" || name == "tests.rs" || name.ends_with("_test.rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for forbidden in ["DragValue::new", "Slider::new", ".custom_formatter("] {
                assert!(
                    !source.contains(forbidden),
                    "{} bypasses shared numeric display policy with {forbidden}",
                    path.display()
                );
            }
        }
    }
    check(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
}

#[test]
fn numeric_factories_enforce_precision_even_without_workspace_style() {
    let ctx = egui::Context::default();
    let mut value = 0.123456_f32;
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add(drag_value(&mut value));
            ui.add(slider(&mut value, -1. ..=1.));
        });
    });
    assert_eq!(numeric_text(&output, "0.12"), 2);
    assert_eq!(value, 0.123456);
}
