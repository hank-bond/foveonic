//! Presents transient native-detail inspection through the existing photo owner and render coordinator.

use crate::{
    editor::Rendition,
    loupe_tool::{self, Geometry},
    rendering::{matching, request::Request, smart, worker::Frame},
    viewport, App,
};
use eframe::egui;
#[cfg(test)]
use foveonic::render::ProcessingQuality;
use foveonic::{
    render::{Scale, Settings},
    source::SourceOrigin,
};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, serde::Serialize)]
struct CoverageObservation {
    capture: u64,
    visible: Option<[u32; 4]>,
    requested_region: Option<[u32; 4]>,
    native_image: Option<(u64, u64)>,
    native_coverage: Option<[u32; 4]>,
    submitted_revision: Option<u64>,
    submitted_region: Option<[u32; 4]>,
    action: &'static str,
}

#[derive(Default)]
pub(crate) struct Loupe {
    path: Option<PathBuf>,
    geometry: Option<Geometry>,
    submitted: Option<(Request, Option<SourceOrigin>)>,
    observation: Option<CoverageObservation>,
}

impl Loupe {
    fn covers_request(
        &self,
        request: &Request,
        origin: &Option<SourceOrigin>,
        visible: Option<[u32; 4]>,
    ) -> bool {
        self.submitted
            .as_ref()
            .is_some_and(|(old, previous_origin)| {
                previous_origin == origin
                    && matching::same_request_context(old, request)
                    && matching::same_authored_edits(&old.settings, &request.settings)
                    && old.settings.preview_region.is_none_or(|region| {
                        visible.is_some_and(|visible| smart::contains(region, visible))
                    })
            })
    }
}

fn coverage(frame: &Frame) -> [u32; 4] {
    frame
        .artifact
        .settings
        .preview_region
        .unwrap_or_else(|| frame.requested.crop.pixels(frame.uncropped_extent()))
}

impl App {
    fn trace_loupe_coverage(
        &mut self,
        path: &Path,
        settings: &Settings,
        geometry: Option<Geometry>,
        action: &'static str,
    ) {
        use foveonic::diagnostics::{self as trace, Category};
        let Some(capture) = trace::capture_id() else {
            return;
        };
        let native = self
            .loupe_images(path, settings)
            .into_iter()
            .flatten()
            .find(|frame| {
                frame.artifact.settings.working_scale == Scale::NATIVE
                    && frame.artifact.settings.output_size.is_none()
                    && matching::same_authored_edits(&frame.requested, settings)
            });
        let submitted = self.loupe.submitted.as_ref().map(|(request, _)| request);
        let observation = CoverageObservation {
            capture,
            visible: geometry.map(|geometry| geometry.visible),
            requested_region: geometry.map(|geometry| geometry.region),
            native_image: native.map(|frame| (frame.diagnostic_task.id, frame.content_revision())),
            native_coverage: native.map(coverage),
            submitted_revision: submitted.map(|request| request.revision),
            submitted_region: submitted.and_then(|request| request.settings.preview_region),
            action,
        };
        if self.loupe.observation == Some(observation) {
            return;
        }
        self.loupe.observation = Some(observation);
        trace::event(Category::Coordinator, "loupe.coverage", |fields| {
            fields
                .str("source", &path.to_string_lossy())
                .str("action", action)
                .str(
                    "geometry",
                    &serde_json::to_string(&observation).expect("coverage diagnostics"),
                );
        });
    }

    pub(crate) fn loupe_inspecting(&self, path: &Path) -> bool {
        self.loupe.path.as_deref() == Some(path)
    }

    fn loupe_settings(&self, path: &Path) -> Option<Settings> {
        let settings = if self.selected.as_deref() == Some(path) && self.sidecars.ready() {
            &self.settings
        } else {
            self.multi_edit.settings(path)?
        };
        Some(self.preferences.render_settings(settings))
    }

    fn loupe_frame_matches(&self, frame: &Frame, path: &Path, settings: &Settings) -> bool {
        let calibration = self
            .noise_calibration
            .selected(self.preferences.noise_profile);
        frame.path == path
            && frame.processing_quality == self.preferences.processing_quality
            && frame.rendition == Rendition::Current
            && frame.output == "final"
            && !frame.framing_preview
            && frame.comparison.is_none()
            && matching::same_image_geometry(&frame.requested, settings)
            && frame.calibration_signature.as_deref()
                == calibration.as_ref().map(|profile| profile.signature())
            && self
                .worker
                .cache
                .source_origin(path)
                .is_none_or(|origin| origin == frame.origin)
    }

