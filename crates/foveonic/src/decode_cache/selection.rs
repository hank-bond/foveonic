//! Keeps complete local selection metadata and thumbnails beside decoded sources.

use super::{lru, previews, Cache, Identity, Settings, TEMP_SEQUENCE};
use crate::{
    preview::Preview,
    render, sidecar,
    source::{Metadata, ProcessingFamily, Result},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path, sync::atomic::Ordering};

const THUMBNAIL_SIDE: u32 = 256;
const MAX_SELECTION_BYTES: u64 = 4 * 1024 * 1024;
// Version 3 retains RGB16 samples and their declared primaries.
const FORMAT_VERSION: u32 = 3;
const CHECKSUM_BYTES: usize = size_of::<u32>();

#[derive(Serialize, Deserialize)]
pub struct Selection {
    pub properties: BTreeMap<String, String>,
    pub settings: Option<render::Settings>,
    pub thumbnail: Option<Preview>,
    pub(super) original_checksum: u32,
}
#[derive(Serialize, Deserialize)]
struct Record {
    version: u32,
    identity: Identity,
    selection: Selection,
}

impl Cache {
    /// A ready entry uses its background-verified identity. No source file is opened or statted.
    pub fn selection(&self, path: &Path) -> Option<Selection> {
        let (identity, settings, recipe) = {
            let control = self.control.lock().unwrap();
            (
                control.identities.get(path)?.clone(),
                control.settings.clone(),
                control.recipes.get(path).cloned(),
            )
        };
        let mut selection = read(&self.root, &identity, &settings).ok()?;
        if let Some(recipe) = recipe {
            selection.settings = Some(recipe);
        }
        // Policy changes and readiness refresh can invalidate the snapshot
        // while its local metadata is being read.
        (self.control.lock().unwrap().identities.get(path) == Some(&identity)).then_some(selection)
    }

    /// Called after a successful sidecar save. Keep selection and preview fingerprints current.
    pub fn saved_settings(&self, path: &Path, settings: &render::Settings) {
        let mut control = self.control.lock().unwrap();
        control.recipes.insert(path.to_owned(), settings.clone());
        control
            .recipe_updates
            .insert(path.to_owned(), settings.clone());
        drop(control);
        let _ = self.writer.try_send(super::Command::Maintain);
    }
}

pub(super) fn read(root: &Path, identity: &Identity, settings: &Settings) -> Result<Selection> {
    let path = root.join(identity.name()).with_extension("metadata");
    let file = fs::File::open(path)?;
    use std::io::Read;
    if file.metadata()?.len() > MAX_SELECTION_BYTES {
        return Err("cached selection too large".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_SELECTION_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() < CHECKSUM_BYTES {
        return Err("truncated cached selection".into());
    }
    let (checksum, json) = bytes.split_at(CHECKSUM_BYTES);
    if crc32fast::hash(json) != u32::from_le_bytes(checksum.try_into()?) {
        return Err("cached selection checksum mismatch".into());
    }
    let record: Record = serde_json::from_slice(json)?;
    let family = record
        .selection
        .properties
        .get("CAMMODEL")
        .and_then(|m| ProcessingFamily::from_camera_model(m))
        .ok_or("unknown selection family")?;
    if record.version != FORMAT_VERSION || &record.identity != identity || !settings.permits(family)
    {
        return Err("obsolete cached selection".into());
    }
    if let Some(recipe) = &record.selection.settings {
        recipe.validate_values()?;
    }
    if let Some(thumb) = &record.selection.thumbnail {
        if thumb.extent.iter().any(|v| *v == 0 || *v > THUMBNAIL_SIDE)
            || thumb.rgb.len() != thumb.extent[0] as usize * thumb.extent[1] as usize * 3
            || thumb.native_extent.contains(&0)
            || ![0, 90, 180, 270].contains(&thumb.rotation)
        {
            return Err("invalid cached thumbnail".into());
        }
    }
    Ok(record.selection)
}

fn resize(preview: Preview, side: u32) -> Result<Preview> {
    preview.resized(side)
}

/// All image sections come from the buffer already used by the RAW decoder.
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    root: &Path,
    path: &Path,
    identity: &Identity,
    metadata: &Metadata,
    original: &[u8],
    settings: &Settings,
    nearby: &[std::path::PathBuf],
    current: &impl Fn() -> bool,
) -> Result<()> {
    let original_checksum =
        super::original::store(root, identity, original, settings, nearby, current)?;
    let recipe = sidecar::load(path)?;
    let small = crate::preview::from_original(original, false).ok();
    crate::background::checkpoint_or_cancel()?;
    let large = crate::preview::from_original(original, true)
        .ok()
        .filter(|p| {
            (f64::from(p.extent[0]) / f64::from(p.extent[1])
                - f64::from(p.native_extent[0]) / f64::from(p.native_extent[1]))
            .abs()
                <= crate::preview::PREVIEW_ASPECT_TOLERANCE
        })
        .or_else(|| small.clone());
    let thumbnail = small
        .or_else(|| large.clone())
        .map(|p| resize(p, THUMBNAIL_SIDE))
        .transpose()?;
    if let Some(large) = large {
        previews::prepared(
            root,
            path,
            identity.clone(),
            recipe.as_ref(),
            metadata.processing_family()?,
            resize(large, previews::MAX_SIDE)?,
            settings,
            nearby,
            current,
        )?;
    }
    crate::background::checkpoint_or_cancel()?;
    let selection = Selection {
        properties: metadata.properties.clone(),
        settings: recipe,
        thumbnail,
        original_checksum,
    };
    write(root, identity, selection, settings, nearby, current)
}

pub(super) fn update_recipe(
    root: &Path,
    identity: &Identity,
    recipe: Option<render::Settings>,
    settings: &Settings,
    nearby: &[std::path::PathBuf],
    current: &impl Fn() -> bool,
) -> Result<()> {
    let mut selection = read(root, identity, settings)?;
    if selection.settings == recipe {
        return Ok(());
    }
    selection.settings = recipe;
    write(root, identity, selection, settings, nearby, current)
}

fn write(
    root: &Path,
    identity: &Identity,
    selection: Selection,
    settings: &Settings,
    nearby: &[std::path::PathBuf],
    current: &impl Fn() -> bool,
) -> Result<()> {
    let record = Record {
        version: FORMAT_VERSION,
        identity: identity.clone(),
        selection,
    };
    let json = serde_json::to_vec(&record)?;
    let bytes = json.len() as u64 + CHECKSUM_BYTES as u64;
    if bytes > MAX_SELECTION_BYTES || !current() {
        return Err("cached selection exceeds limit or was superseded".into());
    }
    let destination = root.join(identity.name()).with_extension("metadata");
    if !lru::reserve(root, &destination, bytes, settings.max_bytes, nearby)? {
        // Callers must keep the newer in-memory recipe until it reaches disk.
        return Err("cached selection admission skipped".into());
    }
    let temporary = root.join(format!(
        "{}.{}.writing",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<()> {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&crc32fast::hash(&json).to_le_bytes())?;
        file.write_all(&json)?;
        drop(file);
        crate::background::checkpoint_or_cancel()?;
        if current() {
            fs::rename(&temporary, destination)?;
        }
        Ok(())
    })();
    let _ = fs::remove_file(temporary);
    result
}
