//! Persists workspace, cache and per-mode processing preferences separately from image edits.

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
/// Source-write policy chosen when opening a folder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FolderMode {
    #[default]
    Browse,
    Import,
}
/// Workspace policy. Smart starts with Fast and refines to Full when viewing detail.
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
    pub last_folder_mode: FolderMode,
    // Keep the persisted key stable while both modes share the destination.
    #[serde(rename = "import_destination")]
    pub processing_destination: Option<std::path::PathBuf>,
    pub browse_processing: crate::processing::Settings,
    pub import_processing: crate::processing::Settings,
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
            last_folder_mode: FolderMode::Browse,
            processing_destination: None,
            browse_processing: Default::default(),
            import_processing: Default::default(),
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
        self.decode_cache.normalize();
        let mut seen = BTreeSet::new();
        self.sections.retain(|s| seen.insert(*s));
        for section in Section::ALL {
            if seen.insert(section) {
                self.sections.push(section);
            }
        }
    }
    /// An explicit path takes precedence. Reopening the remembered folder keeps
    /// its source-write policy, including when launching one of its X3Fs.
    pub fn startup_folder(
        &self,
        explicit: Option<std::path::PathBuf>,
    ) -> Option<(std::path::PathBuf, FolderMode)> {
        match explicit {
            Some(path) => {
                let folder = if path.is_dir() {
                    path.as_path()
                } else {
                    path.parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .unwrap_or(std::path::Path::new("."))
                };
                let mode = if std::fs::canonicalize(folder).ok().is_some_and(|folder| {
                    self.last_folder
                        .as_ref()
                        .is_some_and(|root| folder.starts_with(root))
                }) {
                    self.last_folder_mode
                } else {
                    FolderMode::Browse
                };
                Some((path, mode))
            }
            None => self
                .last_folder
                .clone()
                .map(|path| (path, self.last_folder_mode)),
        }
    }
    pub fn remember_folder(&mut self, path: &std::path::Path, mode: FolderMode) {
        if let Ok(folder) = std::fs::canonicalize(path) {
            self.last_folder = Some(folder);
            self.last_folder_mode = mode;
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
        prefs.remember_folder(&folder, FolderMode::Browse);
        prefs.remember_folder(
            &folder.join("foveonic-nonexistent-saved-folder"),
            FolderMode::Import,
        );
        assert_eq!(prefs.last_folder.as_ref(), Some(&folder));
        let mut store = Storage::default();
        prefs.save(&mut store);
        let loaded = Preferences::load(Some(&store));
        assert_eq!(
            loaded.startup_folder(None),
            Some((folder, FolderMode::Browse))
        );
        let explicit = std::path::PathBuf::from("explicit.x3f");
        assert_eq!(
            loaded.startup_folder(Some(explicit.clone())),
            Some((explicit, FolderMode::Browse))
        );
    }
    #[test]
    fn remembered_import_folder_keeps_its_write_policy_on_restart_and_direct_file_launch() {
        let directory = tempfile::tempdir().unwrap();
        let card = directory.path().join("card");
        let library = directory.path().join("library");
        std::fs::create_dir(&card).unwrap();
        std::fs::create_dir(&library).unwrap();
        let card = std::fs::canonicalize(card).unwrap();
        let mut prefs = Preferences::default();
        prefs.remember_folder(&card, FolderMode::Import);
        let mut store = Storage::default();
        prefs.save(&mut store);
        let mut restored = Preferences::load(Some(&store));
        assert_eq!(
            restored.startup_folder(None),
            Some((card.clone(), FolderMode::Import))
        );
        let nested = card.join("DCIM").join("100SIGMA");
        std::fs::create_dir_all(&nested).unwrap();
        for explicit in [
            card.clone(),
            card.join("photo.x3f"),
            nested.join("photo.x3f"),
        ] {
            assert_eq!(
                restored.startup_folder(Some(explicit.clone())),
                Some((explicit, FolderMode::Import))
            );
        }
        assert_eq!(
            restored.startup_folder(Some(library.clone())),
            Some((library, FolderMode::Browse))
        );
        restored.remember_folder(&directory.path().join("missing"), FolderMode::Browse);
        assert_eq!(restored.last_folder_mode, FolderMode::Import);
        store
            .0
            .insert(STORAGE_KEY.into(), "(last_folder:Some(\"card\"))".into());
        assert_eq!(
            Preferences::load(Some(&store)).last_folder_mode,
            FolderMode::Browse
        );
    }

    #[test]
    fn processing_destination_survives_restart_independently_of_the_source_folder() {
        let mut store = Storage::default();
        store
            .0
            .insert(STORAGE_KEY.into(), "(organizer:List)".into());
        assert_eq!(Preferences::load(Some(&store)).processing_destination, None);
        store.0.insert(
            STORAGE_KEY.into(),
            "(import_destination:Some(\"photos/remembered\"))".into(),
        );
        assert_eq!(
            Preferences::load(Some(&store)).processing_destination,
            Some("photos/remembered".into())
        );
        let prefs = Preferences {
            last_folder: Some("card/DCIM".into()),
            processing_destination: Some("photos/selected".into()),
            ..Default::default()
        };
        prefs.save(&mut store);
        assert_eq!(Preferences::load(Some(&store)), prefs);
    }
    #[test]
    fn processing_choices_default_to_dng_and_remain_independent_across_restart() {
        use crate::export::ExportFormat;
        use foveonic::render::TiffColorSpace;
        let mut store = Storage::default();
        store
            .0
            .insert(STORAGE_KEY.into(), "(organizer:List)".into());
        let mut prefs = Preferences::load(Some(&store));
        assert_eq!(prefs.browse_processing.format, ExportFormat::Dng);
        assert_eq!(prefs.import_processing.format, ExportFormat::Dng);
        assert!(!prefs.import_processing.copy_only);
        prefs.browse_processing.format = ExportFormat::Tiff;
        prefs.browse_processing.tiff_color_space = Some(TiffColorSpace::AdobeRgb);
        prefs.import_processing.copy_only = true;
        prefs.save(&mut store);
        let restored = Preferences::load(Some(&store));
        assert_eq!(restored, prefs);
        assert_eq!(restored.import_processing.format, ExportFormat::Dng);
        assert_eq!(restored.import_processing.tiff_color_space, None);
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
                decode_threads: 1,
                prefetch_behind: 2,
                prefetch_ahead: 5,
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
