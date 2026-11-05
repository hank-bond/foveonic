//! Registers compute kernels and builds pipelines from the owning node shaders.

use super::Gpu;
use crate::source::Result;

#[derive(Clone, Copy)]
pub enum Kernel {
    LinearBasis,
    TransferLinearGain,
    ColorEditorMap,
    ColorEditorFilter,
    ColorEditorApply,
    ColorEditorPick,
    PerceptualGrade,
    AdvancedColor,
    DehazePrepare,
    DehazeHorizontal,
    DehazeVertical,
    #[cfg(test)]
    DehazeEstimate,
    DehazePartition,
    DehazeMerge,
    DehazeApply,
    MonoTextureExtract,
    MonoTextureBlur,
    MonoTextureApply,
    CodeChannelMap,
    ChannelNoiseExtract,
    ChannelNoiseReplace,
    NeighborhoodNoiseStats,
    NeighborhoodDecorrelate,
    ChromaticRatio,
    BoxContrastHorizontal,
    BoxContrastApply,
    ColumnGain,
    CapAccumulate,
    CapProfiles,
    CapDefects,
    CalibratedDefectRepair,
    MultiscalePrepare,
    MultiscaleDownsample,
    MultiscaleMoments,
    MultiscaleCoefficients,
    MultiscaleAverage,
    MultiscaleApply,
    MultiscaleFinish,
    ShieldHistogram,
    ShieldGuard,
    ShieldProfile,
    ShieldCenter,
    ShieldPredict,
    ShieldReduce,
    ShieldApply,
    RowGuardEvidence,
    RowGuardProfiles,
    RowGuardRobust,
    RowGuardReliability,
    RowGuardHighpass,
    RowGuardMean,
    RowGuardConfidence,
    RowGuardHorizontal,
    RowGuardApply,
    DirectionalBandingSmooth,
    DirectionalBandingDeltas,
    DirectionalBandingFit,
    DirectionalBandingApply,
    PurpleFringe,
    Fppd,
    FppdMask,
    BluePattern,
    PhaseExtraction,
    PhaseGain,
    BlueRefresh,
    HlpCalibration,
    UvConvert,
    UvClassify,
    PyramidResample,
    RgbPrefilterGain,
    RgbPrefilterBlend,
    RgbPrefilterRange,
    SparseRepair,
    NoiseEdgeGuide,
    NoiseRecursiveMean,
    NoiseResidualBlend,
    NoiseProtectionMap,
    NoiseProtectionBlend,
    ChromaSingle,
    ChromaGeneral,
    OrderedBlueRepair,
    RawColorNeutralize,
    RawColorResponse,
    RawColorChroma,
    BlueDespeckle,
    RgbDespeckle,
    ChromaClusters,
    LumaCluster,
    DynamicRangePrepare,
    DynamicRangeCells,
    DynamicRangeFit,
    DynamicRangeApply,
    HighlightPrepare,
    HighlightClassify,
    HighlightHorizontal,
    HighlightVertical,
    HighlightFinish,
    BlueMismatch,
    SectorColor,
    CodeResponse,
    HistogramBilateral,
    BlueGuideVertical,
    BlueGuideHorizontal,
    BlueOutliers,
    BlueGuidedRestore,
    ReducedBlue,
    ReducedRestore,
    PackedLab,
    ResidualSharp,
    ResidualFinish,
    ColorSharpen,
    ProportionalSharpen,
    SpatialColorGain,
    DwtExtract,
    DwtForward,
    DwtGain,
    DwtInverse,
    DwtCopyback,
    ByteMaskDouble,
    MaskImage,
    CellGain,
    CodeCrop,
    Histogram,
    BlackMean,
    Calibration,
    Color,
    Tone,
    PhotographicTone,
    PhotographicBrightness,
    UniversalSharpExtract,
    UniversalSharpBlur,
    UniversalSharpen,
    LocalContrastExtract,
    LocalContrastBlur,
    LocalContrastApply,
    ToneMaskReduce,
    ToneMaskHorizontal,
    ToneMaskStatistics,
    ToneMaskCoefficients,
    ToneMaskVertical,
    ToneMaskUpdate,
    ToneMaskFinish,
    ToneEqualizerGain,
    ToneEqualizerApply,
    Encode,
    #[cfg(test)]
    Repair,
    Sharpen,
    ColorDq,
    View,
    Scopes,
    NoiseTile,
    NoiseBlock,
    NoiseRadius,
    NoiseBilateral,
    NoiseEdge,
    NoiseMean,
    NoiseMeanTranspose,
    NoiseMeanRestore,
    NoiseLuminance,
    NoiseDiffuse,
    NoiseOsf,
    NoiseRatio,
    TextureExtract,
    TextureBlur,
    TextureMix,
    TextureInspect,
    NoiseStitch,
    SpeckleDetect,
    SpeckleCompact,
    ShadowNeutralAnalyze,
    ShadowNeutralFilter,
    ShadowNeutralApply,
    SpecklePrefix,
    SpeckleScatter,
    SpeckleClassifyReady,
    SpeckleRepairReady,
    SpeckleRemaining,
    SpeckleRepair,
    CameraRepair,
    ClusterRepair,
    HighlightFlags,
    HighlightMap,
    HighlightDensity,
    HighlightDonors,
    HighlightPrefix,
    HighlightScatter,
    HighlightTable,
    HighlightApply,
    BandingFit,
    BandingResidual,
    BandingQuality,
    BandingMeans,
    BandingReduce,
    BandingApply,
    ColumnHorizontal,
    ColumnDeltas,
    ColumnApply,
    PatternMask,
    PatternCounts,
    PatternPrefix,
    PatternHorizontal,
    PatternVertical,
    PatternBlend,
    PatternMaskImage,
    SharpSignal,
    Resample,
    NoiseGlobalTile,
    NoisePatternSeed,
    NoiseGlobalScalarTile,
    HighlightInterpolation,
    HighlightBinHistogram,
    HighlightBinPrefix,
    HighlightBinScatter,
    CameraDetect,
    CameraPrefix,
    CameraScatter,
    NoiseMedian,
    CameraDetected,
    CameraBlocks,
    DngConvert,
    DngQuantize,
    MonoMix,
    MonoDirectional,
    MonoColumn,
    MonoHistogram,
    MonoDespeckle,
    MonoSignal,
    MonoExpand,
    MonoSharpen,
    OutputResize,
    FixedPointScale,
    FixedPointStatistics,
    FixedPointCorrection,
    FixedPointColor,
    RangePack,
    RangeReduce,
    RangeBlack,
    RangeBilateral,
    RangeGuide,
    RangeNeutral,
    RangeMeanHorizontal,
    RangeMeanVertical,
    RangeExpand,
    RangeLuminance,
    RangeDiffuseNoise,
    RangeDiffuseConstant,
    RangeExtrema,
    RangeFinish,

