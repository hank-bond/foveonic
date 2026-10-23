//! Checks mixed signed-guide and edge-aware reconstruction against bounded native operations.
use super::*;
// Exact selected variance coefficients; the linear term is unrelated to LOG10_E.
#[allow(clippy::approx_constant)]
const BLUE_VARIANCE: [f64; 3] = [20.843, 0.4342, 8e-7];

#[test]
fn same_size_output_has_a_distinct_key_and_checks_filter_geometry() {
    let full = Prepared::new([8, 8], 0., BLUE_VARIANCE, None).unwrap();
    let same = Prepared::same_size([8, 8], BLUE_VARIANCE, None).unwrap();
    assert_eq!(full.output_extent(), [16, 16]);
    assert_eq!(same.output_extent(), [8, 8]);
    assert!(full.key() != same.key());
    let high_filter = super::super::histogram_bilateral::Prepared::new(
        [16, 16],
        7,
        BLUE_VARIANCE,
        0.1,
        0.25,
        vec![1.; 16384],
        13000.,
    )
    .unwrap();
    assert!(Prepared::same_size([8, 8], BLUE_VARIANCE, Some(high_filter)).is_err());
}

fn level_two_blend() -> Vec<f32> {
    let positions = [
        0, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384,
    ];
    let points: [f64; 15] = [
        0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.51, 0.52, 0.53, 0.56, 0.6, 0.66, 0.7, 0.7,
    ];
    let mut table = vec![0.; 16384];
    for segment in 0..14 {
        let start = positions[segment];
        let end = positions[segment + 1];
        for (index, value) in table.iter_mut().enumerate().take(end).skip(start) {
            let ratio = ((index - start) as f64 / (end - start) as f64) as f32;
            *value = ratio.mul_add(
                (points[segment + 1] - points[segment]) as f32,
                points[segment] as f32,
            );
        }
    }
    table[16383] = points[14] as f32;
    table
}

