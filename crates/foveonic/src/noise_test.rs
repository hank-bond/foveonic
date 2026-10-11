//! Checks noise controls, metadata-derived reference parameters, and uniform-gain model scaling.

use super::*;

#[test]
fn noise_controls_have_five_choices_and_neutral_detail() {
    assert_eq!(
        Controls::default(),
        Controls {
            chroma: 2,
            luminance: 2,
            detail: 1.,
            ..Default::default()
        }
    );
    for choice in 0..=4 {
        Controls {
            chroma: choice,
            luminance: 4 - choice,
            detail: 0.,
            ..Default::default()
        }
        .validate()
        .unwrap();
    }
    for invalid in [
        Controls {
            chroma: 5,
            ..Default::default()
        },
        Controls {
            luminance: 5,
            ..Default::default()
        },
        Controls {
            detail: -1.,
            ..Default::default()
        },
        Controls {
            detail: f32::NAN,
            ..Default::default()
        },
    ] {
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn advanced_override_validation_and_link_state_are_parameter_local() {
    let mut controls = Controls::default();
    for parameter in Parameter::ALL {
        assert!(controls.overrides.get(parameter).is_none());
        for value in [*parameter.bounds().start(), *parameter.bounds().end()] {
            controls.overrides.set(parameter, Some(value));
            controls.validate().unwrap();
            assert_eq!(controls.overrides.get(parameter), Some(value));
        }
        controls.overrides.set(parameter, Some(f32::NAN));
        assert!(controls.validate().is_err());
        controls
            .overrides
            .set(parameter, Some(*parameter.bounds().end() + 1.));
        assert!(controls.validate().is_err());
        controls.overrides.set(parameter, None);
    }
    assert_eq!(controls, Controls::default());
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR"]
fn linked_parameters_share_engine_values_capture_without_jumps_and_keep_overrides() {
    let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let source = crate::source::Source::open(path).unwrap();
    let mut controls = Controls::default();
    let original = prepare(&source.metadata, "Sunlight", 1., controls).unwrap();
    let values = calculated_values(&source.metadata, "Sunlight", 1., controls).unwrap();
    for parameter in Parameter::ALL {
        controls
            .overrides
            .set(parameter, Some(values[parameter as usize]));
    }
    assert_eq!(
        bytemuck::bytes_of(&prepare(&source.metadata, "Sunlight", 1., controls).unwrap()),
        bytemuck::bytes_of(&original)
    );
    controls.chroma = 0;
    controls.luminance = 4;
    controls.detail = 0.5;
    let manual = prepare(&source.metadata, "Sunlight", 1., controls).unwrap();
    assert_eq!(
        [
            manual.filter[1],
            manual.filter[2],
            manual.filter[3],
            manual.edge[0],
            manual.diffusion[1],
            manual.residual[0],
            manual.residual[1]
        ],
        values
    );
    assert_eq!(manual.residual[3], 0.5);
    controls.overrides = Overrides::default();
    let latest = calculated_values(&source.metadata, "Sunlight", 1., controls).unwrap();
    assert_ne!(latest, values);
    for parameter in Parameter::ALL {
        let mut other = controls;
        if parameter.parent() == "Chroma noise" {
            other.luminance = 0;
        } else {
            other.chroma = 4;
        }
        assert_eq!(
            calculated_values(&source.metadata, "Sunlight", 1., other).unwrap()[parameter as usize],
            latest[parameter as usize],
            "{} has a second basic parent",
            parameter.key()
        );
    }
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR"]
fn camera_noise_parameters_match_reference_and_scale_with_sensor_gain() {
    let dir = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/noise-parameters.json")).unwrap();
    for fixture in fixtures.as_array().unwrap() {
        let source =
            crate::source::Source::open(dir.join(fixture["file"].as_str().unwrap())).unwrap();
        let p = prepare(&source.metadata, "Sunlight", 1., Controls::default()).unwrap();
        for (name, values) in [
            ("filter", &p.filter[..]),
            ("edge", &p.edge[..]),
            ("weights", &p.weights[..3]),
            ("mono", &p.mono[..3]),
            ("residual", &p.residual[..]),
            ("diffusion", &p.diffusion[..]),
        ] {
            for (actual, expected) in values.iter().zip(fixture[name].as_array().unwrap()) {
                let expected = expected.as_f64().unwrap() as f32;
                assert!(
                    (*actual - expected).abs() <= expected.abs().max(1.) * 1e-5,
                    "{} {name}: {actual} vs {expected}",
                    fixture["file"]
                );
            }
        }
        for (actual, expected) in p
            .model
            .iter()
            .flat_map(|r| &r[..3])
            .zip(fixture["model"].as_array().unwrap())
        {
            let expected = expected.as_f64().unwrap() as f32;
            assert!(
                (*actual - expected).abs() <= expected.abs().max(1.) * 1e-5,
                "model {actual} vs {expected}"
            );
        }
        let gained = prepare(&source.metadata, "Sunlight", 0.5, Controls::default()).unwrap();
        for c in 0..3 {
            assert_eq!(gained.model[0][c], p.model[0][c] * 0.25);
            assert_eq!(gained.model[1][c], p.model[1][c] * 0.5);
            assert_eq!(gained.model[2][c], p.model[2][c]);
            assert_eq!(gained.weights[c], p.weights[c] * 2.);
        }
        assert_eq!(gained.edge[3], p.edge[3]);
    }
}
