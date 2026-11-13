//! Checks request purpose and working geometry without worker threads, source I/O or a GPU.

use super::*;
use crate::export::ExportFormat;

fn metadata() -> Metadata {
    use foveonic::source::{Matrix, MatrixEncoding};
    Metadata {
        width: 5888,
        height: 3776,
        rotation: 0,
        processing_version: Some([3, 1, 2, 0]),
        properties: [("CAMMODEL".into(), "SIGMA sd Quattro".into())].into(),
        matrices: [("CAMERAID", 40.), ("CaptureISO", 100.)]
            .into_iter()
            .map(|(k, v)| {
                (
                    k.into(),
                    Matrix {
                        encoding: MatrixEncoding::Float32,
                        representation: "test",
                        dimensions: vec![1],
                        values: vec![v],
                    },
                )
            })
            .collect(),
        lists: Default::default(),
    }
}
#[test]
fn merrill_smart_uses_fast_overview_and_regional_full_without_changing_edits() {
    use foveonic::source::{Matrix, MatrixEncoding};
    let mut metadata = metadata();
    metadata.width = 4928;
    metadata.height = 3264;
    metadata.processing_version = None;
    metadata
        .properties
        .insert("CAMMODEL".into(), "SIGMA DP2 Merrill".into());
    metadata.matrices.insert(
        "ActiveImageArea".into(),
        Matrix {
            encoding: MatrixEncoding::Uint32,
            representation: "test",
            dimensions: vec![4],
            values: vec![112., 64., 4815., 3199.],
        },
    );
    let mut request = Request {
        detail: false,
        framing_preview: false,
        calibration: None,
        rendition: Default::default(),
        revision: 1,
        path: "merrill.x3f".into(),
        settings: Settings::new("Sunlight", "Standard"),
        preview_quality: PreviewMode::Fast,
        preview_policy: PreviewMode::Smart,
        output: "final".into(),
        comparison: false,
        export: None,
    };
    request.settings.dehaze.amount = 35.;
    let mut resolved = request.settings.clone();
    request.apply_preview(&mut resolved, &metadata).unwrap();
    assert_eq!(resolved.working_scale, Scale::new(2).unwrap());
    assert_eq!(resolved.preview_region, None);
    assert!(resolved.row_correction && resolved.despeckle);
    for (mode, quality) in [
        (PreviewMode::Draft, Scale::new(4).unwrap()),
        (PreviewMode::Thumb, Scale::new(8).unwrap()),
    ] {
        request.preview_quality = mode;
        request.apply_preview(&mut resolved, &metadata).unwrap();
        assert_eq!(resolved.working_scale, quality);
    }
    request.preview_quality = PreviewMode::Fast;
    request.apply_preview(&mut resolved, &metadata).unwrap();
    assert_eq!(resolved.working_scale, Scale::new(2).unwrap());
    assert!(crate::rendering::matching::same_authored_edits(
        &resolved,
        &request.settings
    ));
    request.settings.working_scale = Scale::new(4).unwrap();
    request.preview_quality = PreviewMode::Full;
    request.apply_preview(&mut resolved, &metadata).unwrap();
    assert_eq!(resolved.working_scale, Scale::NATIVE);
    request.preview_quality = PreviewMode::Smart;
    request.detail = true;
    request.settings.preview_region = Some([100, 100, 400, 300]);
    resolved = request.settings.clone();
    request.apply_preview(&mut resolved, &metadata).unwrap();
    assert_eq!(resolved.working_scale, Scale::NATIVE);
    assert_eq!(
        resolved.preview_region, request.settings.preview_region,
        "Merrill must preserve the requested native detail region"
    );
    // A loupe requests Full detail without changing the workspace's fixed preview policy.
    for mode in [
        PreviewMode::Full,
        PreviewMode::Fast,
        PreviewMode::Draft,
        PreviewMode::Thumb,
    ] {
        request.preview_policy = mode;
        request.preview_quality = PreviewMode::Full;
        resolved = request.settings.clone();
        request.apply_preview(&mut resolved, &metadata).unwrap();
        assert_eq!(request.purpose(), Purpose::Detail);
        assert_eq!(resolved.working_scale, Scale::NATIVE);
        assert_eq!(resolved.preview_region, request.settings.preview_region);
    }
    request.settings.output_size = Some([640, 480]);
    resolved = request.settings.clone();
    request.apply_preview(&mut resolved, &metadata).unwrap();
    assert_eq!(
        resolved.output_size, None,
        "native detail must bypass final output sizing"
    );
    request.settings.output_size = None;
    for format in [ExportFormat::Tiff, ExportFormat::Dng] {
        request.export = Some(Export {
            path: "export.output".into(),
            format,
            tiff_color_space: None,
            encoding: Default::default(),
            sections: Default::default(),
            conflict: foveonic::importing::ConflictPolicy::Rename,
        });
        request.apply_preview(&mut resolved, &metadata).unwrap();
        assert_eq!(resolved.working_scale, Scale::NATIVE);
        assert_eq!(resolved.preview_region, None);
    }
}
#[test]
fn classic_thumb_resolves_the_entire_preview_to_draft_resolution() {
    let mut request = Request {
        detail: false,
        framing_preview: false,
        calibration: None,
        rendition: Default::default(),
        revision: 1,
        path: "preview.x3f".into(),
        settings: Settings::new("Sunlight", "Standard"),
        preview_quality: PreviewMode::Thumb,
        preview_policy: PreviewMode::Thumb,
        output: "final".into(),
        comparison: false,
        export: None,
    };
    for (model, divisor) in [
        ("SIGMA SD14", 4),
        ("SIGMA DP1", 4),
        ("SIGMA DP2 Merrill", 8),
        ("SIGMA dp2 Quattro", 8),
    ] {
        let mut metadata = metadata();
        metadata.processing_version = None;
        metadata.properties.insert("CAMMODEL".into(), model.into());
        request.preview_quality = PreviewMode::Thumb;
        let mut resolved = request.settings.clone();
        request.apply_preview(&mut resolved, &metadata).unwrap();
        assert_eq!(resolved.working_scale.divisor(), divisor, "{model}");
        request.preview_quality = PreviewMode::Draft;
        request.apply_preview(&mut resolved, &metadata).unwrap();
        assert_eq!(resolved.working_scale.divisor(), 4);
        request.preview_quality = PreviewMode::Thumb;
        request.output = "linear.base".into();
        request.apply_preview(&mut resolved, &metadata).unwrap();
        assert_eq!(resolved.working_scale, Scale::NATIVE);
        request.output = "final".into();
        request.comparison = true;
        request.apply_preview(&mut resolved, &metadata).unwrap();
        assert_eq!(resolved.working_scale, Scale::NATIVE);
        request.comparison = false;
    }
}

