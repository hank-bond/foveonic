//! Selects a cached white balance's closest preset spatial-gain table.
use super::reference::*;

use crate::source::{Metadata, Result};

const PRESETS: [&str; 6] = [
    "Sunlight",
    "Shade",
    "Overcast",
    "Incandescent",
    "Fluorescent",
    "Flash",
];

fn scalar(m: &Metadata, name: &str, fallback: f64) -> Result<f64> {
    m.matrix(name).map_or(Ok(fallback), |_| m.scalar(name))
}
fn property(m: &Metadata, name: &str, fallback: f64) -> f64 {
    m.properties
        .get(name)
        .and_then(|v| v.parse().ok())
        .unwrap_or(fallback)
}
fn controls(m: &Metadata, preset: &str) -> Result<Option<([f64; 9], [f64; 3])>> {
    // Matrix availability selects both the matrix and its matching gain prefix.
    for prefix in ["DP1_", ""] {
        let name = format!("{prefix}{preset}CCMatrix");
        if let Some(matrix) = m.matrix(&name) {
            if matrix.values.first().copied().unwrap_or(0.) != 0. {
                return Ok(Some((
                    m.fixed(&name)?,
                    m.fixed(&format!("{prefix}{preset}WBGain"))?,
                )));
            }
        }
    }
    Ok(None)
}

pub(super) fn nearest(m: &Metadata, wb: &str, ratio: f64) -> Result<Option<String>> {
    if scalar(m, "CCMatrix_Flag", 0.)? != 1.
        || scalar(m, "SkipStepFlags", 0.)? as u32 & (1 << 25) != 0
        || !matches!(wb, "Auto" | "Custom")
    {
        return Ok(None);
    }
    let Some((selected, gains)) = controls(m, wb)? else {
        return Ok(None);
    };
    let model = m
        .properties
        .get("CAMMODEL")
        .map_or("", String::as_str)
        .trim();
    let threshold = scalar(m, "BVTHRESH", -1000.)?;
    let threshold = if model == "SIGMA DP1" && threshold == 0. {
        4.
    } else {
        threshold
    };
    let shutter = scalar(m, "CaptureShutter", 0.)? / MICROSECONDS_PER_SECOND;
    let shutter = if shutter > 0. {
        shutter
    } else {
        property(m, "EXPTIME", MICROSECONDS_PER_SECOND) / MICROSECONDS_PER_SECOND
    };
    let aperture = property(
        m,
        "EXIF_FNUMBER",
        (f64::from(property(m, "APERTURE", 4.) as f32 * 10.)).round_ties_even() / 10.,
    );
    let brightness = (aperture * aperture / shutter).log10() * 3.322
        + property(m, "EXIF_EXPOSURE_BIAS", property(m, "EXPCOMP", 0.))
        - (ratio * 100. / 3.125).log10() / std::f64::consts::LOG10_2;
    let start = if brightness < threshold { 4 } else { 0 };
    let mut best = None;
    let mut distance = 100000.;
    for preset in &PRESETS[start..] {
        let Some((candidate, candidate_gain)) = controls(m, preset)? else {
            continue;
        };
        let score = (0..9)
            .map(|i| (candidate[i] * candidate_gain[i % 3] - selected[i] * gains[i % 3]).powi(2))
            .sum::<f64>();
        if score < distance {
            distance = score;
            best = Some(format!("SpatialGain_{preset}"));
        }
    }
    Ok(best.filter(|name| m.matrix(name).is_some()))
}

#[test]
#[ignore = "requires CLASSIC_GRID_SOURCE original"]
fn original_auto_selects_overcast_grid() {
    let source =
        crate::source::Source::open(std::env::var("CLASSIC_GRID_SOURCE").unwrap()).unwrap();
    let iso = super::prepare::capture_iso(&source.metadata).unwrap();
    let sensor = scalar(&source.metadata, "SensorISO", 100.).unwrap();
    assert_eq!(
        nearest(&source.metadata, "Auto", iso / sensor)
            .unwrap()
            .as_deref(),
        Some("SpatialGain_Overcast")
    );
}

#[test]
fn cached_selection_compares_gain_weighted_matrices() {
    use crate::source::{Matrix, MatrixEncoding};
    use std::collections::BTreeMap;
    let mut m = Metadata {
        processing_version: None,
        width: 32,
        height: 32,
        rotation: 0,
        properties: BTreeMap::new(),
        matrices: BTreeMap::new(),
        lists: BTreeMap::new(),
    };
    let mut insert = |name: String, values: Vec<f64>| {
        m.matrices.insert(
            name,
            Matrix {
                encoding: MatrixEncoding::Float32,
                representation: "synthetic spatial-gain selection fixture",
                dimensions: vec![values.len()],
                values,
            },
        );
    };
    insert("CCMatrix_Flag".into(), vec![1.]);
    for preset in PRESETS.into_iter().chain(["Auto"]) {
        insert(
            format!("{preset}CCMatrix"),
            vec![1., 0., 0., 0., 1., 0., 0., 0., 1.],
        );
        insert(
            format!("{preset}WBGain"),
            if matches!(preset, "Auto" | "Overcast") {
                vec![2., 1., 1.]
            } else {
                vec![1.; 3]
            },
        );
        insert(format!("SpatialGain_{preset}"), vec![1.; 3]);
    }
    assert_eq!(
        nearest(&m, "Auto", 1.).unwrap().as_deref(),
        Some("SpatialGain_Overcast")
    );
    assert_eq!(nearest(&m, "Sunlight", 1.).unwrap(), None);
    m.matrices.get_mut("CCMatrix_Flag").unwrap().values[0] = 0.;
    assert_eq!(nearest(&m, "Auto", 1.).unwrap(), None);
}
