//! Prepares monochrome channel filtering, reconstruction and finishing controls.

use crate::{
    nodes::{
        color::{channel_noise, chromatic_ratio, neighborhood_decorrelate, rgb_prefilter},
        sensor::blue_despeckle,
    },
    source::{Metadata, Result},
};

// The monochrome helper uses this quarter-scale camera-code noise model before
// color conversion. Rows are constant, linear and quadratic R/G/B coefficients.
const CHANNEL_NOISE_MODEL: [f64; 9] = [
    24.362, 15.16425, 1.3026875, 0.04835, 0.0275, 0.0271375, -1.25e-7, -5e-8, 5e-8,
];
#[allow(clippy::approx_constant)] // Calibrated noise coefficient, not LOG10_E.
const BLUE_SPECKLE_MODEL: [f32; 3] = [20.843, 0.4342, 8e-7];
const RANGE_WEIGHT_SCALE: f32 = 2047. / 8.;
const NEIGHBORHOOD_INTENSITY_ORIGIN: f32 = 0.02;
const NEIGHBORHOOD_INTENSITY_STEP: f32 = 6.812652e-5;
const NEIGHBORHOOD_INTENSITY_EXPONENT: f32 = 1.2;
const NEIGHBORHOOD_INTENSITY_LAST: f32 = f32::from_bits(0x3f7ffaa6);
const RANGE_RADIUS: u32 = 7;

pub(super) fn prepare_stage1(metadata: &Metadata) -> Result<channel_noise::Prepared> {
    let extent = super::calibration::ordinary_extent(metadata)?;
    let despeckle = blue_despeckle::Prepared::new(
        extent,
        BLUE_SPECKLE_MODEL,
        1.,
        0.,
        [0, 0, extent[0] - 1, extent[1] - 1],
        blue_despeckle::Stage::NeighborRange,
        blue_despeckle::MissingContext::Preserve,
    )?;
    // Channel-ratio repair uses the factory Auto estimate. The color route
    // independently uses fixed daylight for its early technical filter.
    let gains = super::calibration::white_balance(metadata, "Auto")?;
    let ratio = chromatic_ratio::Prepared::new(extent, gains, CHANNEL_NOISE_MODEL)?;
    let (_, range) = super::stage1::conditioning(0., false);
    let intensity = range_intensity();
    let decorrelate = neighborhood_decorrelate::Prepared::new(
        extent,
        CHANNEL_NOISE_MODEL,
        neighborhood_intensity(),
        range.clone(),
        RANGE_WEIGHT_SCALE,
    )?;
    let filter = rgb_prefilter::RangePrepared::new(
        extent,
        CHANNEL_NOISE_MODEL,
        intensity,
        range,
        RANGE_WEIGHT_SCALE,
        RANGE_RADIUS,
        None,
    )?;
    channel_noise::Prepared::new(extent, despeckle, ratio, decorrelate, filter)
}

/// Monochrome retains neutralization and chroma protection while bypassing the
/// adaptive camera matrix response. Its source helper uses factory Auto gains.
pub(super) fn prepare_stage2(
    metadata: &Metadata,
    highlight_amount: f32,
    mode: &str,
) -> Result<crate::nodes::color::raw_color_blend::Prepared> {
    Ok(super::stage2::prepare(metadata, highlight_amount, "Auto", mode)?.without_color_response())
}

/// These factors change only the coarsest local-contrast kernel.
pub(super) fn stage3_controls(
    contrast_level: usize,
    sharpen_level: usize,
    sharpen_base: f32,
) -> Result<super::stage3::Controls> {
    const LOWEST_BAND_FACTORS: [f32; 7] = [0.5333, 0.6667, 0.8667, 1., 1.2, 1.4, 1.6];
    let factor = *LOWEST_BAND_FACTORS
        .get(contrast_level)
        .ok_or("unsupported monochrome local contrast level")?;
    Ok(super::stage3::Controls {
        contrast_level,
        sharpen_level,
        sharpen_base,
        lowest_band_factor: Some(factor),
        preserve_dark_detail: false,
    })
}