    ChromaHorizontal,
    ChromaApply,
    ChromaRecursiveInit,
    ChromaRecursiveHorizontal,
    ChromaRecursiveApply,
    SignedPhotographic,
    ReferenceSaturation,
    CodeRoot,
    CodeDetailVertical,
    CodeDetailHorizontal,
    SignedRepair,
    SignedDespeckle,
    RedDetailHorizontal,
    RedDetailApply,
    CodeGain,
    HighlightStream,
    LogPrepare,
    LogGradient,
    LogStats,
    LogReduce,
    LogGuide,
    LogForward,
    LogBackward,
    LogCombine,
    LogFinalize,
    WbBlack,
    WbReduce,
    WbSample,
    WbAutoHistogram,
    WbTone,
    WbScores,
    WbSelect,
    WbHistogram,
    WbAccumulate,
    WbMean,
    AdaptiveToneHistogram,
    AdaptiveToneFit,
    AdaptiveToneSelect,
    AdaptiveToneGain,
    AdaptiveToneSampleRegions,
    AdaptiveToneRegionSummary,
    AdaptiveToneRegionFit,
    AdaptiveToneRefine,
    FieldPrepare,
    FieldRepairs,
    FieldNative,
    FieldTone,
    FieldGrid,
    FieldConnect,
    FieldUnion,
    FieldPeaks,
    FieldPeakIndices,
    FieldCondition,
    FieldDistance,
    FieldColor,
    FieldCommit,
    FieldApply,
    FieldRank,
    FieldPrefix,
    FieldScatter,
    FieldSolveInit,
    FieldSolveState,
    FieldSolveMap,
    FieldSolveAlpha,
    FieldSolveStep,
    FieldSolveBeta,
    FieldSolveDirection,
    FieldRelaxForward,
    FieldRelaxBackward,
}

