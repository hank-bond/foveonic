//! Holds images the application may show again, so the scheduler can drop them before working photos.

use super::MemoryReservation;
use std::{
    any::Any,
    sync::{Arc, Mutex, Weak},
};

struct Cached {
    value: Arc<dyn Any + Send + Sync>,
    reservation: MemoryReservation,
}

/// Shared between the application, which holds handles, and the scheduler,
/// which counts and drops what is cached here.
#[derive(Clone, Default)]
pub struct ImageCache(Arc<Mutex<Vec<Cached>>>);

/// The application's claim on a cached image. Dropping it discards the image.
pub struct CachedImage<T: Any + Send + Sync> {
    /// None once taken.
    value: Option<Weak<T>>,
    store: ImageCache,
}

impl ImageCache {
    /// Cache an image that is not visible now. `reservation` describes its memory.
    pub fn insert<T: Any + Send + Sync>(
        &self,
        value: T,
        reservation: MemoryReservation,
    ) -> CachedImage<T> {
        let value = Arc::new(value);
        let handle = CachedImage {
            value: Some(Arc::downgrade(&value)),
            store: self.clone(),
        };
        self.0.lock().unwrap().push(Cached { value, reservation });
        handle
    }

    /// Memory of every cached image, counted once with shared storage.
    pub fn reservation(&self) -> MemoryReservation {
        let mut total = MemoryReservation::default();
        for entry in self.0.lock().unwrap().iter() {
            total.merge(&entry.reservation);
        }
        total
    }

    /// Drop the oldest cached image. Returns false when the cache is empty.
    pub fn drop_oldest(&self) -> bool {
        // Release the image after unlocking, so freeing it never blocks the store.
        let oldest = {
            let mut cache = self.0.lock().unwrap();
            (!cache.is_empty()).then(|| cache.remove(0))
        };
        oldest.is_some()
    }

    fn remove<T: Any + Send + Sync>(&self, value: &Weak<T>) -> Option<T> {
        let entry = {
            let mut cache = self.0.lock().unwrap();
            let index = cache.iter().position(|entry| {
                std::ptr::addr_eq(Arc::as_ptr(&entry.value), Weak::as_ptr(value))
            })?;
            cache.remove(index)
        };
        Arc::try_unwrap(entry.value.downcast::<T>().ok()?).ok()
    }
}

impl<T: Any + Send + Sync> CachedImage<T> {
    /// Take the image back to show it. None when the scheduler dropped it.
    pub fn take(mut self) -> Option<T> {
        self.store.remove(&self.value.take()?)
    }
}

impl<T: Any + Send + Sync> Drop for CachedImage<T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            self.store.remove(&value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cached_count(cache: &ImageCache) -> usize {
        cache.0.lock().unwrap().len()
    }

    #[test]
    fn taking_returns_ownership_and_dropping_a_handle_discards() {
        let cache = ImageCache::default();
        let first = cache.insert(String::from("before"), MemoryReservation::default());
        let second = cache.insert(String::from("detail"), MemoryReservation::default());
        assert_eq!(second.take().as_deref(), Some("detail"));
        drop(first);
        assert_eq!(cached_count(&cache), 0);
    }

    #[test]
    fn a_dropped_image_cannot_be_taken() {
        let cache = ImageCache::default();
        let handle = cache.insert(String::from("before"), MemoryReservation::default());
        assert!(cache.drop_oldest());
        assert!(handle.take().is_none());
        assert!(!cache.drop_oldest());
    }

    #[test]
    fn reservation_sums_exclusive_bytes() {
        let cache = ImageCache::default();
        let handles: Vec<_> = [100, 23]
            .into_iter()
            .map(|bytes| {
                let mut reservation = MemoryReservation::default();
                reservation.exclusive_bytes = bytes;
                cache.insert(bytes, reservation)
            })
            .collect();
        assert_eq!(cache.reservation().exclusive_bytes, 123);
        drop(handles);
        assert_eq!(cache.reservation().exclusive_bytes, 0);
    }
}
