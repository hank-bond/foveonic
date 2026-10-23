//! Prepares local contrast and adaptive sharpening for Full and reduced preview grids.
use super::WorkingGrid;
use crate::{
    nodes::finish::{proportional_sharpen, wavelet_contrast},
    source::{Metadata, Result},
};
const LCE_PADDING_ALIGNMENT: u32 = 128;
const LCE_RELATIVE_BOUND: f32 = 0.35;
const LOWEST_UNSHARP_BASE: f32 = 0.15;
const LOWEST_UNSHARP_MIX: f32 = 0.3;
// LOW-inspired finishing controls used by the reviewed reduced HIGH preview.
// The wavelet base becomes 0.12 instead of 0.15, and sharpening loses sqrt(2).
const HALF_GRID_LOWEST_UNSHARP_SCALE: f32 = 0.8;
pub(super) const HALF_GRID_SHARPEN_SCALE: f32 = std::f32::consts::FRAC_1_SQRT_2;
const FULL_BOX_RADII: [u32; 2] = [7, 3];
const HALF_BOX_RADII: [u32; 2] = [3, 2];
const BOX_STRENGTHS: [f32; 2] = [0.2, 0.1];
const LOWEST_GAUSSIAN_VARIANCE: f64 = 9.;
const SHARPEN_DETAIL_LIMIT: f32 = 2000.;
const SHARPEN_LUMA_LIMIT: f32 = 0.35;
const ADAPTIVE_TABLE_LENGTH: usize = 16384;
// Fifth-degree gain corrections in log spatial scale, one row per LCE level.
const LEVEL_POLYNOMIALS: [[f64; 6]; 7] = [
    [
        0.994647332,
        0.113350012,
        -0.032330066,
        0.00445408,
        -0.000281684,
        0.00000681,
    ],
    [
        1.099293936,
        0.358326532,
        -0.668526998,
        0.635982596,
        -0.279390474,
        0.046667511,
    ],
    [
        1.121630917,
        0.459364101,
        -0.8892682,
        0.865612984,
        -0.385829071,
        0.064953153,
    ],
    [
        1.144235149,
        0.56701199,
        -1.129763281,
        1.120466442,
        -0.505730121,
        0.085778859,
    ],
    [
        1.156549415,
        0.662350458,
        -1.403245496,
        1.473314702,
        -0.693200928,
        0.12130233,
    ],
    [
        1.167312141,
        0.717966942,
        -1.537551333,
        1.626538431,
        -0.769130554,
        0.135002203,
    ],
    [
        1.176575234,
        0.766115226,
        -1.653807682,
        1.759424184,
        -0.83512344,
        0.146938793,
    ],
];
const DETAIL_LIMITS: [f32; 6] = [100., 100., 100., 100., 25., 25.];
const DETAIL_SLOPES: [f32; 6] = [0.5, 0.55, 0.6, 0.6, 0.7, 0.8];
const LEVEL_FACTORS: [f32; 7] = [0.5333, 0.6667, 0.8667, 1., 1.3, 1.6, 2.];
const SHARPEN_TAILS: [[f64; 4]; 7] = [
    [1., 1., 1., 1.],
    [1.04, 1.05, 1.11, 1.04],
    [1.06, 1.11, 1.25, 1.1],
    [1.08, 1.16, 1.4, 1.16],
    [1.1, 1.21, 1.56, 1.23],
    [1.11, 1.27, 1.73, 1.31],
    [1.13, 1.32, 1.91, 1.39],
];
const KNOT_POSITIONS: [usize; 15] = [
    0, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384,
];
fn extent(metadata: &Metadata, grid: WorkingGrid) -> Result<[u32; 2]> {
    if !super::defects::supported(metadata) {
        return Err("Stage3 requires ordinary full-size Quattro controls".into());
    }
    Ok(grid.extent([
        u32::try_from(metadata.width)?,
        u32::try_from(metadata.height)?,
    ]))
}
fn lce_crop(metadata: &Metadata, grid: WorkingGrid) -> Result<[u32; 4]> {
    let mut area = metadata.fixed::<4>("ActiveImageArea")?;
    if grid.is_half() {
        area = area.map(|v| (v / f64::from(grid.divisor())).floor());
    }
    if area
        .iter()
        .any(|v| !v.is_finite() || *v < 0. || *v >= u32::MAX as f64 || v.fract() != 0.)
    {
        return Err("invalid Stage3 active rectangle".into());
    }
    Ok([
        area[0] as u32,
        area[1] as u32,
        // This caller retains the raw CAMF right/bottom as exclusive bounds.
        // Its finishing crop separately adds one to the same metadata values.
        area[2] as u32,
        area[3] as u32,
    ])
}
pub(super) fn prepare_lce(
    metadata: &Metadata,
    level: usize,
    mode: &str,
    lowest_factor: Option<f32>,
    grid: WorkingGrid,
) -> Result<wavelet_contrast::Prepared> {
    let extent = extent(metadata, grid)?;
    let polynomial = *LEVEL_POLYNOMIALS
        .get(level)
        .ok_or("unsupported local contrast level")?;
    let gains: [f32; 6] = std::array::from_fn(|i| {
        let x = (2. * ((1_u32 << (i + 1)) - 1) as f64).sqrt().ln();
        let mut correction = polynomial[0];
        for (power, coefficient) in polynomial.iter().enumerate().skip(1) {
            correction = x.powf(power as f64).mul_add(*coefficient, correction);
        }
        let correction = correction as f32;
        let correction = if mode == "Portrait" {
            (correction - 1.).mul_add(if i < 5 { 0.9 } else { 0.5 }, 1.)
        } else {
            correction
        };
        correction.clamp(1., 10.)
    });
    wavelet_contrast::Prepared::new(
        extent,
        extent.map(|v| v.div_ceil(LCE_PADDING_ALIGNMENT) * LCE_PADDING_ALIGNMENT),
        lce_crop(metadata, grid)?,
        std::array::from_fn(|i| wavelet_contrast::BandControl {
            gain: gains[i],
            limit: DETAIL_LIMITS[i],
            slope: DETAIL_SLOPES[i],
        }),
        lowest_kernel(level, mode == "Portrait", lowest_factor, grid)?,
        LCE_RELATIVE_BOUND,
    )
}
fn lowest_kernel(
    level: usize,
    portrait: bool,
    supplied_factor: Option<f32>,
    grid: WorkingGrid,
) -> Result<[f32; 9]> {
    let factor = supplied_factor.unwrap_or(
        *LEVEL_FACTORS
            .get(level)
            .ok_or("unsupported local contrast level")?,
    );
    if !factor.is_finite() || factor <= 0. {
        return Err("invalid lowest-band contrast factor".into());
    }
    let base = if portrait {
        LOWEST_UNSHARP_BASE * 0.8
    } else {
        LOWEST_UNSHARP_BASE
    };
    let base = if grid.is_half() {
        base * HALF_GRID_LOWEST_UNSHARP_SCALE
    } else {
        base
    };
    let amount = (base * LOWEST_UNSHARP_MIX) * factor;
    let gaussian: [f32; 9] = std::array::from_fn(|i| {
        let y = i as i32 / 3 - 1;
        let x = i as i32 % 3 - 1;
        ((-0.5 * f64::from(y * y) / LOWEST_GAUSSIAN_VARIANCE).exp()
            * (-0.5 * f64::from(x * x) / LOWEST_GAUSSIAN_VARIANCE).exp()) as f32
    });
    let total = gaussian.iter().fold(0_f32, |sum, v| sum + v);
    let mut kernel = gaussian.map(|v| -amount * (v / total));
    let center = (amount + 1.) + kernel[4];
    kernel[4] = center;
    Ok(kernel)
}
fn sharpening_table(level: usize, iso_gain: f32, base: f32) -> Result<Vec<f32>> {
    let [a, b, c, d] = *SHARPEN_TAILS
        .get(level)
        .ok_or("unsupported sharpening level")?;
    let mut knots = [1_f64; 15];
    knots[7] = a;
    knots[8] = b;
    knots[9..14].fill(c);
    let logarithm = f64::from(iso_gain.log2());
    let variance = logarithm * logarithm / 36. + 1.;
    let base = f64::from(base);
    for value in &mut knots[..14] {
        *value = if base < 0. {
            *value * 1.15 * base.mul_add(0.5, 1.)
        } else {
            variance.mul_add(base, *value * 1.15)
        };
    }
    knots[14] = if base < 0. {
        base.mul_add(0.5, 1.) * d * 0.5
    } else {
        variance.mul_add(base, d * 0.5)
    };
    let mut table = vec![0.; ADAPTIVE_TABLE_LENGTH];
    for segment in 0..14 {
        let first = KNOT_POSITIONS[segment];
        let last = KNOT_POSITIONS[segment + 1];
        let delta = (knots[segment + 1] - knots[segment]) as f32;
        let origin = knots[segment] as f32;
        for (index, value) in table.iter_mut().enumerate().take(last).skip(first) {
            let position = ((index - first) as f64 / (last - first) as f64) as f32;
            *value = position.mul_add(delta, origin);
        }
    }
    table[ADAPTIVE_TABLE_LENGTH - 1] = knots[14] as f32;
    Ok(table)
}
pub(super) fn prepare_sharpen(
    metadata: &Metadata,
    level: usize,
    base: f32,
    preserve_dark: bool,
    grid: WorkingGrid,
) -> Result<proportional_sharpen::Prepared> {
    let iso_gain = capture_iso_gain(metadata)?;
    let log_iso = iso_gain.log2();
    let absolute_limit = ((log_iso * log_iso) * 8000.) / 36. + SHARPEN_DETAIL_LIMIT;
    proportional_sharpen::Prepared::new(
        extent(metadata, grid)?,
        // The ordinary wrapper leaves SharpData's mode string empty. Its
        // standalone initializer supports Portrait, but this caller does not
        // select that branch. Portrait still changes LCE and reconstruction.
        sharpening_table(level, iso_gain, base)?,
        iso_gain,
        absolute_limit,
        SHARPEN_LUMA_LIMIT,
        if grid.is_half() {
            HALF_GRID_SHARPEN_SCALE
        } else {
            1.
        },
        preserve_dark,
    )
}
/// Ordinary single-input color sharpening follows the same base as the
/// proportional table. Portrait and high ISO reduce its caller amount.
pub(super) fn color_sharpen_amount(metadata: &Metadata, mode: &str, base: f32) -> Result<f32> {
    let iso_gain = capture_iso_gain(metadata)?;
    let caller_amount = if mode == "Portrait" || iso_gain >= 8. {
        0.5_f32
    } else {
        1.
    };
    let base = f64::from(base);
    Ok((base + base * 0.5 * f64::from(caller_amount)) as f32 / iso_gain.sqrt())
}

