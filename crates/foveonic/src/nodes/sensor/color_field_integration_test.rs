//! Checks color-field cache dependencies, retained images, and the DNG technical route on a saved edit.
use super::*;
use crate::{
    image::RenderedImage,
    inspection::Content,
    render::{Engine, RenderSession, Settings},
    source::Source,
};

fn session_layers(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
    profile: &mut Profile,
) -> RenderedImage {
    let mut result = engine
        .render_session(session, settings, &["highlight.layers"], || false, profile)
        .unwrap()
        .unwrap();
    match result.pop().unwrap().content {
        Content::Image(image) => *image,
        _ => panic!("image expected"),
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_HIGHLIGHT_DIR"]
fn gpu_color_field_cutoff_updates_cached_layers_and_output_without_mutating_retained_images() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("FOVEONIC_HIGHLIGHT_DIR").expect("set FOVEONIC_HIGHLIGHT_DIR"),
    );
    let path = directory.join("_P2M0095.X3F");
    let original_sidecar = std::fs::read(crate::sidecar::path(&path)).unwrap();
    let mut settings = crate::sidecar::decode(&original_sidecar).unwrap();
    settings.highlight_mode = HighlightMode::ColorField;
    let engine = Engine::new().unwrap();
    let images = |artifacts: Vec<crate::inspection::Artifact>| -> Vec<RenderedImage> {
        artifacts
            .into_iter()
            .map(|a| match a.content {
                Content::Image(image) => *image,
                _ => panic!("image expected"),
            })
            .collect()
    };
    let maximum = |a: &[u8], b: &[u8]| -> f32 {
        assert_eq!(a.len(), b.len());
        bytemuck::cast_slice::<u8, f32>(a)
            .iter()
            .zip(bytemuck::cast_slice::<u8, f32>(b))
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max)
    };
    let requested = ["highlight.layers", "final"];
    for approach in [Approach::Faithful, Approach::LocalGradient] {
        settings.color_field = Controls {
            approach,
            ..Controls::default()
        };
        let mut session = RenderSession::new(Source::open(&path).unwrap());
        let retained = images(
            engine
                .render_session(
                    &mut session,
                    &settings,
                    &requested,
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap(),
        );
        let baseline: Vec<_> = retained
            .iter()
            .map(|image| engine.readback_image(image).unwrap())
            .collect();
        for selectivity in [0.25, 0.5, 1., 0.] {
            settings.color_field.selectivity = selectivity;
            let mut profile = Profile::new(crate::profile::Mode::Wall);
            let cached = images(
                engine
                    .render_session(&mut session, &settings, &requested, || false, &mut profile)
                    .unwrap()
                    .unwrap(),
            );
            assert!(
                !profile.host.iter().any(|t| matches!(
                    t.stage,
                    "upload sensor" | "submit calibration" | "submit repair"
                )),
                "cutoff must reuse unchanged sensor inputs"
            );
            let fresh = images(
                engine
                    .render_artifacts(
                        session.source(),
                        &settings,
                        &requested,
                        &mut Profile::default(),
                    )
                    .unwrap(),
            );
            for (i, bound) in [0.5, 2e-4].into_iter().enumerate() {
                let a = engine.readback_image(&cached[i]).unwrap();
                let b = engine.readback_image(&fresh[i]).unwrap();
                let delta = maximum(&a, &b);
                eprintln!(
                    "selectivity {approach:?} {selectivity:?} output {i}: fresh/cached {delta}"
                );
                assert!(delta <= bound, "fresh/cached divergence: {delta}");
                assert!(
                    a == b,
                    "stable row ordering must also make cached/fresh samples exact"
                );
                if selectivity > 0. {
                    assert!(
                        maximum(&a, &baseline[i]) > bound,
                        "cutoff must affect both layer and Output views"
                    );
                } else {
                    assert!(
                        maximum(&a, &baseline[i]) <= bound,
                        "Zero must restore the original baseline"
                    );
                }
                assert_eq!(
                    engine.readback_image(&retained[i]).unwrap(),
                    baseline[i],
                    "retained image changed"
                );
            }
        }
        // Classic and the independent enable switch do not consume the amount.
        for (mode, enabled) in [
            (HighlightMode::Classic, true),
            (HighlightMode::ColorField, false),
        ] {
            settings.highlight_mode = mode;
            settings.highlight_restoration = enabled;
            // Disabled correction has no highlight.layers artifact.
            let requested = ["final"];
            settings.color_field.selectivity = 0.;
            let before = images(
                engine
                    .render_session(
                        &mut session,
                        &settings,
                        &requested,
                        || false,
                        &mut Profile::default(),
                    )
                    .unwrap()
                    .unwrap(),
            );
            settings.color_field.selectivity = 1.;
            let after = images(
                engine
                    .render_session(
                        &mut session,
                        &settings,
                        &requested,
                        || false,
                        &mut Profile::default(),
                    )
                    .unwrap()
                    .unwrap(),
            );
            for (a, b) in before.iter().zip(&after) {
                assert_eq!(
                    engine.readback_image(a).unwrap(),
                    engine.readback_image(b).unwrap()
                );
            }
        }
        settings.highlight_mode = HighlightMode::ColorField;
        settings.highlight_restoration = true;
    }
    assert_eq!(
        std::fs::read(crate::sidecar::path(&path)).unwrap(),
        original_sidecar
    );
}

