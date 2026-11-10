//! Coordinates photo render requests, matching, refinement and delivery through the existing worker.

pub(crate) mod coordinator;
pub(crate) mod delivery;
pub(crate) mod matching;
pub(crate) mod request;
pub(crate) mod smart;
pub(crate) mod worker;
