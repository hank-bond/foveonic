//! Checks six-color response, neutral protection and identical gain for color and monochrome samples.
use super as color_editor;
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
        let mut params = color_editor::Params::new(
            extent,
            color_editor::Controls::from_lightness([0.; 6]),
            ColorSpace::Srgb,
            4096.,
            4096.,
            1.,
            false,
        );
        params.guide_matrix = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]];
        assert_eq!(
            read(
                &color_editor::apply(
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
            params.bands[[0, 2, 3, 4, 5, 7][sector]][2] = sign * 0.15;
            let result = read(
                &color_editor::apply(
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
                &color_editor::apply(&gpu, &input, &gray, params, extent, &mut Profile::default())
                    .unwrap(),
            );
            assert!(gray_result
                .iter()
                .zip(&mono)
                .all(|(a, b)| (a - b).abs() < 0.001));
        }
    }
    let black = gpu.upload("Black guide", &vec![0_f32; n * 3]).unwrap();
    let params = color_editor::Params::new(
        extent,
        color_editor::Controls::from_lightness([-1.; 6]),
        ColorSpace::Srgb,
        4096.,
        4096.,
        1.,
        false,
    );
    assert_eq!(
        read(
            &color_editor::apply(
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
    let params = color_editor::Params::new(
        extent,
        color_editor::Controls::from_lightness([0.3, -0.2, 0., 0.4, -0.2, 0.1]),
        ColorSpace::Srgb,
        4096.,
        4096.,
        1.,
        false,
    );
    let read = |input: &crate::gpu::ReadGuard| {
        let output =
            color_editor::apply(&gpu, input, &guide, params, extent, &mut Profile::default())
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

#[test]
fn editor_controls_migrate_and_keep_monochrome_chromatic_values_inactive() {
    let legacy = [0.4, -0.2, 0.1, 0.2, -0.3, 0.5];
    let c: color_editor::Controls = serde_json::from_value(serde_json::json!(legacy)).unwrap();
    assert_eq!(c, color_editor::Controls::from_lightness(legacy));
    assert_eq!(c.bands[1].lightness, (legacy[0] + legacy[1]) * 0.5);
    assert_eq!(c.bands[6].lightness, (legacy[4] + legacy[5]) * 0.5);
    assert_eq!(
        serde_json::from_str::<color_editor::Controls>(&serde_json::to_string(&c).unwrap())
            .unwrap(),
        c
    );
    let mut c = color_editor::Controls::default();
    c.bands[5].hue = 0.5;
    c.bands[5].saturation = -0.3;
    assert!(c.is_active(false));
    assert!(!c.is_active(true));
    c.master.lightness = 0.1;
    assert!(c.is_active(true));
    for value in [f32::NAN, f32::INFINITY, -1.01, 1.01] {
        c.bands[0].hue = value;
        assert!(c.validate().is_err());
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_editor_hue_saturation_neutral_and_monochrome_contracts() {
    let gpu = Gpu::new().unwrap();
    let extent = [32, 8];
    let n = (extent[0] * extent[1]) as usize;
    for space in [
        ColorSpace::Srgb,
        ColorSpace::AdobeRgb,
        ColorSpace::ProPhotoRgb,
    ] {
        for (name, pixel) in [
            ("red", [2000_f32, 200., 100.]),
            ("signed", [-30., 800., 3000.]),
            ("gray", [1000.; 3]),
            ("black", [0.; 3]),
        ] {
            let source = pixel.repeat(n);
            let input = gpu.upload(name, &source).unwrap();
            let read = |params| {
                let output = color_editor::apply(
                    &gpu,
                    &input,
                    &input,
                    params,
                    extent,
                    &mut Profile::default(),
                )
                .unwrap();
                let bytes = gpu.readback(&output).unwrap();
                bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
            };
            let mut c = color_editor::Controls::default();
            let params =
                |c, mono| color_editor::Params::new(extent, c, space, 4096., 4096., 1., mono);
            assert_eq!(read(params(c, false)), source);
            c.master.hue = 1.;
            c.master.saturation = 0.5;
            let edited = read(params(c, false));
            assert!(edited.iter().all(|v| v.is_finite()));
            assert_eq!(read(params(c, true)), source);
            if name == "gray" || name == "black" {
                assert!(edited
                    .iter()
                    .zip(&source)
                    .all(|(a, b)| (a - b).abs() < 0.01));
            } else {
                assert_ne!(edited, source);
            }
            c.master.hue = 0.;
            c.master.saturation = -1.;
            let desaturated = read(params(c, false));
            if name == "red" {
                let p = &desaturated[..3];
                assert!(
                    (p[0] - p[1]).abs() < 0.05 && (p[1] - p[2]).abs() < 0.05,
                    "{space:?}: {p:?}"
                );
            }
            assert_eq!(
                bytemuck::cast_slice::<u8, f32>(&gpu.readback(&input).unwrap()),
                source
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_editor_eight_ranges_leave_unselected_color_centers_unchanged() {
    let gpu = Gpu::new().unwrap();
    let colors: [[f32; 3]; 8] = [
        [2000., 0., 0.],
        [2000., 1000., 0.],
        [2000., 2000., 0.],
        [0., 2000., 0.],
        [0., 2000., 2000.],
        [0., 0., 2000.],
        [1000., 0., 2000.],
        [2000., 0., 2000.],
    ];
    // Each stripe has an interior wider than both filter passes' support.
    let extent = [256, 16];
    let pixels: Vec<f32> = (0..extent[0] * extent[1])
        .flat_map(|i| colors[(i % 256 / 32) as usize])
        .collect();
    let input = gpu.upload("Eight subject colors", &pixels).unwrap();
    for selected in 0..8 {
        let mut controls = color_editor::Controls::default();
        controls.bands[selected].hue = 0.8;
        controls.bands[selected].saturation = -0.4;
        let mut params =
            color_editor::Params::new(extent, controls, ColorSpace::Srgb, 4096., 4096., 1., false);
        params.guide_matrix = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]];
        let output = color_editor::apply(
            &gpu,
            &input,
            &input,
            params,
            extent,
            &mut Profile::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        for (band, expected) in colors.iter().enumerate() {
            let index = (8 * 256 + band * 32 + 16) * 3;
            let pixel = &actual[index..index + 3];
            if band == selected {
                assert_ne!(pixel, expected);
            } else {
                assert_eq!(pixel, expected, "selected {selected}, band {band}");
            }
        }
    }
}
