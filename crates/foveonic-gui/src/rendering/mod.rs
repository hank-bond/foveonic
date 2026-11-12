//! Coordinates photo render requests, matching, refinement and delivery through the existing worker.

pub(crate) mod coordinator;
pub(crate) mod delivery;
pub(crate) mod matching;
pub(crate) mod request;
pub(crate) mod runtime;
pub(crate) mod smart;
mod source_loader;
pub(crate) mod worker;
