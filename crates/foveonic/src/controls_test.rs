//! Checks optional GPU repairs, unsharp masking, cropped resizing, and ColorDQ conditioning.

use crate::calibration::EncodeParams;
use crate::gpu::{Gpu, Kernel};

fn floats(gpu: &Gpu, buffer: &wgpu::Buffer) -> Vec<f32> {
    bytemuck::cast_slice::<u8, f32>(&gpu.readback(buffer).unwrap()).to_vec()
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn shader_isolated_repair_keeps_unmarked_layers_and_skips_clusters() {
    let gpu = Gpu::new().unwrap();
    let mut pixels: Vec<f32> = (0..25)
        .flat_map(|p| [(p % 5 + p / 5) as f32, 10., 20.])
        .collect();
    pixels[12 * 3] = 999.;
    let mut flags = vec![0_u32; 25];
    flags[12] = 0x40ff;
    flags[0] = 0x40ff; // Border records remain untouched.
    let input = gpu.upload("Repair input", &pixels).unwrap();
    let uniform = gpu.uniform(&[5_u32, 5, 0, 0]);
    let output = gpu.storage("Repair output", 25 * 3 * 4).unwrap();
    let map = gpu.upload("Repair map", &flags).unwrap();
    gpu.dispatch(
        Kernel::Repair,
        &[(0, &input), (1, &map), (2, &uniform), (3, &output)],
        [1, 1],
        &mut Default::default(),
    );
    let result = floats(&gpu, &output);
    let mut expected = pixels.clone();
    expected[12 * 3] = 4.;
    assert_eq!(result, expected);
    flags[6] = 0x40ff; // A neighboring defect makes both repairs non-isolated.
    let map = gpu.upload("Clustered repair map", &flags).unwrap();
    gpu.dispatch(
        Kernel::Repair,
        &[(0, &input), (1, &map), (2, &uniform), (3, &output)],
        [1, 1],
        &mut Default::default(),
    );
    assert_eq!(floats(&gpu, &output), pixels);
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn shader_separable_blur_and_unsharp_blend_match_small_convolution() {
    let gpu = Gpu::new().unwrap();
    let mut pixels = [0.125_f32, 0.25, 0.5].repeat(25);
    pixels[12 * 3..12 * 3 + 3].copy_from_slice(&[1., 2., 3.]);
    let input = gpu.upload("Sharpen input", &pixels).unwrap();
    let weights = gpu.upload("Blur weights", &[0.25_f32, 0.5, 0.25]).unwrap();
    let horizontal = gpu.storage("Horizontal blur", 300).unwrap();
    let blurred = gpu.storage("Vertical blur", 300).unwrap();
    let output = gpu.storage("Sharpened", 300).unwrap();
    for (axis, source, destination) in [(0, &input, &horizontal), (1, &horizontal, &blurred)] {
        let uniform = gpu.uniform(&[5_u32, 5, axis, 1]);
        gpu.dispatch(
            Kernel::Blur,
            &[(0, source), (1, &weights), (2, &uniform), (3, destination)],
            [1, 1],
            &mut Default::default(),
        );
    }
    let mut expected = vec![0_f32; pixels.len()];
    let kernel = [0.25, 0.5, 0.25];
    for y in 0..5_i32 {
        for x in 0..5_i32 {
            for c in 0..3 {
                let mut sum = 0.;
                for dy in -1..=1_i32 {
                    for dx in -1..=1_i32 {
                        let p = ((y + dy).clamp(0, 4) * 5 + (x + dx).clamp(0, 4)) as usize;
                        sum += kernel[(dx + 1) as usize]
                            * kernel[(dy + 1) as usize]
                            * pixels[p * 3 + c];
                    }
                }
                expected[(y * 5 + x) as usize * 3 + c] = sum;
            }
        }
    }
    for (actual, expected) in floats(&gpu, &blurred).iter().zip(&expected) {
        assert!((actual - expected).abs() < 1e-6);
    }
    let mut bytes = Vec::new();
    bytes.extend(bytemuck::bytes_of(&[5_u32, 5, 0, 0]));
    bytes.extend(bytemuck::bytes_of(&[0.75_f32, 0., 0., 0.]));
    let uniform = gpu.uniform(&<[u8; 32]>::try_from(bytes).unwrap());
    gpu.dispatch(
        Kernel::Sharpen,
        &[(0, &input), (1, &blurred), (2, &uniform), (3, &output)],
        [1, 1],
        &mut Default::default(),
    );
    for ((actual, original), blurred) in floats(&gpu, &output).iter().zip(pixels).zip(expected) {
        assert!((actual - (original + 0.75 * (original - blurred))).abs() < 1e-6);
    }
}

fn code(value: f32, route: u32) -> u16 {
    let linear = value.clamp(0., 1.);
    let encoded = if route == 1 {
        linear.powf(1. / 2.2)
    } else if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1. / 2.4) - 0.055
    };
    (encoded * 65535. + 0.5).floor() as u16
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn shader_resize_preserves_crop_orientation_and_both_transfer_routes() {
    let gpu = Gpu::new().unwrap();
    let pixels: Vec<f32> = (0..20)
        .flat_map(|p| {
            let (x, y) = (p % 5, p / 5);
            if (1..4).contains(&x) && (1..3).contains(&y) {
                [p as f32 / 20., 0.25, 0.75]
            } else {
                [1., 1., 1.]
            } // Must not bleed into the active crop.
        })
        .collect();
    let input = gpu.upload("Resize input", &pixels).unwrap();
    for rotation in [0, 90, 180, 270] {
        let natural = if rotation == 90 || rotation == 270 {
            [2_u32, 3]
        } else {
            [3_u32, 2]
        };
        for size in [[1, 1], [6, 4], natural] {
            for route in [0, 1] {
                let params = EncodeParams {
                    source: [5, 4, rotation, 0],
                    crop: [1, 1, 4, 3],
                    output: [size[0], size[1], route, 0],
                };
                let uniform = gpu.uniform(&params);
                let output = gpu
                    .storage("Resized output", u64::from(size[0] * size[1]) * 8)
                    .unwrap();
                gpu.dispatch(
                    Kernel::Encode,
                    &[(0, &input), (1, &uniform), (2, &output)],
                    [1, 1],
                    &mut Default::default(),
                );
                let bytes = gpu.readback(&output).unwrap();
                for (p, actual) in bytes.as_chunks::<8>().0.iter().enumerate() {
                    let x = (((p as u32 % size[0]) as f32 + 0.5) * natural[0] as f32
                        / size[0] as f32
                        - 0.5)
                        .clamp(0., (natural[0] - 1) as f32);
                    let y = (((p as u32 / size[0]) as f32 + 0.5) * natural[1] as f32
                        / size[1] as f32
                        - 0.5)
                        .clamp(0., (natural[1] - 1) as f32);
                    let (x, y) = match rotation {
                        90 => (y, 1. - x),
                        180 => (2. - x, 1. - y),
                        270 => (2. - y, x),
                        _ => (x, y),
                    };
                    let x = x + 1.;
                    let y = y + 1.;
                    let (left, right, top, bottom) = (
                        x.floor() as usize,
                        (x.ceil() as usize).min(3),
                        y.floor() as usize,
                        (y.ceil() as usize).min(2),
                    );
                    for c in 0..3 {
                        let a = pixels[(top * 5 + left) * 3 + c] * (1. - x.fract())
                            + pixels[(top * 5 + right) * 3 + c] * x.fract();
                        let b = pixels[(bottom * 5 + left) * 3 + c] * (1. - x.fract())
                            + pixels[(bottom * 5 + right) * 3 + c] * x.fract();
                        let expected = code(a * (1. - y.fract()) + b * y.fract(), route);
                        let actual =
                            u16::from_le_bytes(actual[c * 2..c * 2 + 2].try_into().unwrap());
                        assert!(actual.abs_diff(expected) <= 3, "rotation {rotation}, size {size:?}, route {route}: {actual} vs {expected}");
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn shader_color_dq_matches_signed_lookup_math() {
    let gpu = Gpu::new().unwrap();
    let table = crate::color_dq::table([8., 12., 16.]).unwrap();
    let pixels = [
        [0_f32, 0., 0.],
        [16., 16., 16.],
        [16., -2., 0.],
        [-20., -40., 60.],
        [1000., 1800., 2000.],
        [-0.5, 0.5, 10.5],
    ]
    .concat();
    let input = gpu.upload("ColorDQ input", &pixels).unwrap();
    let lookup = gpu.upload("ColorDQ lookup", &table.values).unwrap();
    let uniform = gpu.uniform(&[6_u32, 1, table.period, 0]);
    let output = gpu
        .storage("ColorDQ output", pixels.len() as u64 * 4)
        .unwrap();
    gpu.dispatch(
        Kernel::ColorDq,
        &[(0, &input), (1, &lookup), (2, &uniform), (3, &output)],
        [1, 1],
        &mut Default::default(),
    );
    let result = floats(&gpu, &output);
    let mask = table.period as i32 - 1;
    for (actual, original) in result
        .as_chunks::<3>()
        .0
        .iter()
        .zip(pixels.as_chunks::<3>().0)
    {
        let residual: [i32; 3] = std::array::from_fn(|c| {
            let n = (original[c] + 0.5).floor() as i32;
            n - table.values[(n & mask) as usize * 3 + c]
        });
        let mean = (residual[0] + 2 * residual[1] + residual[2]) >> 2;
        for c in 0..3 {
            assert_eq!(
                actual[c],
                original[c] - table.values[((residual[c] - mean) & mask) as usize * 3 + c] as f32
            );
        }
    }
}
