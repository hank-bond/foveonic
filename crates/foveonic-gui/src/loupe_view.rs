//! Presents transient native-detail inspection; its patch is planned by the view planner like a zoomed view.

use crate::{
    editor::Rendition,
    loupe_tool::{self, Geometry},
    rendering::{matching, request::RenderRequest, smart, worker::RenderResult},
    viewport, App,
};
use eframe::egui;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(crate) struct Loupe {
    path: Option<PathBuf>,
    geometry: Option<Geometry>,
}

/// Whether the lens can place this image over the view: same geometry, quality
/// and calibration as the view's request.
fn placeable(image: &RenderResult, request: &RenderRequest) -> bool {
    image.processing_quality() == request.processing_quality
        && matching::same_image_geometry(&image.request.settings, &request.settings)
        && image.calibration_signature()
            == request
                .calibration
                .as_ref()
                .map(|profile| profile.signature())
}

fn coverage(frame: &RenderResult) -> [u32; 4] {
    frame
        .artifact
        .settings
        .preview_region
        .unwrap_or_else(|| frame.request.settings.crop.pixels(frame.uncropped_extent()))
}

impl App {
    /// The lens geometry drawn this frame.
    pub(crate) fn loupe_geometry(&self) -> Option<Geometry> {
        self.loupe.geometry
    }

    pub(crate) fn loupe_inspecting(&self, path: &Path) -> bool {
        self.loupe.path.as_deref() == Some(path)
    }

    /// The drawn layers the lens can place: those with the view's geometry,
    /// quality and calibration. Older edits show while the current ones render.
    pub(crate) fn loupe_images(&self, path: &Path) -> [Option<&RenderResult>; 2] {
        let Some(view) = self.views.get(path) else {
            return [None, None];
        };
        self.photo_layers(path)
            .map(|frame| frame.filter(|frame| placeable(frame, &view.request)))
    }

    /// The loupe layer that best covers `visible` with the current edits.
    pub(crate) fn loupe_image(&self, path: &Path, visible: [u32; 4]) -> Option<&RenderResult> {
        let settings = &self.views.get(path)?.request.settings;
        self.loupe_images(path)
            .into_iter()
            .flatten()
            .max_by_key(|frame| {
                (
                    smart::contains(coverage(frame), visible),
                    matching::same_authored_edits(&frame.request.settings, settings),
                    !frame.artifact.settings.working_scale.is_reduced(),
                    frame.request.id,
                )
            })
    }

    /// Run after every tile has drawn and before planning, so neither the lens nor its
    /// pointer is owned by selection and planning sees this frame's lens.
    pub(crate) fn show_loupe(&mut self, ui: &egui::Ui, mapping: viewport::Mapping) {
        if self.before.active != Rendition::Current
            || self.output != "final"
            || mapping.input
            || mapping.difference_gain != 0.
        {
            loupe_tool::cancel(ui.ctx());
        } else if loupe_tool::active(ui.ctx()) {
            for path in &self.viewport_selection {
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
                || !self.viewport_selection.contains(&gesture.path)
                || !ui.input(|input| input.pointer.primary_down())
        }) {
            loupe_tool::cancel(ui.ctx());
            gesture = None;
        }
        let path = gesture.as_ref().map(|gesture| gesture.path.as_path());
        if self.loupe.path.as_deref() != path {
            self.multi_edit.view_detail(path);
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
        // The lens is placed from the photo's settings and metadata, so planning
        // knows its regions this frame even before any image can be drawn in it.
        let Some(settings) = self.photo_view_settings(&gesture.path) else {
            return;
        };
        let layers = self.loupe_images(&gesture.path);
        let Some(extent) = self
            .photo_metadata(&gesture.path)
            .and_then(|metadata| foveonic::render::native_extent(&metadata).ok())
            .or_else(|| {
                layers
                    .iter()
                    .flatten()
                    .next()
                    .map(|frame| frame.camera_extent())
            })
        else {
            return;
        };
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
        let layers = layers.map(|frame| {
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
}
