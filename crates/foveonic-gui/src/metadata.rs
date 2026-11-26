//! Formats available X3F metadata as photographic readouts.

use eframe::egui;
use foveonic::source::{CaptureFacts, Metadata, MetadataSummary};
use std::collections::BTreeMap;

fn compact(value: f64) -> String {
    let text = format!("{value:.1}");
    text.strip_suffix(".0").unwrap_or(&text).to_owned()
}
fn aperture(f_number: f64) -> String {
    format!("f/{}", compact(f_number))
}
/// Show the nearest standard shutter speed, as cameras label them.
fn shutter(seconds: f64) -> String {
    if seconds >= 1. {
        return format!("{} s", compact(seconds));
    }
    const SPEEDS: &[f64] = &[
        1., 1.3, 1.6, 2., 2.5, 3., 4., 5., 6., 8., 10., 13., 15., 20., 25., 30., 40., 50., 60.,
        80., 100., 125., 160., 200., 250., 320., 400., 500., 640., 800., 1000., 1250., 1600.,
        2000., 2500., 3200., 4000., 5000., 6400., 8000.,
    ];
    let inverse = 1. / seconds;
    let snapped = SPEEDS
        .iter()
        .min_by(|a, b| ((**a / inverse).ln().abs()).total_cmp(&((**b / inverse).ln().abs())))
        .expect("standard speeds");
    format!("1/{}", compact(*snapped))
}
/// EXIF's `YYYY:MM:DD HH:MM:SS` with the date written with hyphens.
fn captured(taken: &str) -> String {
    taken.replacen(':', "-", 2)
}
fn focal_length(millimetres: f64) -> String {
    format!("{} mm", compact(millimetres))
}
/// Turn negative zero into zero so it does not print as "-0.0".
fn bias(ev: f64) -> f64 {
    if ev == 0. {
        0.
    } else {
        ev
    }
}
/// Capture details come from `CaptureFacts`, which also feeds exported EXIF and
/// Spotlight. This only words them for the panel.
pub fn property_rows(
    facts: &CaptureFacts,
    properties: &BTreeMap<String, String>,
) -> Vec<(&'static str, String)> {
    let raw = |key: &str| {
        properties
            .get(key)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let program = facts.exposure_program.map(|code| {
        match code {
            1 => "Manual",
            2 => "Program",
            3 => "Aperture priority",
            4 => "Shutter priority",
            _ => return code.to_string(),
        }
        .to_owned()
    });
    let metering = facts.metering.map(|code| {
        match code {
            1 => "Average",
            2 => "Center-weighted",
            5 => "Multi-segment",
            _ => return code.to_string(),
        }
        .to_owned()
    });
    [
        ("Camera", facts.model.clone()),
        ("Lens", facts.lens.clone()),
        ("Focal length", facts.focal_length.map(focal_length)),
        ("Aperture", facts.f_number.map(aperture)),
        ("Shutter", facts.exposure_seconds.map(shutter)),
        ("ISO", facts.iso.map(|v| v.to_string())),
        (
            "Exposure comp.",
            facts.exposure_bias.map(|v| compact(bias(v))),
        ),
        ("Program", program),
        ("Metering", metering),
        ("White balance", raw("WB_DESC")),
        ("Color mode", raw("CM_DESC")),
        ("Color space", raw("COLORSPACE")),
        ("Captured", facts.taken.as_deref().map(captured)),
        ("Software", facts.firmware.clone()),
        ("Serial", facts.serial.clone()),
    ]
    .into_iter()
    .filter_map(|(label, value)| Some((label, value?)))
    .collect()
}

/// The short exposure readout under the viewport and in browser tooltips.
pub fn capture_values(facts: &CaptureFacts) -> Vec<String> {
    [
        facts.exposure_seconds.map(shutter),
        facts.f_number.map(aperture),
        facts.focal_length.map(focal_length),
        facts.iso.map(|v| format!("ISO {v}")),
        facts.exposure_bias.map(|v| format!("{:+.1} EV", bias(v))),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// `summary` is the browser's, already resolved; without it the facts are read here.
pub fn rows(metadata: &Metadata, summary: Option<&MetadataSummary>) -> Vec<(&'static str, String)> {
    let resolved;
    let facts = match summary {
        Some(summary) => &summary.capture,
        None => {
            resolved = CaptureFacts::from_properties(&metadata.properties);
            &resolved
        }
    };
    let mut rows = property_rows(facts, &metadata.properties);
    if let Ok(area) = metadata.fixed::<4>("ActiveImageArea") {
        rows.insert(
            rows.len().min(12),
            (
                "Dimensions",
                format!(
                    "{} × {}",
                    (area[2] - area[0] + 1.) as u32,
                    (area[3] - area[1] + 1.) as u32
                ),
            ),
        );
    } else if metadata.width > 0 && metadata.height > 0 {
        rows.push((
            "Sensor size",
            format!("{} × {}", metadata.width, metadata.height),
        ));
    }
    rows
}
pub fn show(ui: &mut egui::Ui, metadata: Option<&Metadata>, summary: Option<&MetadataSummary>) {
    let Some(metadata) = metadata else {
        ui.weak("No metadata loaded");
        return;
    };
    egui::Grid::new("photographic metadata")
        .num_columns(2)
        .spacing([12., 7.])
        .show(ui, |ui| {
            for (label, value) in rows(metadata, summary) {
                ui.weak(label);
                ui.add(egui::Label::new(value.clone()).truncate())
                    .on_hover_text(value);
                ui.end_row();
            }
        });
}
/// Adapt early capture properties to processing-owned defaults without decoding pixels.
pub(crate) fn processing_metadata(
    properties: std::collections::BTreeMap<String, String>,
) -> foveonic::source::Metadata {
    foveonic::source::Metadata {
        processing_version: None,
        width: 0,
        height: 0,
        rotation: 0,
        properties,
        matrices: Default::default(),
        lists: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn facts(pairs: &[(&str, &str)]) -> (CaptureFacts, BTreeMap<String, String>) {
        let properties: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        (CaptureFacts::from_properties(&properties), properties)
    }
    #[test]
    fn camera_shutter_description_is_shared_by_table_and_viewport() {
        let (facts, properties) = facts(&[("SHUTTER", "0.01563"), ("SH_DESC", "1/60")]);
        assert_eq!(
            property_rows(&facts, &properties),
            [("Shutter", "1/60".into())]
        );
        assert_eq!(capture_values(&facts), ["1/60"]);
    }
    #[test]
    fn photographic_not_measured_formatting() {
        assert_eq!(aperture(4.), "f/4");
        assert_eq!(aperture(2.8), "f/2.8");
        assert_eq!(shutter(0.01013), "1/100");
        assert_eq!(shutter(0.00638), "1/160");
        assert_eq!(shutter(2.), "2 s");
        assert_eq!(captured("2026:09:30 18:11:58"), "2026-09-30 18:11:58");
    }
    #[test]
    fn capture_readout_uses_net_compensation_for_bracketed_shots() {
        let (facts, _) = facts(&[
            ("EXPCOMP", "+0.00"),
            ("EXPNET", "+1.00"),
            ("BRACKET", "3 of 3"),
        ]);
        assert_eq!(capture_values(&facts), vec!["+1.0 EV"]);
    }
}
