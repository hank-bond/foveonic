//! Paints capture metadata, the displayed filename, and inclusive visible-image pixel bounds below the viewport.

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
    for (area, text, font, align) in regions {
        if !area.is_positive() {
            continue;
        }
        let at = match align {
            egui::Align2::LEFT_CENTER => area.left_center(),
            egui::Align2::RIGHT_CENTER => area.right_center(),
            _ => area.center(),
        };
        ui.painter()
            .with_clip_rect(area.intersect(ui.clip_rect()))
            .text(at, align, text, font, color);
    }
    response.on_hover_text(format!("{filename}\n{}\n{}\n0-based image pixels; inclusive top-left and bottom-right. Includes partly visible pixels.",
        properties.map(capture).unwrap_or_default(), region.map(region_text).unwrap_or_default()));
}

#[cfg(test)]
mod tests {
    use super::*;
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
