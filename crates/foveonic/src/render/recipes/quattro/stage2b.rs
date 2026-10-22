//! Prepares ordinary HI reconstruction, luminance filtering and independent residual detail.

use crate::{
    graph::{Graph, Operation},
    nodes::color::{blue_guided_reconstruct, histogram_bilateral, residual_finish},
    source::{Metadata, Result},
};

// The ordinary helper replaces its blue variance model with these Double terms.
#[allow(clippy::approx_constant)] // Calibrated noise coefficient, not LOG10_E.
const BLUE_VARIANCE: [f64; 3] = [20.843, 0.4342, 8e-7];
const CHROMA_RESIDUAL_SUPPRESSION: f32 = 0.3;
const BASE_SHARP_THRESHOLD: f32 = 16.;
// Smooth/Crisp presets select these ordinary single-input residual powers.
const RESIDUAL_POWERS: [f32; 7] = [0., 0., 0., 0., 1., 1.25, 1.5];
const BLUE_FILTER_KERNEL: u32 = 11;
const BLUE_FILTER_RANGE_CUTOFF: f32 = 0.25;
const BLUE_FILTER_STRENGTHS: [f32; 5] = [0.2, 0.125, 0.1, 0.075, 0.05];
// Native Stage2B output-code normalization, independent of the sensor ISO gain.
const DP_BLUE_CODE_WHITE: f32 = 13000.;
const SD_BLUE_CODE_WHITE: f32 = 9000.;
const BLUE_BLEND_LENGTH: usize = 16384;
const BLUE_BLEND_KNOTS: [usize; 15] = [
    0, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384,
];
const BLUE_BLEND_POINTS: [[f64; 15]; 4] = [
    [
        0.33, 0.33, 0.33, 0.33, 0.33, 0.33, 0.33, 0.33, 0.34, 0.34, 0.36, 0.39, 0.43, 0.46, 0.46,
    ],
    [
        0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.51, 0.52, 0.53, 0.56, 0.6, 0.66, 0.7, 0.7,
    ],
    [
        0.62, 0.62, 0.62, 0.62, 0.62, 0.62, 0.62, 0.63, 0.64, 0.66, 0.69, 0.74, 0.82, 0.87, 0.87,
    ],
    [
        0.72, 0.72, 0.72, 0.72, 0.72, 0.72, 0.72, 0.73, 0.74, 0.76, 0.8, 0.86, 0.94, 1., 1.,
    ],
];

/// The native luminance presets and an independent residual-detail multiplier.
#[derive(Clone, Copy, Debug)]
pub(super) struct Controls {
    pub luma: u8,
    pub detail: f32,
    pub residual_level: u8,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            luma: 0,
            detail: 1.,
            residual_level: 6,
        }
    }
}

pub(super) struct Prepared {
    pub reconstruct: blue_guided_reconstruct::Prepared,
    pub finish: residual_finish::Params,
}

/// Detail follows capture ISO independently of sensor scaling. Zero luminance
/// skips blue filtering for ordinary non-Portrait modes. Portrait retains its
/// separate adaptive blend even at zero.
pub(super) fn prepare(metadata: &Metadata, controls: Controls, mode: &str) -> Result<Prepared> {
    if !super::defects::supported(metadata) || metadata.processing_version.is_none_or(|v| v[0] != 3)
    {
        return Err("reconstruction preparation requires ordinary full-size Quattro input".into());
    }
    let high = [metadata.width as u32, metadata.height as u32];
    let capture_iso = metadata.scalar("CaptureISO")?;
    if !capture_iso.is_finite() || capture_iso <= 0. {
        return Err("invalid reconstruction capture ISO".into());
    }
    let iso_gain = (capture_iso / 100.) as f32;
    let (sharp_threshold, adaptive) = detail_controls(iso_gain, controls, mode == "Portrait")?;
    // Standard's zero-level adaptive blend discards the filtered plane.
    // Portrait retains a nonzero blend at level zero, so filtering is required.
    let blend = if controls.luma == 0 && mode != "Portrait" {
        None
    } else {
        Some(blue_blend_table(
            controls.luma,
            iso_gain,
            mode == "Portrait",
        ))
    };
    Ok(Prepared {
        reconstruct: prepare_reconstruction(metadata, controls.luma, blend)?,
        finish: residual_finish::Params::new(high, iso_gain, sharp_threshold, 0, adaptive, 1.)?,
    })
}

