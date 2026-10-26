//! Checks graph port types and selective output dependencies.

use super::*;
use crate::color_space::ColorSpace;
use crate::nodes::sensor::calibration::{Calibration, ColorParams, EncodeParams};

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
    let histogram = graph.named_output(black, "black.histogram").unwrap();
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
    let mut params = <crate::noise::Parameters as bytemuck::Zeroable>::zeroed();
    params.reduction[0] = 1.;
    let raw = graph
        .add(
            Operation::RenderedInput {
                data: DataType::CalibratedLayers,
            },
            &[],
        )
        .unwrap();
    let base = graph.add(Operation::NoiseBase(params), &[raw]).unwrap();
    let conditioned = graph
        .add(
            Operation::Column {
                params,
                enabled: true,
            },
            &[raw],
        )
        .unwrap();
    let ratio = graph
        .add(Operation::Ratio(params), &[conditioned, base])
        .unwrap();
    let seed = graph
        .named_output(base, "noise.pattern-seed-tiles")
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
            &[ratio, seed],
        )
        .unwrap();
    let detail = graph
        .add(
            Operation::Texture {
                params,
                prepared: crate::nodes::color::texture::Prepared::new(Default::default(), 1.)
                    .unwrap(),
            },
            &[conditioned, base, pattern],
        )
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
            &[detail],
        )
        .unwrap();
    let restored = graph
        .add(
            Operation::ProjectColorTiles {
                params,
                space: ColorSpace::Srgb,
            },
            &[colour],
        )
        .unwrap();
    let signal = graph
        .add(
            Operation::SharpSignal {
                space: ColorSpace::Srgb,
            },
            &[restored],
        )
        .unwrap();
    let required = graph.required(&[signal]).unwrap();
    assert!(required[raw] && required[base] && required[colour] && required[pattern]);
    assert!(required[detail] && required[restored]);
    assert_eq!(graph.outputs[signal].data, DataType::SignedDetail);
    let mask = graph.named_output(pattern, "pattern.mask").unwrap();
    let masks = graph.required(&[mask]).unwrap();
    assert!(masks[ratio] && masks[seed]);
    assert!(!masks[pattern] && !masks[detail]);
    let raw_output = graph.named_output(ratio, "detail.raw-ratio").unwrap();
    let raw_ratio = graph.required(&[raw_output]).unwrap();
    assert!(raw_ratio[base] && raw_ratio[conditioned]);
    assert!(!raw_ratio[pattern] && !raw_ratio[seed] && !raw_ratio[detail]);
    let edges = graph.named_output(base, "noise.edges").unwrap();
    let required = graph.required(&[edges]).unwrap();
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
        destination: crate::render::TiffColorSpace::Srgb,
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

#[test]
fn named_outputs_are_producer_local_and_keep_port_types() {
    let mut graph = Graph::default();
    let input = graph
        .add(
            Operation::RenderedInput {
                data: DataType::LinearCodeRgb,
            },
            &[],
        )
        .unwrap();
    let root = || {
        Operation::RootDetail(
            crate::nodes::finish::root_detail::Prepared::new([1, 1], [0, 0, 1, 1], 0.5, 1000, 1.)
                .unwrap(),
        )
    };
    let first = graph.add(root(), &[input]).unwrap();
    let second = graph.add(root(), &[input]).unwrap();
    let detail = graph.named_output(second, "detail.signal").unwrap();
    assert_eq!(
        graph.outputs[detail].producer,
        graph.outputs[second].producer
    );
    assert_ne!(detail, graph.named_output(first, "detail.signal").unwrap());
    assert_eq!(graph.named_output(detail, "root.rgb").unwrap(), second);
    assert_eq!(graph.outputs[detail].data, DataType::SignedDetail);
    assert!(graph.add(root(), &[detail]).is_err());
    assert!(graph.named_output(input, "detail.signal").is_err());
    assert!(graph.named_output(second, "missing").is_err());
    assert!(graph.named_output(usize::MAX, "detail.signal").is_err());
}

#[test]
fn code_scale_requires_finalization_once_before_root_detail() {
    use crate::nodes::{color::code_gain, sensor::fixed_point};
    let mut graph = Graph::default();
    let layers = graph
        .add(
            Operation::RenderedInput {
                data: DataType::CalibratedLayers,
            },
            &[],
        )
        .unwrap();
    let color = |output_scale| {
        Operation::FixedPointColor(std::sync::Arc::new(fixed_point::Transform {
            output_scale,
            params: fixed_point::TransformParams {
                source: [1, 1, 2, 14],
                matrix: [[0; 4]; 3],
                bounds: [-500, 24000, 0, 0],
            },
            tables: vec![0; 6],
        }))
    };
    let normalized = graph
        .add(color(fixed_point::CodeScale::Normalized), &[layers])
        .unwrap();
    let ready = graph
        .add(color(fixed_point::CodeScale::Ready), &[layers])
        .unwrap();
    assert_eq!(graph.outputs[normalized].data, DataType::NormalizedCodeRgb);
    assert_eq!(graph.outputs[ready].data, DataType::LinearCodeRgb);
    let gain = || Operation::CodeGain(code_gain::Prepared::new([1, 1], 1.675).unwrap());
    let root = || {
        Operation::RootDetail(
            crate::nodes::finish::root_detail::Prepared::new([1, 1], [0, 0, 1, 1], 0.5, 1000, 1.)
                .unwrap(),
        )
    };
    assert!(graph.add(root(), &[normalized]).is_err());
    assert!(graph.add(gain(), &[ready]).is_err());
    let log = || {
        Operation::LogReconstruct(
            crate::nodes::color::log_reconstruction::Prepared::new([4, 4], [1.; 10]).unwrap(),
        )
    };
    assert!(graph.add(log(), &[ready, layers]).is_err());
    let reconstructed = graph.add(log(), &[normalized, layers]).unwrap();
    assert!(graph.add(root(), &[reconstructed]).is_ok());
    let finalized = graph.add(gain(), &[normalized]).unwrap();
    assert!(graph.add(gain(), &[finalized]).is_err());
    assert!(graph.add(root(), &[finalized]).is_ok());
    assert!(graph.add(root(), &[ready]).is_ok());
    assert_eq!(
        DataType::from(crate::image::Space::NormalizedCodeRgb),
        DataType::NormalizedCodeRgb
    );
    assert!(
        color(fixed_point::CodeScale::Normalized).key("color.codes")
            != color(fixed_point::CodeScale::Ready).key("color.codes")
    );
}

