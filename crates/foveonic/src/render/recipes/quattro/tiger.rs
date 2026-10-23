//! Prepares Tiger's gain-bearing calibration and camera-specific numerical contracts.

use crate::source::{Metadata, Result};

const CAMERA_ID: f64 = 41.;
const SENSOR_EXTENT: [usize; 2] = [6656, 4480];
const PROCESSING_MAJOR: u16 = 4;

pub(super) fn supported(metadata: &Metadata) -> bool {
    metadata
        .processing_version
        .is_some_and(|v| v[0] == PROCESSING_MAJOR)
        && metadata.scalar("CAMERAID").is_ok_and(|id| id == CAMERA_ID)
        && [metadata.width, metadata.height] == SENSOR_EXTENT
}

/// These request gains are independent of sensor-adjustment shading gains.
pub(super) fn digital_gains(metadata: &Metadata) -> Result<[f64; 3]> {
    let gains = if metadata.matrix("DigitalISOGain").is_some() {
        metadata.fixed::<3>("DigitalISOGain")?
    } else {
        [1.; 3]
    };
    if gains
        .iter()
        .any(|g| !g.is_finite() || *g <= 0. || !(*g as f32).is_finite())
    {
        return Err("invalid Tiger digital ISO gains".into());
    }
    Ok(gains)
}

// Native full-frame phase-detection sites. The 26 knots expand every 16 samples.
const GAIN_KNOTS: usize = 26;
const GAIN_INTERVAL: usize = 16;
const CORRECTION_COLUMNS: u32 = 388;
const CORRECTION_ROWS: u32 = 104;
const CHROMA_GRID: [u32; 4] = [116, 296, 8, 16];
const LUMA_GRID: [u32; 4] = [233, 592, 16, 32];
const ANGLE_HEIGHT_INTERVAL: f32 = 10.;
const ANGLE_UNITS_PER_DEGREE: f32 = 512.;

fn expand(knots: &[f32]) -> Result<Vec<f32>> {
    if knots.len() != GAIN_KNOTS || knots.iter().any(|v| !v.is_finite()) {
        return Err("invalid Tiger phase-detection knots".into());
    }
    Ok((0..CORRECTION_COLUMNS as usize)
        .map(|i| {
            let index = i / GAIN_INTERVAL;
            let remainder = i % GAIN_INTERVAL;
            if remainder == 0 {
                knots[index]
            } else {
                let delta = knots[index + 1] - knots[index];
                let product = delta * remainder as f32;
                knots[index] + product / GAIN_INTERVAL as f32
            }
        })
        .collect())
}

pub(super) fn chroma(metadata: &Metadata) -> Result<crate::nodes::sensor::fppd::Prepared> {
    let mut gains = Vec::new();
    for channel in ['R', 'G', 'B'] {
        let knots = metadata.fixed::<GAIN_KNOTS>(&format!("StillFPPDGain_{channel}"))?;
        gains.extend(expand(&knots.map(|v| v as f32))?);
    }
    crate::nodes::sensor::fppd::Prepared::new(
        [(metadata.width / 2) as u32, (metadata.height / 2) as u32],
        CHROMA_GRID,
        [CORRECTION_COLUMNS, CORRECTION_ROWS],
        gains,
        false,
    )
}

pub(super) fn luma(metadata: &Metadata) -> Result<crate::nodes::sensor::fppd::Prepared> {
    let heights = metadata.fixed::<GAIN_KNOTS>("FPPDTargetImageHeight")?;
    let mut angles = [[0_f32; GAIN_KNOTS]; 2];
    for (side, name) in ["UpperRayAngleForInCamera", "LowerRayAngleForInCamera"]
        .into_iter()
        .enumerate()
    {
        let source = metadata
            .matrix(name)
            .ok_or_else(|| format!("missing {name}"))?;
        for (knot, &height) in heights.iter().enumerate() {
            let height = height as f32;
            if !height.is_finite() || height <= 0. {
                return Err("invalid Tiger phase-detection target height".into());
            }
            let index = (height / ANGLE_HEIGHT_INTERVAL).ceil() as usize - 1;
            let low = *source
                .values
                .get(index)
                .ok_or("short Tiger ray-angle table")? as f32;
            let high = *source
                .values
                .get(index + 1)
                .ok_or("short Tiger ray-angle table")? as f32;
            let fraction = height - index as f32 * ANGLE_HEIGHT_INTERVAL;
            let increment = (fraction * (high - low)) / ANGLE_HEIGHT_INTERVAL;
            angles[side][knot] = (increment + low) * (1. / ANGLE_UNITS_PER_DEGREE);
        }
    }
    let mut gains = Vec::new();
    for phase in ["B1", "B3"] {
        let table = metadata.fixed::<{ GAIN_KNOTS * 3 }>(&format!("StillFPPDGain_{phase}"))?;
        let curve = std::array::from_fn::<_, GAIN_KNOTS, _>(|i| {
            let lower = table[GAIN_KNOTS + i] as f32 * angles[1][i];
            let mixed = (table[i] as f32).mul_add(angles[0][i], lower);
            mixed + table[GAIN_KNOTS * 2 + i] as f32
        });
        gains.extend(expand(&curve)?);
    }
    let extent = [metadata.width as u32, metadata.height as u32];
    let severity = super::defects::severity(metadata)?.image;
    let defects = super::defects::expand_records(metadata, "BadPixelsLumaF23")?
        .into_iter()
        .filter_map(|[y, x, flags]| {
            (y > 1
                && y < extent[1] - 2
                && x > 1
                && x < extent[0] - 1
                && severity <= ((flags >> 8) & 7))
                .then_some(y * extent[0] + x)
        })
        .collect();
    crate::nodes::sensor::fppd::Prepared::new(
        extent,
        LUMA_GRID,
        [CORRECTION_COLUMNS, CORRECTION_ROWS],
        gains,
        true,
    )?
    .with_defects(defects)
}

