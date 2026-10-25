//! Retains original X3F bytes for imports under the decoded cache's identity and budget.

use super::{lru, Cache, Identity, Settings, TEMP_SEQUENCE};
use crate::source::Result;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::{Duration, UNIX_EPOCH},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn store(
    root: &Path,
    identity: &Identity,
    original: &[u8],
    settings: &Settings,
    nearby: &[PathBuf],
    current: &impl Fn() -> bool,
) -> Result<u32> {
    if original.len() as u64 != identity.length {
        return Err("original cache length mismatch".into());
    }
    let destination = root.join(identity.name()).with_extension("original");
    if !current()
        || !lru::reserve(
            root,
            &destination,
            identity.length,
            settings.max_bytes,
            nearby,
        )?
    {
        return Err("original cache admission skipped".into());
    }
    let temporary = root.join(format!(
        "{}.{}.writing",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<u32> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        let mut checksum = crc32fast::Hasher::new();
        for chunk in original.chunks(crate::background::IO_CHUNK_BYTES) {
            crate::background::checkpoint_or_cancel()?;
            checksum.update(chunk);
            file.write_all(chunk)?;
        }
        file.set_modified(std::time::SystemTime::now())?;
        drop(file);
        crate::background::checkpoint_or_cancel()?;
        if !current() {
            return Err("original cache preparation superseded".into());
        }
        fs::rename(&temporary, destination)?;
        Ok(checksum.finalize())
    })();
    let _ = fs::remove_file(temporary);
    result
}

impl Cache {
    /// Derive import identity from the verified snapshot without statting the card.
    pub fn import_identity(&self, path: &Path) -> Option<crate::importing::Identity> {
        let identity = self.control.lock().unwrap().identities.get(path).cloned()?;
        Some(crate::importing::Identity::from_snapshot(
            path,
            identity.length,
            identity.modified_seconds,
            identity.modified_nanos,
        ))
    }

    /// Copy locally cached original bytes. A miss falls back to the normal importer.
    pub fn copy_original(
        &self,
        path: &Path,
        destination: &Path,
        expected: &crate::importing::Identity,
        conflict: crate::importing::ConflictPolicy,
    ) -> Result<bool> {
        if self.import_identity(path).as_ref() != Some(expected) {
            return Ok(false);
        }
        let identity = self
            .control
            .lock()
            .unwrap()
            .identities
            .get(path)
            .cloned()
            .ok_or("cached source invalidated")?;
        let Some(selection) = self.selection(path) else {
            return Ok(false);
        };
        let original = self.root.join(identity.name()).with_extension("original");
        let mut file = match fs::File::open(&original) {
            Ok(f) => f,
            Err(_) => return Ok(false),
        };
        let mut temporary = tempfile::NamedTempFile::new_in(
            destination.parent().ok_or("destination has no parent")?,
        )?;
        let mut chunk = vec![0; crate::background::IO_CHUNK_BYTES];
        let mut checksum = crc32fast::Hasher::new();
        let mut length = 0;
        loop {
            crate::background::checkpoint();
            let size = match file.read(&mut chunk) {
                Ok(size) => size,
                Err(_) => return Ok(false),
            };
            if size == 0 {
                break;
            }
            checksum.update(&chunk[..size]);
            temporary.write_all(&chunk[..size])?;
            length += size as u64;
        }
        if length != identity.length || checksum.finalize() != selection.original_checksum {
            return Ok(false);
        }
        if self.import_identity(path).as_ref() != Some(expected) {
            return Err("cached source changed during import".into());
        }
        temporary.as_file().set_modified(
            UNIX_EPOCH + Duration::new(identity.modified_seconds, identity.modified_nanos),
        )?;
        temporary.as_file().sync_all()?;
        match conflict {
            crate::importing::ConflictPolicy::Overwrite => temporary.persist(destination)?,
            crate::importing::ConflictPolicy::Rename => temporary.persist_noclobber(destination)?,
        };
        Ok(true)
    }
}
