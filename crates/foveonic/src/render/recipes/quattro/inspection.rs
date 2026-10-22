//! Describes Quattro inspection outputs and their sensor component meanings.

use crate::{
    inspection::{Channels, ImageOutput},
    source::Metadata,
};

const DECODED: [ImageOutput; 2] = [
    ImageOutput {
        name: "sensor.rgb",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "sensor.blue",
        channels: Channels::Scalar,
    },
];
const EXTRACTED: [ImageOutput; 2] = [
    ImageOutput {
        name: "extraction.rgb",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "extraction.blue",
        channels: Channels::Scalar,
    },
];
const PHASE_GAIN: ImageOutput = ImageOutput {
    name: "phase-gain.blue",
    channels: Channels::Scalar,
};

const DP_CALIBRATED: [ImageOutput; 5] = [
    ImageOutput {
        name: "hlp.rgb",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "hlp.blue",
        channels: Channels::Scalar,
    },
    ImageOutput {
        name: "hlp.scratch",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "hlp-repair.rgb",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "hlp-repair.blue",
        channels: Channels::Scalar,
    },
];
const FACTORY_REPAIRED: [ImageOutput; 12] = [
    ImageOutput {
        name: "chroma-repair.single",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "chroma-repair.rgb",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "chroma-despeckle.rgb",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "chroma-clusters.rgb",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "luma-repair.directional",
        channels: Channels::Scalar,
    },
    ImageOutput {
        name: "luma-repair.dark-offsets",
        channels: Channels::Scalar,
    },
    ImageOutput {
        name: "luma-despeckle.dark-pairs",
        channels: Channels::Scalar,
    },
    ImageOutput {
        name: "luma-repair.blue",
        channels: Channels::Scalar,
    },
    ImageOutput {
        name: "luma-despeckle.blue",
        channels: Channels::Scalar,
    },
    ImageOutput {
        name: "luma-clusters.blue",
        channels: Channels::Scalar,
    },
    ImageOutput {
        name: "blue-mismatch.blue",
        channels: Channels::Scalar,
    },
    ImageOutput {
        name: "highlight-restoration.rgb",
        channels: Channels::Layers,
    },
];
const RECONSTRUCTED: [ImageOutput; 16] = [
    ImageOutput {
        name: "crop.rgb",
        channels: Channels::Rgb,
    },
    ImageOutput {
        name: "final",
        channels: Channels::Rgb,
    },
    ImageOutput {
        name: "tone.rgb",
        channels: Channels::Rgb,
    },
    ImageOutput {
        name: "local-contrast.rgb",
        channels: Channels::Rgb,
    },
    ImageOutput {
        name: "sharpen.rgb",
        channels: Channels::Rgb,
    },
    ImageOutput {
        name: "shading.rgb",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "shading.source-after",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "shading.blue",
        channels: Channels::Scalar,
    },
    ImageOutput {
        name: "raw-color.rgb",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "sector-color.rgb",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "noise.rgb",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "noise.source-after",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "reconstruction.rgb",
        channels: Channels::Rgb,
    },
    ImageOutput {
        name: "reconstruction.residual",
        channels: Channels::Scalar,
    },
    ImageOutput {
        name: "residual.rgb",
        channels: Channels::Rgb,
    },
    ImageOutput {
        name: "reconstruction.detail",
        channels: Channels::Scalar,
    },
];

/// Capture identity establishes decoded planes before calibration metadata arrives.
/// Each recipe advertises the boundaries it actually constructs.
pub(crate) fn outputs(metadata: Option<&Metadata>) -> Vec<ImageOutput> {
    let mut outputs = DECODED.to_vec();
    let Some(metadata) = metadata else {
        return outputs;
    };
    if !super::calibration::supports_extraction(metadata) {
        return outputs;
    }
    outputs.extend(EXTRACTED);
    let is_dp = super::calibration::is_ordinary_dp(metadata);
    let is_sd = super::calibration::is_ordinary_sd(metadata);
    if is_dp {
        outputs.push(PHASE_GAIN);
        outputs.push(ImageOutput {
            name: "blue-refresh.rgb",
            channels: Channels::Layers,
        });
        if super::hlp::supported(metadata) {
            outputs.extend(DP_CALIBRATED);
        }
    }
    if is_sd {
        outputs.extend([
            ImageOutput {
                name: "phase-detection.rgb",
                channels: Channels::Layers,
            },
            ImageOutput {
                name: "phase-detection.blue",
                channels: Channels::Scalar,
            },
        ]);
        if metadata.scalar("CaptureAperture").is_ok_and(|v| v < 6.) {
            outputs.push(ImageOutput {
                name: "blue-pattern.blue",
                channels: Channels::Scalar,
            });
        }
    }
    let calibrated = is_sd || (is_dp && super::hlp::supported(metadata));
    if calibrated && super::defects::supported(metadata) {
        outputs.extend(FACTORY_REPAIRED.into_iter().filter(|output| {
            output.name != "blue-mismatch.blue"
                || super::calibration::normalization(metadata)
                    .is_ok_and(|v| (v - 1.).abs() < f32::EPSILON)
        }));
        if metadata.scalar("CAMERAID").is_ok_and(|id| id == 80.) {
            outputs.push(ImageOutput {
                name: "purple-fringe.rgb",
                channels: Channels::Layers,
            });
        }
        if metadata
            .scalar("CaptureISO")
            .is_ok_and(|iso| iso.is_finite() && iso > 0.)
        {
            outputs.extend(RECONSTRUCTED.into_iter().filter(|output| {
                (has_shading(metadata)
                    || !matches!(
                        output.name,
                        "shading.rgb" | "shading.source-after" | "shading.blue"
                    ))
                    && (super::mcc::enabled(metadata) || output.name != "sector-color.rgb")
            }));
        }
    }
    outputs
}

