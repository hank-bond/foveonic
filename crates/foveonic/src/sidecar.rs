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
struct Recipe {
    format: String,
    version: u32,
    settings: Settings,
}

#[derive(Debug)]
struct SidecarError {
    path: PathBuf,
    source: Box<dyn std::error::Error + Send + Sync>,
}
impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.source)
    }
}
impl std::error::Error for SidecarError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
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
    if crate::diagnostics::enabled() {
        schema_diagnostics(&header, &recipe);
    }
    Ok(recipe.settings)
}

/// Field differences are diagnostic metadata only. Ordinary loads need no model
/// projection or field inventory, and the shared Serde models supply all defaults.
/// The projection reports fields not retained on save, including empty optional labels.
fn schema_diagnostics(input: &serde_json::Value, recipe: &Recipe) {
    let Ok(model) = serde_json::to_value(recipe) else {
        return;
    };
    let mut defaulted = Vec::new();
    let mut ignored = Vec::new();
    schema_fields(input, &model, "", &mut defaulted, &mut ignored);
    if !defaulted.is_empty() || !ignored.is_empty() {
        crate::diagnostics::event(
            crate::diagnostics::Category::Cpu,
            "sidecar.schema",
            |fields| {
                fields
                    .str("defaulted", &serde_json::json!(defaulted).to_string())
                    .str("not_retained", &serde_json::json!(ignored).to_string());
            },
        );
    }
}

fn schema_fields(
    input: &serde_json::Value,
    model: &serde_json::Value,
    path: &str,
    defaulted: &mut Vec<String>,
    ignored: &mut Vec<String>,
) {
    match (input, model) {
        (serde_json::Value::Object(input), serde_json::Value::Object(model)) => {
            let field_path =
                |name: &str| format!("{path}/{}", name.replace('~', "~0").replace('/', "~1"));
            for (name, value) in model {
                if let Some(supplied) = input.get(name) {
                    schema_fields(supplied, value, &field_path(name), defaulted, ignored);
                } else {
                    defaulted.push(field_path(name));
                }
            }
            for name in input.keys().filter(|name| !model.contains_key(*name)) {
                ignored.push(field_path(name));
            }
        }
        (serde_json::Value::Array(input), serde_json::Value::Array(model)) => {
            for (index, (supplied, value)) in input.iter().zip(model).enumerate() {
                schema_fields(
                    supplied,
                    value,
                    &format!("{path}/{index}"),
                    defaulted,
                    ignored,
                );
            }
        }
        _ => {}
    }
}

/// A missing sidecar means start with defaults. Other errors need to reach the
/// caller so an unreadable or newer recipe is not silently replaced.
pub fn load(input: &Path) -> Result<Option<Settings>> {
    load_at(&path(input))
}

fn load_at(destination: &Path) -> Result<Option<Settings>> {
    let file = match File::open(destination) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(Box::new(SidecarError {
                path: destination.into(),
                source: Box::new(error),
            }))
        }
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| SidecarError {
            path: destination.into(),
            source: Box::new(error),
        })?;
    decode(&bytes).map(Some).map_err(|error| {
        Box::new(SidecarError {
            path: destination.into(),
            source: error,
        }) as _
    })
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
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
