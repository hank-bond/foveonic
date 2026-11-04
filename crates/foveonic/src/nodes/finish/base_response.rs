//! Defines regional base-response choices and shared per-profile application caps.

use crate::source::Result;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseResponse {
    Natural,
    Strong,
    #[default]
    Camera,
}
impl BaseResponse {
    pub fn is_adaptive(self) -> bool {
        self != Self::Camera
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Camera => "Disabled",
            Self::Natural => "Natural",
            Self::Strong => "Strong",
        }
    }
    pub(crate) fn strength(self) -> f32 {
        f32::from(self == Self::Strong)
    }
}

/// Maximum normalized HDR amounts. Blacks bounds a local near-black contribution.
/// Shared app tuning is excluded from per-photo editing serialization.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub blacks: f32,
    pub shadows: f32,
    pub highlights: f32,
    pub whites: f32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            blacks: 0.075,
            shadows: 0.25,
            highlights: 0.35,
            whites: 1.,
        }
    }
}
impl Limits {
    pub fn validate(self) -> Result<()> {
        if [self.blacks, self.shadows, self.highlights, self.whites]
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Err("adaptive tone limits must be in 0..1".into());
        }
        Ok(())
    }
    pub(crate) fn values(self) -> [f32; 4] {
        [self.shadows, self.highlights, self.whites, self.blacks]
    }
}

/// Application-wide profile tuning. Photo recipes retain only the selected profile.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tuning {
    pub natural: Limits,
    pub strong: Limits,
}
impl Default for Tuning {
    fn default() -> Self {
        Self {
            natural: Limits::default(),
            strong: Limits {
                blacks: 0.115,
                shadows: 0.55,
                highlights: 0.65,
                whites: 1.,
            },
        }
    }
}
impl Tuning {
    pub fn validate(self) -> Result<()> {
        self.natural.validate()?;
        self.strong.validate()
    }
    pub fn for_response(self, response: BaseResponse) -> Limits {
        if response == BaseResponse::Strong {
            self.strong
        } else {
            self.natural
        }
    }
}