#[test]
fn numeric_modes_preserve_controls_and_native_purposes_are_explicit() {
    let mut r = Request {
        detail: false,
        framing_preview: false,
        calibration: None,
        rendition: Default::default(),
        revision: 1,
        path: "test.x3f".into(),
        settings: Settings::new("Sunlight", "Standard"),
        preview_quality: PreviewMode::Fast,
        preview_policy: PreviewMode::Fast,
        output: "final".into(),
        comparison: false,
        export: None,
    };
    r.settings.rendering = foveonic::render::Rendering::Monochrome;
    r.settings.dehaze.amount = 35.;
    r.settings.photographic.tone_method = foveonic::render::PhotographicMethod::Universal;
    r.settings.photographic.base_response = foveonic::render::BaseResponse::Strong;
    for mode in [
        PreviewMode::Full,
        PreviewMode::Fast,
        PreviewMode::Smart,
        PreviewMode::Draft,
        PreviewMode::Thumb,
    ] {
        r.preview_quality = mode;
        assert_eq!(r.nominal_working_scale(), mode.working_scale());
    }
    r.settings.output_size = Some([640, 480]);
    assert_eq!(r.nominal_working_scale(), Scale::new(8).unwrap());
    r.output = "sensor.rgb".into();
    assert_eq!(r.nominal_working_scale(), Scale::NATIVE);
    r.output = "final".into();
    r.comparison = true;
    assert_eq!(r.nominal_working_scale(), Scale::NATIVE);
    r.comparison = false;
    r.detail = true;
    r.settings.preview_region = Some([100, 100, 400, 300]);
    assert_eq!(r.nominal_working_scale(), Scale::NATIVE);
    r.detail = false;
    r.settings.preview_region = None;
    for format in [ExportFormat::Tiff, ExportFormat::Dng] {
        r.export = Some(Export {
            path: "test.output".into(),
            format,
            tiff_color_space: None,
            encoding: Default::default(),
            sections: Default::default(),
            conflict: foveonic::importing::ConflictPolicy::Rename,
        });
        assert_eq!(r.nominal_working_scale(), Scale::NATIVE);
    }
}

fn request() -> Request {
    Request {
        detail: false,
        framing_preview: false,
        calibration: None,
        rendition: Default::default(),
        revision: 1,
        path: "photo.x3f".into(),
        settings: Settings::new("Sunlight", "Standard"),
        preview_quality: PreviewMode::Thumb,
        preview_policy: PreviewMode::Thumb,
        output: "final".into(),
        comparison: false,
        export: None,
    }
}

