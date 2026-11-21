//! Separates working-resolution, application-guide and regional-execution differences on originals.

use super::*;
use crate::{
    image::RenderedImage, inspection::Content, profile::RenderSettingsSnapshot,
    render::ControlCapabilities, source::Source,
};
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

/// Save only the requested photographic area, with its exact sampling map.
/// Readback and patch extraction are diagnostics, outside processing timing.
fn save_patch(
    engine: &Engine,
    image: &RenderedImage,
    photo_extent: [u32; 2],
    rect: [u32; 4],
    path: &Path,
) -> serde_json::Value {
    let params = image.params();
    let [l, t, r, b] = params.crop.map(f64::from);
    let matrix = if params.mapping[0][3] != 0. {
        params
            .mapping
            .map(|row| [f64::from(row[0]), f64::from(row[1]), f64::from(row[2])])
    } else {
        match params.source[2] {
            0 => [[r - l, 0., l - 0.5], [0., b - t, t - 0.5]],
            90 => [[0., r - l, l - 0.5], [-(b - t), 0., b - 0.5]],
            180 => [[-(r - l), 0., r - 0.5], [0., -(b - t), b - 0.5]],
            270 => [[0., -(r - l), r - 0.5], [b - t, 0., t - 0.5]],
            _ => panic!("unsupported camera orientation"),
        }
    };
    let domain =
        image
            .settings()
            .preview_region
            .unwrap_or([0, 0, photo_extent[0], photo_extent[1]]);
    let size = [
        f64::from(domain[2] - domain[0]),
        f64::from(domain[3] - domain[1]),
    ];
    let mapping = matrix.map(|row| {
        [
            row[0] / size[0],
            row[1] / size[1],
            row[2]
                + row[0] * (0.5 - f64::from(domain[0])) / size[0]
                + row[1] * (0.5 - f64::from(domain[1])) / size[1],
        ]
    });
    let corners = [
        [rect[0], rect[1]],
        [rect[2] - 1, rect[1]],
        [rect[0], rect[3] - 1],
        [rect[2] - 1, rect[3] - 1],
    ]
    .map(|p| mapping.map(|row| row[0] * f64::from(p[0]) + row[1] * f64::from(p[1]) + row[2]));
    let storage = image.description().extent;
    let lower: [u32; 2] = std::array::from_fn(|a| {
        corners
            .iter()
            .map(|p| p[a])
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(f64::from(params.crop[a])) as u32
    });
    let upper: [u32; 2] = std::array::from_fn(|a| {
        ((corners
            .iter()
            .map(|p| p[a])
            .fold(f64::NEG_INFINITY, f64::max)
            .floor()
            + 2.) as u32)
            .min(params.crop[a + 2])
    });
    let extent = [upper[0] - lower[0], upper[1] - lower[1]];
    let bytes = engine.readback_image(image).unwrap();
    assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
        .iter()
        .all(|v| v.is_finite()));
    let mut output = File::create_new(path).unwrap();
    for y in lower[1]..upper[1] {
        let first = ((u64::from(y) * u64::from(storage[0]) + u64::from(lower[0])) * 12) as usize;
        output
            .write_all(&bytes[first..first + extent[0] as usize * 12])
            .unwrap();
    }
    let mapping: [[f64; 3]; 2] = std::array::from_fn(|axis| {
        [
            mapping[axis][0],
            mapping[axis][1],
            mapping[axis][2] - f64::from(lower[axis]),
        ]
    });
    serde_json::json!({"file":path.file_name().unwrap().to_str().unwrap(),"extent":extent,"native_to_patch":mapping,
        "space":format!("{:?}",image.description().space),"view_crop":image.view().crop,"view_extent":image.view().extent,"view_rotation":image.view().rotation,"storage_rect":[lower[0],lower[1],upper[0],upper[1]],"endianness":"little"})
}

#[test]
#[ignore = "requires GPU, FOVEONIC_FINISH_SOURCE, FOVEONIC_FINISH_SETTINGS snapshot and FOVEONIC_FINISH_OUTPUT new directory"]
fn original_adaptive_view_attribution() {
    capture_views(true);
}

#[test]
#[ignore = "requires GPU, FOVEONIC_FINISH_SOURCE, active Dynamic Range FOVEONIC_FINISH_SETTINGS snapshot and FOVEONIC_FINISH_OUTPUT new directory"]
fn original_dynamic_range_view_attribution() {
    capture_views(false);
}

