//! Checks six-color response, neutral protection and identical gain for color and monochrome samples.
use super as sensitivity;
use crate::{color_space::ColorSpace, gpu::Gpu, profile::Profile};

#[test]
#[ignore = "requires a GPU"]
fn gpu_color_sensitivity_sectors_gray_and_signed_samples() {
    let gpu = Gpu::new().unwrap();
    let extent = [24, 16];
    let n = (extent[0] * extent[1]) as usize;
    let mono: Vec<f32> = (0..n)
        .flat_map(|i| [if i == 0 { -1. } else { 100. }; 3])
        .collect();
    let input = gpu.upload("Mono fixture", &mono).unwrap();
    let read = |buffer: &crate::gpu::ReadGuard| {
        let bytes = gpu.readback(buffer).unwrap();
        bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
    };
    // Use explicit reference coordinates to isolate the response arithmetic.
    for (sector, color) in [
        [1_f32, 0., 0.],
        [1., 1., 0.],
        [0., 1., 0.],
        [0., 1., 1.],
        [0., 0., 1.],
        [1., 0., 1.],
    ]
    .into_iter()
    .enumerate()
    {
        let guide = gpu
            .upload("Color fixture", &color.map(|v| v * 1000.).repeat(n))
            .unwrap();
        let mut params = sensitivity::Params::new(extent, [0.; 6], ColorSpace::Srgb, 4096., 1.);
        params.guide_matrix = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]];
        assert_eq!(
            read(
                &sensitivity::apply(
                    &gpu,
                    &input,
                    &guide,
                    params,
                    extent,
                    &mut Profile::default()
                )
                .unwrap()
            ),
            mono
        );
        for sign in [-1., 1.] {
            params.hues[sector / 4][sector % 4] = sign * 0.15;
            let result = read(
                &sensitivity::apply(
                    &gpu,
                    &input,
                    &guide,
                    params,
                    extent,
                    &mut Profile::default(),
                )
                .unwrap(),
            );
            assert!(result.iter().all(|v| v.is_finite()));
            assert!(
                (result[3] - 100.) * sign > 0.,
                "sector {sector}, sign {sign}"
            );
            assert!(result
                .as_chunks::<3>()
                .0
                .iter()
                .all(|p| p[0] == p[1] && p[1] == p[2]));
            let gray = gpu.upload("Gray fixture", &vec![1000_f32; n * 3]).unwrap();
            let gray_result = read(
                &sensitivity::apply(&gpu, &input, &gray, params, extent, &mut Profile::default())
                    .unwrap(),
            );
            assert!(gray_result
                .iter()
                .zip(&mono)
                .all(|(a, b)| (a - b).abs() < 0.001));
        }
    }
    let black = gpu.upload("Black guide", &vec![0_f32; n * 3]).unwrap();
    let params = sensitivity::Params::new(extent, [-1.; 6], ColorSpace::Srgb, 4096., 1.);
    assert_eq!(
        read(
            &sensitivity::apply(
                &gpu,
                &input,
                &black,
                params,
                extent,
                &mut Profile::default()
            )
            .unwrap()
        ),
        mono
    );
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_color_and_monochrome_share_the_same_chromaticity_preserving_gain() {
    let gpu = Gpu::new().unwrap();
    let extent = [32, 24];
    let n = (extent[0] * extent[1]) as usize;
    let guide: Vec<f32> = (0..n)
        .flat_map(|i| {
            if i % 32 < 16 {
                [500., 200., 100.]
            } else {
                [40., 200., 350.]
            }
        })
        .collect();
    let mono = vec![100_f32; n * 3];
    let color: Vec<f32> = (0..n).flat_map(|i| [i as f32 - 30., 500., 900.]).collect();
    let guide = gpu.upload("Neutral color classifier", &guide).unwrap();
    let mono = gpu.upload("Mono brightness", &mono).unwrap();
    let color_input = gpu
        .upload("Color brightness and signed headroom", &color)
        .unwrap();
    let params = sensitivity::Params::new(
        extent,
        [0.3, -0.2, 0., 0.4, -0.2, 0.1],
        ColorSpace::Srgb,
        4096.,
        1.,
    );
    let read = |input: &crate::gpu::ReadGuard| {
        let output =
            sensitivity::apply(&gpu, input, &guide, params, extent, &mut Profile::default())
                .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
    };
    let mono = read(&mono);
    let result = read(&color_input);
    for i in 0..n {
        let gain = mono[i * 3] / 100.;
        for c in 0..3 {
            let expected = color[i * 3 + c] * gain;
            assert!(
                (result[i * 3 + c] - expected).abs() < 0.0003,
                "pixel {i}, channel {c}"
            );
        }
    }
}
