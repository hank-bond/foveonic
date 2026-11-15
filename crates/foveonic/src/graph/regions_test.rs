//! Checks backward branch union and regional storage selection without executing image work.

use super::*;
use crate::{graph::Operation, noise::Parameters};
use bytemuck::Zeroable;

fn parameters() -> Parameters {
    let mut p = Parameters::zeroed();
    p.source = [512, 384, 4, 4];
    p.tile = [144, 112, 8, 11];
    p.reduction[0] = 1.;
    p.full_frame()
}

fn graph() -> (Graph, usize, usize) {
    use crate::source::{Matrix, MatrixEncoding, Metadata};
    let rect = |values: Vec<f64>| Matrix {
        encoding: MatrixEncoding::Uint32,
        representation: "u32",
        dimensions: vec![4],
        values,
    };
    let metadata = Metadata {
        width: 512,
        height: 384,
        rotation: 0,
        processing_version: None,
        properties: Default::default(),
        lists: Default::default(),
        matrices: [
            ("ActiveImageArea".into(), rect(vec![8., 8., 503., 375.])),
            ("KeepImageArea".into(), rect(vec![0., 0., 511., 383.])),
        ]
        .into(),
    };
    let mut graph = Graph::default();
    let source = graph
        .add(
            Operation::RenderedInput {
                data: crate::graph::DataType::CalibratedLayers,
            },
            &[],
        )
        .unwrap();
    let rows = graph
        .add(
            Operation::Banding(
                crate::nodes::sensor::banding::describe(&metadata, [1.; 3]).unwrap(),
            ),
            &[source],
        )
        .unwrap();
    (graph, source, rows)
}

#[test]
fn demands_union_branches_before_propagating_to_shared_inputs() {
    let (mut graph, source, rows) = graph();
    let p = parameters().for_region([140, 110, 160, 130]).unwrap();
    let q = parameters().for_region([390, 210, 410, 230]).unwrap();
    let a = graph.add(Operation::NoiseBase(p), &[rows]).unwrap();
    let b = graph
        .add(
            Operation::Column {
                params: q,
                enabled: true,
            },
            &[rows],
        )
        .unwrap();
    let demands = graph
        .demands(&[(a, Demand::Whole), (b, Demand::Whole)])
        .unwrap();
    let expected = Demand::Region(p.required_sensor_bounds())
        .union(Demand::Region(q.required_sensor_bounds()));
    assert_eq!(demands[rows], expected);
    let Demand::Region(bounds) = expected else {
        panic!("region")
    };
    let Operation::Banding(ref operation) = graph.nodes[graph.outputs[rows].producer].operation
    else {
        panic!("banding")
    };
    assert_eq!(
        demands[source],
        Demand::Region(operation.required_input_bounds(bounds).unwrap())
    );
    assert_eq!(Demand::Unused.union(expected), expected);
    assert_eq!(Demand::Whole.union(expected), Demand::Whole);
    assert!(graph.demands(&[(usize::MAX, Demand::Whole)]).is_err());
}

#[test]
fn whole_table_demand_does_not_become_a_photographic_rectangle() {
    let (mut graph, source, rows) = graph();
    let p = parameters().for_region([140, 110, 160, 130]).unwrap();
    let image = graph.add(Operation::NoiseBase(p), &[rows]).unwrap();
    let table = graph.named_output(rows, "banding.rows").unwrap();
    graph
        .resolve_regions(&[(image, Demand::Whole), (table, Demand::Whole)], |_, _| {
            false
        })
        .unwrap();
    assert_eq!(
        graph
            .demands(&[(image, Demand::Whole), (table, Demand::Whole)])
            .unwrap()[source],
        Demand::Whole
    );
    assert_eq!(
        graph.nodes[graph.outputs[rows].producer]
            .operation
            .output_storage("banding.layers"),
        None
    );
}

#[test]
fn storage_choice_preserves_cached_whole_and_prefers_cached_compact() {
    for cached in ["none", "whole", "both"] {
        let (mut graph, source, rows) = graph();
        graph.retain_for_session(rows);
        let p = parameters().for_region([140, 110, 160, 130]).unwrap();
        let bounds = p.required_sensor_bounds();
        let image = graph.add(Operation::NoiseBase(p), &[rows]).unwrap();
        graph
            .resolve_regions(&[(image, Demand::Whole)], |graph, id| {
                assert_eq!(id, rows);
                let compact = graph.nodes[graph.outputs[id].producer]
                    .operation
                    .output_storage("banding.layers")
                    .is_some();
                cached == "both" || (cached == "whole" && !compact)
            })
            .unwrap();
        let Operation::NoiseBase(resolved) = graph.nodes[graph.outputs[image].producer].operation
        else {
            panic!("base")
        };
        assert_eq!(
            graph.session_outputs,
            [if cached == "whole" { rows } else { source }]
        );
        assert_eq!(
            resolved.input_bounds,
            if cached == "whole" {
                [0, 0, 512, 384]
            } else {
                bounds
            }
        );
    }
}

