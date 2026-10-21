//! Checks source decoding and fixed-point correction against optional original-file reference boundaries.

use super::*;
use crate::nodes::sensor::fixed_point::{apply_correction, apply_transform};

// Some retained diagnostic requests used an explicit exposure instead of PROP.
fn reference_source() -> crate::source::Source {
    let mut source = crate::source::Source::open(std::env::var("CLASSIC_X3F").unwrap()).unwrap();
    if let Ok(exposure) = std::env::var("CLASSIC_REFERENCE_EXPOSURE") {
        std::sync::Arc::make_mut(&mut source.metadata)
            .properties
            .insert("EXPTIME".into(), exposure);
    }
    source
}

#[test]
#[ignore = "requires CLASSIC_X3F and CLASSIC_CORRECTION_REFERENCE original-file fixtures"]
fn complete_original_correction_matches_reference() {
    let source = reference_source();
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
    let p = prepare(
        &source.metadata,
        &std::env::var("CLASSIC_WHITE_BALANCE").unwrap_or_else(|_| "Sunlight".into()),
    )
    .unwrap();
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
    let mut maximum = 0_f32;
    let mut first = Vec::new();
    for (i, (a, b)) in actual
        .iter()
        .zip(expected.as_chunks::<2>().0.iter())
        .enumerate()
    {
        let b = i16::from_le_bytes([b[0], b[1]]) as f32;
        if *a != b {
            differences += 1;
            maximum = maximum.max((*a - b).abs());
            if first.len() < 16 {
                first.push((i, *a, b));
            }
        }
    }
    eprintln!("Complete correction: changed={differences}, max={maximum}");
    let maximum_allowed = std::env::var("CLASSIC_CORRECTION_MAX_DIFFERENCE")
        .map_or(0., |v| v.parse::<f32>().unwrap());
    let changed_allowed = std::env::var("CLASSIC_CORRECTION_MAX_CHANGED")
        .map_or(if maximum_allowed == 0. { 0 } else { 7 }, |v| {
            v.parse::<usize>().unwrap()
        });
    assert!(
        differences <= changed_allowed && maximum <= maximum_allowed,
        "correction exceeds supplied precision budget, first differences: {first:?}"
    );
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
    let source = reference_source();
    let prepared = prepare(
        &source.metadata,
        &std::env::var("CLASSIC_WHITE_BALANCE").unwrap_or_else(|_| "Sunlight".into()),
    )
    .unwrap();
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
    if reference.len() == source.pixels.len() * 2 {
        assert!(
            source
                .pixels
                .iter()
                .zip(reference.as_chunks::<2>().0)
                .all(|(actual, bytes)| *actual == f32::from(i16::from_le_bytes(*bytes))),
            "decoded signed sensor samples differ"
        );
    } else {
        assert_eq!(
            reference.len(),
            source.pixels.len() * 4,
            "expected signed16 or float32 reference samples"
        );
        assert!(
            bytemuck::cast_slice::<f32, u8>(&source.pixels) == reference,
            "decoded float sensor samples differ"
        );
    }
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
    let source = reference_source();
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

#[test]
#[ignore = "requires CLASSIC_X3F and CLASSIC_COLOR_REFERENCE original-file fixtures"]
fn camera_calibration_matches_reference_matrix() {
    let source = reference_source();
    let prepared = prepare(
        &source.metadata,
        &std::env::var("CLASSIC_WHITE_BALANCE").unwrap_or_else(|_| "Sunlight".into()),
    )
    .unwrap();
    let camera = selected_camera(&source.metadata, prepared.color.camera, "Standard").unwrap();
    let directory = std::path::PathBuf::from(std::env::var("CLASSIC_COLOR_REFERENCE").unwrap());
    let expected = std::fs::read(directory.join("matrix-00-camera.bin")).unwrap();
    for (actual, bytes) in camera.iter().zip(expected.as_chunks::<8>().0) {
        let reference = f64::from_le_bytes(*bytes);
        assert!(
            (actual - reference).abs() < 1e-12,
            "camera coefficient {actual} != {reference}"
        );
    }
}

#[test]
fn kept_sensor_origin_and_rotation_define_visible_extent() {
    use crate::source::{Matrix, MatrixEncoding, Metadata};
    let mut metadata = Metadata {
        width: 2304,
        height: 1531,
        rotation: 0,
        properties: Default::default(),
        lists: Default::default(),
        matrices: [
            ("KeepImageArea", [0., 4., 2303., 1534.]),
            ("ActiveImageArea", [18., 12., 2285., 1523.]),
        ]
        .into_iter()
        .map(|(name, values)| {
            (
                name.into(),
                Matrix {
                    encoding: MatrixEncoding::Int32,
                    representation: "native",
                    dimensions: vec![4],
                    values: values.to_vec(),
                },
            )
        })
        .collect(),
    };
    let output = geometry(&metadata).unwrap();
    assert_eq!(output.crop, [18, 8, 2286, 1520]);
    assert_eq!(&output.output[..2], &[2268, 1512]);
    metadata.rotation = 90;
    assert_eq!(&geometry(&metadata).unwrap().output[..2], &[1512, 2268]);
    metadata.matrices.get_mut("ActiveImageArea").unwrap().values[3] = 1540.;
    assert!(geometry(&metadata).is_err());
}

#[test]
fn sd10_matched_green_filter_is_reduced_before_quadratic_fallback() {
    use crate::source::{Matrix, MatrixEncoding};
    let matrix = |values: Vec<f64>| Matrix {
        encoding: MatrixEncoding::Float32,
        representation: "native",
        dimensions: vec![values.len()],
        values,
    };
    let mut metadata = Metadata {
        width: 2304,
        height: 1531,
        rotation: 0,
        properties: [("CAMMODEL".into(), "SIGMA SD10".into())].into(),
        matrices: [("ColumnFilter".into(), matrix(vec![0.8]))].into(),
        lists: Default::default(),
    };
    let expected = [0.8, 0.8 * 0.94, 0.8];
    assert_eq!(
        prepare_column_filters(&metadata).unwrap(),
        (expected, expected)
    );
    metadata
        .matrices
        .insert("ColumnFilterRGB".into(), matrix(vec![0.8, 0.7, 0.6]));
    assert_eq!(
        prepare_column_filters(&metadata).unwrap(),
        ([0.8, 0.7, 0.6], [0.8, 0.7, 0.6])
    );
    metadata
        .matrices
        .insert("ColumnFilterRGB".into(), matrix(vec![0.8; 3]));
    metadata
        .matrices
        .insert("ColumnFilterSquareRGB".into(), matrix(vec![0.1; 3]));
    assert_eq!(
        prepare_column_filters(&metadata).unwrap(),
        (expected, [0.1; 3])
    );
    metadata
        .properties
        .insert("CAMMODEL".into(), "SIGMA SD9".into());
    assert_eq!(
        prepare_column_filters(&metadata).unwrap(),
        ([0.8; 3], [0.1; 3])
    );
}

#[test]
fn despeckle_compares_recorded_exposure_in_microseconds() {
    use crate::source::{Matrix, MatrixEncoding};
    for (model, units) in [
        ("SIGMA SD9", 1.),
        ("SIGMA SD10", 1.),
        ("SIGMA SD14", 1_000_000.),
    ] {
        for (seconds, old) in [(0.15, false), (0.2, true)] {
            let mut metadata = Metadata {
                width: 16,
                height: 16,
                rotation: 0,
                properties: [("CAMMODEL".into(), model.into())].into(),
                matrices: [(
                    "CaptureShutter".into(),
                    Matrix {
                        dimensions: vec![1],
                        representation: "native",
                        values: vec![seconds * units],
                        encoding: MatrixEncoding::Float32,
                    },
                )]
                .into(),
                lists: Default::default(),
            };
            let expected = signed_repair::Despeckle::new([16, 16], old);
            assert!(
                prepare_despeckle(&metadata).unwrap().key() == expected.key(),
                "{model} {seconds}"
            );
            // Stage2's recorded request exposure takes precedence over capture metadata.
            metadata
                .properties
                .insert("EXPTIME".into(), "250000".into());
            assert!(
                prepare_despeckle(&metadata).unwrap().key()
                    == signed_repair::Despeckle::new([16, 16], true).key()
            );
        }
    }
}

#[test]
fn direct_matrix_preserves_gain_without_reconstruction() {
    let camera = [0.3, 0.6, 0.3, 0.3, 0.6, 0.3, 0.3, 0.6, 0.3];
    let (unit, green) = transform(32, 32, camera, [4.; 3], 1., MatrixScale::UnitGreen).unwrap();
    let (direct, direct_green) =
        transform(32, 32, camera, [4.; 3], 1., MatrixScale::PreserveGain).unwrap();
    assert_eq!(
        unit.output_scale,
        crate::nodes::sensor::fixed_point::CodeScale::Normalized
    );
    assert_eq!(
        direct.output_scale,
        crate::nodes::sensor::fixed_point::CodeScale::Ready
    );
    assert_eq!(green, direct_green);
    assert_eq!(unit.tables, direct.tables);
    let sum = |p: &Transform| p.params.matrix[1][..3].iter().sum::<i32>() as f32 / Q14_SCALE as f32;
    assert!((sum(&unit) - 1.).abs() < 0.0002);
    assert!((sum(&direct) - green).abs() < 0.0002);
    let (digital, _) = transform(32, 32, camera, [4.; 3], 2., MatrixScale::PreserveGain).unwrap();
    let value = digital.params.matrix[1][..3].iter().sum::<i32>() as f32
        / (1u32 << digital.params.source[3]) as f32;
    assert!((value - green * 2.).abs() < 0.0004);
}

#[test]
fn disabled_fir_keeps_native_coefficients_and_marks_direct_output() {
    use crate::{
        nodes::sensor::fixed_point::CodeScale,
        source::{Matrix, MatrixEncoding},
    };
    let mut metadata = Metadata {
        width: 32,
        height: 32,
        rotation: 0,
        properties: Default::default(),
        lists: Default::default(),
        matrices: Default::default(),
    };
    let input = ColorInput {
        monochrome: false,
        camera: [0.7, 0.1, 0.2, 0.2, 0.8, 0.1, 0.1, 0.2, 0.9],
        final_gain: [1.; 3],
        relative_gain: [1.; 3],
        ratio: 1.,
        digital_gain: 1.,
        extent: [32, 32],
        statistics_factor: 1.,
        alternate_matrix: None,
    };
    let reconstructed = prepare_unselected_color(&metadata, &input).unwrap();
    metadata.matrices.insert(
        "FIR_DisableStep".into(),
        Matrix {
            encoding: MatrixEncoding::Int32,
            representation: "native",
            dimensions: vec![1],
            values: vec![1.],
        },
    );
    let direct = prepare_unselected_color(&metadata, &input).unwrap();
    assert_eq!(reconstructed.transform.output_scale, CodeScale::Normalized);
    assert_eq!(direct.transform.output_scale, CodeScale::Ready);
    assert_eq!(
        direct.transform.params.matrix,
        reconstructed.transform.params.matrix
    );
    assert_eq!(direct.transform.tables, reconstructed.transform.tables);
    assert!(
        prepare_reconstruction_gain(&metadata, direct.matrix_green_sum, direct.digital_gain)
            .unwrap()
            .is_none()
    );
}

#[test]
fn monochrome_fallback_builds_rank_one_flash_mixture() {
    use crate::source::{Matrix, MatrixEncoding};
    let flash = [0.7, 0.1, 0.2, 0.2, 0.8, 0.1, 0.1, 0.2, 0.9];
    let metadata = Metadata {
        width: 32,
        height: 32,
        rotation: 0,
        properties: Default::default(),
        lists: Default::default(),
        matrices: [(
            "CamToXYZ_Flash".into(),
            Matrix {
                encoding: MatrixEncoding::Float32,
                representation: "native",
                dimensions: vec![3, 3],
                values: flash.to_vec(),
            },
        )]
        .into(),
    };
    let camera = monochrome_camera(&metadata).unwrap();
    assert_eq!(&camera[..3], &camera[3..6]);
    assert_eq!(&camera[..3], &camera[6..]);
    assert_eq!(camera[0], camera[2]);
    assert_eq!(camera[1], camera[0] * 2.);
    let white = response(flash).unwrap();
    let maximum = white.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let expected = flash
        .iter()
        .enumerate()
        .map(|(i, v)| v * white[i % 3] / maximum)
        .sum::<f64>()
        / 9.;
    assert!((camera[0] - expected).abs() < 1e-15);
}

#[test]
#[ignore = "requires CLASSIC_X3F DP2 original"]
fn monochrome_color_mode_changes_sensor_mixture_like_reference() {
    let source = reference_source();
    let prepared = prepare(&source.metadata, "Monochrome").unwrap();
    let camera = selected_camera(&source.metadata, prepared.color.camera, "Vivid").unwrap();
    let expected = [0.33934799727348997, 0.44454811125588006, 0.4565293758892253];
    for row in camera.as_chunks::<3>().0 {
        for (actual, expected) in row.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
        }
    }
}

