//! Selects Classic camera highlight controls before an architecture-unaware row stream.
use super::reference::*;

use crate::{
    nodes::color::highlight_stream::Prepared,
    source::{Metadata, Result},
};

fn scalar(metadata: &Metadata, name: &str, fallback: f64) -> Result<f64> {
    if metadata.matrix(name).is_some() {
        metadata.scalar(name)
    } else {
        Ok(fallback)
    }
}

pub(crate) fn prepare(metadata: &Metadata, final_gain: [f64; 3]) -> Result<Option<Prepared>> {
    if scalar(metadata, "SkipStepFlags", 0.)? as u32 & (1 << 11) != 0 {
        return Ok(None);
    }
    let model = metadata
        .properties
        .get("CAMMODEL")
        .ok_or("missing camera model")?
        .trim()
        .to_ascii_uppercase();
    let requested_iso: f64 = metadata
        .properties
        .get("ISO")
        .ok_or("missing ISO")?
        .parse()?;
    let capture_iso = scalar(metadata, "CaptureISO", requested_iso)?;
    let iso = if capture_iso == 0. {
        requested_iso
    } else {
        capture_iso
    } as f32;
    let sensor = scalar(metadata, "SensorISO", 100.)? as f32;
    let sensor = if sensor == 0. { 100. } else { sensor };
    let ratio = iso / sensor;
    let bulb_frames = scalar(metadata, "BulbFrameCount", 0.)?;
    if bulb_frames > 1. {
        return Err("Classic bulb highlight preparation is outside the selected experiment".into());
    }
    let gain = ratio.min(2.);
    let units = super::policy::capture_shutter_units(&model);
    let mut shutter = scalar(metadata, "CaptureShutter", 0.)? / units;
    if shutter == 0. {
        // The native caller falls back to the recorded exposure. The shared
        // metadata property stores that exposure in seconds.
        shutter = if let Some(value) = metadata.properties.get("SHUTTER") {
            value.parse()?
        } else if let Some(value) = metadata.properties.get("EXPTIME") {
            // Early bodies may store only the exposure in microseconds.
            // Prefer this measured duration to parsing the display label.
            value.parse::<f64>()? / MICROSECONDS_PER_SECOND
        } else {
            return Err("missing recorded exposure for Classic highlight preparation".into());
        };
        if shutter == 0. {
            shutter = 1.;
        }
    }
    let statistics = if scalar(metadata, "CCMatrix_Flag", 0.)? == 1. {
        1.3
    } else {
        1.
    };
    let factor = match model.as_str() {
        "SIGMA SD9" | "SD9" | "SIGMA SD10" | "SD10" => 1.2,
        "SIGMA SD14" | "SD14" => 1.,
        "SIGMA SD15" | "SD15" => 0.8,
        "SIGMA DP2" | "DP2" | "SIGMA DP2S" | "DP2S" | "SIGMA DP2X" | "DP2X" => {
            if ratio < 1. {
                1.3
            } else {
                1.
            }
        }
        "SIGMA DP1" | "DP1" | "SIGMA DP1S" | "DP1S" | "SIGMA DP1X" | "DP1X" => {
            if shutter < 0.5 {
                statistics
            } else if shutter < 1.5 {
                (statistics - 1.) * (1.5 - shutter) + 1.
            } else {
                1.
            }
        }
        _ => return Err("unknown Classic camera highlight controls".into()),
    };
    let values = metadata
        .matrix("SaturationLevel")
        .filter(|m| m.values.len() == 3)
        .ok_or("missing three-channel saturation level")?;
    // The caller first scales and truncates to signed integers, then applies
    // its camera threshold adjustment with a separate rounding step.
    // final_gain already includes the first ISO gain; do not apply it twice.
    let pregain = (f64::from(sensor) / 100.).max(1.);
    let calibrated = std::array::from_fn(|c| (pregain * final_gain[c] * values.values[c]) as i32);
    let sat = adjusted_saturation(&model, calibrated, factor);
    let extent = [metadata.width as u32, metadata.height as u32];
    Ok(Some(Prepared::new(
        extent,
        [1.; 3],
        sat,
        shutter as f32,
        gain,
        false,
    )?))
}

