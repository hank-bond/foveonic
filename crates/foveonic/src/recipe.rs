//! Encodes exact effective operation parameters for graph versioning without hashing photographic samples.

/// Exact bitwise keys avoid hash collisions and preserve signed-zero distinctions.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct Key(Vec<u8>);
impl Key {
    pub fn join(self, other: Self) -> Self {
        self.slice(&other.0)
    }
    pub fn pod<T: bytemuck::Pod>(value: &T) -> Self {
        Self::default().with(value)
    }
    pub fn with<T: bytemuck::Pod>(self, value: &T) -> Self {
        self.slice(std::slice::from_ref(value))
    }
    pub fn slice<T: bytemuck::Pod>(mut self, values: &[T]) -> Self {
        let bytes = bytemuck::cast_slice::<T, u8>(values);
        self.0
            .extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        self.0.extend_from_slice(bytes);
        self
    }
}
