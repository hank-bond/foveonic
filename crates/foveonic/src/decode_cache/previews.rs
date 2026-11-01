//! Stores bounded display previews under the decoded cache's identity, recipe, and shared LRU budget.

use super::{lru, Cache, Command, Control, Identity, Settings, MAX_HEADER, TEMP_SEQUENCE};
use crate::{
    preview::Preview,
    render::ControlCapabilities,
    sidecar,
    source::{Metadata, ProcessingFamily, Result},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Mutex},
    time::SystemTime,
};

pub const MAX_SIDE: u32 = 2048;
// Bump when photographic processing or preview encoding changes its appearance.
const PREVIEW_MAGIC: &[u8; 8] = b"FOVPRE04";
// Allow the largest RGB16 image plus its TIFF and compression overhead.
const MAX_ENCODED: u64 = 32 * 1024 * 1024;
// Signature, JSON length, and JSON CRC32 share the decoded source prefix layout.
const HEADER_LENGTH_OFFSET: usize = PREVIEW_MAGIC.len();
const HEADER_CHECKSUM_OFFSET: usize = HEADER_LENGTH_OFFSET + size_of::<u64>();
const PREVIEW_PREFIX_BYTES: u64 = (HEADER_CHECKSUM_OFFSET + size_of::<u32>()) as u64;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Key {
    calibration: Option<String>,
    source: Identity,
    recipe: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_recipe: Option<Vec<u8>>,
}
impl Key {
    fn same_file_recipe(&self, other: &Self) -> bool {
        self.source == other.source && self.recipe == other.recipe
    }
    fn read(path: &Path) -> Result<Self> {
        Ok(Self {
            calibration: None,
            source: Identity::read(path)?,
            default_recipe: None,
            recipe: sidecar::load(path)?
                .as_ref()
                .map(sidecar::encode)
                .transpose()?,
        })
    }
    fn location(&self, root: &Path) -> PathBuf {
        root.join(self.source.name()).with_extension("preview")
    }
}

#[derive(Serialize, Deserialize)]
struct PreviewHeader {
    key: Key,
    family: ProcessingFamily,
    extent: [u32; 2],
    native_extent: [u32; 2],
    rotation: u32,
    color_space: crate::color_space::TiffColorSpace,
    rendered: bool,
    checksum: u32,
}

fn default_key(
    defaults: &crate::render::RecipeDefaults,
    properties: &std::collections::BTreeMap<String, String>,
    unedited: bool,
) -> Result<Option<Vec<u8>>> {
    if unedited
        && properties
            .get("CAMMODEL")
            .and_then(|model| ProcessingFamily::from_camera_model(model))
            .is_some_and(|family| defaults.configured(family))
    {
        Ok(Some(sidecar::encode(
            &defaults.from_properties(properties)?,
        )?))
    } else {
        Ok(None)
    }
}

pub(super) struct PendingPreview {
    path: PathBuf,
    header: PreviewHeader,
    preview: Preview,
}