/// A supplied blend table enables blue filtering independently of the slider's
/// numerical level. The caller selects the source response, including level zero.
pub(super) fn prepare_reconstruction(
    metadata: &Metadata,
    step: u8,
    blend: Option<Vec<f32>>,
) -> Result<blue_guided_reconstruct::Prepared> {
    let high = [metadata.width as u32, metadata.height as u32];
    let low = [high[0] / 2, high[1] / 2];
    let filter = blend
        .map(|table| {
            histogram_bilateral::Prepared::new(
                high,
                BLUE_FILTER_KERNEL,
                BLUE_VARIANCE,
                BLUE_FILTER_STRENGTHS[usize::from(step)],
                BLUE_FILTER_RANGE_CUTOFF,
                table,
                if super::calibration::is_ordinary_sd(metadata) {
                    SD_BLUE_CODE_WHITE
                } else {
                    DP_BLUE_CODE_WHITE
                },
            )
        })
        .transpose()?;
    blue_guided_reconstruct::Prepared::new(low, CHROMA_RESIDUAL_SUPPRESSION, BLUE_VARIANCE, filter)
}

fn blue_blend_table(step: u8, iso_gain: f32, portrait: bool) -> Vec<f32> {
    if iso_gain < 1. {
        // Below reference ISO the caller selects its constant-alpha blend.
        return vec![[0., 0.425, 0.55, 0.725, 0.9][usize::from(step)]; BLUE_BLEND_LENGTH];
    }
    // Portrait uses a separate blend shape. Its gain comes from the ordinary
    // HI level table, independent of capture ISO and residual-detail strength.
    let points = if portrait {
        let gain = f64::from([0.5_f32, 0.8, 1., 1.15, 1.4][usize::from(step)]);
        std::array::from_fn(|index| {
            (gain
                * if index < 6 {
                    0.7
                } else if index < 8 {
                    0.6
                } else {
                    0.5
                })
            .clamp(f64::from(0.1_f32), 1.)
        })
    } else {
        BLUE_BLEND_POINTS[usize::from(step - 1)]
    };
    interpolate_blend(points)
}

pub(super) fn interpolate_blend(points: [f64; 15]) -> Vec<f32> {
    let mut result = vec![0.; BLUE_BLEND_LENGTH];
    for segment in 0..14 {
        let start = BLUE_BLEND_KNOTS[segment];
        let end = BLUE_BLEND_KNOTS[segment + 1];
        let delta = (points[segment + 1] - points[segment]) as f32;
        let origin = points[segment] as f32;
        for (index, value) in result.iter_mut().enumerate().take(end).skip(start) {
            let position = ((index - start) as f64 / (end - start) as f64) as f32;
            *value = position.mul_add(delta, origin);
        }
    }
    result[BLUE_BLEND_LENGTH - 1] = points[14] as f32;
    result
}

fn detail_controls(iso_gain: f32, controls: Controls, portrait: bool) -> Result<(f32, f32)> {
    if !iso_gain.is_finite()
        || iso_gain <= 0.
        || controls.luma > 4
        || controls.residual_level > 6
        || !controls.detail.is_finite()
        || controls.detail < 0.
    {
        return Err("invalid reconstruction noise or detail controls".into());
    }
    // Ordinary HI and one input. Portrait softens residual restoration.
    let log_iso = iso_gain.log2();
    let mut sharp = ((log_iso * -15.) * log_iso) / 36. + BASE_SHARP_THRESHOLD;
    sharp *= [1., 1., 1.2, 1.5, 2.][usize::from(controls.luma)];
    if iso_gain >= 8. {
        sharp *= 1.5;
    }
    let ramp = (iso_gain - 1.).mul_add(-0.125, 1.);
    let adaptive_base = 0.3_f32.mul_add(ramp, 1.1).max(1.1);
    let mut power = RESIDUAL_POWERS[usize::from(controls.residual_level)] * controls.detail;
    if iso_gain > 8. || portrait {
        power *= 0.75;
    }
    let mut power = f64::from(power);
    if portrait || (iso_gain >= 8. && power > f64::from(1.3_f32)) {
        power /= f64::from(1.1401755_f32);
    }
    let base = if iso_gain > 16. {
        1.
    } else {
        f64::from(adaptive_base)
    };
    let adaptive = power.mul_add(base - 1., 1.) as f32;
    Ok((sharp, adaptive))
}