    /// Full coverage determines request readiness, independently of useful
    /// partial detail that can still be drawn over the overview.
    pub(crate) fn loupe_image(
        &self,
        path: &Path,
        settings: &Settings,
        visible: [u32; 4],
    ) -> Option<&Frame> {
        [self.current_photo_frame(path), self.multi_edit.detail(path)]
            .into_iter()
            .flatten()
            .filter(|frame| self.loupe_frame_matches(frame, path, settings))
            .max_by_key(|frame| {
                (
                    smart::contains(coverage(frame), visible),
                    matching::same_authored_edits(&frame.requested, settings),
                    !frame.artifact.settings.working_scale.is_reduced(),
                    frame.content_revision(),
                )
            })
    }

    pub(crate) fn loupe_images(&self, path: &Path, settings: &Settings) -> [Option<&Frame>; 2] {
        let overview = self
            .current_photo_frame(path)
            .filter(|frame| self.loupe_frame_matches(frame, path, settings));
        let detail = self.compatible_detail(path, overview, settings, Rendition::Current);
        [overview, detail]
    }

    /// Run after every tile has drawn, so neither the lens nor its pointer is owned by selection.
    pub(crate) fn show_loupe(&mut self, ui: &egui::Ui, mapping: viewport::Mapping) {
        if self.exporting
            || self.before.active != Rendition::Current
            || self.output != "final"
            || mapping.input
            || mapping.difference_gain != 0.
        {
            loupe_tool::cancel(ui.ctx());
        } else if loupe_tool::active(ui.ctx()) {
            for path in &self.displayed_paths {
                let view = self.viewports.view(Some(path));
                if let (Some((response, _)), Some((_, image))) = (
                    view.pointer(ui.input(|input| input.pointer.latest_pos())),
                    view.image_rectangles(),
                ) {
                    loupe_tool::update(ui.ctx(), &response, path, image);
                }
            }
        }
        let mut gesture = loupe_tool::gesture(ui.ctx());
        if gesture.as_ref().is_some_and(|gesture| {
            !loupe_tool::active(ui.ctx())
                || !self.displayed_paths.contains(&gesture.path)
                || !ui.input(|input| input.pointer.primary_down())
        }) {
            loupe_tool::cancel(ui.ctx());
            gesture = None;
        }
        let path = gesture.as_ref().map(|gesture| gesture.path.as_path());
        if self.loupe.path.as_deref() != path {
            if let Some(previous) = &self.loupe.path {
                self.worker.retire_detail(previous);
            }
            self.multi_edit.view_detail(path, &mut self.sidecars);
            self.loupe = Loupe {
                path: path.map(Path::to_owned),
                ..Default::default()
            };
        }
        self.loupe.geometry = None;
        let Some(gesture) = gesture else { return };
        let Some(canvas) = crate::photo_tools::viewport_rect(ui.ctx()) else {
            return;
        };
        let painter = ui
            .painter()
            .with_clip_rect(canvas.intersect(ui.clip_rect()));
        if let Some(placement) = loupe_tool::geometry(
            gesture.pointer,
            [0.5, 0.5],
            [0, 0, 1, 1],
            self.preferences.loupe,
            ui.ctx().pixels_per_point(),
            canvas,
        ) {
            painter.circle_filled(
                placement.lens.center(),
                placement.lens.width() * 0.5,
                egui::Color32::from_gray(18),
            );
            painter.circle_stroke(
                placement.lens.center(),
                placement.lens.width() * 0.5,
                egui::Stroke::new(1_f32, egui::Color32::GRAY),
            );
        }
        let Some(settings) = self.loupe_settings(&gesture.path) else {
            return;
        };
        let base = self.current_photo_frame(&gesture.path);
        let detail = self.multi_edit.detail(&gesture.path);
        let candidates = [base, detail].map(|frame| {
            frame.filter(|frame| self.loupe_frame_matches(frame, &gesture.path, &settings))
        });
        let extent = candidates
            .into_iter()
            .flatten()
            .next()
            .map(Frame::camera_extent);
        let Some(extent) = extent else { return };
        let window = settings.crop.pixels(settings.rotation.extent(extent));
        let focus = [
            window[0] as f32 + gesture.focus_uv[0] * (window[2] - window[0]) as f32,
            window[1] as f32 + gesture.focus_uv[1] * (window[3] - window[1]) as f32,
        ];
        let Some(geometry) = loupe_tool::geometry(
            gesture.pointer,
            focus,
            window,
            self.preferences.loupe,
            ui.ctx().pixels_per_point(),
            canvas,
        ) else {
            return;
        };
        let layers = self.loupe_images(&gesture.path, &settings).map(|frame| {
            frame.map(|frame| viewport::ImageLayer {
                texture: frame.texture.clone(),
                coverage: coverage(frame),
                identity: frame.diagnostic_identity(false),
            })
        });
        viewport::show_loupe(
            &painter,
            self.viewports.view(Some(&gesture.path)).id(),
            layers,
            geometry.lens,
            geometry.focus_native,
            geometry.points_per_native_pixel,
            mapping,
            self.srgb_target,
        );
        self.loupe.geometry = Some(geometry);
    }