#[test]
#[ignore = "requires GPU"]
fn filtered_restoration_retains_the_original_blue_guide() {
    // Composed from independently checked native histogram, blend, resize,
    // raw-residual and restoration workers. Separate original/filtered blue
    // inputs are essential: filtering the signed guide changes its RGB output.
    let [w, h] = [32, 64];
    let low = w * h / 4;
    let make =
        |base: f64, multiplier: usize, square: usize, span: usize, offset: f64, channel: f64| {
            (0..(low * 3) as usize)
                .map(|i| {
                    (base + ((i * multiplier + i * i * square) % span) as f64 - offset
                        + (i % 3) as f64 * channel) as f32
                })
                .collect::<Vec<_>>()
        };
    let preprocessed = make(1000., 31, 7, 200, 100., 30.);
    let guide = make(1000., 17, 3, 100, 50., 30.);
    let color = make(1200., 11, 5, 80, 40., 100.);
    let blue: Vec<f32> = (0..(w * h) as usize)
        .map(|i| {
            (1000. + ((i * 137 + i * i * 31) % 180) as f64 - 90. + (i % 13) as f64 / 17.) as f32
        })
        .collect();
    let filter = super::super::histogram_bilateral::Prepared::new(
        [w, h],
        11,
        BLUE_VARIANCE,
        0.1,
        0.25,
        level_two_blend(),
        13000.,
    )
    .unwrap();
    let prepared = Prepared::new([w / 2, h / 2], 0.3, BLUE_VARIANCE, Some(filter)).unwrap();
    let gpu = Gpu::new().unwrap();
    let output = apply(
        &gpu,
        &gpu.upload("Guide", &guide).unwrap(),
        &gpu.upload("Preprocessed", &preprocessed).unwrap(),
        &gpu.upload("Color", &color).unwrap(),
        &gpu.upload("Blue", &blue).unwrap(),
        &prepared,
        &mut Profile::default(),
    )
    .unwrap();
    let fixture = include_bytes!("blue_guided_filtered.fixture");
    let expected: Vec<f32> = fixture
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| f32::from_le_bytes(*v))
        .collect();
    let count = (w * h) as usize;
    for (name, buffer, expected) in [
        ("rgb", output.rgb, &expected[..count * 3]),
        ("residual", output.residual, &expected[count * 3..]),
    ] {
        let bytes = gpu.readback(&buffer).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        let max = actual
            .iter()
            .zip(expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0., f32::max);
        eprintln!("two-blue-history {name}: maximum error{max}");
        assert!(max <= if name == "rgb" { 0. } else { 0.000002 });
    }
}
#[test]
fn requires_finite_bounded_controls() {
    assert!(Prepared::new([8, 8], f32::NAN, [1., 1., 1.], None).is_err());
    assert!(Prepared::new([0, 8], 0.3, [1., 1., 1.], None).is_err());
}
#[test]
#[ignore = "requires BLUE_GUIDED_REFERENCE and GPU"]
fn bounded_native_reconstruction_matches() {
    let dir = std::path::PathBuf::from(std::env::var_os("BLUE_GUIDED_REFERENCE").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let extent = [
            case["extent"][0].as_u64().unwrap() as u32,
            case["extent"][1].as_u64().unwrap() as u32,
        ];
        let read = |key: &str| -> Vec<f32> {
            let b = std::fs::read(dir.join(format!("{name}-{key}.f32"))).unwrap();
            b.as_chunks::<4>()
                .0
                .iter()
                .map(|v| f32::from_le_bytes(*v))
                .collect()
        };
        let upload = |key| gpu.upload(key, &read(key)).unwrap();
        let output = apply(
            &gpu,
            &upload("guide"),
            &upload("pre"),
            &upload("color"),
            &upload("blue"),
            &Prepared::new(extent, 0.3, BLUE_VARIANCE, None)
                .unwrap()
                .with_adaptive_range(case["adaptive_range"].as_bool().unwrap_or(false)),
            &mut Profile::default(),
        )
        .unwrap();
        for (key, buffer) in [("rgb", output.rgb), ("residual", output.residual)] {
            let b = gpu.readback(&buffer).unwrap();
            let actual = bytemuck::cast_slice::<u8, f32>(&b);
            let expected = read(key);
            let differences: Vec<_> = actual
                .iter()
                .zip(&expected)
                .enumerate()
                .filter(|(_, (a, b))| a.to_bits() != b.to_bits())
                .map(|(i, (a, b))| (i, *a, *b))
                .collect();
            let max = differences
                .iter()
                .map(|(_, a, b)| (a - b).abs())
                .fold(0_f32, f32::max);
            eprintln!(
                "{name}/{key}: {} differences max{max} {:?}",
                differences.len(),
                &differences[..differences.len().min(8)]
            );
            // Adaptive support evaluates exp on the GPU rather than using the
            // host-prepared ordinary table. Its accumulated gain can differ by
            // a few Float32 ULPs and move an integral rounding boundary by one code.
            let adaptive = case["adaptive_range"].as_bool().unwrap_or(false);
            let tolerance = if key == "rgb" {
                if adaptive {
                    1.
                } else {
                    0.
                }
            } else if adaptive {
                f32::EPSILON * 8.
            } else {
                f32::EPSILON * 2.
            };
            assert!(max <= tolerance);
        }
    }
}
#[test]
#[ignore = "requires STAGE2B_NATIVE_BOUNDARY and GPU"]
fn selected_full_native_stage2b_rgb_matches() {
    let directory = std::path::PathBuf::from(std::env::var_os("STAGE2B_NATIVE_BOUNDARY").unwrap());
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("boundary.json")).unwrap()).unwrap();
    let extent = [
        receipt["input"]["filtered"]["extent"][0].as_u64().unwrap() as u32,
        receipt["input"]["filtered"]["extent"][1].as_u64().unwrap() as u32,
    ];
    let read = |name: &str| -> Vec<f32> {
        std::fs::read(directory.join(name))
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| f32::from_le_bytes(*v))
            .collect()
    };
    let gpu = Gpu::new().unwrap();
    let upload = |name| gpu.upload(name, &read(name)).unwrap();
    let p = std::fs::read(directory.join("parameters-entry.bin")).unwrap();
    let field = |o| f32::from_le_bytes(p[o..o + 4].try_into().unwrap());
    assert_eq!(field(0x68), 0., "selected zero blue-noise branch");
    let mut profile = Profile::new(crate::profile::Mode::Gpu);
    let output = apply(
        &gpu,
        &upload("filtered-input.f32"),
        &upload("calibrated-input.f32"),
        &upload("processed-input.f32"),
        &upload("blue-input.f32"),
        &Prepared::new(extent, 0.3, BLUE_VARIANCE, None).unwrap(),
        &mut profile,
    )
    .unwrap();
    if directory.join("apply-rgb.bin").exists() {
        for (name, buffer, unsigned) in [
            ("apply-rgb.bin", &output.rgb, true),
            ("apply-residual.bin", &output.residual, false),
        ] {
            let b = gpu.readback(buffer).unwrap();
            let actual = bytemuck::cast_slice::<u8, f32>(&b);
            let expected = std::fs::read(directory.join(name)).unwrap();
            let channels = if unsigned { 3 } else { 1 };
            let width = extent[0] as usize * 2;
            let mut changed = 0;
            let mut maximum = 0_f32;
            for y in 28..=3643 {
                for x in 216..=5639 {
                    for c in 0..channels {
                        let i = (y * width + x) * channels + c;
                        let e = if unsigned {
                            f32::from(u16::from_le_bytes(
                                expected[i * 2..i * 2 + 2].try_into().unwrap(),
                            ))
                        } else {
                            f32::from_le_bytes(expected[i * 4..i * 4 + 4].try_into().unwrap())
                        };
                        let error = (actual[i] - e).abs();
                        changed += usize::from(error != 0.);
                        maximum = maximum.max(error);
                    }
                }
            }
            eprintln!("Reconstruction boundary {name} changed{changed} max{maximum}");
        }
    }
    let mask_values = if directory.join("apply-mask.bin").exists() {
        std::fs::read(directory.join("apply-mask.bin"))
            .unwrap()
            .into_iter()
            .map(u32::from)
            .collect()
    } else {
        vec![0u32; (extent[0] * extent[1] * 4) as usize]
    };
    let mask = gpu
        .upload("Captured detail damping mask", &mask_values)
        .unwrap();
    let adaptive = read("apply-adaptive.f32");
    assert!(adaptive.iter().all(|v| *v == adaptive[0]));
    let adaptive = adaptive[0];
    let result = crate::nodes::color::residual_finish::apply(
        &gpu,
        &output.rgb,
        &output.residual,
        Some(&mask),
        &crate::nodes::color::residual_finish::Params::new(
            extent.map(|v| v * 2),
            field(0x3c),
            field(0xf0),
            0,
            adaptive,
            1.,
        )
        .unwrap(),
        &mut profile,
    )
    .unwrap();
    let bytes = gpu.readback(&result.rgb).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
    let expected = std::fs::read(directory.join("output-rgb.u16")).unwrap();
    let words = expected.as_chunks::<2>().0;
    let width = extent[0] as usize * 2;
    let mut changed = 0usize;
    let mut maximum = 0_f32;
    let mut total = 0_f64;
    let mut examples = vec![];
    for y in 28..=3643 {
        for x in 216..=5639 {
            for c in 0..3 {
                let i = (y * width + x) * 3 + c;
                let e = f32::from(u16::from_le_bytes(words[i]));
                let error = (actual[i] - e).abs();
                if error != 0. {
                    changed += 1;
                    if examples.len() < 12 {
                        examples.push((x, y, c, actual[i], e));
                    }
                }
                maximum = maximum.max(error);
                total += f64::from(error);
            }
        }
    }
    eprintln!(
        "FullStage2B active RGB changed{changed} max{maximum} mean{} examples{examples:?}; {:?}",
        total / (5424. * 3616. * 3.),
        profile.gpu
    );
    assert!(maximum <= 1., "unexpected full-stage discrepancy");
}