#[test]
fn monochrome_chroma_keeps_only_doubled_small_support() {
    use crate::source::{Matrix, MatrixEncoding};
    let metadata = Metadata {
        width: 32,
        height: 32,
        rotation: 0,
        properties: [("ISO".into(), "100".into())].into(),
        lists: Default::default(),
        matrices: [
            ("CCMatrix_Flag", vec![1.]),
            ("ChromaBlur", vec![1.]),
            ("ChromaDQ", vec![4.; 3]),
        ]
        .into_iter()
        .map(|(name, values)| {
            (
                name.into(),
                Matrix {
                    encoding: MatrixEncoding::Float32,
                    representation: "native",
                    dimensions: vec![values.len()],
                    values,
                },
            )
        })
        .collect(),
    };
    let color = prepare_chroma(&metadata, [1.; 3], false).unwrap();
    let mono = prepare_chroma(&metadata, [1.; 3], true).unwrap();
    assert_eq!(color.len(), 2);
    assert_eq!(mono.len(), 1);
    let expected =
        chroma::Prepared::new([32, 32], [16.; 3], [0.8; 3], 0, 3, Support::Binomial3).unwrap();
    assert_eq!(
        bytemuck::bytes_of(&mono[0].params),
        bytemuck::bytes_of(&expected.params)
    );
    assert_eq!(mono[0].tables, expected.tables);
}

