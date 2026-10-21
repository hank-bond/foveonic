//! Owns color noise preparation, reconstruction, and camera color conditioning.

pub(crate) mod color_dq;
pub(crate) mod color_mode;
pub mod noise;
pub(crate) mod pattern;
pub(crate) mod reconstruction;

pub(crate) mod chroma;
pub(crate) mod highlight_stream;

pub(crate) mod log_reconstruction;

pub(crate) mod range_reconstruction;

pub(crate) mod weighted_white_balance;

pub(crate) mod code_gain;
