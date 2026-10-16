//! Checks GPU black estimation, signed calibration, and RGB16 orientation on small images.

use crate::gpu::Binding::{Read, Write};
use crate::gpu::{Gpu, Kernel};
use crate::nodes::sensor::calibration::{CalibrationParams, EncodeParams};

#[test]
#[ignore = "requires a working GPU adapter"]
fn activity_counts_engine_owned_transfers_and_not_device_copies_as_readbacks() {
    let gpu = Gpu::new().unwrap();
    let before = gpu.activity();
    let source = gpu.upload("Activity source", &[1_f32, 2., 3., 4.]).unwrap();
    let mut target = gpu.storage("Activity target", 16).unwrap();
    let _uniform = gpu.uniform(&[1_u32; 4]);
    gpu.copy(&source, target.write(), 16);
    let pending = gpu.activity().since(before);
    assert_eq!(pending.upload_bytes, 16);
    assert_eq!(pending.uniform_bytes, 16);
    assert_eq!(pending.storage_bytes, 16);
    assert_eq!(pending.allocations, 3);
    assert_eq!(pending.submissions, 1);
    assert_eq!(pending.device_copy_bytes, 16);
    assert_eq!(pending.readback_bytes, 0);
    assert_eq!(pending.readbacks, 0);
    assert_eq!(
        bytemuck::cast_slice::<u8, f32>(&gpu.readback(&target).unwrap()),
        &[1., 2., 3., 4.]
    );
    let complete = gpu.activity().since(before);
    assert_eq!(complete.readback_bytes, 16);
    assert_eq!(complete.readbacks, 1);
    assert_eq!(complete.submissions, 2);
    assert_eq!(complete.timestamp_resolves, 0);
}

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
    let mut histogram = gpu.storage("Test histogram", 3 * 4096 * 4).unwrap();
    let mut black = gpu.storage("Test black", 12).unwrap();
    let uniform = gpu.uniform(&params);
    gpu.dispatch(
        Kernel::Histogram,
        &mut [
            (0, Read(&input)),
            (1, Write(histogram.write())),
            (2, Read(&uniform)),
        ],
        [2, 1],
        &mut Default::default(),
    );
    gpu.dispatch(
        Kernel::BlackMean,
        &mut [
            (1, Read(&histogram)),
            (2, Read(&uniform)),
            (3, Write(black.write())),
        ],
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
    let mut calibrated = gpu.storage("Test calibrated", 48).unwrap();
    gpu.dispatch(
        Kernel::Calibration,
        &mut [
            (0, Read(&raw)),
            (1, Read(&black)),
            (2, Read(&grid)),
            (3, Read(&uniform)),
            (4, Write(calibrated.write())),
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
    let transform =
        gpu.uniform(&crate::render::TiffColorSpace::Srgb.transform(crate::image::ColorSpace::Srgb));
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
        let mut output = gpu.storage("Test RGB16", 32).unwrap();
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
