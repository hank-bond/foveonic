//! Checks which photos a menu reset or preset reaches and what each receives.

use super::*;
use std::sync::Arc;

fn photo(
    name: &str,
    model: &str,
    compensation: &str,
) -> (
    PathBuf,
    Settings,
    Settings,
    Option<Arc<foveonic::source::Metadata>>,
) {
    let metadata = crate::metadata::processing_metadata(
        [
            ("CAMMODEL".into(), model.into()),
            ("EXPCOMP".into(), compensation.into()),
        ]
        .into(),
    );
    let mut settings = camera_settings(&metadata, None).unwrap();
    settings.exposure += 0.75;
    settings.exposure_auto = false;
    settings.saturation = 0.6;
    (
        PathBuf::from(name),
        settings.clone(),
        settings,
        Some(Arc::new(metadata)),
    )
}

fn sidecars() -> crate::sidecars::Session {
    let mut sidecars = crate::sidecars::Session::new(
        eframe::egui::Context::default(),
        false,
        Arc::new(foveonic::decode_cache::Cache::new()),
    );
    sidecars.set_memory_only(true);
    sidecars
}

#[test]
fn a_reset_gives_each_photo_its_own_camera_values() {
    let photos = [
        photo("a.x3f", "SIGMA DP2 Merrill", "-1"),
        photo("b.x3f", "SIGMA sd Quattro H", "1"),
    ];
    let paths = photos.clone().map(|photo| photo.0);
    let cameras = photos
        .clone()
        .map(|photo| camera_settings(photo.3.as_deref().unwrap(), None).unwrap());
    assert_ne!(
        cameras[0].reconstruction, cameras[1].reconstruction,
        "the photos need different camera values for this check to mean anything"
    );
    let mut runtime = Runtime::rendered(photos);
    let mut sidecars = sidecars();

    let baselines = reset_baselines(&runtime, None, &paths, Scope::All).unwrap();
    for (path, baseline) in paths.iter().zip(baselines) {
        let change = Change::Scope {
            scope: Scope::All,
            source: Box::new(baseline),
        };
        runtime.edit(std::slice::from_ref(path), &change, &mut sidecars);
    }

    for (path, camera) in paths.iter().zip(&cameras) {
        let settings = runtime.settings(path).unwrap();
        assert_eq!(
            (
                settings.reconstruction,
                settings.exposure,
                settings.saturation
            ),
            (camera.reconstruction, camera.exposure, camera.saturation),
            "{} takes its own camera values",
            path.display()
        );
    }
}

#[test]
fn a_reset_waits_for_every_photo_to_be_rendered() {
    let ready = photo("a.x3f", "SIGMA DP2 Merrill", "0");
    let (waiting, settings, numeric, _) = photo("b.x3f", "SIGMA DP2 Merrill", "0");
    let paths = [ready.0.clone(), waiting.clone()];
    let runtime = Runtime::rendered([ready, (waiting, settings, numeric, None)]);

    assert!(reset_baselines(&runtime, None, &paths, Scope::All)
        .unwrap()
        .is_empty());
    assert_eq!(
        reset_baselines(&runtime, None, &paths[..1], Scope::All)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn a_preset_reaches_only_photos_of_its_camera_family() {
    let photos = [
        photo("a.x3f", "SIGMA DP2 Merrill", "0"),
        photo("b.x3f", "SIGMA sd Quattro H", "0"),
        photo("c.x3f", "SIGMA DP2 Merrill", "0"),
    ];
    let paths = photos.clone().map(|photo| photo.0);
    let runtime = Runtime::rendered(photos);

    assert_eq!(
        preset_targets(&runtime, &paths, runtime.family(&paths[0]).unwrap()),
        [paths[0].clone(), paths[2].clone()]
    );
}
