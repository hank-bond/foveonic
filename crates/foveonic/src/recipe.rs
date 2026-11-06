//! Keeps compact parameter fingerprints with shared immutable payloads and exact collision checks.

use bytemuck::Pod;
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
    sync::Arc,
};

fn fingerprint(bytes: &[u8]) -> u64 {
    // A fingerprint only rejects unequal payloads. Shared ownership or exact
    // bytes still decide every match, including CRC32 collisions.
    u64::from(crc32fast::hash(bytes))
}
fn combined(previous: u64, next: u64, kind: u8) -> u64 {
    let mut hasher = DefaultHasher::new();
    (previous, next, kind).hash(&mut hasher);
    hasher.finish()
}

trait Payload: Send + Sync {
    fn bytes(&self) -> &[u8];
    fn fingerprint(&self) -> u64;
}
struct Data<T> {
    values: Box<[T]>,
    fingerprint: u64,
}
impl<T: Pod + Send + Sync> Payload for Data<T> {
    fn bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.values)
    }
    fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}

/// Counts retained immutable payloads once across numerical tables and their keys.
/// Owners stay alive during the inventory so allocation addresses cannot be reused.
#[derive(Default)]
pub(crate) struct RetainedBytes {
    bytes: usize,
    payload_bytes: usize,
    payloads: HashMap<usize, Arc<dyn Payload>>,
    owners: HashMap<usize, Arc<dyn std::any::Any + Send + Sync>>,
}
impl RetainedBytes {
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub(crate) fn payload_bytes(&self) -> usize {
        self.payload_bytes
    }
    pub(crate) fn add_bytes(&mut self, bytes: usize) {
        self.bytes += bytes;
    }
    pub(crate) fn owner<T: Send + Sync + 'static>(&mut self, owner: Arc<T>, bytes: usize) -> bool {
        let address = Arc::as_ptr(&owner) as usize;
        if let std::collections::hash_map::Entry::Vacant(entry) = self.owners.entry(address) {
            self.bytes += bytes;
            entry.insert(owner);
            true
        } else {
            false
        }
    }
    fn payload(&mut self, payload: Arc<dyn Payload>) {
        let address = Arc::as_ptr(&payload) as *const () as usize;
        self.payloads.entry(address).or_insert_with(|| {
            self.bytes += payload.bytes().len();
            self.payload_bytes += payload.bytes().len();
            payload
        });
    }
}

/// An immutable numerical table whose fingerprint is calculated only on creation.
/// Keys share its allocation and still compare exact bits after a fingerprint match.
pub(crate) struct Shared<T>(Arc<Data<T>>);
impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: Pod + Send + Sync + 'static> Shared<T> {
    pub fn key(&self) -> Key {
        Key::payload(self.0.clone())
    }
    pub fn shares_storage(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    #[cfg(test)]
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of_val(&*self.0.values)
    }
    pub fn account(&self, retained: &mut RetainedBytes) {
        retained.payload(self.0.clone());
    }
}
impl<T: Pod + Send + Sync + 'static> From<Vec<T>> for Shared<T> {
    fn from(values: Vec<T>) -> Self {
        let fingerprint = fingerprint(bytemuck::cast_slice(&values));
        Self(Arc::new(Data {
            values: values.into_boxed_slice(),
            fingerprint,
        }))
    }
}
impl<T: Pod + Send + Sync + 'static> Default for Shared<T> {
    fn default() -> Self {
        Vec::new().into()
    }
}
impl<T> Deref for Shared<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.0.values
    }
}
impl<T: Pod + Send + Sync + 'static> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        self.shares_storage(other)
            || (self.0.fingerprint == other.0.fingerprint
                && bytemuck::cast_slice::<T, u8>(&self.0.values)
                    == bytemuck::cast_slice::<T, u8>(&other.0.values))
    }
}
impl<T: Pod + Send + Sync + 'static> Eq for Shared<T> {}
impl<T> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shared")
            .field("length", &self.0.values.len())
            .field("fingerprint", &self.0.fingerprint)
            .finish()
    }
}

#[derive(Clone)]
enum Part {
    Payload(Arc<dyn Payload>),
    Joined(Key),
}
impl PartialEq for Part {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Payload(a), Self::Payload(b)) => {
                Arc::ptr_eq(a, b) || (a.fingerprint() == b.fingerprint() && a.bytes() == b.bytes())
            }
            (Self::Joined(a), Self::Joined(b)) => a == b,
            _ => false,
        }
    }
}