fn capture_iso_gain(metadata: &Metadata) -> Result<f32> {
    let gain = (metadata.scalar("CaptureISO")? / 100.) as f32;
    if !gain.is_finite() || gain <= 0. {
        return Err("invalid spatial-appearance ISO gain".into());
    }
    Ok(gain)
}

pub(super) fn uses_box_contrast(metadata: &Metadata) -> Result<bool> {
    Ok(capture_iso_gain(metadata)? >= 8.)
}

pub(super) fn prepare_box_contrast(
    metadata: &Metadata,
    level: usize,
    grid: WorkingGrid,
) -> Result<crate::nodes::finish::box_contrast::Prepared> {
    if level > 6 {
        return Err("unsupported local contrast level".into());
    }
    let control = level as f32 / 3.;
    let radii = if grid.is_half() {
        HALF_BOX_RADII
    } else {
        FULL_BOX_RADII
    };
    crate::nodes::finish::box_contrast::Prepared::new(
        extent(metadata, grid)?,
        std::array::from_fn(|i| (radii[i], control * BOX_STRENGTHS[i])),
        capture_iso_gain(metadata)?,
    )
}

// Pinned ordinary conversion globals. These are the reference implementation's
// RGB/XYZ matrices, before the packed-Lab white normalization in the node.
const RGB_TO_XYZ: [f64; 9] = [
    0.41239558896741424,
    0.3575834307637148,
    0.18049264738170157,
    0.21258623078559555,
    0.7151703037034108,
    0.07220049864333623,
    0.019297215491746945,
    0.11918386458084854,
    0.9504971251315798,
];
const XYZ_TO_RGB: [f64; 9] = [
    3.2406, -1.5372, -0.4986, -0.9689, 1.8758, 0.0415, 0.0557, -0.204, 1.057,
];
/// Independent spatial controls selected by the recipe.
#[derive(Clone, Copy)]
pub(super) struct Controls {
    pub contrast_level: usize,
    pub sharpen_level: usize,
    pub sharpen_base: f32,
    pub lowest_band_factor: Option<f32>,
    pub preserve_dark_detail: bool,
    pub grid: WorkingGrid,
    pub region: Option<[u32; 4]>,
}