/// Reconstruction and blue filtering are shared. The channel-mixed route keeps
/// its own ISO-dependent residual response instead of inheriting color limits.
pub(super) fn prepare_stage2b(
    metadata: &Metadata,
    controls: super::stage2b::Controls,
    mode: &str,
) -> Result<super::stage2b::Prepared> {
    let mut prepared = super::stage2b::prepare(metadata, controls, mode)?;
    let iso = (metadata.scalar("CaptureISO")? / 100.) as f32;
    let (sharp, adaptive) = residual_controls(iso, controls, mode == "Portrait")?;
    prepared.reconstruct = super::stage2b::prepare_reconstruction(
        metadata,
        controls.luma,
        Some(blue_blend(controls.luma, iso, mode == "Portrait")),
    )?;
    prepared.finish = crate::nodes::color::residual_finish::Params::new(
        [metadata.width as u32, metadata.height as u32],
        iso,
        sharp,
        0,
        adaptive,
        1.,
    )?;
    Ok(prepared)
}

fn blue_blend(step: u8, iso: f32, portrait: bool) -> Vec<f32> {
    if iso < 1. {
        return vec![[0., 0.425, 0.55, 0.725, 0.9][usize::from(step)]; 16384];
    }
    let gain = f64::from([0.5_f32, 0.8, 1., 1.15, 1.4][usize::from(step)]);
    let points = std::array::from_fn(|index| {
        let weight = if portrait {
            if index < 6 {
                0.7
            } else if index < 8 {
                0.6
            } else {
                0.5
            }
        } else if index < 8 {
            0.6
        } else if index < 10 {
            0.5
        } else {
            0.4
        };
        (gain * weight).clamp(f64::from(0.1_f32), 1.)
    });
    super::stage2b::interpolate_blend(points)
}

fn residual_controls(
    iso: f32,
    controls: super::stage2b::Controls,
    portrait: bool,
) -> Result<(f32, f32)> {
    const RESIDUAL_POWERS: [f32; 7] = [0., 0.1, 0.2, 0.3, 1., 1.25, 1.5];
    const NOISE_THRESHOLD_FACTORS: [f32; 5] = [1., 1., 1.2, 1.5, 2.];
    if !iso.is_finite()
        || iso <= 0.
        || controls.luma > 4
        || controls.residual_level > 6
        || !controls.detail.is_finite()
        || controls.detail < 0.
    {
        return Err("invalid channel-mixed reconstruction controls".into());
    }
    let log_iso = iso.log2();
    let sharp = (((log_iso * -15.) * log_iso) / 36. + 16.)
        * NOISE_THRESHOLD_FACTORS[usize::from(controls.luma)];
    let ramp = (iso - 1.).mul_add(-0.125, 1.);
    let base = 0.3_f32.mul_add(ramp, 1.1).max(1.1);
    let mut power = RESIDUAL_POWERS[usize::from(controls.residual_level)] * controls.detail;
    if portrait {
        power *= 0.8;
    }
    let power = if portrait {
        f64::from(power) / f64::from(1.1401755_f32)
    } else {
        f64::from(power)
    };
    Ok((sharp, power.mul_add(f64::from(base) - 1., 1.) as f32))
}

/// Shared local contrast receives numerical controls, while this recipe selects
/// the distinct channel-mixed adaptive sharpening response.
pub(super) fn build_stage3(
    graph: &mut crate::graph::Graph,
    metadata: &Metadata,
    rgb: usize,
    detail: usize,
    mode: &str,
    controls: super::stage3::Controls,
) -> Result<usize> {
    let contrast = super::stage3::build_contrast(graph, metadata, rgb, mode, controls)?;
    let prepared = prepare_sharpen(metadata, controls.sharpen_level, controls.sharpen_base)?;
    graph.add(
        crate::graph::Operation::ProportionalSharpen(std::sync::Arc::new(prepared)),
        &[contrast, detail],
    )
}

