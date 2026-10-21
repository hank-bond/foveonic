//! Defines supported operations, parameter keys, typed ports, and named side outputs.

use super::DataType;
use crate::color_space::ColorSpace;
use crate::nodes::sensor::calibration::{Calibration, ColorParams, EncodeParams};
use crate::nodes::{
    color::{color_dq, pattern},
    monochrome::{Prepared as MonoPrepared, Stage as MonoStage},
    sensor::{banding, calibration, camera_repair, color_field, despeckle, highlight},
};
use crate::render::TiffColorSpace;
use std::path::PathBuf;

pub enum Operation {
    Source,
    RenderedInput {
        data: DataType,
    },
    FixedPointCalibration(std::sync::Arc<crate::nodes::sensor::fixed_point::Correction>),
    FixedPointColor(std::sync::Arc<crate::nodes::sensor::fixed_point::Transform>),
    ChromaSmooth(crate::nodes::color::chroma::Prepared),
    SignedRepair(std::sync::Arc<crate::nodes::sensor::signed_repair::Defects>),
    SignedDespeckle(crate::nodes::sensor::signed_repair::Despeckle),
    HighlightStream(crate::nodes::color::highlight_stream::Prepared),
    LogReconstruct(crate::nodes::color::log_reconstruction::Prepared),
    RootDetail(crate::nodes::finish::root_detail::Prepared),
    SignedLinearDetail(std::sync::Arc<crate::nodes::finish::signed_response::LinearDetail>),
    SignedPhotographic(std::sync::Arc<crate::nodes::finish::signed_response::Prepared>),
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
    ColorField(color_field::Prepared),
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
        destination: TiffColorSpace,
    },
    Tiff {
        path: PathBuf,
        params: EncodeParams,
        space: TiffColorSpace,
    },
}

