//! Checks Tiger's complete recipe, channel masks, supported looks and immutable session results.

use super::*;
use crate::{inspection::Content, source::Source};
use std::hash::{Hash, Hasher};

fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hash);
    hash.finish()
}

#[test]
#[ignore = "requires GPU and TIGER_X3F original"]
fn original_tiger_modes_masks_and_retained_results() {
    let source = Source::open(std::env::var_os("TIGER_X3F").unwrap()).unwrap();
    let capabilities = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    assert!(capabilities.editing_available(Some(&source.metadata), None));
    assert!(recipes::quattro::fast_available(&source.metadata));
    let modes: Vec<String> = capabilities
        .color_mode_presets(&source.metadata)
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(modes.len(), 10);
    let source_hash = fingerprint(bytemuck::cast_slice(&source.pixels));
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source);
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.output_size = Some([600, 400]);
    let results = engine
        .render_session(
            &mut session,
            &settings,
            &["final", "highlight-restoration.recovery-mask"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    let Content::Image(retained) = &results[0].content else {
        panic!("expected photograph")
    };
    assert_eq!(retained.view().extent, [600, 400]);
    let original = engine.readback_image(retained).unwrap();
    let Content::Image(mask) = &results[1].content else {
        panic!("expected channel mask")
    };
    assert_eq!(mask.description().extent, [3328, 2240]);
    assert_eq!(mask.view().crop, [0, 0, 3328, 2240]);
    assert_eq!(mask.view().extent, [3328, 2240]);
    let mask_bytes = engine.readback_image(mask).unwrap();
    assert!(bytemuck::cast_slice::<u8, f32>(&mask_bytes)
        .iter()
        .all(|&value| value == 0. || value == 1.));
    let texture = engine
        .view_texture(mask, Some([208, 140]), &mut Profile::default())
        .unwrap();
    assert_eq!(texture.description().space, crate::image::Space::SensorMask);
    assert_eq!(texture.description().extent, [208, 140]);
    for mode in &modes {
        settings.color_mode = mode.to_string();
        let image = engine
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
        let Content::Image(image) = image.content else {
            panic!("expected photograph")
        };
        let bytes = engine.readback_image(&image).unwrap();
        assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(v)));
        if mode == "Standard" {
            assert_eq!(bytes, original);
        }
    }
    assert_eq!(engine.readback_image(retained).unwrap(), original);
    assert_eq!(engine.readback_image(mask).unwrap(), mask_bytes);
    assert_eq!(
        fingerprint(bytemuck::cast_slice(&session.source().pixels)),
        source_hash
    );
}

fn session_image(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
    profile: &mut Profile,
) -> RenderedImage {
    let artifact = engine
        .render_session(session, settings, &["final"], || false, profile)
        .unwrap()
        .unwrap()
        .pop()
        .unwrap();
    let Content::Image(image) = artifact.content else {
        panic!("expected photograph")
    };
    *image
}

