//! Reads and atomically replaces bounded versioned JSON editing recipes beside immutable X3F inputs.

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

/// Append to the complete filename, preserving its extension and non-UTF-8 bytes.
pub fn path(input: &Path) -> PathBuf {
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
    // Check the version before interpreting a potentially different settings schema.
    let header: serde_json::Value = serde_json::from_slice(bytes)?;
    if header.get("format").and_then(|v| v.as_str()) != Some(FORMAT) {
        return Err("not a Foveonic editing recipe".into());
    }
    if header.get("version").and_then(|v| v.as_u64()) != Some(u64::from(VERSION)) {
        return Err("unsupported editing sidecar version".into());
    }
    // Parse the original bytes again so duplicate struct fields are rejected too.
    let recipe: Recipe = serde_json::from_slice(bytes)?;
    recipe.settings.validate_values()?;
    Ok(recipe.settings)
}

/// Missing is neutral; all other errors must be surfaced instead of silently replacing a recipe.
pub fn load(input: &Path) -> Result<Option<Settings>> {
    let destination = path(input);
    let file = match File::open(&destination) {
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

/// Replace only a missing or valid recipe. The temporary file lives on the same filesystem.
/// No X3F bytes are opened for writing, and a failed replacement leaves the old recipe intact.
pub fn save(input: &Path, settings: &Settings) -> Result<()> {
    let bytes = encode(settings)?;
    load(input)?;
    let destination = path(input);
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
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