#[test]
fn global_context_and_unconverted_consumers_require_whole_storage() {
    let (mut graph, source, rows) = graph();
    let context = graph
        .add(Operation::NoiseContext(parameters()), &[rows])
        .unwrap();
    let demands = graph
        .resolve_regions(&[(context, Demand::Whole)], |_, _| false)
        .unwrap();
    assert_eq!(demands[rows], Demand::Whole);
    assert_eq!(demands[source], Demand::Whole);
    assert!(graph.nodes[graph.outputs[rows].producer]
        .operation
        .output_storage("banding.layers")
        .is_none());
    let unused = graph
        .add(
            Operation::RenderedInput {
                data: crate::graph::DataType::CalibratedLayers,
            },
            &[],
        )
        .unwrap();
    assert_eq!(
        graph.demands(&[(context, Demand::Whole)]).unwrap()[unused],
        Demand::Unused
    );
}

#[test]
fn repeated_bindings_union_each_input_slots_requirements() {
    use crate::{color_space::ColorSpace, graph::DataType};
    let space = ColorSpace::Srgb;
    let mut graph = Graph::default();
    let input = graph
        .add(
            Operation::RenderedInput {
                data: DataType::LinearRgb(space),
            },
            &[],
        )
        .unwrap();
    let output = graph
        .add(
            Operation::TransferLinearGain {
                data: DataType::LinearRgb(space),
                space,
            },
            &[input, input, input],
        )
        .unwrap();
    let bounds = [[1, 2, 4, 6], [8, 9, 12, 15], [20, 21, 23, 25]];
    let demands = graph
        .propagate_demands(&[(output, Demand::Whole)], |_, _, _, slot| {
            Ok(Demand::Region(bounds[slot]))
        })
        .unwrap();
    assert_eq!(demands[input], Demand::Region([1, 2, 23, 25]));
}

fn finishing_graph() -> (Graph, usize, usize, usize, usize) {
    use crate::{
        color_space::ColorSpace,
        graph::DataType,
        nodes::finish::{adaptive_tone, tone_equalizer},
        render::BaseResponse,
    };
    let space = ColorSpace::Srgb;
    let extent = [4096, 3072];
    let mut graph = Graph::default();
    let rgb = graph
        .add(
            Operation::RenderedInput {
                data: DataType::LinearRgb(space),
            },
            &[],
        )
        .unwrap();
    let mask = graph
        .add(
            Operation::RenderedInput {
                data: DataType::BrightnessGuide,
            },
            &[],
        )
        .unwrap();
    let curve = vec![0., 0.25, 1.];
    let params = adaptive_tone::Params::new(
        extent,
        [0, 0, extent[0], extent[1]],
        4095.,
        space,
        BaseResponse::Natural,
        Default::default(),
        &curve,
        crate::render::ProcessingQuality::Speed,
    )
    .unwrap();
    let adaptive = graph
        .add(
            Operation::AdaptiveTone {
                window: Default::default(),
                params,
                analysis: params,
                curve: curve.into(),
                space,
            },
            &[rgb, mask, rgb, mask],
        )
        .unwrap();
    let guide = tone_equalizer::Guide::new(extent, 384, space).unwrap();
    let mask = graph
        .add(Operation::ToneMask { guide, space }, &[adaptive])
        .unwrap();
    let hdr = graph
        .add(
            Operation::TonalRanges {
                sampling: None,
                window: Default::default(),
                white: 4095.,
                controls: tone_equalizer::Controls {
                    whites: -1.,
                    ..Default::default()
                },
                pivot: -2.,
                space,
            },
            &[adaptive, mask],
        )
        .unwrap();
    let alias = graph
        .add(
            Operation::ImageAlias {
                dehaze_basis: None,
                data: DataType::LinearRgb(space),
                output: "regional.finishing",
            },
            &[hdr],
        )
        .unwrap();
    let tone = graph
        .add(
            Operation::Tone {
                curve: vec![0., 1.].into(),
                exposure: 1.,
                space,
            },
            &[alias],
        )
        .unwrap();
    (graph, adaptive, mask, hdr, tone)
}

