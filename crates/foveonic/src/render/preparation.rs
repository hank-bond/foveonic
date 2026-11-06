//! Reuses bounded immutable metadata preparation independently of decoded sensor ownership.

use super::Engine;
use crate::{
    nodes::{color::color_mode, finish::tone, sensor::highlight},
    recipe::{Key, RetainedBytes, Shared},
    source::{Metadata, Result},
};
use std::sync::{Arc, Weak};

const TONES_PER_SOURCE: usize = 2;
// Eight photos can each retain their Adaptive basis and adjusted response.
const TONE_CAPACITY: usize = 16;

struct Tone {
    metadata: Weak<Metadata>,
    key: Key,
    table: Shared<f32>,
}
struct Highlights {
    metadata: Weak<Metadata>,
    threshold: Option<u32>,
    prepared: Arc<highlight::Prepared>,
}
fn same_metadata(previous: &Weak<Metadata>, metadata: &Arc<Metadata>) -> bool {
    previous
        .upgrade()
        .is_some_and(|old| Arc::ptr_eq(&old, metadata))
}

#[derive(Default)]
pub(super) struct Cache {
    tones: Vec<Tone>,
    highlights: Option<Highlights>,
}
impl Cache {
    fn account(&self, retained: &mut RetainedBytes) {
        for entry in &self.tones {
            entry.key.account(retained);
            entry.table.account(retained);
        }
        if let Some(entry) = &self.highlights {
            entry.prepared.flags.account(retained);
        }
    }
    #[cfg(test)]
    fn bytes(&self) -> usize {
        let mut retained = RetainedBytes::default();
        self.account(&mut retained);
        retained.bytes()
    }
}

impl Engine {
    /// Adds cached payloads to a shared inventory of engine and session ownership.
    pub(crate) fn account_preparation(&self, retained: &mut RetainedBytes) {
        self.preparation
            .lock()
            .expect("metadata preparation cache")
            .account(retained);
    }

    /// Logical retained payloads and key-part storage, excluding allocator/Arc
    /// overhead and preparations owned only by sessions or rendered artifacts.
    pub fn cached_preparation_bytes(&self) -> usize {
        let mut retained = RetainedBytes::default();
        self.account_preparation(&mut retained);
        retained.bytes()
    }

    /// Drops engine-owned preparation when the working-set budget needs space.
    /// Existing graph keys remain immutable readers until their owners release them.
    pub fn clear_preparation(&self) {
        *self.preparation.lock().expect("metadata preparation cache") = Cache::default();
    }

    /// A sealed reduced checkpoint does not need the full sensor's clipping map.
    /// The caller must also release upstream graph keys to release their readers.
    pub fn release_native_preparation(&self, metadata: &Arc<Metadata>) {
        let mut cache = self.preparation.lock().expect("metadata preparation cache");
        if cache
            .highlights
            .as_ref()
            .is_some_and(|entry| same_metadata(&entry.metadata, metadata))
        {
            cache.highlights = None;
        }
    }

    pub(crate) fn prepare_tone(
        &self,
        metadata: &Arc<Metadata>,
        mode: &color_mode::Settings,
        controls: tone::Controls,
        monochrome: bool,
    ) -> Result<Shared<f32>> {
        self.preparation
            .lock()
            .map_err(|_| "metadata preparation cache poisoned")?
            .tone(metadata, mode, controls, monochrome)
    }

    pub(crate) fn prepare_highlights(
        &self,
        metadata: &Arc<Metadata>,
        threshold: Option<f32>,
    ) -> Result<Arc<highlight::Prepared>> {
        self.preparation
            .lock()
            .map_err(|_| "metadata preparation cache poisoned")?
            .highlights(metadata, threshold)
    }
}

impl Cache {
    fn tone(
        &mut self,
        metadata: &Arc<Metadata>,
        mode: &color_mode::Settings,
        controls: tone::Controls,
        monochrome: bool,
    ) -> Result<Shared<f32>> {
        let key = Key::pod(&[controls.contrast, controls.shadows, controls.highlights])
            .with(&u32::from(monochrome))
            .with(&if monochrome { 0 } else { mode.tone_index })
            .with(&if monochrome { 0. } else { mode.contrast });
        self.tones
            .retain(|entry| entry.metadata.strong_count() != 0);
        if let Some(index) = self
            .tones
            .iter()
            .position(|entry| same_metadata(&entry.metadata, metadata) && entry.key == key)
        {
            let entry = self.tones.remove(index);
            let value = entry.table.clone();
            self.tones.push(entry);
            return Ok(value);
        }
        let table: Shared<f32> = if monochrome {
            tone::prepare_monochrome(metadata, controls)?
        } else {
            tone::prepare(metadata, mode, controls)?
        }
        .into();
        if self
            .tones
            .iter()
            .filter(|entry| same_metadata(&entry.metadata, metadata))
            .count()
            == TONES_PER_SOURCE
        {
            let index = self
                .tones
                .iter()
                .position(|entry| same_metadata(&entry.metadata, metadata))
                .unwrap();
            self.tones.remove(index);
        }
        if self.tones.len() == TONE_CAPACITY {
            self.tones.remove(0);
        }
        self.tones.push(Tone {
            metadata: Arc::downgrade(metadata),
            key,
            table: table.clone(),
        });
        Ok(table)
    }

    fn highlights(
        &mut self,
        metadata: &Arc<Metadata>,
        threshold: Option<f32>,
    ) -> Result<Arc<highlight::Prepared>> {
        let key = threshold.map(f32::to_bits);
        let previous = self
            .highlights
            .as_ref()
            .filter(|entry| same_metadata(&entry.metadata, metadata));
        if let Some(entry) = previous.filter(|entry| entry.threshold == key) {
            return Ok(entry.prepared.clone());
        }
        let prepared = Arc::new(match previous {
            Some(entry) => highlight::prepare_with_flags(
                metadata,
                threshold,
                Some(entry.prepared.flags.clone()),
            )?,
            None => highlight::prepare(metadata, threshold)?,
        });
        self.highlights = Some(Highlights {
            metadata: Arc::downgrade(metadata),
            threshold: key,
            prepared: prepared.clone(),
        });
        Ok(prepared)
    }
}

#[cfg(test)]
#[path = "preparation_test.rs"]
mod tests;
