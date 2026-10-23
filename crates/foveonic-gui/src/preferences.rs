//! Persists cache, organizer, scope, preview, zoom and tool preferences separately from image edits.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const APP_ID: &str = "foveonic";
const STORAGE_KEY: &str = "ui.preferences";

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Organizer {
    #[default]
    Icons,
    List,
}
/// Workspace policy. The worker resolves Smart into Fast or regional Full work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewMode {
    #[default]
    Full,
    Fast,
    Smart,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Scope {
    #[default]
    Histogram,
    Parade,
    Waveform,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoomPreset {
    Percent100,
    #[default]
    Percent200,
    Percent400,
}
impl ZoomPreset {
    pub const ALL: [Self; 3] = [Self::Percent100, Self::Percent200, Self::Percent400];

    pub fn multiple(self) -> f32 {
        match self {
            Self::Percent100 => 1.,
            Self::Percent200 => 2.,
            Self::Percent400 => 4.,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Percent100 => "100%",
            Self::Percent200 => "200%",
            Self::Percent400 => "400%",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Section {
    Navigator,
    Scopes,
    Editing,
    Metadata,
}
impl Section {
    pub const ALL: [Self; 4] = [Self::Navigator, Self::Scopes, Self::Editing, Self::Metadata];
    pub fn title(self) -> &'static str {
        match self {
            Self::Navigator => "Navigator",
            Self::Scopes => "Scopes",
            Self::Editing => "Editing",
            Self::Metadata => "Metadata",
        }
    }
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub last_folder: Option<std::path::PathBuf>,
    pub organizer: Organizer,
    pub scope: Scope,
    pub scope_viewport: bool,
    pub zoom_snap: ZoomPreset,
    pub preview_quality: PreviewMode,
    pub decode_cache: foveonic::decode_cache::Settings,
    pub sections: Vec<Section>,
    pub collapsed: BTreeSet<Section>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            last_folder: None,
            organizer: Organizer::Icons,
            scope: Scope::Histogram,
            scope_viewport: true,
            zoom_snap: ZoomPreset::default(),
            preview_quality: PreviewMode::Full,
            decode_cache: Default::default(),
            sections: Section::ALL.to_vec(),
            collapsed: BTreeSet::new(),
        }
    }
}
impl Preferences {
    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        let mut result: Self = storage
            .and_then(|s| eframe::get_value(s, STORAGE_KEY))
            .unwrap_or_default();
        result.normalize();
        result
    }
    fn normalize(&mut self) {
        let mut seen = BTreeSet::new();
        self.sections.retain(|s| seen.insert(*s));
        for section in Section::ALL {
            if seen.insert(section) {
                self.sections.push(section);
            }
        }
    }
    /// A launch path must override the saved folder. Keep a missing saved path
    /// long enough for the app to explain the problem rather than hiding it.
    pub fn startup_path(&self, explicit: Option<std::path::PathBuf>) -> Option<std::path::PathBuf> {
        explicit.or_else(|| self.last_folder.clone())
    }
    pub fn remember_folder(&mut self, path: &std::path::Path) {
        if let Ok(folder) = std::fs::canonicalize(path) {
            self.last_folder = Some(folder);
        }
    }
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, STORAGE_KEY, self);
    }
    pub fn move_section(&mut self, source: Section, target: Section, after: bool) {
        if source == target {
            return;
        }
        self.sections.retain(|s| *s != source);
        let index = self.sections.iter().position(|s| *s == target).unwrap() + usize::from(after);
        self.sections.insert(index, source);
    }
    pub fn toggle(&mut self, section: Section) {
        if !self.collapsed.remove(&section) {
            self.collapsed.insert(section);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Storage(std::collections::HashMap<String, String>);
    impl eframe::Storage for Storage {
        fn get_string(&self, k: &str) -> Option<String> {
            self.0.get(k).cloned()
        }
        fn set_string(&mut self, k: &str, v: String) {
            self.0.insert(k.into(), v);
        }
        fn flush(&mut self) {}
    }
    #[test]
    fn last_folder_survives_restart_and_explicit_launch_wins() {
        let mut prefs = Preferences::default();
        let folder = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        prefs.remember_folder(&folder);
        prefs.remember_folder(&folder.join("foveonic-nonexistent-saved-folder"));
        assert_eq!(prefs.last_folder.as_ref(), Some(&folder));
        let mut store = Storage::default();
        prefs.save(&mut store);
        let loaded = Preferences::load(Some(&store));
        assert_eq!(loaded.startup_path(None), Some(folder));
        let explicit = std::path::PathBuf::from("explicit.x3f");
        assert_eq!(loaded.startup_path(Some(explicit.clone())), Some(explicit));
    }
    #[test]
    fn zoom_snap_defaults_to_200_and_survives_restart() {
        assert_eq!(Preferences::load(None).zoom_snap, ZoomPreset::Percent200);
        let mut store = Storage::default();
        store.0.insert(STORAGE_KEY.into(), "(scope:Parade)".into());
        let loaded = Preferences::load(Some(&store));
        assert_eq!(loaded.scope, Scope::Parade);
        assert_eq!(loaded.zoom_snap, ZoomPreset::Percent200);
        for zoom_snap in ZoomPreset::ALL {
            let prefs = Preferences {
                scope: loaded.scope,
                zoom_snap,
                ..Default::default()
            };
            prefs.save(&mut store);
            assert_eq!(Preferences::load(Some(&store)), prefs);
        }
    }

    #[test]
    fn scope_viewport_defaults_on_and_explicit_off_survives_restart() {
        let mut store = Storage::default();
        store
            .0
            .insert(STORAGE_KEY.into(), "(scope:Waveform)".into());
        assert!(Preferences::load(Some(&store)).scope_viewport);
        let prefs = Preferences {
            scope_viewport: false,
            ..Default::default()
        };
        prefs.save(&mut store);
        assert_eq!(Preferences::load(Some(&store)), prefs);
    }

    #[test]
    fn layout_round_trip_and_reorder() {
        let mut prefs = Preferences {
            organizer: Organizer::List,
            scope: Scope::Parade,
            ..Default::default()
        };
        prefs.move_section(Section::Navigator, Section::Metadata, true);
        prefs.toggle(Section::Editing);
        assert_eq!(
            prefs.sections,
            [
                Section::Scopes,
                Section::Editing,
                Section::Metadata,
                Section::Navigator
            ]
        );
        let mut store = Storage::default();
        prefs.save(&mut store);
        assert_eq!(Preferences::load(Some(&store)), prefs);
        let text = &store.0[STORAGE_KEY];
        assert!(!text.contains("exposure") && !text.contains("white_balance"));
    }
    #[test]
    fn missing_corrupt_or_duplicate_preferences_use_complete_layout() {
        assert_eq!(Preferences::load(None), Preferences::default());
        let mut store = Storage::default();
        store.0.insert(STORAGE_KEY.into(), "invalid".into());
        assert_eq!(Preferences::load(Some(&store)), Preferences::default());
        store
            .0
            .insert(STORAGE_KEY.into(), "(sections:[Metadata,Metadata])".into());
        let prefs = Preferences::load(Some(&store));
        assert_eq!(
            prefs.sections,
            [
                Section::Metadata,
                Section::Navigator,
                Section::Scopes,
                Section::Editing
            ]
        );
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    #[test]
    fn preview_quality_defaults_full_and_round_trips_as_workspace_state() {
        assert_eq!(Preferences::default().preview_quality, PreviewMode::Full);
        let prefs = Preferences {
            preview_quality: PreviewMode::Fast,
            ..Default::default()
        };
        let bytes = serde_json::to_vec(&prefs).unwrap();
        assert_eq!(
            serde_json::from_slice::<Preferences>(&bytes).unwrap(),
            prefs
        );
    }
    #[test]
    fn smart_and_cache_configuration_roundtrip_without_changing_photo_settings() {
        let prefs = Preferences {
            preview_quality: PreviewMode::Smart,
            decode_cache: foveonic::decode_cache::Settings {
                enabled: false,
                max_bytes: 5_000_000_000,
                classic: true,
                merrill: false,
                quattro: true,
            },
            ..Default::default()
        };
        let encoded = serde_json::to_vec(&prefs).unwrap();
        assert_eq!(
            serde_json::from_slice::<Preferences>(&encoded).unwrap(),
            prefs
        );
        let settings = foveonic::render::Settings::new("Sunlight", "Standard");
        assert!(
            !String::from_utf8(foveonic::sidecar::encode(&settings).unwrap())
                .unwrap()
                .contains("decode_cache")
        );
    }
}
