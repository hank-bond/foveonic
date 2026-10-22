//! Measures parallel quantized filtering against independently verified native histogram fixtures.
use super::*;

#[allow(clippy::approx_constant)] // Calibrated coefficient, unrelated to LOG10_E.
const VARIANCE: [f64; 3] = [20.843, 0.4342, 8e-7];

fn blend_table(level: usize) -> Vec<f32> {
    let points: [f64; 15] = match level {
        1 => [
            0.33, 0.33, 0.33, 0.33, 0.33, 0.33, 0.33, 0.33, 0.34, 0.34, 0.36, 0.39, 0.43, 0.46,
            0.46,
        ],
        2 => [
            0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.51, 0.52, 0.53, 0.56, 0.6, 0.66, 0.7, 0.7,
        ],
        4 => [
            0.72, 0.72, 0.72, 0.72, 0.72, 0.72, 0.72, 0.73, 0.74, 0.76, 0.8, 0.86, 0.94, 1., 1.,
        ],
        _ => unreachable!(),
    };
    let positions = [
        0, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384,
    ];
    let mut result = vec![0.; ADAPTIVE_LENGTH];
    for segment in 0..14 {
        let start = positions[segment];
        let end = positions[segment + 1];
        for (index, value) in result.iter_mut().enumerate().take(end).skip(start) {
            *value = ((index - start) as f64 / (end - start) as f64) as f32;
            *value = value.mul_add(
                (points[segment + 1] - points[segment]) as f32,
                points[segment] as f32,
            );
        }
    }
    result[ADAPTIVE_LENGTH - 1] = points[14] as f32;
    result
}

#[test]
fn validates_scalar_filter_domain() {
    let prepare = |kernel, strength, cutoff, normalization| {
        Prepared::new(
            [17, 64],
            kernel,
            VARIANCE,
            strength,
            cutoff,
            blend_table(2),
            normalization,
        )
    };
    assert!(prepare(11, 0.1, 0.25, 13000.).is_ok());
    assert!(prepare(10, 0.1, 0.25, 13000.).is_err());
    assert!(prepare(11, 0., 0.25, 13000.).is_err());
    assert!(prepare(11, 0.1, f32::NAN, 13000.).is_err());
    assert!(prepare(11, 0.1, 0.25, 0.).is_err());
}

#[test]
fn cutoff_bound_keeps_weights_that_round_up_to_one() {
    let prepared =
        Prepared::new([17, 64], 11, [1e12, 0., 0.], 1., 1., blend_table(2), 13000.).unwrap();
    // exp(-160²/1e12) rounds to Float32 one; exp(-176²/1e12)
    // rounds to its predecessor. An exact-double cutoff would lose this row.
    assert_eq!(prepared.ranges[512][0], 502);
    assert_eq!(prepared.ranges[512][2], 21);
}

#[test]
#[ignore = "requires GPU"]
fn parallel_filter_measures_native_histogram_accumulation_error() {
    // Eight complete native histogram+blend calls, independently reproduced
    // by scalar serpentine accumulation before retaining these Float32 results.
    // The native library SHA256 is
    // a0f64d511d4b7fdfbcc11673b74dfdd61a85bd06128804e0f324214b64f21229.
    let fixture = include_bytes!("histogram_bilateral.fixture");
    let gpu = Gpu::new().unwrap();
    let [w, h] = [17, 64];
    let count = (w * h) as usize;
    let mut case = 0;
    for mean in [1000., 14000.] {
        for (kernel, strength, level) in [(5, 0.2, 1), (11, 0.2, 1), (11, 0.1, 2), (11, 0.05, 4)] {
            let pixels: Vec<f32> = (0..count)
                .map(|i| {
                    (mean + ((i * 137 + i * i * 31) % 180) as f64 - 90. + (i % 13) as f64 / 17.)
                        as f32
                })
                .collect();
            let prepared = Prepared::new(
                [w, h],
                kernel,
                VARIANCE,
                strength,
                0.25,
                blend_table(level),
                13000.,
            )
            .unwrap();
            let output = apply(
                &gpu,
                &gpu.upload("Fixture scalar codes", &pixels).unwrap(),
                &prepared,
                &mut Profile::default(),
            )
            .unwrap();
            let bytes = gpu.readback(&output).unwrap();
            let actual: &[f32] = bytemuck::cast_slice(&bytes);
            let expected: Vec<f32> = fixture[case * count * 4..(case + 1) * count * 4]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|v| f32::from_le_bytes(*v))
                .collect();
            let max = actual
                .iter()
                .zip(&expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0., f32::max);
            let mean_error = actual
                .iter()
                .zip(&expected)
                .map(|(a, b)| f64::from((a - b).abs()))
                .sum::<f64>()
                / count as f64;
            eprintln!("histogram case{case} mean{mean_error} max{max}");
            assert!(max <= 0.02, "case{case}: {max}");
            let radius = kernel / 2;
            for y in 0..h {
                for x in 0..w {
                    if x < radius || y < radius || x + radius >= w || y + radius >= h {
                        let index = (y * w + x) as usize;
                        assert_eq!(actual[index], pixels[index]);
                    }
                }
            }
            case += 1;
        }
    }
    assert_eq!(fixture.len(), case * count * 4);
}

