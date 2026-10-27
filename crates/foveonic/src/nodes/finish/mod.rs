//! Prepares tone and resampling with shaders for sharpening and final encoding.

pub(crate) mod code_response;
pub(crate) mod image_region;
pub(crate) mod mask_image;

pub(crate) mod resample;
pub(crate) mod resize;
pub(crate) mod signed_response;
pub(crate) mod tone;
pub(crate) mod tone_equalizer;

pub(crate) mod root_detail;

pub(crate) mod proportional_sharpen;

pub(crate) mod wavelet_contrast;

pub(crate) mod code_crop;

pub(crate) mod box_contrast;

pub(crate) mod color_sharpen;

pub(crate) mod code_channel_map;

pub(crate) mod photographic;

pub(crate) mod sensitivity;

pub(crate) mod linear_gain;
