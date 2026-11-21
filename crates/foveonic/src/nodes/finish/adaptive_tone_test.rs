//! Checks profile tuning, exposure-aware graph reuse, numerical safety and photographic export behavior.

use super::super::base_response::Limits;
use super::*;
use crate::{
    inspection::Content,
    profile::Mode,
    render::{
        ColorFieldApproach, ControlCapabilities, Engine, HighlightMode, NativePhoto, Rendering,
        Settings,
    },
    source::Source,
};
use std::sync::Arc;

#[test]
#[ignore = "requires GPU"]
fn reduced_analysis_keeps_structure_in_histogram_and_regional_samples() {
    let gpu = Gpu::test_executor();
    let extent = [64, 64];
    let values: Vec<f32> = (0..64)
        .flat_map(|_| (0..64).flat_map(|x| [4095. * (-13. + x as f32 / 16.).exp2(); 3]))
        .collect();
    let input = gpu.upload("Working log-light ramp", &values).unwrap();
    let mask = gpu
        .upload("Fixed brightness guide", &vec![0.5_f32; 32 * 32])
        .unwrap();
    let curve: Vec<_> = (0..65536).map(|v| (v as f32 / 4095.).min(1.)).collect();
    for divisor in [1, 2, 4, 8, 16, 64] {
        let params = Params::new(
            extent,
            [0, 0, 64, 64],
            4095.,
            ColorSpace::Srgb,
            BaseResponse::Natural,
            Tuning::default(),
            &curve,
            ProcessingQuality::Speed,
        )
        .unwrap()
        .with_geometry(
            extent,
            [0, 0, 64, 64],
            crate::render::Scale::new(divisor).unwrap().factor(),
        )
        .unwrap();
        let context = gpu.uniform(&params);
        let mut histogram = gpu
            .upload("Empty adaptive histogram", &vec![0_u32; 12288])
            .unwrap();
        let mut samples = gpu
            .storage("Regional structure samples", 256 * 256 * 16)
            .unwrap();
        let mut trace = gpu.trace(false, 2);
        gpu.dispatch(
            Kernel::AdaptiveToneHistogram,
            &mut [
                (0, Read(&input)),
                (1, Read(&mask)),
                (3, Read(&context)),
                (4, Write(histogram.write())),
            ],
            [32, 32],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::AdaptiveToneSampleRegions,
            &mut [
                (0, Read(&input)),
                (1, Read(&mask)),
                (3, Read(&context)),
                (8, Write(samples.write())),
            ],
            [32, 32],
            &mut trace,
        );
        let bytes = gpu.readback(&histogram).unwrap();
        let counts: &[u32] = bytemuck::cast_slice(&bytes);
        assert_eq!(counts[..4096].iter().sum::<u32>(), 256 * 256);
        let structure = counts[4096..8192]
            .iter()
            .map(|v| u64::from(*v))
            .sum::<u64>();
        let bytes = gpu.readback(&samples).unwrap();
        let samples: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
        let regional = samples
            .iter()
            .map(|sample| f64::from(sample[3]))
            .sum::<f64>();
        eprintln!(
            "divisor {divisor}: histogram structure {structure}, regional structure {regional}"
        );
        assert!(
            structure > 0,
            "histogram lost the ramp at divisor {divisor}"
        );
        assert!(
            regional > 0.,
            "regional samples lost the ramp at divisor {divisor}"
        );
    }
}