#[test]
#[ignore = "requires GPU and the full FOVEONIC_HIGHLIGHT_DIR corpus"]
fn gpu_color_field_corpus_is_exact_between_fresh_and_cached_solves() {
    let directory = std::path::PathBuf::from(std::env::var_os("FOVEONIC_HIGHLIGHT_DIR").unwrap());
    let engine = Engine::new().unwrap();
    for scene in [
        "0009", "0010", "0035", "0077", "0089", "0095", "0170", "0195", "0286", "0313", "0322",
        "0331", "0352", "0358", "0373", "0399",
    ] {
        let path = directory.join(format!("_P2M{scene}.X3F"));
        let bytes = std::fs::read(crate::sidecar::path(&path)).ok();
        let saved = bytes
            .as_deref()
            .map(crate::sidecar::decode)
            .transpose()
            .unwrap()
            .unwrap_or_else(|| Settings::new("Sunlight", "Standard"));
        for approach in [Approach::Faithful, Approach::LocalGradient] {
            let mut settings = saved.clone();
            settings.highlight_mode = HighlightMode::ColorField;
            settings.highlight_restoration = true;
            settings.color_field = Controls {
                approach,
                ..Default::default()
            };
            let mut session = RenderSession::new(Source::open(&path).unwrap());
            for selectivity in [0., 0.25, 1., 0.] {
                settings.color_field.selectivity = selectivity;
                let requested = ["highlight.layers", "final"];
                let cached = engine
                    .render_session(
                        &mut session,
                        &settings,
                        &requested,
                        || false,
                        &mut Profile::default(),
                    )
                    .unwrap()
                    .unwrap();
                let fresh = engine
                    .render_artifacts(
                        session.source(),
                        &settings,
                        &requested,
                        &mut Profile::default(),
                    )
                    .unwrap();
                for (name, (a, b)) in requested.iter().zip(cached.iter().zip(&fresh)) {
                    let (Content::Image(a), Content::Image(b)) = (&a.content, &b.content) else {
                        panic!("image expected")
                    };
                    assert!(engine.readback_image(a).unwrap()==engine.readback_image(b).unwrap(),"{scene} {approach:?} selectivity {selectivity} {name}: non-repeatable samples");
                }
            }
            eprintln!("EXACT-COLOR-FIELD {scene} {approach:?}: zero, 0.25, one and restored zero");
        }
        assert_eq!(std::fs::read(crate::sidecar::path(&path)).ok(), bytes);
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_HIGHLIGHT_DIR"]
fn gpu_color_field_session_and_dng_preserve_saved_choices_and_owned_images() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("FOVEONIC_HIGHLIGHT_DIR").expect("set FOVEONIC_HIGHLIGHT_DIR"),
    );
    let path = directory.join("_P2M0095.X3F");
    let original_sidecar = std::fs::read(crate::sidecar::path(&path)).unwrap();
    let mut settings = crate::sidecar::decode(&original_sidecar).unwrap();
    settings.highlight_mode = HighlightMode::ColorField;
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(Source::open(&path).unwrap());
    let retained = session_layers(&engine, &mut session, &settings, &mut Profile::default());
    let retained_bytes = engine.readback_image(&retained).unwrap();
    for step in 0..6 {
        match step {
            0 => settings.color_field.selectivity = 0.25,
            1 => settings.color_field.selectivity = 0.5,
            2 => settings.color_field.selectivity = 1.,
            3 => settings.color_field.selectivity = 0.,
            4 => settings.color_field.approach = Approach::LocalGradient,
            _ => settings.color_field.approach = Approach::Faithful,
        }
        let mut profile = Profile::new(crate::profile::Mode::Wall);
        let cached = session_layers(&engine, &mut session, &settings, &mut profile);
        if step == 1 {
            assert!(
                !profile.host.iter().any(|t| matches!(
                    t.stage,
                    "upload sensor" | "submit calibration" | "submit repair"
                )),
                "donation edits should reuse the unchanged source/repair prefix"
            );
        }
        let mut fresh = engine
            .render_artifacts(
                session.source(),
                &settings,
                &["highlight.layers"],
                &mut Profile::default(),
            )
            .unwrap();
        let Content::Image(fresh) = fresh.pop().unwrap().content else {
            panic!("image expected")
        };
        let a = engine.readback_image(&cached).unwrap();
        let b = engine.readback_image(&fresh).unwrap();
        // Parallel floating reductions are not required to be bit-identical.
        // Existing stock equality tests remain strict and are not changed here.
        let maximum = bytemuck::cast_slice::<u8, f32>(&a)
            .iter()
            .zip(bytemuck::cast_slice::<u8, f32>(&b))
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        eprintln!("color-field cached/fresh step {step}: max {maximum} calibrated codes");
        assert!(
            maximum <= 0.5,
            "cached/fresh color-field divergence: {maximum}"
        );
        assert_eq!(
            engine.readback_image(&retained).unwrap(),
            retained_bytes,
            "retained highlight image changed"
        );
    }
    settings.color_field = Controls {
        selectivity: 0.5,
        ..Controls::default()
    };
    let base = crate::dng::prepare_base(
        &engine,
        session.source(),
        &settings,
        &mut Profile::default(),
    )
    .unwrap();
    assert_eq!(base.settings().highlight_mode, HighlightMode::ColorField);
    assert_eq!(base.settings().color_field.selectivity, 0.5);
    assert!(
        bytemuck::cast_slice::<u8, f32>(&engine.readback_image(&base).unwrap())
            .iter()
            .all(|v| v.is_finite())
    );
    drop(base);
    let destination = std::env::temp_dir().join(format!(
        "foveonic-color-field-{}-{}.dng",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    crate::dng::write_source(
        &engine,
        session.source(),
        &settings,
        &destination,
        &mut Profile::default(),
    )
    .unwrap();
    assert!(std::fs::metadata(&destination).unwrap().len() > 80_000_000);
    let mut file = std::fs::File::open(&destination).unwrap();
    let mut header = [0_u8; 4];
    std::io::Read::read_exact(&mut file, &mut header).unwrap();
    assert_eq!(header, [b'I', b'I', 42, 0]);
    std::fs::remove_file(destination).unwrap();
    assert_eq!(
        std::fs::read(crate::sidecar::path(&path)).unwrap(),
        original_sidecar
    );
}
