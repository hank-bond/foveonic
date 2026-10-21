//! Checks source decoding and fixed-point correction against optional original-file reference boundaries.

use super::*;
use crate::nodes::sensor::fixed_point::{apply_correction, apply_transform};

#[test]
#[ignore = "requires CLASSIC_X3F and CLASSIC_CORRECTION_REFERENCE original-file fixtures"]
fn complete_original_correction_matches_reference() {
    let source = crate::source::Source::open(std::env::var("CLASSIC_X3F").unwrap()).unwrap();
    if let Ok(path) = std::env::var("CLASSIC_DECODED_REFERENCE") {
        assert_eq!(
            bytemuck::cast_slice::<f32, u8>(&source.pixels),
            std::fs::read(path).unwrap()
        );
    }
    assert_eq!(
        source.metadata.processing_family().unwrap(),
        crate::source::ProcessingFamily::Classic
    );
    let p = prepare(&source.metadata, "Sunlight").unwrap();
    let gpu = crate::gpu::Gpu::new().unwrap();
    let input = gpu
        .upload("Original Classic layers", &source.pixels)
        .unwrap();
    let output = apply_correction(&gpu, &input, &p.correction, &mut Default::default()).unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
    let expected = std::fs::read(std::env::var("CLASSIC_CORRECTION_REFERENCE").unwrap()).unwrap();
    assert_eq!(actual.len() * 2, expected.len());
    let mut differences = 0usize;
    let mut first = Vec::new();
    for (i, (a, b)) in actual
        .iter()
        .zip(expected.as_chunks::<2>().0.iter())
        .enumerate()
    {
        let b = i16::from_le_bytes([b[0], b[1]]) as f32;
        if *a != b {
            differences += 1;
            if first.len() < 16 {
                first.push((i, *a, b));
            }
        }
    }
    assert_eq!(differences, 0, "first differences: {first:?}");
}

#[test]
fn inverse_preserves_nondiagonal_camera_matrix() {
    let matrix = [0.7, 0.1, 0.2, 0.2, 0.8, 0.1, 0.1, 0.2, 0.9];
    let product = multiply(matrix, inverse(matrix).unwrap());
    for (i, v) in product.iter().enumerate() {
        assert!((v - if i / 3 == i % 3 { 1. } else { 0. }).abs() < 1e-12);
    }
    assert!(inverse([0.; 9]).is_err());
}

#[test]
#[ignore = "requires CLASSIC_X3F and CLASSIC_COLOR_REFERENCE original-file fixtures"]
fn actual_color_rows_match_reference() {
    let source = crate::source::Source::open(std::env::var("CLASSIC_X3F").unwrap()).unwrap();
    let prepared = prepare(&source.metadata, "Sunlight").unwrap();
    let mut p = prepare_color(&source.metadata, &prepared.color, "Standard").unwrap();
    let gpu = crate::gpu::Gpu::new().unwrap();
    let directory = std::path::PathBuf::from(std::env::var("CLASSIC_COLOR_REFERENCE").unwrap());
    let native_matrix = std::fs::read(directory.join("row-0000-matrix.bin")).unwrap();
    let matrix = bytemuck::cast_slice::<u8, i32>(&native_matrix);
    let ours: Vec<i32> = p
        .transform
        .params
        .matrix
        .iter()
        .flat_map(|r| r[..3].iter().copied())
        .chain(std::iter::once(p.transform.params.source[3] as i32))
        .collect();
    assert_eq!(ours, matrix);
    for (c, name) in ['r', 'g', 'b'].into_iter().enumerate() {
        let bytes = std::fs::read(directory.join(format!("color-00-lut-{name}.bin"))).unwrap();
        let expected: Vec<i32> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as i32)
            .collect();
        let extent = p.transform.params.source[2] as usize;
        assert_eq!(&p.transform.tables[c * extent..(c + 1) * extent], expected);
    }
    p.transform.params.source[1] = 1;
    for row in [0, 200, 900] {
        let before = std::fs::read(directory.join(format!("row-{row:04}-before.bin"))).unwrap();
        let input: Vec<f32> = before
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32)
            .collect();
        let input = gpu.upload("Captured color row", &input).unwrap();
        let (output, layers) =
            apply_transform(&gpu, &input, &p.transform, &mut Default::default()).unwrap();
        let layer_bytes = gpu.readback(&layers).unwrap();
        let layer_values = bytemuck::cast_slice::<u8, f32>(&layer_bytes);
        let layer_expected = std::fs::read(directory.join(format!("row-{row:04}-x3.bin"))).unwrap();
        for (i, (a, b)) in layer_values
            .iter()
            .zip(layer_expected.as_chunks::<2>().0.iter())
            .enumerate()
        {
            assert_eq!(
                *a,
                i16::from_le_bytes([b[0], b[1]]) as f32,
                "X3 row {row} component {i}"
            );
        }
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let expected = std::fs::read(directory.join(format!("row-{row:04}-after.bin"))).unwrap();
        for (i, (a, b)) in actual
            .iter()
            .zip(expected.as_chunks::<2>().0.iter())
            .enumerate()
        {
            assert_eq!(
                *a,
                i16::from_le_bytes([b[0], b[1]]) as f32,
                "row {row} component {i}"
            );
        }
    }
}

#[test]
#[ignore = "requires CLASSIC_X3F and CLASSIC_DECODED_REFERENCE original-file fixtures"]
fn original_decoded_samples_and_presets_match_reference() {
    let path = std::env::var("CLASSIC_X3F").unwrap();
    let source = crate::source::Source::open(&path).unwrap();
    let reference = std::fs::read(std::env::var("CLASSIC_DECODED_REFERENCE").unwrap()).unwrap();
    assert_eq!(bytemuck::cast_slice::<f32, u8>(&source.pixels), reference);
    assert_eq!(
        crate::source::read_capture_properties(path).unwrap(),
        source.metadata.properties
    );
    let wb = source.metadata.recorded_white_balance().unwrap();
    let mode = source.metadata.recorded_color_mode().unwrap();
    let prepared = prepare(&source.metadata, &wb).unwrap();
    prepare_color(&source.metadata, &prepared.color, &mode).unwrap();
}

#[test]
#[ignore = "requires CLASSIC_X3F original-file metadata"]
fn calibration_preparation_defers_color_metadata() {
    let source = crate::source::Source::open(std::env::var("CLASSIC_X3F").unwrap()).unwrap();
    let original = prepare(&source.metadata, "Sunlight").unwrap();
    let mut metadata = (*source.metadata).clone();
    metadata.lists.remove("ColorModeCompensations");
    metadata.lists.remove("ColorModeContrastCompensations");
    metadata
        .matrices
        .get_mut("ColorDQCamRGB")
        .unwrap()
        .values
        .truncate(1);
    let without_appearance = prepare(&metadata, "Sunlight").unwrap();
    assert_eq!(
        bytemuck::bytes_of(&original.correction.params),
        bytemuck::bytes_of(&without_appearance.correction.params)
    );
    assert_eq!(
        original.correction.knots,
        without_appearance.correction.knots
    );
    for mode in ["", "unavailable"] {
        assert!(prepare_color(&metadata, &without_appearance.color, mode).is_err());
    }
    assert!(
        prepare_color(&metadata, &without_appearance.color, "Standard").is_err(),
        "malformed color noise must be validated at the color stage"
    );
}