#[test]
fn image_signals_round_trip_with_their_canonical_layout_and_extent() {
    use crate::image::{Layout, Space};
    let extent = [17, 9];
    for (space, layout) in [
        (Space::SensorRgb, Layout::Rgb32Float),
        (Space::SensorBlue, Layout::Scalar32Float),
        (Space::CorrectedSensorRgb, Layout::Rgb32Float),
        (Space::CorrectedSensorBlue, Layout::Scalar32Float),
        (Space::SensorLayers, Layout::Rgb32Float),
        (Space::ScalarField, Layout::Scalar32Float),
        (Space::SensorMask, Layout::Scalar32Float),
        (Space::Monochrome, Layout::Scalar32Float),
        (Space::LuminanceRatio, Layout::Scalar32Float),
        (Space::SignedDetail, Layout::Scalar32Float),
        (Space::CalibratedLayers, Layout::Rgb32Float),
        (Space::NormalizedCodeRgb, Layout::Rgb32Float),
        (Space::LinearCodeRgb, Layout::Rgb32Float),
        (Space::SignedCodeRgb, Layout::Rgb32Float),
        (Space::UnsignedCodeRgb, Layout::Rgb32Float),
        (Space::LinearRgb(ColorSpace::Srgb), Layout::Rgb32Float),
        (
            Space::ToneMappedRgb(ColorSpace::ProPhotoRgb),
            Layout::Rgb32Float,
        ),
        (
            Space::EncodedRgb16(crate::render::TiffColorSpace::AdobeRgb),
            Layout::PackedRgb16,
        ),
    ] {
        let description = DataType::from(space).image_description(extent).unwrap();
        assert_eq!(description.space, space);
        assert_eq!(description.layout, layout);
        assert_eq!(description.extent, extent);
    }
    for data in [
        DataType::PhaseCells,
        DataType::BlackLevels,
        DataType::Histogram,
        DataType::RowTable,
        DataType::FilteredTiles,
        DataType::ConditionedTiles,
        DataType::PatternSeedTiles,
        DataType::UnconditionedRatioTiles,
        DataType::RatioTiles,
        DataType::ColorTiles(ColorSpace::Srgb),
        DataType::File,
    ] {
        assert!(data.image_description(extent).is_none(), "{data:?}");
    }
}

#[test]
fn linear_code_base_requires_an_explicit_unsigned_code_input() {
    let mut graph = Graph::default();
    let codes = graph
        .add(
            Operation::RenderedInput {
                data: DataType::UnsignedCodeRgb,
            },
            &[],
        )
        .unwrap();
    let linear = graph.add(Operation::LinearCodeBase, &[codes]).unwrap();
    assert_eq!(graph.outputs[linear].data, DataType::LinearCodeRgb);
    assert_eq!(graph.required(&[linear]).unwrap(), vec![true, true]);
    let display = graph
        .add(
            Operation::RenderedInput {
                data: DataType::ToneMappedRgb(ColorSpace::Srgb),
            },
            &[],
        )
        .unwrap();
    assert!(graph.add(Operation::LinearCodeBase, &[display]).is_err());
}

#[test]
fn companion_branches_share_inputs_but_keep_distinct_edit_identities() {
    use crate::{color_space::ColorSpace, nodes::finish::photographic::Params};
    let mut graph = Graph::default();
    let data = DataType::ToneMappedRgb(ColorSpace::Srgb);
    let input = graph.add(Operation::RenderedInput { data }, &[]).unwrap();
    let mut p = Params::new([8, 8], ColorSpace::Srgb);
    p.values = [0., 1., 0., 0.];
    let main = graph
        .add(
            Operation::PhotographicTone {
                params: p,
                space: ColorSpace::Srgb,
            },
            &[input],
        )
        .unwrap();
    let mut branch = Graph::default();
    let guide_input = branch.add(Operation::RenderedInput { data }, &[]).unwrap();
    p.values[0] = 0.5;
    let guide = branch
        .add(
            Operation::PhotographicTone {
                params: p,
                space: ColorSpace::Srgb,
            },
            &[guide_input],
        )
        .unwrap();
    branch.retain_for_session(guide);
    let mapping = graph.append(branch, "guide");
    assert_eq!(mapping[guide_input], input);
    assert_ne!(mapping[guide], main);
    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.outputs[main].cache_name(), "finish.tone");
    assert_eq!(
        graph.outputs[mapping[guide]].cache_name(),
        "guide/finish.tone"
    );
    assert!(graph.session_outputs.contains(&mapping[guide]));
    graph.required(&[main, mapping[guide]]).unwrap();
}
