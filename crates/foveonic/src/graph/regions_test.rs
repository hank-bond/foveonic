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
