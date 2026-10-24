//! Decodes embedded bitmap and JPEG previews into color-managed sRGB display pixels.

use crate::source::Result;
#[path = "preview_color.rs"]
mod color;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

// X3F on-disk layout. Preview reads seek directly to image sections without sensor decoding.
const BASE_HEADER_BYTES: usize = 40;
const V4_HEADER_BYTES: usize = 52;
const DIRECTORY_POINTER_BYTES: u64 = 4;
const DIRECTORY_HEADER_BYTES: usize = 12;
const DIRECTORY_ENTRY_BYTES: usize = 12;
const IMAGE_HEADER_BYTES: usize = 28;
const VERSION_OFFSET: usize = 4;
const V4_EXTENT_OFFSET: usize = 40;
const DIRECTORY_COUNT_OFFSET: usize = 8;
const ENTRY_TAG_OFFSET: usize = 8;
const IMAGE_TYPE_OFFSET: usize = 8;
const IMAGE_FORMAT_OFFSET: usize = 12;
const IMAGE_EXTENT_OFFSET: usize = 16;
const IMAGE_STRIDE_OFFSET: usize = 24;
const WORD_BYTES: usize = std::mem::size_of::<u32>();
const V4_VERSION: u32 = 0x40000;
const DIRECTORY_VERSION: u32 = 0x20000;
const PREVIEW_IMAGE_TYPE: u32 = 2;
const MAX_DIRECTORY_ENTRIES: usize = 1024;
const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy)]
enum Selection {
    Smallest,
    Largest,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Preview {
    /// Tightly packed, unrotated sRGB-encoded RGB8 display samples.
    pub rgb: Vec<u8>,
    pub extent: [u32; 2],
    pub rotation: u32,
    /// Unrotated photograph dimensions from the version-4 header or largest older preview.
    pub native_extent: [u32; 2],
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + WORD_BYTES].try_into().unwrap())
}

/// Decode the smallest supported embedded preview by pixel area.
pub fn read(path: &Path) -> Result<Preview> {
    read_selected(path, Selection::Smallest)
}

/// Decode the largest supported embedded image. It may have a different camera
/// crop from the Raw, so its own extent describes its photographic framing.
pub fn read_full(path: &Path) -> Result<Preview> {
    read_selected(path, Selection::Largest)
}

fn read_selected(path: &Path, selection: Selection) -> Result<Preview> {
    let mut file = File::open(path)?;
    read_reader(&mut file, selection)
}

pub(crate) fn from_original(bytes: &[u8], full: bool) -> Result<Preview> {
    read_reader(
        &mut std::io::Cursor::new(bytes),
        if full {
            Selection::Largest
        } else {
            Selection::Smallest
        },
    )
}

