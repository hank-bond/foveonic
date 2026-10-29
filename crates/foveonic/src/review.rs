//! Persists independent star ratings in a bounded per-folder review file without touching editing recipes.

use crate::source::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
};

const FORMAT: &str = "foveonic-ratings";
const VERSION: u32 = 1;
const MAX_BYTES: u64 = 1024 * 1024;
static WRITER: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Rating(u8);
impl Rating {
    pub const MAX: u8 = 5;
    pub fn stars(self) -> u8 {
        self.0
    }
}
impl TryFrom<u8> for Rating {
    type Error = &'static str;
    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        if value <= Self::MAX {
            Ok(Self(value))
        } else {
            Err("rating must be between zero and five")
        }
    }
}
impl From<Rating> for u8 {
    fn from(value: Rating) -> Self {
        value.0
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: String,
    version: u32,
    ratings: BTreeMap<PathBuf, Rating>,
}
pub fn path(folder: &Path) -> PathBuf {
    folder.join("foveonic").join("ratings.json")
}

/// Keys are sibling filenames. Missing files mean every photograph is unrated.
pub fn load(folder: &Path) -> Result<BTreeMap<PathBuf, Rating>> {
    let file = match fs::File::open(path(folder)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("ratings file exceeds 1 MiB".into());
    }
    let record: Record = serde_json::from_slice(&bytes)?;
    if record.format != FORMAT || record.version != VERSION {
        return Err("unsupported ratings file".into());
    }
    if record
        .ratings
        .keys()
        .any(|name| name.components().count() != 1 || name.file_name() != Some(name.as_os_str()))
    {
        return Err("ratings keys must be sibling filenames".into());
    }
    Ok(record.ratings)
}

/// Merge changes before atomic replacement, preserving other filenames and unreadable files.
pub fn save(photo: &Path, rating: Rating) -> Result<()> {
    let folder = photo
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = photo.file_name().ok_or("rating source has no filename")?;
    save_many(folder, &BTreeMap::from([(PathBuf::from(name), rating)]))
}

/// Publish a highlighted set with one read/merge/write for each containing folder.
pub fn save_many(folder: &Path, changes: &BTreeMap<PathBuf, Rating>) -> Result<()> {
    let _writer = WRITER.lock().unwrap();
    if changes
        .keys()
        .any(|name| name.components().count() != 1 || name.file_name() != Some(name.as_os_str()))
    {
        return Err("ratings keys must be sibling filenames".into());
    }
    let mut ratings = load(folder)?;
    let mut changed = false;
    for (name, rating) in changes {
        changed |= if rating.stars() == 0 {
            ratings.remove(name).is_some()
        } else {
            ratings.insert(name.clone(), *rating) != Some(*rating)
        };
    }
    if !changed {
        return Ok(());
    }
    let record = Record {
        format: FORMAT.into(),
        version: VERSION,
        ratings,
    };
    let mut bytes = serde_json::to_vec_pretty(&record)?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_BYTES {
        return Err("ratings file exceeds 1 MiB".into());
    }
    let destination = path(folder);
    match fs::create_dir(destination.parent().unwrap()) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error.into()),
    }
    crate::importing::write_output(
        &destination,
        crate::importing::ConflictPolicy::Overwrite,
        |temporary| {
            fs::write(temporary, &bytes)?;
            Ok(())
        },
    )
}

#[cfg(test)]
#[path = "review_test.rs"]
mod tests;
