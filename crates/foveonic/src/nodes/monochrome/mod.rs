//! Mixes original layers and runs dedicated monochrome filtering, detail, and gain on GPU.
//!
//! Adapted with owner permission from the reference implementation's monochrome modules.

mod apply;
mod parameters;
pub(crate) use apply::apply;
pub(crate) use parameters::{Prepared, Stage};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Controls {
    /// Actual nonnegative sensor-layer proportions, summing to one.
    pub mix: [f32; 3],
    /// Five reference mono noise presets. Zero skips histogram filtering.
    pub noise: u32,
    /// Five reference column-correction presets.
    pub banding: u32,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            mix: [1. / 3.; 3],
            noise: 2,
            banding: 2,
        }
    }
}
impl Controls {
    pub fn validate(self) -> crate::source::Result<()> {
        if self.mix.iter().any(|v| !v.is_finite() || *v < 0.)
            || (self.mix.iter().sum::<f32>() - 1.).abs() > 0.0001
            || self.noise > 4
            || self.banding > 4
        {
            return Err(
                "mono mix must be nonnegative and sum to one, noise/banding must be in 0..4".into(),
            );
        }
        Ok(())
    }
    pub fn normalized_mix(mix: [f32; 3]) -> crate::source::Result<[f32; 3]> {
        let sum = mix.iter().map(|v| f64::from(*v)).sum::<f64>();
        if mix.iter().any(|v| !v.is_finite() || *v < 0.) || sum <= 0. {
            return Err("mono mix needs nonnegative finite amounts and a positive total".into());
        }
        Ok(mix.map(|v| (f64::from(v) / sum) as f32))
    }
}

#[cfg(test)]
mod tests;