fn read_reader(file: &mut (impl Read + Seek), selection: Selection) -> Result<Preview> {
    // Reuse the open reader. During pre-cache this seeks only within the original
    // memory buffer, including metadata stored in version-4 JPEG MakerNotes.
    let encoding = color::Encoding::camera(&crate::x3f::capture::read(file).unwrap_or_default());
    let length = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(0))?;
    if !((BASE_HEADER_BYTES as u64 + DIRECTORY_POINTER_BYTES)..=MAX_FILE_BYTES).contains(&length) {
        return Err("unsupported X3F size".into());
    }
    let mut header = [0; V4_HEADER_BYTES];
    file.read_exact(&mut header[..BASE_HEADER_BYTES])?;
    if &header[..4] != b"FOVb"
        || ![
            0x20000, 0x20001, 0x20002, 0x20003, 0x30000, 0x40001, 0x40002,
        ]
        .contains(&u32_at(&header, VERSION_OFFSET))
    {
        return Err("unsupported X3F header".into());
    }
    if u32_at(&header, VERSION_OFFSET) >= V4_VERSION {
        file.read_exact(&mut header[BASE_HEADER_BYTES..])?;
    }
    crate::x3f::processing_version(&header)?;
    let rotation = crate::x3f::rotation(&header)?;
    file.seek(SeekFrom::End(-(DIRECTORY_POINTER_BYTES as i64)))?;
    let mut pointer = [0; DIRECTORY_POINTER_BYTES as usize];
    file.read_exact(&mut pointer)?;
    let directory = u64::from(u32_at(&pointer, 0));
    if directory < BASE_HEADER_BYTES as u64
        || directory + DIRECTORY_HEADER_BYTES as u64 > length - DIRECTORY_POINTER_BYTES
    {
        return Err("invalid X3F directory".into());
    }
    file.seek(SeekFrom::Start(directory))?;
    let mut index = [0; DIRECTORY_HEADER_BYTES];
    file.read_exact(&mut index)?;
    let count = u32_at(&index, DIRECTORY_COUNT_OFFSET) as usize;
    if &index[..4] != b"SECd"
        || u32_at(&index, VERSION_OFFSET) != DIRECTORY_VERSION
        || count == 0
        || count > MAX_DIRECTORY_ENTRIES
        || directory + DIRECTORY_HEADER_BYTES as u64 + count as u64 * DIRECTORY_ENTRY_BYTES as u64
            > length - DIRECTORY_POINTER_BYTES
    {
        return Err("invalid X3F directory".into());
    }
    let mut entries = vec![0; count * DIRECTORY_ENTRY_BYTES];
    file.read_exact(&mut entries)?;
    let mut selected: Option<Embedded> = None;
    let mut exif_preview: Option<(Vec<u8>, [u32; 2])> = None;
    let header_extent = (u32_at(&header, VERSION_OFFSET) >= V4_VERSION)
        .then(|| {
            [
                u32_at(&header, V4_EXTENT_OFFSET),
                u32_at(&header, V4_EXTENT_OFFSET + WORD_BYTES),
            ]
        })
        .filter(|e| e.iter().all(|v| *v > 0) && u64::from(e[0]) * u64::from(e[1]) <= MAX_PIXELS);
    let mut native_extent = header_extent.unwrap_or([0_u32; 2]);
    for entry in entries.as_chunks::<DIRECTORY_ENTRY_BYTES>().0 {
        if &entry[ENTRY_TAG_OFFSET..] != b"IMAG" && &entry[ENTRY_TAG_OFFSET..] != b"IMA2" {
            continue;
        }
        let offset = u64::from(u32_at(entry, 0));
        let size = u32_at(entry, 4) as usize;
        if offset < BASE_HEADER_BYTES as u64
            || size < IMAGE_HEADER_BYTES
            || offset + size as u64 > directory
        {
            return Err("invalid X3F image section".into());
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut image = [0; IMAGE_HEADER_BYTES];
        file.read_exact(&mut image)?;
        if &image[..4] != b"SECi" {
            return Err("invalid X3F image".into());
        }
        if u32_at(&image, IMAGE_TYPE_OFFSET) != PREVIEW_IMAGE_TYPE {
            continue;
        }
        let extent = [
            u32_at(&image, IMAGE_EXTENT_OFFSET),
            u32_at(&image, IMAGE_EXTENT_OFFSET + WORD_BYTES),
        ];
        let pixels = u64::from(extent[0]) * u64::from(extent[1]);
        if pixels == 0 || pixels > MAX_PIXELS {
            return Err("invalid embedded preview dimensions".into());
        }
        if header_extent.is_none()
            && pixels > u64::from(native_extent[0]) * u64::from(native_extent[1])
        {
            native_extent = extent;
        }
        let format = u32_at(&image, IMAGE_FORMAT_OFFSET);
        if let Some(native) =
            header_extent.filter(|_| matches!(selection, Selection::Smallest) && format == JPEG)
        {
            // The thumbnail lives near the JPEG header. Never read the complete
            // large JPEG to find it. A differently cropped JPEG (DP0) cannot
            // supply missing Raw borders, so use its full-frame section instead.
            if (f64::from(extent[0]) / f64::from(extent[1])
                - f64::from(native[0]) / f64::from(native[1]))
            .abs()
                < PREVIEW_ASPECT_TOLERANCE
            {
                let mut prefix = vec![0; (size - IMAGE_HEADER_BYTES).min(EXIF_PREFIX_BYTES)];
                file.read_exact(&mut prefix)?;
                if let Some(preview) = crate::x3f::jpeg_thumbnail(&prefix).and_then(|jpeg| {
                    decode_exif_thumbnail(jpeg, extent, &encoding.jpeg(&prefix)).ok()
                }) {
                    if exif_preview
                        .as_ref()
                        .is_none_or(|old| preview.1[0] * preview.1[1] < old.1[0] * old.1[1])
                    {
                        exif_preview = Some(preview);
                    }
                }
            }
        }
        if [BITMAP_RGB, JPEG, JPEG_THUMBNAIL].contains(&format)
            && selected.as_ref().is_none_or(|old| {
                if matches!(selection, Selection::Largest) {
                    pixels > old.pixels()
                } else {
                    pixels < old.pixels()
                }
            })
        {
            selected = Some(Embedded {
                offset: offset + IMAGE_HEADER_BYTES as u64,
                size: size - IMAGE_HEADER_BYTES,
                format,
                extent,
                stride: u32_at(&image, IMAGE_STRIDE_OFFSET) as usize,
            });
        }
    }
    if let Some((rgb, extent)) = exif_preview.filter(|preview| {
        selected
            .as_ref()
            .is_none_or(|s| u64::from(preview.1[0]) * u64::from(preview.1[1]) < s.pixels())
    }) {
        return Ok(Preview {
            rgb,
            extent,
            rotation,
            native_extent,
        });
    }
    let selected = selected.ok_or("no supported embedded preview")?;
    if selected.size > MAX_BYTES {
        return Err("embedded preview too large".into());
    }
    file.seek(SeekFrom::Start(selected.offset))?;
    let mut encoded = vec![0; selected.size];
    for chunk in encoded.chunks_mut(crate::background::IO_CHUNK_BYTES) {
        crate::background::checkpoint_or_cancel()?;
        file.read_exact(chunk)?;
    }
    crate::background::checkpoint_or_cancel()?;
    let rgb = selected.decode(&encoded, &encoding)?;
    crate::background::checkpoint_or_cancel()?;

    Ok(Preview {
        rgb,
        extent: selected.extent,
        rotation,
        native_extent,
    })
}

