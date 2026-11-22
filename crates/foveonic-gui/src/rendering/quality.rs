//! Resolves processing scale from the selected policy and physical display sampling.

use crate::{editor::Rendition, App};
use foveonic::render::{ControlCapabilities, ProcessingQuality, Scale, Settings};
use foveonic::source::ProcessingFamily;
use std::{collections::BTreeMap, path::Path};

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

/// A photo's size and camera family from its capture properties, known before
/// any image of it exists.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PhotoGeometry {
    /// The oriented photograph size; `None` when the file records none.
    pub extent: Option<[u32; 2]>,
    pub family: Option<ProcessingFamily>,
}
impl PhotoGeometry {
    pub fn from_properties(properties: &BTreeMap<String, String>) -> Self {
        Self {
            extent: foveonic::source::oriented_extent(properties),
            family: properties
                .get("CAMMODEL")
                .and_then(|model| ProcessingFamily::from_camera_model(model)),
        }
    }
}

/// The size a photo is shown at for these settings: its authored rotation and
/// crop, or the whole rotated frame while framing.
pub(crate) fn shown_extent(native: [u32; 2], settings: &Settings, framing: bool) -> [u32; 2] {
    if framing {
        return settings.rotation.oriented_extent(native);
    }
    let [left, top, right, bottom] = settings.crop.pixels(settings.rotation.extent(native));
    [right - left, bottom - top]
}

/// The camera's working scale for a requested scale. Classic photos have fewer
/// pixels, so their smallest grid is a quarter rather than an eighth.
pub(crate) fn camera_scale(requested: Scale, family: ProcessingFamily) -> Scale {
    if requested.divisor() == SMALLEST_GRID {
        ControlCapabilities::from_family(family).thumb_scale()
    } else {
        requested
    }
}

/// The coarsest scale whose enlargement on screen stays within the policy's limit.
pub(crate) fn resolve(policy: ProcessingQuality, pixels_per_native_pixel: f32) -> Scale {
    let max_upscale = match policy {
        ProcessingQuality::Quality => 1.1,
        ProcessingQuality::Speed => 1.2,
    };
    [8, 4, 2]
        .into_iter()
        .find(|divisor| pixels_per_native_pixel * *divisor as f32 <= max_upscale)
        .map(|divisor| Scale::new(divisor).unwrap())
        .unwrap_or(Scale::NATIVE)
}

/// Drawn bounds exclude letterboxing and captions and include zoom.
fn display_density(size: [f32; 2], extent: [u32; 2], pixels_per_point: f32) -> f32 {
    (size[0] / extent[0].max(1) as f32).max(size[1] / extent[1].max(1) as f32) * pixels_per_point
}

pub(crate) fn accepts_scale(requested: Scale, retained: Scale) -> bool {
    retained.divisor() <= requested.divisor()
}

/// How a photo's tile samples it: screen pixels per photo pixel at its current
/// zoom, or as fitted. Only an explicit zoom reads the drawn image, whose
/// shape may differ from the photo's while a placeholder is shown. `None` when
/// the photo has never had a tile.
fn tile_density(
    view: &crate::viewport::Viewport,
    shown: [u32; 2],
    pixels_per_point: f32,
    fit: bool,
) -> Option<f32> {
    if !fit && view.explicit_zoom() {
        if let Some(size) = view.current_image_size() {
            return Some(display_density(size, shown, pixels_per_point));
        }
    }
    let tile = view.tile_size()?;
    let fitted = (tile[0] / shown[0].max(1) as f32).min(tile[1] / shown[1].max(1) as f32);
    Some(fitted * pixels_per_point)
}

/// The scale a photo is processed at, from its geometry, its view settings and
/// its tile. A photo whose file records no size renders at full size (owner),
/// and one never given a tile, such as an overflowed editing photo, at the
/// camera's smallest grid.
pub(crate) fn photo_scale(
    policy: ProcessingQuality,
    geometry: PhotoGeometry,
    settings: &Settings,
    framing: bool,
    view: Option<&crate::viewport::Viewport>,
    pixels_per_point: f32,
    fit: bool,
) -> Scale {
    let Some(native) = geometry.extent else {
        return Scale::NATIVE;
    };
    let shown = shown_extent(native, settings, framing);
    view.and_then(|view| tile_density(view, shown, pixels_per_point, fit))
        .map_or_else(
            || Scale::new(SMALLEST_GRID).unwrap(),
            |density| resolve(policy, density),
        )
}

/// The requested divisor for a photo shown only as a browser thumbnail.
const SMALLEST_GRID: u32 = 8;

impl App {
    /// A photo's geometry, from the capture properties the browser reads for
    /// every listed photo; `None` until they are read.
    pub(crate) fn photo_geometry(&self, path: &Path) -> Option<PhotoGeometry> {
        self.browser
            .capture_properties(path)
            .map(PhotoGeometry::from_properties)
    }

    /// The scale a photo's view needs now, including zoom; `None` until its
    /// settings and capture properties are loaded.
    pub(crate) fn preview_scale_for(
        &self,
        path: &Path,
        policy: ProcessingQuality,
    ) -> Option<Scale> {
        self.scale_for(path, policy, false)
    }

    /// The working scale a photo's view needs: its scale on the camera's grid.
    pub(crate) fn working_scale_for(
        &self,
        path: &Path,
        policy: ProcessingQuality,
    ) -> Option<Scale> {
        let requested = self.preview_scale_for(path, policy)?;
        Some(
            self.photo_geometry(path)?
                .family
                .map_or(requested, |family| camera_scale(requested, family)),
        )
    }

    /// The scale a photo's tile resolves when not zoomed.
    pub(crate) fn fit_scale_for(&self, path: &Path, policy: ProcessingQuality) -> Option<Scale> {
        self.scale_for(path, policy, true)
    }