/// The caller supplies the current shaded planes; this builder never infers
/// which earlier cached image happens to own their storage.
pub(super) fn build(
    graph: &mut Graph,
    metadata: &Metadata,
    signals: &super::stage2::Signals,
    highlight_mask: usize,
    prepared: Prepared,
) -> Result<usize> {
    let reconstructed = graph.add(
        Operation::BlueGuidedReconstruct(std::sync::Arc::new(prepared.reconstruct)),
        &[
            signals.filtered,
            signals.calibrated,
            signals.processed,
            signals.blue,
        ],
    )?;
    let residual = graph.named_output(reconstructed, "reconstruction.residual")?;
    let mask = graph.add(
        Operation::ByteMaskDouble(crate::nodes::color::byte_mask_double::Params::new([
            (metadata.width / 2) as u32,
            (metadata.height / 2) as u32,
        ])?),
        &[highlight_mask],
    )?;
    graph.add(
        Operation::ResidualFinish {
            params: prepared.finish,
            with_mask: true,
        },
        &[reconstructed, residual, mask],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires QUATTRO_MODE_PARAMETERS with native color-mode preparation tables"]
    fn color_mode_controls_match_native_setters_and_tables() {
        let directory =
            std::path::PathBuf::from(std::env::var_os("QUATTRO_MODE_PARAMETERS").unwrap());
        let cases: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(directory.join("controls.json")).unwrap())
                .unwrap();
        for case in cases {
            let actual = detail_controls(
                case["iso"].as_f64().unwrap() as f32,
                Controls {
                    luma: case["luma"].as_u64().unwrap() as u8,
                    detail: 1.,
                    residual_level: case["residual_level"].as_u64().unwrap() as u8,
                },
                case["portrait"].as_bool().unwrap(),
            )
            .unwrap();
            assert_eq!(
                actual,
                (
                    case["sharp"].as_f64().unwrap() as f32,
                    case["adaptive"].as_f64().unwrap() as f32
                ),
                "{case}"
            );
        }
        for portrait in [false, true] {
            for step in u8::from(!portrait)..=4 {
                let table = blue_blend_table(step, 1., portrait);
                let expected = std::fs::read(
                    directory.join(format!("blend-{}-{step}.f32", u8::from(portrait))),
                )
                .unwrap();
                assert_eq!(bytemuck::cast_slice::<f32, u8>(&table), expected);
            }
        }
    }

    #[test]
    #[ignore = "requires QUATTRO_X3F, QUATTRO_STAGE2B_CONTROLS and optional QUATTRO_LUMA/QUATTRO_DETAIL"]
    fn selected_luminance_and_detail_match_native_controls() {
        let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
        let directory =
            std::path::PathBuf::from(std::env::var_os("QUATTRO_STAGE2B_CONTROLS").unwrap());
        let controls = Controls {
            luma: std::env::var("QUATTRO_LUMA").map_or(0, |v| v.parse().unwrap()),
            detail: std::env::var("QUATTRO_DETAIL").map_or(1., |v| v.parse().unwrap()),
            residual_level: std::env::var("QUATTRO_RESIDUAL_LEVEL")
                .map_or(6, |v| v.parse().unwrap()),
        };
        let mode = std::env::var("QUATTRO_MODE").unwrap_or_else(|_| "Standard".into());
        let prepared = prepare(&source.metadata, controls, &mode).unwrap();
        let bytes = std::fs::read(directory.join("parameters-entry.bin")).unwrap();
        let field = |offset| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let adaptive = std::fs::read(directory.join("apply-adaptive.bin")).unwrap();
        let adaptive: Vec<_> = adaptive
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| f32::from_le_bytes(*v))
            .collect();
        assert!(adaptive.iter().all(|v| *v == adaptive[0]));
        let data = std::fs::read(directory.join("apply-residual-data.bin")).unwrap();
        let mask = f32::from_le_bytes(data[0x28..0x2c].try_into().unwrap());
        let native = residual_finish::Params::new(
            [source.metadata.width as u32, source.metadata.height as u32],
            field(0x3c),
            field(0xf0),
            0,
            adaptive[0],
            mask,
        )
        .unwrap();
        assert!(prepared.finish.key() == native.key());
    }
    #[test]
    #[ignore = "requires QUATTRO_X3F ISO800 original and QUATTRO_STAGE2B_CONTROLS native metadata capture"]
    fn original_high_iso_detail_controls_match_native_request() {
        let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
        let directory =
            std::path::PathBuf::from(std::env::var_os("QUATTRO_STAGE2B_CONTROLS").unwrap());
        let bytes = std::fs::read(directory.join("parameters-entry.bin")).unwrap();
        let field = |offset| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        assert_eq!(
            field(0x3c),
            (source.metadata.scalar("CaptureISO").unwrap() / 100.) as f32
        );
        assert_ne!(
            field(0x3c),
            super::super::calibration::normalization(&source.metadata).unwrap()
        );
        let (sharp, adaptive) = detail_controls(field(0x3c), Controls::default(), false).unwrap();
        assert_eq!(sharp, field(0xf0));
        let expected = std::fs::read(directory.join("apply-adaptive.bin")).unwrap();
        for value in expected.as_chunks::<4>().0 {
            assert_eq!(adaptive, f32::from_le_bytes(*value));
        }
        let data = std::fs::read(directory.join("apply-residual-data.bin")).unwrap();
        let mask_control = f32::from_le_bytes(data[0x28..0x2c].try_into().unwrap());
        let native_scale = f32::from_le_bytes(data[0x38..0x3c].try_into().unwrap());
        assert_eq!(native_scale, field(0x3c));
        let native = residual_finish::Params::new(
            [source.metadata.width as u32, source.metadata.height as u32],
            native_scale,
            sharp,
            0,
            adaptive,
            mask_control,
        )
        .unwrap();
        assert!(
            prepare(&source.metadata, Controls::default(), "Standard")
                .unwrap()
                .finish
                .key()
                == native.key()
        );
    }
    #[test]
    fn detail_controls_follow_capture_iso_and_preserve_the_unit_route() {
        let unit_adaptive = (f64::from(0.3_f32.mul_add(1., 1.1)) - 1.).mul_add(1.5, 1.) as f32;
        assert_eq!(
            detail_controls(1., Controls::default(), false).unwrap(),
            (16., unit_adaptive)
        );
        // Independent actual ISO800 request: SensorISO400, CaptureISO800.
        assert_eq!(
            detail_controls(8., Controls::default(), false).unwrap(),
            (18.375, 1.1808933)
        );
        assert_eq!(
            detail_controls(32., Controls::default(), false).unwrap().1,
            1.
        );
        assert!(detail_controls(0., Controls::default(), false).is_err());
    }
    #[test]
    fn luminance_and_residual_detail_have_independent_controls() {
        // The independent ordinary HI setter fixture at ISO gain one and
        // half-position luminance selects step two and threshold 19.2.
        let selected = Controls {
            luma: 2,
            detail: 1.,
            residual_level: 6,
        };
        let (threshold, adaptive) = detail_controls(1., selected, false).unwrap();
        assert_eq!(threshold, 19.2);
        let without_detail = detail_controls(
            1.,
            Controls {
                detail: 0.,
                ..selected
            },
            false,
        )
        .unwrap();
        assert_eq!(without_detail, (threshold, 1.));
        assert!(adaptive > without_detail.1);
        assert!(detail_controls(
            1.,
            Controls {
                luma: 5,
                detail: 1.,
                residual_level: 6,
            },
            false,
        )
        .is_err());
        assert!(detail_controls(
            1.,
            Controls {
                luma: 0,
                detail: f32::NAN,
                residual_level: 6,
            },
            false,
        )
        .is_err());
    }
    #[test]
    #[ignore = "requires QUATTRO_X3F and STAGE2B_NATIVE_BOUNDARY with apply inputs"]
    fn ordinary_defaults_match_native_controls() {
        let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
        let mode = std::env::var("QUATTRO_MODE").unwrap_or_else(|_| "Standard".into());
        let prepared = prepare(&source.metadata, Controls::default(), &mode).unwrap();
        let directory =
            std::path::PathBuf::from(std::env::var_os("STAGE2B_NATIVE_BOUNDARY").unwrap());
        let parameters = std::fs::read(directory.join("parameters-entry.bin")).unwrap();
        let field = |o| f32::from_le_bytes(parameters[o..o + 4].try_into().unwrap());
        assert_eq!(field(0x68), 0., "zero blue-noise blend");
        let adaptive = std::fs::read(directory.join("apply-adaptive.f32")).unwrap();
        let adaptive = adaptive
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| f32::from_le_bytes(*v))
            .collect::<Vec<_>>();
        assert!(adaptive.iter().all(|v| *v == adaptive[0]));
        let data = std::fs::read(directory.join("apply-data.bin")).unwrap();
        let actual = residual_finish::Params::new(
            [5888, 3672],
            field(0x3c),
            field(0xf0),
            0,
            adaptive[0],
            f32::from_le_bytes(data[0x28..0x2c].try_into().unwrap()),
        )
        .unwrap();
        assert!(prepared.finish.key() == actual.key());
    }
}