const CHANNEL_HIGHLIGHT_CEILINGS: [f32; 3] = [22000., 24000., 14000.];
const EXTENDED_GREEN_LEVEL: f32 = 24000.;
const ORDINARY_GREEN_LEVEL: f32 = 13000.;
const ORDINARY_HIGHLIGHT_CEILING: f32 = 14000.;

pub(super) fn highlight_limits(
    metadata: &Metadata,
    prepared: crate::nodes::sensor::highlight_restoration::Prepared,
) -> Result<crate::nodes::sensor::highlight_restoration::Prepared> {
    let normalization = super::calibration::normalization(metadata)?;
    let extend = digital_gains(metadata)?.iter().any(|&v| v > 1.);
    let (ceilings, green) = if normalization == 1. && extend {
        (CHANNEL_HIGHLIGHT_CEILINGS, EXTENDED_GREEN_LEVEL)
    } else {
        (
            [ORDINARY_HIGHLIGHT_CEILING / normalization; 3],
            ORDINARY_GREEN_LEVEL / normalization,
        )
    };
    prepared.with_channel_limits(ceilings, green, extend)
}

pub(super) fn masked_highlights(
    prepared: crate::nodes::color::raw_color_blend::Prepared,
    strength: f32,
) -> Result<crate::nodes::color::raw_color_blend::Prepared> {
    prepared.with_masked_highlights(CHANNEL_HIGHLIGHT_CEILINGS, strength)
}

// Native color Stage1 uses these fixed preservation weights with nonunit request gains.
const COLOR_HIGHLIGHT_PRESERVATION: [f32; 3] = [0.6, 0.625, 1.];
pub(super) fn stage1_preservation(metadata: &Metadata) -> Result<[f32; 3]> {
    Ok(if digital_gains(metadata)?.iter().any(|&v| v != 1.) {
        COLOR_HIGHLIGHT_PRESERVATION
    } else {
        [1.; 3]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires TIGER_X3F and TIGER_FPPD_REFERENCE"]
    fn original_phase_detection_tables_match_independent_camera_equations() {
        let source = crate::source::Source::open(std::env::var_os("TIGER_X3F").unwrap()).unwrap();
        let root = std::path::PathBuf::from(std::env::var_os("TIGER_FPPD_REFERENCE").unwrap());
        assert!(supported(&source.metadata));
        for (name, prepared) in [
            ("chroma", chroma(&source.metadata).unwrap()),
            ("luma", luma(&source.metadata).unwrap()),
        ] {
            assert_eq!(
                bytemuck::cast_slice::<f32, u8>(prepared.gains()),
                std::fs::read(root.join(format!("{name}.bin"))).unwrap(),
                "{name}"
            );
        }
    }

    #[test]
    #[ignore = "requires GPU and TIGER_X3F metadata"]
    fn tiger_fast_blue_repairs_phase_detection_pairs_before_reduction() {
        use crate::{
            profile::Profile,
            render::{execute::Input, Engine, PreviewQuality, Settings},
            source::Source,
        };
        use std::sync::Arc;

        let engine = Engine::new().unwrap();
        let mut source = Source::open(std::env::var_os("TIGER_X3F").unwrap()).unwrap();
        // Isolate phase-detection sites from unrelated factory defects in a flat field.
        Arc::make_mut(&mut source.metadata)
            .matrices
            .remove("BadPixelsLumaF23");
        let offset = source.metadata.scalar("RawOffset").unwrap() as f32;
        let flat_code = offset + 1000.;
        for cell in source.pixels.as_chunks_mut::<6>().0 {
            cell[2..].fill(flat_code);
        }
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.preview_quality = PreviewQuality::Fast;
        let blue_for_reduction = |source: &Source| {
            let built = super::super::build(
                &engine,
                source,
                &settings,
                &["final"],
                &mut crate::interactive::Execution::default(),
            )
            .unwrap();
            let reconstruction = built
                .graph
                .nodes
                .iter()
                .find(|node| {
                    matches!(
                        node.operation,
                        crate::graph::Operation::BlueGuidedReconstruct(_)
                    )
                })
                .unwrap();
            let blue = engine
                .execute(
                    Input::Source(source),
                    &built.graph,
                    &[reconstruction.inputs[3]],
                    &mut Profile::default(),
                )
                .unwrap();
            engine.gpu.readback(&blue[0].buffer).unwrap()
        };
        let expected = blue_for_reduction(&source);
        // Native paired-site grid: origin (233,592), spacing (16,32).
        // Test dark and bright defects at the start, middle and end of the grid.
        for ([column, row], error) in [([1, 1], 100.), ([100, 40], 4000.), ([387, 103], 100.)] {
            let x = 233 + column * 16;
            let y = 592 + row * 32;
            for y in [y, y + 1] {
                let cell = (y / 2 * (source.metadata.width / 2) + x / 2) * 6;
                let phase = 2 + (y % 2) * 2 + x % 2;
                source.pixels[cell + phase] = offset + error;
            }
        }
        assert!(
            blue_for_reduction(&source) == expected,
            "phase-detection errors survived into Fast blue reduction"
        );
    }
}
