//! Persists organizer, scope, and tool-section preferences.

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Scope {
    #[default]
    Histogram,
    Parade,
    Waveform,
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
    pub organizer: Organizer,
    pub scope: Scope,
    pub sections: Vec<Section>,
    pub collapsed: BTreeSet<Section>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            organizer: Organizer::Icons,
            scope: Scope::Histogram,
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
        assert!(
            !text.contains("exposure")
                && !text.contains("white_balance")
                && !text.contains("sensor_gain")
        );
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