    /// Publish one replaceable Detail, independently of the ordinary preview's resolution.
    pub(crate) fn update_loupe(&mut self) {
        let Some(path) = self.loupe.path.clone() else {
            return;
        };
        let geometry = self.loupe.geometry;
        let Some(mut settings) = self.loupe_settings(&path) else {
            return;
        };
        let covered_revision = geometry.and_then(|geometry| {
            self.loupe_image(&path, &settings, geometry.visible)
                .filter(|frame| {
                    frame.artifact.settings.working_scale == Scale::NATIVE
                        && frame.artifact.settings.output_size.is_none()
                        && matching::same_authored_edits(&frame.requested, &settings)
                        && smart::contains(coverage(frame), geometry.visible)
                })
                .map(Frame::content_revision)
        });
        if let Some(revision) = covered_revision {
            let withdraw = self
                .loupe
                .submitted
                .as_ref()
                .is_some_and(|(request, _)| request.revision > revision);
            self.trace_loupe_coverage(
                &path,
                &settings,
                geometry,
                if withdraw { "withdraw" } else { "covered" },
            );
            if withdraw {
                // The retained image is enough again. Withdraw newer demand,
                // leaving displayed pixels and numerical caches with their owners.
                self.worker.retire_detail(&path);
                self.loupe.submitted = None;
            }
            return;
        }
        settings.working_scale = Scale::NATIVE;
        settings.preview_region = geometry.map(|geometry| geometry.region);
        let request = Request {
            detail: true,
            framing_preview: false,
            calibration: self
                .noise_calibration
                .selected(self.preferences.noise_profile),
            rendition: Rendition::Current,
            revision: self.revision + 1,
            path: path.clone(),
            settings,
            preview_scale: foveonic::render::Scale::NATIVE,
            processing_quality: self.preferences.processing_quality,
            output: "final".into(),
            comparison: false,
            export: None,
        };
        let origin = self.worker.cache.source_origin(&path);
        if self
            .loupe
            .covers_request(&request, &origin, geometry.map(|geometry| geometry.visible))
        {
            self.trace_loupe_coverage(&path, &request.settings, geometry, "pending");
            return;
        }
        self.revision = request.revision;
        self.publish_render_view();
        self.loupe.submitted = Some((request.clone(), origin));
        self.trace_loupe_coverage(&path, &request.settings, geometry, "request");
        self.worker.submit(request);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(region: [u32; 4], exposure: f32) -> Request {
        let settings = Settings {
            preview_region: Some(region),
            exposure,
            ..Default::default()
        };
        Request {
            detail: true,
            framing_preview: false,
            calibration: None,
            rendition: Rendition::Current,
            revision: 1,
            path: "photo.x3f".into(),
            settings,
            preview_scale: foveonic::render::Scale::NATIVE,
            processing_quality: ProcessingQuality::Speed,
            output: "final".into(),
            comparison: false,
            export: None,
        }
    }

    #[test]
    fn lens_reuses_overscan_but_replaces_uncovered_or_changed_requests() {
        let loupe = Loupe {
            submitted: Some((request([100, 100, 500, 500], 0.), None)),
            ..Default::default()
        };
        assert!(loupe.covers_request(
            &request([120, 120, 520, 520], 0.),
            &None,
            Some([150, 150, 480, 480])
        ));
        assert!(!loupe.covers_request(
            &request([120, 120, 520, 520], 0.),
            &None,
            Some([150, 150, 510, 480])
        ));
        assert!(!loupe.covers_request(
            &request([100, 100, 500, 500], 0.5),
            &None,
            Some([150, 150, 480, 480])
        ));
    }

    #[test]
    fn initial_whole_detail_supplies_geometry_without_becoming_main_demand() {
        let mut request = request([100, 100, 500, 500], 0.);
        request.settings.preview_region = None;
        assert_eq!(
            request.purpose(),
            crate::rendering::request::Purpose::Detail
        );
        let loupe = Loupe {
            submitted: Some((request.clone(), None)),
            ..Default::default()
        };
        assert!(loupe.covers_request(&request, &None, None));
        assert!(loupe.covers_request(&request, &None, Some([500, 500, 800, 800])));
    }
}
