//! Checks request purpose and working geometry without worker threads, source I/O or a GPU.

use super::*;
use crate::export::ExportFormat;
use foveonic::render::ProcessingQuality;

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

fn request() -> Request {
    Request {
        detail: false,
        framing_preview: false,
        calibration: None,
        rendition: Default::default(),
        revision: 1,
        path: "camera.x3f".into(),
        settings: Settings::new("Sunlight", "Standard"),
        preview_scale: Scale::new(2).unwrap(),
        processing_quality: ProcessingQuality::Quality,
        output: "final".into(),
        comparison: false,
        export: None,
    }
}

#[test]
fn preview_scale_and_processing_policy_resolve_independently() {
    for policy in [ProcessingQuality::Quality, ProcessingQuality::Speed] {
        for divisor in [1, 2, 4, 8] {
            let mut request = request();
            request.preview_scale = Scale::new(divisor).unwrap();
            request.processing_quality = policy;
            let mut settings = request.settings.clone();
            request.apply_preview(&mut settings, &metadata()).unwrap();
            assert_eq!(settings.working_scale, request.preview_scale);
            assert_eq!(settings.processing_quality, policy);
            assert!(super::super::matching::same_authored_edits(
                &settings,
                &request.settings
            ));
            assert_eq!(request.uses_reduced_session(), divisor != 1);
        }
    }
}

#[test]
fn native_detail_and_whole_continuation_preserve_selected_policy() {
    for policy in [ProcessingQuality::Quality, ProcessingQuality::Speed] {
        let mut first = request();
        first.processing_quality = policy;
        first.detail = true;
        first.settings.preview_region = Some([100, 100, 400, 300]);
        let mut settings = first.settings.clone();
        first.apply_preview(&mut settings, &metadata()).unwrap();
        assert_eq!(settings.working_scale, Scale::NATIVE);
        assert_eq!(settings.processing_quality, policy);
        assert_eq!(settings.output_size, None);
        assert!(!first.uses_reduced_session());
        let mut whole = first.clone();
        whole.detail = false;
        whole.preview_scale = Scale::NATIVE;
        whole.settings.preview_region = None;
        whole.apply_preview(&mut settings, &metadata()).unwrap();
        assert_eq!(settings.working_scale, Scale::NATIVE);
        assert_eq!(settings.processing_quality, policy);
    }
}

#[test]
fn inspection_and_comparison_use_native_quality() {
    for comparison in [false, true] {
        let mut request = request();
        request.processing_quality = ProcessingQuality::Speed;
        request.comparison = comparison;
        if !comparison {
            request.output = "sensor".into();
        }
        let mut settings = request.settings.clone();
        request.apply_preview(&mut settings, &metadata()).unwrap();
        assert_eq!(settings.working_scale, Scale::NATIVE);
        assert_eq!(settings.processing_quality, ProcessingQuality::Quality);
        assert_eq!(settings.preview_region, None);
        assert!(!request.uses_reduced_session());
    }
}

#[test]
fn both_policies_accept_completed_finer_sampling() {
    for policy in [ProcessingQuality::Quality, ProcessingQuality::Speed] {
        let mut request = request();
        request.processing_quality = policy;
        request.preview_scale = Scale::new(4).unwrap();
        assert!(request.accepts_scale(Scale::NATIVE, &metadata()).unwrap());
        assert!(request
            .accepts_scale(Scale::new(4).unwrap(), &metadata())
            .unwrap());
        assert!(!request
            .accepts_scale(Scale::new(8).unwrap(), &metadata())
            .unwrap());
    }
}

#[test]
fn camera_specific_smallest_preview_scale_remains_numeric() {
    let mut metadata = metadata();
    metadata.processing_version = None;
    metadata
        .properties
        .insert("CAMMODEL".into(), "SIGMA SD14".into());
    assert_eq!(
        camera_preview_scale(Scale::new(8).unwrap(), &metadata)
            .unwrap()
            .divisor(),
        4
    );
    assert_eq!(
        camera_preview_scale(Scale::new(2).unwrap(), &metadata)
            .unwrap()
            .divisor(),
        2
    );
}

#[test]
fn before_renditions_preserve_policy_and_resolved_scale() {
    for rendition in [Rendition::Camera, Rendition::Defaults] {
        let mut request = request();
        request.rendition = rendition;
        request.processing_quality = ProcessingQuality::Speed;
        request.preview_scale = Scale::new(4).unwrap();
        assert_eq!(request.nominal_working_scale(), Scale::new(4).unwrap());
        assert!(request.uses_reduced_session());
    }
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
fn output_resize_and_photographic_options_preserve_resolved_preview_grid() {
    let mut request = request();
    request.settings.output_size = Some([640, 480]);
    request.settings.rendering = foveonic::render::Rendering::Monochrome;
    request.settings.dehaze.amount = 35.;
    request.settings.photographic.tone_method = foveonic::render::PhotographicMethod::Universal;
    request.settings.photographic.base_response = foveonic::render::BaseResponse::Strong;
    for divisor in [2, 4, 8] {
        request.preview_scale = Scale::new(divisor).unwrap();
        assert!(request.uses_reduced_session());
        assert_eq!(request.nominal_working_scale(), request.preview_scale);
    }
    request.detail = true;
    let mut settings = request.settings.clone();
    request.apply_preview(&mut settings, &metadata()).unwrap();
    assert_eq!(settings.output_size, None);
    assert_eq!(settings.working_scale, Scale::NATIVE);
}
