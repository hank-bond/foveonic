//! Reuses bounded immutable metadata preparation independently of decoded sensor ownership.

use super::Engine;
use crate::{
    nodes::{
        color::color_mode,
        finish::tone,
        sensor::{chroma_repair, highlight, ordered_blue_repair},
    },
    recipe::{Key, RetainedBytes, Shared},
    source::{Metadata, Result},
};
use std::sync::{Arc, Weak};

const TONES_PER_SOURCE: usize = 2;
// Eight photos can each retain their Adaptive basis and adjusted response.
const TONE_CAPACITY: usize = 16;
const REPAIR_SOURCES: usize = 8;
pub(super) const REPAIR_BYTES: usize = 64 * 1024 * 1024;

struct Tone {
    metadata: Weak<Metadata>,
    key: Key,
    table: Shared<f32>,
}
struct Clipping {
    metadata: Weak<Metadata>,
    grid: Option<super::WorkingGrid>,
    flags: Shared<u32>,
}
enum Repair {
    Blue(Arc<ordered_blue_repair::Prepared>),
    Chroma(Arc<chroma_repair::Prepared>),
}
impl Repair {
    fn account(&self, retained: &mut RetainedBytes) {
        match self {
            Self::Blue(prepared) => {
                if retained.owner(
                    prepared.clone(),
                    std::mem::size_of::<ordered_blue_repair::Prepared>(),
                ) {
                    prepared.account(retained);
                }
            }
            Self::Chroma(prepared) => {
                if retained.owner(
                    prepared.clone(),
                    std::mem::size_of::<chroma_repair::Prepared>(),
                ) {
                    prepared.account(retained);
                }
            }
        }
    }
}
struct RepairEntry {
    metadata: Weak<Metadata>,
    prepared: Repair,
    bytes: usize,
}
fn same_metadata(previous: &Weak<Metadata>, metadata: &Arc<Metadata>) -> bool {
    previous
        .upgrade()
        .is_some_and(|old| Arc::ptr_eq(&old, metadata))
}

