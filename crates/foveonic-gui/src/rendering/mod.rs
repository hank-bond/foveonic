//! Plans viewport photos' images, reconciles render work against them and stores delivered images.

pub(crate) mod coordinator;
pub(crate) mod matching;
pub(crate) mod quality;
pub(crate) mod rendered;
pub(crate) mod request;
pub(crate) mod runtime;
pub(crate) mod smart;
mod source_loader;
pub(crate) mod view_plan;
pub(crate) mod viewport_plan;
pub(crate) mod views;
pub(crate) mod worker;
