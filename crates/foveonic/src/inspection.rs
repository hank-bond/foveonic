//! Describes named inspection artifacts and preserves the configuration behind each result.

use crate::{image::RenderedImage, render::Settings, source::Metadata};
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
pub fn output_names(settings: &Settings) -> Vec<&'static str> {
    let mut names = vec![
        "final",
        "sensor.layers",
        "black.levels",
        "black.histogram",
        "calibration.layers",
    ];
    if settings.sensor_gain != 1. {
        names.push("gain.layers");
    }
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
    if settings.sharpen_amount > 0. {
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

pub struct ImageOutput {
    pub name: &'static str,
    pub channels: Channels,
}

/// Visual buffers and their component meanings. Components remain views of one retained image.
pub fn image_outputs(settings: &Settings) -> Vec<ImageOutput> {
    output_names(settings)
        .into_iter()
        .filter_map(|name| {
            let channels = match name {
                "black.levels" | "black.histogram" | "banding.rows" => return None,
                "sensor.layers" | "calibration.layers" | "gain.layers" | "repair.layers"
                | "despeckle.layers" | "highlight.layers" | "banding.layers" | "noise.layers"
                | "column.layers" => Channels::Layers,
                "noise.edges" | "detail.ratio" | "detail.raw-ratio" | "pattern.mask"
                | "detail.signal" | "highlight.density" => Channels::Scalar,
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
pub fn comparison_input(settings: &Settings, output: &str) -> Option<&'static str> {
    let mut input = "sensor.layers";
    for (name, enabled) in [
        ("calibration.layers", true),
        ("gain.layers", settings.sensor_gain != 1.),
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
        "sharpen.rgb" if settings.sharpen_amount > 0. => {
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
    fn comparison_inputs_follow_enabled_stages_and_branches() {
        let mut s = crate::render::Settings::new("Sunlight", "Standard");
        assert_eq!(
            super::comparison_input(&s, "repair.layers"),
            Some("calibration.layers")
        );
        assert_eq!(
            super::comparison_input(&s, "banding.layers"),
            Some("despeckle.layers")
        );
        s.sensor_gain = 1.3;
        assert_eq!(
            super::comparison_input(&s, "repair.layers"),
            Some("gain.layers")
        );
        s.repair = None;
        s.highlight_restoration = false;
        s.despeckle = false;
        s.row_correction = false;
        s.reconstruction = Some(Default::default());
        for output in ["noise.layers", "column.layers"] {
            assert_eq!(super::comparison_input(&s, output), Some("gain.layers"));
        }
        assert_eq!(
            super::comparison_input(&s, "reconstruction.rgb"),
            Some("color-dq.rgb")
        );
        s.color_dq_gain = Some(0.);
        assert_eq!(
            super::comparison_input(&s, "reconstruction.rgb"),
            Some("color.rgb")
        );
        s.sharpen_amount = 0.75;
        assert_eq!(
            super::comparison_input(&s, "sharpen.rgb"),
            Some("reconstruction.rgb")
        );
        s.reconstruction = None;
        assert_eq!(
            super::comparison_input(&s, "sharpen.rgb"),
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
            assert_eq!(super::comparison_input(&s, output), None, "{output}");
        }
        s.saturation = 1.2;
        assert_eq!(
            super::comparison_input(&s, "saturation.rgb"),
            Some("tone.rgb")
        );
        let images = super::image_outputs(&s);
        for output in &images {
            if let Some(input) = super::comparison_input(&s, output.name) {
                assert!(images.iter().any(|i| i.name == input), "{input}");
            }
        }
    }

    #[test]
    fn visual_catalog_excludes_tables_and_keeps_enabled_image_stages() {
        let mut settings = crate::render::Settings::new("Sunlight", "Standard");
        assert_eq!(
            super::image_outputs(&settings)
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
        settings.sensor_gain = 1.3;
        settings.saturation = 1.1;
        settings.sharpen_amount = 0.5;
        let images: Vec<_> = super::image_outputs(&settings)
            .iter()
            .map(|o| o.name)
            .collect();
        for name in [
            "repair.layers",
            "color-dq.rgb",
            "gain.layers",
            "saturation.rgb",
            "detail.signal",
            "sharpen.rgb",
        ] {
            assert!(images.contains(&name));
        }
        assert!(!images.contains(&"black.levels") && !images.contains(&"black.histogram"));
        assert!(super::output_names(&settings).contains(&"black.histogram")); // still available to engine consumers
        settings.reconstruction = Some(Default::default());
        let images = super::image_outputs(&settings);
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
