//! Loads the shipped cap calibration's color basis and ISO-dependent native row priors.

use crate::source::Result;
use std::sync::OnceLock;

pub(crate) const ROW_PRIOR_SIGMA: f64 = 1.25;
pub(crate) const GAUSSIAN_SIGMAS: [usize; 4] = [2, 8, 32, 128];
pub(crate) const COLUMN_REFERENCE_LENGTH: usize = 4704;
pub(crate) const MODEL_COUNT: usize = 5;
pub(crate) const SCALES: usize = GAUSSIAN_SIGMAS.len() + 1;
pub(crate) const WEIGHTS_PER_MODEL: usize = SCALES * 3;
pub(crate) const GAUSSIAN_WEIGHT_COUNT: usize = 8 * (2 + 8 + 32 + 128) + 4;
pub(crate) const REFERENCE_OFFSET: usize = MODEL_COUNT * WEIGHTS_PER_MODEL + GAUSSIAN_WEIGHT_COUNT;
pub(crate) const MODEL_FLOATS: usize = REFERENCE_OFFSET + 2 * COLUMN_REFERENCE_LENGTH;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RowPrior {
    pub(crate) iso: u32,
    pub(crate) inverse_covariance: [f32; 9],
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Model {
    pub(crate) shield_color_axis: [f32; 3],
    pub(crate) row_priors: Vec<RowPrior>,
    #[serde(default)]
    pub(crate) shield_tables: Vec<f32>,
}

impl Model {
    fn parse(bytes: &[u8]) -> Result<Self> {
        let mut model: Self = serde_json::from_slice(bytes)?;
        model.shield_tables = include_bytes!("shield_noise_model.bin")
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| f32::from_le_bytes(*v))
            .collect();
        model.validate()?;
        Ok(model)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let model = self;
        if model.shield_tables.len() != MODEL_FLOATS
            || model.shield_tables.iter().any(|v| !v.is_finite())
            || model.shield_color_axis.iter().any(|v| !v.is_finite())
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
            return Err("invalid Merrill noise profile data".into());
        }
        Ok(())
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