    fn scale_for(&self, path: &Path, policy: ProcessingQuality, fit: bool) -> Option<Scale> {
        Some(photo_scale(
            policy,
            self.photo_geometry(path)?,
            &self.photo_view_settings(path)?,
            self.image_key(path).framing,
            self.viewports.photo(path),
            self.pixels_per_point,
            fit,
        ))
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
            let density = tile_density(&view, [6000, 4002], pixels_per_point, false).unwrap();
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
                assert_eq!(resolve(policy, limit / divisor as f32).divisor(), divisor);
                assert_eq!(
                    resolve(policy, limit / divisor as f32 + 0.0001).divisor(),
                    divisor / 2
                );
            }
            assert_eq!(resolve(policy, 4.), Scale::NATIVE);
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
            assert_eq!(resolve(policy, density).divisor(), divisor);
        }
    }

    /// A viewport whose tile was painted at `size` points with nothing drawn.
    fn tiled(size: [f32; 2]) -> crate::viewport::Viewport {
        let ctx = eframe::egui::Context::default();
        let mut view = crate::viewport::Viewport::default();
        let _ = ctx.run(
            eframe::egui::RawInput {
                screen_rect: Some(eframe::egui::Rect::from_min_size(
                    eframe::egui::Pos2::ZERO,
                    eframe::egui::vec2(size[0], size[1]),
                )),
                ..Default::default()
            },
            |ctx| {
                eframe::egui::CentralPanel::default()
                    .frame(eframe::egui::Frame::NONE)
                    .show(ctx, |ui| view.show_empty(ui));
            },
        );
        view
    }

    fn merrill(extent: Option<[u32; 2]>) -> PhotoGeometry {
        PhotoGeometry {
            extent,
            family: Some(ProcessingFamily::Merrill),
        }
    }

    #[test]
    fn a_photo_is_planned_from_its_geometry_before_any_image_is_drawn() {
        let quality = ProcessingQuality::Quality;
        let settings = Settings::default();
        let scale = |geometry, view: &crate::viewport::Viewport| {
            photo_scale(quality, geometry, &settings, false, Some(view), 2., false).divisor()
        };
        // 553 screen pixels across 4704 photo pixels: an eighth fits.
        let tile = tiled([276.5, 184.]);
        assert_eq!(scale(merrill(Some([4704, 3136])), &tile), 8);
        assert_eq!(scale(merrill(Some([2268, 1512])), &tile), 4);
        assert_eq!(
            scale(merrill(None), &tile),
            1,
            "a file that records no size renders at full size"
        );
        assert_eq!(
            scale(
                merrill(Some([4704, 3136])),
                &crate::viewport::Viewport::default()
            ),
            8,
            "a photo never given a tile uses the smallest grid"
        );
        assert_eq!(
            photo_scale(
                quality,
                merrill(Some([4704, 3136])),
                &settings,
                false,
                None,
                2.,
                false
            )
            .divisor(),
            8,
            "so does one with no viewport of its own"
        );
    }

    #[test]
    fn the_shown_size_follows_crop_and_rotation_and_framing_shows_the_whole_frame() {
        let mut settings = Settings::default();
        settings.crop.bounds = [0., 0., 0.5, 0.5];
        assert_eq!(shown_extent([4000, 2000], &settings, false), [2000, 1000]);
        assert_eq!(shown_extent([4000, 2000], &settings, true), [4000, 2000]);
        let quality = ProcessingQuality::Quality;
        let tile = tiled([500., 250.]);
        let scale = |framing| {
            photo_scale(
                quality,
                merrill(Some([4000, 2000])),
                &settings,
                framing,
                Some(&tile),
                1.,
                false,
            )
            .divisor()
        };
        assert_eq!(scale(true), 8, "500 of 4000 pixels");
        assert_eq!(scale(false), 4, "500 of the 2000 cropped pixels");
    }

    #[test]
    fn a_fitted_photo_ignores_the_shape_of_a_placeholder_still_drawn() {
        // The uncropped image is still drawn after a crop to a wide strip.
        let ctx = eframe::egui::Context::default();
        let mut view = crate::viewport::Viewport::default();
        let _ = ctx.run(
            eframe::egui::RawInput {
                screen_rect: Some(eframe::egui::Rect::from_min_size(
                    eframe::egui::Pos2::ZERO,
                    eframe::egui::vec2(600., 300.),
                )),
                ..Default::default()
            },
            |ctx| {
                eframe::egui::CentralPanel::default()
                    .frame(eframe::egui::Frame::NONE)
                    .show(ctx, |ui| {
                        view.show_preview(
                            ui,
                            (
                                eframe::egui::TextureId::User(1),
                                eframe::egui::vec2(4704., 3136.),
                            ),
                            [4704, 3136],
                            Default::default(),
                        )
                    });
            },
        );
        let mut settings = Settings::default();
        settings.crop.bounds = [0., 0., 1., 0.375];
        let scale = photo_scale(
            ProcessingQuality::Quality,
            merrill(Some([4704, 3136])),
            &settings,
            false,
            Some(&view),
            1.,
            false,
        );
        assert_eq!(
            scale.divisor(),
            8,
            "600×300 fits 4704×1176 at 0.128, not the drawn image's 0.255"
        );
    }

    #[test]
    fn classic_photos_use_a_quarter_for_the_smallest_grid() {
        let eighth = Scale::new(SMALLEST_GRID).unwrap();
        assert_eq!(camera_scale(eighth, ProcessingFamily::Classic).divisor(), 4);
        assert_eq!(camera_scale(eighth, ProcessingFamily::Merrill).divisor(), 8);
        let half = Scale::new(2).unwrap();
        assert_eq!(camera_scale(half, ProcessingFamily::Classic), half);
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
