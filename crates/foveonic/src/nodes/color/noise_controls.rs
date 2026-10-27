//! Defines independent Merrill cleanup and texture controls, retaining saved Quattro Smooth/Crisp values.

use crate::source::Result;
use serde::{Deserialize, Serialize};

/// Five cleanup choices and independent texture restoration.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "StoredControls")]
pub struct Controls {
    /// Merrill cleanup and automatic ColorDQ. Inactive values stay saved.
    pub noise_enabled: bool,
    /// Merrill Fine/Coarse adjustments. Disabled detail uses neutral amounts.
    pub detail_enabled: bool,
    pub chroma: u32,
    pub luminance: u32,
    pub banding: u32,
    /// Quattro Smooth/Crisp (0..1). Merrill uses `texture` instead.
    pub detail: f32,
    pub texture: TextureControls,
}

/// Log-luminance band amounts and sizes. One restores neutral detail.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "StoredTextureControls")]
pub struct TextureControls {
    pub fine: f32,
    pub coarse: f32,
    /// Dilation of the Fine/Coarse boundary in native sensor pixels.
    pub fine_size: f32,
    /// Additional dilation separating Coarse from broad shading.
    pub coarse_size: f32,
}
impl TextureControls {
    pub const AMOUNT_RANGE: std::ops::RangeInclusive<f32> = 0. ..=2.;
    // At 2× the four binomial passes need 60 pixels of the 64-pixel tile context.
    pub const SIZE_RANGE: std::ops::RangeInclusive<f32> = 0.5..=2.;

    pub(crate) fn is_neutral(self) -> bool {
        self.fine == 1. && self.coarse == 1.
    }
}

/// Effective Merrill effects. Saved enable choices and inactive sizes remain
/// in Controls, while graph construction and GPU preparation use this result.
#[derive(Clone, Copy)]
pub(crate) struct MerrillEffects {
    pub noise: bool,
    pub texture: Option<TextureControls>,
    pub strength: [f32; 4],
}
impl MerrillEffects {
    pub fn needs_tiles(self) -> bool {
        self.noise || self.texture.is_some()
    }
    pub fn changes_layers(self) -> bool {
        self.texture.is_some() || self.strength[2] != 0. || self.strength[3] != 0.
    }
}
impl Default for TextureControls {
    fn default() -> Self {
        Self {
            fine: 1.,
            coarse: 1.,
            fine_size: 1.,
            coarse_size: 1.,
        }
    }
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            noise_enabled: true,
            detail_enabled: true,
            chroma: 2,
            luminance: 2,
            banding: 2,
            detail: 1.,
            texture: TextureControls::default(),
        }
    }
}
impl Controls {
    /// Resolve disabled and neutral controls once for processing and inspection.
    pub(crate) fn merrill_effects(self) -> MerrillEffects {
        const MAX_NOISE_CHOICE: f32 = 4.;
        let texture = (self.detail_enabled && !self.texture.is_neutral()).then_some(self.texture);
        let detail = texture.unwrap_or_default();
        let chroma = if self.noise_enabled {
            self.chroma as f32 / MAX_NOISE_CHOICE
        } else {
            0.
        };
        let luminance = if self.noise_enabled {
            self.luminance as f32 / MAX_NOISE_CHOICE
        } else {
            0.
        };
        MerrillEffects {
            noise: self.noise_enabled,
            texture,
            // Ease chroma cleanup without boosting brightness texture.
            strength: [
                detail.fine,
                detail.coarse,
                luminance,
                1. - (1. - chroma).powi(2),
            ],
        }
    }

    /// Merrill's camera-derived base stays fixed as residual cleanup is edited.
    pub(crate) fn base_controls(self) -> Self {
        Self {
            banding: self.banding,
            ..Self::default()
        }
    }
    pub fn validate(self) -> Result<()> {
        if [self.texture.fine_size, self.texture.coarse_size]
            .iter()
            .any(|v| !v.is_finite() || !TextureControls::SIZE_RANGE.contains(v))
        {
            return Err("fine and coarse texture sizes must be finite and in 0.5..2".into());
        }
        if [self.texture.fine, self.texture.coarse]
            .iter()
            .any(|v| !v.is_finite() || !TextureControls::AMOUNT_RANGE.contains(v))
        {
            return Err("fine and coarse detail must be finite and in 0..2".into());
        }
        if self.chroma > 4
            || self.luminance > 4
            || self.banding > 4
            || !self.detail.is_finite()
            || self.detail < 0.
        {
            return Err(
                "noise choices must be in 0..4 and Smooth/Crisp must be finite and nonnegative"
                    .into(),
            );
        }
        Ok(())
    }
}

// Read retired fields only at the serialization boundary. They cannot select
// the former renderer and are not written back. Old Merrill residual amounts
// have no equivalent in the new bands, so missing texture starts neutral.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct StoredControls {
    noise_enabled: bool,
    detail_enabled: bool,
    chroma: u32,
    luminance: u32,
    banding: u32,
    detail: f32,
    texture: TextureControls,
    #[serde(rename = "overrides")]
    _retired_overrides: Option<RetiredOverrides>,
}
impl Default for StoredControls {
    fn default() -> Self {
        let c = Controls::default();
        Self {
            noise_enabled: c.noise_enabled,
            detail_enabled: c.detail_enabled,
            chroma: c.chroma,
            luminance: c.luminance,
            banding: c.banding,
            detail: c.detail,
            texture: c.texture,
            _retired_overrides: None,
        }
    }
}
impl From<StoredControls> for Controls {
    fn from(c: StoredControls) -> Self {
        Self {
            noise_enabled: c.noise_enabled,
            detail_enabled: c.detail_enabled,
            chroma: c.chroma,
            luminance: c.luminance,
            banding: c.banding,
            detail: c.detail,
            texture: c.texture,
        }
    }
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct StoredTextureControls {
    fine: f32,
    coarse: f32,
    fine_size: f32,
    coarse_size: f32,
    #[serde(rename = "enabled")]
    _retired_enabled: Option<bool>,
}
impl Default for StoredTextureControls {
    fn default() -> Self {
        let t = TextureControls::default();
        Self {
            fine: t.fine,
            coarse: t.coarse,
            fine_size: t.fine_size,
            coarse_size: t.coarse_size,
            _retired_enabled: None,
        }
    }
}
impl From<StoredTextureControls> for TextureControls {
    fn from(t: StoredTextureControls) -> Self {
        Self {
            fine: t.fine,
            coarse: t.coarse,
            fine_size: t.fine_size,
            coarse_size: t.coarse_size,
        }
    }
}

// The retired schema remains strict so malformed or newer sidecars are never
// silently accepted and overwritten. These values do not reach processing.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetiredOverrides {
    #[serde(rename = "base-blend")]
    _base_blend: Option<f32>,
    #[serde(rename = "edge-cutoff")]
    _edge_cutoff: Option<f32>,
    #[serde(rename = "mean-decay")]
    _mean_decay: Option<f32>,
    #[serde(rename = "edge-scale")]
    _edge_scale: Option<f32>,
    #[serde(rename = "luma-noise-scale")]
    _luma_noise_scale: Option<f32>,
    #[serde(rename = "brightness-mix")]
    _brightness_mix: Option<f32>,
    #[serde(rename = "layer-mix")]
    _layer_mix: Option<f32>,
}
