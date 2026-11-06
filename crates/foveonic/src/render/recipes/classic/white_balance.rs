//! Prepares source-dependent neutral and channel-mean statistics for Classic white balance.
use super::reference::*;

use super::prepare;
use crate::{
    nodes::color::weighted_white_balance::{self, Prepared},
    profile::Profile,
    recipe::Key,
    render::{recipes::RecipeInput, source_preparation::StatisticKind, Engine},
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
    source: RecipeInput<'_>,
    white_balance: &str,
    profile: &mut Profile,
) -> Result<crate::render::WhiteBalanceStats> {
    let mut result = crate::render::WhiteBalanceStats::default();
    let auto = white_balance.eq_ignore_ascii_case("Auto");
    if prepare::capture_iso(source.metadata())? >= RANGE_RECONSTRUCTION_ISO
        && white_balance.eq_ignore_ascii_case("Incandescent")
    {
        let prepared = mean_preparation(source.metadata())?;
        result.mean = Some(engine.source_statistic(
            source,
            StatisticKind::ChannelMean,
            prepared.key(),
            |original| gray_mean(engine, original, &prepared, profile),
        )?);
    }
    if auto {
        if let Some(p) = preparation(source.metadata())? {
            result.neutral = Some(engine.source_statistic(
                source,
                StatisticKind::Neutral,
                neutral_key(&p),
                |original| {
                    let pixels = engine
                        .gpu
                        .upload("Original layers for white balance", &original.pixels)?;
                    weighted_white_balance::estimate(&engine.gpu, &pixels, &p, profile)
                },
            )?);
        }
    }
    Ok(result)
}

fn neutral_key(p: &Prepared) -> Key {
    Key::pod(&p.extent)
        .slice(&p.camera)
        .slice(&p.tone)
        .slice(&p.saturation)
        .with(&[p.brightness, p.scale])
        .with(&[
            u32::from(p.flash),
            u32::from(p.constrained),
            u32::from(p.estimate_drift),
        ])
        .with(&p.shields)
        .with(&p.drift)
        .with(&p.flare)
        .with(&p.polynomial)
}

struct MeanPreparation {
    extent: [u32; 2],
    shields: [[u32; 4]; 2],
    estimate_drift: bool,
    drift: [f32; 3],
}
impl MeanPreparation {
    fn key(&self) -> Key {
        Key::pod(&self.extent)
            .with(&self.shields)
            .with(&u32::from(self.estimate_drift))
            .with(&self.drift)
    }
}
fn mean_preparation(metadata: &Metadata) -> Result<MeanPreparation> {
    // The signed channel means are measured before white-balance gain.
    let prepared = prepare::prepare(metadata, "Sunlight")?;
    let p = &prepared.correction.params;
    let drift_value = |i: usize| p.drift[i / 2][(i % 2) * 2] + p.drift[i / 2][(i % 2) * 2 + 1];
    Ok(MeanPreparation {
        extent: [metadata.width as u32, metadata.height as u32],
        shields: p.shields,
        estimate_drift: p.drift_controls[2] != 0,
        drift: std::array::from_fn(|c| (drift_value(3 + c) + drift_value(9 + c)) * 0.5),
    })
}

fn gray_mean(
    engine: &Engine,
    source: &Source,
    prepared: &MeanPreparation,
    profile: &mut Profile,
) -> Result<[f64; 3]> {
    let pixels = engine
        .gpu
        .upload("Original layers for gray-world statistics", &source.pixels)?;
    weighted_white_balance::mean(
        &engine.gpu,
        &pixels,
        prepared.extent,
        prepared.shields,
        prepared.estimate_drift,
        prepared.drift,
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
    let neutral = estimate(
        &engine,
        RecipeInput::Original(&source),
        "Auto",
        &mut Profile::default(),
    )
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
        estimate(
            &engine,
            RecipeInput::Original(&source),
            "Auto",
            &mut Profile::default()
        )
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

#[test]
#[ignore = "requires GPU and CLASSIC_AWB_SOURCE"]
fn source_free_statistics_survive_engine_eviction_and_reject_changed_descriptors() {
    use crate::render::source_preparation::SourcePreparation;
    use std::sync::Arc;
    let source = Arc::new(Source::open(std::env::var_os("CLASSIC_AWB_SOURCE").unwrap()).unwrap());
    let weak = Arc::downgrade(&source);
    let metadata = source.metadata.clone();
    let origin = source.origin().clone();
    let prepared = SourcePreparation::default();
    let engine = Engine::new().unwrap();
    let original = estimate(
        &engine,
        RecipeInput::Preparing {
            source: &source,
            preparation: &prepared,
        },
        "Auto",
        &mut Profile::default(),
    )
    .unwrap();
    assert!(
        original.neutral.is_some(),
        "fixture must exercise the source-derived estimator"
    );
    drop(source);
    assert!(weak.upgrade().is_none());
    engine.clear_preparation();
    let before = engine.gpu_activity();
    let retained = estimate(
        &engine,
        RecipeInput::Metadata {
            metadata: &metadata,
            origin: &origin,
            preparation: &prepared,
        },
        "Auto",
        &mut Profile::default(),
    )
    .unwrap();
    assert_eq!(
        original.neutral.map(|v| v.map(f64::to_bits)),
        retained.neutral.map(|v| v.map(f64::to_bits))
    );
    assert_eq!(engine.gpu_activity().dispatches, before.dispatches);
    assert_eq!(engine.gpu_activity().readbacks, before.readbacks);
    let missing = SourcePreparation::default();
    let error = estimate(
        &engine,
        RecipeInput::Metadata {
            metadata: &metadata,
            origin: &origin,
            preparation: &missing,
        },
        "Auto",
        &mut Profile::default(),
    )
    .err()
    .expect("missing preparation needs native samples");
    assert!(error
        .downcast_ref::<crate::render::execute::SourceRequired>()
        .is_some());
    let mut changed = (*metadata).clone();
    changed
        .properties
        .insert("EXIF_EXPOSURE_BIAS".into(), "123".into());
    let changed = Arc::new(changed);
    let error = estimate(
        &engine,
        RecipeInput::Metadata {
            metadata: &changed,
            origin: &origin,
            preparation: &prepared,
        },
        "Auto",
        &mut Profile::default(),
    )
    .err()
    .expect("changed estimator descriptor needs materialization");
    assert!(error
        .downcast_ref::<crate::render::execute::SourceRequired>()
        .is_some());
    assert_eq!(engine.gpu_activity().dispatches, before.dispatches);
}