pub(super) fn prepare_sharpen(
    metadata: &Metadata,
    level: usize,
    base: f32,
) -> Result<crate::nodes::finish::proportional_sharpen::Prepared> {
    let extent = [metadata.width as u32, metadata.height as u32];
    let normalization = (metadata.scalar("CaptureISO")? / 100.) as f32;
    crate::nodes::finish::proportional_sharpen::Prepared::new(
        extent,
        sharpening_table(level, base)?,
        normalization,
        2000.,
        0.2,
        1.,
        false,
    )
}

fn sharpening_table(level: usize, base: f32) -> Result<Vec<f32>> {
    // Source rows cover smoothness levels zero through three, four, five and six.
    const KNOTS: [[f64; 15]; 4] = [
        [
            1., 1., 1., 1., 1., 1., 1., 1.1, 1.2, 1.3, 1.3, 1.3, 1.3, 1.3, 1.1,
        ],
        [
            1., 1., 1., 1., 1., 1., 1., 1.1, 1.2, 1.5, 1.5, 1.5, 1.5, 1.5, 1.2,
        ],
        [
            1., 1., 1., 1., 1., 1., 1.1, 1.2, 1.3, 1.6, 1.7, 1.7, 1.7, 1.7, 1.4,
        ],
        [
            1., 1., 1., 1., 1., 1., 1.1, 1.2, 1.3, 1.7, 2., 2., 2., 2., 1.7,
        ],
    ];
    const POSITIONS: [usize; 15] = [
        0, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384,
    ];
    if level > 6 || !base.is_finite() || !(-2. ..=2.).contains(&base) {
        return Err("invalid channel-mixed sharpening controls".into());
    }
    let mut knots = KNOTS[level.saturating_sub(3)];
    let base = f64::from(base);
    let tail = knots[14];
    for value in &mut knots[..14] {
        *value = if base < 0. {
            *value * 1.15 * base.mul_add(0.5, 1.)
        } else {
            2_f64.mul_add(base, *value * 1.15)
        };
    }
    knots[14] = if base < 0. {
        base.mul_add(0.5, 1.) * tail * 0.5
    } else {
        2_f64.mul_add(base, tail * 0.5)
    };
    let mut table = vec![0.; 16384];
    for segment in 0..14 {
        let first = POSITIONS[segment];
        let end = POSITIONS[segment + 1];
        let delta = (knots[segment + 1] - knots[segment]) as f32;
        let origin = knots[segment] as f32;
        for (index, value) in table.iter_mut().enumerate().take(end).skip(first) {
            let t = ((index - first) as f64 / (end - first) as f64) as f32;
            *value = t.mul_add(delta, origin);
        }
    }
    table[16383] = knots[14] as f32;
    Ok(table)
}

fn range_intensity() -> Vec<f32> {
    let mut table: Vec<_> = (0..14384)
        .map(|index| {
            let x = (((index as f32).mul_add(0.0021551724, 1.)) - 1.) / 31.;
            31_f64.mul_add(f64::from(x).powf(4.), 1.) as f32
        })
        .collect();
    table.push(32.);
    table
}