#[test]
#[ignore = "requires GPU, FOVEONIC_TONAL_SOURCE and FOVEONIC_TONAL_OUTPUT for an isolated limit sweep"]
fn photographic_shadow_limit_sweep() {
    use std::path::PathBuf;
    let path = PathBuf::from(std::env::var_os("FOVEONIC_TONAL_SOURCE").unwrap());
    let directory = PathBuf::from(std::env::var_os("FOVEONIC_TONAL_OUTPUT").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let source = Arc::new(Source::open(&path).unwrap());
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut settings = crate::sidecar::load(&path).unwrap().unwrap_or_else(|| {
        caps.editing_defaults(&source.metadata, Rendering::Color)
            .unwrap()
    });
    settings.processing_quality = ProcessingQuality::Speed;
    if settings.white_balance.is_empty() {
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
    }
    if settings.color_mode.is_empty() {
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
    }
    if let Ok(value) = std::env::var("FOVEONIC_TONAL_EXPOSURE") {
        settings.exposure = value.parse().unwrap();
    }
    settings.photographic.base_response = BaseResponse::Natural;
    settings.photographic.tone_method = crate::render::PhotographicMethod::Universal;
    settings.output_size = Some([864, 1296]);
    let engine = Engine::new().unwrap();
    let mut session = NativePhoto::new(source.clone());
    let mut artifacts = engine
        .render_native_photo(
            &mut session,
            &settings,
            &["linear.edits-input", "base-response.mask"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    let Content::Image(input) = artifacts.remove(0).content else {
        panic!("RGB")
    };
    let Content::Image(mask) = artifacts.remove(0).content else {
        panic!("guide")
    };
    let crate::image::Space::LinearRgb(space) = input.description.space else {
        panic!("linear RGB")
    };
    let mode =
        crate::nodes::color::color_mode::prepare(&source.metadata, &settings.color_mode).unwrap();
    let curve = super::super::tone::prepare(&source.metadata, &mode, Default::default()).unwrap();
    let mut records = Vec::new();
    let mut previous: Option<(Vec<f32>, f64)> = None;
    for hundredths in 0..=100 {
        settings.adaptive_tone_tuning.natural.shadows = hundredths as f32 / 100.;
        let params = Params::new(
            input.description.extent,
            input.view.crop,
            input.description.linear_white as f32,
            space,
            BaseResponse::Natural,
            settings.adaptive_tone_tuning,
            &curve,
            ProcessingQuality::Speed,
        )
        .unwrap()
        .with_exposure(settings.exposure)
        .unwrap();
        let result = apply(
            engine.test_gpu(),
            &input.buffer,
            &mask.buffer,
            &curve,
            params,
            &mut Profile::default(),
        )
        .unwrap();
        let chosen = engine.test_gpu().readback(&result.amounts).unwrap();
        let chosen = bytemuck::cast_slice::<u8, f32>(&chosen).to_vec();
        let values = engine.test_gpu().readback(&result.rgb).unwrap();
        let values = bytemuck::cast_slice::<u8, f32>(&values);
        assert!(values.iter().all(|v| v.is_finite()));
        let mean = values.iter().map(|v| f64::from(*v)).sum::<f64>() / values.len() as f64;
        if let Some((target, before)) = &previous {
            assert_eq!(&chosen[..3], &target[..3], "cap changed the scene target");
            assert!(
                mean + 1e-5 >= *before,
                "raising the shadow cap darkened the frame"
            );
        }
        previous = Some((chosen.clone(), mean));
        eprintln!("shadow cap {hundredths}: fit {chosen:?}; mean linear RGB {mean}");
        if (87..=92).contains(&hundredths) {
            let mut images = engine
                .render_native_photo(
                    &mut session,
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap();
            let Content::Image(image) = images.remove(0).content else {
                panic!("photo")
            };
            engine
                .write_tiff(
                    &image,
                    &directory.join(format!("cap-{hundredths}.tiff")),
                    &mut Profile::default(),
                )
                .unwrap();
        }
        records.push(serde_json::json!({"cap":settings.adaptive_tone_tuning.natural.shadows,"chosen":chosen,"mean_linear_rgb":mean}));
    }
    std::fs::write(
        directory.join("metrics.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"settings":settings,"sweep":records}))
            .unwrap(),
    )
    .unwrap();
}

#[test]
fn limits_are_shared_runtime_inputs_and_profile_is_a_photo_edit() {
    let settings = Settings {
        adaptive_tone_tuning: Tuning {
            natural: Limits {
                blacks: 0.03,
                shadows: 0.9,
                highlights: 0.8,
                whites: 0.7,
            },
            ..Default::default()
        },
        photographic: super::super::photographic::Controls {
            base_response: BaseResponse::Strong,
            ..Default::default()
        },
        ..Default::default()
    };
    let encoded = crate::sidecar::encode(&settings).unwrap();
    let decoded = crate::sidecar::decode(&encoded).unwrap();
    assert_eq!(decoded.photographic.base_response, BaseResponse::Strong);
    assert_eq!(decoded.adaptive_tone_tuning, Tuning::default());
    assert_eq!(settings.adaptive_tone_tuning.natural.shadows, 0.9);
    for value in [f32::NAN, -0.1, 1.1] {
        assert!(Limits {
            shadows: value,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_TONAL_SOURCE pointing to a Merrill X3F"]
fn regional_app_response_bakes_exports_and_preserves_reference() {
    let source =
        Arc::new(Source::open(std::env::var_os("FOVEONIC_TONAL_SOURCE").unwrap()).unwrap());
    let engine = Engine::new().unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        let mut settings = caps.editing_defaults(&source.metadata, rendering).unwrap();
        settings.processing_quality = ProcessingQuality::Speed;
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
        settings.highlight_mode = HighlightMode::ColorField;
        settings.color_field.approach = ColorFieldApproach::LocalGradient;
        settings.output_size = Some([320, 480]);
        let original = engine
            .render_dng_base(&source, &settings, &mut Profile::default())
            .unwrap();
        let original = engine.readback_image(&original).unwrap();
        let camera = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        let camera = engine.readback_image(&camera).unwrap();
        settings.photographic.base_response = BaseResponse::Strong;
        settings.photographic.tone_method = crate::render::PhotographicMethod::Reference;
        let reference = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(camera, engine.readback_image(&reference).unwrap());
        let reference_dng = engine
            .render_dng_base(&source, &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(original, engine.readback_image(&reference_dng).unwrap());
        settings.photographic.tone_method = crate::render::PhotographicMethod::Universal;
        for response in [BaseResponse::Natural, BaseResponse::Strong] {
            settings.photographic.base_response = response;
            let mut images = engine
                .render_artifacts(
                    &source,
                    &settings,
                    &["linear.base", "base-response.mask", "base-response.rgb"],
                    &mut Profile::default(),
                )
                .unwrap();
            let Content::Image(rgb) = images.remove(0).content else {
                panic!("RGB")
            };
            let Content::Image(mask) = images.remove(0).content else {
                panic!("mask")
            };
            let Content::Image(actual) = images.remove(0).content else {
                panic!("adaptive RGB")
            };
            assert!(!engine.readback_image(&actual).unwrap().is_empty());
            assert!(bytemuck::cast_slice::<u8, f32>(
                &engine.test_gpu().readback(&actual.buffer).unwrap()
            )
            .iter()
            .all(|v| v.is_finite()));
            drop((images, rgb, mask, actual));
            let baked = engine
                .render_dng_base(&source, &settings, &mut Profile::default())
                .unwrap();
            let baked = engine.readback_image(&baked).unwrap();
            assert_ne!(baked, original, "adaptive gain must be baked for DNG");
            assert!(bytemuck::cast_slice::<u8, f32>(&baked)
                .iter()
                .all(|v| v.is_finite()));
        }
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_TONAL_SOURCE pointing to a Merrill X3F"]
fn adaptive_sessions_reuse_guide_and_invalidate_limits_without_touching_camera() {
    let source =
        Arc::new(Source::open(std::env::var_os("FOVEONIC_TONAL_SOURCE").unwrap()).unwrap());
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut settings = caps
        .editing_defaults(&source.metadata, Rendering::Color)
        .unwrap();
    settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
    settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
    settings.processing_quality = ProcessingQuality::Speed;
    // Keep the positive reuse check below the checkpoint cache capacity. Full
    // admission pressure is a separate cache-policy case, not invalidation.
    settings.working_scale = crate::render::Scale::new(2).unwrap();
    settings.output_size = Some([320, 480]);
    settings.photographic.tone_method = crate::render::PhotographicMethod::Universal;
    let engine = Engine::new().unwrap();
    let camera = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    let camera = engine.readback_image(&camera).unwrap();
    settings.photographic.base_response = BaseResponse::Natural;
    let mut session = NativePhoto::new(source.clone());
    engine
        .render_native_photo(
            &mut session,
            &settings,
            &["final"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    settings.exposure = -0.2;
    let mut profile = Profile::new(Mode::Gpu);
    engine
        .render_native_photo(&mut session, &settings, &["final"], || false, &mut profile)
        .unwrap()
        .unwrap();
    assert!(
        profile
            .gpu
            .iter()
            .any(|t| t.stage.starts_with("adaptive tone")),
        "Exposure did not refit the baseline"
    );
    assert!(
        !profile.gpu.iter().any(|t| t.stage == "reduced tonal guide"),
        "Exposure rebuilt the guide"
    );
    settings.adaptive_tone_tuning.natural.shadows = 0.9;
    let mut profile = Profile::new(Mode::Gpu);
    let mut cached = engine
        .render_native_photo(&mut session, &settings, &["final"], || false, &mut profile)
        .unwrap()
        .unwrap();
    assert!(profile
        .gpu
        .iter()
        .any(|t| t.stage == "adaptive tone regional candidate fit"));
    assert!(
        !profile.gpu.iter().any(|t| t.stage == "reduced tonal guide"),
        "limit edit rebuilt the guide"
    );
    let Content::Image(cached) = cached.remove(0).content else {
        panic!("image")
    };
    let fresh = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    assert_eq!(
        engine.readback_image(&cached).unwrap(),
        engine.readback_image(&fresh).unwrap()
    );
    settings.photographic.base_response = BaseResponse::Camera;
    settings.exposure = 0.;
    let after = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    assert_eq!(camera, engine.readback_image(&after).unwrap());
}

#[test]
#[ignore = "requires GPU"]
fn adaptive_response_preserves_signed_input_and_zero_caps_apply_no_exposure() {
    use crate::color_space::ColorSpace;
    let gpu = Gpu::test_executor();
    let extent = [256, 256];
    let mut values = vec![0.01 * 4095.; 256 * 256 * 3];
    for y in 100..130 {
        for x in 100..130 {
            let i = (y * 256 + x) * 3;
            values[i..i + 3].fill((1.1 + (x - 100) as f32 / 100.) * 4095.);
        }
    }
    values[..3].copy_from_slice(&[-4., -2., -1.]);
    values[3..6].fill(0.);
    let input = gpu.upload("signed adaptive fixture", &values).unwrap();
    let before = gpu.readback(&input).unwrap();
    let curve: Vec<_> = (0..65536).map(|v| (v as f32 / 4095.).min(1.)).collect();
    let guide = tone_equalizer::build_guide(
        &gpu,
        &input,
        tone_equalizer::Guide::new(extent, 48, ColorSpace::Srgb).unwrap(),
        &mut Profile::default(),
    )
    .unwrap();
    let mut dark = Vec::new();
    for response in [BaseResponse::Natural, BaseResponse::Strong] {
        let params = Params::new(
            extent,
            [0, 0, 256, 256],
            4095.,
            ColorSpace::Srgb,
            response,
            Tuning::default(),
            &curve,
            ProcessingQuality::Speed,
        )
        .unwrap();
        let output = apply(
            &gpu,
            &input,
            &guide,
            &curve,
            params,
            &mut Profile::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output.rgb).unwrap();
        let result = bytemuck::cast_slice::<u8, f32>(&bytes);
        assert!(result.iter().all(|v| v.is_finite()));
        assert!(result[..3].iter().all(|v| *v < 0.));
        assert!((result[0] / result[1] - 2.).abs() < 1e-5);
        assert_eq!(&result[3..6], &[0., 0., 0.]);
        dark.push(result[(200 * 256 + 200) * 3]);
        let zero = Limits {
            blacks: 0.,
            shadows: 0.,
            highlights: 0.,
            whites: 0.,
        };
        let mut params = Params::new(
            extent,
            [0, 0, 256, 256],
            4095.,
            ColorSpace::Srgb,
            response,
            Tuning {
                natural: zero,
                strong: zero,
            },
            &curve,
            ProcessingQuality::Speed,
        )
        .unwrap()
        .with_exposure(2.)
        .unwrap();
        params.values[2] = response.strength();
        let output = apply(
            &gpu,
            &input,
            &guide,
            &curve,
            params,
            &mut Profile::default(),
        )
        .unwrap();
        assert_eq!(
            before,
            gpu.readback(&output.rgb).unwrap(),
            "zero caps must not apply Exposure or other edits"
        );
    }
    assert!(
        dark[1] + 1e-5 >= dark[0],
        "Strong darkened the protected shadow cell"
    );
    assert_eq!(
        before,
        gpu.readback(&input).unwrap(),
        "retained input changed"
    );

    let uniform = gpu
        .upload(
            "already visible flat gray",
            &vec![0.18_f32 * 4095.; 256 * 256 * 3],
        )
        .unwrap();
    let original = gpu.readback(&uniform).unwrap();
    let guide = tone_equalizer::build_guide(
        &gpu,
        &uniform,
        tone_equalizer::Guide::new(extent, 48, ColorSpace::Srgb).unwrap(),
        &mut Profile::default(),
    )
    .unwrap();
    for response in [BaseResponse::Natural, BaseResponse::Strong] {
        let params = Params::new(
            extent,
            [0, 0, 256, 256],
            4095.,
            ColorSpace::Srgb,
            response,
            Tuning::default(),
            &curve,
            ProcessingQuality::Speed,
        )
        .unwrap();
        let output = apply(
            &gpu,
            &uniform,
            &guide,
            &curve,
            params,
            &mut Profile::default(),
        )
        .unwrap();
        assert_eq!(
            original,
            gpu.readback(&output.rgb).unwrap(),
            "profile forced a correction without evidence"
        );
    }
}

#[test]
#[ignore = "requires GPU"]
fn blacks_rescue_only_structured_near_black_without_flattening_or_touching_visible_tones() {
    use crate::color_space::ColorSpace;
    let gpu = Gpu::test_executor();
    let extent = [512, 512];
    let mut values = vec![0_f32; 512 * 512 * 3];
    for y in 32..224 {
        for x in 32..224 {
            let i = (y * 512 + x) * 3;
            values[i..i + 3].fill((0.0001 + (x - 32) as f32 / 192. * 0.0004) * 4095.);
        }
    }
    for y in 32..224 {
        for x in 288..480 {
            values[(y * 512 + x) * 3..(y * 512 + x) * 3 + 3].fill(0.00025 * 4095.);
        }
    }
    for y in 288..480 {
        for x in 32..224 {
            values[(y * 512 + x) * 3..(y * 512 + x) * 3 + 3].fill(0.18 * 4095.);
        }
    }
    values[..3].copy_from_slice(&[-4., -2., -1.]);
    let input = gpu.upload("toe protection fixture", &values).unwrap();
    let before = gpu.readback(&input).unwrap();
    let curve: Vec<_> = (0..65536).map(|v| (v as f32 / 4095.).min(1.)).collect();
    let mask = tone_equalizer::build_guide(
        &gpu,
        &input,
        tone_equalizer::Guide::new(extent, 48, ColorSpace::Srgb).unwrap(),
        &mut Profile::default(),
    )
    .unwrap();
    for response in [BaseResponse::Natural, BaseResponse::Strong] {
        let mut disabled = Tuning::default();
        disabled.natural.blacks = 0.;
        disabled.strong.blacks = 0.;
        let params = Params::new(
            extent,
            [0, 0, 512, 512],
            4095.,
            ColorSpace::Srgb,
            response,
            disabled,
            &curve,
            ProcessingQuality::Speed,
        )
        .unwrap();
        let baseline = apply(&gpu, &input, &mask, &curve, params, &mut Profile::default()).unwrap();
        let baseline = gpu.readback(&baseline.rgb).unwrap();
        let original = bytemuck::cast_slice::<u8, f32>(&baseline);
        let params = Params::new(
            extent,
            [0, 0, 512, 512],
            4095.,
            ColorSpace::Srgb,
            response,
            Tuning::default(),
            &curve,
            ProcessingQuality::Speed,
        )
        .unwrap();
        let after = apply(&gpu, &input, &mask, &curve, params, &mut Profile::default()).unwrap();
        let after = gpu.readback(&after.rgb).unwrap();
        let result = bytemuck::cast_slice::<u8, f32>(&after);
        assert!(result.iter().all(|v| v.is_finite()));
        assert_eq!(result[(350 * 512 + 350) * 3], 0., "exact black moved");
        assert_eq!(
            result[(128 * 512 + 380) * 3],
            original[(128 * 512 + 380) * 3],
            "flat darkness was lifted"
        );
        assert_eq!(
            result[(380 * 512 + 128) * 3],
            original[(380 * 512 + 128) * 3],
            "visible midtones changed"
        );
        let left = (128 * 512 + 112) * 3;
        let right = (128 * 512 + 144) * 3;
        eprintln!(
            "{response:?}: dark {} -> {}, gradient {} -> {}",
            original[left],
            result[left],
            original[right] - original[left],
            result[right] - result[left]
        );
        assert!(
            result[left] > original[left] * 1.01,
            "structured near-black detail was not rescued"
        );
        assert!(
            result[right] > result[left],
            "toe flattened distinct dark tones"
        );
        let cap = Tuning::default().for_response(response).blacks;
        let bound = 1. + cap * (4.2_f32.exp2() - 1.);
        for (a, b) in original.iter().zip(result) {
            if a.abs() > 1e-4 {
                assert!(
                    b.abs() <= a.abs() * (bound + 0.002),
                    "black cap was exceeded"
                );
            }
        }
    }
    assert_eq!(before, gpu.readback(&input).unwrap());
}

#[test]
#[ignore = "requires GPU"]
fn mapped_application_guide_keeps_fit_and_overlapping_sections_aligned() {
    let gpu = Gpu::test_executor();
    let extent = [64, 64];
    let values: Vec<f32> = (0..64)
        .flat_map(|y| {
            (0..64).flat_map(move |x| {
                let v = 4095. * (-11. + (x + y) as f32 / 16.).exp2();
                [v, v * 0.9, v * 0.8]
            })
        })
        .collect();
    let input = gpu.upload("Native structured RGB", &values).unwrap();
    let analysis_values: Vec<f32> = (0..256).flat_map(|i| [16. + i as f32; 3]).collect();
    let analysis_rgb = gpu.upload("Analysis RGB", &analysis_values).unwrap();
    let mask_values: Vec<f32> = (0..64).map(|i| 0.2 + i as f32 / 128.).collect();
    let mask = gpu.upload("Compact analysis guide", &mask_values).unwrap();
    let curve: Vec<_> = (0..65536).map(|v| (v as f32 / 4095.).min(1.)).collect();
    let params = Params::new(
        extent,
        [3, 5, 60, 58],
        4095.,
        ColorSpace::Srgb,
        BaseResponse::Natural,
        Tuning::default(),
        &curve,
        ProcessingQuality::Speed,
    )
    .unwrap();
    let analysis_params = params
        .with_geometry([16, 16], [0, 0, 16, 16], 0.25)
        .unwrap();
    let native_mask_values: Vec<f32> = (0..1024).map(|i| 0.2 + i as f32 / 2048.).collect();
    let native_mask = gpu
        .upload("Nonconstant native guide", &native_mask_values)
        .unwrap();
    let mut identity_outputs = Vec::new();
    for application in [
        params,
        params
            .with_application_guide(params, [[1., 0., 0., 0.], [0., 1., 0., 0.]], true)
            .unwrap(),
    ] {
        let result = apply_with_analysis(
            &gpu,
            Samples {
                rgb: &input,
                mask: &native_mask,
            },
            Samples {
                rgb: &analysis_rgb,
                mask: &mask,
            },
            &curve,
            application,
            analysis_params,
            &mut Profile::default(),
        )
        .unwrap();
        identity_outputs.push(gpu.readback(&result.rgb).unwrap());
    }
    assert_ne!(
        identity_outputs[0],
        bytemuck::cast_slice::<f32, u8>(&values),
        "fixture needs an active correction"
    );
    assert_eq!(
        identity_outputs[0], identity_outputs[1],
        "mapped identity shifted compact guide centers"
    );
    // Odd active boundaries exercise nearest-cell probes, while variable
    // padding cells still contribute to the interpolated application gains.
    let cropped: Vec<f32> = (16..64)
        .flat_map(|y| values[(y * 64 + 16) * 3..(y * 64 + 64) * 3].iter().copied())
        .collect();
    let cropped = gpu.upload("Same-grid edge RGB", &cropped).unwrap();
    let regional = apply_with_analysis(
        &gpu,
        Samples {
            rgb: &cropped,
            mask: &native_mask,
        },
        Samples {
            rgb: &analysis_rgb,
            mask: &mask,
        },
        &curve,
        params
            .with_geometry([48, 48], [0, 0, 44, 42], 1.)
            .unwrap()
            .with_application_guide(params, [[1., 0., 16., 0.], [0., 1., 16., 0.]], true)
            .unwrap(),
        analysis_params,
        &mut Profile::default(),
    )
    .unwrap();
    let regional = gpu.readback(&regional.rgb).unwrap();
    let regional: &[f32] = bytemuck::cast_slice(&regional);
    let whole: &[f32] = bytemuck::cast_slice(&identity_outputs[0]);
    // Include odd view coordinates and the active right and bottom edges.
    for y in 5..42 {
        for x in 5..44 {
            let a = (y * 48 + x) * 3;
            let b = ((y + 16) * 64 + x + 16) * 3;
            assert_eq!(
                &regional[a..a + 3],
                &whole[b..b + 3],
                "same-grid edge shifted at {x},{y}"
            );
        }
    }
    for mapping in [
        [[0.25, 0., -0.375, 0.], [0., 0.25, -0.375, 0.]],
        [[0., 0.25, -0.375, 0.], [-0.25, 0., 15.375, 0.]],
    ] {
        let whole = apply_with_analysis(
            &gpu,
            Samples {
                rgb: &input,
                mask: &mask,
            },
            Samples {
                rgb: &analysis_rgb,
                mask: &mask,
            },
            &curve,
            params
                .with_application_guide(analysis_params, mapping, false)
                .unwrap(),
            analysis_params,
            &mut Profile::default(),
        )
        .unwrap();
        let cropped: Vec<f32> = (16..48)
            .flat_map(|y| values[(y * 64 + 16) * 3..(y * 64 + 48) * 3].iter().copied())
            .collect();
        let cropped = gpu.upload("Regional native RGB", &cropped).unwrap();
        let mut regional_mapping = mapping;
        for row in &mut regional_mapping {
            row[2] += 16. * (row[0] + row[1]);
        }
        let region = apply_with_analysis(
            &gpu,
            Samples {
                rgb: &cropped,
                mask: &mask,
            },
            Samples {
                rgb: &analysis_rgb,
                mask: &mask,
            },
            &curve,
            params
                .with_geometry([32, 32], [0, 0, 32, 32], 1.)
                .unwrap()
                .with_application_guide(analysis_params, regional_mapping, false)
                .unwrap(),
            analysis_params,
            &mut Profile::default(),
        )
        .unwrap();
        assert_eq!(
            gpu.readback(&whole.amounts).unwrap(),
            gpu.readback(&region.amounts).unwrap(),
            "application geometry changed fitted amounts"
        );
        let whole = gpu.readback(&whole.rgb).unwrap();
        let region = gpu.readback(&region.rgb).unwrap();
        let whole: &[f32] = bytemuck::cast_slice(&whole);
        let region: &[f32] = bytemuck::cast_slice(&region);
        for y in 4..28 {
            for x in 4..28 {
                let w = ((y + 16) * 64 + x + 16) * 3;
                let r = (y * 32 + x) * 3;
                assert_eq!(
                    &whole[w..w + 3],
                    &region[r..r + 3],
                    "overlap shifted at {x},{y}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires GPU"]
fn mapped_application_guide_clamps_probes_to_photo_cells_inside_padding() {
    let gpu = Gpu::test_executor();
    let values: Vec<f32> = (0..32)
        .flat_map(|y| (0..32).flat_map(move |x| [4095. * (-13. + (x + y) as f32 / 5.).exp2(); 3]))
        .collect();
    let input = gpu.upload("Native edge RGB", &values).unwrap();
    let analysis_values: Vec<f32> = (0..16)
        .flat_map(|y| (0..16).flat_map(move |x| [4095. * (-13. + (x + y) as f32 / 2.5).exp2(); 3]))
        .collect();
    let analysis_rgb = gpu.upload("Padded analysis RGB", &analysis_values).unwrap();
    let curve: Vec<_> = (0..65536).map(|v| (v as f32 / 4095.).min(1.)).collect();
    let params = Params::new(
        [32, 32],
        [0, 0, 32, 32],
        4095.,
        ColorSpace::Srgb,
        BaseResponse::Natural,
        Tuning::default(),
        &curve,
        ProcessingQuality::Speed,
    )
    .unwrap();
    let analysis = params
        .with_geometry([16, 16], [4, 4, 12, 12], 0.25)
        .unwrap();
    let params = params
        .with_application_guide(analysis, [[0.25, 0., 4., 0.], [0., 0.25, 4., 0.]], false)
        .unwrap();
    let mut outputs = Vec::new();
    for padding in [0_f32, 1_f32] {
        let mask: Vec<_> = (0..8)
            .flat_map(|y| {
                (0..8).map(move |x| {
                    if (2..6).contains(&x) && (2..6).contains(&y) {
                        0.3
                    } else {
                        padding
                    }
                })
            })
            .collect();
        let mask = gpu.upload("Guide with inactive margin", &mask).unwrap();
        let result = apply_with_analysis(
            &gpu,
            Samples {
                rgb: &input,
                mask: &mask,
            },
            Samples {
                rgb: &analysis_rgb,
                mask: &mask,
            },
            &curve,
            params,
            analysis,
            &mut Profile::default(),
        )
        .unwrap();
        outputs.push((
            gpu.readback(&result.rgb).unwrap(),
            gpu.readback(&result.amounts).unwrap(),
        ));
    }
    assert_eq!(
        outputs[0], outputs[1],
        "inactive margins affected edge application or fit"
    );
}

#[test]
fn quality_analysis_covers_valid_pixels_with_half_grid_gains() {
    let curve: Vec<_> = (0..4096).map(|v| v as f32 / 4095.).collect();
    let create = |quality| {
        Params::new(
            [321, 257],
            [3, 5, 319, 255],
            4095.,
            ColorSpace::Srgb,
            BaseResponse::Natural,
            Tuning::default(),
            &curve,
            quality,
        )
        .unwrap()
    };
    let quality = create(ProcessingQuality::Quality);
    assert_eq!(quality.shape, [321, 257, 316, 250]);
    assert_eq!(quality.sampling, [2, 2, 1, 0]);
    assert_eq!(
        quality.aligned_output([3, 5, 17, 21]).unwrap(),
        [0, 0, 24, 24]
    );
    let speed = create(ProcessingQuality::Speed);
    assert_eq!(speed.shape, [321, 257, 256, 256]);
    assert_eq!(speed.sampling, [2, 2, 0, 0]);
    let reduced = quality
        .with_geometry([81, 65], [1, 2, 79, 63], 0.25)
        .unwrap();
    assert_eq!(reduced.shape, [81, 65, 78, 61]);
    let mapped = quality
        .with_application_guide(
            reduced,
            [[0.25, 0., -0.375, 0.], [0., 0.25, -0.375, 0.]],
            false,
        )
        .unwrap();
    assert_eq!(mapped.guide.cell_step(), 2);
    assert_eq!(mapped.guide.gain_step(), 2);
}

#[test]
#[ignore = "requires GPU"]
fn dense_analysis_counts_every_valid_pixel_carries_structure_and_summarizes_complete_windows() {
    let gpu = Gpu::test_executor();
    let extent = [303_u32, 269];
    let bounds = [5_u32, 3, 298, 264];
    let valid = [bounds[2] - bounds[0], bounds[3] - bounds[1]];
    let values: Vec<f32> = (0..extent[1])
        .flat_map(|y| {
            (0..extent[0]).flat_map(move |x| {
                if x < bounds[0] || x >= bounds[2] || y < bounds[1] || y >= bounds[3] {
                    [0.; 3]
                } else {
                    let ev =
                        -13. + (x - bounds[0]) as f32 * 0.125 + (y - bounds[1]) as f32 * 0.0625;
                    [4095. * ev.exp2(); 3]
                }
            })
        })
        .collect();
    let rgb = gpu
        .upload("Dense analytical log-light ramp", &values)
        .unwrap();
    let mask = gpu
        .upload(
            "Constant half-grid analysis guide",
            &vec![0.5_f32; (extent[0].div_ceil(2) * extent[1].div_ceil(2)) as usize],
        )
        .unwrap();
    let curve: Vec<_> = (0..65536).map(|v| (v as f32 / 4095.).min(1.)).collect();
    let params = Params::new(
        extent,
        bounds,
        4095.,
        ColorSpace::Srgb,
        BaseResponse::Natural,
        Tuning::default(),
        &curve,
        ProcessingQuality::Quality,
    )
    .unwrap();
    let mut initial = vec![0_u32; 4096 * 3];
    // Seed the low words just below a carry. This exercises the dense-safe
    // accumulator without allocating a multi-million-pixel overflow fixture.
    initial[4096..8192].fill(u32::MAX - 1023);
    let mut histogram = gpu
        .upload("Seeded wide structure accumulator", &initial)
        .unwrap();
    let mut samples = gpu
        .storage(
            "All valid working samples",
            u64::from(valid[0]) * u64::from(valid[1]) * 16,
        )
        .unwrap();
    let mut regions = gpu
        .storage("Dense normalized regional summaries", 1235 * 64)
        .unwrap();
    let context = gpu.uniform(&params);
    let curve = gpu.upload("Dense analysis curve", &curve).unwrap();
    let groups = [valid[0].div_ceil(8), valid[1].div_ceil(8)];
    let mut trace = gpu.trace(false, 3);
    gpu.dispatch(
        Kernel::AdaptiveToneHistogram,
        &mut [
            (0, Read(&rgb)),
            (1, Read(&mask)),
            (3, Read(&context)),
            (4, Write(histogram.write())),
        ],
        groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::AdaptiveToneSampleRegions,
        &mut [
            (0, Read(&rgb)),
            (1, Read(&mask)),
            (3, Read(&context)),
            (8, Write(samples.write())),
        ],
        groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::AdaptiveToneRegionSummary,
        &mut [
            (2, Read(&curve)),
            (3, Read(&context)),
            (8, Read(&samples)),
            (9, Write(regions.write())),
        ],
        [1235_u32.div_ceil(64), 1],
        &mut trace,
    );
    let bytes = gpu.readback(&histogram).unwrap();
    let histogram: &[u32] = bytemuck::cast_slice(&bytes);
    let count = valid[0] * valid[1];
    assert!(count > 256 * 256);
    assert_eq!(histogram[..4096].iter().sum::<u32>(), count);
    let sum: u64 = (0..4096)
        .map(|i| {
            ((u64::from(histogram[8192 + i]) << 32) + u64::from(histogram[4096 + i]))
                - u64::from(initial[4096 + i])
        })
        .sum();
    // This ramp's fine and broad gradients agree strongly even at clamped
    // edges, so every valid pixel contributes exactly the full 1024 units.
    assert_eq!(sum, u64::from(count) * 1024);
    assert!(histogram[8192..].iter().any(|v| *v > 0));
    let bytes = gpu.readback(&samples).unwrap();
    let samples: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
    assert_eq!(samples.len(), count as usize);
    for y in 0..valid[1] {
        for x in 0..valid[0] {
            let sample = samples[(y * valid[0] + x) as usize];
            let expected = -13. + x as f32 * 0.125 + y as f32 * 0.0625;
            assert!((sample[0] - expected).abs() < 0.00002);
            assert!((sample[1] + 4.).abs() < 0.00002);
            assert_eq!(sample[3], 1.);
        }
    }
    let bytes = gpu.readback(&regions).unwrap();
    let regions: &[[f32; 16]] = bytemuck::cast_slice(&bytes);
    for (index, region) in regions.iter().enumerate() {
        let (grid, step, size, local) = if index < 961 {
            (31, 8, 16, index)
        } else if index < 1186 {
            (15, 16, 32, index - 961)
        } else {
            (7, 32, 64, index - 1186)
        };
        let start = [(local % grid * step) as u32, (local / grid * step) as u32];
        let low = [start[0] * valid[0] / 256, start[1] * valid[1] / 256];
        let high = [
            ((start[0] + size) * valid[0]).div_ceil(256),
            ((start[1] + size) * valid[1]).div_ceil(256),
        ];
        let width = f64::from(high[0] - low[0]);
        let height = f64::from(high[1] - low[1]);
        let expected = -13.
            + f64::from(low[0] + high[0] - 1) * 0.0625
            + f64::from(low[1] + high[1] - 1) * 0.03125;
        let deviation = (((width * width - 1.) * 0.125_f64.powi(2)
            + (height * height - 1.) * 0.0625_f64.powi(2))
            / 12.)
            .sqrt();
        let support = width * height * 65536. / f64::from(count);
        assert!(
            (f64::from(region[0]) - expected).abs() < 0.0001,
            "region {index} mean"
        );
        assert!(
            (f64::from(region[3]) - deviation).abs() < 0.005,
            "region {index} deviation: {} vs {deviation}",
            region[3]
        );
        assert!(
            (f64::from(region[14]) - support).abs() < 0.001,
            "region {index} support"
        );
    }
}

#[test]
#[ignore = "requires GPU"]
fn dense_large_windows_preserve_constant_and_weak_population_variance() {
    let gpu = Gpu::test_executor();
    let curve: Vec<_> = (0..65536).map(|v| (v as f32 / 4095.).min(1.)).collect();
    let curve_buffer = gpu.upload("Large-window analytical curve", &curve).unwrap();
    for (extent, textured) in [
        ([2048_u32, 2048], false),
        ([2048, 2048], true),
        ([1, 1], false),
    ] {
        let params = Params::new(
            extent,
            [0, 0, extent[0], extent[1]],
            4095.,
            ColorSpace::Srgb,
            BaseResponse::Natural,
            Tuning::default(),
            &curve,
            ProcessingQuality::Quality,
        )
        .unwrap();
        let context = gpu.uniform(&params);
        let cycle: Vec<[f32; 4]> = (0..16)
            .map(|x| {
                let ev = if !textured {
                    -5.3
                } else {
                    let center = if x < 8 { -8.3 } else { -0.3 };
                    center + ((x % 8) as f32 - 3.5) * 0.001
                };
                [ev, -6., 0., 0.5]
            })
            .collect();
        let samples: Vec<_> = (0..extent[0] * extent[1])
            .map(|i| cycle[(i % 16) as usize])
            .collect();
        let samples = gpu
            .upload("Large constant or weakly textured populations", &samples)
            .unwrap();
        let mut regions = gpu
            .storage("Large-window stable summaries", 1235 * 64)
            .unwrap();
        let mut trace = gpu.trace(false, 1);
        gpu.dispatch(
            Kernel::AdaptiveToneRegionSummary,
            &mut [
                (2, Read(&curve_buffer)),
                (3, Read(&context)),
                (8, Read(&samples)),
                (9, Write(regions.write())),
            ],
            [1235_u32.div_ceil(64), 1],
            &mut trace,
        );
        let bytes = gpu.readback(&regions).unwrap();
        let regions: &[[f32; 16]] = bytemuck::cast_slice(&bytes);
        let moments = |values: &[[f32; 4]]| {
            let mean = values.iter().map(|v| f64::from(v[0])).sum::<f64>() / values.len() as f64;
            let variance = values
                .iter()
                .map(|v| (f64::from(v[0]) - mean).powi(2))
                .sum::<f64>()
                / values.len() as f64;
            (mean, variance.sqrt())
        };
        for (index, region) in regions.iter().enumerate() {
            let (low, high) = dense_summary_window(extent, index);
            let selected: Vec<_> = dense_summary_axis(low[0], high[0])
                .into_iter()
                .map(|x| cycle[(x % 16) as usize])
                .collect();
            let all = moments(&selected);
            let dark_values: Vec<_> = selected.iter().copied().filter(|v| v[0] < -4.).collect();
            let bright_values: Vec<_> = selected.iter().copied().filter(|v| v[0] > -4.).collect();
            let support = f64::from(high[0] - low[0]) * f64::from(high[1] - low[1]) * 65536.
                / (f64::from(extent[0]) * f64::from(extent[1]));
            assert!((f64::from(region[14]) - support).abs() < 0.001);
            assert!(
                (f64::from(region[0]) - all.0).abs() < 0.000002,
                "{extent:?} textured={textured} window {index} all mean {} vs {} (region {region:?})",
                region[0],
                all.0
            );
            assert!(
                (f64::from(region[3]) - all.1).abs() < 0.000003,
                "window {index} all deviation {} vs {}",
                region[3],
                all.1
            );
            assert!((region[1] + 6.).abs() < 0.000002);
            assert!((region[2] - 0.5).abs() < 0.000002);
            if !textured {
                assert!(
                    (f64::from(region[8]) - all.0).abs() < 0.000002,
                    "window {index} small weighted population mean"
                );
                assert_eq!(region[11], 0.);
            }
            if textured {
                for (offset, expected) in [(4, moments(&bright_values)), (8, moments(&dark_values))]
                {
                    assert!(
                        (f64::from(region[offset]) - expected.0).abs() < 0.000002,
                        "window {index} population {offset} mean"
                    );
                    assert!(
                        (f64::from(region[offset + 3]) - expected.1).abs() < 0.000003,
                        "window {index} population {offset} deviation {} vs {}",
                        region[offset + 3],
                        expected.1
                    );
                }
                assert!((region[12] / region[14] - 0.5).abs() < 0.000002);
                assert!((region[13] / region[14] - 0.5).abs() < 0.000002);
            }
        }
    }
}

fn dense_summary_window(extent: [u32; 2], index: usize) -> ([u32; 2], [u32; 2]) {
    let (grid, step, size, local) = if index < 961 {
        (31, 8, 16, index)
    } else if index < 1186 {
        (15, 16, 32, index - 961)
    } else {
        (7, 32, 64, index - 1186)
    };
    let start = [(local % grid * step) as u32, (local / grid * step) as u32];
    (
        [start[0] * extent[0] / 256, start[1] * extent[1] / 256],
        [
            ((start[0] + size) * extent[0]).div_ceil(256),
            ((start[1] + size) * extent[1]).div_ceil(256),
        ],
    )
}

fn dense_summary_axis(low: u32, high: u32) -> Vec<u32> {
    let extent = u64::from(high - low);
    let count = extent.min(128);
    (0..count)
        .map(|index| low + ((2 * index + 1) * extent / (2 * count)) as u32)
        .collect()
}

#[test]
fn bounded_region_midpoints_are_distinct_and_inside_odd_narrow_windows() {
    for (low, high) in [
        (0, 1),
        (7, 18),
        (3, 130),
        (5, 134),
        (7, 4106),
        (0, u32::MAX),
    ] {
        let points = dense_summary_axis(low, high);
        assert_eq!(points.len(), ((high - low).min(128)) as usize);
        assert!(points.iter().all(|point| (low..high).contains(point)));
        assert!(points.windows(2).all(|pair| pair[0] < pair[1]));
        if high - low <= 128 {
            assert_eq!(points, (low..high).collect::<Vec<_>>());
        }
    }
}

#[test]
#[ignore = "requires GPU"]
fn bounded_dense_regions_keep_scheduled_support_and_thin_feature_phases() {
    let gpu = Gpu::test_executor();
    let curve: Vec<_> = (0..65536).map(|v| (v as f32 / 4095.).min(1.)).collect();
    let curve_buffer = gpu
        .upload("Bounded-summary analytical curve", &curve)
        .unwrap();
    for (extent, phase) in [
        ([1_u32, 1], 0_u32),
        ([513, 259], 0),
        ([4099, 17], 0),
        ([4099, 17], 1),
        ([4099, 17], 2),
        ([4099, 17], 3),
        ([4099, 17], 7),
        ([4099, 17], 15),
    ] {
        let params = Params::new(
            extent,
            [0, 0, extent[0], extent[1]],
            4095.,
            ColorSpace::Srgb,
            BaseResponse::Natural,
            Tuning::default(),
            &curve,
            ProcessingQuality::Quality,
        )
        .unwrap();
        let context = gpu.uniform(&params);
        let values: Vec<[f32; 4]> = (0..extent[1])
            .flat_map(|y| {
                (0..extent[0]).map(move |x| {
                    let light = if (x + 3 * y) % 11 == 0 {
                        -32.
                    } else if (x + phase) % 257 == 0 || (y + phase) % 13 == 0 {
                        -0.3
                    } else {
                        -8.3 + (x % 17) as f32 * 0.001 + (y % 11) as f32 * 0.0001
                    };
                    [light, -6., 0., 0.5]
                })
            })
            .collect();
        let samples = gpu
            .upload("Thin highlights, weak shadows and invalid cells", &values)
            .unwrap();
        let mut regions = gpu.storage("Bounded-window summaries", 1235 * 64).unwrap();
        let mut trace = gpu.trace(false, 1);
        gpu.dispatch(
            Kernel::AdaptiveToneRegionSummary,
            &mut [
                (2, Read(&curve_buffer)),
                (3, Read(&context)),
                (8, Read(&samples)),
                (9, Write(regions.write())),
            ],
            [1235_u32.div_ceil(64), 1],
            &mut trace,
        );
        let bytes = gpu.readback(&regions).unwrap();
        let regions: &[[f32; 16]] = bytemuck::cast_slice(&bytes);
        let mut missed_highlight_windows = 0;
        for (index, region) in regions.iter().enumerate() {
            assert!(region.iter().all(|value| value.is_finite()));
            let (low, high) = dense_summary_window(extent, index);
            let xs = dense_summary_axis(low[0], high[0]);
            let ys = dense_summary_axis(low[1], high[1]);
            let scheduled = xs.len() * ys.len();
            let mut selected = Vec::new();
            for &y in &ys {
                for &x in &xs {
                    let light = values[(y * extent[0] + x) as usize][0];
                    if light >= -20. {
                        selected.push(light);
                    }
                }
            }
            let area = f64::from(high[0] - low[0]) * f64::from(high[1] - low[1]);
            let support_per_point =
                area * 65536. / (scheduled as f64 * f64::from(extent[0]) * f64::from(extent[1]));
            for (offset, support_offset, population) in [
                (0, 14, selected.clone()),
                (
                    4,
                    12,
                    selected.iter().copied().filter(|v| *v > -4.).collect(),
                ),
                (
                    8,
                    13,
                    selected.iter().copied().filter(|v| *v < -4.).collect(),
                ),
            ] {
                let expected_support = population.len() as f64 * support_per_point;
                assert!(
                    (f64::from(region[support_offset]) - expected_support).abs() < 0.001,
                    "{extent:?} phase {phase} window {index} support {support_offset}"
                );
                if population.is_empty() {
                    continue;
                }
                let mean =
                    population.iter().map(|v| f64::from(*v)).sum::<f64>() / population.len() as f64;
                let deviation = (population
                    .iter()
                    .map(|v| (f64::from(*v) - mean).powi(2))
                    .sum::<f64>()
                    / population.len() as f64)
                    .sqrt();
                assert!(
                    (f64::from(region[offset]) - mean).abs() < 0.000003,
                    "{extent:?} phase {phase} window {index} mean {offset}"
                );
                // A rare first highlight can put the all-population origin
                // eight stops from most samples. Its variance subtracts large
                // moments, so bound that cancellation at Float32 precision.
                // The separate weak-population test keeps its tight bound.
                let origin = f64::from(population[0]);
                let residual_second = population
                    .iter()
                    .map(|v| (f64::from(*v) - origin).powi(2))
                    .sum::<f64>()
                    / population.len() as f64;
                let depth = usize::BITS - (population.len() - 1).leading_zeros();
                let variance_roundoff = f64::from(depth + 4)
                    * f64::from(f32::EPSILON)
                    * (residual_second + (mean - origin).powi(2));
                let tolerance = 0.000005 * (2. * deviation + 0.000005) + variance_roundoff;
                let actual = f64::from(region[offset + 3]);
                assert!(
                    (actual.powi(2) - deviation.powi(2)).abs() <= tolerance,
                    "{extent:?} phase {phase} window {index} deviation component {offset}: \
                     {actual} vs {deviation}, variance tolerance {tolerance}"
                );
            }
            if region[12] == 0.
                && (low[1]..high[1]).any(|y| {
                    (low[0]..high[0]).any(|x| values[(y * extent[0] + x) as usize][0] > -4.)
                })
            {
                missed_highlight_windows += 1;
            }
        }
        eprintln!("bounded summary {extent:?} phase {phase}: {missed_highlight_windows} windows missed thin highlights");
    }
}
