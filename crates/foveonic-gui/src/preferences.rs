//! Persists workspace, display, cache and per-mode processing preferences separately from image edits.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
// Separate review bundles can retain their own workspace and recipe defaults.
pub const APP_ID: &str = match option_env!("FOVEONIC_APP_ID") {
    Some(id) => id,
    None => "foveonic",
};
pub const APP_NAME: &str = match option_env!("FOVEONIC_APP_NAME") {
    Some(name) => name,
    None => "Foveonic",
};
const STORAGE_KEY: &str = "ui.preferences";

/// Application display limits. The window always remains host color managed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DisplayGamut {
    #[default]
    Monitor,
    AdobeRgb,
    Srgb,
    #[serde(rename = "prophoto-rgb")]
    ProPhotoRgb,
}
impl DisplayGamut {
    pub const ALL: [Self; 4] = [Self::Monitor, Self::Srgb, Self::AdobeRgb, Self::ProPhotoRgb];
    pub fn label(self) -> &'static str {
        match self {
            Self::Monitor => "Monitor",
            Self::AdobeRgb => "Adobe RGB",
            Self::Srgb => "sRGB",
            Self::ProPhotoRgb => "ProPhoto RGB (ROMM)",
        }
    }
    pub fn transform(self) -> eframe::egui_wgpu::ColorTransform {
        use foveonic::color_space::{ColorSpace, TiffColorSpace};
        let destination = match self {
            Self::Monitor => return Default::default(),
            Self::AdobeRgb => TiffColorSpace::AdobeRgb,
            Self::Srgb => TiffColorSpace::Srgb,
            Self::ProPhotoRgb => TiffColorSpace::ProPhotoRgb,
        };
        eframe::egui_wgpu::ColorTransform::new(
            destination.transform(ColorSpace::Srgb),
            TiffColorSpace::Srgb.transform(destination.into()),
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Organizer {
    #[default]
    Icons,
    List,
    Large,
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
pub use crate::editor::Section as EditingSection;

pub const MIB: u64 = 1024 * 1024;
pub const DEFAULT_THUMBNAIL_BYTES: u64 = 256 * MIB;
pub const THUMBNAIL_BYTES_RANGE: std::ops::RangeInclusive<u64> = 16 * MIB..=2048 * MIB;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub last_folder: Option<std::path::PathBuf>,
    pub last_folder_mode: FolderMode,
    pub processing_destination: Option<std::path::PathBuf>,
    pub browse_processing: crate::processing::Settings,
    pub import_processing: crate::processing::Settings,
    pub organizer: Organizer,
    pub browser_sort: crate::browser::Sort,
    pub browser_show_ratings: bool,
    #[serde(with = "crate::editor::storage")]
    pub editor: crate::editor::Library,
    pub direct_color: crate::editor::direct_color::Configuration,
    pub crop_ratios: Vec<crate::crop_tool::NamedRatio>,
    pub scope: Scope,
    pub scope_viewport: bool,
    pub zoom_snap: ZoomPreset,
    pub preview_quality: PreviewMode,
    pub white_viewport_background: bool,
    pub decode_cache: foveonic::decode_cache::Settings,
    pub thumbnail_cache_bytes: u64,
    pub noise_profile: crate::noise_calibration::Slot,
    pub display_gamut: crate::preferences::DisplayGamut,
    pub adaptive_tone_tuning: foveonic::render::AdaptiveToneTuning,
    pub sections: Vec<Section>,
    pub editing_sections: Vec<EditingSection>,
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
            browser_sort: Default::default(),
            browser_show_ratings: false,
            editor: Default::default(),
            direct_color: Default::default(),
            crop_ratios: Vec::new(),
            scope: Scope::Histogram,
            scope_viewport: true,
            zoom_snap: ZoomPreset::default(),
            preview_quality: PreviewMode::Full,
            white_viewport_background: false,
            decode_cache: Default::default(),
            thumbnail_cache_bytes: DEFAULT_THUMBNAIL_BYTES,
            noise_profile: Default::default(),
            display_gamut: Default::default(),
            adaptive_tone_tuning: Default::default(),
            sections: Section::ALL.to_vec(),
            editing_sections: EditingSection::ALL.to_vec(),
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
        if self.adaptive_tone_tuning.validate().is_err() {
            self.adaptive_tone_tuning = Default::default();
        }
        self.direct_color.normalize();
        self.thumbnail_cache_bytes = self
            .thumbnail_cache_bytes
            .clamp(*THUMBNAIL_BYTES_RANGE.start(), *THUMBNAIL_BYTES_RANGE.end());
        crate::editor::normalize_sections(&mut self.editing_sections);
        let mut seen = BTreeSet::new();
        self.sections.retain(|s| seen.insert(*s));
        for section in Section::ALL {
            if seen.insert(section) {
                self.sections.push(section);
            }
        }
    }
    /// Resolve startup intent without probing the filesystem on the UI thread.
    pub fn startup_request(
        &self,
        explicit: Option<std::path::PathBuf>,
    ) -> Option<(std::path::PathBuf, crate::folder_loader::Mode)> {
        if let Some(path) = explicit {
            Some((
                path,
                crate::folder_loader::Mode::Explicit {
                    previous_folder: self.last_folder.clone(),
                    previous_mode: self.last_folder_mode,
                },
            ))
        } else {
            self.last_folder.clone().map(|path| {
                (
                    path,
                    crate::folder_loader::Mode::Fixed(self.last_folder_mode),
                )
            })
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
    fn display_gamut_defaults_for_existing_preferences_and_survives_restart() {
        let mut store = Storage::default();
        store
            .0
            .insert(STORAGE_KEY.into(), "(scope:Histogram)".into());
        assert_eq!(
            Preferences::load(Some(&store)).display_gamut,
            crate::preferences::DisplayGamut::Monitor
        );
        for gamut in crate::preferences::DisplayGamut::ALL {
            let prefs = Preferences {
                display_gamut: gamut,
                ..Default::default()
            };
            prefs.save(&mut store);
            assert_eq!(Preferences::load(Some(&store)).display_gamut, gamut);
        }
    }
    #[test]
    fn startup_request_preserves_remembered_mode_and_explicit_precedence() {
        let folder = std::path::PathBuf::from("card");
        let prefs = Preferences {
            last_folder: Some(folder.clone()),
            last_folder_mode: FolderMode::Import,
            ..Default::default()
        };
        let mut store = Storage::default();
        prefs.save(&mut store);
        let loaded = Preferences::load(Some(&store));
        assert_eq!(
            loaded.startup_request(None),
            Some((
                folder.clone(),
                crate::folder_loader::Mode::Fixed(FolderMode::Import)
            ))
        );
        let explicit = std::path::PathBuf::from("explicit.x3f");
        assert_eq!(
            loaded.startup_request(Some(explicit.clone())),
            Some((
                explicit,
                crate::folder_loader::Mode::Explicit {
                    previous_folder: Some(folder),
                    previous_mode: FolderMode::Import
                }
            ))
        );
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
            "(processing_destination:Some(\"photos/remembered\"))".into(),
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
    fn browser_view_and_numeric_sort_survive_restart_without_persisting_queries() {
        assert!(!Preferences::default().browser_show_ratings);
        let mut store = Storage::default();
        let prefs = Preferences {
            organizer: Organizer::Large,
            browser_show_ratings: true,
            browser_sort: crate::browser::Sort {
                field: crate::browser::Field::Iso,
                reverse: true,
            },
            ..Default::default()
        };
        prefs.save(&mut store);
        assert_eq!(Preferences::load(Some(&store)), prefs);
        assert!(!store.0[STORAGE_KEY].contains("ratings.json"));
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
    #[test]
    fn named_section_presets_and_family_defaults_survive_application_storage() {
        let mut prefs = Preferences::default();
        let recipe = foveonic::render::Settings {
            exposure: 0.4,
            ..Default::default()
        };
        prefs
            .editor
            .save(
                foveonic::source::ProcessingFamily::Merrill,
                crate::editor::Scope::All,
                "Portrait",
                &recipe,
            )
            .unwrap();
        prefs.editor.defaults.set(
            foveonic::source::ProcessingFamily::Merrill,
            crate::editor::Scope::Tool(crate::editor::Section::Tone),
            &recipe,
        );
        let mut storage = Storage::default();
        prefs.save(&mut storage);
        assert_eq!(Preferences::load(Some(&storage)), prefs);
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    #[test]
    fn thumbnail_memory_defaults_and_persists_independently() {
        let old: Preferences = serde_json::from_str("{}").unwrap();
        assert_eq!(old.thumbnail_cache_bytes, 256 * MIB);
        let prefs = Preferences {
            thumbnail_cache_bytes: 512 * MIB,
            ..Default::default()
        };
        let bytes = serde_json::to_vec(&prefs).unwrap();
        assert_eq!(
            serde_json::from_slice::<Preferences>(&bytes).unwrap(),
            prefs
        );
        let mut invalid = Preferences {
            thumbnail_cache_bytes: 0,
            ..Default::default()
        };
        invalid.normalize();
        assert_eq!(
            invalid.thumbnail_cache_bytes,
            *THUMBNAIL_BYTES_RANGE.start()
        );
        invalid.thumbnail_cache_bytes = u64::MAX;
        invalid.normalize();
        assert_eq!(invalid.thumbnail_cache_bytes, *THUMBNAIL_BYTES_RANGE.end());
    }
    #[test]
    fn preview_quality_defaults_full_and_round_trips_as_workspace_state() {
        assert_eq!(Preferences::default().preview_quality, PreviewMode::Full);
        let prefs = Preferences {
            preview_quality: PreviewMode::Fast,
            white_viewport_background: true,
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

#[cfg(test)]
mod editing_order_tests {
    use super::*;
    #[test]
    fn editing_order_round_trips_and_restores_missing_hidden_sections() {
        let mut prefs = Preferences::default();
        assert_eq!(
            prefs.editing_sections[2..5],
            [
                EditingSection::Tone,
                EditingSection::Saturation,
                EditingSection::ColorEditor
            ]
        );
        crate::editor::move_section(
            &mut prefs.editing_sections,
            EditingSection::ColorEditor,
            EditingSection::Corrections,
            true,
        );
        let saved = serde_json::to_vec(&prefs).unwrap();
        let mut restored: Preferences = serde_json::from_slice(&saved).unwrap();
        assert_eq!(restored.editing_sections, prefs.editing_sections);
        restored.editing_sections = vec![
            EditingSection::ColorEditor,
            EditingSection::Tone,
            EditingSection::ColorEditor,
        ];
        restored.normalize();
        assert_eq!(restored.editing_sections.len(), EditingSection::ALL.len());
        assert_eq!(
            &restored.editing_sections[..3],
            &[
                EditingSection::WhiteBalance,
                EditingSection::ColorEditor,
                EditingSection::Tone
            ]
        );
    }
}
