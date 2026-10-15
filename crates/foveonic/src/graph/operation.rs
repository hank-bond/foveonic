//! Defines supported operations, parameter keys, typed ports, and named side outputs.

use super::DataType;
use crate::nodes::sensor::calibration::{Calibration, ColorParams, ColorSpace, EncodeParams};
use crate::nodes::{
    color::{color_dq, pattern},
    monochrome::{Prepared as MonoPrepared, Stage as MonoStage},
    sensor::{banding, calibration, camera_repair, despeckle, highlight},
};
use std::path::PathBuf;

pub enum Operation {
    Source,
    RenderedInput {
        data: DataType,
    },
    Black(calibration::CalibrationParams),
    Calibrate(Calibration),
    Color {
        params: ColorParams,
        space: ColorSpace,
    },
    Repair(camera_repair::Prepared),
    Despeckle(despeckle::Prepared),
    Highlights(std::sync::Arc<highlight::Prepared>),
    HighlightDensity(std::sync::Arc<highlight::Prepared>),
    Banding(banding::Prepared),
    Monochrome {
        stage: MonoStage,
        prepared: std::sync::Arc<MonoPrepared>,
        space: ColorSpace,
    },
    MonoSharpen {
        amount: f32,
        space: ColorSpace,
    },
    NoiseBase(crate::noise::Parameters),
    Column(crate::noise::Parameters),
    Ratio(crate::noise::Parameters),
    Pattern {
        params: crate::noise::Parameters,
        prepared: pattern::Prepared,
    },
    Detail(crate::noise::Parameters),
    TiledColor {
        params: crate::noise::Parameters,
        color: ColorParams,
        space: ColorSpace,
    },
    TiledColorDq {
        params: crate::noise::Parameters,
        table: color_dq::Table,
        space: ColorSpace,
    },
    Reconstruct {
        params: crate::noise::Parameters,
        space: ColorSpace,
    },
    ColorDq {
        table: color_dq::Table,
        space: ColorSpace,
    },
    Saturation {
        params: ColorParams,
        space: ColorSpace,
    },
    SharpSignal {
        space: ColorSpace,
    },
    Sharpen {
        amount: f32,
        space: ColorSpace,
    },
    Tone {
        curve: Vec<f32>,
        exposure: f32,
        space: ColorSpace,
    },
    Encode {
        params: EncodeParams,
        space: ColorSpace,
    },
    Tiff {
        path: PathBuf,
        params: EncodeParams,
        space: ColorSpace,
    },
}

