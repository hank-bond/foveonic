//! Checks mixed-resolution sensor inspection, selective execution, and immutable retained planes.

use super::*;
use crate::{
    image::{Layout, Space},
    inspection::Content,
    source::{Metadata, ProcessingFamily, Source, SourceStorage},
};

fn source() -> Source {
    Source {
        storage: SourceStorage::PackedRgbBluePhases,
        metadata: Arc::new(Metadata {
            processing_version: Some([3, 1, 2, 0]),
            width: 6,
            height: 4,
            rotation: 90,
            properties: [("CAMMODEL".into(), "SIGMA dp2 Quattro".into())].into(),
            matrices: Default::default(),
            lists: Default::default(),
        }),
        pixels: (0..6)
            .flat_map(|i| [2100. + i as f32, 2200., 2300., 2400., 2500., 2600.])
            .collect(),
        camf: vec![],
        timings: vec![],
    }
}

#[test]
fn raw_catalogue_does_not_require_calibration_fields() {
    let mut source = source();
    let m = Arc::make_mut(&mut source.metadata);
    m.processing_version = Some([4, 0, 0, 0]);
    assert_eq!(m.processing_family().unwrap(), ProcessingFamily::Quattro);
    let settings = Settings::new("", "");
    assert_eq!(
        crate::inspection::output_names(&settings, Some(m)),
        ["sensor.rgb", "sensor.blue"]
    );
}

#[test]
#[ignore = "requires a GPU"]
fn planes_keep_their_extents_and_retained_siblings() {
    let engine = Engine::new().unwrap();
    let settings = Settings::new("", "");
    let mut session = RenderSession::new(source());
    let mut retained = Vec::new();
    // Begin with a side output. Later requests must preserve earlier snapshots.
    for name in [
        "sensor.blue",
        "sensor.rgb",
        "extraction.blue",
        "extraction.rgb",
        "sensor.blue",
    ] {
        let artifacts = engine
            .render_session(
                &mut session,
                &settings,
                &[name],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap();
        let Content::Image(image) = &artifacts[0].content else {
            panic!("expected plane");
        };
        let blue = name.ends_with("blue");
        let corrected = name.starts_with("extraction.");
        let extent = if blue { [6, 4] } else { [3, 2] };
        assert_eq!(image.description().extent, extent);
        assert_eq!(
            image.description().layout,
            if blue {
                Layout::Scalar32Float
            } else {
                Layout::Rgb32Float
            }
        );
        assert_eq!(
            image.description().space,
            match (blue, corrected) {
                (false, false) => Space::SensorRgb,
                (true, false) => Space::SensorBlue,
                (false, true) => Space::CorrectedSensorRgb,
                (true, true) => Space::CorrectedSensorBlue,
            }
        );
        assert_eq!(image.view().crop, [0, 0, extent[0], extent[1]]);
        assert_eq!(image.view().extent, [extent[1], extent[0]]);
        let bytes = engine.readback_image(image).unwrap();
        let values: &[f32] = bytemuck::cast_slice(&bytes);
        assert_eq!(
            values[0],
            if blue {
                if corrected {
                    252.
                } else {
                    2300.
                }
            } else if corrected {
                52.
            } else {
                2100.
            }
        );
        let scopes = engine.scopes(image, None, &mut Profile::default()).unwrap();
        assert_eq!(scopes.channels(), if blue { 1 } else { 3 });
        assert_eq!(
            scopes.histogram(0).iter().sum::<u32>(),
            extent[0] * extent[1]
        );
        assert_eq!(scopes.outside(0), [0, 0, 0]);
        retained.push((image.clone(), bytes));
        for (prior, expected) in &retained {
            assert_eq!(&engine.readback_image(prior).unwrap(), expected);
        }
    }
    let mixed = engine
        .render_session(
            &mut session,
            &settings,
            &["sensor.blue", "extraction.rgb"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(mixed.len(), 2);
    assert!(engine
        .render(session.source(), &settings, &mut Profile::default())
        .is_err());

    // Earlier requests must stop before unrelated calibration preparation.
    let mut uncalibrated = source();
    Arc::make_mut(&mut uncalibrated.metadata).matrices.insert(
        "RawOffset".into(),
        crate::source::Matrix {
            encoding: crate::source::MatrixEncoding::Float32,
            representation: "f64",
            dimensions: vec![1],
            values: vec![f64::NAN],
        },
    );
    assert!(engine
        .render_artifacts(
            &uncalibrated,
            &settings,
            &["sensor.rgb", "sensor.blue"],
            &mut Profile::default(),
        )
        .is_ok());
    assert!(engine
        .render_artifacts(
            &uncalibrated,
            &settings,
            &["extraction.rgb"],
            &mut Profile::default(),
        )
        .is_err());
}

#[test]
#[ignore = "requires QUATTRO_X3F and a GPU"]
fn original_quattro_planes_have_independent_geometry() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let settings = Settings::new("", "");
    let directory = std::env::var_os("QUATTRO_RESOURCE_DIRECTORY");
    let engine = if let Some(directory) = &directory {
        Engine::new()
            .unwrap()
            .with_calibration_resources(directory)
            .unwrap()
    } else {
        Engine::new().unwrap()
    };
    let names = crate::inspection::output_names(&settings, Some(&source.metadata));
    for name in names
        .into_iter()
        .filter(|name| *name != "phase-gain.blue" || directory.is_some())
    {
        let result = engine
            .render_artifacts(&source, &settings, &[name], &mut Profile::default())
            .unwrap();
        let Content::Image(image) = &result[0].content else {
            panic!("expected plane");
        };
        let extent = source.storage_extent().map(|v| v as u32);
        assert_eq!(
            image.description().extent,
            if name.ends_with("blue") {
                extent.map(|v| v * 2)
            } else {
                extent
            }
        );
        let bytes = engine.readback_image(image).unwrap();
        assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .all(|v| v.is_finite()));
    }
}

#[test]
#[ignore = "requires DP2 QUATTRO_X3F, QUATTRO_RESOURCE_DIRECTORY and PHASE_GAIN_REFERENCE"]
fn original_gain_stage_matches_independent_native_output() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let engine = Engine::new()
        .unwrap()
        .with_calibration_resources(std::env::var_os("QUATTRO_RESOURCE_DIRECTORY").unwrap())
        .unwrap();
    let mut session = RenderSession::new(source);
    let settings = Settings::new("", "");
    let result = engine
        .render_session(
            &mut session,
            &settings,
            &["phase-gain.blue"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    let Content::Image(image) = &result[0].content else {
        panic!("expected blue plane");
    };
    let native = std::path::PathBuf::from(std::env::var_os("PHASE_GAIN_REFERENCE").unwrap())
        .join("c81-original-output.f32");
    assert_eq!(
        engine.readback_image(image).unwrap(),
        std::fs::read(native).unwrap()
    );
    let stored = engine.readback_image(image).unwrap();
    let again = engine
        .render_session(
            &mut session,
            &settings,
            &["extraction.blue", "phase-gain.blue"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(again.len(), 2);
    assert_eq!(engine.readback_image(image).unwrap(), stored);
}