/// Retain full physical planes here. Generic finishing owns the active crop.
/// Local contrast and sharpening have independently selected reference levels.
pub(super) fn build(
    graph: &mut crate::graph::Graph,
    metadata: &Metadata,
    rgb: usize,
    detail: usize,
    mode: &str,
    controls: Controls,
) -> Result<usize> {
    use crate::graph::Operation;
    let converted = build_contrast(graph, metadata, rgb, mode, controls)?;
    let prepared = prepare_sharpen(
        metadata,
        controls.sharpen_level,
        controls.sharpen_base,
        controls.preserve_dark_detail,
        controls.grid,
    )?;
    let prepared = if let Some(rect) = controls.region {
        prepared.with_extent([rect[2] - rect[0], rect[3] - rect[1]])?
    } else {
        prepared
    };
    graph.add(
        Operation::ProportionalSharpen(std::sync::Arc::new(prepared)),
        &[converted, detail],
    )
}

/// Composes the shared local-contrast operations before a supplied sharpening response.
pub(super) fn build_contrast(
    graph: &mut crate::graph::Graph,
    metadata: &Metadata,
    rgb: usize,
    mode: &str,
    controls: Controls,
) -> Result<usize> {
    use crate::{
        graph::Operation,
        nodes::color::packed_lab::{Direction, Prepared},
    };
    let extent = if let Some(rect) = controls.region {
        [rect[2] - rect[0], rect[3] - rect[1]]
    } else {
        extent(metadata, controls.grid)?
    };
    let converted = if uses_box_contrast(metadata)? {
        let prepared = prepare_box_contrast(metadata, controls.contrast_level, controls.grid)?;
        let prepared = if controls.region.is_some() {
            prepared.with_extent(extent)?
        } else {
            prepared
        };
        graph.add(
            Operation::BoxContrast(std::sync::Arc::new(prepared)),
            &[rgb],
        )?
    } else {
        let lab = graph.add(
            Operation::PackedLab(Prepared::new(extent, Direction::Forward, RGB_TO_XYZ)?),
            &[rgb],
        )?;
        let prepared = prepare_lce(
            metadata,
            controls.contrast_level,
            mode,
            controls.lowest_band_factor,
            controls.grid,
        )?;
        let prepared = if let Some(rect) = controls.region {
            prepared.on_region(rect)?
        } else {
            prepared
        };
        let adjusted = graph.add(
            Operation::WaveletContrast(std::sync::Arc::new(prepared)),
            &[lab],
        )?;
        graph.add(
            Operation::PackedLab(Prepared::new(extent, Direction::Inverse, XYZ_TO_RGB)?),
            &[adjusted],
        )?
    };
    Ok(converted)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires QUATTRO_MODE_PARAMETERS with native sharpening tables"]
    fn color_mode_sharpening_tables_match_native() {
        let directory =
            std::path::PathBuf::from(std::env::var_os("QUATTRO_MODE_PARAMETERS").unwrap());
        for level in 0..=6 {
            let table = sharpening_table(level, 1., 0.).unwrap();
            let expected = std::fs::read(directory.join(format!("sharp-0-{level}.f32"))).unwrap();
            assert_eq!(bytemuck::cast_slice::<f32, u8>(&table), expected);
        }
        let cases: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(directory.join("sharp-cases.json")).unwrap())
                .unwrap();
        for case in cases {
            if case["portrait"].as_bool().unwrap() {
                continue;
            }
            let table = sharpening_table(
                case["level"].as_u64().unwrap() as usize,
                case["iso"].as_f64().unwrap() as f32,
                case["base"].as_f64().unwrap() as f32,
            )
            .unwrap();
            let expected = std::fs::read(directory.join(case["file"].as_str().unwrap())).unwrap();
            assert_eq!(bytemuck::cast_slice::<f32, u8>(&table), expected, "{case}");
        }
    }

    #[test]
    fn packed_lab_ports_require_explicit_conversion() {
        use crate::{
            graph::{DataType, Graph, Operation},
            nodes::color::packed_lab::{Direction, Prepared},
        };
        let mut graph = Graph::default();
        let rgb = graph
            .add(
                Operation::RenderedInput {
                    data: DataType::UnsignedCodeRgb,
                },
                &[],
            )
            .unwrap();
        assert!(graph
            .add(
                Operation::PackedLab(
                    Prepared::new([128, 128], Direction::Inverse, XYZ_TO_RGB).unwrap()
                ),
                &[rgb]
            )
            .is_err());
        let lab = graph
            .add(
                Operation::PackedLab(
                    Prepared::new([128, 128], Direction::Forward, RGB_TO_XYZ).unwrap(),
                ),
                &[rgb],
            )
            .unwrap();
        assert_eq!(graph.outputs[lab].data, DataType::PackedLabCodes);
        assert!(graph
            .add(
                Operation::PackedLab(
                    Prepared::new([128, 128], Direction::Forward, RGB_TO_XYZ).unwrap()
                ),
                &[lab]
            )
            .is_err());
    }
    #[test]
    #[ignore = "requires STAGE3_NATIVE"]
    fn source_sharpen_table_matches_actual_caller() {
        let path =
            std::path::PathBuf::from(std::env::var_os("STAGE3_NATIVE").expect("set STAGE3_NATIVE"));
        let actual = std::fs::read(path.join("sharp-lut.f32")).unwrap();
        let expected = sharpening_table(6, 1., 0.).unwrap();
        assert_eq!(actual, bytemuck::cast_slice::<f32, u8>(&expected));
    }
    #[test]
    #[ignore = "requires QUATTRO_X3F,STAGE3_NATIVE and GPU"]
    fn complete_original_local_contrast_boundary() {
        let source =
            crate::source::Source::open(std::env::var_os("QUATTRO_X3F").expect("set QUATTRO_X3F"))
                .unwrap();
        let directory =
            std::path::PathBuf::from(std::env::var_os("STAGE3_NATIVE").expect("set STAGE3_NATIVE"));
        let read = |name: &str| -> Vec<f32> {
            let b = std::fs::read(directory.join(name)).unwrap();
            b.as_chunks::<2>()
                .0
                .iter()
                .map(|v| f32::from(u16::from_le_bytes(*v)))
                .collect()
        };
        let gpu = crate::gpu::Gpu::new().unwrap();
        let header = std::fs::read(directory.join("lce-data.bin")).unwrap();
        let captured_crop = std::array::from_fn(|i| {
            u32::from_le_bytes(header[16 + i * 4..20 + i * 4].try_into().unwrap())
        });
        assert_eq!(
            lce_crop(&source.metadata, WorkingGrid::Full).unwrap(),
            captured_crop
        );
        let original = read("main-input-rgb.u16");
        let input = gpu.upload("Original pre-LCE RGB", &original).unwrap();
        let size = extent(&source.metadata, WorkingGrid::Full).unwrap();
        let start = std::time::Instant::now();
        let lab = crate::nodes::color::packed_lab::apply(
            &gpu,
            &input,
            &crate::nodes::color::packed_lab::Prepared::new(
                size,
                crate::nodes::color::packed_lab::Direction::Forward,
                RGB_TO_XYZ,
            )
            .unwrap(),
            &mut Default::default(),
        )
        .unwrap();
        let adjusted = wavelet_contrast::apply(
            &gpu,
            &lab,
            &prepare_lce(&source.metadata, 6, "Standard", None, WorkingGrid::Full).unwrap(),
            &mut Default::default(),
        )
        .unwrap();
        let output = crate::nodes::color::packed_lab::apply(
            &gpu,
            &adjusted,
            &crate::nodes::color::packed_lab::Prepared::new(
                size,
                crate::nodes::color::packed_lab::Direction::Inverse,
                XYZ_TO_RGB,
            )
            .unwrap(),
            &mut Default::default(),
        )
        .unwrap();
        let b = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&b);
        let expected = read("sharp-input-rgb.u16");
        let mut maximum = 0_f32;
        let mut total = 0_f64;
        let mut changed = 0;
        for (&a, &b) in actual.iter().zip(&expected) {
            let e = (a - b).abs();
            maximum = maximum.max(e);
            total += f64::from(e);
            changed += usize::from(a != b);
        }
        eprintln!(
            "Complete original LCE {:?}: changed{changed}/{} mean{} max{maximum}",
            start.elapsed(),
            actual.len(),
            total / actual.len() as f64
        );
        // This comparison includes Metal's approximate cube root and the selected
        // source's level-six shared-kernel race. It measures the composed boundary.
        // This retained original permits the observed packed-Lab quantization
        // and level-six kernel variation. The measured maximum is13 RGB codes.
        const MAXIMUM_CAPTURE_ERROR: f32 = 16.;
        assert!(maximum <= MAXIMUM_CAPTURE_ERROR);
    }
    #[test]
    #[ignore = "requires GPU, QUATTRO_X3F and QUATTRO_MONO_STAGE3_COMPLETE owned native physical boundary"]
    fn monochrome_spatial_controls_match_native_full_stage3() {
        use crate::{
            gpu::Gpu,
            nodes::{
                color::packed_lab::{self, Direction, Prepared},
                finish::{proportional_sharpen, wavelet_contrast},
            },
        };
        let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
        let root =
            std::path::PathBuf::from(std::env::var_os("QUATTRO_MONO_STAGE3_COMPLETE").unwrap());
        let read = |name| std::fs::read(root.join(name)).unwrap();
        let bytes = read("after-mix.u16");
        let input: Vec<f32> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|x| f32::from(u16::from_le_bytes(*x)))
            .collect();
        let bytes = read("detail-input.i16");
        let detail: Vec<f32> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|x| f32::from(i16::from_le_bytes(*x)))
            .collect();
        let gpu = Gpu::new().unwrap();
        let mut profile = Default::default();
        let extent = extent(&source.metadata, WorkingGrid::Full).unwrap();
        let image = gpu
            .upload("Native monochrome Stage3 mixed RGB", &input)
            .unwrap();
        let detail = gpu
            .upload("Native monochrome Stage3 detail", &detail)
            .unwrap();
        let lab = packed_lab::apply(
            &gpu,
            &image,
            &Prepared::new(extent, Direction::Forward, RGB_TO_XYZ).unwrap(),
            &mut profile,
        )
        .unwrap();
        let adjusted = wavelet_contrast::apply(
            &gpu,
            &lab,
            &prepare_lce(
                &source.metadata,
                6,
                "Standard",
                Some(1.6),
                WorkingGrid::Full,
            )
            .unwrap(),
            &mut profile,
        )
        .unwrap();
        let rgb = packed_lab::apply(
            &gpu,
            &adjusted,
            &Prepared::new(extent, Direction::Inverse, XYZ_TO_RGB).unwrap(),
            &mut profile,
        )
        .unwrap();
        let output = proportional_sharpen::apply(
            &gpu,
            &rgb,
            &detail,
            &super::super::monochrome::prepare_sharpen(&source.metadata, 6, 0.).unwrap(),
            &mut profile,
        )
        .unwrap();
        let actual = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&actual);
        let bytes = read("after-stage3.u16");
        let expected: Vec<f32> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|x| f32::from(u16::from_le_bytes(*x)))
            .collect();
        let mut maximum = 0_f32;
        let mut sum = 0_f64;
        let mut changed = 0;
        let mut count = 0;
        for y in 80..=3695 {
            for x in 216..=5639 {
                for channel in 0..3 {
                    let i = (y * 5888 + x) * 3 + channel;
                    let a = actual[if channel == 2 { i - 2 } else { i }];
                    let error = (a - expected[i]).abs();
                    maximum = maximum.max(error);
                    sum += f64::from(error);
                    changed += usize::from(error != 0.);
                    count += 1;
                }
            }
        }
        eprintln!(
            "Mono fullStage3 max={maximum} mean={} changed={changed}/{count}",
            sum / count as f64
        );
        // The shared integral Lab and wavelet passes move a few code-boundary
        // samples even when supplied identical native monochrome inputs.
        assert!(maximum <= 4. && sum / (count as f64) < 0.19);
        assert!(changed * 100 < count * 13);
    }
}
