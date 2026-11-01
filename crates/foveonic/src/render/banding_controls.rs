//! Stores independent native-pixel widths and strengths for Merrill directional band cleanup.

use crate::source::Result;
use std::ops::RangeInclusive;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MerrillBandingMethod {
    #[default]
    Reference,
    Directional,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BandingBand {
    /// Odd neighborhood width in native sensor pixels.
    pub size: u32,
    pub strength: f32,
}

impl BandingBand {
    pub const SIZE_RANGE: RangeInclusive<u32> = 3..=119;
    pub const STRENGTH_RANGE: RangeInclusive<f32> = 0. ..=3.;
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BandingAxisControls {
    /// Fine and Coarse, in increasing size order.
    pub bands: [BandingBand; 2],
}

impl Default for BandingAxisControls {
    fn default() -> Self {
        Self {
            bands: Self::DEFAULT_BANDS,
        }
    }
}

impl BandingAxisControls {
    pub const LABELS: [&'static str; 2] = ["Fine", "Coarse"];
    pub const DEFAULT_BANDS: [BandingBand; 2] = [
        BandingBand {
            size: 27,
            strength: 1.,
        },
        BandingBand {
            size: 119,
            strength: 0.,
        },
    ];

    /// Keep a size edit between the adjacent bands without changing their values.
    pub fn size_range(&self, index: usize) -> RangeInclusive<u32> {
        let lower = if index == 0 {
            *BandingBand::SIZE_RANGE.start()
        } else {
            self.bands[index - 1].size + 2
        };
        let upper = if index + 1 == self.bands.len() {
            *BandingBand::SIZE_RANGE.end()
        } else {
            self.bands[index + 1].size - 2
        };
        lower..=upper
    }

    fn validate(&self) -> Result<()> {
        for band in self.bands {
            if !BandingBand::SIZE_RANGE.contains(&band.size) || band.size.is_multiple_of(2) {
                return Err("banding sizes must be odd native pixel widths in 3..119".into());
            }
            if !band.strength.is_finite() || !BandingBand::STRENGTH_RANGE.contains(&band.strength) {
                return Err("banding strengths must be finite and in 0..3".into());
            }
        }
        if self
            .bands
            .windows(2)
            .any(|pair| pair[0].size >= pair[1].size)
        {
            return Err("banding sizes must increase from Fine to Coarse".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MerrillBandingControls {
    pub enabled: bool,
    pub method: MerrillBandingMethod,
    /// Native sensor rows. Display orientation changes only their user-facing label.
    pub rows: BandingAxisControls,
    pub columns: BandingAxisControls,
}

impl Default for MerrillBandingControls {
    fn default() -> Self {
        Self {
            enabled: true,
            method: MerrillBandingMethod::Reference,
            rows: Default::default(),
            columns: Default::default(),
        }
    }
}

impl MerrillBandingControls {
    pub fn validate(&self) -> Result<()> {
        self.rows.validate()?;
        self.columns.validate()
    }
}