fn has_shading(metadata: &Metadata) -> bool {
    metadata.lists.contains_key("ColorShadingGainRatio")
}

pub(crate) fn comparison_input(metadata: &Metadata, output: &str) -> Option<&'static str> {
    let is_sd = super::calibration::is_ordinary_sd(metadata);
    let blue = if is_sd {
        if metadata.scalar("CaptureAperture").is_ok_and(|v| v < 6.) {
            "blue-pattern.blue"
        } else {
            "phase-detection.blue"
        }
    } else {
        "hlp-repair.blue"
    };
    match output {
        "chroma-repair.single" if is_sd => return Some("phase-detection.rgb"),
        "luma-repair.directional" => return Some(blue),
        "raw-color.rgb" if !has_shading(metadata) => return Some("noise.rgb"),
        "highlight-restoration.rgb" if metadata.scalar("CAMERAID").is_ok_and(|v| v == 80.) => {
            return Some("purple-fringe.rgb")
        }
        _ => {}
    }

    match output {
        "blue-pattern.blue" => Some("phase-detection.blue"),
        "purple-fringe.rgb" => Some("chroma-clusters.rgb"),
        "phase-detection.rgb" => Some("extraction.rgb"),
        "phase-detection.blue" => Some("extraction.blue"),
        "shading.rgb" => Some("noise.rgb"),
        "shading.source-after" => Some("noise.source-after"),
        "shading.blue" => Some("blue-mismatch.blue"),
        "raw-color.rgb" => Some("shading.rgb"),
        "sector-color.rgb" => Some("raw-color.rgb"),
        "reconstruction.rgb" => Some("noise.rgb"),
        "residual.rgb" => Some("reconstruction.rgb"),
        "noise.rgb" | "noise.source-after" => Some("highlight-restoration.rgb"),
        "phase-gain.blue" => Some("extraction.blue"),
        "blue-refresh.rgb" => Some("extraction.rgb"),
        "hlp.rgb" => Some("blue-refresh.rgb"),
        "hlp.blue" => Some("phase-gain.blue"),
        "hlp-repair.rgb" => Some("hlp.rgb"),
        "hlp-repair.blue" => Some("hlp.blue"),
        "chroma-repair.single" => Some("hlp-repair.rgb"),
        "chroma-repair.rgb" => Some("chroma-repair.single"),
        "chroma-despeckle.rgb" => Some("chroma-repair.rgb"),
        "chroma-clusters.rgb" => Some("chroma-despeckle.rgb"),
        "luma-repair.directional" => Some("hlp-repair.blue"),
        "luma-repair.dark-offsets" => Some("luma-repair.directional"),
        "luma-despeckle.dark-pairs" => Some("luma-repair.dark-offsets"),
        "luma-repair.blue" => Some("luma-despeckle.dark-pairs"),
        "luma-despeckle.blue" => Some("luma-repair.blue"),
        "luma-clusters.blue" => Some("luma-despeckle.blue"),
        "blue-mismatch.blue" => Some("luma-clusters.blue"),
        "highlight-restoration.rgb" => Some("chroma-clusters.rgb"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Matrix, MatrixEncoding};

    #[test]
    fn catalogue_keeps_names_and_component_meanings_together() {
        let early = outputs(None);
        assert_eq!(
            early
                .iter()
                .map(|o| (o.name, o.channels))
                .collect::<Vec<_>>(),
            [
                ("sensor.rgb", Channels::Layers),
                ("sensor.blue", Channels::Scalar)
            ]
        );
        let mut metadata = Metadata {
            width: 5888,
            height: 3672,
            rotation: 0,
            processing_version: Some([3, 1, 2, 0]),
            properties: Default::default(),
            lists: Default::default(),
            matrices: [(
                "CAMERAID".into(),
                Matrix {
                    encoding: MatrixEncoding::Float32,
                    representation: "f64",
                    dimensions: vec![1],
                    values: vec![81.],
                },
            )]
            .into(),
        };
        let full = outputs(Some(&metadata));
        assert_eq!(
            full.iter().map(|o| o.name).collect::<Vec<_>>(),
            [
                "sensor.rgb",
                "sensor.blue",
                "extraction.rgb",
                "extraction.blue",
                "phase-gain.blue",
                "blue-refresh.rgb"
            ]
        );
        for output in full {
            assert_eq!(
                output.channels,
                if output.name.ends_with(".blue") {
                    Channels::Scalar
                } else {
                    Channels::Layers
                }
            );
        }
        metadata.matrices.remove("CAMERAID");
        assert_eq!(outputs(Some(&metadata)).len(), 4);
        metadata.processing_version = Some([4, 0, 1, 0]);
        assert_eq!(outputs(Some(&metadata)).len(), 2);
    }
}