#[test]
#[ignore = "requires GPU"]
fn same_size_averages_each_cell_and_preserves_rgb_packing() {
    let gpu = Gpu::new().unwrap();
    let [w, h] = [8_u32, 8_u32];
    let mut source = vec![0_f32; (w * h * 3) as usize];
    let mut color = source.clone();
    let mut blue = vec![0_f32; (w * h * 4) as usize];
    let samples = [-10., 0., 100., 10999., 11000., 12500., 14000., 16000.];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let center = samples[i % samples.len()];
            source[i * 3 + 2] = center;
            color[i * 3] = -2.5;
            color[i * 3 + 1] = 1234.75;
            color[i * 3 + 2] = 70000.;
            let j = ((y * 2) * (w * 2) + x * 2) as usize;
            for (offset, value) in [
                (0, center - 12.),
                (1, center - 4.),
                ((w * 2) as usize, center + 4.),
                ((w * 2 + 1) as usize, center + 12.),
            ] {
                blue[j + offset] = value * 0.8;
            }
        }
    }
    let p = Prepared::same_size([w, h], BLUE_VARIANCE, None).unwrap();
    let output = apply(
        &gpu,
        &gpu.upload("Guide", &source).unwrap(),
        &gpu.upload("Original", &source).unwrap(),
        &gpu.upload("Color", &color).unwrap(),
        &gpu.upload("Blue", &blue).unwrap(),
        &p,
        &mut Profile::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&output.rgb).unwrap();
    let rgb: &[f32] = bytemuck::cast_slice(&bytes);
    for c in rgb.as_chunks::<3>().0 {
        assert_eq!(*c, [0., 1234., 65535.]);
    }
    let bytes = gpu.readback(&output.residual).unwrap();
    let residual: &[f32] = bytemuck::cast_slice(&bytes);
    for (i, &actual) in residual.iter().enumerate() {
        let b = samples[i % samples.len()];
        let mut ratio = if b > 0. {
            (b * 0.8 + 5.) / (b + 5.)
        } else {
            1.
        };
        if b >= 11000. && ratio < 1. {
            ratio = (ratio - 1.).mul_add(1. - ((b - 11000.) / 3000.).clamp(0., 1.), 1.);
        }
        assert!((actual - ratio).abs() < 1e-6, "{i} {actual} {ratio}");
    }
}