// The earliest sensors lower their channel limits before the final threshold
// multiplier. Integer division keeps the reference's truncation toward zero.
fn adjusted_saturation(model: &str, mut levels: [i32; 3], factor: f64) -> [i32; 3] {
    match model {
        "SIGMA SD9" | "SD9" => {
            levels = levels.map(|v| v - v / 32);
            levels[0] -= levels[0] / 6;
        }
        "SIGMA SD10" | "SD10" => {
            levels = levels.map(|v| v - v / 64);
            levels[0] -= levels[0] / 8;
            levels[0] = levels[0].min(levels[1] - levels[1] / 16);
            levels[2] = levels[2].min(levels[1] + levels[1] / 16);
        }
        _ => {}
    }
    levels.map(|v| (f64::from(v) * factor + 0.5) as i32)
}

#[cfg(test)]
#[test]
fn legacy_thresholds_adjust_channels_before_scaling() {
    assert_eq!(
        adjusted_saturation("SIGMA SD9", [6400, 3200, 9600], 1.2),
        [6200, 3720, 11160]
    );
    assert_eq!(
        adjusted_saturation("SIGMA SD10", [6400, 3200, 9600], 1.2),
        [3545, 3780, 4015]
    );
    assert_eq!(
        adjusted_saturation("SIGMA SD14", [6400, 3200, 9600], 1.),
        [6400, 3200, 9600]
    );
}

#[cfg(test)]
#[test]
fn selected_sd15_metadata_reproduces_native_threshold_triples() {
    use crate::source::{Matrix, MatrixEncoding};
    use std::collections::BTreeMap;
    let mut metadata = Metadata {
        processing_version: None,
        width: 2688,
        height: 1792,
        rotation: 0,
        properties: BTreeMap::from([
            ("CAMMODEL".into(), "SIGMA SD15".into()),
            ("ISO".into(), "50".into()),
        ]),
        matrices: BTreeMap::new(),
        lists: BTreeMap::new(),
    };
    for (name, values) in [
        ("CaptureISO", vec![50.]),
        ("SensorISO", vec![64.76654052734375]),
        ("CaptureShutter", vec![3010.]),
        ("SaturationLevel", vec![7477., 7063., 6742.]),
    ] {
        metadata.matrices.insert(
            name.into(),
            Matrix {
                encoding: MatrixEncoding::Float32,
                representation: "native",
                dimensions: vec![values.len()],
                values,
            },
        );
    }
    let p = prepare(
        &metadata,
        [0.9180922243820624, 0.7721018791002336, 0.8447293243413362],
    )
    .unwrap()
    .unwrap();
    assert_eq!(p.extent, [2688, 1792]);
    assert_eq!(p.fix, [3926; 3]);
    assert_eq!(p.apriori, [4907; 3]);
    assert_eq!(p.scaled, [1365; 3]);
    assert!(!p.bulb);
    metadata.matrices.get_mut("CaptureISO").unwrap().values[0] = 0.;
    metadata.matrices.get_mut("CaptureShutter").unwrap().values[0] = 0.;
    metadata
        .properties
        .insert("SHUTTER".into(), "0.00301".into());
    let fallback = prepare(
        &metadata,
        [0.9180922243820624, 0.7721018791002336, 0.8447293243413362],
    )
    .unwrap()
    .unwrap();
    assert_eq!(p, fallback);
}

#[cfg(test)]
#[test]
fn early_cameras_keep_capture_shutter_in_seconds() {
    use crate::source::{Matrix, MatrixEncoding};
    let metadata = Metadata {
        processing_version: None,
        width: 2304,
        height: 1531,
        rotation: 0,
        properties: [
            ("CAMMODEL".into(), "SIGMA SD10".into()),
            ("ISO".into(), "100".into()),
        ]
        .into(),
        lists: Default::default(),
        matrices: [
            ("CaptureShutter", vec![4.]),
            ("SaturationLevel", vec![6400., 3200., 9600.]),
        ]
        .into_iter()
        .map(|(name, values)| {
            (
                name.into(),
                Matrix {
                    encoding: MatrixEncoding::Float32,
                    representation: "native",
                    dimensions: vec![values.len()],
                    values,
                },
            )
        })
        .collect(),
    };
    let actual = prepare(&metadata, [1.; 3]).unwrap().unwrap();
    let expected = Prepared::new([2304, 1531], [1.; 3], [3545, 3780, 4015], 4., 1., false).unwrap();
    assert_eq!(actual.fix, expected.fix);
    assert_eq!(actual.apriori, expected.apriori);
}
