//! Retains the two source-derived calibration statistics without owning sensor samples.

use super::{recipes::RecipeInput, Engine};
use crate::{
    recipe::{Key, RetainedBytes},
    source::{Result, Source, SourceOrigin},
};
use std::sync::{Arc, Mutex};

const CAPACITY: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatisticKind {
    Neutral,
    ChannelMean,
}

pub(crate) struct CalibrationEstimate {
    origin: SourceOrigin,
    kind: StatisticKind,
    descriptor: Key,
    values: [f64; 3],
}
impl CalibrationEstimate {
    fn matches(&self, origin: &SourceOrigin, kind: StatisticKind, descriptor: &Key) -> bool {
        self.origin == *origin && self.kind == kind && self.descriptor == *descriptor
    }
    fn account(record: &Arc<Self>, retained: &mut RetainedBytes) {
        if retained.owner(
            record.clone(),
            std::mem::size_of::<Self>() + record.origin.allocated_bytes(),
        ) {
            record.descriptor.account(retained);
        }
    }
}

/// Only statistics actually consumed by this source's authored recipes are pinned.
#[derive(Default)]
pub(crate) struct SourcePreparation {
    estimates: Mutex<Vec<Arc<CalibrationEstimate>>>,
}
impl SourcePreparation {
    fn find(
        &self,
        origin: &SourceOrigin,
        kind: StatisticKind,
        descriptor: &Key,
    ) -> Option<Arc<CalibrationEstimate>> {
        let mut estimates = self.estimates.lock().expect("source preparation");
        let index = estimates
            .iter()
            .position(|entry| entry.matches(origin, kind, descriptor))?;
        let record = estimates.remove(index);
        estimates.push(record.clone());
        Some(record)
    }
    fn pin(&self, record: &Arc<CalibrationEstimate>) {
        let mut estimates = self.estimates.lock().expect("source preparation");
        estimates.retain(|entry| !Arc::ptr_eq(entry, record));
        if estimates.len() == CAPACITY {
            estimates.remove(0);
        }
        estimates.push(record.clone());
    }
    pub(crate) fn account(&self, retained: &mut RetainedBytes) -> usize {
        let estimates = self.estimates.lock().expect("source preparation");
        for entry in estimates.iter() {
            CalibrationEstimate::account(entry, retained);
        }
        estimates.capacity() * std::mem::size_of::<Arc<CalibrationEstimate>>()
    }
}

impl Engine {
    pub(super) fn trim_cold_source_preparation(&self, protected: &[&SourceOrigin]) -> bool {
        let mut cache = self
            .calibration_statistics
            .lock()
            .expect("calibration statistics cache");
        let Some(index) = cache
            .iter()
            .position(|entry| !protected.contains(&&entry.origin))
        else {
            return false;
        };
        cache.remove(index);
        if cache.is_empty() {
            cache.shrink_to_fit();
        }
        true
    }
    pub(super) fn source_statistic(
        &self,
        input: RecipeInput<'_>,
        kind: StatisticKind,
        descriptor: Key,
        calculate: impl FnOnce(&Source) -> Result<[f64; 3]>,
    ) -> Result<[f64; 3]> {
        if let Some(record) = input
            .preparation()
            .and_then(|prepared| prepared.find(input.origin(), kind, &descriptor))
        {
            return Ok(record.values);
        }
        let mut cache = self
            .calibration_statistics
            .lock()
            .map_err(|_| "calibration statistics cache poisoned")?;
        let record = if let Some(index) = cache
            .iter()
            .position(|entry| entry.matches(input.origin(), kind, &descriptor))
        {
            let entry = cache.remove(index);
            cache.push(entry.clone());
            entry
        } else {
            let values = calculate(input.original()?)?;
            let record = Arc::new(CalibrationEstimate {
                origin: input.origin().clone(),
                kind,
                descriptor,
                values,
            });
            if cache.len() == CAPACITY {
                cache.remove(0);
            }
            cache.push(record.clone());
            record
        };
        drop(cache);
        if let Some(prepared) = input.preparation() {
            prepared.pin(&record);
        }
        Ok(record.values)
    }
    pub(super) fn account_source_preparation(&self, retained: &mut RetainedBytes) {
        let cache = self
            .calibration_statistics
            .lock()
            .expect("calibration statistics cache");
        retained.add_bytes(cache.capacity() * std::mem::size_of::<Arc<CalibrationEstimate>>());
        for entry in cache.iter() {
            CalibrationEstimate::account(entry, retained);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_keep_exact_statistics_alive_and_inventory_deduplicates_owners() {
        let origin = SourceOrigin::default();
        let descriptor = Key::pod(&[1_u32, 2, 3]);
        let record = Arc::new(CalibrationEstimate {
            origin: origin.clone(),
            kind: StatisticKind::Neutral,
            descriptor: descriptor.clone(),
            values: [1., 2., 3.],
        });
        let weak = Arc::downgrade(&record);
        let first = SourcePreparation::default();
        let second = SourcePreparation::default();
        first.pin(&record);
        second.pin(&record);
        let mut inventory = RetainedBytes::default();
        first.account(&mut inventory);
        let once = inventory.bytes();
        second.account(&mut inventory);
        CalibrationEstimate::account(&record, &mut inventory);
        assert_eq!(inventory.bytes(), once);
        drop(inventory);
        drop(record);
        assert!(
            weak.upgrade().is_some(),
            "session pins survive lookup eviction"
        );
        assert_eq!(
            first
                .find(&origin, StatisticKind::Neutral, &descriptor)
                .unwrap()
                .values,
            [1., 2., 3.]
        );
        assert!(first
            .find(
                &SourceOrigin::default(),
                StatisticKind::Neutral,
                &descriptor
            )
            .is_none());
        assert!(first
            .find(&origin, StatisticKind::ChannelMean, &descriptor)
            .is_none());
        assert!(first
            .find(&origin, StatisticKind::Neutral, &Key::pod(&[1_u32, 2, 4]))
            .is_none());
        drop(first);
        drop(second);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn preparation_history_is_bounded_without_aliasing_changed_descriptors() {
        let prepared = SourcePreparation::default();
        let origin = SourceOrigin::default();
        for value in 0..CAPACITY + 1 {
            prepared.pin(&Arc::new(CalibrationEstimate {
                origin: origin.clone(),
                kind: StatisticKind::Neutral,
                descriptor: Key::pod(&(value as u32)),
                values: [value as f64; 3],
            }));
        }
        assert_eq!(prepared.estimates.lock().unwrap().len(), CAPACITY);
        assert!(prepared
            .find(&origin, StatisticKind::Neutral, &Key::pod(&0_u32))
            .is_none());
        assert_eq!(
            prepared
                .find(
                    &origin,
                    StatisticKind::Neutral,
                    &Key::pod(&(CAPACITY as u32))
                )
                .unwrap()
                .values,
            [CAPACITY as f64; 3]
        );
    }
}
