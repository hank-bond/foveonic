//! Checks original-source mono recipes, selective inspection, mode-switch cache identity, and export boundaries.

use super::*;
use crate::{
    image::{Layout, Space},
    inspection::{Artifact, Content},
    profile::Mode,
};

fn source() -> Source {
    Source::open(
        std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f"),
    )
    .unwrap()
}
fn settings() -> Settings {
    let mut s = Settings::new("Sunlight", "Vivid");
    s.rendering = Rendering::Monochrome;
    s.monochrome.mix = [0., 0., 1.];
    s.output_size = Some([80, 120]);
    s
}
fn image(artifact: Artifact) -> RenderedImage {
    let Content::Image(image) = artifact.content else {
        panic!("expected an image")
    };
    *image
}
fn session_image(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
) -> RenderedImage {
    image(
        engine
            .render_session(
                session,
                settings,
                &["final"],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap()
            .remove(0),
    )
}
fn same(a: &[u8], b: &[u8], context: &str) {
    assert_eq!(a.len(), b.len());
    let a = bytemuck::cast_slice::<u8, f32>(a);
    let b = bytemuck::cast_slice::<u8, f32>(b);
    let first = a
        .iter()
        .zip(b)
        .position(|(a, b)| a.to_bits() != b.to_bits());
    let count = a
        .iter()
        .zip(b)
        .filter(|(a, b)| a.to_bits() != b.to_bits())
        .count();
    let max = a
        .iter()
        .zip(b)
        .map(|(a, b)| (a - b).abs())
        .fold(0_f32, f32::max);
    assert!(
        first.is_none(),
        "{context}: {count} differences, max {max}, first {:?}",
        first.map(|i| (i, a[i], b[i]))
    );
}
fn neutral(bytes: &[u8]) {
    for triple in bytemuck::cast_slice::<u8, f32>(bytes).as_chunks::<3>().0 {
        assert!(triple.iter().all(|v| v.is_finite()));
        assert_eq!(triple[0], triple[1]);
        assert_eq!(triple[1], triple[2]);
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn mono_inspection_stops_before_unrelated_processing_and_keeps_original_layers() {
    let source = source();
    let engine = Engine::new().unwrap();
    let mut s = settings();
    s.color_mode = "unknown retained color mode".into();
    s.reconstruction = Some(Default::default());
    let mut p = Profile::new(Mode::Gpu);
    let mixed = image(
        engine
            .render_artifacts(&source, &s, &["mono.mix"], &mut p)
            .unwrap()
            .remove(0),
    );
    assert_eq!(mixed.description().space, Space::Monochrome);
    assert_eq!(mixed.description().layout, Layout::Scalar32Float);
    assert!(!p
        .host
        .iter()
        .any(|t| t.stage.contains("color mode") || t.stage == "prepare tone curve"));
    assert!(!p
        .gpu
        .iter()
        .any(|t| t.stage.contains("monochrome") && t.stage != "monochrome layer mixing"));
    let before = engine.readback_image(&mixed).unwrap();
    let scopes = engine
        .scopes(&mixed, None, &mut Profile::default())
        .unwrap();
    assert_eq!(scopes.channels(), 1);
    assert_eq!(
        scopes.histogram(0).iter().sum::<u32>(),
        scopes.samples[0] * scopes.samples[1]
    );
    let texture = engine
        .view_texture(&mixed, Some([64, 96]), &mut Profile::default())
        .unwrap();
    assert_eq!(texture.description().space, Space::Monochrome);
    let mut p = Profile::new(Mode::Gpu);
    let signed = image(
        engine
            .render_artifacts(&source, &s, &["detail.signal"], &mut p)
            .unwrap()
            .remove(0),
    );
    assert_eq!(signed.description().space, Space::SignedDetail);
    let bytes = engine.readback_image(&signed).unwrap();
    let values = bytemuck::cast_slice::<u8, f32>(&bytes);
    assert!(values.iter().all(|v| v.is_finite()));
    assert!(values.iter().any(|v| *v < 0.) && values.iter().any(|v| *v > 0.));
    assert!(!p.gpu.iter().any(|t| matches!(
        t.stage,
        "tone" | "color matrix" | "residual detail mixing" | "monochrome gain replication"
    )));
    assert_eq!(engine.readback_image(&mixed).unwrap(), before);
    s.monochrome.noise = 0;
    let mut p = Profile::new(Mode::Gpu);
    let final_image = engine.render(&source, &s, &mut p).unwrap();
    neutral(&engine.readback_image(&final_image).unwrap());
    assert!(!p
        .gpu
        .iter()
        .any(|t| t.stage == "monochrome histogram filtering"));
    assert!(p
        .gpu
        .iter()
        .any(|t| t.stage == "monochrome column correction"));
    assert_eq!(engine.readback_image(&mixed).unwrap(), before);
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn mono_mix_noise_sharpen_and_mode_switches_match_fresh_and_retain_snapshots() {
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source());
    let mut s = settings();
    s.rendering = Rendering::Color;
    s.reconstruction = Some(Default::default());
    let color = session_image(&engine, &mut session, &s);
    let color_bytes = engine.readback_image(&color).unwrap();
    let color_controls = s.clone();
    for (mix, noise, banding, sharpen) in [
        ([0., 0., 1.], 2, 2, 0.),
        ([1., 0., 0.], 0, 0, 0.75),
        ([0., 1., 0.], 4, 4, 0.5),
        ([1. / 3.; 3], 2, 2, 0.),
    ] {
        s.rendering = Rendering::Monochrome;
        s.monochrome = MonochromeControls {
            mix,
            noise,
            banding,
            ..Default::default()
        };
        s.sharpen_amount = Some(sharpen);
        let cached = session_image(&engine, &mut session, &s);
        let cached_bytes = engine.readback_image(&cached).unwrap();
        neutral(&cached_bytes);
        let fresh = engine
            .render(session.source(), &s, &mut Profile::default())
            .unwrap();
        same(
            &engine.readback_image(&fresh).unwrap(),
            &cached_bytes,
            "fresh vs cached mono",
        );
        assert_eq!(
            engine.readback_image(&color).unwrap(),
            color_bytes,
            "mode switch overwrote color snapshot"
        );
        let mut inactive = s.clone();
        inactive.color_mode = "unknown but inactive".into();
        inactive.saturation = 0.1;
        inactive.reconstruction = None;
        inactive.color_dq_gain = Some(1.9);
        // The cache retains continuation boundaries, not every final image.
        // An inactive edit must cost no more than an identical-settings request.
        let before = engine.gpu_activity();
        let repeated = session_image(&engine, &mut session, &s);
        let unchanged_dispatches = engine.gpu_activity().since(before).dispatches;
        drop(repeated);
        let before = engine.gpu_activity();
        let changed = session_image(&engine, &mut session, &inactive);
        assert_eq!(
            engine.gpu_activity().since(before).dispatches,
            unchanged_dispatches,
            "inactive color settings invalidated additional mono work"
        );
        assert_eq!(engine.readback_image(&changed).unwrap(), cached_bytes);
        assert_eq!(
            engine.readback_image(&cached).unwrap(),
            cached_bytes,
            "later request overwrote retained mono"
        );
        let restored = session_image(&engine, &mut session, &color_controls);
        assert_eq!(engine.readback_image(&restored).unwrap(), color_bytes);
    }
}

/// Independent classic-TIFF tag extraction, without using the DNG writer's helpers.
fn tag(bytes: &[u8], number: u16) -> (u16, u32, Vec<u8>) {
    let u16_at = |p| u16::from_le_bytes(bytes[p..p + 2].try_into().unwrap());
    let u32_at = |p| u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
    assert_eq!(&bytes[..4], b"II\x2a\0");
    let root = u32_at(4) as usize;
    for i in 0..usize::from(u16_at(root)) {
        let p = root + 2 + i * 12;
        if u16_at(p) != number {
            continue;
        }
        let kind = u16_at(p + 2);
        let count = u32_at(p + 4);
        let unit = match kind {
            1 | 2 => 1,
            3 => 2,
            4 | 11 => 4,
            5 | 10 => 8,
            _ => panic!("unsupported TIFF kind"),
        };
        let n = count as usize * unit;
        let start = if n <= 4 {
            p + 8
        } else {
            u32_at(p + 8) as usize
        };
        return (kind, count, bytes[start..start + n].to_vec());
    }
    panic!("missing TIFF tag {number}")
}
fn dng_samples(bytes: &[u8]) -> Vec<u16> {
    let offsets = tag(bytes, 273).2;
    let counts = tag(bytes, 279).2;
    offsets
        .as_chunks::<4>()
        .0
        .iter()
        .zip(counts.as_chunks::<4>().0)
        .flat_map(|(o, c)| {
            let o = u32::from_le_bytes(*o) as usize;
            let c = u32::from_le_bytes(*c) as usize;
            bytes[o..o + c]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|v| u16::from_le_bytes(*v))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn mono_originals_keep_finite_neutral_samples_and_valid_metadata() {
    let engine = Engine::new().unwrap();
    let dir = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    for name in [
        "blue-parking-sign",
        "sunlit-red-flower",
        "paper-and-recycling-can",
        "rocks-and-tree-shadows",
        "glossy-leaves-and-buds",
        "yellow-flowers-and-stroller",
        "turquoise-still-life",
    ] {
        let source = Source::open(dir.join(format!("{name}.x3f"))).unwrap();
        let image = engine
            .render(&source, &settings(), &mut Profile::default())
            .unwrap();
        let bytes = engine.readback_image(&image).unwrap();
        neutral(&bytes);
        assert!(
            bytemuck::cast_slice::<u8, f32>(&bytes)
                .iter()
                .any(|v| *v > 0.),
            "black monochrome: {name}"
        );
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn mono_tiff_and_dng_keep_neutral_samples_and_defer_only_presentation() {
    let engine = Engine::new().unwrap();
    let source = source();
    let mut s = settings();
    s.sharpen_amount = Some(0.75);
    let base = crate::dng::prepare_base(&engine, &source, &s, &mut Profile::default()).unwrap();
    let original = engine.readback_image(&base).unwrap();
    neutral(&original);
    let mut appearance = s.clone();
    appearance.exposure = 0.75;
    appearance.tone.shadows = 0.5;
    appearance.tone.highlights = -0.25;
    appearance.tone.contrast = 0.2;
    appearance.color_mode = "inactive unknown mode".into();
    appearance.saturation = 1.9;
    let changed =
        crate::dng::prepare_base(&engine, &source, &appearance, &mut Profile::default()).unwrap();
    for stage in [
        "mono.mix",
        "mono.directional",
        "mono.column",
        "mono.filtered",
        "mono.despeckle",
        "detail.signal",
        "monochrome.rgb",
        "sharpen.rgb",
    ] {
        let a = image(
            engine
                .render_artifacts(&source, &s, &[stage], &mut Profile::default())
                .unwrap()
                .remove(0),
        );
        let b = image(
            engine
                .render_artifacts(&source, &appearance, &[stage], &mut Profile::default())
                .unwrap()
                .remove(0),
        );
        same(
            &engine.readback_image(&a).unwrap(),
            &engine.readback_image(&b).unwrap(),
            stage,
        );
        eprintln!("repeatable stage {stage}");
    }
    same(
        &engine.readback_image(&changed).unwrap(),
        &original,
        "DNG presentation base",
    );
    drop(changed);
    let root = std::env::temp_dir().join(format!("foveonic-mono-export-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let tiff = root.join("mono.tiff");
    let rendered = engine.render(&source, &s, &mut Profile::default()).unwrap();
    engine
        .write_tiff(&rendered, &tiff, &mut Profile::default())
        .unwrap();
    let mut decoder = tiff::decoder::Decoder::new(std::fs::File::open(&tiff).unwrap()).unwrap();
    assert_eq!(decoder.dimensions().unwrap(), (80, 120));
    let tiff::decoder::DecodingResult::U16(values) = decoder.read_image().unwrap() else {
        panic!("expected RGB16")
    };
    for t in values.as_chunks::<3>().0 {
        assert_eq!(t[0], t[1]);
        assert_eq!(t[1], t[2]);
    }
    let mut samples = None;
    let mut curve = None;
    for (i, settings) in [&s, &appearance].into_iter().enumerate() {
        let path = root.join(format!("mono-{i}.dng"));
        crate::dng::write_source(&engine, &source, settings, &path, &mut Profile::default())
            .unwrap();
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(tag(&bytes, 50936).2, b"Foveonic Monochrome\0");
        let neutral = tag(&bytes, 50728);
        assert_eq!((neutral.0, neutral.1), (5, 3));
        for r in neutral.2.as_chunks::<8>().0 {
            assert_eq!(&r[..4], &r[4..]);
        }
        let current = dng_samples(&bytes);
        assert_eq!(current.len(), 80 * 120 * 3);
        for t in current.as_chunks::<3>().0 {
            assert_eq!(t[0], t[1]);
            assert_eq!(t[1], t[2]);
        }
        if let Some(previous) = &samples {
            assert_eq!(previous, &current);
        } else {
            samples = Some(current);
        }
        let current = tag(&bytes, 50940).2;
        if let Some(previous) = &curve {
            assert_ne!(previous, &current);
        } else {
            curve = Some(current);
        }
        let history = tag(&bytes, 50740).2;
        let history: serde_json::Value = serde_json::from_slice(&history[9..]).unwrap();
        assert_eq!(history["settings"]["rendering"], "monochrome");
        assert!(history["baked_controls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "mono_mix"));
        assert_eq!(
            history["metadata_controls"],
            serde_json::json!(["exposure", "tone"])
        );
    }
    s.monochrome.mix = [1., 0., 0.];
    let changed = crate::dng::prepare_base(&engine, &source, &s, &mut Profile::default()).unwrap();
    assert_ne!(engine.readback_image(&changed).unwrap(), original);
    assert_eq!(engine.readback_image(&base).unwrap(), original);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE pointing to a Merrill X3F"]
fn mono_sensitivity_and_detail_preserve_mix_cache_and_dng_boundaries() {
    let source = std::sync::Arc::new(
        Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap(),
    );
    let engine = Engine::new().unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut s = caps
        .editing_defaults(&source.metadata, Rendering::Monochrome)
        .unwrap();
    s.white_balance = caps.default_white_balance(&source.metadata).unwrap();
    s.monochrome.mix = [0., 0., 1.];
    let mut session = RenderSession::new(source.clone());
    let mut last = None;
    for (sensitivity, fine, mix) in [
        (0., 1., [0., 0., 1.]),
        (0.3, 1., [0., 0., 1.]),
        (0.5, 1.2, [0., 0., 1.]),
        (0.5, 1.2, [0.5, 0., 0.5]),
    ] {
        s.photographic.color_editor.bands[0].lightness = sensitivity;
        s.monochrome.texture.fine = fine;
        s.monochrome.mix = mix;
        let cached = session_image(&engine, &mut session, &s);
        let cached_bytes = engine.readback_image(&cached).unwrap();
        neutral(&cached_bytes);
        drop(cached);
        let fresh = engine.render(&source, &s, &mut Profile::default()).unwrap();
        same(
            &cached_bytes,
            &engine.readback_image(&fresh).unwrap(),
            "mono sensitivity/detail cache",
        );
        if let Some(previous) = last {
            assert!(previous != cached_bytes);
        }
        last = Some(cached_bytes);
    }
    let outputs = [
        "color-editor.guide",
        "color-editor.rgb",
        "color-editor.input",
        "mono.texture",
        "mono.despeckle",
    ];
    let mut artifacts = engine
        .render_artifacts(&source, &s, &outputs, &mut Profile::default())
        .unwrap();
    for artifact in artifacts.drain(..) {
        let name = artifact.name;
        let img = image(artifact);
        if name == "mono.texture" || name == "mono.despeckle" {
            assert_eq!(img.description().layout, Layout::Scalar32Float);
        } else {
            assert_eq!(img.description().layout, Layout::Rgb32Float);
        }
        assert!(
            bytemuck::cast_slice::<u8, f32>(&engine.readback_image(&img).unwrap())
                .iter()
                .all(|v| v.is_finite())
        );
    }
    let dng = engine
        .render_dng_base(&source, &s, &mut Profile::default())
        .unwrap();
    let dng_bytes = engine.readback_image(&dng).unwrap();
    drop(dng);
    s.photographic.color_editor = Default::default();
    let plain = engine
        .render_dng_base(&source, &s, &mut Profile::default())
        .unwrap();
    let neutral_sensitivity_bytes = engine.readback_image(&plain).unwrap();
    assert!(
        dng_bytes != neutral_sensitivity_bytes,
        "DNG bakes sensitivity"
    );
    drop(plain);
    s.monochrome.detail_enabled = false;
    let plain = engine
        .render_dng_base(&source, &s, &mut Profile::default())
        .unwrap();
    assert!(
        neutral_sensitivity_bytes != engine.readback_image(&plain).unwrap(),
        "DNG retains technical detail"
    );
}
