//! Checks ordered named native outputs, warm root changes and cancellation recovery.

use super::tests::{fixture, settings};
use crate::{
    inspection::{Artifact, Content},
    profile::{Mode, Profile, Timing},
    render::{Engine, ProcessingWorkingSet, RenderSession, Scale, SessionRender, SessionUse},
};
use std::{cell::Cell, sync::Arc};

fn assert_outputs_match(
    engine: &Engine,
    observed: &[Artifact],
    expected: &[Artifact],
    names: &[&str],
) {
    assert_eq!(observed.len(), names.len());
    assert_eq!(expected.len(), names.len());
    for ((observed, expected), &name) in observed.iter().zip(expected).zip(names) {
        assert_eq!(observed.name, name);
        assert_eq!(expected.name, name);
        assert_eq!(observed.settings, expected.settings);
        assert_eq!(observed.dehaze_estimate, expected.dehaze_estimate);
        assert_eq!(observed.metadata.properties, expected.metadata.properties);
        let (Content::Image(actual), Content::Image(expected)) =
            (&observed.content, &expected.content)
        else {
            panic!("{name} must be a photographic image");
        };
        assert_eq!(actual.description(), expected.description());
        assert_eq!(actual.view(), expected.view());
        assert_eq!(actual.settings(), expected.settings());
        assert!(
            engine.readback_image(actual).unwrap() == engine.readback_image(expected).unwrap(),
            "complete samples differ for {name} in {names:?}"
        );
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn native_named_outputs_match_ordinary_order_samples_and_warm_root_changes() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let mut settings = settings();
    settings.working_scale = Scale::NATIVE;
    settings.reconstruction.as_mut().unwrap().chroma = 1;
    let mut ordinary = RenderSession::new(source.clone());
    let mut working = ProcessingWorkingSet::new();
    working
        .insert_native(
            1,
            RenderSession::new(source.clone()),
            &engine,
            SessionUse::Foreground,
        )
        .unwrap();
    // Return to earlier roots in the opposite order after using another branch.
    let requests: [&[&str]; 3] = [
        &["repair.layers", "calibration.layers"],
        &["color-dq.rgb", "color.rgb"],
        &["calibration.layers", "repair.layers"],
    ];
    for names in requests {
        let expected = engine
            .render_session(
                &mut ordinary,
                &settings,
                names,
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap();
        let Some(SessionRender::Rendered(observed)) = engine
            .render_native(
                &mut working,
                1,
                &settings,
                names,
                SessionUse::Foreground,
                || false,
                &mut Profile::default(),
                |_| {},
                None,
            )
            .unwrap()
        else {
            panic!("named native outputs must complete: {names:?}");
        };
        assert_outputs_match(&engine, &observed, &expected, names);
        assert!(Arc::ptr_eq(
            &working.native(1).unwrap().shared_source(),
            &source
        ));
        println!("native named outputs exact: {names:?}");
    }
    working.clear().unwrap();
}

fn cache_stages(profile: &Profile) -> Vec<&'static str> {
    profile
        .host
        .iter()
        .filter_map(|timing| {
            matches!(
                timing.stage,
                "prepare session cache" | "finish session cache"
            )
            .then_some(timing.stage)
        })
        .collect()
}

fn assert_restored_profile(profile: &Profile, stages_before: usize) {
    assert_eq!(profile.mode, Mode::Wall);
    assert_eq!(profile.host[0].stage, "caller observation");
    assert_eq!(
        &cache_stages(profile)[stages_before..],
        ["prepare session cache", "finish session cache"]
    );
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn native_named_failure_and_submitted_cancel_preserve_attempts_profile_and_exact_retry() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let mut settings = settings();
    settings.working_scale = Scale::NATIVE;
    settings.reconstruction.as_mut().unwrap().chroma = 1;
    let names = ["color-dq.rgb", "color.rgb"];
    let mut working = ProcessingWorkingSet::new();
    working
        .insert_native(
            1,
            RenderSession::new(source.clone()),
            &engine,
            SessionUse::Foreground,
        )
        .unwrap();
    let mut profile = Profile::new(Mode::Wall);
    profile.host.push(Timing {
        stage: "caller observation",
        milliseconds: 0.,
    });
    let warm = engine
        .render_native(
            &mut working,
            1,
            &settings,
            &names,
            SessionUse::Foreground,
            || false,
            &mut profile,
            |_| {},
            None,
        )
        .unwrap();
    assert!(matches!(warm, Some(SessionRender::Rendered(_))));
    drop(warm);
    assert_restored_profile(&profile, 0);
    for submitted_cancel in [false, true] {
        let mut requested = settings.clone();
        if submitted_cancel {
            // This camera white balance changes inputs upstream of both roots.
            requested.white_balance = "Incandescent".into();
        } else {
            requested.exposure = f32::NAN;
        }
        let before = engine.gpu_activity().submissions;
        let stages_before = cache_stages(&profile).len();
        let cancelled_after_submission = Cell::new(false);
        let outcome = engine.render_native(
            &mut working,
            1,
            &requested,
            &names,
            SessionUse::Foreground,
            || {
                let cancel = submitted_cancel && engine.gpu_activity().submissions > before;
                cancelled_after_submission.set(cancel);
                cancel
            },
            &mut profile,
            |_| {},
            None,
        );
        let submissions = engine.gpu_activity().submissions - before;
        println!("native named interruption: submitted_cancel={submitted_cancel}, submissions={submissions}");
        if submitted_cancel {
            assert!(cancelled_after_submission.get());
            assert!(engine.gpu_activity().submissions > before);
            assert!(matches!(outcome.unwrap(), Some(SessionRender::Superseded)));
            assert!(working
                .native(1)
                .unwrap()
                .changed_parameters(&requested)
                .is_empty());
        } else {
            assert!(outcome.is_err());
            assert_eq!(engine.gpu_activity().submissions, before);
            assert!(working
                .native(1)
                .unwrap()
                .last_settings
                .as_ref()
                .unwrap()
                .exposure
                .is_nan());
        }
        assert_restored_profile(&profile, stages_before);
        assert_eq!(working.with_native(1, |_| Ok(42)).unwrap(), Some(42));
        let retry = if submitted_cancel {
            requested
        } else {
            settings.clone()
        };
        let stages_before = cache_stages(&profile).len();
        let Some(SessionRender::Rendered(observed)) = engine
            .render_native(
                &mut working,
                1,
                &retry,
                &names,
                SessionUse::Foreground,
                || false,
                &mut profile,
                |_| {},
                None,
            )
            .unwrap()
        else {
            panic!("failed or cancelled named outputs must be retryable");
        };
        assert_restored_profile(&profile, stages_before);
        assert!(Arc::ptr_eq(
            &working.native(1).unwrap().shared_source(),
            &source
        ));
        let expected = engine
            .render_session(
                &mut RenderSession::new(source.clone()),
                &retry,
                &names,
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap();
        assert_outputs_match(&engine, &observed, &expected, &names);
        println!("native named retry exact: submitted_cancel={submitted_cancel}, roots={names:?}");
    }
    working.clear().unwrap();
}
