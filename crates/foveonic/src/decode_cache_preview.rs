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
const PREVIEW_MAGIC: &[u8; 8] = b"FOVPRE01";
const MAX_ENCODED: u64 = 32 * 1024 * 1024;
const JPEG_QUALITY: u8 = 92;
// Signature, JSON length, and JSON CRC32 share the decoded source prefix layout.
const HEADER_LENGTH_OFFSET: usize = PREVIEW_MAGIC.len();
const HEADER_CHECKSUM_OFFSET: usize = HEADER_LENGTH_OFFSET + size_of::<u64>();
const PREVIEW_PREFIX_BYTES: u64 = (HEADER_CHECKSUM_OFFSET + size_of::<u32>()) as u64;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Key {
    source: Identity,
    recipe: Option<Vec<u8>>,
}
impl Key {
    fn read(path: &Path) -> Result<Self> {
        Ok(Self {
            source: Identity::read(path)?,
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
    rendered: bool,
    checksum: u32,
}

pub(super) struct PendingPreview {
    path: PathBuf,
    header: PreviewHeader,
    preview: Preview,
}

impl Cache {
    /// Call on a dedicated background thread. Missing, stale,
    /// disabled, or corrupt entries all fall back to the embedded thumbnail.
    pub fn preview(&self, path: &Path) -> Option<Preview> {
        // These small reads must remain available while the main render runs,
        // so they use low priority rather than waiting behind its activity gate.
        crate::background::lower_priority();
        let settings = self.control.lock().unwrap().settings.clone();
        if !settings.enabled || settings.max_bytes == 0 {
            return None;
        }
        let key = Key::read(path).ok()?;
        let (header, encoded) = read(&key.location(&self.root), &key, &settings).ok()?;
        use image::ImageDecoder;
        let mut decoder =
            image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(encoded)).ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_SIDE);
        limits.max_image_height = Some(MAX_SIDE);
        limits.max_alloc = Some(u64::from(MAX_SIDE).pow(2) * 4);
        decoder.set_limits(limits).ok()?;
        if decoder.dimensions() != (header.extent[0], header.extent[1]) {
            return None;
        }
        let rgb = image::DynamicImage::from_decoder(decoder)
            .ok()?
            .into_rgb8()
            .into_raw();
        if Key::read(path).ok()? != key {
            return None;
        }
        let _ =
            File::open(key.location(&self.root)).and_then(|f| f.set_modified(SystemTime::now()));
        Some(Preview {
            rgb,
            extent: header.extent,
            rotation: header.rotation,
            native_extent: header.native_extent,
        })
    }

    /// Queue the newest whole-photo display preview. Recipe checks happen on
    /// the caller's background thread, never in the UI or main render worker.
    pub fn save_preview(
        &self,
        path: &Path,
        settings: &crate::render::Settings,
        metadata: &Metadata,
        preview: Preview,
    ) -> Result<()> {
        let key = Key::read(path)?;
        let mut expected = match &key.recipe {
            Some(bytes) => sidecar::decode(bytes)?,
            None => {
                let mut defaults = crate::render::Settings::default();
                let capabilities = ControlCapabilities::from_metadata(metadata)?;
                defaults.rendering = capabilities.default_rendering(metadata)?;
                capabilities.apply_capture_defaults(&mut defaults, metadata)?;
                defaults
            }
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
    if &header.key != key
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

pub(super) fn embedded(
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
    let image = image::RgbImage::from_raw(preview.extent[0], preview.extent[1], preview.rgb)
        .ok_or("invalid preview samples")?;
    let image = image::DynamicImage::ImageRgb8(image)
        .thumbnail(MAX_SIDE, MAX_SIDE)
        .into_rgb8();
    let preview = Preview {
        extent: [image.width(), image.height()],
        rgb: image.into_raw(),
        rotation: preview.rotation,
        native_extent: preview.native_extent,
    };
    let write = PendingPreview {
        path: path.to_owned(),
        header: PreviewHeader {
            key,
            family,
            extent: preview.extent,
            native_extent: preview.native_extent,
            rotation: preview.rotation,
            rendered: false,
            checksum: 0,
        },
        preview,
    };
    write_checked(root, write, settings, nearby, current)
}

pub(super) fn location(root: &Path, path: &Path) -> Result<PathBuf> {
    Ok(Key::read(path)?.location(root))
}

pub(super) fn store(root: &Path, write: PendingPreview, control: &Mutex<Control>) -> Result<()> {
    let (settings, generation, nearby) = {
        let state = control.lock().unwrap();
        (
            state.settings.clone(),
            state.generation,
            state.nearby.clone(),
        )
    };
    write_checked(root, write, &settings, &nearby, &|| {
        let state = control.lock().unwrap();
        !state.clear && state.generation == generation && state.settings == settings
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
    if Key::read(&write.path)? != write.header.key {
        return Ok(());
    }
    let destination = write.header.key.location(root);
    if !write.header.rendered
        && read(&destination, &write.header.key, settings).is_ok_and(|(h, _)| h.rendered)
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
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY).encode(
        &write.preview.rgb,
        write.preview.extent[0],
        write.preview.extent[1],
        image::ExtendedColorType::Rgb8,
    )?;
    write.header.checksum = crc32fast::hash(&jpeg);
    let header = serde_json::to_vec(&write.header)?;
    let bytes = PREVIEW_PREFIX_BYTES + header.len() as u64 + jpeg.len() as u64;
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
        file.write_all(&jpeg)?;
        drop(file);
        crate::background::checkpoint_or_cancel()?;
        if current() && Key::read(&write.path)? == write.header.key {
            fs::rename(&temporary, destination)?;
        }
        Ok(())
    })();
    let _ = fs::remove_file(temporary);
    result
}

#[cfg(test)]
#[path = "decode_cache_preview_test.rs"]
mod tests;
