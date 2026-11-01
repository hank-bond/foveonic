//! Checks calibration artifact validity and nearest-ISO selection without hard-coding fitted parameters.

use super::*;

fn data() -> serde_json::Value {
    serde_json::json!({
        "shield_color_axis": [1., 0., 0.],
        "row_priors": [
            {"iso":100, "inverse_covariance":[1.,0.,0.,0.,1.,0.,0.,0.,1.]},
            {"iso":200, "inverse_covariance":[2.,0.,0.,0.,2.,0.,0.,0.,2.]}
        ]
    })
}

#[test]
fn embedded_model_and_nearest_iso_selection_are_valid() {
    assert!(embedded().is_ok());
    let model = Model::parse(&serde_json::to_vec(&data()).unwrap()).unwrap();
    for iso in [50, 100, 150] {
        assert_eq!(model.row_inverse_covariance(iso)[0], 1.);
    }
    for iso in [151, 200, 6400] {
        assert_eq!(model.row_inverse_covariance(iso)[0], 2.);
    }
}

#[test]
fn rejects_missing_empty_ambiguous_and_invalid_model_data() {
    assert!(Model::parse(b"{}").is_err());
    for change in 0..6 {
        let mut value = data();
        match change {
            0 => value["row_priors"] = serde_json::json!([]),
            1 => value["row_priors"][0]["iso"] = serde_json::json!(0),
            2 => value["row_priors"][1]["iso"] = serde_json::json!(100),
            3 => value["row_priors"][1]["iso"] = serde_json::json!(50),
            4 => value["row_priors"][0]["inverse_covariance"] = serde_json::json!([1.]),
            _ => value["shield_color_axis"] = serde_json::json!([0., 0., 0.]),
        }
        assert!(Model::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}
