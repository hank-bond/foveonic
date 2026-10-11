//! Reads embedded JPEG previews and presentation dimensions from X3F sections.

use crate::source::Result;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

pub struct Preview {
    pub jpeg: Vec<u8>,
    pub rotation: u32,
    /// Unrotated dimensions of the largest embedded JPEG, read from section headers only.
    pub native_extent: [u32; 2],
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

/// Select the smallest embedded JPEG for initial browser thumbnails.
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
    let mut selected: Option<(u64, usize)> = None;
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
        if u32_at(&image, 8) == 2 && u32_at(&image, 12) == 18 {
            let extent = [u32_at(&image, 16), u32_at(&image, 20)];
            if u64::from(extent[0]) * u64::from(extent[1])
                > u64::from(native_extent[0]) * u64::from(native_extent[1])
            {
                native_extent = extent;
            }
            if selected.is_none_or(|(_, old)| size < old) {
                selected = Some((offset + 28, size - 28));
            }
        }
    }
    let (offset, size) = selected.ok_or("no embedded JPEG")?;
    if size > 32 * 1024 * 1024 {
        return Err("embedded JPEG too large".into());
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut jpeg = vec![0; size];
    file.read_exact(&mut jpeg)?;
    if !jpeg.starts_with(&[0xff, 0xd8]) {
        return Err("invalid embedded JPEG".into());
    }
    Ok(Preview {
        jpeg,
        rotation: u32_at(&header, 36),
        native_extent,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires FOVEONIC_FIXTURE_DIR"]
    fn original_previews_are_jpegs() {
        let dir = std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap();
        for file in std::fs::read_dir(dir).unwrap() {
            let file = file.unwrap().path();
            if file
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("x3f"))
            {
                let p = super::read(&file).unwrap();
                assert_eq!(p.native_extent, [4704, 3136]);
                assert!(p.jpeg.len() < 100_000);
                assert!(p.jpeg.starts_with(&[0xff, 0xd8]));
            }
        }
    }
}
