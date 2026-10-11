//! Describes named inspection artifacts and preserves the configuration behind each result.

use crate::{image::RenderedImage, render::Settings, source::Metadata};
use std::sync::Arc;

pub enum Content {
    Image(RenderedImage),
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
    if settings.repair_badness.is_some() {
        names.push("repair.layers");
    }
    names.push("color.rgb");
    if settings.color_dq_gain > 0. {
        names.push("color-dq.rgb");
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

/// Visual image buffers offered by the GUI. Non-image node diagnostics have a separate, deferred UI.
pub fn image_output_names(settings: &Settings) -> Vec<&'static str> {
    output_names(settings)
        .into_iter()
        .filter(|name| !matches!(*name, "black.levels" | "black.histogram"))
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
            super::image_output_names(&settings),
            [
                "final",
                "sensor.layers",
                "calibration.layers",
                "color.rgb",
                "tone.rgb"
            ]
        );
        settings.repair_badness = Some(8);
        settings.color_dq_gain = 1.;
        settings.sensor_gain = 1.3;
        settings.saturation = 1.1;
        settings.sharpen_amount = 0.5;
        let images = super::image_output_names(&settings);
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
    }
}
