//! Checks coarse warm-render headroom and native photograph capacity without GPU inputs.

use super::*;
use foveonic::{
    render::Scale,
    source::{Matrix, MatrixEncoding},
};

#[test]
fn four_retained_native_photographs_fit_four_gib_without_double_charging_inputs() {
    let actual = 11 * GIB / 4;
    let headroom = 4 * 256 * MIB;
    assert_eq!(actual + headroom, 15 * GIB / 4);
    assert!(fits(4 * GIB, actual, headroom, 4));
    assert!(!fits(3 * GIB, actual, headroom, 4));
    assert!(!fits(3 * GIB, GIB, headroom, 4));
}

#[test]
fn reduced_jobs_use_workspace_headroom_without_a_native_capacity_floor() {
    let hint = hints_for_grid([128, 128], false, ProcessingQuality::Speed).unwrap();
    assert_eq!(hint.bytes, 18 * MIB);
    assert!(!hint.native);
    assert!(fits(GIB, 512 * MIB, 8 * hint.bytes, 0));
    let actual = 700 * MIB;
    let headroom = 100 * MIB;
    assert!(fits(actual + headroom, actual, headroom, 0));
    assert!(!fits(actual + headroom - 1, actual, headroom, 0));
}

#[test]
fn native_workspace_scales_for_quality_area_and_long_narrow_regions() {
    let estimate = |extent, quality| hints_for_grid(extent, true, quality).unwrap().bytes;
    assert_eq!(estimate([1, 1], ProcessingQuality::Speed), 256 * MIB);
    assert_eq!(estimate([512, 512], ProcessingQuality::Quality), 384 * MIB);
    assert_eq!(estimate([1024, 1024], ProcessingQuality::Speed), GIB);
    assert_eq!(
        estimate([1024, 1024], ProcessingQuality::Quality),
        3 * GIB / 2
    );
    for extent in [[1, 2048], [2048, 1]] {
        assert_eq!(estimate(extent, ProcessingQuality::Speed), GIB);
    }
    assert_eq!(
        estimate([513, 1], ProcessingQuality::Speed),
        256 * MIB + MIB / 2
    );
    assert_eq!(ceil_scaled_bytes(1, 3, 2), Some(2));
}

#[test]
fn invalid_geometry_and_overflow_refuse_additional_work() {
    for native in [false, true] {
        for extent in [[0, 1], [1, 0], [u64::MAX, 2], [u64::MAX, 1]] {
            assert_eq!(
                hints_for_grid(extent, native, ProcessingQuality::Speed),
                None
            );
        }
    }
    assert!(!fits(u64::MAX, u64::MAX, 1, 0));
    if let Ok(native_count) = usize::try_from(u64::MAX / GIB + 1) {
        assert!(!fits(u64::MAX, 0, 0, native_count));
    }
}

#[test]
fn workspace_requires_known_merrill_geometry_and_supported_region_shape() {
    let mut metadata = Metadata {
        width: 4928,
        height: 3264,
        rotation: 0,
        processing_version: None,
        properties: Default::default(),
        matrices: Default::default(),
        lists: Default::default(),
    };
    let mut settings = Settings {
        preview_region: Some([10, 20, 522, 532]),
        processing_quality: ProcessingQuality::Speed,
        ..Default::default()
    };
    assert_eq!(workspace(&settings, &metadata), None);
    metadata
        .properties
        .insert("CAMMODEL".into(), "SIGMA DP1 Merrill".into());
    assert_eq!(workspace(&settings, &metadata), None);
    metadata.matrices.insert(
        "ActiveImageArea".into(),
        Matrix {
            encoding: MatrixEncoding::Uint32,
            representation: "u32",
            dimensions: vec![4],
            values: vec![0., 0., 1000., 1000.],
        },
    );
    assert_eq!(
        workspace(&settings, &metadata),
        Some(WorkspaceHint {
            bytes: 256 * MIB,
            native: true
        })
    );
    for region in [[10, 20, 9, 532], [10, 20, 522, 20]] {
        settings.preview_region = Some(region);
        assert_eq!(workspace(&settings, &metadata), None);
    }
    settings.preview_region = None;
    assert_eq!(workspace(&settings, &metadata), None);
    settings.working_scale = Scale::new(8).unwrap();
    // 1001 native pixels cover 126 reduced pixels along each axis.
    assert_eq!(
        workspace(&settings, &metadata),
        Some(WorkspaceHint {
            bytes: 16 * MIB + 126 * 126 * 16 * 8,
            native: false,
        })
    );
    settings.preview_region = Some([0, 0, 512, 512]);
    assert_eq!(workspace(&settings, &metadata), None);
}