#[derive(Default)]
pub(super) struct Cache {
    tones: Vec<Tone>,
    clipping: Option<Clipping>,
    repairs: Vec<RepairEntry>,
}
impl Cache {
    fn trim_cold(&mut self, retained: &[&Arc<Metadata>]) -> bool {
        let cold = |metadata: &Weak<Metadata>| {
            !retained
                .iter()
                .any(|current| same_metadata(metadata, current))
        };
        if let Some(index) = self.tones.iter().position(|entry| cold(&entry.metadata)) {
            self.tones.remove(index);
            if self.tones.is_empty() {
                self.tones.shrink_to_fit();
            }
            return true;
        }
        if self
            .clipping
            .as_ref()
            .is_some_and(|entry| cold(&entry.metadata))
        {
            self.clipping = None;
            return true;
        }
        if let Some(index) = self.repairs.iter().position(|entry| cold(&entry.metadata)) {
            self.repairs.remove(index);
            if self.repairs.is_empty() {
                self.repairs.shrink_to_fit();
            }
            return true;
        }
        false
    }
    /// Metadata is immutable; the numerical stage is the only additional input.
    /// Neither the cache nor these preparations retain decoded sensor storage.
    fn blue_repair(
        &mut self,
        metadata: &Arc<Metadata>,
        stage: ordered_blue_repair::Stage,
        build: impl FnOnce() -> Result<ordered_blue_repair::Prepared>,
    ) -> Result<Arc<ordered_blue_repair::Prepared>> {
        self.repairs
            .retain(|entry| entry.metadata.strong_count() > 0);
        if let Some(index) = self.repairs.iter().position(|entry| {
            same_metadata(&entry.metadata, metadata)
                && matches!(&entry.prepared, Repair::Blue(prepared) if prepared.stage() == stage)
        }) {
            let entry = self.repairs.remove(index);
            let Repair::Blue(prepared) = &entry.prepared else {
                unreachable!()
            };
            let prepared = prepared.clone();
            self.repairs.push(entry);
            return Ok(prepared);
        }
        let prepared = Arc::new(build()?);
        self.insert_repair(metadata, Repair::Blue(prepared.clone()));
        Ok(prepared)
    }
    fn chroma_repair(
        &mut self,
        metadata: &Arc<Metadata>,
        build: impl FnOnce() -> Result<chroma_repair::Prepared>,
    ) -> Result<Arc<chroma_repair::Prepared>> {
        self.repairs
            .retain(|entry| entry.metadata.strong_count() > 0);
        if let Some(index) = self.repairs.iter().position(|entry| {
            same_metadata(&entry.metadata, metadata) && matches!(&entry.prepared, Repair::Chroma(_))
        }) {
            let entry = self.repairs.remove(index);
            let Repair::Chroma(prepared) = &entry.prepared else {
                unreachable!()
            };
            let prepared = prepared.clone();
            self.repairs.push(entry);
            return Ok(prepared);
        }
        let prepared = Arc::new(build()?);
        self.insert_repair(metadata, Repair::Chroma(prepared.clone()));
        Ok(prepared)
    }
    fn account(&self, retained: &mut RetainedBytes) {
        retained.add_bytes(self.tones.capacity() * size_of::<Tone>());
        for entry in &self.tones {
            entry.key.account(retained);
            entry.table.account(retained);
        }
        if let Some(entry) = &self.clipping {
            entry.flags.account(retained);
        }
        retained.add_bytes(self.repairs.capacity() * std::mem::size_of::<RepairEntry>());
        for entry in &self.repairs {
            entry.prepared.account(retained);
        }
    }
    fn insert_repair(&mut self, metadata: &Arc<Metadata>, prepared: Repair) {
        let mut retained = RetainedBytes::default();
        prepared.account(&mut retained);
        self.repairs.push(RepairEntry {
            metadata: Arc::downgrade(metadata),
            prepared,
            bytes: retained.bytes(),
        });
        // Keep every lazily consumed stage of the current photo together. An
        // unusually large catalogue can exceed the target alone; retaining that
        // explicitly accounted bundle avoids rebuilding it on every edit.
        loop {
            let bytes: usize = self.repairs.iter().map(|entry| entry.bytes).sum();
            let mut sources = Vec::new();
            for entry in &self.repairs {
                if !sources.contains(&entry.metadata.as_ptr()) {
                    sources.push(entry.metadata.as_ptr());
                }
            }
            if bytes <= REPAIR_BYTES && sources.len() <= REPAIR_SOURCES {
                break;
            }
            let Some(index) = self
                .repairs
                .iter()
                .position(|entry| !same_metadata(&entry.metadata, metadata))
            else {
                break;
            };
            self.repairs.remove(index);
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
    pub(super) fn trim_cold_preparation(
        &self,
        retained: &[&Arc<Metadata>],
        origins: &[&crate::source::SourceOrigin],
    ) -> bool {
        if self
            .preparation
            .lock()
            .expect("metadata preparation cache")
            .trim_cold(retained)
        {
            return true;
        }
        self.trim_cold_source_preparation(origins)
    }
    pub(crate) fn prepare_blue_repair(
        &self,
        metadata: &Arc<Metadata>,
        stage: ordered_blue_repair::Stage,
        build: impl FnOnce() -> Result<ordered_blue_repair::Prepared>,
    ) -> Result<Arc<ordered_blue_repair::Prepared>> {
        self.preparation
            .lock()
            .map_err(|_| "metadata preparation cache poisoned")?
            .blue_repair(metadata, stage, build)
    }
    pub(crate) fn prepare_chroma_repair(
        &self,
        metadata: &Arc<Metadata>,
        build: impl FnOnce() -> Result<chroma_repair::Prepared>,
    ) -> Result<Arc<chroma_repair::Prepared>> {
        self.preparation
            .lock()
            .map_err(|_| "metadata preparation cache poisoned")?
            .chroma_repair(metadata, build)
    }
    /// Adds cached payloads to a shared inventory of engine and session ownership.
    pub(crate) fn account_preparation(&self, retained: &mut RetainedBytes) {
        self.preparation
            .lock()
            .expect("metadata preparation cache")
            .account(retained);
        self.account_source_preparation(retained);
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
        *self
            .calibration_statistics
            .lock()
            .expect("calibration statistics cache") = Vec::new();
    }

    /// Reduced sensor data does not need the full sensor's clipping map.
    /// The caller must also release upstream graph keys to release their readers.
    pub fn release_native_preparation(&self, metadata: &Arc<Metadata>) {
        let mut cache = self.preparation.lock().expect("metadata preparation cache");
        if cache
            .clipping
            .as_ref()
            .is_some_and(|entry| same_metadata(&entry.metadata, metadata) && entry.grid.is_none())
        {
            cache.clipping = None;
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

    pub(crate) fn clipping_flags(
        &self,
        metadata: &Arc<Metadata>,
        grid: Option<super::WorkingGrid>,
    ) -> Result<Shared<u32>> {
        self.preparation
            .lock()
            .map_err(|_| "metadata preparation cache poisoned")?
            .clipping_flags(metadata, grid)
    }

    pub(crate) fn materialize_highlights(
        &self,
        metadata: &Arc<Metadata>,
        description: &highlight::Prepared,
    ) -> Result<Arc<highlight::Prepared>> {
        self.preparation
            .lock()
            .map_err(|_| "metadata preparation cache poisoned")?
            .materialize_highlights(metadata, description)
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

    #[cfg(test)]
    fn highlights(
        &mut self,
        metadata: &Arc<Metadata>,
        threshold: Option<f32>,
    ) -> Result<Arc<highlight::Prepared>> {
        self.materialize_highlights(metadata, &highlight::deferred(metadata, threshold)?)
    }

    fn clipping_flags(
        &mut self,
        metadata: &Arc<Metadata>,
        grid: Option<super::WorkingGrid>,
    ) -> Result<Shared<u32>> {
        if let Some(entry) = self.clipping.as_ref().filter(|entry| {
            same_metadata(&entry.metadata, metadata)
                && highlight::clipping_key(entry.grid) == highlight::clipping_key(grid)
        }) {
            return Ok(entry.flags.clone());
        }
        let flags: Shared<u32> = highlight::clipping(metadata, grid)?.into();
        self.clipping = Some(Clipping {
            metadata: Arc::downgrade(metadata),
            grid,
            flags: flags.clone(),
        });
        Ok(flags)
    }
    fn materialize_highlights(
        &mut self,
        metadata: &Arc<Metadata>,
        description: &highlight::Prepared,
    ) -> Result<Arc<highlight::Prepared>> {
        let flags = self.clipping_flags(metadata, description.grid)?;
        Ok(Arc::new(highlight::materialize(
            metadata,
            description,
            Some(flags),
        )?))
    }
}

#[cfg(test)]
#[path = "preparation_test.rs"]
mod tests;
