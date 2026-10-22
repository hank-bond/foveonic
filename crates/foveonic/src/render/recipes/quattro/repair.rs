//! Prepares the ordinary DP sparse-repair noise model and correction switch.

use crate::{
    nodes::sensor::sparse_repair::Prepared,
    source::{Metadata, Result},
};

// Ordinary preprocessing initializes these Float32 constants immediately before
// sparse repair. They are independent of CAMF ColorNoiseModel and edit WB.
// RGB stores one constant/linear/quadratic triple per channel, in R/G/B order.
pub(super) const RGB_NOISE: [f32; 9] = [
    24.362,
    0.04835,
    -0.000000125,
    15.16425,
    0.0275,
    -0.00000005,
    1.3026875,
    0.0271375,
    0.00000005,
];
// This is a measured noise-model coefficient, not a logarithm constant.
#[allow(clippy::approx_constant)]
pub(super) const BLUE_NOISE: [f32; 3] = [20.843, 0.4342, 0.0000008];

/// `color_hp_correction` is the recipe's explicit correction-switch choice.
/// The reference implementation inverts that switch before passing `preserve`
/// to the ordinary single-image HI wrapper. It is not a camera metadata field.
pub(super) fn prepare(metadata: &Metadata, color_hp_correction: bool) -> Result<Prepared> {
    let extent = super::calibration::ordinary_dp_extent(metadata)?;
    let scale = super::calibration::normalization(metadata)?;
    Prepared::new(extent, RGB_NOISE, BLUE_NOISE, scale, !color_hp_correction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Matrix, MatrixEncoding};

    #[test]
    fn correction_switch_and_normalization_change_prepared_controls() {
        let scalar = |v| Matrix {
            encoding: MatrixEncoding::Float32,
            representation: "f64",
            dimensions: vec![1],
            values: vec![v],
        };
        let mut metadata = Metadata {
            width: 5888,
            height: 3672,
            rotation: 0,
            processing_version: Some([3, 1, 2, 0]),
            properties: Default::default(),
            lists: Default::default(),
            matrices: [
                ("CAMERAID".into(), scalar(81.)),
                ("SensorISO".into(), scalar(100.)),
            ]
            .into(),
        };
        let preserve = prepare(&metadata, false).unwrap();
        let correct = prepare(&metadata, true).unwrap();
        assert_eq!(preserve.extent(), [2944, 1836]);
        assert!(preserve.key() != correct.key());
        // The same numerical model is seeded by the ordinary model-22 wrapper.
        for camera in 80..=83 {
            metadata
                .matrices
                .insert("CAMERAID".into(), scalar(f64::from(camera)));
            assert!(prepare(&metadata, false).unwrap().key() == preserve.key());
        }
        metadata.matrices.insert("SensorISO".into(), scalar(200.));
        assert!(prepare(&metadata, false).unwrap().key() != preserve.key());
    }
    #[test]
    #[ignore = "requires SPARSE_NATIVE_BOUNDARY containing native control metadata"]
    fn caller_noise_literals_match_native_control_words() {
        let directory = std::path::PathBuf::from(
            std::env::var_os("SPARSE_NATIVE_BOUNDARY").expect("set SPARSE_NATIVE_BOUNDARY"),
        );
        let controls: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("sparse-boundary.json")).unwrap())
                .unwrap();
        for (name, expected) in [
            ("rgb_noise", RGB_NOISE.as_slice()),
            ("blue_noise", BLUE_NOISE.as_slice()),
        ] {
            let actual = controls[name].as_array().unwrap();
            assert_eq!(actual.len(), expected.len());
            for (&expected, actual) in expected.iter().zip(actual) {
                assert_eq!(
                    expected.to_bits(),
                    (actual.as_f64().unwrap() as f32).to_bits()
                );
            }
        }
    }
}
