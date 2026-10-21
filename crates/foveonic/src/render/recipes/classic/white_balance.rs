//! Prepares source-dependent neutral and channel-mean statistics for Classic white balance.
use super::reference::*;

use super::prepare;
use crate::{
    nodes::color::weighted_white_balance::{self, Prepared},
    profile::Profile,
    render::Engine,
    source::{Metadata, Result, Source},
};
fn scalar(m: &Metadata, name: &str, fallback: f64) -> Result<f64> {
    if m.matrix(name).is_some() {
        m.scalar(name)
    } else {
        Ok(fallback)
    }
}
fn property(m: &Metadata, name: &str, fallback: f64) -> f64 {
    m.properties
        .get(name)
        .and_then(|v| v.parse().ok())
        .unwrap_or(fallback)
}

pub(crate) fn estimate(
    engine: &Engine,
    source: &Source,
    white_balance: &str,
    profile: &mut Profile,
) -> Result<crate::render::WhiteBalanceStats> {
    let mut result = crate::render::WhiteBalanceStats::default();
    let auto = white_balance.eq_ignore_ascii_case("Auto");
    if prepare::capture_iso(&source.metadata)? >= RANGE_RECONSTRUCTION_ISO
        && white_balance.eq_ignore_ascii_case("Incandescent")
    {
        result.mean =
            Some(engine.cached_channel_mean(source, || gray_mean(engine, source, profile))?);
    }
    if auto {
        if let Some(p) = preparation(&source.metadata)? {
            result.neutral = Some(engine.cached_neutral(source, || {
                let pixels = engine
                    .gpu
                    .upload("Original layers for white balance", &source.pixels)?;
                weighted_white_balance::estimate(&engine.gpu, &pixels, &p, profile)
            })?);
        }
    }
    Ok(result)
}

/// Measures signed camera layers after constant drift subtraction and before gain.
pub(crate) fn gray_mean(
    engine: &Engine,
    source: &Source,
    profile: &mut Profile,
) -> Result<[f64; 3]> {
    // This choice retrieves the shared correction controls. The measured layers
    // have no white-balance gain or color matrix applied.
    let prepared = prepare::prepare(&source.metadata, "Sunlight")?;
    let p = &prepared.correction.params;
    let drift_value = |i: usize| p.drift[i / 2][(i % 2) * 2] + p.drift[i / 2][(i % 2) * 2 + 1];
    let drift = std::array::from_fn(|c| (drift_value(3 + c) + drift_value(9 + c)) * 0.5);
    let pixels = engine
        .gpu
        .upload("Original layers for gray-world statistics", &source.pixels)?;
    weighted_white_balance::mean(
        &engine.gpu,
        &pixels,
        [source.metadata.width as u32, source.metadata.height as u32],
        p.shields,
        p.drift_controls[2] != 0,
        drift,
        profile,
    )
}

