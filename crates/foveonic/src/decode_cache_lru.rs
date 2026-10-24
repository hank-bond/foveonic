//! Applies the shared disk budget while favoring nearby decoded sources and display previews.

use super::{previews, Identity};
use crate::source::Result;
use std::{
    cmp::Reverse,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub(super) type Entry = (PathBuf, u64, SystemTime);

// Lower priorities survive longer. Prefer a photo's decode over its preview.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EntryKind {
    Decoded,
    Preview,
}
type Priority = (usize, EntryKind);

pub(super) fn prune(root: &Path, budget: u64, nearby: &[PathBuf]) -> Result<Vec<Entry>> {
    let entries = entries(root)?;
    let remove = evictions(entries, budget, &priorities(root, nearby));
    let mut retained = Vec::new();
    for (entry, evict) in remove {
        if evict {
            fs::remove_file(entry.0)?;
        } else {
            retained.push(entry);
        }
    }
    Ok(retained)
}

/// Consider the incoming file before evicting anything. A distant insertion must
/// be skipped when fitting it would evict a closer photo. Replacements count once.
pub(super) fn reserve(
    root: &Path,
    destination: &Path,
    bytes: u64,
    budget: u64,
    nearby: &[PathBuf],
) -> Result<bool> {
    if bytes > budget {
        return Ok(false);
    }
    let mut entries = entries(root)?;
    entries.retain(|entry| entry.0 != destination);
    entries.push((destination.to_owned(), bytes, SystemTime::now()));
    let remove = evictions(entries, budget, &priorities(root, nearby));
    if remove
        .iter()
        .any(|(entry, evict)| entry.0 == destination && *evict)
    {
        return Ok(false);
    }
    for (entry, evict) in remove {
        if evict {
            fs::remove_file(entry.0)?;
        }
    }
    Ok(true)
}

fn entries(root: &Path) -> Result<Vec<Entry>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    Ok(fs::read_dir(root)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if !path
                .extension()
                .is_some_and(|s| s == "decode" || s == "preview")
            {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            Some((path, metadata.len(), metadata.modified().ok()?))
        })
        .collect())
}

fn priorities(root: &Path, nearby: &[PathBuf]) -> HashMap<PathBuf, Priority> {
    let mut priorities = HashMap::new();
    for (rank, path) in nearby.iter().enumerate() {
        if let Ok(identity) = Identity::read(path) {
            priorities.insert(root.join(identity.name()), (rank, EntryKind::Decoded));
        }
        if let Ok(location) = previews::location(root, path) {
            priorities.insert(location, (rank, EntryKind::Preview));
        }
    }
    priorities
}

fn evictions(
    mut entries: Vec<Entry>,
    budget: u64,
    priorities: &HashMap<PathBuf, Priority>,
) -> Vec<(Entry, bool)> {
    // Unrelated entries retain ordinary LRU order. Within the current window,
    // evict distant photos before closer ones, even when those are older.
    entries.sort_by_key(|entry| {
        (
            Reverse(
                priorities
                    .get(&entry.0)
                    .copied()
                    .unwrap_or((usize::MAX, EntryKind::Preview)),
            ),
            entry.2,
        )
    });
    let mut total: u64 = entries.iter().map(|entry| entry.1).sum();
    entries
        .into_iter()
        .map(|entry| {
            let evict = total > budget;
            if evict {
                total -= entry.1;
            }
            (entry, evict)
        })
        .collect()
}

#[cfg(test)]
#[path = "decode_cache_lru_test.rs"]
mod tests;