#[test]
fn smart_concrete_levels_share_reduced_sessions_but_require_sufficient_pixels() {
    let mut request = request();
    request.preview_policy = PreviewMode::Smart;
    for quality in [PreviewMode::Thumb, PreviewMode::Draft, PreviewMode::Fast] {
        request.preview_quality = quality;
        assert!(request.uses_reduced_session());
        assert!(request.accepts_scale(Scale::NATIVE, &metadata()).unwrap());
        assert!(request
            .accepts_scale(quality.working_scale(), &metadata())
            .unwrap());
        let coarser = Scale::new(quality.working_scale().divisor() * 2).unwrap();
        assert!(!request.accepts_scale(coarser, &metadata()).unwrap());
    }
    request.preview_policy = PreviewMode::Fast;
    assert!(!request.accepts_scale(Scale::NATIVE, &metadata()).unwrap());
    request.detail = true;
    request.settings.preview_region = Some([100, 100, 400, 300]);
    request.preview_policy = PreviewMode::Smart;
    assert!(!request.uses_reduced_session());
    let mut settings = request.settings.clone();
    request.apply_preview(&mut settings, &metadata()).unwrap();
    assert_eq!(settings.working_scale, Scale::NATIVE);
    assert_eq!(settings.preview_region, request.settings.preview_region);
}

#[test]
fn consumer_purpose_preserves_export_before_inspection_and_detail_precedence() {
    let mut r = request();
    assert_eq!(r.purpose(), Purpose::Current);
    r.detail = true;
    r.settings.preview_region = Some([100, 100, 400, 300]);
    assert_eq!(r.purpose(), Purpose::Detail);
    r.comparison = true;
    assert_eq!(r.purpose(), Purpose::Inspection("final".into(), true));
    r.output = "sensor.rgb".into();
    r.comparison = false;
    assert_eq!(r.purpose(), Purpose::Inspection("sensor.rgb".into(), false));
    r.rendition = Rendition::Camera;
    assert_eq!(r.purpose(), Purpose::Camera);
    r.rendition = Rendition::Defaults;
    assert_eq!(r.purpose(), Purpose::Defaults);
    r.export = Some(Export {
        path: "export.output".into(),
        format: ExportFormat::Tiff,
        tiff_color_space: None,
        encoding: Default::default(),
        sections: Default::default(),
        conflict: foveonic::importing::ConflictPolicy::Rename,
    });
    assert_eq!(r.purpose(), Purpose::Export);
    assert!(!r.uses_reduced_session());
    assert_eq!(r.nominal_working_scale(), Scale::NATIVE);
}

#[test]
fn before_renditions_keep_the_requested_reduced_preview_policy() {
    let mut r = request();
    for (rendition, purpose) in [
        (Rendition::Current, Purpose::Current),
        (Rendition::Camera, Purpose::Camera),
        (Rendition::Defaults, Purpose::Defaults),
    ] {
        r.rendition = rendition;
        assert_eq!(r.purpose(), purpose);
        assert!(r.uses_reduced_session());
        assert_eq!(r.nominal_working_scale(), Scale::new(8).unwrap());
    }
    r.preview_quality = PreviewMode::Smart;
    assert!(!r.uses_reduced_session());
    assert_eq!(r.nominal_working_scale(), Scale::new(2).unwrap());
}

#[test]
fn only_explicit_small_final_requests_use_the_reduced_set() {
    let mut request = request();
    assert!(request.uses_reduced_session());
    request.preview_quality = PreviewMode::Draft;
    assert!(request.uses_reduced_session());
    for quality in [PreviewMode::Full, PreviewMode::Smart] {
        request.preview_quality = quality;
        assert!(!request.uses_reduced_session());
    }
    request.preview_quality = PreviewMode::Thumb;
    request.comparison = true;
    assert!(!request.uses_reduced_session());
    request.comparison = false;
    request.output = "linear.base".into();
    assert!(!request.uses_reduced_session());
}

#[test]
fn reduced_requests_have_no_camera_or_option_eligibility_gate() {
    let mut request = request();
    request.settings.rendering = foveonic::render::Rendering::Monochrome;
    request.settings.dehaze.amount = 35.;
    request.settings.photographic.tone_method = foveonic::render::PhotographicMethod::Universal;
    request.settings.photographic.base_response = foveonic::render::BaseResponse::Strong;
    for mode in [PreviewMode::Fast, PreviewMode::Draft, PreviewMode::Thumb] {
        request.preview_quality = mode;
        assert!(request.uses_reduced_session());
        assert_eq!(request.nominal_working_scale(), mode.working_scale());
    }
    request.preview_quality = PreviewMode::Smart;
    assert!(
        !request.uses_reduced_session(),
        "Smart retains native warm continuation"
    );
}