/// Fingerprints reject changed parameters quickly. Matching fingerprints still
/// require shared ownership or exact byte equality, preserving collision safety.
#[derive(Clone, Default)]
pub(crate) struct Key {
    fingerprint: u64,
    parts: Vec<Part>,
}
impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint && self.parts == other.parts
    }
}
impl Eq for Key {}
impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Key")
            .field("fingerprint", &self.fingerprint)
            .field("parts", &self.parts.len())
            .finish()
    }
}
impl Key {
    /// Includes the key's owned part storage and shared numerical payloads.
    /// Inline key structs and allocator overhead belong to their containing owner.
    pub fn account(&self, retained: &mut RetainedBytes) {
        retained.bytes += self.parts.capacity() * std::mem::size_of::<Part>();
        for part in &self.parts {
            match part {
                Part::Payload(payload) => retained.payload(payload.clone()),
                Part::Joined(key) => key.account(retained),
            }
        }
    }
    fn payload(payload: Arc<dyn Payload>) -> Self {
        Self {
            fingerprint: combined(0, payload.fingerprint(), 0),
            parts: vec![Part::Payload(payload)],
        }
    }
    pub fn join(mut self, other: Self) -> Self {
        self.fingerprint = combined(self.fingerprint, other.fingerprint, 1);
        self.parts.push(Part::Joined(other));
        self
    }
    pub fn pod<T: Pod>(value: &T) -> Self {
        Self::default().with(value)
    }
    pub fn with<T: Pod>(self, value: &T) -> Self {
        self.slice(std::slice::from_ref(value))
    }
    pub fn slice<T: Pod>(mut self, values: &[T]) -> Self {
        let payload: Shared<u8> = bytemuck::cast_slice::<T, u8>(values).to_vec().into();
        self.fingerprint = combined(self.fingerprint, payload.0.fingerprint, 0);
        self.parts.push(Part::Payload(payload.0));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_keys_preserve_boundaries_and_exact_float_bits() {
        assert_ne!(Key::pod(&0_f32), Key::pod(&-0_f32));
        assert_ne!(
            Key::default().slice(&[1_u8, 2]),
            Key::pod(&1_u8).with(&2_u8)
        );
        assert_ne!(Key::default(), Key::default().join(Key::default()));
        assert_ne!(
            Key::pod(&1_u8).join(Key::pod(&2_u8)),
            Key::pod(&1_u8).with(&2_u8)
        );
        assert_eq!(
            Key::pod(&f32::from_bits(0x7fc0_0001)),
            Key::pod(&f32::from_bits(0x7fc0_0001))
        );
        assert_ne!(
            Key::pod(&f32::from_bits(0x7fc0_0001)),
            Key::pod(&f32::from_bits(0x7fc0_0002))
        );
    }
    #[test]
    fn fingerprint_matches_cannot_hide_different_payloads() {
        let a = Key::pod(&1_u32);
        let mut b = Key::pod(&2_u32);
        b.fingerprint = a.fingerprint;
        assert_ne!(a, b);
        let a = Shared::<u32>::from(vec![1, 2]);
        let mut b = Shared::<u32>::from(vec![1, 3]);
        Arc::get_mut(&mut b.0).unwrap().fingerprint = a.0.fingerprint;
        assert_ne!(a, b);
        assert_ne!(a.key(), b.key());
    }
    #[test]
    fn shared_payload_keys_retain_one_immutable_allocation() {
        let values: Shared<u32> = vec![1, 2, 3].into();
        let a = values.key();
        let b = values.key();
        assert_eq!(a, b);
        assert_eq!(values.retained_bytes(), 12);
        assert_eq!(Arc::strong_count(&values.0), 3);
        let recreated: Shared<u32> = vec![1, 2, 3].into();
        assert_eq!(a, recreated.key());
        assert_eq!(a, Key::default().slice(&[1_u32, 2, 3]));
    }

    #[test]
    fn retained_inventory_counts_shared_payloads_once_and_keeps_owners_alive() {
        let values: Shared<u32> = vec![1, 2, 3].into();
        let key = values.key();
        let mut retained = RetainedBytes::default();
        values.account(&mut retained);
        key.account(&mut retained);
        assert_eq!(
            retained.bytes(),
            values.retained_bytes() + key.parts.capacity() * std::mem::size_of::<Part>()
        );
        assert_eq!(Arc::strong_count(&values.0), 3);
        let recreated: Shared<u32> = vec![1, 2, 3].into();
        recreated.account(&mut retained);
        assert_eq!(
            retained.bytes(),
            2 * values.retained_bytes() + key.parts.capacity() * std::mem::size_of::<Part>()
        );
        let weak = Arc::downgrade(&recreated.0);
        drop(recreated);
        assert!(weak.upgrade().is_some());
        drop(retained);
        assert!(weak.upgrade().is_none());
    }
}