impl Cache {
    fn preview_key(&self, path: &Path) -> Result<Key> {
        let defaults = self.control.lock().unwrap().photo_defaults.clone();
        if let Some(selection) = self.selection(path) {
            let source = self
                .control
                .lock()
                .unwrap()
                .identities
                .get(path)
                .cloned()
                .ok_or("selection invalidated")?;
            return Ok(Key {
                calibration: if selection
                    .properties
                    .get("CAMMODEL")
                    .and_then(|v| ProcessingFamily::from_camera_model(v))
                    == Some(ProcessingFamily::Merrill)
                {
                    self.control.lock().unwrap().noise_calibration.clone()
                } else {
                    None
                },
                source,
                default_recipe: default_key(
                    &defaults,
                    &selection.properties,
                    selection.settings.is_none(),
                )?,
                recipe: selection
                    .settings
                    .as_ref()
                    .map(sidecar::encode)
                    .transpose()?,
            });
        }
        let mut key = Key::read(path)?;
        let calibration = self.control.lock().unwrap().noise_calibration.clone();
        if calibration.is_some()
            && self
                .capture_properties(path)?
                .get("CAMMODEL")
                .and_then(|v| ProcessingFamily::from_camera_model(v))
                == Some(ProcessingFamily::Merrill)
        {
            key.calibration = calibration;
        }

        if key.recipe.is_none() && !defaults.is_empty() {
            key.default_recipe = default_key(&defaults, &self.capture_properties(path)?, true)?;
        }
        Ok(key)
    }
    /// Call on a dedicated background thread. Missing, stale,
    /// disabled, or corrupt entries all fall back to the embedded thumbnail.
    pub fn preview(&self, path: &Path) -> Option<Preview> {
        // The caller owns cooperative scheduling. Keep larger-image decoding
        // at background QoS on this dedicated thread.
        crate::background::lower_priority();
        let settings = self.control.lock().unwrap().settings.clone();
        if !settings.enabled || settings.max_bytes == 0 {
            return None;
        }
        let key = self.preview_key(path).ok()?;
        let (header, encoded) = read(&key.location(&self.root), &key, &settings).ok()?;
        let mut limits = tiff::decoder::Limits::default();
        limits.decoding_buffer_size = MAX_SIDE as usize * MAX_SIDE as usize * 3 * size_of::<u16>();
        limits.intermediate_buffer_size = limits.decoding_buffer_size;
        let mut decoder = tiff::decoder::Decoder::new(std::io::Cursor::new(encoded))
            .ok()?
            .with_limits(limits);
        if decoder.dimensions().ok()? != (header.extent[0], header.extent[1])
            || decoder.colortype().ok()? != tiff::ColorType::RGB(16)
        {
            return None;
        }
        let tiff::decoder::DecodingResult::U16(rgb) = decoder.read_image().ok()? else {
            return None;
        };
        if self.preview_key(path).ok()? != key {
            return None;
        }
        let _ =
            File::open(key.location(&self.root)).and_then(|f| f.set_modified(SystemTime::now()));
        Some(Preview {
            rgb,
            color_space: header.color_space,
            extent: header.extent,
            rotation: header.rotation,
            native_extent: header.native_extent,
        })
    }

    /// Queue the newest whole-photo RGB16 display preview with its declared primaries. Recipe checks happen on
    /// the caller's background thread, never in the UI or main render worker.
    pub fn save_preview(
        &self,
        path: &Path,
        settings: &crate::render::Settings,
        metadata: &Metadata,
        preview: Preview,
        calibration: Option<&str>,
    ) -> Result<()> {
        let key = self.preview_key(path)?;
        let family = metadata.processing_family()?;
        let calibration = if family == ProcessingFamily::Merrill {
            calibration
        } else {
            None
        };
        if key.calibration.as_deref() != calibration {
            return Ok(());
        }

        let mut expected = match &key.recipe {
            Some(bytes) => sidecar::decode(bytes)?,
            None => match &key.default_recipe {
                Some(bytes) => sidecar::decode(bytes)?,
                None => crate::render::camera_settings(metadata, None)?,
            },
        };
        let capabilities = ControlCapabilities::from_metadata(metadata)?;
        if expected.white_balance.is_empty() {
            expected.white_balance = capabilities
                .default_white_balance(metadata)
                .unwrap_or_default();
        }
        if expected.color_mode.is_empty() {
            expected.color_mode = capabilities
                .default_color_mode(metadata)
                .unwrap_or_default();
        }
        if sidecar::encode(&expected)? != sidecar::encode(settings)? {
            return Ok(()); // An unsaved or superseded render cannot label the disk recipe.
        }
        let family = metadata.processing_family()?;
        let mut control = self.control.lock().unwrap();
        if control.settings.permits(family) && !control.clear {
            control.preview = Some(PendingPreview {
                path: path.to_owned(),
                header: PreviewHeader {
                    key,
                    family,
                    extent: preview.extent,
                    native_extent: preview.native_extent,
                    rotation: preview.rotation,
                    color_space: preview.color_space,
                    rendered: true,
                    checksum: 0,
                },
                preview,
            });
        }
        drop(control);
        let _ = self.writer.try_send(Command::Maintain);
        Ok(())
    }
}

