//! Describes named inspection artifacts and preserves the configuration behind each result.

use crate::{
    image::RenderedImage,
    render::Settings,
    source::{Metadata, ProcessingFamily},
};
use std::sync::Arc;

pub enum Content {
    Image(Box<RenderedImage>),
    /// Small diagnostic tables only. Photo samples stay on the GPU.
    Table {
        columns: usize,
        values: Vec<f32>,
    },
}

/// Named outputs available for the selected recipe, independent of transient graph indexes.
pub fn output_names(settings: &Settings, metadata: Option<&Metadata>) -> Vec<&'static str> {
    if metadata.is_some_and(|m| {
        m.processing_family().ok() == Some(crate::source::ProcessingFamily::Quattro)
    }) {
        return crate::render::quattro_inspection_outputs(metadata)
            .into_iter()
            .map(|o| o.name)
            .collect();
    }
    if let Some(metadata) = metadata
        .filter(|m| m.processing_family().ok() == Some(crate::source::ProcessingFamily::Classic))
    {
        return crate::render::classic_inspection_outputs(metadata, settings);
    }
    let mut names = vec![
        "final",
        "sensor.layers",
        "black.levels",
        "black.histogram",
        "calibration.layers",
    ];
    if settings.repair.is_some() {
        names.push("repair.layers");
    }
    if settings.highlight_restoration {
        names.extend(["highlight.layers", "highlight.density"]);
    }
    if settings.despeckle {
        names.push("despeckle.layers");
    }
    if settings.row_correction {
        names.extend(["banding.layers", "banding.rows"]);
    }
    if settings.is_monochrome() {
        names.extend(["mono.mix", "mono.directional", "mono.column"]);
        if settings.monochrome.noise > 0 {
            names.push("mono.filtered");
        }
        if settings.despeckle {
            names.push("mono.despeckle");
        }
        names.extend(["detail.signal", "monochrome.rgb"]);
        if settings.sharpen_selected() {
            names.push("sharpen.rgb");
        }
        names.push("tone.rgb");
        return names;
    }
    if settings.reconstruction.is_some() {
        names.extend([
            "noise.layers",
            "noise.edges",
            "column.layers",
            "detail.raw-ratio",
            "pattern.mask",
            "detail.ratio",
        ]);
    }
    names.push("color.rgb");
    if settings.color_dq_selected() {
        names.push("color-dq.rgb");
    }
    if settings.reconstruction.is_some() {
        names.extend(["reconstruction.rgb", "detail.signal"]);
    }
    names.push("tone.rgb");
    if settings.saturation != 1. {
        names.push("saturation.rgb");
    }
    if settings.sharpen_selected() {
        if settings.reconstruction.is_none() {
            names.push("detail.signal");
        }
        names.push("sharpen.rgb");
    }
    names
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channels {
    Layers,
    Rgb,
    Scalar,
}

#[derive(Clone, Copy, Debug)]
pub struct ImageOutput {
    pub name: &'static str,
    pub channels: Channels,
}

/// Visual buffers and their component meanings. Components remain views of one retained image.
pub fn image_outputs(settings: &Settings, metadata: Option<&Metadata>) -> Vec<ImageOutput> {
    image_outputs_with_family(settings, metadata, None)
}

/// Use early capture identity until complete processing metadata becomes available.
/// A family hint exposes only planes established by decoding, without guessing calibration.
pub fn image_outputs_with_family(
    settings: &Settings,
    metadata: Option<&Metadata>,
    family: Option<ProcessingFamily>,
) -> Vec<ImageOutput> {
    let family = if let Some(metadata) = metadata {
        metadata.processing_family().ok()
    } else {
        family
    };
    if family == Some(ProcessingFamily::Quattro) {
        return crate::render::quattro_inspection_outputs(metadata);
    }
    let names = output_names(settings, metadata);
    names
        .into_iter()
        .filter_map(|name| {
            let channels = match name {
                "black.levels" | "black.histogram" | "banding.rows" => return None,
                "sensor.layers"
                | "calibration.layers"
                | "repair.layers"
                | "despeckle.layers"
                | "highlight.layers"
                | "banding.layers"
                | "noise.layers"
                | "column.layers"
                | "red-detail.layers"
                | "chroma.small-layers"
                | "chroma.layers"
                | "color.corrected-layers" => Channels::Layers,
                "noise.edges" | "detail.ratio" | "detail.raw-ratio" | "pattern.mask"
                | "detail.signal" | "highlight.density" | "mono.mix" | "mono.directional"
                | "mono.column" | "mono.filtered" | "mono.despeckle" => Channels::Scalar,
                _ => Channels::Rgb,
            };
            Some(ImageOutput { name, channels })
        })
        .collect()
}

