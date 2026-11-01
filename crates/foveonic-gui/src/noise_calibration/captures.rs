//! Retains cap identities and shared metadata, discovers input files and checks source availability off-thread.

use super::{calibration, Capture, Condition, Properties};
use eframe::egui;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

const CAPTURES_FILE: &str = "noise-captures.json";
const PROPERTIES_FILE: &str = "noise-capture-properties.json";

#[derive(Clone, Default)]
pub(super) struct Dataset {
    pub captures: Vec<Capture>,
    pub properties: BTreeMap<PathBuf, Properties>,
}
impl Dataset {
    pub fn load(directory: Option<&Path>, fallback: &[Capture]) -> (Self, Option<String>) {
        let mut errors = Vec::new();
        let captures = read_optional(directory, CAPTURES_FILE)
            .unwrap_or_else(|e| {
                errors.push(e);
                None
            })
            .unwrap_or_else(|| fallback.to_vec());
        let mut properties: BTreeMap<PathBuf, Properties> =
            read_optional(directory, PROPERTIES_FILE)
                .unwrap_or_else(|e| {
                    errors.push(e);
                    None
                })
                .unwrap_or_default();
        properties.retain(|path, _| captures.iter().any(|c| &c.path == path));
        (
            Self {
                captures,
                properties,
            },
            (!errors.is_empty()).then(|| errors.join("\n")),
        )
    }
    pub fn save(&self, directory: &Path) -> foveonic::source::Result<()> {
        save_json(directory, CAPTURES_FILE, &self.captures)?;
        save_json(directory, PROPERTIES_FILE, &self.properties)
    }
    pub fn insert(&mut self, capture: Capture, properties: Properties) -> bool {
        self.properties.insert(capture.path.clone(), properties);
        if let Some(old) = self.captures.iter_mut().find(|c| c.path == capture.path) {
            if *old == capture {
                return false;
            }
            *old = capture;
        } else {
            self.captures.push(capture);
        }
        true
    }
    pub fn clear(&mut self) {
        self.captures.clear();
        self.properties.clear();
    }
    pub fn remove(&mut self, condition: &Condition) {
        self.captures.retain(|c| &c.condition != condition);
        self.properties
            .retain(|p, _| self.captures.iter().any(|c| &c.path == p));
    }
    pub fn missing_metadata(&self) -> Vec<PathBuf> {
        calibration::groups(&self.captures)
            .into_values()
            .filter(|paths| !paths.iter().any(|p| self.properties.contains_key(p)))
            .filter_map(|paths| paths.into_iter().next())
            .collect()
    }
}
fn read_optional<T: DeserializeOwned>(
    directory: Option<&Path>,
    name: &str,
) -> Result<Option<T>, String> {
    let Some(directory) = directory else {
        return Ok(None);
    };
    let file = match std::fs::File::open(directory.join(name)) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read {name}: {e}")),
    };
    serde_json::from_reader(file)
        .map(Some)
        .map_err(|e| format!("Cannot read {name}: {e}"))
}
fn save_json(directory: &Path, name: &str, value: &impl Serialize) -> foveonic::source::Result<()> {
    std::fs::create_dir_all(directory)?;
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    serde_json::to_writer(file.as_file_mut(), value)?;
    file.as_file_mut().sync_all()?;
    file.persist(directory.join(name))?;
    Ok(())
}

pub(super) fn visit(
    mut paths: Vec<PathBuf>,
    cache: &foveonic::decode_cache::Cache,
    mut loaded: impl FnMut(Result<(Capture, Properties), String>) -> bool,
) {
    let mut seen = HashSet::new();
    while let Some(path) = paths.pop() {
        let result = (|| -> foveonic::source::Result<_> {
            let canonical = path.canonicalize()?;
            if !seen.insert(canonical.clone()) {
                return Ok(None);
            }
            if canonical.is_dir() {
                for entry in std::fs::read_dir(canonical)? {
                    paths.push(entry?.path());
                }
                return Ok(None);
            }
            if !canonical
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("x3f"))
            {
                return Ok(None);
            }
            let properties = cache.capture_properties(&canonical)?;
            let capture = Capture::from_properties(canonical, &properties)?;
            Ok(Some((capture, properties)))
        })();
        let result = match result {
            Ok(None) => continue,
            Ok(Some(capture)) => Ok(capture),
            Err(e) => Err(format!("{}: {e}", path.display())),
        };
        if !loaded(result) {
            return;
        }
    }
}

#[derive(Default)]
pub(super) struct Availability {
    pub missing: BTreeSet<PathBuf>,
    receiver: Option<mpsc::Receiver<(u64, BTreeSet<PathBuf>)>>,
    generation: u64,
    checked_generation: Option<u64>,
    checked_at: Option<Instant>,
}
impl Availability {
    pub fn invalidate(&mut self) {
        self.generation += 1;
        self.checked_at = None;
    }
    pub fn ready(&self) -> bool {
        self.checked_generation == Some(self.generation)
    }
    pub fn refresh(
        &mut self,
        captures: &[Capture],
        cache: &Arc<foveonic::decode_cache::Cache>,
        ctx: &egui::Context,
    ) {
        const CHECK_INTERVAL: Duration = Duration::from_secs(2);
        if let Some(rx) = &self.receiver {
            match rx.try_recv() {
                Ok((generation, missing)) => {
                    if generation == self.generation {
                        self.missing = missing;
                        self.checked_generation = Some(generation);
                    }
                    self.receiver = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.receiver = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.receiver.is_none()
            && self
                .checked_at
                .is_none_or(|t| t.elapsed() >= CHECK_INTERVAL)
        {
            let paths: Vec<_> = captures.iter().map(|c| c.path.clone()).collect();
            let cache = cache.clone();
            let ctx = ctx.clone();
            let generation = self.generation;
            let (tx, rx) = mpsc::channel();
            self.receiver = Some(rx);
            self.checked_at = Some(Instant::now());
            std::thread::spawn(move || {
                let missing =
                    cache.background(|| paths.into_iter().filter(|p| !p.is_file()).collect());
                let _ = tx.send((generation, missing));
                ctx.request_repaint();
            });
        }
        ctx.request_repaint_after(CHECK_INTERVAL);
    }
}

#[cfg(test)]
#[path = "captures_test.rs"]
mod tests;
