//! Describes named inspection artifacts and preserves the configuration behind each result.

use crate::{image::RenderedImage, render::Settings, source::Metadata};
use std::sync::Arc;

pub enum Content {
    Image(Box<RenderedImage>),
    /// Small diagnostic tables only; photographic pixels remain on the GPU.
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
    if settings.despeckle {
        names.push("despeckle.layers");
    }
    if settings.reconstruction.is_some() {
        names.extend(["noise.layers", "noise.edges", "detail.ratio"]);
    }
    names.push("color.rgb");
    if settings.color_dq_gain > 0. {
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
        names.extend(["sharpen.horizontal", "sharpen.blurred", "sharpen.rgb"]);
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
                "black.levels" | "black.histogram" => return None,
                "sensor.layers" | "calibration.layers" | "gain.layers" | "repair.layers"
                | "despeckle.layers" | "noise.layers" => Channels::Layers,
                "noise.edges" | "detail.ratio" | "detail.signal" => Channels::Scalar,
                _ => Channels::Rgb,
            };
            Some(ImageOutput { name, channels })
        })
        .collect()
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
                "despeckle.layers",
                "color.rgb",
                "tone.rgb"
            ]
        );
        settings.repair = Some(crate::render::RepairThreshold::Camera);
        settings.color_dq_gain = 1.;
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
            "sharpen.horizontal",
            "sharpen.blurred",
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
            ("noise.edges", super::Channels::Scalar),
            ("detail.ratio", super::Channels::Scalar),
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
