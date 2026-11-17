//! Resolves processing scale from the selected policy and physical display sampling.

use crate::{editor::Rendition, App};
use foveonic::render::{ProcessingQuality, Scale};
use std::path::Path;

/// Before is photographic even while the saved Current root is an inspector.
pub(crate) fn for_view(
    policy: ProcessingQuality,
    rendition: Rendition,
    output: &str,
    comparison: bool,
) -> ProcessingQuality {
    if rendition == Rendition::Current && (output != "final" || comparison) {
        ProcessingQuality::Quality
    } else {
        policy
    }
}

pub(crate) fn resolve(policy: ProcessingQuality, pixels_per_native_pixel: Option<f32>) -> Scale {
    let Some(density) = pixels_per_native_pixel.filter(|v| v.is_finite() && *v > 0.) else {
        return Scale::new(2).unwrap();
    };
    let max_upscale = match policy {
        ProcessingQuality::Quality => 1.1,
        ProcessingQuality::Speed => 1.2,
    };
    [8, 4, 2]
        .into_iter()
        .find(|divisor| density * *divisor as f32 <= max_upscale)
        .map(|divisor| Scale::new(divisor).unwrap())
        .unwrap_or(Scale::NATIVE)
}

/// Drawn bounds exclude letterboxing and captions and include zoom.
pub(crate) fn display_density(size: [f32; 2], extent: [u32; 2], pixels_per_point: f32) -> f32 {
    (size[0] / extent[0].max(1) as f32).max(size[1] / extent[1].max(1) as f32) * pixels_per_point
}

pub(crate) fn accepts_scale(requested: Scale, retained: Scale) -> bool {
    retained.divisor() <= requested.divisor()
}

/// Sampling demand after a transform change, before the next paint.
pub(crate) fn view_density(
    view: &crate::viewport::Viewport,
    native: Option<[u32; 2]>,
    pixels_per_point: f32,
) -> Option<f32> {
    if let Some(native) = native {
        view.current_image_size()
            .map(|size| display_density(size, native, pixels_per_point))
    } else {
        view.zoom_percent(pixels_per_point)
            .map(|percent| percent / 100.)
    }
}

impl App {
    pub(crate) fn observe_photo_view_density(&mut self, path: &Path, pixels_per_point: f32) {
        let view = self.viewports.view(Some(path));
        let frame = if self.selected.as_deref() == Some(path) {
            if self.output != "final"
                || self.comparison.requested()
                || self
                    .frame
                    .as_ref()
                    .is_some_and(|frame| frame.output != "final")
                || (self.framing_preview_active() && !view.explicit_zoom_pending())
            {
                return;
            }
            self.frame.as_ref()
        } else {
            self.rendition_frame(path, self.photo_rendition(path))
                .or_else(|| self.current_photo_frame(path))
        };
        if let Some(density) = view_density(
            view,
            frame.map(|frame| frame.native_display_extent()),
            pixels_per_point,
        ) {
            self.observe_preview_density(path, density);
        }
    }

    pub(crate) fn current_processing_quality(&self) -> ProcessingQuality {
        for_view(
            self.preferences.processing_quality,
            Rendition::Current,
            &self.output,
            self.comparison.requested(),
        )
    }

    pub(crate) fn preview_scale_for(&self, path: &Path, policy: ProcessingQuality) -> Scale {
        self.preview_densities
            .get(path)
            .copied()
            .map(|density| resolve(policy, Some(density)))
            .unwrap_or_else(|| resolve(policy, None))
    }

