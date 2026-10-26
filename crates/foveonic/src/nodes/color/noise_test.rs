//! Checks noise controls, metadata-derived reference parameters, and uniform-gain model scaling.

use super::*;

#[test]
fn noise_controls_have_five_choices_and_neutral_detail() {
    assert_eq!(
        Controls::default(),
        Controls {
            chroma: 2,
            luminance: 2,
            banding: 2,
            detail: 1.,
            ..Default::default()
        }
    );
    for choice in 0..=4 {
        Controls {
            chroma: choice,
            luminance: 4 - choice,
            banding: choice,
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
            banding: 5,
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
#[ignore = "requires FOVEONIC_FIXTURE_DIR"]
fn camera_noise_parameters_match_reference() {
    let dir = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/data/noise-parameters.json")).unwrap();
    let patterns: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/data/pattern-parameters.json")).unwrap();
    let recipes: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/data/recipe-parameters.json")).unwrap();
    for fixture in fixtures.as_array().unwrap() {
        let source =
            crate::source::Source::open(dir.join(fixture["file"].as_str().unwrap())).unwrap();
        let p = prepare(&source.metadata, "Sunlight", Controls::default()).unwrap();
        for (name, values) in [
            ("filter", &p.filter[..]),
            ("edge", &p.edge[..]),
            ("weights", &p.weights[..3]),
            ("mono", &p.mono[..3]),
            ("diffusion", &p.diffusion[..]),
        ] {
            for (index, (actual, expected)) in values
                .iter()
                .zip(fixture[name].as_array().unwrap())
                .enumerate()
            {
                let mut expected = expected.as_f64().unwrap() as f32;
                // Fixtures store the preset before the native reduced-local cap.
                // Shared preparation now returns the value used by filtering.
                if name == "filter" && index == 3 && p.reduction[0] > 0.5 && p.reduction[0] < 1. {
                    expected = expected.min(0.92);
                }
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
        for expected in patterns
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["file"] == fixture["file"] && row["chroma"] == 2)
        {
            let controls = Controls {
                chroma: expected["chroma"].as_u64().unwrap() as u32,
                ..Controls::default()
            };
            let params = prepare(&source.metadata, "Sunlight", controls).unwrap();
            let pattern = pattern_parameters(&source.metadata, params).unwrap();
            assert_eq!(
                u64::from(pattern.params.shape[3]),
                expected["half"].as_u64().unwrap()
            );
            for (c, v) in expected["thresholds"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
            {
                assert!(
                    (pattern.params.thresholds[c] - v.as_f64().unwrap() as f32).abs() < 0.000001,
                    "pattern {} preset {} field {c}",
                    expected["file"],
                    expected["chroma"]
                );
            }
        }
        for expected in recipes
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["file"] == fixture["file"] && r["choice"] == 2)
        {
            let choice = expected["choice"].as_u64().unwrap() as u32;
            let p = prepare(
                &source.metadata,
                "Sunlight",
                Controls {
                    chroma: choice,
                    luminance: choice,
                    ..Default::default()
                },
            )
            .unwrap();
            let recipe = couplings(
                &source.metadata,
                "Sunlight",
                Controls {
                    chroma: choice,
                    luminance: choice,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                recipe.despeckle_scale,
                expected["despeckle_scale"].as_f64().unwrap() as f32
            );
            assert_eq!(
                recipe.color_dq_limit,
                expected["color_dq_limit"].as_f64().unwrap() as f32
            );
            assert_eq!(p.reduction[0], expected["scale"].as_f64().unwrap() as f32);
            assert_eq!(
                u64::from(p.geometry[2]),
                expected["fixed_size"].as_u64().unwrap()
            );
        }
        let mut small = (*source.metadata).clone();
        small.width = 1;
        small.height = 1;
        assert!(couplings(&small, "Sunlight", Controls::default()).is_ok());
        assert!(prepare(&small, "Sunlight", Controls::default()).is_err());
        assert_eq!(p.column_area, [0; 4]);
        let column = column_parameters(&source.metadata, p).unwrap();
        let active =
            crate::nodes::sensor::calibration::rectangle(&source.metadata, "ActiveImageArea")
                .unwrap();
        assert_eq!(column.column_area[1], active[1]);
        assert_eq!(column.column_area[3], active[3]);
        assert_eq!(
            column.column[0],
            0.375
                * if p.edge[3] < 800. {
                    1.
                } else {
                    f32::from_bits(0x3faaa993)
                }
        );
        for banding in 0..=4 {
            let selected = prepare(
                &source.metadata,
                "Sunlight",
                Controls {
                    banding,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                selected.column[0],
                [0.625, 0.5, 0.375, 0.25, 0.125][banding as usize]
                    * if selected.edge[3] < 800. {
                        1.
                    } else {
                        f32::from_bits(0x3faaa993)
                    }
            );
            assert_eq!(selected.column[1], (15 + 6 * banding) as f32);
            assert!(selected.key(RecipeStage::Base) == p.key(RecipeStage::Base));
            let mut only_column = p;
            only_column.column = selected.column;
            assert_eq!(
                bytemuck::bytes_of(&selected),
                bytemuck::bytes_of(&only_column)
            );
            assert_eq!(
                selected.key(RecipeStage::Column) == p.key(RecipeStage::Column),
                banding == 2
            );
        }
        let mut incomplete = (*source.metadata).clone();
        incomplete.matrices.remove("DarkShieldTop");
        assert!(prepare(&incomplete, "Sunlight", Controls::default()).is_ok());
        assert!(column_parameters(&incomplete, p).is_err());
    }
}

#[test]
fn regional_geometry_rejects_invalid_grids_rectangles_and_reduction() {
    use bytemuck::Zeroable;
    let mut params = Parameters::zeroed();
    params.source = [256, 256, 4, 4];
    params.tile = [192, 192, 64, 11];
    params.reduction[0] = 0.375;
    params = params.full_frame();
    assert_eq!(params.reduced_tile_extent().unwrap(), [72, 72]);
    for rect in [[10, 10, 10, 20], [10, 20, 20, 10], [0, 0, 257, 256]] {
        assert!(params.for_region(rect).is_err());
    }
    for grid in [[256, 256, 0, 4], [0, 256, 4, 4], [255, 256, 4, 4]] {
        let mut invalid = params;
        invalid.source = grid;
        assert!(invalid.for_region([0, 0, 20, 20]).is_err());
    }
    for scale in [0., 0.25, 1.125, f32::NAN] {
        let mut invalid = params;
        invalid.reduction[0] = scale;
        assert!(invalid.reduced_tile_extent().is_err());
    }
    params.tile[0] = 193;
    assert!(params.reduced_tile_extent().is_err());
}
