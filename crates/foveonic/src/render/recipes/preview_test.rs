//! Checks reduced-preview framing and fallback through exposure and reconstruction edits.

use crate::{
    inspection::Content,
    profile::Profile,
    render::{ControlCapabilities, Engine, PreviewQuality, RenderSession, Rendering, Settings},
    source::Source,
};

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F, optional SMALL_PREVIEW_OUT"]
fn small_overview_keeps_framing_through_exposure_and_reconstruction_edits() {
    let source = Source::open(std::env::var_os("SMALL_PREVIEW_X3F").unwrap()).unwrap();
    let native = crate::render::native_extent(&source.metadata).unwrap();
    let cap = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut settings = Settings::new(
        cap.default_white_balance(&source.metadata).unwrap(),
        cap.default_color_mode(&source.metadata).unwrap(),
    );
    settings.reconstruction = Some(crate::noise::Controls::default());
    settings.rendering = Rendering::Color;
    assert!(settings.fast_preview_available(&source.metadata));
    let mut session = RenderSession::new(source);
    let engine = Engine::new().unwrap();
    let output = std::env::var_os("SMALL_PREVIEW_OUT").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir(path).unwrap();
    }
    for quality in [
        PreviewQuality::Full,
        PreviewQuality::Fast,
        PreviewQuality::Draft,
        PreviewQuality::Thumb,
    ] {
        settings.preview_quality = quality;
        for (exposure, chroma) in [(0., 0), (0.75, 0), (0.75, 2)] {
            settings.exposure = exposure;
            settings.reconstruction.as_mut().unwrap().chroma = chroma;
            let artifact = engine
                .render_session(
                    &mut session,
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap()
                .pop()
                .unwrap();
            let Content::Image(image) = artifact.content else {
                panic!("expected image")
            };
            assert_eq!(artifact.settings.preview_quality, quality);
            assert_eq!(image.view().extent, native.map(|v| v / quality.divisor()));
            let texture = engine
                .view_texture(&image, None, &mut Profile::default())
                .unwrap();
            engine.device().poll(wgpu::Maintain::Wait);
            assert_eq!(texture.description().extent, image.view().extent);
            if exposure == 0.75 && chroma == 0 {
                if let Some(path) = &output {
                    engine
                        .write_tiff(
                            &image,
                            &path.join(format!("{quality:?}.tiff")),
                            &mut Profile::default(),
                        )
                        .unwrap();
                }
            }
        }
    }
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn small_overview_falls_back_for_resized_regional_monochrome_and_inspection_requests() {
    let source = Source::open(std::env::var_os("SMALL_PREVIEW_X3F").unwrap()).unwrap();
    let cap = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let base = Settings::new(
        cap.default_white_balance(&source.metadata).unwrap(),
        cap.default_color_mode(&source.metadata).unwrap(),
    );
    let mut session = RenderSession::new(source);
    let engine = Engine::new().unwrap();
    for case in 0..4 {
        let mut settings = base.clone();
        settings.preview_quality = PreviewQuality::Draft;
        let names = match case {
            0 => {
                settings.output_size = Some([64, 96]);
                &["final"][..]
            }
            1 => {
                settings.preview_region = Some([100, 100, 164, 196]);
                &["final"][..]
            }
            2 => {
                settings.rendering = Rendering::Monochrome;
                &["final"][..]
            }
            _ => &["linear.base"][..],
        };
        let artifacts = engine
            .render_session(
                &mut session,
                &settings,
                names,
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap();
        assert!(!artifacts.is_empty());
        assert!(
            artifacts
                .iter()
                .all(|a| a.settings.preview_quality == PreviewQuality::Full),
            "case {case}"
        );
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR originals"]
fn small_color_guides_match_fresh_edits_and_keep_public_inspection_full() {
    use crate::{nodes::finish::advanced_color, source::ProcessingFamily};
    use std::{path::PathBuf, sync::Arc};

    let directory = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let engine = Engine::new().unwrap();
    let verifier = engine.executor();
    for (name, family) in [
        ("blue-parking-sign.x3f", ProcessingFamily::Merrill),
        ("dp1s.x3f", ProcessingFamily::Classic),
        ("dp2-quattro.x3f", ProcessingFamily::Quattro),
    ] {
        let source = Arc::new(Source::open(directory.join(name)).unwrap());
        assert_eq!(source.metadata.processing_family().unwrap(), family);
        let native = crate::render::native_extent(&source.metadata).unwrap();
        let cap = ControlCapabilities::from_metadata(&source.metadata).unwrap();
        let mut base = cap
            .editing_defaults(&source.metadata, Rendering::Color)
            .unwrap();
        base.white_balance = cap.default_white_balance(&source.metadata).unwrap();
        base.color_mode = cap.default_color_mode(&source.metadata).unwrap();
        let mut session = RenderSession::new(source.clone());
        for quality in [PreviewQuality::Draft, PreviewQuality::Thumb] {
            base.preview_quality = quality;
            assert!(
                base.small_preview_available(&source.metadata, quality),
                "{name}"
            );
            let neutral = engine
                .render_session(
                    &mut session,
                    &base,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap()
                .pop()
                .unwrap();
            let Content::Image(neutral) = neutral.content else {
                panic!("expected image")
            };
            let neutral_bytes = engine.readback_image(&neutral).unwrap();
            for advanced in [false, true] {
                for saturation in [-0.3, 0.4] {
                    let mut settings = base.clone();
                    if advanced {
                        settings
                            .photographic
                            .color_editor
                            .advanced
                            .push(advanced_color::Range {
                                width: 360.,
                                adjustment: crate::nodes::finish::color_editor::Adjustment {
                                    saturation,
                                    ..Default::default()
                                },
                                ..Default::default()
                            });
                    } else {
                        settings.photographic.color_editor.master.saturation = saturation;
                    }
                    let artifact = engine
                        .render_session(
                            &mut session,
                            &settings,
                            &["final"],
                            || false,
                            &mut Profile::default(),
                        )
                        .unwrap()
                        .unwrap()
                        .pop()
                        .unwrap();
                    assert_eq!(artifact.settings.preview_quality, quality);
                    let Content::Image(image) = artifact.content else {
                        panic!("expected image")
                    };
                    assert_eq!(image.view().extent, native.map(|v| v / quality.divisor()));
                    let fresh = verifier
                        .render(&source, &settings, &mut Profile::default())
                        .unwrap();
                    assert_eq!(image.description(), fresh.description());
                    assert_eq!(image.view(), fresh.view());
                    let actual = engine.readback_image(&image).unwrap();
                    assert!(
                        actual == verifier.readback_image(&fresh).unwrap(),
                        "{name} {quality:?} advanced={advanced} saturation={saturation}"
                    );
                    assert!(
                        actual != neutral_bytes,
                        "active color edit must change {name}"
                    );
                }
            }
            let inspection = engine
                .render_session(
                    &mut session,
                    &base,
                    &["linear.base"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap()
                .pop()
                .unwrap();
            assert_eq!(inspection.settings.preview_quality, PreviewQuality::Full);
            let Content::Image(image) = inspection.content else {
                panic!("expected image")
            };
            assert!(image.description().extent[0] > neutral.description().extent[0]);
            eprintln!(
                "{name} {quality:?}: cached/fresh basic and advanced edits exact, inspection Full"
            );
        }
    }
}
