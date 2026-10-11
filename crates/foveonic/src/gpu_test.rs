//! Checks GPU black estimation, signed calibration, and RGB16 orientation on small images.

use crate::calibration::{CalibrationParams, EncodeParams};
use crate::gpu::{Gpu, Kernel};

#[test]
#[ignore = "requires a working GPU adapter"]
fn shader_black_calibration_and_orientation() {
    let gpu = Gpu::new().unwrap();
    let mut params = CalibrationParams {
        source: [10, 10, 100, 0],
        dark: [0, 0, 10, 10],
        gains: [1., 2., 0.5, 0.],
        shading: [0.; 4],
        shading_geometry: [0., 0., 1., 0.],
        grids: [[0, 1, 1, 0], [1, 1, 1, 0], [2, 1, 1, 0]],
    };
    let mut raw: Vec<f32> = (0..100).flat_map(|_| [100., 200., 300.]).collect();
    raw[..3].fill(0.);
    raw[297..].fill(2000.);
    let input = gpu.upload("Test raw", &raw).unwrap();
    let histogram = gpu.storage("Test histogram", 3 * 4096 * 4).unwrap();
    let black = gpu.storage("Test black", 12).unwrap();
    let uniform = gpu.uniform(&params);
    gpu.dispatch(
        Kernel::Histogram,
        &[(0, &input), (1, &histogram), (2, &uniform)],
        [2, 1],
        &mut Default::default(),
    );
    gpu.dispatch(
        Kernel::BlackMean,
        &[(1, &histogram), (2, &uniform), (3, &black)],
        [3, 1],
        &mut Default::default(),
    );
    let bytes = gpu.readback(&black).unwrap();
    assert_eq!(bytemuck::cast_slice::<u8, f32>(&bytes), &[100., 200., 300.]);

    params.source = [2, 2, 4, 0];
    let raw = gpu
        .upload(
            "Test calibration input",
            &[
                99_f32, 202., 305., 100., 200., 300., 101., 201., 301., 102., 202., 302.,
            ],
        )
        .unwrap();
    let grid = gpu.upload("Test gains", &[2_f32, 3., 4.]).unwrap();
    let uniform = gpu.uniform(&params);
    let calibrated = gpu.storage("Test calibrated", 48).unwrap();
    gpu.dispatch(
        Kernel::Calibration,
        &[
            (0, &raw),
            (1, &black),
            (2, &grid),
            (3, &uniform),
            (4, &calibrated),
        ],
        [1, 1],
        &mut Default::default(),
    );
    let bytes = gpu.readback(&calibrated).unwrap();
    assert_eq!(
        bytemuck::cast_slice::<u8, f32>(&bytes),
        &[-2., 12., 10., 0., 0., 0., 2., 6., 2., 4., 12., 4.]
    );

    let linear = [
        [0_f32, 0.25, 1.],
        [1., 0., 0.25],
        [0.25, 1., 0.],
        [0.5, 0.5, 0.5],
    ];
    let input = gpu.upload("Test RGB", &linear.concat()).unwrap();
    let expected_codes = |rgb: [f32; 3]| {
        rgb.map(|v| {
            let encoded = if v <= 0.0031308 {
                v * 12.92
            } else {
                1.055 * v.powf(1. / 2.4) - 0.055
            };
            (encoded * 65535. + 0.5).floor() as u16
        })
    };
    for (rotation, order) in [
        (0, [0, 1, 2, 3]),
        (90, [2, 0, 3, 1]),
        (180, [3, 2, 1, 0]),
        (270, [1, 3, 0, 2]),
    ] {
        let params = EncodeParams {
            source: [2, 2, rotation, 0],
            crop: [0, 0, 2, 2],
            output: [2, 2, 0, 0],
        };
        let uniform = gpu.uniform(&params);
        let output = gpu.storage("Test RGB16", 32).unwrap();
        gpu.dispatch(
            Kernel::Encode,
            &[(0, &input), (1, &uniform), (2, &output)],
            [1, 1],
            &mut Default::default(),
        );
        let bytes = gpu.readback(&output).unwrap();
        for (pixel, index) in bytes.as_chunks::<8>().0.iter().zip(order) {
            for (offset, expected) in [0, 2, 4].into_iter().zip(expected_codes(linear[index])) {
                let actual = u16::from_le_bytes(pixel[offset..offset + 2].try_into().unwrap());
                assert!(
                    actual.abs_diff(expected) <= 2,
                    "rotation {rotation}: {actual} vs {expected}"
                );
            }
        }
    }
}