fn neighborhood_intensity() -> Vec<f32> {
    let mut table: Vec<_> = (0..14384)
        .map(|index| {
            let value =
                (index as f32).mul_add(NEIGHBORHOOD_INTENSITY_STEP, NEIGHBORHOOD_INTENSITY_ORIGIN);
            value.powf(NEIGHBORHOOD_INTENSITY_EXPONENT)
        })
        .collect();
    table.push(NEIGHBORHOOD_INTENSITY_LAST);
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditioning_tables_retain_native_domain_and_endpoints() {
        let intensity = neighborhood_intensity();
        assert_eq!(intensity.len(), 14385);
        assert_eq!(intensity[0], 0.02_f32.powf(1.2));
        assert_eq!(intensity[14384].to_bits(), 0x3f7ffaa6);
        assert!(intensity.windows(2).all(|v| v[0] <= v[1]));
    }

    #[test]
    #[ignore = "requires QUATTRO_MONO_CONTROLS captured native lookup tables"]
    fn generated_conditioning_matches_native_tables() {
        let root = std::path::PathBuf::from(std::env::var_os("QUATTRO_MONO_CONTROLS").unwrap());
        let (_, range) = super::super::stage1::conditioning(0., false);
        let intensity = range_intensity();
        for (name, actual) in [
            ("ndcr-intensity.f32", neighborhood_intensity()),
            ("filter-intensity.f32", intensity),
            ("filter-range.f32", range),
        ] {
            let expected = std::fs::read(root.join(name)).unwrap();
            let expected: &[f32] = bytemuck::cast_slice(&expected);
            let different = actual
                .iter()
                .zip(expected)
                .filter(|(a, b)| a.to_bits() != b.to_bits())
                .count();
            let maximum = actual
                .iter()
                .zip(expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0_f32, f32::max);
            assert!(
                different == 0,
                "{name}: {different} different, maximum {maximum}"
            );
        }
    }

    #[test]
    #[ignore = "requires a GPU, QUATTRO_X3F and QUATTRO_MONO_STAGE1 owned native boundary"]
    fn original_channel_filter_matches_native_stage1() {
        let root = std::path::PathBuf::from(std::env::var_os("QUATTRO_MONO_STAGE1").unwrap());
        let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
        let input = std::fs::read(root.join("input-rgb.f32")).unwrap();
        let expected = std::fs::read(root.join("output-rgb.f32")).unwrap();
        assert_eq!(
            input,
            std::fs::read(root.join("source-after-rgb.f32")).unwrap()
        );
        let gpu = crate::gpu::Gpu::new().unwrap();
        let image = gpu
            .upload(
                "Native monochrome calibrated RGB",
                bytemuck::cast_slice::<u8, f32>(&input),
            )
            .unwrap();
        let prepared = prepare_stage1(&source.metadata).unwrap();
        let profile_path = std::env::var_os("QUATTRO_MONO_STAGE1_PROFILE");
        let repeats = if profile_path.is_some() { 5 } else { 1 };
        let mut timings = Vec::new();
        let mut captured = None;
        for iteration in 0..repeats {
            let mode = if profile_path.is_some() {
                crate::profile::Mode::Gpu
            } else {
                crate::profile::Mode::Off
            };
            let mut profile = crate::profile::Profile::new(mode);
            let start = std::time::Instant::now();
            let output = channel_noise::apply(&gpu, &image, &prepared, &mut profile).unwrap();
            let actual = gpu.readback(&output.rgb).unwrap();
            timings.push(serde_json::json!({
                "iteration": iteration,
                "completed_with_readback_ms": start.elapsed().as_secs_f64() * 1000.,
                "profile": profile,
            }));
            captured = Some((output, actual));
        }
        if let Some(path) = profile_path {
            std::fs::write(path, serde_json::to_vec_pretty(&timings).unwrap()).unwrap();
        }
        let (output, actual) = captured.unwrap();
        if let Some(path) = std::env::var_os("QUATTRO_MONO_STAGE1_OUTPUT") {
            std::fs::write(path, &actual).unwrap();
        }
        let actual: &[f32] = bytemuck::cast_slice(&actual);
        let expected: &[f32] = bytemuck::cast_slice(&expected);
        let maximum = actual
            .iter()
            .zip(expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        let mean = actual
            .iter()
            .zip(expected)
            .map(|(a, b)| f64::from((a - b).abs()))
            .sum::<f64>()
            / actual.len() as f64;
        eprintln!("monochrome Stage1 maximum={maximum} mean={mean}");
        assert!(
            maximum <= 0.6 && mean <= 0.0001,
            "maximum={maximum} mean={mean}"
        );
        assert_eq!(gpu.readback(&output.source_after).unwrap(), input);
    }

    #[test]
    #[ignore = "requires a GPU, QUATTRO_X3F and QUATTRO_MONO_IMAGES owned native internal boundaries"]
    fn original_decorrelation_and_range_boundaries_match_native() {
        let root = std::path::PathBuf::from(std::env::var_os("QUATTRO_MONO_IMAGES").unwrap());
        let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
        let extent = super::super::calibration::ordinary_extent(&source.metadata).unwrap();
        let gpu = crate::gpu::Gpu::new().unwrap();
        let (_, range) = super::super::stage1::conditioning(0., false);
        let decorrelate = neighborhood_decorrelate::Prepared::new(
            extent,
            CHANNEL_NOISE_MODEL,
            neighborhood_intensity(),
            range.clone(),
            RANGE_WEIGHT_SCALE,
        )
        .unwrap();
        let filter = rgb_prefilter::RangePrepared::new(
            extent,
            CHANNEL_NOISE_MODEL,
            range_intensity(),
            range,
            RANGE_WEIGHT_SCALE,
            RANGE_RADIUS,
            None,
        )
        .unwrap();
        for (name, input_name, output_name) in [
            ("decorrelate", "ndcr-input.f32", "ndcr-output.f32"),
            (
                "range",
                "filter-input.f32",
                "../monochrome-stage1/output-rgb.f32",
            ),
        ] {
            let input = std::fs::read(root.join(input_name)).unwrap();
            let expected = std::fs::read(root.join(output_name)).unwrap();
            let image = gpu
                .upload(
                    "Owned native boundary",
                    bytemuck::cast_slice::<u8, f32>(&input),
                )
                .unwrap();
            let result = if name == "decorrelate" {
                neighborhood_decorrelate::apply(&gpu, &image, &decorrelate, &mut Default::default())
                    .unwrap()
            } else {
                rgb_prefilter::apply_range(&gpu, &image, &filter, &mut Default::default()).unwrap()
            };
            let actual = gpu.readback(&result).unwrap();
            if let Some(output) = std::env::var_os("QUATTRO_MONO_BOUNDARY_OUTPUT") {
                std::fs::write(
                    std::path::PathBuf::from(output).join(format!("gpu-{name}.f32")),
                    &actual,
                )
                .unwrap();
            }
            let actual: &[f32] = bytemuck::cast_slice(&actual);
            let expected: &[f32] = bytemuck::cast_slice(&expected);
            let maximum = actual
                .iter()
                .zip(expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0_f32, f32::max);
            let mean = actual
                .iter()
                .zip(expected)
                .map(|(a, b)| f64::from((a - b).abs()))
                .sum::<f64>()
                / actual.len() as f64;
            eprintln!("{name}: maximum={maximum}, mean={mean}");
            if name == "decorrelate" {
                assert!(maximum <= 3. && mean <= 0.000003);
            } else {
                assert!(maximum <= 0.00004 && mean <= 1e-9);
            }
        }
    }

    #[test]
    #[ignore = "requires QUATTRO_MONO_STAGE3 captured native adaptive sharpening table"]
    fn channel_mixed_sharpening_matches_native_lookup() {
        let root = std::path::PathBuf::from(std::env::var_os("QUATTRO_MONO_STAGE3").unwrap());
        let actual = sharpening_table(6, 0.).unwrap();
        let expected = std::fs::read(root.join("sharp-lut.f32")).unwrap();
        assert_eq!(bytemuck::cast_slice::<f32, u8>(&actual), expected);
    }

    #[test]
    fn channel_mixed_residuals_keep_their_high_iso_response() {
        let controls = super::super::stage2b::Controls {
            luma: 2,
            detail: 1.,
            residual_level: 3,
        };
        let low = residual_controls(1., controls, false).unwrap();
        assert_eq!(low.0, 16. * 1.2);
        assert!(low.1 > 1.);
        let high = residual_controls(32., controls, false).unwrap();
        assert!(high.1 > 1.);
        assert!(residual_controls(0., controls, false).is_err());
    }

    #[test]
    #[ignore = "requires QUATTRO_MONO_STAGE2B captured native residual controls"]
    fn channel_mixed_residuals_match_native_default_controls() {
        let root = std::path::PathBuf::from(std::env::var_os("QUATTRO_MONO_STAGE2B").unwrap());
        let (sharp, adaptive) =
            residual_controls(1., super::super::stage2b::Controls::default(), false).unwrap();
        let params = std::fs::read(root.join("parameters-entry.bin")).unwrap();
        assert_eq!(
            sharp,
            f32::from_le_bytes(params[0xf0..0xf4].try_into().unwrap())
        );
        let table = std::fs::read(root.join("apply-adaptive.f32")).unwrap();
        let values: &[f32] = bytemuck::cast_slice(&table);
        assert!(values.iter().all(|v| v.to_bits() == adaptive.to_bits()));
    }
    #[test]
    #[ignore = "requires GPU, QUATTRO_X3F and QUATTRO_MONO_STAGE2 native full return"]
    fn neutralized_conversion_matches_native_monochrome_stage2() {
        use crate::{
            gpu::Gpu,
            nodes::color::{raw_color_blend, sector_color, spatial_color_gain},
        };
        let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
        let root = std::path::PathBuf::from(std::env::var_os("QUATTRO_MONO_STAGE2").unwrap());
        let gpu = Gpu::new().unwrap();
        let bytes = std::fs::read(root.join("input-rgb.f32")).unwrap();
        let input = gpu
            .upload(
                "Native monochrome Stage1 return",
                bytemuck::cast_slice::<u8, f32>(&bytes),
            )
            .unwrap();
        let mut profile = Default::default();
        let shaded = if let Some(p) =
            super::super::stage2::prepare_shading(&source.metadata, "Auto").unwrap()
        {
            spatial_color_gain::apply(&gpu, &input, &p, &mut profile).unwrap()
        } else {
            input
        };
        let raw = raw_color_blend::apply(
            &gpu,
            &shaded,
            &prepare_stage2(&source.metadata, 1., "Standard").unwrap(),
            &mut profile,
        )
        .unwrap();
        let output = sector_color::apply(
            &gpu,
            &raw.rgb,
            &super::super::mcc::prepare(&source.metadata, "Standard").unwrap(),
            &mut profile,
        )
        .unwrap();
        let actual = gpu.readback(&output).unwrap();
        let expected = std::fs::read(root.join("output-rgb.f32")).unwrap();
        let a: &[f32] = bytemuck::cast_slice(&actual);
        let b: &[f32] = bytemuck::cast_slice(&expected);
        assert_eq!(a.len(), b.len());
        let max = a
            .iter()
            .zip(b)
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        let mean = a
            .iter()
            .zip(b)
            .map(|(a, b)| f64::from((a - b).abs()))
            .sum::<f64>()
            / a.len() as f64;
        eprintln!("Monochrome Stage2 maximum={max} mean={mean}");
        assert!(max <= 0.02);
    }
    #[test]
    #[ignore = "requires GPU, QUATTRO_X3F and QUATTRO_MONO_STAGE2B full native inputs"]
    fn reconstruction_matches_native_monochrome_stage2b() {
        use crate::{
            gpu::Gpu,
            nodes::color::{blue_guided_reconstruct, residual_finish},
        };
        let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
        let root = std::path::PathBuf::from(std::env::var_os("QUATTRO_MONO_STAGE2B").unwrap());
        let gpu = Gpu::new().unwrap();
        let mut profile = Default::default();
        let read = |name: &str| std::fs::read(root.join(name)).unwrap();
        let upload = |name| {
            let b = read(name);
            gpu.upload(name, bytemuck::cast_slice::<u8, f32>(&b))
                .unwrap()
        };
        let prepared = prepare_stage2b(
            &source.metadata,
            super::super::stage2b::Controls::default(),
            "Standard",
        )
        .unwrap();
        let rgb = blue_guided_reconstruct::apply(
            &gpu,
            &upload("filtered-input.f32"),
            &upload("calibrated-input.f32"),
            &upload("processed-input.f32"),
            &upload("blue-input.f32"),
            &prepared.reconstruct,
            &mut profile,
        )
        .unwrap();
        let gain_bytes = gpu.readback(&rgb.residual).unwrap();
        let gains: &[f32] = bytemuck::cast_slice(&gain_bytes);
        let native_gain_bytes = read("apply-residual.bin");
        let native_gains: &[f32] = bytemuck::cast_slice(&native_gain_bytes);
        let mut maximum = 0_f32;
        let mut sum = 0_f64;
        let mut count = 0;
        for y in 80..=3695 {
            for x in 216..=5639 {
                let i = y * 5888 + x;
                let error = (gains[i] - native_gains[i]).abs();
                maximum = maximum.max(error);
                sum += f64::from(error);
                count += 1;
            }
        }
        eprintln!(
            "MonoStage2B residual gain max={maximum} mean={}",
            sum / count as f64
        );
        assert!(maximum <= 0.0001 && sum / count as f64 <= 0.0000001);
        let mask: Vec<u32> = read("apply-mask.bin").into_iter().map(u32::from).collect();
        let output = residual_finish::apply(
            &gpu,
            &rgb.rgb,
            &rgb.residual,
            Some(&gpu.upload("Native full saturation mask", &mask).unwrap()),
            &prepared.finish,
            &mut profile,
        )
        .unwrap();
        for (name, buffer) in [
            ("apply-rgb.bin", rgb.rgb),
            ("output-rgb.u16", output.rgb),
            ("output-detail.i16", output.detail),
        ] {
            let bytes = gpu.readback(&buffer).unwrap();
            let actual: &[f32] = bytemuck::cast_slice(&bytes);
            let b = read(name);
            let expected: Vec<f32> = b
                .as_chunks::<2>()
                .0
                .iter()
                .map(|x| {
                    if name.contains("detail") {
                        f32::from(i16::from_le_bytes(*x))
                    } else {
                        f32::from(u16::from_le_bytes(*x))
                    }
                })
                .collect();
            assert_eq!(actual.len(), expected.len());
            let channels = if name.contains("detail") { 1 } else { 3 };
            let active = |i: usize| {
                let pixel = i / channels;
                let x = pixel % 5888;
                let y = pixel / 5888;
                (216..=5639).contains(&x) && (80..=3695).contains(&y)
            };
            let (max, sum, count) = actual
                .iter()
                .zip(&expected)
                .enumerate()
                .filter(|(i, _)| active(*i))
                .fold((0_f32, 0_f64, 0_usize), |(max, sum, count), (_, (a, b))| {
                    let error = (a - b).abs();
                    (max.max(error), sum + f64::from(error), count + 1)
                });
            let mean = sum / count as f64;
            eprintln!("MonoStage2B {name} max={max} mean={mean}");
            assert!(max <= 1.);
            assert!(mean <= 0.0003);
        }
    }
    #[test]
    #[ignore = "requires GPU and QUATTRO_MONO_STAGE3_IMAGES actual full channel mixing boundary"]
    fn neutral_mixing_matches_native_full_stage3_input() {
        use crate::{gpu::Gpu, nodes::finish::code_channel_map};
        let root =
            std::path::PathBuf::from(std::env::var_os("QUATTRO_MONO_STAGE3_IMAGES").unwrap());
        let read = |name| std::fs::read(root.join(name)).unwrap();
        let bytes = read("before-mix.u16");
        let input: Vec<f32> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|x| f32::from(u16::from_le_bytes(*x)))
            .collect();
        let gpu = Gpu::new().unwrap();
        let image = gpu
            .upload("Native full RGB before neutral channel mixing", &input)
            .unwrap();
        let output = code_channel_map::apply(
            &gpu,
            &image,
            &code_channel_map::Prepared::mix([5888, 3776], [1.; 3]).unwrap(),
            &mut Default::default(),
        )
        .unwrap();
        let actual = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&actual);
        let bytes = read("after-mix.u16");
        let expected: Vec<f32> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|x| f32::from(u16::from_le_bytes(*x)))
            .collect();
        assert_eq!(actual.len(), expected.len());
        let mut maximum = 0_f32;
        let mut sum = 0_f64;
        let mut changed = 0;
        for (a, b) in actual.iter().zip(&expected) {
            let error = (a - b).abs();
            maximum = maximum.max(error);
            sum += f64::from(error);
            changed += usize::from(error != 0.);
        }
        eprintln!(
            "Actual fullMono mix max={maximum} mean={} changed={changed}/{}",
            sum / actual.len() as f64,
            actual.len()
        );
        assert!(maximum <= 1. && sum / (actual.len() as f64) < 0.071);
        assert!(changed * 1000 < actual.len() * 71);
    }
}