fn capture_views(adaptive_variants: bool) {
    let source_path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap());
    let output = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FINISH_OUTPUT").unwrap());
    assert!(!output.exists(), "diagnostic outputs must be new");
    fs::create_dir_all(&output).unwrap();
    let snapshot: RenderSettingsSnapshot = serde_json::from_slice(
        &fs::read(std::env::var_os("FOVEONIC_FINISH_SETTINGS").unwrap()).unwrap(),
    )
    .unwrap();
    let source = Source::open(&source_path).unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut initial = snapshot.into_settings();
    assert!(
        initial.crop == Default::default()
            && initial.rotation.is_neutral()
            && initial.output_size.is_none()
    );
    if adaptive_variants {
        assert!(initial.adaptive_tone_active());
    } else {
        assert!(!initial.tonal_ranges.is_neutral());
    }
    assert!(initial.regional_preview_available(&source.metadata));
    if initial.white_balance.is_empty() {
        initial.white_balance = caps.default_white_balance(&source.metadata).unwrap();
    }
    if initial.color_mode.is_empty() {
        initial.color_mode = caps.default_color_mode(&source.metadata).unwrap();
    }
    initial.preview_region = None;
    initial.working_scale = Scale::NATIVE;
    let photo_extent = super::super::native_extent(&source.metadata).unwrap();
    let rect = if let Ok(value) = std::env::var("FOVEONIC_FINISH_REGION") {
        let values: Vec<u32> = value.split(',').map(|v| v.parse().unwrap()).collect();
        <[u32; 4]>::try_from(values).unwrap()
    } else {
        let size = [992.min(photo_extent[0]), 1002.min(photo_extent[1])];
        let origin = [
            (photo_extent[0] - size[0]) / 2,
            (photo_extent[1] - size[1]) / 2,
        ];
        [
            origin[0],
            origin[1],
            origin[0] + size[0],
            origin[1] + size[1],
        ]
    };
    assert!(
        rect[0] < rect[2]
            && rect[1] < rect[3]
            && rect[2] <= photo_extent[0]
            && rect[3] <= photo_extent[1]
    );
    let engine = Engine::new().unwrap();
    let mut rows = Vec::new();
    for (label, divisor, shared, regional) in [
        ("full-local", 1, false, false),
        ("full-shared", 1, true, false),
        ("fast-local", 2, false, false),
        ("fast-shared", 2, true, false),
        ("draft-local", 4, false, false),
        ("draft-shared", 4, true, false),
        ("region-shared", 1, true, true),
    ] {
        if !adaptive_variants && label != "full-local" && label != "region-shared" {
            continue;
        }
        let mut settings = initial.clone();
        settings.working_scale = Scale::new(divisor).unwrap();
        settings.preview_region = regional.then_some(rect);
        let resolved = settings.resolved_exposure(&source.metadata);
        let effective = resolved.processing_settings(source.metadata.processing_family().unwrap());
        let mut execution = Execution::default();
        let mut profile = Profile::default();
        let input = || super::super::RecipeInput::Original(&source);
        let mut built = super::super::build_photographic(
            &engine,
            input(),
            &effective,
            &settings,
            &["final"],
            &mut profile,
            &mut execution,
        )
        .unwrap();
        if regional {
            built
                .graph
                .set_view(built.final_id, super::super::camera_view(built.params));
        }
        if shared && !regional {
            // The shared-edit input precedes photographic framing in Merrill.
            // Ask the camera boundary for that grid rather than using final RGB geometry.
            let camera_settings =
                super::super::finishing::camera_settings(&effective.active_tone_settings());
            let main = super::super::build_camera(
                &engine,
                input(),
                &camera_settings,
                &["linear.base"],
                &mut profile,
                &mut execution,
            )
            .unwrap();
            let analysis_scale =
                adaptive_analysis_scale(&camera_settings, caps.thumb_scale()).unwrap();
            let (_, analysis_grid) = whole_photo_rgb(
                &engine,
                input(),
                camera_settings,
                analysis_scale,
                &mut profile,
                &mut execution,
            )
            .unwrap();
            let mapping = guide_mapping(main.rgb_to_native, analysis_grid.rgb_to_native).unwrap();
            let node = built
                .graph
                .nodes
                .iter_mut()
                .find(|n| matches!(n.operation, Operation::AdaptiveTone { .. }))
                .unwrap();
            let Operation::AdaptiveTone {
                params, analysis, ..
            } = &mut node.operation
            else {
                unreachable!()
            };
            *params = params
                .with_application_guide(
                    *analysis,
                    mapping,
                    analysis_scale == effective.working_scale,
                )
                .unwrap();
            node.inputs[1] = node.inputs[3];
            for &output in &node.outputs {
                built.graph.outputs[output]
                    .dependencies
                    .clone_from(&node.inputs);
            }
        }
        let mut artifacts = {
            let input = input();
            let plan = engine
                .plan_artifacts(
                    &settings,
                    &mut built.graph,
                    built.final_id,
                    &["final"],
                    &mut profile,
                    &mut execution,
                )
                .unwrap();
            let values = engine
                .run_graph(
                    input.context(),
                    &built.graph,
                    &plan.roots,
                    &mut profile,
                    &mut execution,
                )
                .unwrap();
            engine.package_artifacts(input, &settings, &built.graph, plan, values)
        }
        .unwrap();
        let Content::Image(image) = artifacts.pop().unwrap().content else {
            panic!("expected RGB")
        };
        assert_eq!(
            image.settings().preview_region,
            settings.preview_region,
            "unexpected whole fallback"
        );
        let patch = save_patch(
            &engine,
            &image,
            photo_extent,
            rect,
            &output.join(format!("{label}.f32")),
        );
        rows.push(serde_json::json!({"label":label,"scale_divisor":divisor,"shared_guide":shared,"regional":regional,"patch":patch}));
        let report = serde_json::json!({"source":source_path,"settings":RenderSettingsSnapshot::from(&initial),"photo_extent":photo_extent,"rect":rect,"adapter":engine.adapter_name(),"processing_quality":initial.processing_quality,"application_guide_divisor":initial.processing_quality.guide_scale(initial.working_scale).divisor(),"variants":rows,"timing":"quality attribution only; graph preparation, fresh GPU execution and diagnostic readback are not latency comparisons"});
        fs::write(
            output.join("report.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(rows.len(), if adaptive_variants { 7 } else { 2 });
}
