//! Tracks independent per-photo Exposure expectations for native journeys.

use serde::Serialize;

/// Prior feedback may remain while an edit is pending. Current feedback must not regress.
#[derive(Default)]
pub(crate) struct ImageFeedback {
    desired: Option<f32>,
    current_seen: bool,
}
impl ImageFeedback {
    pub(crate) fn observe(&mut self, desired: f32, shown: Option<f32>) -> Result<(), String> {
        if self.desired != Some(desired) {
            self.desired = Some(desired);
            self.current_seen = false;
        }
        match shown {
            Some(value) if value == desired => self.current_seen = true,
            Some(value) if self.current_seen => {
                return Err(format!(
                    "current preview regressed: expected {desired}, observed {value}"
                ))
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ExposureJourney {
    selection: Vec<usize>,
    exposures: Vec<f32>,
}

impl ExposureJourney {
    pub(crate) fn new(exposures: Vec<f32>) -> Self {
        Self {
            selection: (0..exposures.len()).collect(),
            exposures,
        }
    }

    pub(crate) fn select(&mut self, indices: Vec<usize>) {
        assert!(indices.iter().all(|&index| index < self.exposures.len()));
        assert!(indices.windows(2).all(|pair| pair[0] < pair[1]));
        self.selection = indices;
    }

    pub(crate) fn adjust_exposure(&mut self, delta: f32) {
        for &index in &self.selection {
            self.exposures[index] = (self.exposures[index] + delta).clamp(-4., 4.);
        }
    }

    pub(crate) fn exposures(&self) -> &[f32] {
        &self.exposures
    }

    pub(crate) fn check_selection(&self, observed: &[usize]) -> Result<(), String> {
        if observed == self.selection {
            Ok(())
        } else {
            Err(format!(
                "selection: expected {:?}, observed {:?}",
                self.selection, observed
            ))
        }
    }

    pub(crate) fn check_exposures(&self, observed: &[f32]) -> Result<(), String> {
        if observed.len() != self.exposures.len() {
            return Err(format!(
                "exposure count: expected {}, observed {}",
                self.exposures.len(),
                observed.len()
            ));
        }
        for (index, (&expected, &actual)) in self.exposures.iter().zip(observed).enumerate() {
            if actual != expected {
                return Err(format!(
                    "photo {index} exposure: expected {expected:?}, observed {actual:?}"
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ExposureJourney, ImageFeedback};

    #[test]
    fn feedback_allows_pending_images_but_rejects_regression_after_current() {
        let mut feedback = ImageFeedback::default();
        assert!(feedback.observe(0.5, Some(0.)).is_ok());
        assert!(feedback.observe(0.5, Some(0.5)).is_ok());
        assert!(feedback.observe(0.5, None).is_ok());
        assert!(feedback.observe(0.5, Some(0.)).is_err());
        assert!(feedback.observe(1., Some(0.5)).is_ok());
        assert!(feedback.observe(1., Some(1.)).is_ok());
    }

    #[test]
    fn subset_edits_survive_removal_and_readdition() {
        let mut journey = ExposureJourney::new(vec![0., 1., -1.]);
        assert!(journey.check_selection(&[0, 1, 2]).is_ok());
        journey.select(vec![0, 2]);
        journey.adjust_exposure(0.5);
        assert_eq!(journey.exposures(), &[0.5, 1., -0.5]);
        journey.select(vec![1, 2]);
        journey.adjust_exposure(-0.25);
        journey.select(vec![0, 1, 2]);
        assert_eq!(journey.exposures(), &[0.5, 0.75, -0.75]);
    }

    #[test]
    fn successive_edits_clamp_each_photo_independently() {
        let mut journey = ExposureJourney::new(vec![3.75, -3.75, 0.]);
        journey.adjust_exposure(0.5);
        journey.adjust_exposure(-1.);
        journey.select(vec![1, 2]);
        journey.adjust_exposure(-4.);
        assert_eq!(journey.exposures(), &[3., -4., -4.]);
    }

    #[test]
    fn checks_reject_wrong_selection_and_any_wrong_photo() {
        let mut journey = ExposureJourney::new(vec![0., 1., 2.]);
        journey.select(vec![0, 2]);
        journey.adjust_exposure(0.5);
        assert!(journey.check_selection(&[0, 2]).is_ok());
        assert!(journey.check_selection(&[0, 1, 2]).is_err());
        assert!(journey.check_exposures(&[0.5, 1., 2.5]).is_ok());
        assert!(journey.check_exposures(&[0.5, 1.5, 2.5]).is_err());
        assert!(journey.check_exposures(&[0.5, 1.]).is_err());
        assert!(journey.check_exposures(&[0.5, 1., f32::NAN]).is_err());
    }
}