fn decode_exif_thumbnail(
    encoded: &[u8],
    photograph: [u32; 2],
    encoding: &color::Encoding,
) -> Result<(Vec<u8>, [u32; 2])> {
    use image::ImageDecoder;
    let mut decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(encoded))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_EXIF_THUMBNAIL_SIDE);
    limits.max_image_height = Some(MAX_EXIF_THUMBNAIL_SIDE);
    limits.max_alloc = Some(u64::from(MAX_EXIF_THUMBNAIL_SIDE).pow(2) * 4);
    decoder.set_limits(limits)?;
    let image = image::DynamicImage::from_decoder(decoder)?.into_rgb8();
    // Exif thumbnails often have black padding to a 4:3 canvas. Remove only
    // confirmed dark padding around the rectangle implied by the JPEG frame.
    let (width, height) = if u64::from(image.width()) * u64::from(photograph[1])
        <= u64::from(image.height()) * u64::from(photograph[0])
    {
        (
            image.width(),
            (u64::from(image.width()) * u64::from(photograph[1]) / u64::from(photograph[0])).max(1)
                as u32,
        )
    } else {
        (
            (u64::from(image.height()) * u64::from(photograph[0]) / u64::from(photograph[1])).max(1)
                as u32,
            image.height(),
        )
    };
    let x = (image.width() - width) / 2;
    let y = (image.height() - height) / 2;
    // JPEG ringing in the reviewed black bars reaches 30/255. A 32-code
    // allowance keeps that padding removable while rejecting image content.
    const PADDING_BLACK_LIMIT: u8 = 32;
    if image.enumerate_pixels().any(|(px, py, rgb)| {
        (px < x || py < y || px >= x + width || py >= y + height)
            && rgb.0.iter().any(|c| *c > PADDING_BLACK_LIMIT)
    }) {
        return Err("Exif thumbnail has no matching photograph padding".into());
    }
    let mut rgb = image::imageops::crop_imm(&image, x, y, width, height)
        .to_image()
        .into_raw();
    encoding.jpeg(encoded).to_srgb(&mut rgb)?;
    Ok((rgb, [width, height]))
}

const BITMAP_RGB: u32 = 3;
const JPEG: u32 = 18;
// Version-4 files carry an additional JPEG thumbnail with the full Raw framing.
const JPEG_THUMBNAIL: u32 = 25;
const MAX_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_BYTES: usize = 256 * 1024 * 1024;
const EXIF_PREFIX_BYTES: usize = 128 * 1024;
const MAX_EXIF_THUMBNAIL_SIDE: u32 = 512;
// Rounded thumbnail dimensions may differ slightly from the nominal Raw ratio.
pub(crate) const PREVIEW_ASPECT_TOLERANCE: f64 = 0.02;

struct Embedded {
    offset: u64,
    size: usize,
    format: u32,
    extent: [u32; 2],
    stride: usize,
}

impl Embedded {
    fn pixels(&self) -> u64 {
        u64::from(self.extent[0]) * u64::from(self.extent[1])
    }

    fn decode(&self, encoded: &[u8], encoding: &color::Encoding) -> Result<Vec<u8>> {
        if self.format == BITMAP_RGB {
            let row_bytes = self.extent[0] as usize * 3;
            let required = self
                .stride
                .checked_mul(self.extent[1] as usize)
                .ok_or("invalid embedded bitmap stride")?;
            if self.stride < row_bytes || required > encoded.len() {
                return Err("invalid embedded bitmap stride or length".into());
            }
            let mut rgb = Vec::with_capacity(self.pixels() as usize * 3);
            for row in encoded[..required].chunks_exact(self.stride) {
                crate::background::checkpoint_or_cancel()?;
                rgb.extend_from_slice(&row[..row_bytes]);
            }
            encoding.to_srgb(&mut rgb)?;
            Ok(rgb)
        } else if matches!(self.format, JPEG | JPEG_THUMBNAIL) {
            use image::ImageDecoder;
            let mut decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(encoded))?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(self.extent[0]);
            limits.max_image_height = Some(self.extent[1]);
            limits.max_alloc = Some(MAX_BYTES as u64);
            decoder.set_limits(limits)?;
            if decoder.dimensions() != (self.extent[0], self.extent[1]) {
                return Err("embedded JPEG dimensions disagree with section header".into());
            }
            let mut rgb = image::DynamicImage::from_decoder(decoder)?
                .into_rgb8()
                .into_raw();
            encoding.jpeg(encoded).to_srgb(&mut rgb)?;
            Ok(rgb)
        } else {
            Err("unsupported embedded preview encoding".into())
        }
    }
}

#[cfg(test)]
#[path = "preview_test.rs"]
mod tests;
