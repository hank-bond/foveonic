//! Checks optional GPU repairs, unsharp masking, cropped resizing, and ColorDQ conditioning.

use crate::gpu::Binding::{Read, Write};
use crate::gpu::{Gpu, Kernel};
use crate::nodes::sensor::calibration::EncodeParams;

fn floats(gpu: &Gpu, buffer: &crate::gpu::ReadGuard) -> Vec<f32> {
    bytemuck::cast_slice::<u8, f32>(&gpu.readback(buffer).unwrap()).to_vec()
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn shader_native_high_pass_and_final_sharpen_match_reference_limits_and_packing() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/sharpen-reference.json")).unwrap();
    let array = |v: &serde_json::Value| {
        v.as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect::<Vec<_>>()
    };
    let gpu = Gpu::new().unwrap();
    let pixels = array(&fixture["input"]);
    let input = gpu.upload("Native sharpen source", &pixels).unwrap();
    let uniform = |amount: f32| {
        let mut bytes = Vec::new();
        bytes.extend(bytemuck::bytes_of(&[24_u32, 20, 0, 0]));
        bytes.extend(bytemuck::bytes_of(&[amount, 0., 0., 0.]));
        gpu.uniform(&<[u8; 32]>::try_from(bytes).unwrap())
    };
    let mut signal = gpu
        .storage("Native high-pass fixture", 24 * 20 * 4)
        .unwrap();
    let p = uniform(0.);
    gpu.dispatch(
        Kernel::SharpSignal,
        &mut [(0, Read(&input)), (2, Read(&p)), (3, Write(signal.write()))],
        [3, 3],
        &mut Default::default(),
    );
    let expected = array(&fixture["signal"]);
    for (i, (a, b)) in floats(&gpu, &signal).iter().zip(expected).enumerate() {
        assert!((a - b).abs() < 0.004, "signal component {i}: {a} vs {b}");
    }
    let raw = gpu
        .upload(
            "Native quarter-code packing cases",
            &array(&fixture["application_signal"]),
        )
        .unwrap();
    for output in fixture["outputs"].as_array().unwrap() {
        let p = uniform(output["amount"].as_f64().unwrap() as f32);
        let mut target = gpu.storage("Native sharpen output", 24 * 20 * 12).unwrap();
        gpu.dispatch(
            Kernel::Sharpen,
            &mut [
                (0, Read(&input)),
                (1, Read(&raw)),
                (2, Read(&p)),
                (3, Write(target.write())),
            ],
            [3, 3],
            &mut Default::default(),
        );
        for (i, (a, b)) in floats(&gpu, &target)
            .iter()
            .zip(array(&output["rgb"]))
            .enumerate()
        {
            assert!((a - b).abs() < 0.005, "sharpen component {i}: {a} vs {b}");
        }
    }
    assert!(floats(&gpu, &input)
        .iter()
        .zip(&pixels)
        .all(|(a, b)| a.to_bits() == b.to_bits()));
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
    let transform =
        gpu.uniform(&crate::render::TiffColorSpace::Srgb.transform(crate::image::ColorSpace::Srgb));
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
                let mut output = gpu
                    .storage("Resized output", u64::from(size[0] * size[1]) * 8)
                    .unwrap();
                gpu.dispatch(
                    Kernel::Encode,
                    &mut [
                        (0, Read(&input)),
                        (1, Read(&uniform)),
                        (2, Write(output.write())),
                        (3, Read(&transform)),
                    ],
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
    let table = crate::nodes::color::color_dq::table([8., 12., 16.]).unwrap();
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
    let mut output = gpu
        .storage("ColorDQ output", pixels.len() as u64 * 4)
        .unwrap();
    gpu.dispatch(
        Kernel::ColorDq,
        &mut [
            (0, Read(&input)),
            (1, Read(&lookup)),
            (2, Read(&uniform)),
            (3, Write(output.write())),
        ],
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
