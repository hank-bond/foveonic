//! Checks independently generated metadata matrices against the stopped native preparation.
use super::*;
#[test]
#[ignore = "requires QUATTRO_X3F and QUATTRO_STAGE2_BOUNDARY prepared native controls"]
fn standard_chroma_sectors_match_native_preparation() {
    let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let root = std::path::PathBuf::from(std::env::var_os("QUATTRO_STAGE2_BOUNDARY").unwrap());
    assert_eq!(
        bytemuck::cast_slice::<f32, u8>(&RGB_TO_CHROMA),
        std::fs::read(root.join("mcc-chroma.bin")).unwrap()
    );
    assert_eq!(
        bytemuck::cast_slice::<f32, u8>(&CHROMA_BIN_BOUNDARIES),
        std::fs::read(root.join("mcc-bins.bin")).unwrap()
    );
    let mut bytes = Vec::new();
    for sector in sectors(&source.metadata, "Standard").unwrap() {
        bytes.extend_from_slice(bytemuck::cast_slice::<f32, u8>(&sector.matrix));
        bytes.extend_from_slice(&sector.quadrant.to_le_bytes());
        bytes.extend_from_slice(&sector.bin.to_le_bytes());
    }
    assert_eq!(bytes, std::fs::read(root.join("mcc-sectors.bin")).unwrap());
    let prepared = prepare(&source.metadata, "Standard").unwrap();
    assert_eq!(prepared.extent(), [2944, 1836]);
    let _key = prepared.key();
}

#[test]
fn ordinary_dp_models_share_the_operation_but_keep_their_metadata_sectors() {
    use crate::source::Matrix;
    let matrix = |dimensions, values| Matrix {
        encoding: MatrixEncoding::Float32,
        representation: "f64",
        dimensions,
        values,
    };
    let mut metadata = Metadata {
        width: 5888,
        height: 3672,
        rotation: 0,
        processing_version: Some([3, 1, 2, 0]),
        properties: Default::default(),
        lists: Default::default(),
        matrices: [
            ("CAMERAID".into(), matrix(vec![1], vec![80.])),
            (
                "MultiAxisTable_Standard".into(),
                matrix(
                    vec![2, 5, 21],
                    (0..210).map(|i| if i >= 105 { 1. } else { 0. }).collect(),
                ),
            ),
        ]
        .into(),
    };
    let original = prepare(&metadata, "Standard").unwrap();
    for camera in 80..=83 {
        metadata.matrices.get_mut("CAMERAID").unwrap().values[0] = f64::from(camera);
        assert!(prepare(&metadata, "Standard").unwrap().key() == original.key());
    }
    metadata
        .matrices
        .get_mut("MultiAxisTable_Standard")
        .unwrap()
        .values[1] = 2.;
    assert!(prepare(&metadata, "Standard").unwrap().key() != original.key());
    metadata.processing_version = Some([4, 0, 0, 0]);
    assert!(prepare(&metadata, "Standard").is_err());
}

#[test]
fn stored_version_selects_sector_correction_independently_of_table_presence() {
    let mut metadata = crate::source::Metadata {
        width: 5888,
        height: 3672,
        rotation: 0,
        processing_version: None,
        properties: Default::default(),
        lists: Default::default(),
        matrices: Default::default(),
    };
    for (version, expected) in [
        ([3, 0, 2, 0], false),
        ([3, 0, 3, 65535], false),
        ([3, 0, 4, 0], true),
        ([3, 1, 3, 0], true),
        ([4, 0, 1, 0], false),
    ] {
        metadata.processing_version = Some(version);
        assert_eq!(enabled(&metadata), expected, "{version:?}");
    }
}