/// Compare against the operation's actual input, not the previous item in the UI.
/// The graph branches, so neighboring list entries may not be related. Keep both
/// sides in the same units. A tone or color-space change would otherwise look
/// like correction detail rather than a change in how values are represented.
pub fn comparison_input(
    settings: &Settings,
    output: &str,
    metadata: Option<&Metadata>,
) -> Option<&'static str> {
    if metadata.is_some_and(|m| {
        m.processing_family().ok() == Some(crate::source::ProcessingFamily::Quattro)
    }) {
        return (output == "phase-gain.blue").then_some("extraction.blue");
    }
    if let Some(metadata) = metadata
        .filter(|m| m.processing_family().ok() == Some(crate::source::ProcessingFamily::Classic))
    {
        return crate::render::classic_inspection_comparison(metadata, settings, output);
    }
    let mut input = "sensor.layers";
    for (name, enabled) in [
        ("calibration.layers", true),
        ("repair.layers", settings.repair.is_some()),
        ("highlight.layers", settings.highlight_restoration),
        ("despeckle.layers", settings.despeckle),
        ("banding.layers", settings.row_correction),
    ] {
        if enabled {
            if output == name {
                return Some(input);
            }
            input = name;
        }
    }
    if settings.is_monochrome() {
        return match output {
            "mono.directional" | "mono.column" => Some("mono.mix"),
            "mono.filtered" if settings.monochrome.noise > 0 => Some("mono.column"),
            "mono.despeckle" if settings.despeckle => Some(if settings.monochrome.noise > 0 {
                "mono.filtered"
            } else {
                "mono.column"
            }),
            "sharpen.rgb" if settings.sharpen_selected() => Some("monochrome.rgb"),
            _ => None,
        };
    }
    if settings.reconstruction.is_some() && matches!(output, "noise.layers" | "column.layers") {
        return Some(input);
    }
    let base = if settings.color_dq_selected() {
        "color-dq.rgb"
    } else {
        "color.rgb"
    };
    match output {
        "color-dq.rgb" if settings.color_dq_selected() => Some("color.rgb"),
        "reconstruction.rgb" if settings.reconstruction.is_some() => Some(base),
        "sharpen.rgb" if settings.sharpen_selected() => {
            Some(if settings.reconstruction.is_some() {
                "reconstruction.rgb"
            } else {
                base
            })
        }
        "saturation.rgb" if settings.saturation != 1. => Some("tone.rgb"),
        _ => None,
    }
}

pub struct Artifact {
    pub name: &'static str,
    pub metadata: Arc<Metadata>,
    pub settings: Settings,
    pub content: Content,
}

#[cfg(test)]
mod tests {
    #[test]
    fn monochrome_outputs_ignore_retained_color_controls() {
        let mut s = crate::render::Settings::new("Sunlight", "Vivid");
        s.rendering = crate::render::Rendering::Monochrome;
        s.reconstruction = Some(Default::default());
        s.sharpen_amount = Some(0.5);
        s.saturation = 0.;
        let names = super::output_names(&s, None);
        for name in [
            "mono.mix",
            "mono.directional",
            "mono.column",
            "mono.filtered",
            "mono.despeckle",
            "detail.signal",
            "monochrome.rgb",
            "sharpen.rgb",
            "tone.rgb",
            "final",
        ] {
            assert!(names.contains(&name));
        }
        for name in [
            "color.rgb",
            "color-dq.rgb",
            "reconstruction.rgb",
            "noise.layers",
            "detail.ratio",
            "saturation.rgb",
        ] {
            assert!(!names.contains(&name));
        }
        assert_eq!(
            super::comparison_input(&s, "sharpen.rgb", None),
            Some("monochrome.rgb")
        );
        assert_eq!(
            super::comparison_input(&s, "mono.despeckle", None),
            Some("mono.filtered")
        );
        s.monochrome.noise = 0;
        assert!(!super::output_names(&s, None).contains(&"mono.filtered"));
        assert_eq!(
            super::comparison_input(&s, "mono.despeckle", None),
            Some("mono.column")
        );
        assert!(super::image_outputs(&s, None)
            .iter()
            .filter(|o| o.name.starts_with("mono."))
            .all(|o| o.channels == super::Channels::Scalar));
    }