#[test]
fn finishing_windows_shrink_after_adaptive_and_hdr_without_replanning_camera_support() {
    let (mut graph, adaptive, mask, hdr, tone) = finishing_graph();
    let requested = [1800, 1200, 2280, 1680];
    graph.set_view(
        tone,
        crate::image::View {
            crop: requested,
            rotation: 0,
            extent: [480, 480],
        },
    );
    let demands = graph
        .resolve_regions(&[(tone, Demand::Region(requested))], |_, _| false)
        .unwrap();
    // The HDR guide's existing 976-pixel support plus interpolation belongs on
    // its Adaptive input. Adaptive's own evidence remains unchanged and Whole.
    let adaptive_bounds = [816, 216, 3264, 2664];
    assert_eq!(demands[adaptive], Demand::Region(adaptive_bounds));
    assert_eq!(demands[0], Demand::Whole);
    let operation = |id: usize| &graph.nodes[graph.outputs[id].producer].operation;
    assert_eq!(
        operation(adaptive).output_storage("base-response.rgb"),
        Some(adaptive_bounds)
    );
    let Operation::ToneMask { guide, .. } = operation(mask) else {
        panic!("guide")
    };
    assert_eq!(guide.extent(), [2448, 2448]);
    let Operation::TonalRanges { window, .. } = operation(hdr) else {
        panic!("HDR")
    };
    assert_eq!(window.input, Some(adaptive_bounds));
    assert_eq!(window.output, Some(requested));
    let copy = window.crop([2448, 2448]).unwrap().unwrap();
    assert_eq!(copy.extent(), [480, 480]);
    assert_eq!(copy.input_bounds(None).unwrap(), [984, 984, 1464, 1464]);
    assert_eq!(graph.outputs[tone].view.unwrap().crop, [0, 0, 480, 480]);
}

#[test]
fn whole_finishing_inspection_keeps_the_original_storage_and_keys() {
    let (mut graph, adaptive, mask, hdr, tone) = finishing_graph();
    let keys: Vec<_> = graph
        .nodes
        .iter()
        .map(|node| node.operation.key(graph.outputs[node.outputs[0]].name))
        .collect();
    graph
        .resolve_regions(&[(tone, Demand::Whole)], |_, _| false)
        .unwrap();
    for (node, key) in graph.nodes.iter().zip(keys) {
        assert!(node.operation.key(graph.outputs[node.outputs[0]].name) == key);
    }
    assert!(graph.nodes[graph.outputs[adaptive].producer]
        .operation
        .output_storage("base-response.rgb")
        .is_none());
    assert!(graph.nodes[graph.outputs[hdr].producer]
        .operation
        .output_storage("tonal-ranges.rgb")
        .is_none());
    let Operation::ToneMask { guide, .. } = graph.nodes[graph.outputs[mask].producer].operation
    else {
        panic!("guide")
    };
    assert_eq!(guide.extent(), [4096, 3072]);
}

#[test]
fn retained_broad_adaptive_storage_remains_usable_by_a_compact_hdr_output() {
    let (mut graph, adaptive, mask, hdr, tone) = finishing_graph();
    let requested = [1800, 1200, 2280, 1680];
    graph
        .resolve_regions(&[(tone, Demand::Region(requested))], |graph, id| {
            id == adaptive
                && graph.nodes[graph.outputs[id].producer]
                    .operation
                    .output_storage("base-response.rgb")
                    .is_none()
        })
        .unwrap();
    let operation = |id: usize| &graph.nodes[graph.outputs[id].producer].operation;
    assert!(operation(adaptive)
        .output_storage("base-response.rgb")
        .is_none());
    let Operation::ToneMask { guide, .. } = operation(mask) else {
        panic!("guide")
    };
    assert_eq!(guide.extent(), [4096, 3072]);
    let Operation::TonalRanges { window, .. } = operation(hdr) else {
        panic!("HDR")
    };
    assert!(window.input.is_none());
    assert_eq!(window.output, Some(requested));
    assert_eq!(
        window
            .crop([4096, 3072])
            .unwrap()
            .unwrap()
            .input_bounds(None)
            .unwrap(),
        requested
    );
}

#[test]
fn unconverted_finishing_filter_keeps_its_preceding_adaptive_and_hdr_work_whole() {
    use crate::{color_space::ColorSpace, nodes::finish::photographic};
    let (mut graph, adaptive, _, hdr, _) = finishing_graph();
    let filter = graph
        .add(
            Operation::LinearLocalContrast {
                params: photographic::Params::new([4096, 3072], ColorSpace::Srgb),
                space: ColorSpace::Srgb,
            },
            &[hdr],
        )
        .unwrap();
    graph
        .resolve_regions(
            &[(filter, Demand::Region([1800, 1200, 2280, 1680]))],
            |_, _| false,
        )
        .unwrap();
    for id in [adaptive, hdr] {
        let node = &graph.nodes[graph.outputs[id].producer];
        assert!(node
            .operation
            .output_storage(graph.outputs[id].name)
            .is_none());
    }
}

