//! Defines Merrill speckle detection thresholds and compact-neighborhood support.

use crate::source::Result;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MerrillSpeckleMethod {
    #[default]
    Reference,
    Compact,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MerrillSpeckleControls {
    pub method: MerrillSpeckleMethod,
    /// Multiplier for camera-derived thresholds. Lower values detect more points.
    pub threshold: f32,
    /// Maximum native neighborhood diameter. Compact detection checks each radius.
    pub size: u32,
}
impl Default for MerrillSpeckleControls {
    fn default() -> Self {
        Self {
            method: MerrillSpeckleMethod::Reference,
            threshold: 1.,
            size: 3,
        }
    }
}
impl MerrillSpeckleControls {
    pub const THRESHOLD_RANGE: std::ops::RangeInclusive<f32> = 0.05..=4.;
    pub const SIZE_RANGE: std::ops::RangeInclusive<u32> = 3..=7;
    pub(crate) fn validate(self) -> Result<()> {
        if !self.threshold.is_finite() || !Self::THRESHOLD_RANGE.contains(&self.threshold) {
            return Err("speckle threshold must be between 0.05 and 4".into());
        }
        if !Self::SIZE_RANGE.contains(&self.size) || self.size.is_multiple_of(2) {
            return Err("speckle size must be 3, 5 or 7 native pixels".into());
        }
        Ok(())
    }
}