fn read(path: &Path, key: &Key, settings: &Settings) -> Result<(PreviewHeader, Vec<u8>)> {
    read_file(path, key, settings, true)
}
fn read_file(
    path: &Path,
    key: &Key,
    settings: &Settings,
    check_defaults: bool,
) -> Result<(PreviewHeader, Vec<u8>)> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    if length > MAX_ENCODED || length > settings.max_bytes {
        return Err("preview cache entry exceeds limit".into());
    }
    let mut prefix = [0; PREVIEW_PREFIX_BYTES as usize];
    file.read_exact(&mut prefix)?;
    if &prefix[..HEADER_LENGTH_OFFSET] != PREVIEW_MAGIC {
        return Err("invalid preview cache signature".into());
    }
    let header_size =
        u64::from_le_bytes(prefix[HEADER_LENGTH_OFFSET..HEADER_CHECKSUM_OFFSET].try_into()?);
    if header_size > MAX_HEADER || header_size + PREVIEW_PREFIX_BYTES >= length {
        return Err("invalid preview cache header".into());
    }
    let mut bytes = vec![0; header_size as usize];
    file.read_exact(&mut bytes)?;
    if crc32fast::hash(&bytes) != u32::from_le_bytes(prefix[HEADER_CHECKSUM_OFFSET..].try_into()?) {
        return Err("preview cache metadata checksum mismatch".into());
    }
    let header: PreviewHeader = serde_json::from_slice(&bytes)?;
    if header.key.source != key.source
        || header.key.recipe != key.recipe
        || (header.rendered && header.key.calibration != key.calibration)
        || (check_defaults && header.rendered && header.key.default_recipe != key.default_recipe)
        || !settings.permits(header.family)
        || header.extent.iter().any(|n| *n == 0 || *n > MAX_SIDE)
        || header.native_extent.contains(&0)
        || ![0, 90, 180, 270].contains(&header.rotation)
    {
        return Err("obsolete preview cache entry".into());
    }
    let mut encoded = Vec::new();
    file.take(MAX_ENCODED).read_to_end(&mut encoded)?;
    if crc32fast::hash(&encoded) != header.checksum {
        return Err("preview cache checksum mismatch".into());
    }
    Ok((header, encoded))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepared(
    root: &Path,
    path: &Path,
    identity: Identity,
    recipe: Option<&crate::render::Settings>,
    family: ProcessingFamily,
    preview: Preview,
    settings: &Settings,
    nearby: &[PathBuf],
    current: &impl Fn() -> bool,
) -> Result<()> {
    let key = Key {
        calibration: None,
        source: identity,
        default_recipe: None,
        recipe: recipe.map(sidecar::encode).transpose()?,
    };
    let write = PendingPreview {
        path: path.to_owned(),
        header: PreviewHeader {
            key,
            family,
            extent: preview.extent,
            native_extent: preview.native_extent,
            rotation: preview.rotation,
            color_space: preview.color_space,
            rendered: false,
            checksum: 0,
        },
        preview,
    };
    write_checked(root, write, settings, nearby, current)
}

#[cfg(test)]
fn embedded(
    root: &Path,
    path: &Path,
    family: ProcessingFamily,
    settings: &Settings,
    nearby: &[PathBuf],
    current: &impl Fn() -> bool,
) -> Result<()> {
    let key = Key::read(path)?;
    if read(&key.location(root), &key, settings).is_ok() {
        return Ok(());
    }
    crate::background::checkpoint_or_cancel()?;
    let preview = crate::preview::read_full(path)?;
    // DP0's large JPEG omits Raw borders. Keep its correctly framed small
    // thumbnail until a photographic render can supply those missing pixels.
    if (f64::from(preview.extent[0]) / f64::from(preview.extent[1])
        - f64::from(preview.native_extent[0]) / f64::from(preview.native_extent[1]))
    .abs()
        > crate::preview::PREVIEW_ASPECT_TOLERANCE
    {
        return Ok(());
    }
    crate::background::checkpoint_or_cancel()?;
    let preview = preview.resized(MAX_SIDE)?;
    let write = PendingPreview {
        path: path.to_owned(),
        header: PreviewHeader {
            key,
            family,
            extent: preview.extent,
            native_extent: preview.native_extent,
            rotation: preview.rotation,
            color_space: preview.color_space,
            rendered: false,
            checksum: 0,
        },
        preview,
    };
    write_checked(root, write, settings, nearby, current)
}