#[test]
fn compact_finishing_cache_probe_uses_its_rebased_explicit_view() {
    let (mut graph, adaptive, _, _, tone) = finishing_graph();
    let requested = [1800, 1200, 2280, 1680];
    graph.set_view(
        adaptive,
        crate::image::View {
            crop: requested,
            rotation: 0,
            extent: [480, 480],
        },
    );
    let probes = std::cell::Cell::new(0);
    graph
        .resolve_regions(&[(tone, Demand::Region(requested))], |graph, id| {
            if id != adaptive {
                return false;
            }
            let bounds = graph.nodes[graph.outputs[id].producer]
                .operation
                .output_storage("base-response.rgb");
            assert_eq!(bounds, Some([816, 216, 3264, 2664]));
            assert_eq!(graph.outputs[id].view.unwrap().crop, [984, 984, 1464, 1464]);
            probes.set(probes.get() + 1);
            true
        })
        .unwrap();
    assert_eq!(probes.get(), 1);
}

#[test]
fn extraction_requests_its_complete_authored_rect_until_it_can_materialize_a_subregion() {
    use crate::{color_space::ColorSpace, graph::DataType, nodes::finish::image_region};
    let (mut graph, adaptive, _, _, _) = finishing_graph();
    let extract = graph
        .add(
            Operation::ImageRegion {
                prepared: image_region::Prepared::new([4096, 3072], [1200, 800, 2600, 2200], 3)
                    .unwrap(),
                data: DataType::LinearRgb(ColorSpace::Srgb),
                output: "regional.extraction",
            },
            &[adaptive],
        )
        .unwrap();
    let demands = graph
        .resolve_regions(&[(extract, Demand::Region([20, 30, 100, 110]))], |_, _| {
            false
        })
        .unwrap();
    assert_eq!(demands[adaptive], Demand::Region([1200, 800, 2600, 2200]));
    let Operation::ImageRegion { prepared, .. } =
        graph.nodes[graph.outputs[extract].producer].operation
    else {
        panic!("extract")
    };
    assert_eq!(prepared.extent(), [1400, 1400]);
    assert_eq!(prepared.input_bounds(None).unwrap(), [0, 0, 1400, 1400]);
}

#[test]
fn mapped_hdr_keeps_its_guide_whole_and_requests_only_local_rgb() {
    use crate::{color_space::ColorSpace, graph::DataType, nodes::finish::tone_equalizer};
    let space = ColorSpace::Srgb;
    let mut graph = Graph::default();
    let rgb = graph
        .add(
            Operation::RenderedInput {
                data: DataType::LinearRgb(space),
            },
            &[],
        )
        .unwrap();
    let mask = graph
        .add(
            Operation::RenderedInput {
                data: DataType::BrightnessGuide,
            },
            &[],
        )
        .unwrap();
    let hdr = graph
        .add(
            Operation::TonalRanges {
                sampling: Some(
                    tone_equalizer::GuideSampling::new(
                        [1024, 768],
                        [0, 0, 1024, 768],
                        [[0.5, 0., 0., 0.], [0., 0.5, 0., 0.]],
                        2,
                        2,
                    )
                    .unwrap(),
                ),
                window: Default::default(),
                white: 4095.,
                pivot: -2.,
                space,
                controls: tone_equalizer::Controls {
                    shadows: 0.5,
                    ..Default::default()
                },
            },
            &[rgb, mask],
        )
        .unwrap();
    let requested = [101, 203, 301, 403];
    let demands = graph
        .resolve_regions(&[(hdr, Demand::Region(requested))], |_, _| false)
        .unwrap();
    assert_eq!(demands[rgb], Demand::Region([98, 200, 304, 406]));
    assert_eq!(demands[mask], Demand::Whole);
    let Operation::TonalRanges { window, .. } = &graph.nodes[graph.outputs[hdr].producer].operation
    else {
        panic!("HDR")
    };
    // RenderedInput already owns Whole storage. A compact producer can supply
    // just the requested RGB without changing the independent guide's storage.
    assert_eq!(window.input, None);
    let compact = &mut graph.nodes[graph.outputs[hdr].producer].operation;
    compact
        .resolve_input_storage(&[Some([98, 200, 304, 406]), None])
        .unwrap();
    let Operation::TonalRanges { window, .. } = compact else {
        panic!("HDR")
    };
    assert_eq!(window.input, Some([98, 200, 304, 406]));
    assert_eq!(window.output, Some(requested));
}
