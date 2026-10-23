//! Keeps bounded editing recipes in a visible source-folder subdirectory with atomic replacement.

use crate::{render::Settings, source::Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(test)]
#[path = "sidecar_test.rs"]
mod tests;

const FORMAT: &str = "foveonic-editing";
const VERSION: u32 = 1;
const MAX_BYTES: u64 = 64 * 1024;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
    format: String,
    version: u32,
    settings: Settings,
}

/// Keep the complete filename and its non-UTF-8 bytes inside a visible folder.
pub fn path(input: &Path) -> PathBuf {
    let mut name = input.file_name().unwrap_or_default().to_os_string();
    name.push(".foveonic.json");
    input
        .parent()
        .unwrap_or(Path::new(""))
        .join("foveonic")
        .join(name)
}

fn legacy_path(input: &Path) -> PathBuf {
    let mut name = input.as_os_str().to_os_string();
    name.push(".foveonic.json");
    PathBuf::from(name)
}

/// Validate unresolved camera choices too: empty WB/mode means recorded X3F defaults.
pub fn encode(settings: &Settings) -> Result<Vec<u8>> {
    settings.validate_values()?;
    let mut bytes = serde_json::to_vec_pretty(&Recipe {
        format: FORMAT.into(),
        version: VERSION,
        settings: settings.clone(),
    })?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_BYTES {
        return Err("editing sidecar exceeds 64 KiB".into());
    }
    Ok(bytes)
}

pub fn decode(bytes: &[u8]) -> Result<Settings> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("editing sidecar exceeds 64 KiB".into());
    }
    // A newer version may store settings differently. Do not interpret it
    // using today's schema and then overwrite edits we cannot understand.
    let header: serde_json::Value = serde_json::from_slice(bytes)?;
    if header.get("format").and_then(|v| v.as_str()) != Some(FORMAT) {
        return Err("not a Foveonic editing recipe".into());
    }
    if header.get("version").and_then(|v| v.as_u64()) != Some(u64::from(VERSION)) {
        return Err("unsupported editing sidecar version".into());
    }
    // Parsing through Value loses duplicate fields. Read the original bytes
    // again so duplicated settings cannot slip through validation.
    let recipe: Recipe = serde_json::from_slice(bytes)?;
    recipe.settings.validate_values()?;
    Ok(recipe.settings)
}

/// A missing sidecar means start with defaults. Other errors need to reach the
/// caller so an unreadable or newer recipe is not silently replaced.
pub fn load(input: &Path) -> Result<Option<Settings>> {
    match load_at(&path(input))? {
        Some(settings) => Ok(Some(settings)),
        None => load_at(&legacy_path(input)),
    }
}

fn load_at(destination: &Path) -> Result<Option<Settings>> {
    let file = match File::open(destination) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", destination.display()).into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    decode(&bytes)
        .map(Some)
        .map_err(|error| format!("{}: {error}", destination.display()).into())
}

/// Do not overwrite a recipe we cannot read. A temporary file beside the
/// destination lets replacement stay atomic and keeps the old recipe on failure.
/// The original X3F is never opened for writing.
pub fn save(input: &Path, settings: &Settings) -> Result<()> {
    let bytes = encode(settings)?;
    load(input)?;
    let destination = path(input);
    // Create only our direct child. A missing source folder remains an error.
    match fs::create_dir(destination.parent().unwrap()) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error.into()),
    }
    let mut temporary = destination.as_os_str().to_os_string();
    temporary.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = PathBuf::from(temporary);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &destination)?;
        // The new recipe is durable before retiring an older adjacent copy.
        // A malformed older file remains available for manual recovery.
        let legacy = legacy_path(input);
        if load_at(&legacy).is_ok_and(|s| s.is_some()) {
            fs::remove_file(legacy)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
