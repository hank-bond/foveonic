//! Loads the shipped cap calibration's color basis and ISO-dependent native row priors.

use crate::source::Result;
use std::sync::OnceLock;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RowPrior {
    iso: u32,
    inverse_covariance: [f32; 9],
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Model {
    shield_color_axis: [f32; 3],
    row_priors: Vec<RowPrior>,
}

impl Model {
    fn parse(bytes: &[u8]) -> Result<Self> {
        let model: Self = serde_json::from_slice(bytes)?;
        if model.shield_color_axis.iter().any(|v| !v.is_finite())
            || model.shield_color_axis.iter().all(|v| *v == 0.)
            || model.row_priors.is_empty()
            || model.row_priors.iter().any(|prior| {
                prior.iso == 0 || prior.inverse_covariance.iter().any(|v| !v.is_finite())
            })
            || model
                .row_priors
                .windows(2)
                .any(|pair| pair[0].iso >= pair[1].iso)
        {
            return Err("invalid Merrill noise model metadata".into());
        }
        Ok(model)
    }

    pub(crate) fn shield_color_axis(&self) -> [f32; 3] {
        self.shield_color_axis
    }

    /// Selects the nearest measured ISO. Ties keep the lower ISO, and values
    /// outside the calibrated range use its closest endpoint.
    pub(crate) fn row_inverse_covariance(&self, iso: u32) -> [f32; 9] {
        self.row_priors
            .iter()
            .min_by_key(|prior| prior.iso.abs_diff(iso))
            .expect("validated nonempty row priors")
            .inverse_covariance
    }
}

pub(crate) fn embedded() -> Result<&'static Model> {
    static MODEL: OnceLock<std::result::Result<Model, String>> = OnceLock::new();
    match MODEL.get_or_init(|| {
        Model::parse(include_bytes!("merrill_noise_model.json")).map_err(|error| error.to_string())
    }) {
        Ok(model) => Ok(model),
        Err(error) => Err(error.as_str().into()),
    }
}

#[cfg(test)]
#[path = "noise_model_test.rs"]
mod tests;
