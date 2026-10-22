//! Checks camera-recorded white-balance aliases against complete native shading grids.
use super::*;
#[test]
#[ignore = "requires QUATTRO_X3F and RECORDED_COLOR_SHADING_FIXTURES"]
fn all_recorded_alias_grids_match_native_tables() {
    let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let root =
        std::path::PathBuf::from(std::env::var_os("RECORDED_COLOR_SHADING_FIXTURES").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("cases.json")).unwrap()).unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["value"].as_str().unwrap();
        for channel in ["R", "B"] {
            let grid = color(&source.metadata, name, channel).unwrap();
            let fixture = &case["channels"][channel];
            assert_eq!(
                [grid.width, grid.height],
                [
                    fixture["width"].as_u64().unwrap() as usize,
                    fixture["height"].as_u64().unwrap() as usize
                ]
            );
            let expected = std::fs::read(root.join(fixture["file"].as_str().unwrap())).unwrap();
            assert_eq!(
                bytemuck::cast_slice::<f32, u8>(&grid.values),
                expected,
                "{name}/{channel}"
            );
        }
    }
}
#[test]
fn camera_recorded_white_balance_names_are_separate_from_edit_controls() {
    use crate::source::{Matrix, MatrixEncoding};
    let mut metadata = Metadata {
        width: 5888,
        height: 3672,
        rotation: 0,
        processing_version: Some([3, 1, 2, 0]),
        properties: Default::default(),
        lists: Default::default(),
        matrices: Default::default(),
    };
    for (number, name) in [
        (0, "Auto"),
        (1, "Auto"),
        (2, "Sunlight"),
        (3, "Shade"),
        (4, "Overcast"),
        (5, "Incandescent"),
        (6, "Fluorescent"),
        (7, "Flash"),
        (8, "Custom"),
        (9, "Auto"),
        (10, "AutoLSP"),
        (11, "ColorTemp"),
        (12, "Auto"),
    ] {
        metadata.matrices.insert(
            "WhiteBalance".into(),
            Matrix {
                encoding: MatrixEncoding::Uint8,
                representation: "f64",
                dimensions: vec![1],
                values: vec![f64::from(number)],
            },
        );
        assert_eq!(
            super::super::hlp::recorded_balance(&metadata).unwrap(),
            name
        );
    }
}
