//! Copies originals, publishes complete outputs, and stores destination import and hidden decisions.

use crate::source::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, FileTimes},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const HISTORY_VERSION: u32 = 1;
// Bound accidental or malformed history reads without limiting ordinary card imports.
const MAX_HISTORY_BYTES: u64 = 32 * 1024 * 1024;
const COPY_CHUNK_BYTES: usize = 1024 * 1024;

/// A deliberate inexpensive heuristic. Camera clocks can repeat. No source hash is read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    filename: Vec<u8>,
    modified_seconds: u64,
    modified_nanos: u32,
    length: u64,
}
impl Identity {
    pub(crate) fn from_snapshot(
        path: &Path,
        length: u64,
        modified_seconds: u64,
        modified_nanos: u32,
    ) -> Self {
        Self {
            filename: path
                .file_name()
                .unwrap_or_default()
                .as_encoded_bytes()
                .to_vec(),
            length,
            modified_seconds,
            modified_nanos,
        }
    }
    pub fn read(path: &Path) -> Result<Self> {
        let metadata = fs::metadata(path)?;
        if !metadata.is_file() {
            return Err("import source is not a regular file".into());
        }
        let modified = metadata.modified()?.duration_since(UNIX_EPOCH)?;
        Ok(Self {
            filename: path
                .file_name()
                .ok_or("source has no filename")?
                .as_encoded_bytes()
                .to_vec(),
            modified_seconds: modified.as_secs(),
            modified_nanos: modified.subsec_nanos(),
            length: metadata.len(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Disposition {
    Hidden,
    Imported { original: PathBuf },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub identity: Identity,
    pub source: String,
    pub disposition: Disposition,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    version: u32,
    records: Vec<Record>,
}
impl Default for History {
    fn default() -> Self {
        Self {
            version: HISTORY_VERSION,
            records: Vec::new(),
        }
    }
}
impl History {
    pub fn path(destination: &Path) -> PathBuf {
        destination.join("foveonic").join("import-history.json")
    }
    pub fn load(destination: &Path) -> Result<Self> {
        let file = match File::open(Self::path(destination)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        file.take(MAX_HISTORY_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_HISTORY_BYTES {
            return Err("import history exceeds 32 MiB".into());
        }
        let history: Self = serde_json::from_slice(&bytes)?;
        if history.version != HISTORY_VERSION {
            return Err("unsupported import history version".into());
        }
        Ok(history)
    }
    /// Only one current decision is retained per identity. This is not an audit log.
    pub fn set(&mut self, record: Record) {
        // Replacing an original retires the previous source's claim to that path.
        self.records.retain(|old| {
            old.identity != record.identity
                && !matches!((&old.disposition, &record.disposition),
                    (Disposition::Imported { original: old }, Disposition::Imported { original: new }) if old == new)
        });
        self.records.push(record);
    }
    pub fn forget(&mut self, identity: &Identity) {
        self.records.retain(|record| &record.identity != identity);
    }
    pub fn disposition(&self, destination: &Path, identity: &Identity) -> Option<&Disposition> {
        self.records
            .iter()
            .find(|record| &record.identity == identity)
            .and_then(|record| match &record.disposition {
                Disposition::Hidden => Some(&record.disposition),
                Disposition::Imported { original }
                    if original.is_relative()
                        && original
                            .components()
                            .all(|part| matches!(part, std::path::Component::Normal(_)))
                        && destination.join(original).is_file() =>
                {
                    Some(&record.disposition)
                }
                _ => None,
            })
    }
    pub fn save(&self, destination: &Path) -> Result<()> {
        let path = Self::path(destination);
        fs::create_dir_all(path.parent().unwrap())?;
        let bytes = serde_json::to_vec_pretty(self)?;
        if bytes.len() as u64 > MAX_HISTORY_BYTES {
            return Err("import history exceeds 32 MiB".into());
        }
        let mut temporary = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist(path)?;
        Ok(())
    }
}

/// Keep each photo's outputs together, with distinct names within a submitted batch.
pub fn available_name(
    source: &Path,
    originals: &Path,
    destination: &Path,
    extensions: &[&str],
    conflict: ConflictPolicy,
    reserved: &HashSet<PathBuf>,
) -> Result<PathBuf> {
    let name = source.file_name().ok_or("source has no filename")?;
    let stem = source.file_stem().ok_or("source has no stem")?;
    let extension = source.extension().ok_or("source has no extension")?;
    for suffix in 0..u32::MAX {
        let name = if suffix == 0 {
            name.to_os_string()
        } else {
            let mut name = stem.to_os_string();
            name.push(format!("-{suffix}."));
            name.push(extension);
            name
        };
        let name = PathBuf::from(name);
        let paths: Vec<_> = std::iter::once(originals.join(&name))
            .chain(std::iter::once(crate::sidecar::path(
                &originals.join(&name),
            )))
            .chain(
                extensions
                    .iter()
                    .map(|extension| destination.join(&name).with_extension(extension)),
            )
            .collect();
        if paths.iter().all(|path| {
            !reserved.contains(path) && (conflict == ConflictPolicy::Overwrite || !path.exists())
        }) {
            return Ok(name);
        }
    }
    Err("no free import filename".into())
}

/// Copy with the selected conflict policy, publishing only a completed file.
/// The original's modification time is preserved. Creation time is platform-specific.
pub fn copy_original(
    source: &Path,
    destination: &Path,
    expected: &Identity,
    conflict: ConflictPolicy,
) -> Result<()> {
    if &Identity::read(source)? != expected {
        return Err("source changed since import was submitted".into());
    }
    let mut input = File::open(source)?;
    let metadata = input.metadata()?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(destination.parent().ok_or("destination has no parent")?)?;
    let mut chunk = vec![0; COPY_CHUNK_BYTES];
    let mut length = 0;
    loop {
        crate::background::checkpoint();
        let read = input.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        temporary.write_all(&chunk[..read])?;
        length += read as u64;
    }
    if length != expected.length
        || &Identity::read(source)? != expected
        || fs::metadata(source)?.modified()? != metadata.modified()?
    {
        return Err("source changed while importing".into());
    }
    temporary
        .as_file()
        .set_times(FileTimes::new().set_modified(metadata.modified()?))?;
    temporary.as_file().sync_all()?;
    match conflict {
        ConflictPolicy::Overwrite => temporary.persist(destination)?,
        ConflictPolicy::Rename => temporary.persist_noclobber(destination)?,
    };
    Ok(())
}

/// Determines whether a completed file replaces an existing destination or keeps both versions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConflictPolicy {
    #[default]
    Overwrite,
    Rename,
}

/// Render beside the destination and publish only a complete, synced output.
/// Overwrite replaces atomically. Rename refuses a collision at publication.
pub fn write_output(
    destination: &Path,
    conflict: ConflictPolicy,
    write: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let directory = tempfile::tempdir_in(destination.parent().ok_or("destination has no parent")?)?;
    let temporary = directory.path().join(
        destination
            .file_name()
            .ok_or("destination has no filename")?,
    );
    write(&temporary)?;
    File::open(&temporary)?.sync_all()?;
    let temporary = tempfile::TempPath::try_from_path(temporary)?;
    match conflict {
        ConflictPolicy::Overwrite => temporary.persist(destination)?,
        ConflictPolicy::Rename => temporary.persist_noclobber(destination)?,
    }
    Ok(())
}

#[cfg(test)]
#[path = "importing_test.rs"]
mod tests;
