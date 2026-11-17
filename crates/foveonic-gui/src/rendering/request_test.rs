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
        auxiliary_preview: false,

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

#[test]
fn auxiliary_preview_bounds_processing_without_changing_authored_output_size() {
    let mut request = request();
    request.auxiliary_preview = true;
    request.preview_scale = Scale::new(8).unwrap();
    request.processing_quality = ProcessingQuality::Speed;
    request.settings.output_size = Some([6000, 4000]);
    request.settings.preview_region = Some([100, 100, 400, 300]);
    let mut effective = request.settings.clone();
    request.apply_preview(&mut effective, &metadata()).unwrap();
    assert_eq!(request.purpose(), Purpose::Preview);
    assert!(request.uses_reduced_session());
    assert_eq!(effective.working_scale, Scale::new(8).unwrap());
    assert_eq!(effective.processing_quality, ProcessingQuality::Speed);
    assert_eq!(effective.output_size, None);
    assert_eq!(effective.preview_region, None);
    assert_eq!(request.settings.output_size, Some([6000, 4000]));
}

#[test]
fn native_coverage_reuses_overscan_until_visible_pixels_leave_the_patch() {
    let mut first = request();
    first.detail = true;
    first.preview_scale = Scale::NATIVE;
    first.settings.preview_region = Some([36, 36, 464, 364]);
    let existing = Refinement {
        first,
        visible_region: Some([100, 100, 400, 300]),
        preview: None,
        whole: None,
    };
    let mut pan = existing.clone();
    pan.first.revision += 1;
    pan.first.settings.preview_region = Some([100, 100, 528, 428]);
    pan.visible_region = Some([164, 164, 464, 364]);
    assert!(existing.same_sequence(&pan));
    assert!(
        existing.covers(&pan),
        "changed overscan is not uncovered visible demand"
    );
    pan.visible_region = Some([165, 164, 465, 364]);
    assert!(
        !existing.covers(&pan),
        "one uncovered visible column needs native detail"
    );
    pan.visible_region = Some([0, 0, 300, 200]);
    assert!(!existing.covers(&pan), "coverage must hold on every edge");
    pan.first.settings.exposure += 0.5;
    assert!(!existing.same_sequence(&pan));
    let mut whole = existing;
    whole.first.settings.preview_region = None;
    assert!(whole.covers(&pan));
}

#[test]
fn whole_before_refinement_tracks_increased_sampling_demand() {
    let mut first = request();
    first.rendition = crate::editor::Rendition::Camera;
    first.preview_scale = Scale::new(8).unwrap();
    let reduced = Refinement {
        first,
        visible_region: None,
        preview: None,
        whole: None,
    };
    let mut finer = reduced.clone();
    finer.first.preview_scale = Scale::new(4).unwrap();
    assert!(!reduced.same_sequence(&finer));
    assert!(reduced.same_sequence(&reduced.clone()));
}
