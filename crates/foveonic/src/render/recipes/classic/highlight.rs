//! Selects Classic camera highlight controls before an architecture-unaware row stream.
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
    let mut shutter = scalar(metadata, "CaptureShutter", 0.)? / 1_000_000.;
    if shutter == 0. {
        // The native caller falls back to the recorded exposure. The shared
        // metadata property stores that exposure in seconds.
        shutter = metadata
            .properties
            .get("SHUTTER")
            .ok_or("missing recorded exposure for Classic highlight preparation")?
            .parse()?;
        if shutter == 0. {
            shutter = 1.;
        }
    }
    let statistics = if scalar(metadata, "CCMatrix_Flag", 0.)? == 1. {
        1.3
    } else {
        1.
    };
    let factor =
        match model.as_str() {
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
            _ => return Err(
                "Classic highlight preparation currently supports DP1/DP2/SD15 generation cameras"
                    .into(),
            ),
        };
    let values = metadata
        .matrix("SaturationLevel")
        .filter(|m| m.values.len() == 3)
        .ok_or("missing three-channel saturation level")?;
    // The caller first scales and truncates to signed integers, then applies
    // its camera threshold adjustment with a separate rounding step.
    // final_gain already includes the first ISO gain; do not apply it twice.
    let pregain = (f64::from(sensor) / 100.).max(1.);
    let sat = std::array::from_fn(|c| {
        let calibrated = (pregain * final_gain[c] * values.values[c]) as i32;
        (f64::from(calibrated) * factor + 0.5) as i32
    });
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

#[cfg(test)]
#[test]
fn selected_sd15_metadata_reproduces_native_threshold_triples() {
    use crate::source::{Matrix, MatrixEncoding};
    use std::collections::BTreeMap;
    let mut metadata = Metadata {
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
