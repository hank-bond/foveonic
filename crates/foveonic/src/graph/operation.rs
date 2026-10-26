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
    /// Low-pass calibrated layers onto a grid with half the width and height.
    ReduceLayers(crate::nodes::color::pyramid_resample::Prepared),
    ImageRegion {
        prepared: crate::nodes::finish::image_region::Prepared,
        data: DataType,
        output: &'static str,
    },
    /// Publish a recipe-established linear-light interpretation without changing its codes.
    LinearCodeBase,
    CodeChannelMap(std::sync::Arc<crate::nodes::finish::code_channel_map::Prepared>),
    ChannelNoise(std::sync::Arc<crate::nodes::color::channel_noise::Prepared>),
    BoxContrast(std::sync::Arc<crate::nodes::finish::box_contrast::Prepared>),
    ColumnGain(std::sync::Arc<crate::nodes::sensor::column_gain::Prepared>),
    BluePattern(std::sync::Arc<crate::nodes::sensor::blue_pattern::Prepared>),
    Fppd(std::sync::Arc<crate::nodes::sensor::fppd::Prepared>),
    PurpleFringe(std::sync::Arc<crate::nodes::color::purple_fringe::Prepared>),
    PackedLab(crate::nodes::color::packed_lab::Prepared),
    WaveletContrast(std::sync::Arc<crate::nodes::finish::wavelet_contrast::Prepared>),
    ColorSharpen(crate::nodes::finish::color_sharpen::Params),
    ProportionalSharpen(std::sync::Arc<crate::nodes::finish::proportional_sharpen::Prepared>),
    CellGain(std::sync::Arc<crate::nodes::color::cell_gain::Prepared>),
    SpatialColorGain {
        prepared: std::sync::Arc<crate::nodes::color::spatial_color_gain::Prepared>,
        output: &'static str,
    },
    RawColorBlend(std::sync::Arc<crate::nodes::color::raw_color_blend::Prepared>),
    SectorColor(std::sync::Arc<crate::nodes::color::sector_color::Prepared>),
    ByteMaskDouble(crate::nodes::color::byte_mask_double::Params),
    MaskImage {
        params: crate::nodes::finish::mask_image::Params,
        output: &'static str,
    },
    BlueGuidedReconstruct(std::sync::Arc<crate::nodes::color::blue_guided_reconstruct::Prepared>),
    ResidualFinish {
        params: crate::nodes::color::residual_finish::Params,
        with_mask: bool,
    },
    Source,
    SourcePhaseCells,
    CodeCrop(std::sync::Arc<crate::nodes::finish::code_crop::Prepared>),
    EmptyDefectFlags([u32; 2]),
    TwoScaleNoise(std::sync::Arc<crate::nodes::color::two_scale_noise::Prepared>),
    PhaseExtraction(crate::nodes::sensor::phase_extraction::Params),
    PhaseGain(std::sync::Arc<crate::nodes::sensor::phase_gain::Prepared>),
    BlueRefresh(crate::nodes::sensor::blue_refresh::Params),
    HlpCalibration(std::sync::Arc<crate::nodes::sensor::hlp_calibration::Prepared>),
    OrderedBlueRepair(std::sync::Arc<crate::nodes::sensor::ordered_blue_repair::Prepared>),
    BlueMismatch(std::sync::Arc<crate::nodes::sensor::blue_mismatch::Prepared>),
    HighlightRestoration(std::sync::Arc<crate::nodes::sensor::highlight_restoration::Prepared>),
    BlueDespeckle(std::sync::Arc<crate::nodes::sensor::blue_despeckle::Prepared>),
    LumaClusters(std::sync::Arc<crate::nodes::sensor::luma_cluster::Prepared>),
    SparseRepair(std::sync::Arc<crate::nodes::sensor::sparse_repair::Prepared>),
    RgbDespeckle(std::sync::Arc<crate::nodes::sensor::rgb_despeckle::Prepared>),
    ChromaClusters(std::sync::Arc<crate::nodes::sensor::chroma_clusters::Prepared>),
    ChromaRepair {
        prepared: std::sync::Arc<crate::nodes::sensor::chroma_repair::Prepared>,
        stage: crate::nodes::sensor::chroma_repair::Stage,
    },
    RenderedInput {
        data: DataType,
    },
    FixedPointCalibration(std::sync::Arc<crate::nodes::sensor::fixed_point::Correction>),
    FixedPointColor(std::sync::Arc<crate::nodes::sensor::fixed_point::Transform>),
    ChromaSmooth(crate::nodes::color::chroma::Prepared),
    CodeChromaSmooth(crate::nodes::color::chroma::Prepared),
    SignedRepair(std::sync::Arc<crate::nodes::sensor::signed_repair::Defects>),
    SignedDespeckle(crate::nodes::sensor::signed_repair::Despeckle),
    RedDetail(crate::nodes::sensor::red_detail::Params),
    HighlightStream(crate::nodes::color::highlight_stream::Prepared),
    LogReconstruct(crate::nodes::color::log_reconstruction::Prepared),
    CodeGain(crate::nodes::color::code_gain::Prepared),
    RangeReconstruct(std::sync::Arc<crate::nodes::color::range_reconstruction::Prepared>),
    RootDetail(crate::nodes::finish::root_detail::Prepared),
    SignedLinearDetail(std::sync::Arc<crate::nodes::finish::signed_response::LinearDetail>),
    SignedPhotographic(std::sync::Arc<crate::nodes::finish::signed_response::Prepared>),
    CodeResponse {
        prepared: std::sync::Arc<crate::nodes::finish::code_response::Prepared>,
        space: ColorSpace,
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
    NoiseContext(crate::noise::Parameters),
    NoiseBase(crate::noise::Parameters),
    Column {
        params: crate::noise::Parameters,
        enabled: bool,
    },
    Ratio(crate::noise::Parameters),
    Pattern {
        params: crate::noise::Parameters,
        prepared: pattern::Prepared,
    },
    Texture {
        params: crate::noise::Parameters,
        prepared: crate::nodes::color::texture::Prepared,
    },
    ProjectColorTiles {
        params: crate::noise::Parameters,
        space: ColorSpace,
    },
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
        let key = match self {
            Self::ReduceLayers(p) => p.key(),
            Self::ImageRegion {
                prepared,
                data,
                output,
            } => prepared
                .key()
                .slice(format!("{data:?}").as_bytes())
                .slice(output.as_bytes()),
            Self::LinearCodeBase => Key::default(),
            Self::BoxContrast(p) => p.key(),
            Self::ColumnGain(p) => p.key(),
            Self::BluePattern(p) => p.key(),
            Self::Fppd(p) => p.key(),
            Self::PurpleFringe(p) => p.key(),
            Self::PackedLab(p) => p.key(),
            Self::WaveletContrast(p) => p.key(),
            Self::CodeChannelMap(p) => p.key(),
            Self::ChannelNoise(p) => p.key(),
            Self::ColorSharpen(p) => p.key(),
            Self::ProportionalSharpen(p) => p.key(),
            Self::CellGain(prepared) => prepared.key(),
            Self::EmptyDefectFlags(extent) => Key::pod(extent),
            Self::CodeCrop(p) => p.key(),
            Self::SpatialColorGain { prepared, .. } => prepared.key(),
            Self::RawColorBlend(prepared) => prepared.key(),
            Self::SectorColor(prepared) => prepared.key(),
            Self::Source
            | Self::SourcePhaseCells
            | Self::RenderedInput { .. }
            | Self::SharpSignal { .. } => Key::default(),
            Self::ByteMaskDouble(p) => p.key(),
            Self::MaskImage { params, .. } => params.key(),
            Self::BlueGuidedReconstruct(p) => p.key(),
            Self::ResidualFinish { params, with_mask } => params.key().with(&u32::from(*with_mask)),
            Self::PhaseExtraction(p) => p.key(),
            Self::PhaseGain(p) => p.key(),
            Self::BlueRefresh(p) => p.key(),
            Self::HlpCalibration(p) => p.key(),
            Self::OrderedBlueRepair(p) => p.key(),
            Self::BlueMismatch(p) => p.key(),
            Self::HighlightRestoration(p) => p.key(),
            Self::TwoScaleNoise(p) => p.key(),
            Self::BlueDespeckle(p) => p.key(),
            Self::LumaClusters(p) => p.key(),
            Self::SparseRepair(p) => p.key(),
            Self::RgbDespeckle(p) => p.key(),
            Self::ChromaClusters(p) => p.key(),
            Self::ChromaRepair { prepared, stage } => {
                let stage = match stage {
                    crate::nodes::sensor::chroma_repair::Stage::Single => 0_u32,
                    crate::nodes::sensor::chroma_repair::Stage::General => 1_u32,
                };
                prepared.key().join(Key::pod(&stage))
            }
            Self::FixedPointCalibration(p) => p.key(),
            Self::FixedPointColor(p) => p.key(),
            Self::RangeReconstruct(p) => p.key(),
            Self::ChromaSmooth(p) | Self::CodeChromaSmooth(p) => p.key(),
            Self::RootDetail(p) => p.key(),
            Self::LogReconstruct(p) => p.key(),
            Self::CodeGain(p) => p.key(),
            Self::HighlightStream(p) => Key::default()
                .with(&p.extent)
                .with(&p.fix)
                .with(&p.apriori)
                .with(&p.scaled)
                .with(&(p.bulb as u32)),
            Self::SignedRepair(p) => p.key(),
            Self::SignedDespeckle(p) => p.key(),
            Self::RedDetail(p) => Key::pod(p),
            Self::SignedLinearDetail(p) => p.key(),
            Self::SignedPhotographic(p) => p.key(),
            Self::CodeResponse { prepared, space } => prepared.key().with(&(*space as u32)),
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
            Self::NoiseContext(p) => p.full_frame().key(N::Base),
            Self::NoiseBase(p) => p.key(N::Base),
            Self::Column { params, enabled } => params.key(N::Column).with(&u32::from(*enabled)),
            Self::Ratio(p) => p.key(N::Ratio),
            Self::Pattern { params, prepared } => params.key(N::Geometry).join(prepared.key()),
            Self::Texture { params, prepared } => params.key(N::Texture).join(prepared.key()),
            Self::ProjectColorTiles { params, .. } => params.key(N::Geometry).with(&params.region),
            Self::TiledColor { params, color, .. } => params.key(N::Geometry).with(color),
            Self::TiledColorDq { params, table, .. } => params.key(N::Geometry).join(table.key()),
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
        };
        // Compact tile buffers depend on their window. Only projected images
        // also depend on the physical output rectangle within those tiles.
        match self {
            Self::NoiseBase(p) if matches!(output, "noise.layers" | "noise.edges") => {
                key.with(&p.region)
            }
            Self::Column { params, .. } if output == "column.layers" => key.with(&params.region),
            Self::Ratio(p) if output == "detail.raw-ratio" => key.with(&p.region),
            Self::Pattern { params, .. } if matches!(output, "detail.ratio" | "pattern.mask") => {
                key.with(&params.region)
            }
            Self::Texture { params, .. } if output == "texture.change" => key.with(&params.region),
            Self::TiledColor { params, .. } if output == "color.rgb" => key.with(&params.region),
            Self::TiledColorDq { params, .. } if output == "color-dq.rgb" => {
                key.with(&params.region)
            }
            _ => key,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::ReduceLayers(_) => "reduce calibrated preview layers",
            Self::LinearCodeBase => "retain linear RGB code base",
            Self::PackedLab(p) => match p.direction() {
                crate::nodes::color::packed_lab::Direction::Forward => {
                    "convert RGB codes to packed Lab"
                }
                crate::nodes::color::packed_lab::Direction::Inverse => {
                    "convert packed Lab to RGB codes"
                }
            },
            Self::WaveletContrast(_) => "apply wavelet local contrast",
            Self::CodeChannelMap(_) => "map integral color channels",
            Self::ChannelNoise(_) => "filter channel noise",
            Self::ColorSharpen(_) => "sharpen individual RGB channels",
            Self::ProportionalSharpen(_) => "apply adaptive proportional sharpening",
            Self::BoxContrast(_) => "apply averaged local contrast",
            Self::ColumnGain(_) => "correct packed sensor columns",
            Self::BluePattern(_) => "correct directional blue patterns",
            Self::Fppd(_) => "correct sparse phase gains",
            Self::PurpleFringe(_) => "correct spatial purple fringes",
            Self::CellGain(_) => "apply gain to phase cells",
            Self::SpatialColorGain { .. } => "apply spatial color gain",
            Self::RawColorBlend(_) => "apply adaptive raw color",
            Self::SectorColor(_) => "apply chroma-sector matrices",
            Self::Source => "upload sensor",
            Self::SourcePhaseCells => "upload sensor phase cells",
            Self::EmptyDefectFlags(_) => "initialize empty defect flags",
            Self::ImageRegion { .. } => "copy numerical viewport region",
            Self::CodeCrop(_) => "crop and resample unsigned RGB codes",
            Self::PhaseExtraction(_) => "extract sensor phases",
            Self::PhaseGain(_) => "apply sensor phase gain maps",
            Self::BlueRefresh(_) => "refresh RGB blue from spatial phases",
            Self::HlpCalibration(_) => "calibrate sparse highlight pixels",
            Self::ByteMaskDouble(_) => "double highlight byte mask",
            Self::MaskImage { .. } => "display integer mask",
            Self::BlueGuidedReconstruct(p) => {
                if p.output_extent() == p.extent() {
                    "reconstruct same-resolution color"
                } else {
                    "reconstruct full-resolution color"
                }
            }
            Self::ResidualFinish { .. } => "finish reconstruction residual",
            Self::OrderedBlueRepair(_) => "repair ordered blue defects",
            Self::BlueMismatch(_) => "repair highlight blue mismatch",
            Self::HighlightRestoration(_) => "restore directional highlights",
            Self::TwoScaleNoise(_) => "filter RGB at two scales",
            Self::BlueDespeckle(_) => "filter blue speckles",
            Self::LumaClusters(_) => "repair ordered blue defect clusters",
            Self::SparseRepair(_) => "repair sparse highlight samples",
            Self::ChromaRepair { .. } => "repair ordered RGB defects",
            Self::RgbDespeckle(_) => "filter camera RGB speckles",
            Self::ChromaClusters(_) => "repair ordered RGB defect clusters",
            Self::RenderedInput { .. } => "import rendered image",
            Self::FixedPointCalibration(_) => "submit fixed-point calibration",
            Self::FixedPointColor(_) => "submit fixed-point color",
            Self::RangeReconstruct(_) => "submit range reconstruction",
            Self::ChromaSmooth(_) | Self::CodeChromaSmooth(_) => "submit signed chroma smoothing",
            Self::RootDetail(_) => "submit root-space detail",
            Self::LogReconstruct(_) => "submit logarithmic guided reconstruction",
            Self::CodeGain(_) => "submit signed code gain",
            Self::HighlightStream(_) => "submit streaming signed highlight neutralization",
            Self::SignedRepair(_) => "submit ordered signed defect repair",
            Self::SignedDespeckle(_) => "submit ordered signed despeckle",
            Self::RedDetail(_) => "submit red-layer detail",
            Self::SignedLinearDetail(_) => "submit signed linear detail",
            Self::SignedPhotographic(_) => "submit signed photographic response",
            Self::CodeResponse { .. } => "submit unsigned photographic response",
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
            Self::NoiseContext(_) => "prepare reduced whole-image noise context",
            Self::NoiseBase(_) => "submit reference noise base",
            Self::Column { enabled: true, .. } => "submit reference column conditioning",
            Self::Column { enabled: false, .. } => "pack sensor tiles",
            Self::Ratio(_) => "submit reference luminance ratio",
            Self::Pattern { .. } => "submit reference pattern conditioning",
            Self::Texture { .. } => "submit texture restoration",
            Self::ProjectColorTiles { .. } => "submit color tile projection",
            Self::TiledColor { .. } => "submit tiled color",
            Self::TiledColorDq { .. } => "submit tiled ColorDQ",
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
            Self::ReduceLayers(_) => (vec![CalibratedLayers], CalibratedLayers),
            Self::ImageRegion { data, .. } => (vec![*data], *data),
            Self::PackedLab(p) => match p.direction() {
                crate::nodes::color::packed_lab::Direction::Forward => {
                    (vec![UnsignedCodeRgb], PackedLabCodes)
                }
                crate::nodes::color::packed_lab::Direction::Inverse => {
                    (vec![PackedLabCodes], UnsignedCodeRgb)
                }
            },
            Self::BoxContrast(_) => (vec![UnsignedCodeRgb], UnsignedCodeRgb),
            Self::WaveletContrast(_) => (vec![PackedLabCodes], PackedLabCodes),
            Self::ChannelNoise(_) => (vec![CorrectedSensorRgb], CorrectedSensorRgb),
            Self::CodeChannelMap(_) | Self::ColorSharpen(_) => {
                (vec![UnsignedCodeRgb], UnsignedCodeRgb)
            }
            Self::ProportionalSharpen(_) => (vec![UnsignedCodeRgb, SignedDetail], UnsignedCodeRgb),
            Self::Fppd(p) => {
                if p.mono() {
                    (vec![CorrectedSensorBlue], CorrectedSensorBlue)
                } else {
                    (vec![CorrectedSensorRgb], CorrectedSensorRgb)
                }
            }
            Self::BluePattern(_) | Self::CellGain(_) => {
                (vec![CorrectedSensorBlue], CorrectedSensorBlue)
            }
            Self::PurpleFringe(_) | Self::SpatialColorGain { .. } | Self::SectorColor(_) => {
                (vec![CorrectedSensorRgb], CorrectedSensorRgb)
            }
            Self::RawColorBlend(p) => (
                if p.requires_highlight_mask() {
                    vec![CorrectedSensorRgb, HighlightMask]
                } else {
                    vec![CorrectedSensorRgb]
                },
                CorrectedSensorRgb,
            ),
            Self::ByteMaskDouble(_) => (vec![HighlightMask], HighlightMask),
            Self::MaskImage { .. } => (vec![HighlightMask], SensorMask),
            Self::BlueGuidedReconstruct(_) => (
                vec![
                    CorrectedSensorRgb,
                    CorrectedSensorRgb,
                    CorrectedSensorRgb,
                    CorrectedSensorBlue,
                ],
                UnsignedCodeRgb,
            ),
            Self::ResidualFinish { with_mask, .. } => (
                if *with_mask {
                    vec![UnsignedCodeRgb, LuminanceRatio, HighlightMask]
                } else {
                    vec![UnsignedCodeRgb, LuminanceRatio]
                },
                UnsignedCodeRgb,
            ),
            Self::Source => (vec![], SensorLayers),
            Self::ColumnGain(_) => (vec![PhaseCells], PhaseCells),
            Self::SourcePhaseCells => (vec![], PhaseCells),
            Self::EmptyDefectFlags(_) => (vec![], DefectFlags),
            Self::CodeCrop(_) => (vec![UnsignedCodeRgb], UnsignedCodeRgb),
            Self::LinearCodeBase => (vec![UnsignedCodeRgb], LinearCodeRgb),
            Self::BlueMismatch(_)
            | Self::BlueDespeckle(_)
            | Self::LumaClusters(_)
            | Self::PhaseGain(_) => (vec![CorrectedSensorBlue], CorrectedSensorBlue),
            Self::OrderedBlueRepair(p) => (
                if p.stage() == crate::nodes::sensor::ordered_blue_repair::Stage::General {
                    vec![CorrectedSensorBlue, DefectFlags]
                } else {
                    vec![CorrectedSensorBlue]
                },
                CorrectedSensorBlue,
            ),
            Self::HighlightRestoration(_) | Self::TwoScaleNoise(_) => {
                (vec![CorrectedSensorRgb], CorrectedSensorRgb)
            }
            Self::RgbDespeckle(_) | Self::ChromaClusters(_) => {
                (vec![CorrectedSensorRgb, DefectFlags], CorrectedSensorRgb)
            }
            Self::ChromaRepair { .. } => {
                (vec![CorrectedSensorRgb, DefectFlags], CorrectedSensorRgb)
            }
            Self::BlueRefresh(_) | Self::HlpCalibration(_) | Self::SparseRepair(_) => (
                vec![CorrectedSensorRgb, CorrectedSensorBlue],
                CorrectedSensorRgb,
            ),
            Self::PhaseExtraction(p) => (
                vec![PhaseCells],
                if p.is_calibrated() {
                    CorrectedSensorRgb
                } else {
                    SensorRgb
                },
            ),
            Self::RenderedInput { data } => (vec![], *data),
            Self::FixedPointCalibration(_) => (vec![SensorLayers], CalibratedLayers),
            Self::FixedPointColor(p) => (
                vec![CalibratedLayers],
                match p.output_scale {
                    crate::nodes::sensor::fixed_point::CodeScale::Normalized => NormalizedCodeRgb,
                    crate::nodes::sensor::fixed_point::CodeScale::Ready => LinearCodeRgb,
                },
            ),
            Self::RangeReconstruct(_) => (vec![CalibratedLayers], LinearCodeRgb),
            Self::RootDetail(_) => (vec![LinearCodeRgb], SignedCodeRgb),
            Self::CodeGain(_) => (vec![NormalizedCodeRgb], LinearCodeRgb),
            Self::LogReconstruct(_) => (vec![NormalizedCodeRgb, CalibratedLayers], LinearCodeRgb),
            Self::HighlightStream(_) => (vec![CalibratedLayers], CalibratedLayers),
            Self::SignedRepair(_) | Self::SignedDespeckle(_) | Self::RedDetail(_) => {
                (vec![CalibratedLayers], CalibratedLayers)
            }
            Self::ChromaSmooth(_) => (vec![CalibratedLayers], CalibratedLayers),
            Self::CodeChromaSmooth(_) => (vec![LinearCodeRgb], LinearCodeRgb),
            Self::SignedLinearDetail(_) => (vec![SignedCodeRgb, SignedDetail], LinearCodeRgb),
            Self::CodeResponse { space, .. } => (vec![UnsignedCodeRgb], ToneMappedRgb(*space)),
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
            Self::NoiseContext(_) => (vec![CalibratedLayers], CalibratedLayers),
            Self::NoiseBase(p) => (
                if p.uses_global_context() {
                    vec![CalibratedLayers, ScalarField]
                } else {
                    vec![CalibratedLayers]
                },
                FilteredTiles,
            ),
            Self::Column { .. } => (vec![CalibratedLayers], ConditionedTiles),
            Self::Ratio(_) => (
                vec![ConditionedTiles, FilteredTiles],
                UnconditionedRatioTiles,
            ),
            Self::Pattern { .. } => (vec![UnconditionedRatioTiles, PatternSeedTiles], RatioTiles),
            Self::Texture { prepared, .. } => {
                let mut inputs = vec![ConditionedTiles];
                if prepared.needs_base() {
                    inputs.push(FilteredTiles);
                }
                if prepared.needs_ratio() {
                    inputs.push(RatioTiles);
                }
                (inputs, FilteredTiles)
            }
            Self::ProjectColorTiles { space, .. } => (vec![ColorTiles(*space)], LinearRgb(*space)),
            Self::TiledColor { space, .. } => (vec![FilteredTiles], ColorTiles(*space)),
            Self::TiledColorDq { space, .. } => (vec![ColorTiles(*space)], ColorTiles(*space)),
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
            Self::RawColorBlend(p) => {
                let dependencies = if p.requires_highlight_mask() {
                    vec![0, 1]
                } else {
                    vec![0]
                };
                vec![
                    (
                        "raw-color.neutralized",
                        CorrectedSensorRgb,
                        dependencies.clone(),
                    ),
                    ("raw-color.response", CorrectedSensorRgb, dependencies),
                ]
            }
            Self::BlueGuidedReconstruct(_) => {
                vec![("reconstruction.residual", LuminanceRatio, vec![0, 1, 2, 3])]
            }
            Self::ResidualFinish { with_mask, .. } => vec![(
                "reconstruction.detail",
                SignedDetail,
                if *with_mask {
                    vec![0, 1, 2]
                } else {
                    vec![0, 1]
                },
            )],
            Self::TwoScaleNoise(_) | Self::ChannelNoise(_) => {
                vec![("noise.source-after", CorrectedSensorRgb, vec![0])]
            }
            Self::HighlightRestoration(p) => {
                let mut outputs = vec![("highlight-restoration.mask", HighlightMask, vec![0])];
                if p.has_recovery_mask() {
                    outputs.push((
                        "highlight-restoration.recovery-flags",
                        HighlightMask,
                        vec![0],
                    ));
                }
                outputs
            }
            Self::OrderedBlueRepair(p)
                if p.stage() == crate::nodes::sensor::ordered_blue_repair::Stage::Directional =>
            {
                vec![("luma-repair.defects", DefectFlags, vec![0])]
            }
            Self::SparseRepair(_) => vec![
                ("hlp-repair.blue", CorrectedSensorBlue, vec![0, 1]),
                ("hlp-repair.defects", DefectFlags, vec![0, 1]),
            ],
            Self::HlpCalibration(_) => vec![
                ("hlp.blue", CorrectedSensorBlue, vec![0, 1]),
                ("hlp.scratch", CorrectedSensorRgb, vec![0, 1]),
            ],
            Self::PhaseExtraction(p) => vec![(
                if p.is_calibrated() {
                    "extraction.blue"
                } else {
                    "sensor.blue"
                },
                if p.is_calibrated() {
                    CorrectedSensorBlue
                } else {
                    SensorBlue
                },
                vec![0],
            )],
            Self::FixedPointColor(_) => vec![("color.corrected-layers", CalibratedLayers, vec![0])],
            Self::RootDetail(_) => vec![("detail.signal", SignedDetail, vec![0])],
            Self::Black(_) => vec![("black.histogram", Histogram, vec![0])],
            Self::Banding(_) => vec![("banding.rows", RowTable, vec![0])],
            Self::NoiseContext(_) => vec![("noise.global-edges", ScalarField, vec![0])],
            Self::NoiseBase(p) => {
                let inputs = if p.uses_global_context() {
                    vec![0, 1]
                } else {
                    vec![0]
                };
                vec![
                    ("noise.layers", CalibratedLayers, inputs.clone()),
                    ("noise.edges", ScalarField, inputs.clone()),
                    ("noise.pattern-seed-tiles", PatternSeedTiles, inputs),
                ]
            }
            Self::Column { .. } => vec![("column.layers", CalibratedLayers, vec![0])],
            Self::Ratio(_) => vec![("detail.raw-ratio", LuminanceRatio, vec![0, 1])],
            Self::Pattern { .. } => vec![
                ("detail.ratio", LuminanceRatio, vec![0, 1]),
                ("pattern.mask", ScalarField, vec![0, 1]),
            ],
            Self::Texture { .. } => vec![(
                "texture.change",
                LuminanceRatio,
                (0..self.types().0.len()).collect(),
            )],
            Self::TiledColor { space, .. } => vec![("color.rgb", LinearRgb(*space), vec![0])],
            Self::TiledColorDq { space, .. } => vec![("color-dq.rgb", LinearRgb(*space), vec![0])],
            _ => Vec::new(),
        }
    }

    pub fn output_name(&self) -> &'static str {
        match self {
            Self::ReduceLayers(_) => "preview.layers",
            Self::ImageRegion { output, .. } => output,
            Self::PackedLab(p) => match p.direction() {
                crate::nodes::color::packed_lab::Direction::Forward => "local-contrast.lab",
                crate::nodes::color::packed_lab::Direction::Inverse => "local-contrast.rgb",
            },
            Self::WaveletContrast(_) => "local-contrast.filtered-lab",
            Self::CodeChannelMap(_) => "channel-map.rgb",
            Self::ColorSharpen(_) => "color-sharpen.rgb",
            Self::ProportionalSharpen(_) => "sharpen.rgb",
            Self::Fppd(p) => {
                if p.mono() {
                    "phase-detection.blue"
                } else {
                    "phase-detection.rgb"
                }
            }
            Self::BoxContrast(_) => "local-contrast.rgb",
            Self::ColumnGain(_) => "column-gain.phase-cells",
            Self::BluePattern(_) => "blue-pattern.blue",
            Self::PurpleFringe(_) => "purple-fringe.rgb",
            Self::CellGain(_) => "shading.blue",
            Self::SpatialColorGain { output, .. } => output,
            Self::RawColorBlend(_) => "raw-color.rgb",
            Self::SectorColor(_) => "sector-color.rgb",
            Self::Source => "sensor.layers",
            Self::SourcePhaseCells => "sensor.phase-cells",
            Self::EmptyDefectFlags(_) => "repair.empty-defects",
            Self::CodeCrop(_) => "crop.rgb",
            Self::LinearCodeBase => "base.linear-codes",
            Self::PhaseGain(_) => "phase-gain.blue",
            Self::BlueRefresh(_) => "blue-refresh.rgb",
            Self::HlpCalibration(_) => "hlp.rgb",
            Self::ByteMaskDouble(_) => "reconstruction.mask",
            Self::MaskImage { output, .. } => output,
            Self::BlueGuidedReconstruct(_) => "reconstruction.rgb",
            Self::ResidualFinish { .. } => "residual.rgb",
            Self::OrderedBlueRepair(p) => match p.stage() {
                crate::nodes::sensor::ordered_blue_repair::Stage::Directional => {
                    "luma-repair.directional"
                }
                crate::nodes::sensor::ordered_blue_repair::Stage::DarkOffsets => {
                    "luma-repair.dark-offsets"
                }
                crate::nodes::sensor::ordered_blue_repair::Stage::General => "luma-repair.blue",
            },
            Self::BlueDespeckle(p) => match p.stage() {
                crate::nodes::sensor::blue_despeckle::Stage::DarkPairs => {
                    "luma-despeckle.dark-pairs"
                }
                crate::nodes::sensor::blue_despeckle::Stage::General => "luma-despeckle.blue",
                crate::nodes::sensor::blue_despeckle::Stage::NeighborRange => {
                    "neighbor-range-despeckle.blue"
                }
            },
            Self::LumaClusters(_) => "luma-clusters.blue",
            Self::BlueMismatch(_) => "blue-mismatch.blue",
            Self::HighlightRestoration(_) => "highlight-restoration.rgb",
            Self::TwoScaleNoise(_) | Self::ChannelNoise(_) => "noise.rgb",
            Self::SparseRepair(_) => "hlp-repair.rgb",
            Self::RgbDespeckle(_) => "chroma-despeckle.rgb",
            Self::ChromaClusters(_) => "chroma-clusters.rgb",
            Self::ChromaRepair { stage, .. } => match stage {
                crate::nodes::sensor::chroma_repair::Stage::Single => "chroma-repair.single",
                crate::nodes::sensor::chroma_repair::Stage::General => "chroma-repair.rgb",
            },
            Self::PhaseExtraction(p) => {
                if p.is_calibrated() {
                    "extraction.rgb"
                } else {
                    "sensor.rgb"
                }
            }
            Self::RenderedInput { .. } => "import.image",
            Self::FixedPointCalibration(_) => "calibration.layers",
            Self::FixedPointColor(_) => "color.codes",
            Self::RangeReconstruct(_) => "reconstruction.codes",
            Self::CodeChromaSmooth(_) => "chroma.rgb-codes",
            Self::ChromaSmooth(p) => {
                if p.params.shape[2] == 0 {
                    "chroma.small-layers"
                } else {
                    "chroma.layers"
                }
            }
            Self::RootDetail(_) => "root.rgb",
            Self::LogReconstruct(_) => "reconstruction.codes",
            Self::CodeGain(_) => "color.gained-codes",
            Self::HighlightStream(_) => "highlight.layers",
            Self::SignedRepair(_) => "repair.layers",
            Self::SignedDespeckle(_) => "despeckle.layers",
            Self::RedDetail(_) => "red-detail.layers",
            Self::SignedLinearDetail(_) => "detail.linear-codes",
            Self::SignedPhotographic(_) | Self::CodeResponse { .. } => "tone.rgb",
            Self::Black(_) => "black.levels",
            Self::Calibrate(_) => "calibration.layers",
            Self::Repair(_) => "repair.layers",
            Self::Despeckle(_) => "despeckle.layers",
            Self::Highlights(_) | Self::ColorField(_) => "highlight.layers",
            Self::HighlightDensity(_) => "highlight.density",
            Self::Banding(_) => "banding.layers",
            Self::Monochrome { stage, .. } => stage.output_name(),
            Self::MonoSharpen { .. } => "sharpen.rgb",
            Self::NoiseContext(_) => "noise.global-context",
            Self::NoiseBase(_) => "noise.tiles",
            Self::Column { .. } => "column.tiles",
            Self::Ratio(_) => "detail.raw-ratio-tiles",
            Self::Pattern { .. } => "detail.ratio-tiles",
            Self::Texture { .. } => "texture.layers-tiles",
            Self::ProjectColorTiles { .. } => "reconstruction.rgb",
            Self::TiledColor { .. } => "color.tiles",
            Self::TiledColorDq { .. } => "color-dq.tiles",
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