#[test]
fn selected_mode_contrast_uses_shared_photographic_response() {
    use crate::source::{Matrix, MatrixEncoding, PropertyList};
    let metadata = Metadata {
        width: 32,
        height: 32,
        rotation: 0,
        properties: [("ISO".into(), "100".into())].into(),
        matrices: [(
            "VividContrast".into(),
            Matrix {
                encoding: MatrixEncoding::Float32,
                representation: "native",
                dimensions: vec![1],
                values: vec![f64::from(0.3_f32)],
            },
        )]
        .into(),
        lists: [(
            "ColorModeContrastCompensations".into(),
            PropertyList::new(vec![("Vivid".into(), "VividContrast".into())]).unwrap(),
        )]
        .into(),
    };
    for rendering in [
        crate::render::Rendering::Color,
        crate::render::Rendering::Monochrome,
    ] {
        let mut settings = Settings::new("Sunlight", "Vivid");
        settings.sharpen_amount = Some(1.);
        settings.rendering = rendering;
        let selected = prepare_photographic(&metadata, &settings).unwrap();
        settings.color_mode = "Standard".into();
        settings.tone.contrast = f64::from(0.3_f32);
        let adjusted = prepare_photographic(&metadata, &settings).unwrap();
        assert_eq!(selected, adjusted);
    }
}
