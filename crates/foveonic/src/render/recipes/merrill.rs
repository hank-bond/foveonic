//! Selects Merrill photographic and editable DNG recipes over their shared technical stages.

mod dng_base;
mod monochrome;
mod photographic;
mod technical;

pub(super) use dng_base::build as build_dng;
pub(super) use photographic::build;
#[cfg(test)]
pub(in crate::render) use photographic::saturation;
