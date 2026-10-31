//! Shares scoped photographic recipes and user defaults between interactive loading, batch work and preview caching.

use super::Settings;
use crate::source::ProcessingFamily;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Camera-relative photographic defaults. Optional source choices remain authored as As Shot.
pub fn camera_settings(
    metadata: &crate::source::Metadata,
    rendering: Option<super::Rendering>,
) -> crate::source::Result<Settings> {
    let capabilities = super::ControlCapabilities::from_metadata(metadata)?;
    let mut settings = Settings {
        rendering: match rendering {
            Some(rendering) => rendering,
            None => capabilities.default_rendering(metadata)?,
        },
        ..Settings::default()
    };
    capabilities.apply_capture_defaults(&mut settings, metadata)?;
    Ok(settings)
}

/// Capture metadata is sufficient to initialize defaults without decoding image samples.
pub fn camera_settings_from_properties(
    properties: BTreeMap<String, String>,
) -> crate::source::Result<Settings> {
    let metadata = crate::source::Metadata {
        processing_version: None,
        width: 0,
        height: 0,
        rotation: 0,
        properties,
        matrices: Default::default(),
        lists: Default::default(),
    };
    camera_settings(&metadata, None)
}

/// Photographic sections used by scoped recipes, independently of pipeline execution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Section {
    WhiteBalance,
    Mix,
    Tone,
    ColorEditor,
    Noise,
    Detail,
    Sharpening,
    Corrections,
}
impl Section {
    pub const ALL: [Self; 8] = [
        Self::WhiteBalance,
        Self::Mix,
        Self::Tone,
        Self::ColorEditor,
        Self::Noise,
        Self::Detail,
        Self::Sharpening,
        Self::Corrections,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    All,
    Tool(Section),
}
impl Scope {
    pub fn apply(self, source: &Settings, target: &mut Settings, family: ProcessingFamily) {
        match self {
            Self::All => {
                let (quality, region, size, space, output) = (
                    target.preview_quality,
                    target.preview_region,
                    target.output_size,
                    target.tiff_color_space,
                    target.processing_sections,
                );
                *target = source.clone();
                target.preview_quality = quality;
                target.preview_region = region;
                target.output_size = size;
                target.tiff_color_space = space;
                target.processing_sections = output;
            }
            Self::Tool(Section::Mix) => target.monochrome.mix = source.monochrome.mix,
            Self::Tool(Section::WhiteBalance) => {
                target.white_balance.clone_from(&source.white_balance);
                target.white_balance_bias = source.white_balance_bias;
            }
            Self::Tool(Section::Tone) => {
                target.tone_enabled = source.tone_enabled;
                target.exposure = source.exposure;
                target.saturation = source.saturation;
                target.tone = source.tone;
                target.tonal_ranges = source.tonal_ranges;
                target.photographic.tone_method = source.photographic.tone_method;
                target.photographic.local_contrast = source.photographic.local_contrast;
            }
            Self::Tool(Section::ColorEditor) => {
                target.color_editor_enabled = source.color_editor_enabled;
                target.photographic.color_editor = source.photographic.color_editor;
            }
            Self::Tool(Section::Noise) => {
                if family == ProcessingFamily::Merrill {
                    merge_reconstruction(source, target, false);
                    target.merrill_noise_method = source.merrill_noise_method;
                } else {
                    target.reconstruction = source.reconstruction;
                    target.monochrome_smoothness = source.monochrome_smoothness;
                }
                target.monochrome.noise = source.monochrome.noise;
                target.monochrome.banding = source.monochrome.banding;
                target.color_dq_gain = source.color_dq_gain;
            }
            Self::Tool(Section::Detail) => {
                merge_reconstruction(source, target, true);
                target.monochrome.detail_enabled = source.monochrome.detail_enabled;
                target.monochrome.texture = source.monochrome.texture;
            }
            Self::Tool(Section::Sharpening) => {
                target.sharpening_enabled = source.sharpening_enabled;
                target.sharpen_amount = source.sharpen_amount;
                target.photographic.sharpening = source.photographic.sharpening;
            }
            Self::Tool(Section::Corrections) => {
                target.repair = source.repair;
                target.despeckle = source.despeckle;
                target.row_correction = source.row_correction;
                target.highlight_restoration = source.highlight_restoration;
                target.highlight_amount = source.highlight_amount;
                target.highlight_threshold_ev = source.highlight_threshold_ev;
                target.highlight_mode = source.highlight_mode;
                target.color_field = source.color_field;
            }
        }
    }
}

fn merge_reconstruction(source: &Settings, target: &mut Settings, detail: bool) {
    if source.reconstruction.is_none() && target.reconstruction.is_none() {
        return;
    }
    let disabled = crate::noise::Controls {
        noise_enabled: false,
        detail_enabled: false,
        ..Default::default()
    };
    let source = source.reconstruction.unwrap_or(disabled);
    let mut controls = target.reconstruction.unwrap_or(disabled);
    if detail {
        controls.detail_enabled = source.detail_enabled;
        controls.texture = source.texture;
    } else {
        controls.noise_enabled = source.noise_enabled;
        controls.chroma = source.chroma;
        controls.luminance = source.luminance;
        controls.banding = source.banding;
    }
    target.reconstruction = Some(controls);
}

pub(super) fn snapshot(settings: &Settings) -> Settings {
    Settings {
        preview_quality: Default::default(),
        preview_region: None,
        output_size: None,
        tiff_color_space: None,
        processing_sections: Default::default(),
        ..settings.clone()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DefaultRecipe {
    all: Option<Settings>,
    tools: BTreeMap<Section, Settings>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Defaults {
    families: BTreeMap<ProcessingFamily, DefaultRecipe>,
}
impl Defaults {
    pub fn configured(&self, family: ProcessingFamily) -> bool {
        self.families
            .get(&family)
            .is_some_and(|defaults| defaults.all.is_some() || !defaults.tools.is_empty())
    }
    pub fn is_empty(&self) -> bool {
        self.families
            .values()
            .all(|defaults| defaults.all.is_none() && defaults.tools.is_empty())
    }
    pub fn for_metadata(
        &self,
        metadata: &crate::source::Metadata,
    ) -> crate::source::Result<Settings> {
        let capabilities = super::ControlCapabilities::from_metadata(metadata)?;
        let mut settings = self.apply(capabilities.family, &camera_settings(metadata, None)?);
        capabilities.normalize(&mut settings);
        settings.validate_values()?;
        Ok(settings)
    }
    pub fn from_properties(
        &self,
        properties: &BTreeMap<String, String>,
    ) -> crate::source::Result<Settings> {
        self.for_metadata(&crate::source::Metadata {
            processing_version: None,
            width: 0,
            height: 0,
            rotation: 0,
            properties: properties.clone(),
            matrices: Default::default(),
            lists: Default::default(),
        })
    }
    pub fn apply(&self, family: ProcessingFamily, camera: &Settings) -> Settings {
        let mut settings = camera.clone();
        if let Some(defaults) = self.families.get(&family) {
            if let Some(all) = &defaults.all {
                Scope::All.apply(all, &mut settings, family);
            }
            for (section, recipe) in &defaults.tools {
                Scope::Tool(*section).apply(recipe, &mut settings, family);
            }
        }
        settings
    }
    pub fn set(&mut self, family: ProcessingFamily, scope: Scope, settings: &Settings) {
        let defaults = self.families.entry(family).or_default();
        match scope {
            Scope::All => {
                defaults.all = Some(snapshot(settings));
                defaults.tools.clear();
            }
            Scope::Tool(section) => {
                defaults.tools.insert(section, snapshot(settings));
            }
        }
    }
    pub fn clear(&mut self, family: ProcessingFamily) {
        self.families.remove(&family);
    }
}

#[cfg(test)]
#[path = "edits_test.rs"]
mod tests;
