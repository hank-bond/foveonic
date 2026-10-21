//! Decodes embedded bitmap and JPEG previews independently of sensor processing.

use crate::source::Result;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

pub struct Preview {
    /// Tightly packed, unrotated RGB8 samples.
    pub rgb: Vec<u8>,
    pub extent: [u32; 2],
    pub rotation: u32,
    /// Unrotated dimensions of the largest embedded preview, read from section headers only.
    pub native_extent: [u32; 2],
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

/// Decode the smallest supported embedded preview by pixel area.
pub fn read(path: &Path) -> Result<Preview> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    if !(44..=1024 * 1024 * 1024).contains(&length) {
        return Err("unsupported X3F size".into());
    }
    let mut header = [0; 40];
    file.read_exact(&mut header)?;
    if &header[..4] != b"FOVb"
        || ![0x20000, 0x20001, 0x20002, 0x20003, 0x30000].contains(&u32_at(&header, 4))
    {
        return Err("unsupported X3F header".into());
    }
    file.seek(SeekFrom::End(-4))?;
    let mut pointer = [0; 4];
    file.read_exact(&mut pointer)?;
    let directory = u64::from(u32_at(&pointer, 0));
    if directory < 40 || directory + 12 > length - 4 {
        return Err("invalid X3F directory".into());
    }
    file.seek(SeekFrom::Start(directory))?;
    let mut index = [0; 12];
    file.read_exact(&mut index)?;
    let count = u32_at(&index, 8) as usize;
    if &index[..4] != b"SECd"
        || u32_at(&index, 4) != 0x20000
        || count == 0
        || count > 1024
        || directory + 12 + count as u64 * 12 > length - 4
    {
        return Err("invalid X3F directory".into());
    }
    let mut entries = vec![0; count * 12];
    file.read_exact(&mut entries)?;
    let mut selected: Option<Embedded> = None;
    let mut native_extent = [0_u32; 2];
    for entry in entries.as_chunks::<12>().0 {
        if &entry[8..] != b"IMAG" && &entry[8..] != b"IMA2" {
            continue;
        }
        let offset = u64::from(u32_at(entry, 0));
        let size = u32_at(entry, 4) as usize;
        if offset < 40 || size < 28 || offset + size as u64 > directory {
            return Err("invalid X3F image section".into());
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut image = [0; 28];
        file.read_exact(&mut image)?;
        if &image[..4] != b"SECi" {
            return Err("invalid X3F image".into());
        }
        if u32_at(&image, 8) != 2 {
            continue;
        }
        let extent = [u32_at(&image, 16), u32_at(&image, 20)];
        let pixels = u64::from(extent[0]) * u64::from(extent[1]);
        if pixels == 0 || pixels > MAX_PIXELS {
            return Err("invalid embedded preview dimensions".into());
        }
        if pixels > u64::from(native_extent[0]) * u64::from(native_extent[1]) {
            native_extent = extent;
        }
        let format = u32_at(&image, 12);
        if [BITMAP_RGB, JPEG].contains(&format)
            && selected.as_ref().is_none_or(|old| pixels < old.pixels())
        {
            selected = Some(Embedded {
                offset: offset + 28,
                size: size - 28,
                format,
                extent,
                stride: u32_at(&image, 24) as usize,
            });
        }
    }
    let selected = selected.ok_or("no supported embedded preview")?;
    if selected.size > MAX_BYTES {
        return Err("embedded preview too large".into());
    }
    file.seek(SeekFrom::Start(selected.offset))?;
    let mut encoded = vec![0; selected.size];
    file.read_exact(&mut encoded)?;
    let rgb = selected.decode(&encoded)?;

    Ok(Preview {
        rgb,
        extent: selected.extent,
        rotation: u32_at(&header, 36),
        native_extent,
    })
}

const BITMAP_RGB: u32 = 3;
const JPEG: u32 = 18;
const MAX_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_BYTES: usize = 256 * 1024 * 1024;

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

    fn decode(&self, encoded: &[u8]) -> Result<Vec<u8>> {
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
                rgb.extend_from_slice(&row[..row_bytes]);
            }
            Ok(rgb)
        } else if self.format == JPEG {
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
            Ok(image::DynamicImage::from_decoder(decoder)?
                .into_rgb8()
                .into_raw())
        } else {
            Err("unsupported embedded preview encoding".into())
        }
    }
}

#[cfg(test)]
#[path = "preview_test.rs"]
mod tests;
