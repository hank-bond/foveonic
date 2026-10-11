//! Formats available X3F metadata as photographic readouts.

use eframe::egui;
use foveonic::source::Metadata;

fn compact(value: f64) -> String {
    let text = format!("{value:.1}");
    text.strip_suffix(".0").unwrap_or(&text).to_owned()
}
pub fn aperture(value: &str) -> Option<String> {
    let n = value.trim().trim_start_matches("f/").parse::<f64>().ok()?;
    (n.is_finite() && n > 0.).then(|| format!("f/{}", compact(n)))
}
pub fn shutter(value: &str) -> Option<String> {
    let seconds = value.parse::<f64>().ok()?;
    if !seconds.is_finite() || seconds <= 0. {
        return None;
    }
    if seconds >= 1. {
        return Some(format!("{} s", compact(seconds)));
    }
    const SPEEDS: &[f64] = &[
        1., 1.3, 1.6, 2., 2.5, 3., 4., 5., 6., 8., 10., 13., 15., 20., 25., 30., 40., 50., 60.,
        80., 100., 125., 160., 200., 250., 320., 400., 500., 640., 800., 1000., 1250., 1600.,
        2000., 2500., 3200., 4000., 5000., 6400., 8000.,
    ];
    let inverse = 1. / seconds;
    let snapped = SPEEDS
        .iter()
        .min_by(|a, b| ((**a / inverse).ln().abs()).total_cmp(&((**b / inverse).ln().abs())))?;
    Some(format!("1/{}", compact(*snapped)))
}
fn captured(value: &str) -> Option<String> {
    let seconds = value.parse::<u64>().ok()?;
    if seconds > 253402300799 {
        return None;
    }
    let mut days = seconds / 86400;
    let mut year = 1970_u32;
    let leap = |y: u32| y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    while days >= if leap(year) { 366 } else { 365 } {
        days -= if leap(year) { 366 } else { 365 };
        year += 1;
    }
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while days >= months[month] {
        days -= months[month];
        month += 1;
    }
    Some(format!(
        "{year:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        month + 1,
        days + 1,
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    ))
}
pub fn rows(metadata: &Metadata) -> Vec<(&'static str, String)> {
    let p = &metadata.properties;
    let get = |key: &str| p.get(key).map(|s| s.trim()).filter(|s| !s.is_empty());
    let mut rows = Vec::new();
    for (label, key) in [
        ("Camera", "CAMMODEL"),
        ("Lens", "LENSMODEL"),
        ("Focal length", "FLENGTH"),
        ("Aperture", "APERTURE"),
        ("Shutter", "SHUTTER"),
        ("ISO", "ISO"),
        ("Exposure comp.", "EXPCOMP"),
        ("Program", "PMODE"),
        ("Metering", "AEMODE"),
        ("White balance", "WB_DESC"),
        ("Color mode", "CM_DESC"),
        ("Color space", "COLORSPACE"),
        ("Captured", "TIME"),
        ("Software", "FIRMVERS"),
        ("Serial", "CAMSERIAL"),
    ] {
        let Some(raw) = get(key) else {
            continue;
        };
        let value = match key {
            "LENSMODEL" => match raw {
                "1003" => "Sigma 19mm F2.8".into(),
                "1004" => "Sigma 30mm F2.8".into(),
                _ => format!("Lens ID {raw}"),
            },
            "FLENGTH" => raw
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(|v| format!("{} mm", compact(v)))
                .unwrap_or(raw.into()),
            "APERTURE" => aperture(get("AP_DESC").unwrap_or(raw))
                .or_else(|| aperture(raw))
                .unwrap_or(raw.into()),
            "SHUTTER" => get("SH_DESC")
                .map(str::to_owned)
                .or_else(|| shutter(raw))
                .unwrap_or(raw.into()),
            "PMODE" => match raw {
                "A" => "Aperture priority",
                "S" => "Shutter priority",
                "P" => "Program",
                "M" => "Manual",
                _ => raw,
            }
            .into(),
            "AEMODE" => match raw {
                "8" => "Multi-segment",
                "C" => "Center-weighted",
                "A" => "Average",
                _ => raw,
            }
            .into(),
            "TIME" => captured(raw).unwrap_or(raw.into()),
            _ => raw.into(),
        };
        rows.push((label, value));
    }
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
pub fn show(ui: &mut egui::Ui, metadata: Option<&Metadata>) {
    let Some(metadata) = metadata else {
        ui.weak("No metadata loaded");
        return;
    };
    egui::Grid::new("photographic metadata")
        .num_columns(2)
        .spacing([12., 7.])
        .show(ui, |ui| {
            for (label, value) in rows(metadata) {
                ui.weak(label);
                ui.add(egui::Label::new(value.clone()).truncate())
                    .on_hover_text(value);
                ui.end_row();
            }
        });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn photographic_not_measured_formatting() {
        assert_eq!(aperture("4.0").unwrap(), "f/4");
        assert_eq!(aperture("2.8").unwrap(), "f/2.8");
        assert_eq!(shutter("0.01013").unwrap(), "1/100");
        assert_eq!(shutter("0.00638").unwrap(), "1/160");
        assert_eq!(shutter("2").unwrap(), "2 s");
        assert!(shutter("NaN").is_none());
        assert_eq!(captured("1790791918").unwrap(), "2026-09-30 18:11:58");
        assert_eq!(captured("951782400").unwrap(), "2000-02-29 00:00:00");
    }
}