impl Operation {
    /// Let each processing stage decide which settings affect its result.
    /// Track inputs separately for each output so side outputs can be reused safely.
    pub(crate) fn key(&self, output: &str) -> crate::recipe::Key {
        use crate::{noise::RecipeStage as N, recipe::Key};
        match self {
            Self::Source | Self::RenderedInput { .. } | Self::SharpSignal { .. } => Key::default(),
            Self::Black(p) => Key::pod(p),
            Self::Calibrate(p) => p.key(),
            Self::Color { params, .. } | Self::Saturation { params, .. } => Key::pod(params),
            Self::Repair(p) => p.key(),
            Self::Despeckle(p) => p.key(),
            Self::Highlights(p) | Self::HighlightDensity(p) => p.key(),
            Self::Banding(p) => p.key(),
            Self::Monochrome {
                stage, prepared, ..
            } => prepared.key(*stage),
            Self::MonoSharpen { amount, .. } => Key::pod(amount),
            Self::NoiseBase(p) => p.key(N::Base),
            Self::Column(p) => p.key(N::Column),
            Self::Ratio(p) => p.key(N::Ratio),
            Self::Pattern { params, prepared } => params.key(N::Geometry).join(prepared.key()),
            Self::Detail(p) => p.key(N::Detail),
            Self::TiledColor { params, color, .. } => params.key(N::Geometry).with(color),
            Self::TiledColorDq { params, table, .. } => params.key(N::Geometry).join(table.key()),
            Self::Reconstruct { params, .. } => params.key(if output == "detail.signal" {
                N::Signal
            } else {
                N::Geometry
            }),
            Self::ColorDq { table, .. } => table.key(),
            Self::Sharpen { amount, .. } => Key::pod(amount),
            Self::Tone {
                curve, exposure, ..
            } => Key::pod(exposure).slice(curve),
            Self::Encode { params, .. } | Self::Tiff { params, .. } => Key::pod(params),
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Source => "upload sensor",
            Self::RenderedInput { .. } => "import rendered image",
            Self::Black(_) => "submit black",
            Self::Calibrate(_) => "submit calibration",
            Self::Color { .. } => "submit color",
            Self::Repair(_) => "submit repair",
            Self::Despeckle(_) => "submit despeckle",
            Self::Highlights(_) => "submit highlight restoration",
            Self::HighlightDensity(_) => "submit original saturation density",
            Self::Banding(_) => "submit row banding correction",
            Self::Monochrome { stage, .. } => stage.label(),
            Self::MonoSharpen { .. } => "submit monochrome sharpening",
            Self::NoiseBase(_) => "submit reference noise base",
            Self::Column(_) => "submit reference column conditioning",
            Self::Ratio(_) => "submit reference luminance ratio",
            Self::Pattern { .. } => "submit reference pattern conditioning",
            Self::Detail(_) => "submit reference detail",
            Self::TiledColor { .. } => "submit tiled color",
            Self::TiledColorDq { .. } => "submit tiled ColorDQ",
            Self::Reconstruct { .. } => "submit reconstruction",
            Self::ColorDq { .. } => "submit ColorDQ",
            Self::Saturation { .. } => "submit saturation",
            Self::SharpSignal { .. } => "submit native high-pass",
            Self::Sharpen { .. } => "submit sharpen",
            Self::Tone { .. } => "submit tone",
            Self::Encode { .. } => "submit encode",
            Self::Tiff { .. } => "TIFF",
        }
    }

    pub(super) fn types(&self) -> (Vec<DataType>, DataType) {
        use DataType::*;
        match self {
            Self::Source => (vec![], SensorLayers),
            Self::RenderedInput { data } => (vec![], *data),
            Self::Black(_) => (vec![SensorLayers], BlackLevels),
            Self::Calibrate(_) => (vec![SensorLayers, BlackLevels], CalibratedLayers),
            Self::Color { space, .. } => (vec![CalibratedLayers], LinearRgb(*space)),
            Self::Repair(_) => (vec![CalibratedLayers, SensorLayers], CalibratedLayers),
            Self::Despeckle(_) | Self::Highlights(_) | Self::Banding(_) => {
                (vec![CalibratedLayers], CalibratedLayers)
            }
            Self::HighlightDensity(_) => (vec![], ScalarField),
            Self::Monochrome { stage, space, .. } => match stage {
                MonoStage::Mix => (vec![CalibratedLayers], Monochrome),
                MonoStage::Directional => (vec![Monochrome], Monochrome),
                MonoStage::Column | MonoStage::Histogram | MonoStage::Despeckle => {
                    (vec![Monochrome, Monochrome], Monochrome)
                }
                MonoStage::Signal => (vec![Monochrome], SignedDetail),
                MonoStage::Expand => (vec![Monochrome], LinearRgb(*space)),
            },
            Self::MonoSharpen { space, .. } => {
                (vec![LinearRgb(*space), SignedDetail], LinearRgb(*space))
            }
            Self::NoiseBase(_) => (vec![CalibratedLayers], FilteredTiles),
            Self::Column(_) => (vec![CalibratedLayers], ConditionedTiles),
            Self::Ratio(_) => (
                vec![ConditionedTiles, FilteredTiles],
                UnconditionedRatioTiles,
            ),
            Self::Pattern { .. } => (vec![UnconditionedRatioTiles, PatternSeedTiles], RatioTiles),
            Self::Detail(_) => (
                vec![ConditionedTiles, FilteredTiles, RatioTiles],
                ResidualTiles,
            ),
            Self::TiledColor { space, .. } => (vec![FilteredTiles], ColorTiles(*space)),
            Self::TiledColorDq { space, .. } => (vec![ColorTiles(*space)], ColorTiles(*space)),
            Self::Reconstruct { space, .. } => (
                vec![ColorTiles(*space), ResidualTiles, RatioTiles],
                LinearRgb(*space),
            ),
            Self::ColorDq { space, .. } => (vec![LinearRgb(*space)], LinearRgb(*space)),
            Self::Saturation { space, .. } => (vec![ToneMappedRgb(*space)], ToneMappedRgb(*space)),
            Self::SharpSignal { space } => (vec![LinearRgb(*space)], SignedDetail),
            Self::Sharpen { space, .. } => {
                (vec![LinearRgb(*space), SignedDetail], LinearRgb(*space))
            }
            Self::Tone { space, .. } => (vec![LinearRgb(*space)], ToneMappedRgb(*space)),
            Self::Encode { space, .. } => (vec![ToneMappedRgb(*space)], EncodedRgb16(*space)),
            Self::Tiff { space, .. } => (vec![EncodedRgb16(*space)], File),
        }
    }
}

