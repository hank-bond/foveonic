//! Shows selectable capture metadata, the displayed filename, and inclusive visible-image pixel bounds below the viewport.

use crate::viewport::PixelRegion;
use eframe::egui;
use std::collections::BTreeMap;

pub const HEIGHT: f32 = 26.;

pub fn capture(properties: &BTreeMap<String, String>) -> String {
    let rows = crate::metadata::property_rows(properties);
    [
        "Shutter",
        "Aperture",
        "Focal length",
        "ISO",
        "Exposure comp.",
    ]
    .into_iter()
    .filter_map(|key| {
        rows.iter()
            .find(|(label, _)| *label == key)
            .map(|(_, value)| match key {
                "ISO" => format!("ISO {value}"),
                "Exposure comp." => value
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .map(|v| format!("{v:+.1} EV"))
                    .unwrap_or_else(|| format!("{value} EV")),
                _ => value.clone(),
            })
    })
    .collect::<Vec<_>>()
    .join("  ")
}
pub fn region_text(region: PixelRegion) -> String {
    let [w, h] = region.extent;
    let [x0, y0] = region.top_left;
    let [x1, y1] = region.bottom_right;
    format!("{w}×{h} · ({x0},{y0})–({x1},{y1})")
}

pub fn show(
    ui: &mut egui::Ui,
    properties: Option<&BTreeMap<String, String>>,
    filename: &str,
    region: Option<PixelRegion>,
) {
    let left = properties.map(capture).unwrap_or_default();
    let right = region.map(region_text).unwrap_or_default();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), HEIGHT),
        egui::Sense::hover(),
    );
    ui.painter().line_segment(
        [rect.left_top(), rect.right_top()],
        egui::Stroke::new(1_f32, egui::Color32::from_gray(40)),
    );
    let color = ui.visuals().text_color();
    let font = egui::FontId::proportional(11.);
    let right_font = egui::FontId::monospace(10.);
    let left_width = ui
        .painter()
        .layout_no_wrap(left.clone(), font.clone(), color)
        .size()
        .x;
    let right_width = ui
        .painter()
        .layout_no_wrap(right.clone(), right_font.clone(), color)
        .size()
        .x;
    // Center the filename independently of the side readouts so it does not
    // jump when those values change. A tooltip covers names that no longer fit.
    let side = left_width.max(right_width).min(rect.width() * 0.48);
    let regions = [
        (
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.left() + side, rect.bottom())),
            left,
            font.clone(),
            egui::Align2::LEFT_CENTER,
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(rect.left() + side + 4., rect.top()),
                egui::pos2(rect.right() - side - 4., rect.bottom()),
            ),
            filename.into(),
            font,
            egui::Align2::CENTER_CENTER,
        ),
        (
            egui::Rect::from_min_max(egui::pos2(rect.right() - side, rect.top()), rect.max),
            right,
            right_font,
            egui::Align2::RIGHT_CENTER,
        ),
    ];
    for (index, (area, text, font, align)) in regions.into_iter().enumerate() {
        if !area.is_positive() {
            continue;
        }
        let at = match align {
            egui::Align2::LEFT_CENTER => area.left_center(),
            egui::Align2::RIGHT_CENTER => area.right_center(),
            _ => area.center(),
        };
        let galley = ui.painter().layout_no_wrap(text, font, color);
        let text_rect = align.anchor_size(at, galley.size());
        // Keep the original alignment and clipping, but let egui handle selection
        // and copying instead of painting inert text.
        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt(("information", index))
                .max_rect(area),
            |ui| {
                ui.set_clip_rect(area.intersect(ui.clip_rect()));
                ui.put(text_rect, egui::Label::new(galley).selectable(true));
            },
        );
    }
    response.on_hover_text(format!("{filename}\n{}\n{}\n0-based image pixels; inclusive top-left and bottom-right. Includes partly visible pixels.",
        properties.map(capture).unwrap_or_default(), region.map(region_text).unwrap_or_default()));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_readout_supports_drag_selection_and_copy() {
        let properties = [("ISO".into(), "100".into())].into_iter().collect();
        let region = PixelRegion {
            top_left: [1150, 2094],
            bottom_right: [2292, 3123],
            extent: [1143, 1030],
        };
        for expected in [
            capture(&properties),
            "sunlit-red-flower.x3f".into(),
            region_text(region),
        ] {
            let ctx = egui::Context::default();
            let mut time = 0.;
            let mut frame = |events| {
                time += 0.1;
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1400., 100.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            show(ui, Some(&properties), "sunlit-red-flower.x3f", Some(region));
                        });
                    },
                )
            };
            frame(vec![]);
            let output = frame(vec![]);
            let rect = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == expected => {
                        Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    _ => None,
                })
                .expect("readout is visible");
            let start = rect.left_center() + egui::vec2(0.1, 0.);
            let end = rect.right_center() - egui::vec2(0.1, 0.);
            let pointer = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(vec![egui::Event::PointerMoved(start), pointer(start, true)]);
            frame(vec![egui::Event::PointerMoved(end)]);
            frame(vec![pointer(end, false)]);
            let output = frame(vec![egui::Event::Copy]);
            assert!(
                output.platform_output.commands.iter().any(|command| {
                    matches!(command, egui::OutputCommand::CopyText(text) if text == &expected)
                }),
                "could not copy {expected:?}"
            );
        }
    }

    #[test]
    fn region_labels_and_capture_summary_use_available_photographic_values() {
        assert_eq!(
            region_text(PixelRegion {
                top_left: [100, 200],
                bottom_right: [399, 499],
                extent: [300, 300]
            }),
            "300×300 · (100,200)–(399,499)"
        );
        let properties = [
            ("SHUTTER", "0.01013"),
            ("APERTURE", "2.8"),
            ("FLENGTH", "30"),
            ("ISO", "100"),
            ("EXPCOMP", "0"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        assert_eq!(
            capture(&properties),
            "1/100  f/2.8  30 mm  ISO 100  +0.0 EV"
        );
    }
}
