//! Selects a preview's processing level from workspace policy and physical display sampling.

use crate::{preferences::PreviewMode, App};
use foveonic::render::Scale;
use std::path::Path;

/// Provisional visual-quality limit, measured per dimension rather than by area.
/// The display filter and source photograph affect how visible this enlargement is.
const MAX_UPSCALE: f32 = 1.5;

pub(crate) fn resolve(mode: PreviewMode, pixels_per_native_pixel: Option<f32>) -> PreviewMode {
    if mode != PreviewMode::Smart {
        return mode;
    }
    // A thumbnail or a completed image supplies geometry on the next UI pass.
    let Some(density) = pixels_per_native_pixel.filter(|v| v.is_finite() && *v > 0.) else {
        return PreviewMode::Fast;
    };
    [PreviewMode::Thumb, PreviewMode::Draft, PreviewMode::Fast]
        .into_iter()
        .find(|quality| density * quality.working_scale().divisor() as f32 <= MAX_UPSCALE)
        .unwrap_or(PreviewMode::Full)
}

/// Drawn image bounds exclude letterboxing and captions, and include zoom.
/// Extent describes the native image area, including its saved composition.
pub(crate) fn display_density(size: [f32; 2], extent: [u32; 2], pixels_per_point: f32) -> f32 {
    (size[0] / extent[0].max(1) as f32).max(size[1] / extent[1].max(1) as f32) * pixels_per_point
}

pub(crate) fn accepts_scale(policy: PreviewMode, requested: Scale, retained: Scale) -> bool {
    if policy == PreviewMode::Smart {
        retained.divisor() <= requested.divisor()
    } else {
        retained == requested
    }
}

impl App {
    pub(crate) fn preview_quality_for(&self, path: &Path, mode: PreviewMode) -> PreviewMode {
        if mode == PreviewMode::Smart {
            self.smart_qualities
                .get(path)
                .copied()
                .unwrap_or(PreviewMode::Fast)
        } else {
            resolve(mode, None)
        }
    }

    /// Keep only the effective quality, so tiny layout changes within a level
    /// do not become photographic requests. Viewing selection does not enter policy.
    pub(crate) fn observe_preview_density(&mut self, path: &Path, density: f32) {
        let quality = resolve(PreviewMode::Smart, Some(density));
        let previous = self
            .smart_qualities
            .insert(path.to_owned(), quality)
            .unwrap_or(PreviewMode::Fast);
        self.preview_demand_changed |= self.preferences.preview_quality == PreviewMode::Smart
            && self.selected.as_deref() == Some(path)
            && previous != quality;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_selects_the_smallest_existing_level_within_the_upscale_limit() {
        for (density, expected) in [
            (0.01, PreviewMode::Thumb),
            (0.1875, PreviewMode::Thumb),
            (0.1876, PreviewMode::Draft),
            (0.375, PreviewMode::Draft),
            (0.3751, PreviewMode::Fast),
            (0.75, PreviewMode::Fast),
            (0.7501, PreviewMode::Full),
            (4., PreviewMode::Full),
        ] {
            assert_eq!(resolve(PreviewMode::Smart, Some(density)), expected);
        }
    }

    #[test]
    fn sampling_uses_drawn_bounds_native_extent_and_display_density() {
        assert_eq!(display_density([400., 800.], [2000, 4000], 1.), 0.2);
        let landscape = display_density([1000., 500.], [4000, 2000], 1.);
        assert_eq!(landscape, 0.25);
        assert_eq!(display_density([1000., 500.], [4000, 2000], 2.), 0.5);
        assert_eq!(display_density([4000., 2000.], [4000, 2000], 2.), 2.);
        assert_eq!(
            resolve(PreviewMode::Smart, Some(landscape)),
            PreviewMode::Draft
        );
        assert_eq!(
            resolve(PreviewMode::Smart, Some(landscape * 2.)),
            PreviewMode::Fast
        );
    }

    #[test]
    fn explicit_modes_ignore_display_sampling_and_unknown_geometry_starts_fast() {
        for mode in [
            PreviewMode::Full,
            PreviewMode::Fast,
            PreviewMode::Draft,
            PreviewMode::Thumb,
        ] {
            for density in [None, Some(0.01), Some(0.5), Some(8.)] {
                assert_eq!(resolve(mode, density), mode);
            }
        }
        for density in [None, Some(0.), Some(f32::NAN)] {
            assert_eq!(resolve(PreviewMode::Smart, density), PreviewMode::Fast);
        }
    }
}