#[test]
#[ignore = "requires GPU and TIGER_X3F original"]
fn tiger_fast_edits_and_regional_full_preserve_geometry_samples_and_reuse() {
    let engine = Engine::new().unwrap();
    for rotation in [0, 90] {
        let mut source = Source::open(std::env::var_os("TIGER_X3F").unwrap()).unwrap();
        Arc::make_mut(&mut source.metadata).rotation = rotation;
        let source_hash = fingerprint(bytemuck::cast_slice(source.samples()));
        let mut session = RenderSession::new(source);
        let mut settings = Settings::new(
            "Sunlight",
            if rotation == 0 {
                "Standard"
            } else {
                "Portrait"
            },
        );
        settings.highlight_amount = Some(0.4);
        let full = engine
            .render(session.source(), &settings, &mut Profile::default())
            .unwrap();
        let [width, height] = full.view().extent;
        let expected = engine.readback_image(&full).unwrap();
        drop(full);

        settings.preview_quality = PreviewQuality::Fast;
        let started = Instant::now();
        let fast = session_image(&engine, &mut session, &settings, &mut Profile::default());
        let fast_bytes = engine.readback_image(&fast).unwrap();
        eprintln!(
            "Tiger {rotation}° first Fast including readback: {:?}",
            started.elapsed()
        );
        assert_eq!(fast.view().extent, [width / 2, height / 2]);
        assert!(bytemuck::cast_slice::<u8, f32>(&fast_bytes)
            .iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(v)));
        let fresh = engine
            .render(session.source(), &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(engine.readback_image(&fresh).unwrap(), fast_bytes);
        drop(fresh);
        let fast_hash = fingerprint(&fast_bytes);
        drop(fast_bytes);
        let mut edited = settings.clone();
        edited.exposure = 0.3;
        edited.reconstruction = Some(crate::noise::Controls {
            chroma: 1,
            luminance: 4,
            detail: 0.25,
            ..recipes::quattro::default_noise_controls()
        });
        let cached = session_image(&engine, &mut session, &edited, &mut Profile::default());
        let fresh = engine
            .render(session.source(), &edited, &mut Profile::default())
            .unwrap();
        assert_eq!(
            engine.readback_image(&cached).unwrap(),
            engine.readback_image(&fresh).unwrap()
        );
        assert_ne!(
            fingerprint(&engine.readback_image(&cached).unwrap()),
            fast_hash
        );
        drop((cached, fresh));

        let restored = session_image(&engine, &mut session, &settings, &mut Profile::default());
        assert_eq!(
            fingerprint(&engine.readback_image(&restored).unwrap()),
            fast_hash
        );
        drop(restored);

        settings.preview_quality = PreviewQuality::Full;
        for [x, y] in [[0, 0], [width / 2, height / 2], [width - 480, height - 320]] {
            settings.preview_region = Some([x, y, x + 480, y + 320]);
            let started = Instant::now();
            let mut profile = Profile::new(crate::profile::Mode::Wall);
            let tile = session_image(&engine, &mut session, &settings, &mut profile);
            for stage in [
                "restore directional highlights",
                "filter RGB at two scales",
                "apply adaptive raw color",
            ] {
                assert!(
                    !profile.host.iter().any(|timing| timing.stage == stage),
                    "detail repeated {stage}"
                );
            }
            let bytes = engine.readback_image(&tile).unwrap();
            assert_eq!(tile.view().extent, [480, 320]);
            for (row, actual) in bytes.as_chunks::<{ 480 * 12 }>().0.iter().enumerate() {
                let start = (((y as usize + row) * width as usize + x as usize) * 3) * 4;
                assert_eq!(
                    actual,
                    &expected[start..start + 480 * 12],
                    "Tiger regional Full differs at {x},{y}, rotation {rotation}"
                );
            }
            eprintln!(
                "Tiger {rotation}° Full tile {x},{y} including readback: {:?}",
                started.elapsed()
            );
        }
        settings.preview_region = None;
        let before = engine.gpu_activity();
        let mut profile = Profile::new(crate::profile::Mode::Wall);
        let completed = session_image(&engine, &mut session, &settings, &mut profile);
        let activity = engine.gpu_activity().since(before);
        assert!(
            activity.upload_bytes < session.source().samples().len() as u64 * 4,
            "continuation uploaded original again"
        );
        for stage in [
            "extract sensor phases",
            "restore directional highlights",
            "filter RGB at two scales",
            "apply adaptive raw color",
        ] {
            assert!(
                !profile.host.iter().any(|timing| timing.stage == stage),
                "continuation repeated {stage}"
            );
        }
        assert_eq!(engine.readback_image(&completed).unwrap(), expected);
        assert_eq!(
            fingerprint(&engine.readback_image(&fast).unwrap()),
            fast_hash
        );
        assert_eq!(
            fingerprint(bytemuck::cast_slice(session.source().samples())),
            source_hash
        );
    }
}
