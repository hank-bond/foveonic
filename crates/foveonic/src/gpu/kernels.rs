//! Registers compute kernels and builds pipelines from the owning node shaders.

use super::Gpu;
use crate::source::Result;

#[derive(Clone, Copy)]
pub enum Kernel {
    Histogram,
    BlackMean,
    Calibration,
    Color,
    Tone,
    Encode,
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
    NoiseLuminance,
    NoiseDiffuse,
    NoiseOsf,
    NoiseRatio,
    NoiseMix,
    NoiseRestore,
    NoiseSignal,
    NoiseStitch,
    SpeckleDetect,
    SpecklePrefix,
    SpeckleScatter,
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
            Self::Histogram => "black histogram",
            Self::BlackMean => "black mean",
            Self::Calibration => "calibration",
            Self::Color => "color matrix",
            Self::Tone => "tone",
            Self::Encode => "encode",
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
            Self::NoiseLuminance => "reconstruction luminance",
            Self::NoiseDiffuse => "luminance diffusion",
            Self::NoiseOsf => "luminance extrema conditioning",
            Self::NoiseRatio => "detail ratio",
            Self::NoiseMix => "residual detail mixing",
            Self::NoiseRestore => "detail reconstruction",
            Self::NoiseSignal => "signed detail signal",
            Self::NoiseStitch => "processing tile projection",
            Self::SpeckleDetect => "speckle detection",
            Self::SpecklePrefix => "speckle bank prefixes",
            Self::SpeckleScatter => "speckle record compaction",
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
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let black = include_str!("../nodes/sensor/black.wgsl");
        let noise = include_str!("../nodes/color/noise_base.wgsl");
        let detail = include_str!("../nodes/color/noise_detail.wgsl");
        let despeckle = include_str!("../nodes/sensor/despeckle.wgsl");
        let camera = include_str!("../nodes/sensor/camera_repair.wgsl");
        let highlight = include_str!("../nodes/sensor/highlight.wgsl");
        let banding = include_str!("../nodes/sensor/banding.wgsl");
        let column = include_str!("../nodes/color/noise_column.wgsl");
        let pattern = include_str!("../nodes/color/pattern.wgsl");
        let sharpen = include_str!("../nodes/finish/sharpen.wgsl");
        let dng = include_str!("../dng/dng.wgsl");
        let mono = include_str!("../nodes/monochrome/mono.wgsl");
        let kernels = [
            (black, "histogram_pass"),
            (black, "mean_pass"),
            (include_str!("../nodes/sensor/calibrate.wgsl"), "main"),
            (include_str!("../nodes/color/color.wgsl"), "main"),
            (include_str!("../nodes/finish/tone.wgsl"), "main"),
            (include_str!("../nodes/finish/encode.wgsl"), "main"),
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
            (detail, "luminance"),
            (detail, "diffuse"),
            (detail, "osf"),
            (detail, "ratio"),
            (detail, "residual_mix"),
            (detail, "restore"),
            (detail, "signal"),
            (include_str!("../nodes/color/noise_stitch.wgsl"), "stitch"),
            (despeckle, "detect"),
            (despeckle, "prefix"),
            (despeckle, "scatter"),
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
        ];
        #[cfg(test)]
        let mut shader_writes = Vec::new();
        let pipelines = kernels
            .iter()
            .enumerate()
            .map(|(index, &(code, entry))| {
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
                {
                    format!(
                        "{}\n{code}",
                        include_str!("../nodes/color/noise_common.wgsl")
                    )
                    .into()
                } else if [
                    Kernel::Repair as usize,
                    Kernel::CameraRepair as usize,
                    Kernel::CameraDetect as usize,
                    Kernel::CameraPrefix as usize,
                    Kernel::CameraScatter as usize,
                    Kernel::CameraDetected as usize,
                    Kernel::CameraBlocks as usize,
                ]
                .contains(&index)
                {
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
        if let Some(error) = pollster::block_on(device.pop_error_scope()) {
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
