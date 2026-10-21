//! Selects Classic stages without preparing later numerical parameters.

use super::{prepare, reference::*};
use crate::{
    render::Settings,
    source::{Metadata, Result},
};

/// Stage selection is shared by graph construction and its inspection catalogue.
/// Keep parameter validation at each stage so early inspection stays independent.
pub(super) struct Route<'a> {
    metadata: &'a Metadata,
    pub monochrome: bool,
    pub repair: bool,
    pub despeckle: bool,
    filtering: bool,
    highlights: bool,
}

impl<'a> Route<'a> {
    pub fn new(metadata: &'a Metadata, settings: &Settings) -> Self {
        Self {
            metadata,
            monochrome: settings.is_monochrome(),
            repair: settings.repair.is_some(),
            despeckle: settings.despeckle,
            filtering: settings.reconstruction.is_some(),
            highlights: settings.highlight_restoration,
        }
    }

    fn scalar(&self, name: &str, fallback: f64) -> Result<f64> {
        if self.metadata.matrix(name).is_some() {
            self.metadata.scalar(name)
        } else {
            Ok(fallback)
        }
    }

    pub fn red_detail(&self) -> Result<bool> {
        Ok(!self.monochrome && (self.scalar("RedSharpen", 0.)? as u32).wrapping_mul(2) != 0)
    }

    pub fn highlights(&self) -> Result<bool> {
        Ok(!self.monochrome
            && self.highlights
            && self.scalar("SkipStepFlags", 0.)? as u32 & (1 << 11) == 0)
    }

    pub fn small_chroma(&self) -> Result<bool> {
        Ok(self.filtering && self.scalar("ChromaBlur", 0.)? != 0.)
    }

    pub fn large_chroma(&self) -> Result<bool> {
        Ok(!self.monochrome && self.small_chroma()?)
    }

    pub fn logarithmic(&self) -> Result<bool> {
        Ok(!self.monochrome && self.filtering && self.scalar("FIR_DisableStep", 0.)? == 0.)
    }

    pub fn restore_gain(&self) -> Result<bool> {
        Ok(!self.monochrome && !self.filtering && self.scalar("FIR_DisableStep", 0.)? == 0.)
    }

    pub fn recursive_chroma(&self) -> Result<bool> {
        Ok(!self.monochrome
            && self.filtering
            && self.scalar("ChromaBlur", 0.)? >= RECURSIVE_CHROMA_THRESHOLD)
    }

    /// ISO is required only on reaching reconstruction, after sensor inspection.
    pub fn high_iso(&self) -> Result<bool> {
        Ok(prepare::capture_iso(self.metadata)? >= RANGE_RECONSTRUCTION_ISO)
    }

    pub fn white_balance<'s>(&self, settings: &'s Settings) -> &'s str {
        if self.monochrome {
            "Monochrome"
        } else {
            &settings.white_balance
        }
    }

    /// Color DNG defers its selected mode. Mono has already baked the mixture.
    pub fn defer_color_mode(&self, dng: bool) -> bool {
        dng && !self.monochrome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        render::Rendering,
        source::{Matrix, MatrixEncoding},
    };

    fn metadata(entries: &[(&str, &[f64])]) -> Metadata {
        Metadata {
            processing_version: None,
            width: 1,
            height: 1,
            rotation: 0,
            properties: [("ISO".into(), "100".into())].into(),
            lists: Default::default(),
            matrices: entries
                .iter()
                .map(|(name, values)| {
                    (
                        (*name).into(),
                        Matrix {
                            encoding: MatrixEncoding::Float32,
                            representation: "native",
                            dimensions: vec![values.len()],
                            values: values.to_vec(),
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn malformed_selectors_fail_only_when_their_stage_is_reached() {
        let metadata = metadata(&[("ChromaBlur", &[1., 2.]), ("FIR_DisableStep", &[0., 0.])]);
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        let route = Route::new(&metadata, &settings);
        // Earlier sensor stages remain selectable despite invalid later controls.
        assert!(!route.red_detail().unwrap());
        assert!(!route.high_iso().unwrap());
        assert!(route.small_chroma().is_err());
        assert!(route.large_chroma().is_err());
        assert!(route.logarithmic().is_err());
        assert!(route.recursive_chroma().is_err());

        settings.reconstruction = None;
        let route = Route::new(&metadata, &settings);
        assert!(!route.small_chroma().unwrap());
        assert!(!route.logarithmic().unwrap());
        // Filtering bypass still requires the gain selector, so errors propagate.
        assert!(route.restore_gain().is_err());

        settings.rendering = Rendering::Monochrome;
        let route = Route::new(&metadata, &settings);
        assert!(!route.large_chroma().unwrap());
        assert!(!route.logarithmic().unwrap());
        assert!(!route.restore_gain().unwrap());
        assert!(!route.recursive_chroma().unwrap());
    }

    #[test]
    fn red_detail_selection_matches_integer_strength_conversion() {
        let settings = Settings::new("Sunlight", "Standard");
        for (strength, enabled) in [
            (-1., false),
            (0., false),
            (0.5, false),
            (1., true),
            (2_147_483_648., false),
        ] {
            let metadata = metadata(&[("RedSharpen", &[strength])]);
            assert_eq!(
                Route::new(&metadata, &settings).red_detail().unwrap(),
                enabled
            );
        }
        let malformed = metadata(&[("RedSharpen", &[1., 2.])]);
        assert!(Route::new(&malformed, &settings).red_detail().is_err());
        let mut monochrome = settings;
        monochrome.rendering = Rendering::Monochrome;
        assert!(!Route::new(&malformed, &monochrome).red_detail().unwrap());
    }
}