#[test]
#[ignore = "requires GPU; measures a full physical blue plane"]
fn full_size_quantized_filter_profile() {
    let extent = [5888, 3672];
    let source: Vec<f32> = (0..extent[0] * extent[1])
        .map(|i| 1000. + (i % 180) as f32)
        .collect();
    let prepared = Prepared::new(extent, 11, VARIANCE, 0.1, 0.25, blend_table(2), 13000.).unwrap();
    let gpu = Gpu::new().unwrap();
    let input = gpu.upload("Full scalar codes", &source).unwrap();
    for iteration in 0..6 {
        let mut profile = Profile::new(crate::profile::Mode::Gpu);
        let output = apply(&gpu, &input, &prepared, &mut profile).unwrap();
        eprintln!("full histogram iteration{iteration}: {:?}", profile.gpu);
        assert_eq!(output.size(), input.size());
    }
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&source)
    );
}

#[test]
#[ignore = "requires HISTOGRAM_CAPTURE and GPU"]
fn acquired_blue_tile_measures_native_histogram_history() {
    let directory = std::path::PathBuf::from(std::env::var_os("HISTOGRAM_CAPTURE").unwrap());
    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("blend-entry.json")).unwrap())
            .unwrap();
    let dimensions = &record["original"]["roi"]["dimensions"];
    let extent = [
        dimensions[0].as_u64().unwrap() as u32,
        dimensions[1].as_u64().unwrap() as u32,
    ];
    let read = |name: &str| {
        std::fs::read(directory.join(name))
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| f32::from_le_bytes(*v))
            .collect::<Vec<_>>()
    };
    let original = read("blend-original-entry.roi.bin");
    let expected = read("blend-return.roi.bin");
    let parameters = std::fs::read(directory.join("hist-parameters.bin")).unwrap();
    let normalization = f32::from_le_bytes(parameters[0xfc..0x100].try_into().unwrap());
    let prepared = Prepared::new(
        extent,
        11,
        VARIANCE,
        0.1,
        0.25,
        blend_table(2),
        normalization,
    )
    .unwrap();
    let native_rows: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("hist-weights.json")).unwrap())
            .unwrap();
    for (index, row) in native_rows.as_array().unwrap().iter().enumerate() {
        let [first, offset, count, _] = prepared.ranges[index];
        assert_eq!(u64::from(first), row["start"].as_u64().unwrap());
        let encoded = row["values_hex"].as_str().unwrap();
        let bytes: Vec<u8> = encoded
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        assert_eq!(
            bytes.as_slice(),
            bytemuck::cast_slice::<f32, u8>(
                &prepared.weights[offset as usize..(offset + count) as usize]
            ),
            "range row{index}"
        );
    }
    assert_eq!(
        std::fs::read(directory.join("blend-adaptive.bin")).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&prepared.adaptive)
    );
    let gpu = Gpu::new().unwrap();
    let input = gpu.upload("Acquired repaired blue", &original).unwrap();
    let output = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let actual: &[f32] = bytemuck::cast_slice(&bytes);
    let mut maximum = 0_f32;
    let mut total = 0_f64;
    let mut samples = 0;
    for y in 5..extent[1] - 5 {
        for x in 5..extent[0] - 5 {
            let index = (y * extent[0] + x) as usize;
            let error = (actual[index] - expected[index]).abs();
            maximum = maximum.max(error);
            total += f64::from(error);
            samples += 1;
        }
    }
    eprintln!(
        "acquired blue blend: {samples} samples mean{} max{maximum}",
        total / f64::from(samples)
    );
    // The retained first-strip and far-right acquired tiles measure maxima
    // 0.009766 and 0.018982 respectively, in pre-render camera codes.
    assert!(maximum <= 0.03125);
}
