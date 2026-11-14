//! Separates photographic processing quality from the image's working resolution.

use super::Scale;

/// Quality uses complete working-grid inputs. Speed permits bounded intermediate reductions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProcessingQuality {
    #[default]
    Quality,
    Speed,
}

impl ProcessingQuality {
    /// The application guide cannot request finer data than the chosen image grid.
    pub fn guide_scale(self, working: Scale) -> Scale {
        match self {
            Self::Quality => working,
            Self::Speed => working.coarsest(Scale::new(2).expect("two is a power of two")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processing_policy_is_transient_and_source_exports_are_quality() {
        let settings = super::super::Settings {
            processing_quality: ProcessingQuality::Speed,
            working_scale: Scale::new(8).unwrap(),
            preview_region: Some([1, 2, 10, 12]),
            exposure: 0.75,
            ..Default::default()
        };
        assert!(serde_json::to_value(&settings)
            .unwrap()
            .get("processing_quality")
            .is_none());
        let export = settings.for_export();
        assert_eq!(export.processing_quality, ProcessingQuality::Quality);
        assert_eq!(export.working_scale, Scale::NATIVE);
        assert_eq!(export.preview_region, None);
        assert_eq!(export.exposure, settings.exposure);
        assert_eq!(settings.processing_quality, ProcessingQuality::Speed);
    }

    #[test]
    fn guide_budget_is_independent_of_region_and_bounded_by_working_resolution() {
        for divisor in [1, 2, 4, 8, 16] {
            let scale = Scale::new(divisor).unwrap();
            assert_eq!(ProcessingQuality::Quality.guide_scale(scale), scale);
            assert_eq!(
                ProcessingQuality::Speed.guide_scale(scale).divisor(),
                divisor.max(2)
            );
        }
    }
}
