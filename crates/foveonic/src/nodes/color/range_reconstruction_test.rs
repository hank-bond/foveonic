//! Checks range reconstruction contracts and GPU matrix ordering.
use super::*;
fn controls() -> Controls {
    Controls {
        enabled: true,
        downsample: 1,
        window: 3,
        subsample: false,
        noise_scaling: 1.,
        edge_strength: 0.5,
        local_decay: 0.9,
        local_iterations: 0,
        dark_low: 2.,
        dark_high: 50.,
        diffusion_iterations: 0,
        constant_iterations: 0,
        diffusion_k: 2.,
        diffusion_step: 0.25,
        residual_strength: 0.,
        residual_epsilon: 5.,
        color_noise: [[1.; 3], [0.01; 3]],
        intensity_noise: [9., 0.0075],
        luma_weights: [1. / 3.; 3],
        matrix: [1., 0., 0., 0., 1., 0., 0., 0., 1.],
        channel_noise: [1.; 3],
        digital_gain: 1.,
    }
}
#[test]
fn rejects_invalid_controls() {
    let mut c = controls();
    c.downsample = 3;
    assert!(Prepared::new([32, 32], c).is_err());
    let mut c = controls();
    c.matrix[0] = f64::NAN;
    assert!(Prepared::new([32, 32], c).is_err());
}
#[test]
#[ignore = "requires a GPU"]
fn gpu_reconstruction_toggle_preserves_flat_signal_color_and_gain() {
    let gpu = Gpu::new().unwrap();
    let mut c = controls();
    // A bright constant field avoids the intentional dark neutralization and
    // black estimation branches. Filtering has no spatial signal to change.
    c.matrix = [2.6, -0.4, 0.2, -0.3, 2.4, 0.1, 0.2, -0.5, 2.3];
    c.channel_noise = [41., 63., 27.];
    c.digital_gain = 2.;
    c.local_iterations = 1;
    c.diffusion_iterations = 2;
    c.constant_iterations = 1;
    c.residual_strength = 1.;
    let input: Vec<f32> = [500_f32, 700., 900.].repeat(32 * 32);
    let source = gpu.upload("Flat camera RGB", &input).unwrap();
    let mut results = Vec::new();
    for enabled in [false, true] {
        c.enabled = enabled;
        let prepared = Prepared::new([32, 32], c.clone()).unwrap();
        let result = apply(&gpu, &source, &prepared, &mut Profile::default()).unwrap();
        results.push(gpu.readback(&result).unwrap());
    }
    let off = bytemuck::cast_slice::<u8, f32>(&results[0]);
    let on = bytemuck::cast_slice::<u8, f32>(&results[1]);
    // Permit one signed code from Float32 filter accumulation and final
    // integer rounding. A missing matrix or output gain cannot satisfy this.
    assert!(off.iter().zip(on).all(|(a, b)| (a - b).abs() <= 1.));
    assert!(off[0] > input[0] * 1.5);
    assert!(off[1] > input[1] * 1.5);
    assert!(off[2] > input[2] * 1.5);
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_range_constant_and_deterministic_diffusion() {
    let gpu = Gpu::new().unwrap();
    let input = vec![100_f32; 32 * 32 * 3];
    let src = gpu.upload("range constant fixture", &input).unwrap();
    for factor in [1, 2, 4] {
        let mut c = controls();
        c.downsample = factor;
        c.local_iterations = 1;
        c.diffusion_iterations = 2;
        c.constant_iterations = 1;
        let p = Prepared::new([32, 32], c).unwrap();
        let a = apply(&gpu, &src, &p, &mut Profile::default()).unwrap();
        let b = apply(&gpu, &src, &p, &mut Profile::default()).unwrap();
        let a = gpu.readback(&a).unwrap();
        assert_eq!(a, gpu.readback(&b).unwrap());
        let values = bytemuck::cast_slice::<u8, f32>(&a);
        assert!(values
            .iter()
            .all(|v| v.is_finite() && *v >= 90. && *v <= 110.));
    }
    assert_eq!(
        gpu.readback(&src).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&input)
    );
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_finalizer_applies_matrix_before_nonlinear_correction() {
    let gpu = Gpu::new().unwrap();
    let mut c = controls();
    c.channel_noise = [41., 63., 27.];
    c.matrix = [1.3, -0.2, 0.1, -0.15, 1.2, 0.05, 0.1, -0.25, 1.15];
    c.residual_strength = 0.75;
    let p = Prepared::new([8, 8], c.clone()).unwrap();
    let mut input = Vec::new();
    let mut ratios = Vec::new();
    let mut expected = Vec::new();
    for i in 0..64 {
        let rgb = [
            (i * 73 % 1100) as f32 - 150.,
            (i * 37 % 900) as f32 - 100.,
            (i * 119 % 1700) as f32 - 200.,
        ];
        input.extend([rgb[0], rgb[1], rgb[2], 0.]);
        let lum = rgb
            .iter()
            .zip(c.luma_weights)
            .map(|(v, w)| v * w)
            .sum::<f32>();
        let filtered = lum * 0.8;
        ratios.extend([filtered, 0., 0., 0.]);
        let value: [f64; 3] = std::array::from_fn(|row| {
            c.matrix[row * 3 + 2].mul_add(
                f64::from(rgb[2]),
                c.matrix[row * 3]
                    .mul_add(f64::from(rgb[0]), c.matrix[row * 3 + 1] * f64::from(rgb[1])),
            )
        });
        let length = p.tables.len() / 3;
        let limit = (length / 2 - 1) as i32;
        let lookup = |channel: usize, v: i32| {
            p.tables[channel * length + (v.clamp(-limit, limit) as u32 as usize & (length - 1))]
        };
        let first: [i32; 3] = std::array::from_fn(|ch| lookup(ch, value[ch] as i32));
        let remaining: [i32; 3] = std::array::from_fn(|ch| value[ch] as i32 - first[ch]);
        let mean = (remaining[0] + 2 * remaining[1] + remaining[2]) >> 2;
        let ratio = (c.residual_epsilon + filtered) / (c.residual_epsilon + lum.max(0.));
        let amount = (f64::from(ratio) - 1.).mul_add(f64::from(c.residual_strength), 1.);
        for ch in 0..3 {
            expected.push(
                (((value[ch] - f64::from(first[ch]) - f64::from(lookup(ch, remaining[ch] - mean)))
                    * amount)
                    .min(24000.)
                    + 0.5) as i32 as i16 as f32,
            );
        }
    }
    let input = gpu.upload("finalizer RGB", &input).unwrap();
    let ratios = gpu.upload("finalizer luminance", &ratios).unwrap();
    let uniform = gpu.uniform(&p.params);
    let tables = gpu.upload("finalizer lookup", &p.tables).unwrap();
    let black = gpu.upload("finalizer black", &[0_f32; 4]).unwrap();
    let mut output = gpu.storage("finalizer result", 64 * 12).unwrap();
    let mut trace = gpu.trace(false, 1);
    gpu.dispatch(
        Kernel::RangeFinish,
        &mut [
            (0, Read(&input)),
            (1, Read(&uniform)),
            (2, Read(&ratios)),
            (3, Write(output.write())),
            (4, Read(&tables)),
            (5, Read(&black)),
        ],
        [1, 1],
        &mut trace,
    );
    let bytes = gpu.readback(&output).unwrap();
    for (i, (actual, expected)) in bytemuck::cast_slice::<u8, f32>(&bytes)
        .iter()
        .zip(expected)
        .enumerate()
    {
        assert!(
            (*actual - expected).abs() <= 1.,
            "component {i}: {actual} vs {expected}"
        );
    }
}

#[test]
fn shader_contract_validates() {
    let module = wgpu::naga::front::wgsl::parse_str(include_str!("range_reconstruction.wgsl"))
        .expect("range shader parses");
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("range shader validates");
}

#[test]
#[ignore = "requires a GPU"]
fn disabled_filter_retains_spatial_samples_and_floating_color() {
    let gpu = Gpu::new().unwrap();
    let mut c = controls();
    c.enabled = false;
    c.downsample = 4;
    c.local_iterations = 3;
    c.diffusion_iterations = 4;
    let p = Prepared::new([16, 16], c).unwrap();
    let values: Vec<f32> = (0..16 * 16 * 3)
        .map(|i| 1000. + (i * 173 % 4000) as f32)
        .collect();
    let input = gpu.upload("unfiltered color fixture", &values).unwrap();
    let result = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
    let bytes = gpu.readback(&result).unwrap();
    for (actual, original) in bytemuck::cast_slice::<u8, f32>(&bytes).iter().zip(values) {
        assert!((*actual - original).abs() <= 1., "{actual} vs {original}");
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_reduction_matches_recovered_finite_support() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/range-reduction-reference.json"
    ))
    .unwrap();
    let w = fixture["width"].as_u64().unwrap() as u32;
    let h = fixture["height"].as_u64().unwrap() as u32;
    let input: Vec<f32> = fixture["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect();
    let gpu = Gpu::new().unwrap();
    let src = gpu.upload("reduction signed fixture", &input).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut c = controls();
        c.downsample = case["factor"].as_u64().unwrap() as u32;
        let factor = c.downsample;
        let p = Prepared::new([w, h], c).unwrap();
        let uniform = gpu.uniform(&p.params);
        let rw = w / factor;
        let rh = h / factor;
        let mut output = gpu
            .storage("reduction output", u64::from(rw * rh) * 16)
            .unwrap();
        let mut trace = gpu.trace(false, 1);
        gpu.dispatch(
            Kernel::RangeReduce,
            &mut [
                (0, Read(&src)),
                (1, Read(&uniform)),
                (3, Write(output.write())),
            ],
            [rw.div_ceil(8), rh.div_ceil(8)],
            &mut trace,
        );
        let bytes = gpu.readback(&output).unwrap();
        let values = bytemuck::cast_slice::<u8, f32>(&bytes);
        let rows = case["written_rows"].as_u64().unwrap() as usize;
        for i in 0..rows * rw as usize {
            for ch in 0..3 {
                let expected = case["expected"][3 * i + ch].as_f64().unwrap() as f32;
                assert!(
                    (values[4 * i + ch] - expected).abs() < 0.00025,
                    "factor {factor} pixel {i} channel {ch}: {} vs {expected}",
                    values[4 * i + ch]
                );
            }
        }
    }
}
