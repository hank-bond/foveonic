//! Owns color noise preparation, reconstruction, and camera color conditioning.

pub(crate) mod sector_color;

pub(crate) mod color_dq;
pub(crate) mod color_mode;
pub mod noise;
pub(crate) mod pattern;
pub(crate) mod pyramid_resample;
pub(crate) mod raw_color_blend;
pub(crate) mod reconstruction;
pub(crate) mod rgb_prefilter;
pub(crate) mod two_scale_noise;
pub(crate) mod uv_repair;

pub(crate) mod chroma;
pub(crate) mod highlight_stream;

pub(crate) mod log_reconstruction;

pub(crate) mod range_reconstruction;

pub(crate) mod weighted_white_balance;

pub(crate) mod code_gain;

pub(crate) mod blue_guided_reconstruct;

pub(crate) mod packed_lab;

pub(crate) mod residual_finish;

pub(crate) mod spatial_color_gain;

pub(crate) mod byte_mask_double;
pub(crate) mod cell_gain;

pub(crate) mod histogram_bilateral;
pub(crate) mod purple_fringe;

pub(crate) mod chromatic_ratio;

pub(crate) mod neighborhood_decorrelate;

pub(crate) mod channel_noise;
