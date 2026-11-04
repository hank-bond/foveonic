//! Checks profile tuning, exposure-aware graph reuse, numerical safety and photographic export behavior.

use super::super::base_response::Limits;
use super::*;
use crate::{
    inspection::Content,
    profile::Mode,
    render::{
        ColorFieldApproach, ControlCapabilities, Engine, HighlightMode, RenderSession, Rendering,
        Settings,
    },
    source::Source,
};
use std::sync::Arc;

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
    let mut session = RenderSession::new(source.clone());
    let mut artifacts = engine
        .render_session(
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
                .render_session(
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
            assert_eq!(
                settings.fast_preview_available(&source.metadata),
                rendering == Rendering::Color
            );
            assert!(!settings.regional_preview_available(&source.metadata));
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
    settings.output_size = Some([320, 480]);
    settings.photographic.tone_method = crate::render::PhotographicMethod::Universal;
    let engine = Engine::new().unwrap();
    let camera = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    let camera = engine.readback_image(&camera).unwrap();
    settings.photographic.base_response = BaseResponse::Natural;
    let mut session = RenderSession::new(source.clone());
    engine
        .render_session(
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
        .render_session(&mut session, &settings, &["final"], || false, &mut profile)
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
        .render_session(&mut session, &settings, &["final"], || false, &mut profile)
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
    let gpu = Gpu::new().unwrap();
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
    let guide = tone_equalizer::mask(
        &gpu,
        &input,
        tone_equalizer::Guide::new(extent, 24, ColorSpace::Srgb).unwrap(),
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
    let guide = tone_equalizer::mask(
        &gpu,
        &uniform,
        tone_equalizer::Guide::new(extent, 24, ColorSpace::Srgb).unwrap(),
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
    let gpu = Gpu::new().unwrap();
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
    let mask = tone_equalizer::mask(
        &gpu,
        &input,
        tone_equalizer::Guide::new(extent, 24, ColorSpace::Srgb).unwrap(),
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
