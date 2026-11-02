//! Stores named scoped presets and camera-family defaults in application preferences.

use super::Section;
pub use foveonic::render::{EditScope as Scope, RecipeDefaults as Defaults};
use foveonic::{render::Settings, source::ProcessingFamily};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub family: ProcessingFamily,
    pub scope: Scope,
    pub settings: Settings,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub defaults: Defaults,
    pub presets: Vec<Preset>,
}
impl Library {
    pub fn save(
        &mut self,
        family: ProcessingFamily,
        scope: Scope,
        name: &str,
        settings: &Settings,
    ) -> foveonic::source::Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err("preset name cannot be empty".into());
        }
        settings.validate_values()?;
        let preset = Preset {
            name: name.into(),
            family,
            scope,
            settings: settings.photographic_recipe(),
        };
        if let Some(old) = self
            .presets
            .iter_mut()
            .find(|old| old.name == name && old.family == family && old.scope == scope)
        {
            *old = preset;
        } else {
            self.presets.push(preset);
        }
        Ok(())
    }
    pub fn choices(
        &self,
        family: ProcessingFamily,
        scope: Scope,
    ) -> impl Iterator<Item = (usize, &Preset)> {
        self.presets.iter().enumerate().filter(move |(_, preset)| {
            preset.scope == scope
                && (preset.family == family
                    || matches!(
                        scope,
                        Scope::Tool(
                            Section::Tone
                                | Section::Saturation
                                | Section::ColorEditor
                                | Section::Sharpening
                        )
                    ))
        })
    }
}

/// Photo recipes have JSON compatibility readers. Preserve that representation
/// inside RON application preferences instead of asking RON to interpret them.
pub mod storage {
    use super::Library;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(library: &Library, serializer: S) -> Result<S::Ok, S::Error> {
        serializer
            .serialize_str(&serde_json::to_string(library).map_err(serde::ser::Error::custom)?)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Library, D::Error> {
        let encoded = String::deserialize(deserializer)?;
        serde_json::from_str(&encoded).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "recipes_test.rs"]
mod tests;