impl Operation {
    pub(super) fn side_outputs(&self) -> Vec<(&'static str, DataType, Vec<usize>)> {
        use DataType::*;
        match self {
            Self::Black(_) => vec![("black.histogram", Histogram, vec![0])],
            Self::Banding(_) => vec![("banding.rows", RowTable, vec![0])],
            Self::NoiseBase(_) => vec![
                ("noise.layers", CalibratedLayers, vec![0]),
                ("noise.edges", ScalarField, vec![0]),
                ("noise.pattern-seed-tiles", PatternSeedTiles, vec![0]),
            ],
            Self::Column(_) => vec![("column.layers", CalibratedLayers, vec![0])],
            Self::Ratio(_) => vec![("detail.raw-ratio", LuminanceRatio, vec![0, 1])],
            Self::Pattern { .. } => vec![
                ("detail.ratio", LuminanceRatio, vec![0, 1]),
                ("pattern.mask", ScalarField, vec![0, 1]),
            ],
            Self::TiledColor { space, .. } => vec![("color.rgb", LinearRgb(*space), vec![0])],
            Self::TiledColorDq { space, .. } => vec![("color-dq.rgb", LinearRgb(*space), vec![0])],
            Self::Reconstruct { .. } => vec![("detail.signal", SignedDetail, vec![0, 2])],
            _ => Vec::new(),
        }
    }

    pub fn output_name(&self) -> &'static str {
        match self {
            Self::Source => "sensor.layers",
            Self::RenderedInput { .. } => "import.image",
            Self::Black(_) => "black.levels",
            Self::Calibrate(_) => "calibration.layers",
            Self::Repair(_) => "repair.layers",
            Self::Despeckle(_) => "despeckle.layers",
            Self::Highlights(_) => "highlight.layers",
            Self::HighlightDensity(_) => "highlight.density",
            Self::Banding(_) => "banding.layers",
            Self::Monochrome { stage, .. } => stage.output_name(),
            Self::MonoSharpen { .. } => "sharpen.rgb",
            Self::NoiseBase(_) => "noise.tiles",
            Self::Column(_) => "column.tiles",
            Self::Ratio(_) => "detail.raw-ratio-tiles",
            Self::Pattern { .. } => "detail.ratio-tiles",
            Self::Detail(_) => "detail.gain-tiles",
            Self::TiledColor { .. } => "color.tiles",
            Self::TiledColorDq { .. } => "color-dq.tiles",
            Self::Reconstruct { .. } => "reconstruction.rgb",
            Self::Color { .. } => "color.rgb",
            Self::ColorDq { .. } => "color-dq.rgb",
            Self::Tone { .. } => "tone.rgb",
            Self::Saturation { .. } => "saturation.rgb",
            Self::SharpSignal { .. } => "detail.signal",
            Self::Sharpen { .. } => "sharpen.rgb",
            Self::Encode { .. } => "export.rgb16",
            Self::Tiff { .. } => "export.file",
        }
    }
}