fn preparation(m: &Metadata) -> Result<Option<Prepared>> {
    if scalar(m, "CCMatrix_Flag", 0.)? == 1. {
        return Ok(None);
    }
    if scalar(m, "SkipStepFlags", 0.)? as u32 & (1 << 24) != 0 {
        return Err("the original histogram white-balance estimator is not supported".into());
    }
    let prepared = prepare::prepare(m, "Auto")?;
    let input = &prepared.color;
    let correction = &prepared.correction.params;
    // Undo the calibration column normalization to recover the estimator matrix.
    let camera = std::array::from_fn(|i| input.camera[i] * input.relative_gain[i % 3]);
    let mut tone = weighted_white_balance::multiply(COLOR_BASIS, camera);
    let response: [f64; 3] = std::array::from_fn(|r| tone[r * 3..r * 3 + 3].iter().sum());
    let floor = neutral_weight(response) * NEUTRAL_ROW_FLOOR;
    let response = response.map(|v| v.max(floor));
    let mean = neutral_weight(response);
    for (i, v) in tone.iter_mut().enumerate() {
        *v *= mean / response[i / 3] * f64::from(input.ratio);
    }
    let model = m.properties.get("CAMMODEL").map_or("", String::as_str);
    let shutter =
        scalar(m, "CaptureShutter", 0.)? / super::policy::calibration_shutter_units(model.trim());
    let shutter = if shutter > 0. {
        shutter
    } else {
        property(
            m,
            "SHUTTER",
            property(m, "EXPTIME", MICROSECONDS_PER_SECOND) / MICROSECONDS_PER_SECOND,
        )
    };
    // Match the property parser's Float32 tenths rounding when EXIF is absent.
    let fallback_aperture =
        (f64::from(property(m, "APERTURE", scalar(m, "CaptureAperture", 4.)?) as f32 * 10.))
            .round_ties_even()
            / 10.;
    let aperture = property(m, "EXIF_FNUMBER", fallback_aperture);
    let exposure = property(m, "EXIF_EXPOSURE_BIAS", property(m, "EXPCOMP", 0.));
    let scale = f64::from(input.ratio);
    let brightness = aperture * aperture / shutter;
    let brightness = brightness.log10() * 3.322 + exposure
        - (scale * 100. / 3.125).log10() / std::f64::consts::LOG10_2;
    let drift_value =
        |i: usize| correction.drift[i / 2][(i % 2) * 2] + correction.drift[i / 2][(i % 2) * 2 + 1];
    let p = Prepared {
        extent: [m.width as u32, m.height as u32],
        camera,
        tone,
        saturation: m.fixed("SaturationLevel")?,
        brightness,
        scale,
        flash: m
            .properties
            .get("FLASH")
            .is_some_and(|s| !matches!(s.trim(), "OFF" | "0" | "None")),
        constrained: shutter <= {
            let limit = scalar(m, "MaxConstrainTime", 0.5)?;
            if limit == 0. {
                0.5
            } else {
                limit
            }
        },
        shields: correction.shields,
        estimate_drift: correction.drift_controls[2] != 0,
        drift: std::array::from_fn(|c| (drift_value(3 + c) + drift_value(9 + c)) * 0.5),
        flare: [
            correction.flare[0],
            correction.flare[1],
            correction.flare[2],
        ],
        polynomial: correction.polynomial,
    };
    Ok(Some(p))
}

#[test]
#[ignore = "requires CLASSIC_AWB_SOURCE original and reference receipt"]
fn original_neutral_matches_reference() {
    let source = Source::open(std::env::var("CLASSIC_AWB_SOURCE").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let neutral = estimate(&engine, &source, "Auto", &mut Profile::default())
        .unwrap()
        .neutral
        .unwrap();
    let receipt: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("CLASSIC_AWB_REFERENCE").unwrap()).unwrap(),
    )
    .unwrap();
    let expected: [f64; 3] = std::array::from_fn(|i| receipt["expected"][i].as_f64().unwrap());
    let controls = preparation(&source.metadata).unwrap().unwrap();
    for (label, actual) in [("camera", controls.camera), ("tone_matrix", controls.tone)] {
        for (i, value) in actual.into_iter().enumerate() {
            assert!(
                (value - receipt["controls"][label][i].as_f64().unwrap()).abs() < 1e-11,
                "{label} coefficient {i}"
            );
        }
    }
    assert!(
        (controls.brightness - receipt["controls"]["brightness"].as_f64().unwrap()).abs() < 1e-6,
        "brightness {} vs {}",
        controls.brightness,
        receipt["controls"]["brightness"]
    );
    let activity = engine.gpu_activity();
    assert_eq!(
        estimate(&engine, &source, "Auto", &mut Profile::default())
            .unwrap()
            .neutral,
        Some(neutral)
    );
    let after = engine.gpu_activity();
    assert_eq!(after.dispatches, activity.dispatches);
    assert_eq!(after.readbacks, activity.readbacks);
    println!("neutral {neutral:?}, reference {expected:?}");
    for i in 0..3 {
        assert!((neutral[i] - expected[i]).abs() < 0.001);
    }
}