pub(super) fn location(root: &Path, path: &Path) -> Result<PathBuf> {
    // Eviction needs only the source identity, never its editing recipe.
    Ok(root
        .join(Identity::read(path)?.name())
        .with_extension("preview"))
}

pub(super) fn store(root: &Path, write: PendingPreview, control: &Mutex<Control>) -> Result<()> {
    let (settings, generation, nearby, defaults) = {
        let state = control.lock().unwrap();
        (
            state.settings.clone(),
            state.generation,
            state.nearby.clone(),
            state.photo_defaults.clone(),
        )
    };
    if write.header.rendered && write.header.key.recipe.is_none() {
        let expected = if defaults.configured(write.header.family) {
            let properties = super::selection::read(root, &write.header.key.source, &settings)
                .map(|selection| selection.properties)
                .or_else(|_| crate::source::read_capture_properties(&write.path))?;
            default_key(&defaults, &properties, true)?
        } else {
            None
        };
        if expected != write.header.key.default_recipe {
            return Ok(());
        }
    }
    write_checked(root, write, &settings, &nearby, &|| {
        let state = control.lock().unwrap();
        !state.clear
            && state.generation == generation
            && state.settings == settings
            && state.photo_defaults == defaults
    })
}

fn write_checked(
    root: &Path,
    mut write: PendingPreview,
    settings: &Settings,
    nearby: &[PathBuf],
    current: &impl Fn() -> bool,
) -> Result<()> {
    if !current() || !settings.permits(write.header.family) {
        return Ok(());
    }
    crate::background::checkpoint_or_cancel()?;
    if !Key::read(&write.path)?.same_file_recipe(&write.header.key) {
        return Ok(());
    }
    let destination = write.header.key.location(root);
    if !write.header.rendered
        && read_file(&destination, &write.header.key, settings, false)
            .is_ok_and(|(h, _)| h.rendered)
    {
        return Ok(());
    }
    if write
        .preview
        .extent
        .iter()
        .any(|n| *n == 0 || *n > MAX_SIDE)
    {
        return Err("preview too large".into());
    }
    if write.preview.rgb.len()
        != write.preview.extent[0] as usize * write.preview.extent[1] as usize * 3
        || write.preview.native_extent.contains(&0)
        || ![0, 90, 180, 270].contains(&write.preview.rotation)
    {
        return Err("invalid display preview".into());
    }
    let mut encoded = std::io::Cursor::new(Vec::new());
    {
        let mut encoder = tiff::encoder::TiffEncoder::new(&mut encoded)?
            .with_compression(tiff::encoder::Compression::Deflate(
                tiff::encoder::DeflateLevel::Fast,
            ))
            .with_predictor(tiff::encoder::Predictor::Horizontal);
        encoder.write_image::<tiff::encoder::colortype::RGB16>(
            write.preview.extent[0],
            write.preview.extent[1],
            &write.preview.rgb,
        )?;
    }
    let encoded = encoded.into_inner();
    write.header.checksum = crc32fast::hash(&encoded);
    let header = serde_json::to_vec(&write.header)?;
    let bytes = PREVIEW_PREFIX_BYTES + header.len() as u64 + encoded.len() as u64;
    if bytes > settings.max_bytes || bytes > MAX_ENCODED {
        return Ok(());
    }
    fs::create_dir_all(root)?;
    crate::background::checkpoint_or_cancel()?;
    if !current() || !lru::reserve(root, &destination, bytes, settings.max_bytes, nearby)? {
        return Ok(());
    }
    let temporary = root.join(format!(
        "{}.{}.writing",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(PREVIEW_MAGIC)?;
        file.write_all(&(header.len() as u64).to_le_bytes())?;
        file.write_all(&crc32fast::hash(&header).to_le_bytes())?;
        file.write_all(&header)?;
        file.write_all(&encoded)?;
        drop(file);
        crate::background::checkpoint_or_cancel()?;
        if current() && Key::read(&write.path)?.same_file_recipe(&write.header.key) {
            fs::rename(&temporary, destination)?;
        }
        Ok(())
    })();
    let _ = fs::remove_file(temporary);
    result
}

#[cfg(test)]
#[path = "previews_test.rs"]
mod tests;
