//! Checks graph port types and selective output dependencies.

use super::*;
use crate::nodes::sensor::calibration::{Calibration, ColorParams, ColorSpace, EncodeParams};

#[test]
fn demand_tracks_ports_without_requiring_siblings_or_unrelated_nodes() {
    let mut graph = Graph::default();
    let source = graph.add(Operation::Source, &[]).unwrap();
    let black = graph
        .add(
            Operation::Black(crate::nodes::sensor::calibration::CalibrationParams {
                source: [1, 1, 1, 0],
                dark: [0, 0, 1, 1],
                gains: [1.; 4],
                shading: [0.; 4],
                shading_geometry: [0., 0., 1., 0.],
                grids: [[0, 1, 1, 0]; 3],
            }),
            &[source],
        )
        .unwrap();
    let histogram = black + 1;
    let unrelated = graph.add(Operation::Source, &[]).unwrap();
    let required = graph.required(&[histogram]).unwrap();
    assert!(required[source]);
    assert!(required[histogram]);
    assert!(!required[black]);
    assert!(!required[unrelated]);
    let required = graph.required(&[black, histogram]).unwrap();
    assert!(required[black] && required[histogram]);
    assert!(graph.required(&[999]).is_err());
}

#[test]
fn reference_outputs_have_stage_local_dependencies_and_typed_atlases() {
    let mut graph = Graph::default();
    let params = <crate::noise::Parameters as bytemuck::Zeroable>::zeroed();
    let raw = graph
        .add(
            Operation::RenderedInput {
                data: DataType::CalibratedLayers,
            },
            &[],
        )
        .unwrap();
    let base = graph.add(Operation::NoiseBase(params), &[raw]).unwrap();
    let conditioned = graph.add(Operation::Column(params), &[raw]).unwrap();
    let ratio = graph
        .add(Operation::Ratio(params), &[conditioned, base])
        .unwrap();
    let pattern = graph
        .add(
            Operation::Pattern {
                params,
                prepared: crate::nodes::color::pattern::Prepared::new(
                    [40, 40, 1, 4],
                    [0.025, 0.025, 0., 0.],
                )
                .unwrap(),
            },
            &[ratio, base + 3],
        )
        .unwrap();
    let detail = graph
        .add(Operation::Detail(params), &[conditioned, base, pattern])
        .unwrap();
    let colour = graph
        .add(
            Operation::TiledColor {
                params,
                color: ColorParams {
                    source: [0; 4],
                    matrix: [[0.; 4]; 3],
                },
                space: ColorSpace::Srgb,
            },
            &[base],
        )
        .unwrap();
    let restored = graph
        .add(
            Operation::Reconstruct {
                params,
                space: ColorSpace::Srgb,
            },
            &[colour, detail, pattern],
        )
        .unwrap();
    let required = graph.required(&[restored + 1]).unwrap();
    assert!(required[raw] && required[base] && required[colour] && required[pattern]);
    assert!(!required[detail] && !required[restored]);
    assert_eq!(graph.outputs[restored + 1].data, DataType::SignedDetail);
    let masks = graph.required(&[pattern + 2]).unwrap();
    assert!(masks[ratio] && masks[base + 3]);
    assert!(!masks[pattern] && !masks[detail]);
    let raw_ratio = graph.required(&[ratio + 1]).unwrap();
    assert!(raw_ratio[base] && raw_ratio[conditioned]);
    assert!(!raw_ratio[pattern] && !raw_ratio[base + 3] && !raw_ratio[detail]);
    let required = graph.required(&[base + 2]).unwrap();
    assert!(required[raw]);
    assert!(!required[base] && !required[detail] && !required[colour]);
    let tone = Operation::Tone {
        curve: vec![0., 1.],
        exposure: 1.,
        space: ColorSpace::Srgb,
    };
    assert!(graph.add(tone, &[colour]).is_err()); // projection establishes full-sensor geometry
}

#[test]
fn rejects_wrong_space_and_forward_dependencies() {
    let mut graph = Graph::default();
    let source = graph.add(Operation::Source, &[]).unwrap();
    let encode = || Operation::Encode {
        params: EncodeParams {
            source: [1, 1, 0, 0],
            crop: [0, 0, 1, 1],
            output: [1, 1, 0, 0],
        },
        space: ColorSpace::Srgb,
    };
    assert!(graph.add(encode(), &[source]).is_err());
    assert!(graph.add(encode(), &[10]).is_err());
    let black = crate::nodes::sensor::calibration::CalibrationParams {
        source: [1, 1, 1, 0],
        dark: [0, 0, 1, 1],
        gains: [1.; 4],
        shading: [0.; 4],
        shading_geometry: [0., 0., 1., 0.],
        grids: [[0, 1, 1, 0]; 3],
    };
    let black_id = graph.add(Operation::Black(black), &[source]).unwrap();
    let calibrated = graph
        .add(
            Operation::Calibrate(Calibration {
                params: black,
                grid_values: vec![1.; 3],
            }),
            &[source, black_id],
        )
        .unwrap();
    let linear = graph
        .add(
            Operation::Color {
                params: ColorParams {
                    source: [1, 1, 0, 0],
                    matrix: [[0.; 4]; 3],
                },
                space: ColorSpace::Srgb,
            },
            &[calibrated],
        )
        .unwrap();
    assert!(graph
        .add(
            Operation::Repair(crate::nodes::sensor::camera_repair::Prepared::default()),
            &[linear, source]
        )
        .is_err());
    let color_dq = |space| Operation::ColorDq {
        table: crate::nodes::color::color_dq::table([8.; 3]).unwrap(),
        space,
    };
    assert!(graph
        .add(color_dq(ColorSpace::Srgb), &[calibrated])
        .is_err());
    assert!(graph
        .add(color_dq(ColorSpace::AdobeRgb), &[linear])
        .is_err());
    assert!(graph.add(color_dq(ColorSpace::Srgb), &[linear]).is_ok());
    let toned = graph
        .add(
            Operation::Tone {
                curve: vec![0., 1.],
                exposure: 1.,
                space: ColorSpace::Srgb,
            },
            &[linear],
        )
        .unwrap();
    assert!(graph.add(color_dq(ColorSpace::Srgb), &[toned]).is_err());
    let sharp = graph
        .add(
            Operation::SharpSignal {
                space: ColorSpace::Srgb,
            },
            &[linear],
        )
        .unwrap();
    assert!(graph
        .add(
            Operation::Sharpen {
                amount: 1.,
                space: ColorSpace::AdobeRgb
            },
            &[toned, toned]
        )
        .is_err());
    assert!(graph
        .add(
            Operation::Sharpen {
                amount: 1.,
                space: ColorSpace::Srgb
            },
            &[toned]
        )
        .is_err());
    assert!(graph
        .add(
            Operation::Sharpen {
                amount: 1.,
                space: ColorSpace::Srgb
            },
            &[linear, sharp]
        )
        .is_ok());
}