impl Kernel {
    pub fn label(self) -> &'static str {
        match self {
            Self::TransferLinearGain => "transfer linear brightness gain",
            Self::LinearBasis => "map linear RGB basis",
            Self::ColorEditorMap => "map Color Editor",
            Self::ColorEditorFilter => "stabilize Color Editor",
            Self::ColorEditorApply => "apply Color Editor",
            Self::ColorEditorPick => "pick Color Range",
            Self::PerceptualGrade => "perceptual grading",
            Self::AdvancedColor => "Advanced Color Range",
            Self::DehazePrepare => "prepare haze analysis",
            Self::DehazeHorizontal => "horizontal haze minimum",
            Self::DehazeVertical => "vertical haze minimum",
            #[cfg(test)]
            Self::DehazeEstimate => "estimate haze color",
            Self::DehazePartition => "partition haze candidates",
            Self::DehazeMerge => "merge haze candidates",
            Self::DehazeApply => "apply Dehaze",
            Self::MonoTextureExtract => "extract monochrome texture",
            Self::MonoTextureBlur => "filter monochrome texture",
            Self::MonoTextureApply => "apply monochrome texture",
            Self::CodeChannelMap => "map integral color channels",
            Self::ChannelNoiseExtract => "extract a noise guide channel",
            Self::ChannelNoiseReplace => "replace and clamp corrected channels",
            Self::NeighborhoodNoiseStats => "measure neighborhood noise",
            Self::NeighborhoodDecorrelate => "decorrelate neighborhood noise",
            Self::ChromaticRatio => "suppress chromatic ratio noise",
            Self::BoxContrastHorizontal => "average horizontal luma",
            Self::BoxContrastApply => "apply averaged local contrast",
            Self::ColumnGain => "correct packed sensor columns",
            Self::CapAccumulate => "accumulate cap moments",
            Self::CapProfiles => "measure active cap profiles",
            Self::CapDefects => "identify persistent cap defects",
            Self::CalibratedDefectRepair => "repair serial-matched sensor defects",
            Self::MultiscalePrepare => "prepare native layer color filtering",
            Self::MultiscaleDownsample => "average native layers for color filtering",
            Self::MultiscaleMoments => "measure native layer horizontal moments",
            Self::MultiscaleCoefficients => "fit joint native layer guidance",
            Self::MultiscaleAverage => "average native layer guidance horizontally",
            Self::MultiscaleApply => "apply sum-preserving native color correction",
            Self::MultiscaleFinish => "restore original native layer geometry",
            Self::ShieldHistogram => "count original shield samples",
            Self::ShieldGuard => "measure shield medians and robust guards",
            Self::ShieldProfile => "measure guarded native shield profiles",
            Self::ShieldCenter => "remove shield profile spatial means",
            Self::ShieldPredict => "predict native color bands from shields",
            Self::ShieldReduce => "remove predicted color spatial means",
            Self::ShieldApply => "correct shield-predicted native color errors",
            Self::RowGuardEvidence => "measure native row pixel evidence",
            Self::RowGuardProfiles => "measure independent native row spans",
            Self::RowGuardRobust => "combine robust native row evidence",
            Self::RowGuardReliability => "estimate native row reliability",
            Self::RowGuardHighpass => "filter native row reliability field",
            Self::RowGuardMean => "center native row correction fields",
            Self::RowGuardConfidence => "bound native row correction confidence",
            Self::RowGuardHorizontal => "prepare native row signal guide",
            Self::RowGuardApply => "apply guarded fine native row correction",
            Self::DirectionalBandingSmooth => "measure native directional neighborhoods",
            Self::DirectionalBandingDeltas => "estimate native directional band offsets",
            Self::DirectionalBandingFit => "fit axis-coherent Coarse offsets",
            Self::DirectionalBandingApply => "apply weighted native directional bands",
            Self::PurpleFringe => "correct spatial purple fringes",
            Self::Fppd => "correct sparse phase gains",
            Self::FppdMask => "mark sparse phase defects",
            Self::BluePattern => "correct directional blue patterns",
            Self::PhaseExtraction => "extract sensor phases",
            Self::PhaseGain => "apply phase gain maps",
            Self::BlueRefresh => "refresh RGB blue from spatial phases",
            Self::HlpCalibration => "calibrate sparse highlight pixels",
            Self::UvConvert => "convert camera components",
            Self::UvClassify => "classify chroma neighborhoods",
            Self::PyramidResample => "resample RGB pyramid",
            Self::RgbPrefilterGain => "scale RGB prefilter components",
            Self::RgbPrefilterBlend => "blend repaired RGB components",
            Self::RgbPrefilterRange => "filter RGB range neighborhoods",
            Self::SparseRepair => "repair sparse highlight samples",
            Self::NoiseEdgeGuide => "measure RGB edge guidance",
            Self::NoiseRecursiveMean => "filter guided recursive means",
            Self::NoiseResidualBlend => "blend guided residual noise",
            Self::NoiseProtectionMap => "measure RGB filter protection",
            Self::NoiseProtectionBlend => "blend RGB filter scales",
            Self::ChromaSingle => "repair directional RGB defects",
            Self::ChromaGeneral => "repair masked RGB defects",
            Self::OrderedBlueRepair => "repair ordered blue defects",
            Self::RawColorNeutralize => "neutralize camera-code highlights",
            Self::RawColorResponse => "apply adaptive camera color response",
            Self::RawColorChroma => "limit reconstructed chroma neighborhoods",
            Self::BlueDespeckle => "filter blue speckles",
            Self::RgbDespeckle => "filter camera RGB speckles",
            Self::ChromaClusters => "repair ordered RGB defect clusters",
            Self::LumaCluster => "repair ordered mono defect clusters",
            Self::DynamicRangePrepare => "prepare channel highlight limits",
            Self::DynamicRangeCells => "measure highlight color cells",
            Self::DynamicRangeFit => "fit and propagate highlight colors",
            Self::DynamicRangeApply => "reconstruct highlight channel colors",
            Self::HighlightPrepare => "highlight prepare",
            Self::HighlightClassify => "highlight classify",
            Self::HighlightHorizontal => "highlight horizontal",
            Self::HighlightVertical => "highlight vertical",
            Self::HighlightFinish => "highlight finish",

            Self::WbBlack => "weighted white balance black",
            Self::WbReduce => "weighted white balance reduce",
            Self::WbSample => "weighted white balance sample",
            Self::WbAutoHistogram => "weighted white balance autohistogram",
            Self::WbTone => "weighted white balance tone",
            Self::WbScores => "weighted white balance scores",
            Self::WbSelect => "weighted white balance select",
            Self::WbHistogram => "weighted white balance histogram",
            Self::WbMean => "camera-layer row means",
            Self::WbAccumulate => "weighted white balance accumulate",
            Self::BlueMismatch => "correct spatial blue highlight mismatch",
            Self::SectorColor => "apply chroma sector matrices",
            Self::CodeResponse => "apply unsigned code response",
            Self::HistogramBilateral => "filter and blend quantized scalar noise",
            Self::BlueGuideVertical => "resize signed guide vertically",
            Self::BlueGuideHorizontal => "resize signed guide horizontally",
            Self::BlueOutliers => "repair isolated blue extrema",
            Self::BlueGuidedRestore => "reconstruct color from blue guidance",
            Self::ReducedBlue => "average calibrated blue to RGB cells",
            Self::ReducedRestore => "pack same-resolution RGB and residual",
            Self::PackedLab => "convert packed RGB and Lab codes",
            Self::ResidualSharp => "derive signed luma detail",
            Self::ResidualFinish => "apply reconstruction residual",
            Self::ColorSharpen => "sharpen individual RGB channels",
            Self::ProportionalSharpen => "sharpen RGB with proportional luma detail",
            Self::SpatialColorGain => "apply spatial red and blue color gains",
            Self::DwtExtract => "extract padded contrast lightness",
            Self::DwtForward => "decompose contrast wavelets",
            Self::DwtGain => "adjust contrast wavelet bands",
            Self::DwtInverse => "reconstruct contrast wavelets",
            Self::DwtCopyback => "restore bounded contrast lightness",
            Self::ByteMaskDouble => "double a byte mask with cubic weights",
            Self::MaskImage => "display integer mask",
            Self::CellGain => "apply per-cell spatial gain",
            Self::CodeCrop => "crop and resample unsigned RGB codes",
            Self::Histogram => "black histogram",
            Self::BlackMean => "black mean",
            Self::Calibration => "calibration",
            Self::Color => "color matrix",
            Self::ReferenceSaturation => "protected reference saturation",
            Self::Tone => "tone",
            Self::PhotographicTone => "photographic tone",
            Self::PhotographicBrightness => "photographic brightness",
            Self::UniversalSharpExtract => "sharp extract",
            Self::UniversalSharpBlur => "sharp blur",
            Self::UniversalSharpen => "universal sharpen",
            Self::LocalContrastExtract => "local extract",
            Self::LocalContrastBlur => "local blur",
            Self::LocalContrastApply => "local apply",

            Self::ToneMaskReduce => "reduced tonal guide",
            Self::ToneMaskHorizontal => "tonal guide horizontal mean",
            Self::ToneMaskStatistics => "HDR local moments and residual",
            Self::ToneMaskCoefficients => "tonal guided coefficients",
            Self::ToneMaskVertical => "tonal coefficient vertical mean",
            Self::ToneMaskUpdate => "HDR multiscale base update",
            Self::ToneMaskFinish => "spatial tonal brightness mask",
            Self::ToneEqualizerGain => "HDR coordinated gains",
            Self::ToneEqualizerApply => "apply tonal range exposure",
            Self::Encode => "encode",
            #[cfg(test)]
            Self::Repair => "repair",
            Self::Sharpen => "sharpen",
            Self::ColorDq => "ColorDQ",
            Self::View => "view texture",
            Self::Scopes => "current buffer scopes",
            Self::NoiseTile => "noise tile context",
            Self::NoiseBlock => "noise block means",
            Self::NoiseRadius => "adaptive noise radius",
            Self::NoiseBilateral => "noise bilateral base",
            Self::NoiseEdge => "noise edge weights",
            Self::NoiseMean => "directional local mean",
            Self::NoiseMeanTranspose => "transpose recursive filter input",
            Self::NoiseMeanRestore => "restore recursive filter layout",
            Self::NoiseLuminance => "reconstruction luminance",
            Self::NoiseDiffuse => "luminance diffusion",
            Self::NoiseOsf => "luminance extrema conditioning",
            Self::NoiseRatio => "detail ratio",
            Self::TextureExtract => "texture luminance cleanup",
            Self::TextureBlur => "texture spatial scale",
            Self::TextureMix => "texture restoration",
            Self::TextureInspect => "texture band inspection",
            Self::NoiseStitch => "processing tile projection",
            Self::SpeckleDetect => "speckle detection",
            Self::SpeckleCompact => "compact bright speckle correction",
            Self::ShadowNeutralAnalyze => "measure shadow color",
            Self::ShadowNeutralFilter => "measure regional shadow color",
            Self::ShadowNeutralApply => "neutralize shadow color",
            Self::SpecklePrefix => "speckle bank prefixes",
            Self::SpeckleScatter => "speckle record compaction",
            Self::SpeckleClassifyReady => "classify independent speckle repairs",
            Self::SpeckleRepairReady => "parallel speckle correction",
            Self::SpeckleRemaining => "remaining speckle ranks",
            Self::SpeckleRepair => "ordered speckle correction",
            Self::CameraRepair => "complete camera record repair",
            Self::ClusterRepair => "masked camera cluster matching",
            Self::HighlightFlags => "connected original saturation flags",
            Self::HighlightMap => "clipped highlight neighborhood map",
            Self::HighlightDensity => "highlight neighborhood density",
            Self::HighlightDonors => "highlight donor selection",
            Self::HighlightPrefix => "highlight donor prefixes",
            Self::HighlightScatter => "ordered highlight donor compaction",
            Self::HighlightTable => "highlight blue correction table",
            Self::HighlightApply => "highlight restoration",
            Self::BandingFit => "row smooth-surface fits",
            Self::BandingResidual => "row patch residuals",
            Self::BandingQuality => "row patch correlation rejection",
            Self::BandingMeans => "row patch mean contributions",
            Self::BandingReduce => "ordered row bias reduction",
            Self::BandingApply => "row bias correction",
            Self::ColumnHorizontal => "column horizontal bilateral",
            Self::ColumnDeltas => "column block means",
            Self::ColumnApply => "column conditioning",
            Self::PatternMask => "pattern minmax mask",
            Self::PatternCounts => "pattern line counts",
            Self::PatternPrefix => "ordered pattern prefixes",
            Self::PatternHorizontal => "pattern horizontal swaps",
            Self::PatternVertical => "pattern vertical swaps",
            Self::PatternBlend => "pattern ratio blend",
            Self::PatternMaskImage => "pattern mask image",
            Self::SharpSignal => "native mono high-pass",
            Self::Resample => "native polyphase resampling",
            Self::NoiseGlobalTile => "global reduced tile context",
            Self::NoisePatternSeed => "native transposed pattern seed",
            Self::NoiseGlobalScalarTile => "global reduced edge context",
            Self::HighlightInterpolation => "highlight table interpolation",
            Self::HighlightBinHistogram => "highlight donor bin counts",
            Self::HighlightBinPrefix => "highlight donor bin offsets",
            Self::HighlightBinScatter => "stable highlight donor bin grouping",
            Self::CameraDetect => "parallel original-signal bad-red detection",
            Self::CameraPrefix => "stable capped bad-red prefixes",
            Self::CameraScatter => "ordered bad-red records and mask overrides",
            Self::NoiseMedian => "reusable noise cross medians",
            Self::CameraDetected => "ordered detected camera corrections",
            Self::CameraBlocks => "stable capped bad-red block prefixes",
            Self::DngConvert => "DNG camera basis and range",
            Self::DngQuantize => "DNG linear uint16 encoding",
            Self::MonoMix => "monochrome layer mixing",
            Self::MonoDirectional => "monochrome directional filtering",
            Self::MonoColumn => "monochrome column correction",
            Self::MonoHistogram => "monochrome histogram filtering",
            Self::MonoDespeckle => "monochrome speckle correction",
            Self::MonoSignal => "monochrome signed detail",
            Self::MonoExpand => "monochrome gain replication",
            Self::MonoSharpen => "monochrome final sharpening",
            Self::OutputResize => "photographic output resize",
            Self::FixedPointScale => "signed layer scaling",
            Self::FixedPointStatistics => "signed layer flare statistics",
            Self::FixedPointCorrection => "fixed-point layer correction",
            Self::FixedPointColor => "fixed-point nonlinear color",
            Self::RangePack => "floating RGB packing",
            Self::RangeReduce => "range reconstruction reduction",
            Self::RangeBlack => "range reconstruction black estimate",
            Self::RangeBilateral => "RGB range filtering",
            Self::RangeGuide => "range reconstruction edge guide",
            Self::RangeNeutral => "range reconstruction dark neutralization",
            Self::RangeMeanHorizontal => "guided horizontal means",
            Self::RangeMeanVertical => "guided vertical means",
            Self::RangeExpand => "range reconstruction expansion",
            Self::RangeLuminance => "range reconstruction luminance",
            Self::RangeDiffuseNoise => "signal-dependent residual diffusion",
            Self::RangeDiffuseConstant => "constant-scale residual diffusion",
            Self::RangeExtrema => "residual high-extremum cap",
            Self::RangeFinish => "floating color and residual recovery",

            Self::ChromaHorizontal => "signed chroma horizontal",
            Self::ChromaApply => "signed chroma correction",
            Self::ChromaRecursiveInit => "recursive chroma coarse initialization",
            Self::ChromaRecursiveHorizontal => "recursive chroma horizontal",
            Self::ChromaRecursiveApply => "recursive chroma vertical correction",
            Self::SignedPhotographic => "signed photographic response",
            Self::CodeRoot => "signed root-space RGB",
            Self::CodeDetailVertical => "signed detail vertical",
            Self::CodeDetailHorizontal => "signed detail horizontal",
            Self::SignedRepair => "ordered signed pixel repair",
            Self::SignedDespeckle => "ordered signed despeckle",
            Self::RedDetailHorizontal => "red-layer horizontal detail",
            Self::RedDetailApply => "red-layer detail",
            Self::CodeGain => "signed code gain",
            Self::HighlightStream => "streaming signed highlight neutralization",
            Self::LogPrepare => "log reconstruction prepare",
            Self::LogGradient => "log reconstruction gradient",
            Self::LogStats => "log reconstruction row statistics",
            Self::LogReduce => "log reconstruction reduce statistics",
            Self::LogGuide => "log reconstruction edge guide",
            Self::LogForward => "log reconstruction forward",
            Self::LogBackward => "log reconstruction backward",
            Self::LogCombine => "log reconstruction combine",
            Self::LogFinalize => "log reconstruction finalize",
            Self::AdaptiveToneHistogram => "adaptive tone joint histogram",
            Self::AdaptiveToneFit => "adaptive tone candidate fit",
            Self::AdaptiveToneSelect => "adaptive tone response selection",
            Self::AdaptiveToneGain => "adaptive tone adaptive gains",
            Self::AdaptiveToneSampleRegions => "adaptive tone regional samples",
            Self::AdaptiveToneRegionSummary => "adaptive tone regional summaries",
            Self::AdaptiveToneRegionFit => "adaptive tone regional candidate fit",
            Self::AdaptiveToneRefine => "adaptive tone refinement",
            Self::FieldPrepare => "color-field original reliability and anchors",
            Self::FieldRepairs => "color-field repaired donor exclusion",
            Self::FieldNative => "color-field native gradient equations",
            Self::FieldTone => "color-field gradient application",
            Self::FieldGrid => "color-field coarse evidence",
            Self::FieldConnect => "color-field local graph",
            Self::FieldUnion => "color-field region connectivity",
            Self::FieldPeaks => "color-field region peaks",
            Self::FieldPeakIndices => "color-field peak identities",
            Self::FieldCondition => "color-field boundary conditioning",
            Self::FieldDistance => "color-field donor distance",
            Self::FieldColor => "color-field color equations",
            Self::FieldCommit => "color-field color storage",
            Self::FieldApply => "color-field native color application",
            Self::FieldRank => "color-field stable row ranks",
            Self::FieldPrefix => "color-field stable row prefixes",
            Self::FieldScatter => "color-field stable row scatter",
            Self::FieldSolveInit => "color-field true residual check",
            Self::FieldSolveState => "color-field initial reduction",
            Self::FieldSolveMap => "color-field solver matrix product",
            Self::FieldSolveAlpha => "color-field solver alpha reduction",
            Self::FieldSolveStep => "color-field solver update",
            Self::FieldSolveBeta => "color-field solver beta reduction",
            Self::FieldSolveDirection => "color-field solver direction",
            Self::FieldRelaxForward => "local gradient damped Jacobi forward",
            Self::FieldRelaxBackward => "local gradient damped Jacobi backward",
        }
    }
}