    /// Only a change of resolved grid creates new photographic demand.
    pub(crate) fn observe_preview_density(&mut self, path: &Path, density: f32) {
        let scale = resolve(self.preferences.processing_quality, Some(density));
        self.preview_densities.insert(path.to_owned(), density);
        let previous = self
            .preview_scales
            .insert(path.to_owned(), scale)
            .unwrap_or_else(|| resolve(self.preferences.processing_quality, None));
        self.preview_demand_changed |= self.selected.as_deref() == Some(path) && previous != scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_group_density_respects_resized_output_sampling() {
        for pixels_per_point in [1., 2.] {
            let ctx = eframe::egui::Context::default();
            let mut view = crate::viewport::Viewport::default();
            let _ = ctx.run(
                eframe::egui::RawInput {
                    screen_rect: Some(eframe::egui::Rect::from_min_size(
                        eframe::egui::Pos2::ZERO,
                        eframe::egui::vec2(400., 300.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    eframe::egui::CentralPanel::default().show(ctx, |ui| {
                        view.show_preview(
                            ui,
                            (
                                eframe::egui::TextureId::User(1),
                                eframe::egui::vec2(1000., 667.),
                            ),
                            [1000, 667],
                            Default::default(),
                        );
                    });
                },
            );
            view.apply_view_change(
                crate::viewport::ViewChange::Snap {
                    scale: 1. / pixels_per_point,
                    center: eframe::egui::vec2(0.5, 0.5),
                },
                pixels_per_point,
            );
            assert_eq!(view.zoom_percent(pixels_per_point), Some(100.));
            let density = view_density(&view, Some([6000, 4002]), pixels_per_point).unwrap();
            assert!(
                (density - 1. / 6.).abs() < 1e-6,
                "resized display samples must not be mistaken for native pixels: {density}"
            );
        }
    }

    #[test]
    fn both_policies_resolve_display_density_with_their_upscale_limit() {
        for (policy, limit) in [
            (ProcessingQuality::Quality, 1.1),
            (ProcessingQuality::Speed, 1.2),
        ] {
            for divisor in [8, 4, 2] {
                assert_eq!(
                    resolve(policy, Some(limit / divisor as f32)).divisor(),
                    divisor
                );
                assert_eq!(
                    resolve(policy, Some(limit / divisor as f32 + 0.0001)).divisor(),
                    divisor / 2
                );
            }
            assert_eq!(resolve(policy, Some(4.)), Scale::NATIVE);
            for density in [None, Some(0.), Some(f32::NAN)] {
                assert_eq!(resolve(policy, density).divisor(), 2);
            }
        }
    }

    #[test]
    fn fitted_half_resolution_uses_the_approved_enlargement_margins() {
        for (policy, density, divisor) in [
            (ProcessingQuality::Quality, 0.52, 2),
            (ProcessingQuality::Quality, 0.55, 2),
            (ProcessingQuality::Quality, 0.5501, 1),
            (ProcessingQuality::Speed, 0.60, 2),
            (ProcessingQuality::Speed, 0.6001, 1),
        ] {
            assert_eq!(resolve(policy, Some(density)).divisor(), divisor);
        }
    }

    #[test]
    fn sampling_uses_drawn_bounds_native_extent_and_display_density() {
        assert_eq!(display_density([400., 800.], [2000, 4000], 1.), 0.2);
        assert_eq!(display_density([1000., 500.], [4000, 2000], 2.), 0.5);
        assert_eq!(display_density([4000., 2000.], [4000, 2000], 2.), 2.);
    }
}

#[cfg(test)]
mod view_policy_tests {
    use super::*;

    #[test]
    fn saved_current_inspection_and_comparison_restore_quality_in_a_speed_workspace() {
        for (output, comparison) in [("sensor.rgb", false), ("final", true)] {
            assert_eq!(
                for_view(
                    ProcessingQuality::Speed,
                    Rendition::Current,
                    output,
                    comparison
                ),
                ProcessingQuality::Quality
            );
            for before in [Rendition::Camera, Rendition::Defaults] {
                assert_eq!(
                    for_view(ProcessingQuality::Speed, before, output, comparison),
                    ProcessingQuality::Speed
                );
            }
        }
        assert_eq!(
            for_view(ProcessingQuality::Speed, Rendition::Current, "final", false),
            ProcessingQuality::Speed
        );
    }
}
