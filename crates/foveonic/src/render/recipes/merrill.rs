//! Selects Merrill photographic and editable DNG recipes over their shared technical stages.

mod appearance;
mod dng_base;
mod monochrome;
mod photographic;
mod preview;
mod region;
mod technical;

pub(super) use dng_base::build as build_dng;
pub(super) use photographic::build;
pub(in crate::render) use preview::available as fast_available;

pub(super) use appearance::prepare as prepare_dng_appearance;