    #[test]
    fn comparison_inputs_follow_enabled_stages_and_branches() {
        let mut s = crate::render::Settings::new("Sunlight", "Standard");
        assert_eq!(
            super::comparison_input(&s, "repair.layers", None),
            Some("calibration.layers")
        );
        assert_eq!(
            super::comparison_input(&s, "banding.layers", None),
            Some("despeckle.layers")
        );
        s.repair = None;
        s.highlight_restoration = false;
        s.despeckle = false;
        s.row_correction = false;
        s.reconstruction = Some(Default::default());
        for output in ["noise.layers", "column.layers"] {
            assert_eq!(
                super::comparison_input(&s, output, None),
                Some("calibration.layers")
            );
        }
        assert_eq!(
            super::comparison_input(&s, "reconstruction.rgb", None),
            Some("color-dq.rgb")
        );
        s.color_dq_gain = Some(0.);
        assert_eq!(
            super::comparison_input(&s, "reconstruction.rgb", None),
            Some("color.rgb")
        );
        s.sharpen_amount = Some(0.75);
        assert_eq!(
            super::comparison_input(&s, "sharpen.rgb", None),
            Some("reconstruction.rgb")
        );
        s.reconstruction = None;
        assert_eq!(
            super::comparison_input(&s, "sharpen.rgb", None),
            Some("color.rgb")
        );
        for output in [
            "sensor.layers",
            "final",
            "color.rgb",
            "tone.rgb",
            "noise.edges",
            "detail.ratio",
            "repair.layers",
        ] {
            assert_eq!(super::comparison_input(&s, output, None), None, "{output}");
        }
        s.saturation = 1.2;
        assert_eq!(
            super::comparison_input(&s, "saturation.rgb", None),
            Some("tone.rgb")
        );
        let images = super::image_outputs(&s, None);
        for output in &images {
            if let Some(input) = super::comparison_input(&s, output.name, None) {
                assert!(images.iter().any(|i| i.name == input), "{input}");
            }
        }
    }

    #[test]
    fn visual_catalog_excludes_tables_and_keeps_enabled_image_stages() {
        let mut settings = crate::render::Settings::new("Sunlight", "Standard");
        assert_eq!(
            super::image_outputs(&settings, None)
                .iter()
                .map(|o| o.name)
                .collect::<Vec<_>>(),
            [
                "final",
                "sensor.layers",
                "calibration.layers",
                "repair.layers",
                "highlight.layers",
                "highlight.density",
                "despeckle.layers",
                "banding.layers",
                "color.rgb",
                "tone.rgb"
            ]
        );
        settings.repair = Some(crate::render::RepairThreshold::Camera);
        settings.color_dq_gain = Some(1.);
        settings.saturation = 1.1;
        settings.sharpen_amount = Some(0.5);
        let images: Vec<_> = super::image_outputs(&settings, None)
            .iter()
            .map(|o| o.name)
            .collect();
        for name in [
            "repair.layers",
            "color-dq.rgb",
            "saturation.rgb",
            "detail.signal",
            "sharpen.rgb",
        ] {
            assert!(images.contains(&name));
        }
        assert!(!images.contains(&"black.levels") && !images.contains(&"black.histogram"));
        assert!(super::output_names(&settings, None).contains(&"black.histogram")); // still available to engine consumers
        settings.reconstruction = Some(Default::default());
        let images = super::image_outputs(&settings, None);
        for (name, kind) in [
            ("noise.layers", super::Channels::Layers),
            ("column.layers", super::Channels::Layers),
            ("noise.edges", super::Channels::Scalar),
            ("detail.ratio", super::Channels::Scalar),
            ("detail.raw-ratio", super::Channels::Scalar),
            ("pattern.mask", super::Channels::Scalar),
            ("detail.signal", super::Channels::Scalar),
            ("reconstruction.rgb", super::Channels::Rgb),
        ] {
            assert_eq!(
                images.iter().find(|o| o.name == name).unwrap().channels,
                kind
            );
        }
    }
}

#[cfg(test)]
mod quattro_tests {
    use super::*;

    #[test]
    fn early_family_catalogue_is_decoded_only_and_metadata_remains_authoritative() {
        let settings = Settings::new("Sunlight", "Standard");
        let early = image_outputs_with_family(&settings, None, Some(ProcessingFamily::Quattro));
        assert_eq!(
            early.iter().map(|o| o.name).collect::<Vec<_>>(),
            ["sensor.rgb", "sensor.blue"]
        );
        let mut metadata = Metadata {
            width: 5888,
            height: 3672,
            rotation: 0,
            processing_version: Some([3, 1, 2, 0]),
            properties: [("CAMMODEL".into(), "SIGMA dp2 Quattro".into())].into(),
            matrices: [(
                "CAMERAID".into(),
                crate::source::Matrix {
                    encoding: crate::source::MatrixEncoding::Float32,
                    representation: "f64",
                    dimensions: vec![1],
                    values: vec![81.],
                },
            )]
            .into(),
            lists: Default::default(),
        };
        let full =
            image_outputs_with_family(&settings, Some(&metadata), Some(ProcessingFamily::Merrill));
        for image in full {
            let expected = if image.name.ends_with(".blue") {
                Channels::Scalar
            } else {
                Channels::Layers
            };
            assert_eq!(image.channels, expected, "{}", image.name);
        }
        metadata
            .properties
            .insert("CAMMODEL".into(), "SIGMA DP2 Merrill".into());
        let full =
            image_outputs_with_family(&settings, Some(&metadata), Some(ProcessingFamily::Quattro));
        assert!(full.iter().any(|o| o.name == "final"));
        assert!(!full.iter().any(|o| o.name == "sensor.blue"));
    }
}