impl Gpu {
    pub fn from_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter_name: String,
    ) -> Result<Self> {
        Self::from_device_profiled(
            device,
            queue,
            adapter_name,
            &mut crate::profile::Profile::default(),
        )
    }

    pub(crate) fn from_device_profiled(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter_name: String,
        profile: &mut crate::profile::Profile,
    ) -> Result<Self> {
        let began = profile.mode.enabled().then(std::time::Instant::now);
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let black = include_str!("../nodes/sensor/black.wgsl");
        let noise = include_str!("../nodes/color/noise_base.wgsl");
        let luminance = include_str!("../nodes/color/noise_luminance.wgsl");
        let despeckle = include_str!("../nodes/sensor/despeckle.wgsl");
        let camera = include_str!("../nodes/sensor/camera_repair.wgsl");
        let highlight = include_str!("../nodes/sensor/highlight.wgsl");
        let banding = include_str!("../nodes/sensor/banding.wgsl");
        let column = include_str!("../nodes/color/noise_column.wgsl");
        let pattern = include_str!("../nodes/color/pattern.wgsl");
        let sharpen = include_str!("../nodes/finish/sharpen.wgsl");
        let dng = include_str!("../dng/dng.wgsl");
        let mono = include_str!("../nodes/monochrome/mono.wgsl");
        let sensor_pair = include_str!("../nodes/sensor/pair_arithmetic.wgsl");
        let chroma_repair = format!(
            "{sensor_pair}\n{}",
            include_str!("../nodes/sensor/chroma_repair.wgsl")
        );
        let rgb_despeckle = format!(
            "{sensor_pair}\n{}",
            include_str!("../nodes/sensor/rgb_despeckle.wgsl")
        );
        let photographic_tone = format!(
            "{}\n{}",
            include_str!("../nodes/finish/photographic/common.wgsl"),
            include_str!("../nodes/finish/photographic/tone.wgsl")
        );
        let photographic_sharpen = format!(
            "{}\n{}",
            include_str!("../nodes/finish/photographic/common.wgsl"),
            include_str!("../nodes/finish/photographic/sharpen.wgsl")
        );
        let photographic_local_contrast = format!(
            "{}\n{}",
            include_str!("../nodes/finish/photographic/common.wgsl"),
            include_str!("../nodes/finish/photographic/local_contrast.wgsl")
        );
        let oklab = include_str!("../nodes/finish/oklab.wgsl");
        let shadow_neutralize = format!(
            "{oklab}\n{}",
            include_str!("../nodes/finish/shadow_neutralize.wgsl")
        );
        let saturation_guard = include_str!("../nodes/finish/saturation_guard.wgsl");
        let reference_saturation = format!(
            "{saturation_guard}\n{}",
            include_str!("../nodes/finish/reference_saturation.wgsl")
        );
        let signed_photographic = format!(
            "{saturation_guard}\n{}",
            include_str!("../nodes/finish/signed_response.wgsl")
        );
        let color_selection = include_str!("../nodes/finish/color_editor_selection.wgsl");
        let saturation = include_str!("../nodes/finish/perceptual_saturation.wgsl");
        let color_editor = format!(
            "{color_selection}\n{}\n{saturation}\n{}\n{}",
            include_str!("../nodes/finish/perceptual_color.wgsl"),
            include_str!("../nodes/finish/color_editor.wgsl"),
            include_str!("../nodes/finish/color_editor_response.wgsl")
        );
        let color_pick = format!(
            "{color_selection}\n{}",
            include_str!("../nodes/finish/color_editor_pick.wgsl")
        );
        let perceptual = format!(
            "{}\n{saturation}\n{}",
            include_str!("../nodes/finish/perceptual_color.wgsl"),
            include_str!("../nodes/finish/perceptual_grade.wgsl")
        );
        let texture = include_str!("../nodes/monochrome/texture.wgsl");
        let advanced = format!(
            "{color_selection}\n{}\n{saturation}\n{}",
            include_str!("../nodes/finish/perceptual_color.wgsl"),
            include_str!("../nodes/finish/advanced_color.wgsl")
        );
        let tonal = format!(
            "{}\n{}",
            include_str!("../nodes/finish/hdr_response.wgsl"),
            include_str!("../nodes/finish/tone_equalizer.wgsl")
        );
        let visibility = format!(
            "{}\n{}\n{}",
            include_str!("../nodes/finish/hdr_response.wgsl"),
            include_str!("../nodes/finish/adaptive_tone.wgsl"),
            include_str!("../nodes/finish/adaptive_tone_regions.wgsl")
        );
        let kernels: [(&str, &str); Kernel::FieldPrepare as usize] = [
            (
                include_str!("../nodes/finish/linear_basis.wgsl"),
                "linear_basis",
            ),
            (
                include_str!("../nodes/finish/linear_gain.wgsl"),
                "transfer_linear_gain",
            ),
            (color_editor.as_str(), "color_editor_map"),
            (color_editor.as_str(), "color_editor_filter"),
            (color_editor.as_str(), "color_editor_apply"),
            (color_pick.as_str(), "color_editor_pick"),
            (perceptual.as_str(), "perceptual_grade"),
            (advanced.as_str(), "advanced_color"),
            (
                include_str!("../nodes/finish/dehaze_estimate.wgsl"),
                "dehaze_prepare",
            ),
            (
                include_str!("../nodes/finish/dehaze_estimate.wgsl"),
                "dehaze_horizontal",
            ),
            (
                include_str!("../nodes/finish/dehaze_estimate.wgsl"),
                "dehaze_vertical",
            ),
            #[cfg(test)]
            (
                include_str!("../nodes/finish/dehaze_estimate.wgsl"),
                "dehaze_estimate",
            ),
            (
                include_str!("../nodes/finish/dehaze_estimate.wgsl"),
                "dehaze_partition",
            ),
            (
                include_str!("../nodes/finish/dehaze_estimate.wgsl"),
                "dehaze_merge",
            ),
            (include_str!("../nodes/finish/dehaze.wgsl"), "dehaze_apply"),
            (texture, "mono_texture_extract"),
            (texture, "mono_texture_blur"),
            (texture, "mono_texture_apply"),
            (
                include_str!("../nodes/finish/code_channel_map.wgsl"),
                "code_channel_map",
            ),
            (
                include_str!("../nodes/color/channel_noise.wgsl"),
                "channel_noise_extract",
            ),
            (
                include_str!("../nodes/color/channel_noise_replace.wgsl"),
                "channel_noise_replace",
            ),
            (
                include_str!("../nodes/color/neighborhood_decorrelate_stats.wgsl"),
                "neighborhood_noise_stats",
            ),
            (
                include_str!("../nodes/color/neighborhood_decorrelate.wgsl"),
                "neighborhood_decorrelate",
            ),
            (
                include_str!("../nodes/color/chromatic_ratio.wgsl"),
                "chromatic_ratio",
            ),
            (
                include_str!("../nodes/finish/box_contrast_horizontal.wgsl"),
                "box_contrast_horizontal",
            ),
            (
                include_str!("../nodes/finish/box_contrast_apply.wgsl"),
                "box_contrast_apply",
            ),
            (
                include_str!("../nodes/sensor/column_gain.wgsl"),
                "column_gain",
            ),
            (
                include_str!("../noise_calibration/training.wgsl"),
                "cap_accumulate",
            ),
            (
                include_str!("../noise_calibration/training.wgsl"),
                "cap_profiles",
            ),
            (
                include_str!("../noise_calibration/training.wgsl"),
                "cap_defects",
            ),
            (
                include_str!("../nodes/sensor/calibrated_defects.wgsl"),
                "calibrated_defect_repair",
            ),
            (
                include_str!("../nodes/sensor/multiscale.wgsl"),
                "multiscale_prepare",
            ),
            (
                include_str!("../nodes/sensor/multiscale.wgsl"),
                "multiscale_downsample",
            ),
            (
                include_str!("../nodes/sensor/multiscale.wgsl"),
                "multiscale_moments",
            ),
            (
                include_str!("../nodes/sensor/multiscale.wgsl"),
                "multiscale_coefficients",
            ),
            (
                include_str!("../nodes/sensor/multiscale.wgsl"),
                "multiscale_average",
            ),
            (
                include_str!("../nodes/sensor/multiscale.wgsl"),
                "multiscale_apply",
            ),
            (
                include_str!("../nodes/sensor/multiscale.wgsl"),
                "multiscale_finish",
            ),
            (
                include_str!("../nodes/sensor/shield_noise.wgsl"),
                "shield_histogram",
            ),
            (
                include_str!("../nodes/sensor/shield_noise.wgsl"),
                "shield_guard",
            ),
            (
                include_str!("../nodes/sensor/shield_noise.wgsl"),
                "shield_profile",
            ),
            (
                include_str!("../nodes/sensor/shield_noise.wgsl"),
                "shield_center",
            ),
            (
                include_str!("../nodes/sensor/shield_noise.wgsl"),
                "shield_predict",
            ),
            (
                include_str!("../nodes/sensor/shield_noise.wgsl"),
                "shield_reduce",
            ),
            (
                include_str!("../nodes/sensor/shield_noise.wgsl"),
                "shield_apply",
            ),
            (
                include_str!("../nodes/sensor/row_guard.wgsl"),
                "row_guard_evidence",
            ),
            (
                include_str!("../nodes/sensor/row_guard.wgsl"),
                "row_guard_profiles",
            ),
            (
                include_str!("../nodes/sensor/row_guard.wgsl"),
                "row_guard_robust",
            ),
            (
                include_str!("../nodes/sensor/row_guard.wgsl"),
                "row_guard_reliability",
            ),
            (
                include_str!("../nodes/sensor/row_guard.wgsl"),
                "row_guard_highpass",
            ),
            (
                include_str!("../nodes/sensor/row_guard.wgsl"),
                "row_guard_mean",
            ),
            (
                include_str!("../nodes/sensor/row_guard.wgsl"),
                "row_guard_confidence",
            ),
            (
                include_str!("../nodes/sensor/row_guard.wgsl"),
                "row_guard_horizontal",
            ),
            (
                include_str!("../nodes/sensor/row_guard.wgsl"),
                "row_guard_apply",
            ),
            (
                include_str!("../nodes/color/directional_banding.wgsl"),
                "neighborhoods",
            ),
            (
                include_str!("../nodes/color/directional_banding.wgsl"),
                "block_deltas",
            ),
            (
                include_str!("../nodes/color/directional_banding.wgsl"),
                "fit_offsets",
            ),
            (
                include_str!("../nodes/color/directional_banding.wgsl"),
                "correct",
            ),
            (
                include_str!("../nodes/color/purple_fringe.wgsl"),
                "purple_fringe",
            ),
            (include_str!("../nodes/sensor/fppd.wgsl"), "fppd"),
            (include_str!("../nodes/sensor/fppd_mask.wgsl"), "fppd_mask"),
            (
                include_str!("../nodes/sensor/blue_pattern.wgsl"),
                "blue_pattern",
            ),
            (
                include_str!("../nodes/sensor/phase_extraction.wgsl"),
                "phase_extraction",
            ),
            (
                include_str!("../nodes/sensor/phase_gain.wgsl"),
                "phase_gain",
            ),
            (
                include_str!("../nodes/sensor/blue_refresh.wgsl"),
                "blue_refresh",
            ),
            (
                include_str!("../nodes/sensor/hlp_calibration.wgsl"),
                "hlp_calibration",
            ),
            (
                include_str!("../nodes/color/uv_repair_convert.wgsl"),
                "uv_convert",
            ),
            (
                include_str!("../nodes/color/uv_repair_classify.wgsl"),
                "uv_classify",
            ),
            (
                include_str!("../nodes/color/pyramid_resample.wgsl"),
                "pyramid_resample",
            ),
            (
                include_str!("../nodes/color/rgb_prefilter_gain.wgsl"),
                "rgb_prefilter_gain",
            ),
            (
                include_str!("../nodes/color/rgb_prefilter_blend.wgsl"),
                "rgb_prefilter_blend",
            ),
            (
                include_str!("../nodes/color/rgb_prefilter_range.wgsl"),
                "rgb_prefilter_range",
            ),
            (
                include_str!("../nodes/sensor/sparse_repair.wgsl"),
                "sparse_repair",
            ),
            (
                include_str!("../nodes/color/two_scale_noise_guide.wgsl"),
                "noise_edge_guide",
            ),
            (
                include_str!("../nodes/color/two_scale_noise_recursive.wgsl"),
                "noise_recursive_mean",
            ),
            (
                include_str!("../nodes/color/two_scale_noise_residual.wgsl"),
                "noise_residual_blend",
            ),
            (
                include_str!("../nodes/color/two_scale_noise_map.wgsl"),
                "noise_protection_map",
            ),
            (
                include_str!("../nodes/color/two_scale_noise_blend.wgsl"),
                "noise_protection_blend",
            ),
            (chroma_repair.as_str(), "chroma_single"),
            (chroma_repair.as_str(), "chroma_general"),
            (
                include_str!("../nodes/sensor/ordered_blue_repair.wgsl"),
                "ordered_blue_repair",
            ),
            (
                include_str!("../nodes/color/raw_color_blend_neutralize.wgsl"),
                "raw_color_neutralize",
            ),
            (
                include_str!("../nodes/color/raw_color_blend_response.wgsl"),
                "raw_color_response",
            ),
            (
                include_str!("../nodes/color/raw_color_blend_chroma.wgsl"),
                "raw_color_chroma",
            ),
            (
                include_str!("../nodes/sensor/blue_despeckle.wgsl"),
                "blue_despeckle",
            ),
            (rgb_despeckle.as_str(), "rgb_despeckle"),
            (
                include_str!("../nodes/sensor/chroma_clusters.wgsl"),
                "chroma_clusters",
            ),
            (
                include_str!("../nodes/sensor/luma_cluster.wgsl"),
                "luma_cluster",
            ),
            (
                include_str!("../nodes/sensor/dynamic_range.wgsl"),
                "dynamic_range_prepare",
            ),
            (
                include_str!("../nodes/sensor/dynamic_range.wgsl"),
                "dynamic_range_cells",
            ),
            (
                include_str!("../nodes/sensor/dynamic_range.wgsl"),
                "dynamic_range_fit",
            ),
            (
                include_str!("../nodes/sensor/dynamic_range.wgsl"),
                "dynamic_range_apply",
            ),
            (
                include_str!("../nodes/sensor/highlight_restoration.wgsl"),
                "highlight_prepare",
            ),
            (
                include_str!("../nodes/sensor/highlight_restoration.wgsl"),
                "highlight_classify",
            ),
            (
                include_str!("../nodes/sensor/highlight_restoration.wgsl"),
                "highlight_horizontal",
            ),
            (
                include_str!("../nodes/sensor/highlight_restoration.wgsl"),
                "highlight_vertical",
            ),
            (
                include_str!("../nodes/sensor/highlight_restoration.wgsl"),
                "highlight_finish",
            ),
            (
                include_str!("../nodes/sensor/blue_mismatch.wgsl"),
                "blue_mismatch",
            ),
            (
                include_str!("../nodes/color/sector_color.wgsl"),
                "sector_color",
            ),
            (
                include_str!("../nodes/finish/code_response.wgsl"),
                "code_response",
            ),
            (
                include_str!("../nodes/color/histogram_bilateral.wgsl"),
                "histogram_bilateral",
            ),
            (
                include_str!("../nodes/color/blue_guided_reconstruct.wgsl"),
                "blue_guide_vertical",
            ),
            (
                include_str!("../nodes/color/blue_guided_reconstruct.wgsl"),
                "blue_guide_horizontal",
            ),
            (
                include_str!("../nodes/color/blue_guided_reconstruct.wgsl"),
                "blue_outliers",
            ),
            (
                include_str!("../nodes/color/blue_guided_reconstruct.wgsl"),
                "blue_guided_restore",
            ),
            (
                include_str!("../nodes/color/blue_guided_reconstruct.wgsl"),
                "reduced_blue",
            ),
            (
                include_str!("../nodes/color/blue_guided_reconstruct.wgsl"),
                "reduced_restore",
            ),
            (include_str!("../nodes/color/packed_lab.wgsl"), "packed_lab"),
            (
                include_str!("../nodes/color/residual_finish.wgsl"),
                "residual_sharp",
            ),
            (
                include_str!("../nodes/color/residual_finish.wgsl"),
                "residual_finish",
            ),
            (
                include_str!("../nodes/finish/color_sharpen.wgsl"),
                "color_sharpen",
            ),
            (
                include_str!("../nodes/finish/proportional_sharpen.wgsl"),
                "proportional_sharpen",
            ),
            (
                include_str!("../nodes/color/spatial_color_gain.wgsl"),
                "spatial_color_gain",
            ),
            (
                include_str!("../nodes/finish/wavelet_contrast.wgsl"),
                "dwt_extract",
            ),
            (
                include_str!("../nodes/finish/wavelet_contrast.wgsl"),
                "dwt_forward",
            ),
            (
                include_str!("../nodes/finish/wavelet_contrast.wgsl"),
                "dwt_gain",
            ),
            (
                include_str!("../nodes/finish/wavelet_contrast.wgsl"),
                "dwt_inverse",
            ),
            (
                include_str!("../nodes/finish/wavelet_contrast.wgsl"),
                "dwt_copyback",
            ),
            (
                include_str!("../nodes/color/byte_mask_double.wgsl"),
                "byte_mask_double",
            ),
            (
                include_str!("../nodes/finish/mask_image.wgsl"),
                "mask_image",
            ),
            (include_str!("../nodes/color/cell_gain.wgsl"), "cell_gain"),
            (include_str!("../nodes/finish/code_crop.wgsl"), "code_crop"),
            (black, "histogram_pass"),
            (black, "mean_pass"),
            (include_str!("../nodes/sensor/calibrate.wgsl"), "main"),
            (include_str!("../nodes/color/color.wgsl"), "main"),
            (include_str!("../nodes/finish/tone.wgsl"), "main"),
            (photographic_tone.as_str(), "photographic_tone"),
            (photographic_tone.as_str(), "photographic_brightness"),
            (photographic_sharpen.as_str(), "sharp_extract"),
            (photographic_sharpen.as_str(), "sharp_blur"),
            (photographic_sharpen.as_str(), "universal_sharpen"),
            (photographic_local_contrast.as_str(), "local_extract"),
            (photographic_local_contrast.as_str(), "local_blur"),
            (photographic_local_contrast.as_str(), "local_apply"),
            (tonal.as_str(), "reduce_guide"),
            (tonal.as_str(), "mean_horizontal"),
            (tonal.as_str(), "statistics"),
            (tonal.as_str(), "coefficients"),
            (tonal.as_str(), "mean_vertical"),
            (tonal.as_str(), "update_base"),
            (tonal.as_str(), "finish_mask"),
            (tonal.as_str(), "calculate_gain"),
            (tonal.as_str(), "apply_ranges"),
            (include_str!("../nodes/finish/encode.wgsl"), "main"),
            #[cfg(test)]
            (include_str!("../nodes/sensor/repair.wgsl"), "main"),
            (sharpen, "main"),
            (include_str!("../nodes/color/color_dq.wgsl"), "main"),
            (include_str!("shaders/view.wgsl"), "main"),
            (include_str!("shaders/scopes.wgsl"), "main"),
            (noise, "tile_input"),
            (noise, "block_mean"),
            (noise, "radius_map"),
            (noise, "bilateral"),
            (noise, "edge_map"),
            (noise, "local_mean"),
            (noise, "transpose_mean_input"),
            (noise, "transpose_mean_output"),
            (luminance, "luminance"),
            (luminance, "diffuse"),
            (luminance, "osf"),
            (luminance, "ratio"),
            (include_str!("../nodes/color/texture.wgsl"), "extract"),
            (include_str!("../nodes/color/texture.wgsl"), "blur"),
            (include_str!("../nodes/color/texture.wgsl"), "mix_texture"),
            (
                include_str!("../nodes/color/texture.wgsl"),
                "inspect_texture",
            ),
            (include_str!("../nodes/color/noise_stitch.wgsl"), "stitch"),
            (despeckle, "detect"),
            (despeckle, "compact"),
            (shadow_neutralize.as_str(), "analyze"),
            (shadow_neutralize.as_str(), "regional_filter"),
            (shadow_neutralize.as_str(), "neutralize"),
            (despeckle, "prefix"),
            (despeckle, "scatter"),
            (despeckle, "classify_ready"),
            (despeckle, "repair_ready"),
            (despeckle, "remaining"),
            (despeckle, "repair"),
            (camera, "main"),
            (include_str!("../nodes/sensor/cluster_repair.wgsl"), "main"),
            (highlight, "connected_flags"),
            (highlight, "neighborhood_map"),
            (highlight, "density_field"),
            (highlight, "donor_marks"),
            (highlight, "donor_prefix"),
            (highlight, "donor_scatter"),
            (highlight, "correction_table"),
            (highlight, "restore"),
            (banding, "fit"),
            (banding, "residual_field"),
            (banding, "quality"),
            (banding, "row_means"),
            (banding, "reduce_rows"),
            (banding, "apply"),
            (column, "horizontal"),
            (column, "block_deltas"),
            (column, "correct"),
            (pattern, "mask"),
            (pattern, "line_counts"),
            (pattern, "line_prefix"),
            (pattern, "horizontal"),
            (pattern, "vertical"),
            (pattern, "blend"),
            (pattern, "mask_image"),
            (sharpen, "signal"),
            (include_str!("../nodes/finish/resample.wgsl"), "main"),
            (noise, "global_tile"),
            (noise, "pattern_seed"),
            (noise, "global_tile_scalar"),
            (highlight, "table_interpolation"),
            (highlight, "bin_histogram"),
            (highlight, "bin_prefix"),
            (highlight, "bin_scatter"),
            (camera, "detect_records"),
            (camera, "detection_prefix"),
            (camera, "scatter_detections"),
            (noise, "cross_medians"),
            (camera, "detected_correct"),
            (camera, "detection_block_prefix"),
            (dng, "convert"),
            (dng, "quantize"),
            (mono, "mixing"),
            (mono, "directional"),
            (mono, "column"),
            (mono, "histogram"),
            (mono, "despeckle"),
            (mono, "signal"),
            (mono, "expand"),
            (sharpen, "monochrome"),
            (include_str!("../nodes/finish/resize.wgsl"), "main"),
            (
                include_str!("../nodes/sensor/fixed_point_correction.wgsl"),
                "scale",
            ),
            (
                include_str!("../nodes/sensor/fixed_point_correction.wgsl"),
                "statistics",
            ),
            (
                include_str!("../nodes/sensor/fixed_point_correction.wgsl"),
                "correct",
            ),
            (
                include_str!("../nodes/sensor/fixed_point_color.wgsl"),
                "main",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "pack",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "reduce",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "estimate_black",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "bilateral",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "edge_guide",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "neutralize",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "mean_horizontal",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "mean_vertical",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "expand",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "luminance",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "diffuse_noise",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "diffuse_constant",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "extrema",
            ),
            (
                include_str!("../nodes/color/range_reconstruction.wgsl"),
                "finish",
            ),
            (include_str!("../nodes/color/chroma.wgsl"), "horizontal"),
            (include_str!("../nodes/color/chroma.wgsl"), "apply"),
            (include_str!("../nodes/color/chroma.wgsl"), "recursive_init"),
            (
                include_str!("../nodes/color/chroma.wgsl"),
                "recursive_horizontal_pass",
            ),
            (
                include_str!("../nodes/color/chroma.wgsl"),
                "recursive_apply",
            ),
            (&signed_photographic, "main"),
            (&reference_saturation, "main"),
            (include_str!("../nodes/finish/root_detail.wgsl"), "root_map"),
            (
                include_str!("../nodes/finish/root_detail.wgsl"),
                "detail_vertical",
            ),
            (
                include_str!("../nodes/finish/root_detail.wgsl"),
                "detail_horizontal",
            ),
            (include_str!("../nodes/sensor/signed_repair.wgsl"), "repair"),
            (
                include_str!("../nodes/sensor/signed_repair.wgsl"),
                "despeckle",
            ),
            (
                include_str!("../nodes/sensor/red_detail.wgsl"),
                "horizontal",
            ),
            (include_str!("../nodes/sensor/red_detail.wgsl"), "apply"),
            (include_str!("../nodes/color/code_gain.wgsl"), "main"),
            (include_str!("../nodes/color/highlight_stream.wgsl"), "main"),
            (
                include_str!("../nodes/color/log_reconstruction.wgsl"),
                "prepare",
            ),
            (
                include_str!("../nodes/color/log_reconstruction.wgsl"),
                "gradient",
            ),
            (
                include_str!("../nodes/color/log_reconstruction.wgsl"),
                "row_statistics",
            ),
            (
                include_str!("../nodes/color/log_reconstruction.wgsl"),
                "reduce_statistics",
            ),
            (
                include_str!("../nodes/color/log_reconstruction.wgsl"),
                "edge_guide",
            ),
            (
                include_str!("../nodes/color/log_reconstruction.wgsl"),
                "forward",
            ),
            (
                include_str!("../nodes/color/log_reconstruction.wgsl"),
                "backward",
            ),
            (
                include_str!("../nodes/color/log_reconstruction.wgsl"),
                "combine",
            ),
            (
                include_str!("../nodes/color/log_reconstruction.wgsl"),
                "finalize",
            ),
            (
                include_str!("../nodes/color/weighted_white_balance.wgsl"),
                "black_statistics",
            ),
            (
                include_str!("../nodes/color/weighted_white_balance.wgsl"),
                "reduce",
            ),
            (
                include_str!("../nodes/color/weighted_white_balance.wgsl"),
                "sample_tone",
            ),
            (
                include_str!("../nodes/color/weighted_white_balance.wgsl"),
                "auto_histogram",
            ),
            (
                include_str!("../nodes/color/weighted_white_balance.wgsl"),
                "tone_image",
            ),
            (
                include_str!("../nodes/color/weighted_white_balance.wgsl"),
                "scores",
            ),
            (
                include_str!("../nodes/color/weighted_white_balance.wgsl"),
                "select_neutral",
            ),
            (
                include_str!("../nodes/color/weighted_white_balance.wgsl"),
                "neutral_histogram",
            ),
            (
                include_str!("../nodes/color/weighted_white_balance.wgsl"),
                "accumulate",
            ),
            (
                include_str!("../nodes/color/weighted_white_balance.wgsl"),
                "row_means",
            ),
            (visibility.as_str(), "histogram"),
            (visibility.as_str(), "fit"),
            (visibility.as_str(), "select_fit"),
            (visibility.as_str(), "adaptive_gain"),
            (visibility.as_str(), "sample_regions"),
            (visibility.as_str(), "summarize_regions"),
            (visibility.as_str(), "fit_regions"),
            (visibility.as_str(), "refine"),
        ];
        #[cfg(test)]
        let mut shader_writes = Vec::new();
        let pipelines = kernels
            .iter()
            .enumerate()
            .map(|(index, &(code, entry))| {
                let camera_repair = [
                    Kernel::CameraRepair as usize,
                    Kernel::CameraDetect as usize,
                    Kernel::CameraPrefix as usize,
                    Kernel::CameraScatter as usize,
                    Kernel::CameraDetected as usize,
                    Kernel::CameraBlocks as usize,
                ]
                .contains(&index);
                #[cfg(test)]
                let camera_repair = camera_repair || index == Kernel::Repair as usize;
                let code: std::borrow::Cow<'_, str> = if [
                    Kernel::Encode as usize,
                    Kernel::View as usize,
                    Kernel::Scopes as usize,
                    Kernel::DngConvert as usize,
                    Kernel::DngQuantize as usize,
                    Kernel::OutputResize as usize,
                ]
                .contains(&index)
                {
                    format!("{}\n{code}", include_str!("shaders/sample.wgsl")).into()
                } else if (Kernel::NoiseTile as usize..=Kernel::NoiseStitch as usize)
                    .contains(&index)
                    || (Kernel::ColumnHorizontal as usize..=Kernel::ColumnApply as usize)
                        .contains(&index)
                    || (Kernel::NoiseGlobalTile as usize..=Kernel::NoiseGlobalScalarTile as usize)
                        .contains(&index)
                    || index == Kernel::NoiseMedian as usize
                    || (Kernel::DirectionalBandingSmooth as usize
                        ..=Kernel::DirectionalBandingApply as usize)
                        .contains(&index)
                {
                    format!(
                        "{}\n{code}",
                        include_str!("../nodes/color/noise_common.wgsl")
                    )
                    .into()
                } else if (Kernel::DynamicRangePrepare as usize..=Kernel::HighlightFinish as usize)
                    .contains(&index)
                {
                    format!(
                        "{}\n{code}",
                        include_str!("../nodes/sensor/highlight_common.wgsl")
                    )
                    .into()
                } else if camera_repair {
                    format!(
                        "{}\n{code}",
                        include_str!("../nodes/sensor/camera_common.wgsl")
                    )
                    .into()
                } else {
                    code.into()
                };
                // A second handwritten write list could repeat the same mistake.
                // Tests check shader writes, including writes in called functions.
                #[cfg(test)]
                {
                    use wgpu::naga::{
                        front::wgsl,
                        valid::{Capabilities, GlobalUse, ValidationFlags, Validator},
                    };
                    let module = wgsl::parse_str(&code).expect("valid shader for guard audit");
                    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
                        .validate(&module)
                        .expect("validated guard audit");
                    let ep = module
                        .entry_points
                        .iter()
                        .position(|p| p.name == entry)
                        .unwrap();
                    let function = info.get_entry_point(ep);
                    let mut writes = 0;
                    for (handle, variable) in module.global_variables.iter() {
                        if let Some(binding) = &variable.binding {
                            if function[handle].intersects(GlobalUse::WRITE | GlobalUse::ATOMIC) {
                                assert_eq!(binding.group, 0);
                                writes |= 1 << binding.binding;
                            }
                        }
                    }
                    shader_writes.push(writes);
                }
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(entry),
                    source: wgpu::ShaderSource::Wgsl(code),
                });
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: None,
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                })
            })
            .collect();
        let validation = pollster::block_on(device.pop_error_scope());
        if let Some(began) = began {
            profile.record("gpu pipelines and validation", began);
        }
        if let Some(error) = validation {
            return Err(error.into());
        }
        Ok(Self {
            device,
            queue,
            pipelines,
            field: Default::default(),
            #[cfg(test)]
            shader_writes,
            adapter_name,
            activity: Default::default(),
        })
    }
}