impl Operation {
    /// Let each processing stage decide which settings affect its result.
    /// Track inputs separately for each output so side outputs can be reused safely.
    pub(crate) fn key(&self, output: &str) -> crate::recipe::Key {
        use crate::{noise::RecipeStage as N, recipe::Key};
        match self {
            Self::Source | Self::RenderedInput { .. } | Self::SharpSignal { .. } => Key::default(),
            Self::FixedPointCalibration(p) => p.key(),
            Self::FixedPointColor(p) => p.key(),
            Self::ChromaSmooth(p) => p.key(),
            Self::RootDetail(p) => p.key(),
            Self::LogReconstruct(p) => p.key(),
            Self::HighlightStream(p) => Key::default()
                .with(&p.extent)
                .with(&p.fix)
                .with(&p.apriori)
                .with(&p.scaled)
                .with(&(p.bulb as u32)),
            Self::SignedRepair(p) => p.key(),
            Self::SignedDespeckle(p) => p.key(),
            Self::SignedLinearDetail(p) => p.key(),
            Self::SignedPhotographic(p) => p.key(),
            Self::Black(p) => Key::pod(p),
            Self::Calibrate(p) => p.key(),
            Self::Color { params, .. } | Self::Saturation { params, .. } => Key::pod(params),
            Self::Repair(p) => p.key(),
            Self::Despeckle(p) => p.key(),
            Self::Highlights(p) | Self::HighlightDensity(p) => p.key(),
            Self::ColorField(p) => p.key(),
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
            Self::Encode {
                params,
                destination,
                ..
            } => Key::pod(params).with(&(*destination as u32)),
            Self::Tiff { params, space, .. } => Key::pod(params).with(&(*space as u32)),
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Source => "upload sensor",
            Self::RenderedInput { .. } => "import rendered image",
            Self::FixedPointCalibration(_) => "submit fixed-point calibration",
            Self::FixedPointColor(_) => "submit fixed-point color",
            Self::ChromaSmooth(_) => "submit signed chroma smoothing",
            Self::RootDetail(_) => "submit root-space detail",
            Self::LogReconstruct(_) => "submit logarithmic guided reconstruction",
            Self::HighlightStream(_) => "submit streaming signed highlight neutralization",
            Self::SignedRepair(_) => "submit ordered signed defect repair",
            Self::SignedDespeckle(_) => "submit ordered signed despeckle",
            Self::SignedLinearDetail(_) => "submit signed linear detail",
            Self::SignedPhotographic(_) => "submit signed photographic response",
            Self::Black(_) => "submit black",
            Self::Calibrate(_) => "submit calibration",
            Self::Color { .. } => "submit color",
            Self::Repair(_) => "submit repair",
            Self::Despeckle(_) => "submit despeckle",
            Self::Highlights(_) => "submit highlight restoration",
            Self::HighlightDensity(_) => "submit original saturation density",
            Self::ColorField(_) => "complete color-field highlights",
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
            Self::FixedPointCalibration(_) => (vec![SensorLayers], CalibratedLayers),
            Self::FixedPointColor(_) => (vec![CalibratedLayers], LinearCodeRgb),
            Self::RootDetail(_) => (vec![LinearCodeRgb], SignedCodeRgb),
            Self::LogReconstruct(_) => (vec![LinearCodeRgb, CalibratedLayers], LinearCodeRgb),
            Self::HighlightStream(_) => (vec![CalibratedLayers], CalibratedLayers),
            Self::SignedRepair(_) | Self::SignedDespeckle(_) => {
                (vec![CalibratedLayers], CalibratedLayers)
            }
            Self::ChromaSmooth(_) => (vec![CalibratedLayers], CalibratedLayers),
            Self::SignedLinearDetail(_) => (vec![SignedCodeRgb, SignedDetail], LinearCodeRgb),
            Self::SignedPhotographic(_) => (
                vec![SignedCodeRgb, SignedDetail],
                ToneMappedRgb(ColorSpace::ProPhotoRgb),
            ),
            Self::Black(_) => (vec![SensorLayers], BlackLevels),
            Self::Calibrate(_) => (vec![SensorLayers, BlackLevels], CalibratedLayers),
            Self::Color { space, .. } => (vec![CalibratedLayers], LinearRgb(*space)),
            Self::Repair(_) => (vec![CalibratedLayers, SensorLayers], CalibratedLayers),
            Self::Despeckle(_) | Self::Highlights(_) | Self::Banding(_) => {
                (vec![CalibratedLayers], CalibratedLayers)
            }
            Self::HighlightDensity(_) => (vec![], ScalarField),
            Self::ColorField(_) => (
                vec![
                    CalibratedLayers,
                    SensorLayers,
                    BlackLevels,
                    CalibratedLayers,
                ],
                CalibratedLayers,
            ),
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
            Self::Encode {
                space, destination, ..
            } => (vec![ToneMappedRgb(*space)], EncodedRgb16(*destination)),
            Self::Tiff { space, .. } => (vec![EncodedRgb16(*space)], File),
        }
    }
}

impl Operation {
    pub(super) fn side_outputs(&self) -> Vec<(&'static str, DataType, Vec<usize>)> {
        use DataType::*;
        match self {
            Self::FixedPointColor(_) => vec![("color.corrected-layers", CalibratedLayers, vec![0])],
            Self::RootDetail(_) => vec![("detail.signal", SignedDetail, vec![0])],
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
            Self::FixedPointCalibration(_) => "calibration.layers",
            Self::FixedPointColor(_) => "color.codes",
            Self::ChromaSmooth(p) => {
                if p.params.shape[2] == 0 {
                    "chroma.small-layers"
                } else {
                    "chroma.layers"
                }
            }
            Self::RootDetail(_) => "root.rgb",
            Self::LogReconstruct(_) => "reconstruction.codes",
            Self::HighlightStream(_) => "highlight.layers",
            Self::SignedRepair(_) => "repair.layers",
            Self::SignedDespeckle(_) => "despeckle.layers",
            Self::SignedLinearDetail(_) => "detail.linear-codes",
            Self::SignedPhotographic(_) => "tone.rgb",
            Self::Black(_) => "black.levels",
            Self::Calibrate(_) => "calibration.layers",
            Self::Repair(_) => "repair.layers",
            Self::Despeckle(_) => "despeckle.layers",
            Self::Highlights(_) | Self::ColorField(_) => "highlight.layers",
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
