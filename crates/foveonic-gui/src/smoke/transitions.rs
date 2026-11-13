//! Runs replayable transitions through the existing native scenario.

use crate::{preferences::PreviewMode, App};
use eframe::egui;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
enum Step {
    Select(usize),
    Edit(bool),
    SpaceClick,
    SmartGroupFit,
    SmartGroupZoom,
    SmartGroupZoomOther,
    SmartGroupSelectPending,
    SmartGroupEdit,
    SmartGroupSolo,
    SmartGroupEnd,
    TileZoom,
    TilePointerZoom,
    TilePan,
    TileSelect,
    TileFit,
    Single,
    Quality(u8),
    Zoom,
    Pan,
    ViewportScopes,
    Fit,
    WholeScopes,
    HeldSupersession,
    Compose,
    Before,
    BeforeZoom,
    RefreshOverviews,
    Inspection,
    RefreshInspection,
    Photograph,
    After,
    Restore,
}

pub(super) fn valid_count(count: usize) -> bool {
    (1..=64).contains(&count)
}

fn steps(seed: u64, count: usize, photos: usize) -> Vec<Step> {
    let mut state = seed;
    let mut result = Vec::new();
    for _ in 0..count {
        // SplitMix64 has a defined sequence even for seed zero.
        state = state.wrapping_add(0x9e3779b97f4a7c15);
        let mut n = state;
        n = (n ^ (n >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        n = (n ^ (n >> 27)).wrapping_mul(0x94d049bb133111eb);
        n ^= n >> 31;
        result.push(Step::Select((n % photos as u64) as usize));
        result.push(Step::Edit(n & 0x100 != 0));
    }
    result.extend([
        Step::SmartGroupFit,
        Step::SmartGroupEdit,
        Step::SmartGroupEdit,
        Step::SmartGroupEdit,
        Step::SmartGroupZoom,
        Step::SmartGroupEdit,
        Step::SmartGroupEdit,
        Step::SmartGroupEdit,
        Step::SmartGroupZoomOther,
        Step::SmartGroupEdit,
        Step::SmartGroupEdit,
        Step::SmartGroupEdit,
        Step::SmartGroupSelectPending,
        Step::SmartGroupSolo,
        Step::SmartGroupEdit,
        Step::SmartGroupEdit,
        Step::SmartGroupEdit,
        Step::SmartGroupEnd,
        Step::Quality(0),
        Step::Quality(1),
        Step::Quality(2),
        Step::Quality(3),
        Step::Quality(4),
        Step::Quality(2),
        Step::Before,
        Step::RefreshOverviews,
        Step::After,
        Step::Inspection,
        Step::RefreshInspection,
        Step::Photograph,
        Step::Quality(4),
        Step::TileZoom,
        Step::TilePan,
        Step::TileSelect,
        Step::TileFit,
        Step::TilePointerZoom,
        Step::TileFit,
        Step::SpaceClick,
        Step::Single,
        Step::Quality(0),
        Step::Quality(1),
        Step::Quality(2),
        Step::Quality(3),
        Step::Quality(4),
        Step::Quality(3),
        Step::Zoom,
        Step::Pan,
        Step::Fit,
        Step::Quality(4),
        Step::Zoom,
        Step::Pan,
        Step::ViewportScopes,
        Step::Fit,
        Step::WholeScopes,
        Step::HeldSupersession,
        Step::Compose,
        Step::Before,
        Step::BeforeZoom,
        Step::After,
        Step::Restore,
    ]);
    result
}

struct TileView {
    index: usize,
    zoom: f32,
    pan: egui::Vec2,
    task: (u64, u64),
}

pub(super) struct TransitionJourney {
    pub(super) model: super::journey::ExposureJourney,
    steps: Vec<Step>,
    index: usize,
    pending: bool,
    single: Option<usize>,
    tile_view: Option<TileView>,
    next_double_click: Option<std::time::Instant>,
    smart_active: Option<usize>,
    smart_zoomed: bool,
    smart_zoomed_paths: Vec<PathBuf>,
    smart_began: Option<std::time::Instant>,
    smart_observed: Vec<PathBuf>,
    smart_fit_scales: Vec<(PathBuf, foveonic::render::Scale)>,
    events: Vec<serde_json::Value>,
    seed: u64,
    fit_image: Option<egui::Rect>,
    zoom_image: Option<egui::Rect>,
    view_request: Option<(foveonic::render::Settings, foveonic::render::Scale)>,
    inspection_output: Option<String>,
    photograph_task: Option<(u64, u64)>,
    hold: Option<crate::rendering::worker::DeliveryHold>,
    held_delta: f32,
    pub(super) composition: Option<(usize, foveonic::crop::Crop, foveonic::rotation::Rotation)>,
}
impl TransitionJourney {
    pub(super) fn new(exposures: Vec<f32>, seed: u64, count: usize) -> Self {
        let sequence = steps(seed, count, exposures.len());
        Self {
            model: super::journey::ExposureJourney::new(exposures),
            steps: sequence,
            index: 0,
            pending: false,
            single: None,
            tile_view: None,
            next_double_click: None,
            smart_active: None,
            smart_zoomed: false,
            smart_zoomed_paths: Vec::new(),
            smart_began: None,
            smart_observed: Vec::new(),
            smart_fit_scales: Vec::new(),
            events: Vec::new(),
            seed,
            composition: None,
            fit_image: None,
            zoom_image: None,
            view_request: None,
            inspection_output: None,
            photograph_task: None,
            hold: None,
            held_delta: 0.,
        }
    }
    pub(super) fn evidence(&self) -> serde_json::Value {
        serde_json::json!({"seed":self.seed,"sequence":self.steps,"events":self.events,"expected":self.model,"composition":self.composition})
    }
    fn check(&self, app: &App, paths: &[PathBuf]) -> Result<(), String> {
        let selection: Result<Vec<_>, _> = app
            .edit_selection
            .paths()
            .iter()
            .map(|path| {
                paths
                    .iter()
                    .position(|candidate| candidate == path)
                    .ok_or("unexpected selected photo")
            })
            .collect();
        self.model
            .check_selection(&selection.map_err(str::to_owned)?)?;
        let exposures: Result<Vec<_>, String> = paths
            .iter()
            .map(|path| {
                let settings = if app.selected.as_ref() == Some(path) {
                    Some(&app.settings)
                } else {
                    app.multi_edit.settings(path)
                }
                .ok_or_else(|| format!("missing settings for {}", path.display()))?;
                if settings.exposure_auto {
                    return Err("automatic exposure enabled".into());
                }
                Ok(settings.exposure)
            })
            .collect();
        self.model.check_exposures(&exposures?)
    }
    pub(super) fn release(&mut self) {
        self.hold.take();
    }
    pub(super) fn advance(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        smoke: &mut super::Smoke,
        paths: &[PathBuf],
    ) -> Result<bool, String> {
        let result = self.advance_inner(app, ctx, smoke, paths);
        if let Err(error) = &result {
            self.events.push(serde_json::json!({"failure":error}));
            self.release();
        }
        result
    }
    fn observe_smart_edit(&mut self, app: &App, _paths: &[PathBuf]) -> Result<(), String> {
        for path in app.edit_selection.paths() {
            if self.smart_observed.contains(path) {
                continue;
            }
            let frame = app
                .multi_edit
                .detail(path)
                .filter(|detail| {
                    app.multi_edit.settings(path).is_some_and(|settings| {
                        crate::rendering::matching::same_authored_edits(&detail.requested, settings)
                    })
                })
                .or_else(|| app.current_photo_frame(path));
            let settings = if app.selected.as_ref() == Some(path) {
                Some(&app.settings)
            } else {
                app.multi_edit.settings(path)
            };
            if let Some(frame) = frame.filter(|frame| {
                settings.is_some_and(|settings| {
                    crate::rendering::matching::same_authored_edits(&frame.requested, settings)
                })
            }) {
                if self.smart_zoomed {
                    let expected = if self.smart_zoomed_paths.contains(path) {
                        foveonic::render::Scale::NATIVE
                    } else {
                        self.smart_fit_scales
                            .iter()
                            .find(|(photo, _)| photo == path)
                            .map(|(_, scale)| *scale)
                            .ok_or("missing Fit scale")?
                    };
                    if frame.artifact.settings.working_scale != expected {
                        return Err(format!(
                            "Smart changed {} to scale {} instead of {}",
                            path.display(),
                            frame.artifact.settings.working_scale.divisor(),
                            expected.divisor()
                        ));
                    }
                }
                let event = serde_json::json!({"phase":"current","zoomed":self.smart_zoomed,"editing_count":app.edit_selection.paths().len(),"source":path,
                    "elapsed_ms":self.smart_began.unwrap().elapsed().as_secs_f64()*1000.,
                    "task":frame.diagnostic_task.id,"revision":frame.diagnostic_task.revision,
                    "scale_divisor":frame.artifact.settings.working_scale.divisor(),
                    "region":frame.artifact.settings.preview_region});
                crate::timing::emit("scenario.smart_edit", || event.clone());
                self.events.push(event);
                self.smart_observed.push(path.clone());
            }
        }
        Ok(())
    }

    fn check_tile_view(
        &mut self,
        app: &App,
        ctx: &egui::Context,
        paths: &[PathBuf],
        step: Step,
    ) -> Result<(), String> {
        let expected = self
            .tile_view
            .as_mut()
            .ok_or("missing tile view expectation")?;
        let path = &paths[expected.index];
        let view = app.viewports.view(Some(path));
        let zoom = view
            .zoom_percent(ctx.pixels_per_point())
            .ok_or("missing tile zoom")?;
        let frame = if app.selected.as_ref() == Some(path) {
            app.frame.as_ref()
        } else {
            app.multi_edit.frame(path)
        }
        .ok_or("missing tile photograph")?;
        if (frame.diagnostic_task.id, frame.diagnostic_task.revision) != expected.task {
            return Err("fixed-quality tile zoom or selection rendered unchanged pixels".into());
        }
        match step {
            Step::TileZoom | Step::TilePointerZoom => {
                self.next_double_click = None;
                if !view.explicit_zoom() || zoom < expected.zoom * 1.5 {
                    return Err("inactive tile did not respond to double-click zoom".into());
                }
                expected.zoom = zoom;
                expected.pan = view.pan;
            }
            Step::TilePan => {
                if (view.pan - expected.pan).length() < 20. {
                    return Err("inactive tile did not respond to Pointer drag".into());
                }
                expected.pan = view.pan;
            }
            Step::TileSelect => {
                if (zoom - expected.zoom).abs() > 0.01 || (view.pan - expected.pan).length() > 0.1 {
                    return Err("activating a tile lost its zoom or pan".into());
                }
            }
            Step::TileFit => {
                self.next_double_click = None;
                if view.explicit_zoom() || view.pan != egui::Vec2::ZERO {
                    return Err(format!("second tile double-click did not restore Fit: zoom={zoom}, pan={:?}, fixed={}", view.pan, view.explicit_zoom()));
                }
                crate::photo_tools::select(ctx, crate::photo_tools::Tool::Pointer);
            }
            _ => unreachable!(),
        }
        self.events
            .push(serde_json::json!({"tile_view":path,"zoom":zoom,"pan":[view.pan.x,view.pan.y]}));
        Ok(())
    }

    fn advance_inner(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        smoke: &mut super::Smoke,
        paths: &[PathBuf],
    ) -> Result<bool, String> {
        if self.pending
            && matches!(
                self.steps[self.index],
                Step::HeldSupersession | Step::SmartGroupSelectPending
            )
            && self.hold.is_some()
        {
            match self.hold.as_ref().unwrap().reached.try_recv() {
                Ok(()) => {}
                Err(crossbeam_channel::TryRecvError::Empty) => return Ok(false),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    return Err("delivery hold disconnected before reaching an image".into())
                }
            }
            self.events.push(serde_json::json!({"observation":"delivery held", "coverage":"controlled harness wait"}));
            if self.steps[self.index] == Step::SmartGroupSelectPending {
                let next = self.smart_active.ok_or("missing pending Smart selection")?;
                app.select(paths[next].clone(), false);
                self.events.push(serde_json::json!({"action":"activate photo while regional delivery is held", "photo":next, "coverage":"direct-command"}));
                self.release();
                return Ok(false);
            }
            app.preferences.preview_quality = PreviewMode::Draft;
            self.model.adjust_exposure(self.held_delta);
            super::scenarios::apply_exposure(app, f64::from(self.held_delta));
            self.events.push(serde_json::json!({"action":"newer exposure and Draft", "delta":self.held_delta, "coverage":"direct-command"}));
            self.release();
            self.events.push(serde_json::json!({"action":"release delivery", "coverage":"controlled harness wait"}));
            return Ok(false);
        }
        if self.pending && self.steps[self.index] == Step::SmartGroupEdit {
            self.observe_smart_edit(app, paths)?;
        }
        if !smoke.direct_events.is_empty() || !app.sidecars.ready() || app.worker.busy() {
            return Ok(false);
        }
        if self.pending {
            let step = self.steps[self.index];
            if step == Step::SmartGroupFit
                && (app.edit_selection.paths() != paths || !app.multi_edit.editable_ready())
            {
                return Ok(false);
            }
            if step == Step::SmartGroupEdit {
                if self.smart_observed.len() != app.edit_selection.paths().len() {
                    return Ok(false);
                }
                if !self.smart_zoomed {
                    self.smart_fit_scales = paths
                        .iter()
                        .map(|path| {
                            let frame = app
                                .frame
                                .as_ref()
                                .filter(|frame| frame.path == *path)
                                .or_else(|| app.multi_edit.frame(path))
                                .ok_or("missing settled Fit photograph")?;
                            Ok((path.clone(), frame.artifact.settings.working_scale))
                        })
                        .collect::<Result<_, String>>()?;
                }
                let event = serde_json::json!({"phase":"complete","zoomed":self.smart_zoomed,"editing_count":app.edit_selection.paths().len(),
                    "elapsed_ms":self.smart_began.unwrap().elapsed().as_secs_f64()*1000.});
                crate::timing::emit("scenario.smart_edit", || event.clone());
                self.events.push(event);
            }
            if matches!(step, Step::SmartGroupZoom | Step::SmartGroupZoomOther) {
                let path = self
                    .smart_zoomed_paths
                    .last()
                    .ok_or("missing zoomed Smart photo")?;
                let frame = if app.selected.as_ref() == Some(path) {
                    app.frame.as_ref()
                } else {
                    app.multi_edit.frame(path)
                };
                if frame.is_none_or(|frame| {
                    frame.artifact.settings.working_scale != foveonic::render::Scale::NATIVE
                }) {
                    return Ok(false);
                }
            }
            if step == Step::SmartGroupSelectPending {
                for path in paths {
                    if app.current_photo_frame(path).is_none_or(|frame| {
                        app.multi_edit.settings(path).is_none_or(|settings| {
                            !crate::rendering::matching::same_authored_edits(
                                &frame.requested,
                                settings,
                            )
                        })
                    }) {
                        return Ok(false);
                    }
                }
            }
            if step == Step::Single {
                if app.multi_photo_overview() {
                    return Ok(false);
                }
                let selected = app
                    .selected
                    .as_ref()
                    .ok_or("single browser selection missing")?;
                let index = paths
                    .iter()
                    .position(|p| p == selected)
                    .ok_or("single selection outside journey")?;
                if self.single != Some(index) {
                    return Err(format!(
                        "browser selected photo {index}, expected {:?}",
                        self.single
                    ));
                }
                self.model.select(vec![index]);
            }
            if matches!(
                step,
                Step::TileZoom
                    | Step::TilePointerZoom
                    | Step::TilePan
                    | Step::TileSelect
                    | Step::TileFit
            ) {
                self.check_tile_view(app, ctx, paths, step)?;
            }
            if step == Step::Restore {
                if !app.multi_photo_overview() {
                    return Ok(false);
                }
                self.model.select((0..paths.len()).collect());
            }
            if let Step::Quality(which) = step {
                let frame = match &app.frame {
                    Some(frame) if frame.rendition == crate::editor::Rendition::Current => frame,
                    _ => return Ok(false),
                };
                let divisor = match which {
                    0 => 1,
                    1 => 2,
                    2 => {
                        let (_, image) = app
                            .viewport()
                            .image_rectangles()
                            .ok_or("missing Smart display geometry")?;
                        let extent = frame.display_extent(false);
                        let density = (image.width() / extent[0] as f32)
                            .max(image.height() / extent[1] as f32)
                            * ctx.pixels_per_point();
                        let nominal = [8, 4, 2, 1]
                            .into_iter()
                            .find(|divisor| density * *divisor as f32 <= 1.5)
                            .unwrap_or(1);
                        if nominal == 8 {
                            foveonic::render::ControlCapabilities::from_metadata(
                                &frame.artifact.metadata,
                            )
                            .map_err(|e| e.to_string())?
                            .thumb_scale()
                            .divisor()
                        } else {
                            nominal
                        }
                    }
                    3 => 4,
                    _ => foveonic::render::ControlCapabilities::from_metadata(
                        &frame.artifact.metadata,
                    )
                    .map_err(|e| e.to_string())?
                    .thumb_scale()
                    .divisor(),
                };
                if frame.artifact.settings.working_scale.divisor() != divisor
                    || !crate::rendering::matching::same_authored_edits(
                        &frame.requested,
                        &app.settings,
                    )
                {
                    return Ok(false);
                }
                if app.multi_photo_overview() {
                    for path in app.multi_edit.render_paths() {
                        if app.selected.as_ref() == Some(path) {
                            continue;
                        }
                        let Some(frame) = app.multi_edit_frame(path) else {
                            return Ok(false);
                        };
                        let expected = match which {
                            0 => 1,
                            1 => 2,
                            3 => 4,
                            2 => crate::rendering::request::camera_preview_scale(
                                app.preview_quality_for(path, PreviewMode::Smart),
                                &frame.artifact.metadata,
                            )
                            .map_err(|e| e.to_string())?
                            .divisor(),
                            _ => foveonic::render::ControlCapabilities::from_metadata(
                                &frame.artifact.metadata,
                            )
                            .map_err(|e| e.to_string())?
                            .thumb_scale()
                            .divisor(),
                        };
                        let actual = frame.artifact.settings.working_scale.divisor();
                        if (if which == 2 {
                            actual > expected
                        } else {
                            actual != expected
                        }) || !app.multi_edit.settings(path).is_some_and(|settings| {
                            crate::rendering::matching::same_authored_edits(
                                &frame.requested,
                                settings,
                            )
                        }) {
                            return Ok(false);
                        }
                        self.events.push(serde_json::json!({"quality_photo":path,"expected_divisor":expected,"actual_divisor":actual}));
                    }
                }
            }
            if matches!(step, Step::Zoom | Step::Pan | Step::Fit) {
                let (_, image) = app
                    .viewport()
                    .image_rectangles()
                    .ok_or("viewport image was not drawn")?;
                let frame = app.frame.as_ref().ok_or("missing viewport frame")?;
                let (request, scale) = self
                    .view_request
                    .as_ref()
                    .ok_or("missing viewport request")?;
                if frame.rendition != crate::editor::Rendition::Current
                    || frame.artifact.settings.working_scale != *scale
                    || !crate::rendering::matching::same_authored_edits(&frame.requested, request)
                {
                    return Err("zoom or pan changed the photographic request or scale".into());
                }
                let fit = self.fit_image.ok_or("missing Fit geometry")?;
                match step {
                    Step::Zoom => {
                        if image.width() < fit.width() * 3.9 || image.height() < fit.height() * 3.9
                        {
                            return Err("drawn image did not zoom".into());
                        }
                        self.zoom_image = Some(image);
                    }
                    Step::Pan => {
                        let zoom = self.zoom_image.ok_or("missing zoom geometry")?;
                        if (image.center() - zoom.center()).length() < 1. {
                            return Err("drawn image did not pan".into());
                        }
                    }
                    Step::Fit => {
                        if (image.min - fit.min).length() > 1.
                            || (image.max - fit.max).length() > 1.
                        {
                            return Err("drawn image did not return to Fit geometry".into());
                        }
                    }
                    _ => unreachable!(),
                }
                self.events.push(serde_json::json!({"drawn_image":[image.min.x,image.min.y,image.max.x,image.max.y]}));
            }
            if matches!(step, Step::ViewportScopes | Step::WholeScopes) {
                let frame = app.frame.as_ref().ok_or("missing frame for scopes")?;
                let expected_region = if step == Step::ViewportScopes {
                    let pixels = app
                        .viewport()
                        .visible_pixels_for(frame.texture.description().extent)
                        .ok_or("missing visible pixel bounds")?;
                    Some([
                        pixels.top_left[0],
                        pixels.top_left[1],
                        pixels.bottom_right[0] + 1,
                        pixels.bottom_right[1] + 1,
                    ])
                } else {
                    None
                };
                let key = crate::rendering::worker::ScopeKey {
                    revision: frame.revision,
                    region: expected_region,
                };
                if app.scope_key != Some(key)
                    || app
                        .scope_snapshot
                        .as_ref()
                        .is_none_or(|snapshot| snapshot.key != key)
                {
                    return Ok(false);
                }
                self.events.push(
                    serde_json::json!({"scope_revision":key.revision,"scope_region":key.region}),
                );
            }
            if step == Step::HeldSupersession {
                let frame = match &app.frame {
                    Some(frame) if frame.rendition == crate::editor::Rendition::Current => frame,
                    _ => return Ok(false),
                };
                if frame.artifact.settings.working_scale.divisor() != 4
                    || !crate::rendering::matching::same_authored_edits(
                        &frame.requested,
                        &app.settings,
                    )
                {
                    return Ok(false);
                }
            }
            if step == Step::Compose
                && app.frame.as_ref().is_none_or(|frame| {
                    !crate::rendering::matching::same_authored_edits(
                        &frame.requested,
                        &app.settings,
                    )
                })
            {
                return Ok(false);
            }
            if step == Step::Before
                && app
                    .frame
                    .as_ref()
                    .is_none_or(|frame| frame.rendition != crate::editor::Rendition::Camera)
            {
                return Ok(false);
            }
            if step == Step::BeforeZoom {
                let frame = app.frame.as_ref().ok_or("missing Before zoom image")?;
                if app.before.active != crate::editor::Rendition::Camera
                    || frame.rendition != crate::editor::Rendition::Camera
                {
                    return Err("Smart display demand left the Before rendition".into());
                }
                if frame.artifact.settings.working_scale != foveonic::render::Scale::NATIVE {
                    return Ok(false);
                }
            }
            if step == Step::RefreshOverviews {
                if app.before.active != crate::editor::Rendition::Camera
                    || app
                        .frame
                        .as_ref()
                        .is_none_or(|frame| frame.rendition != crate::editor::Rendition::Camera)
                {
                    return Err("overview refresh replaced the active Before view".into());
                }
                if app.multi_edit.render_paths().len() != paths.len() {
                    return Err("overview refresh lost displayed retained photos".into());
                }
                self.events.push(serde_json::json!({"background_overviews":app.multi_edit.render_paths(),"editing_targets":app.edit_selection.paths(),"active_rendition":"Camera"}));
            }
            if matches!(step, Step::Inspection | Step::RefreshInspection) {
                let Some(frame) = &app.frame else {
                    return Ok(false);
                };
                if Some(frame.output.as_str()) != self.inspection_output.as_deref() {
                    return Ok(false);
                }
                if !app.worker.is_settled_and_admitted(frame) {
                    return Err("overview refresh retired the active inspection request".into());
                }
            }
            if step == Step::Photograph
                && app
                    .frame
                    .as_ref()
                    .is_none_or(|frame| frame.output != "final")
            {
                return Ok(false);
            }
            if step == Step::Photograph {
                let frame = app.frame.as_ref().unwrap();
                if Some((frame.diagnostic_task.id, frame.diagnostic_task.revision))
                    != self.photograph_task
                {
                    return Err(
                        "returning from inspection rendered an already owned photograph again"
                            .into(),
                    );
                }
            }
            if step == Step::After
                && app
                    .frame
                    .as_ref()
                    .is_none_or(|frame| frame.rendition != crate::editor::Rendition::Current)
            {
                return Ok(false);
            }
            if let Some((index, crop, rotation)) = self.composition {
                let settings = if app.selected.as_ref() == Some(&paths[index]) {
                    Some(&app.settings)
                } else {
                    app.multi_edit.settings(&paths[index])
                }
                .ok_or("missing composed settings")?;
                if settings.crop != crop || settings.rotation != rotation {
                    return Err("composition did not persist".into());
                }
            }
            self.check(app, paths)?;
            if step == Step::SpaceClick {
                if crate::photo_tools::selected(ctx) != crate::photo_tools::Tool::Pointer {
                    return Err("Pointer tool changed after temporary Space input".into());
                }
                crate::photo_tools::select(ctx, crate::photo_tools::Tool::Pointer);
            }
            self.events
                .push(serde_json::json!({"completed":step,"selection":app.edit_selection.paths()}));
            self.pending = false;
            self.index += 1;
        }
        if self.index == self.steps.len() {
            return Ok(true);
        }
        let step = self.steps[self.index];
        if matches!(step, Step::TileZoom | Step::TilePointerZoom | Step::TileFit) {
            // egui groups clicks globally across widgets, including a triple-click
            // interval twice as long as its double-click interval.
            let ready = self.next_double_click.get_or_insert_with(|| {
                let delay = ctx.options(|options| options.input_options.max_double_click_delay);
                std::time::Instant::now() + std::time::Duration::from_secs_f64(delay * 2. + 0.05)
            });
            if std::time::Instant::now() < *ready {
                return Ok(false);
            }
        }
        self.events.push(serde_json::json!({"action":step,"coverage": if matches!(step, Step::Select(_) | Step::SpaceClick | Step::TileZoom | Step::TilePointerZoom | Step::TilePan | Step::TileSelect | Step::TileFit) {"egui-input"} else {"direct-command"}}));
        match step {
            Step::Select(index) => {
                self.model.select(vec![index]);
                app.smoke_multi_click(ctx, smoke, index, egui::Modifiers::NONE);
                smoke.direct_events.push_back(vec![]);
            }
            Step::Edit(positive) => {
                let delta = if positive { 0.125 } else { -0.125 };
                self.model.adjust_exposure(delta);
                super::scenarios::apply_exposure(app, f64::from(delta));
            }
            Step::SmartGroupFit => {
                self.smart_active = app
                    .selected
                    .as_ref()
                    .and_then(|p| paths.iter().position(|v| v == p));
                self.smart_zoomed = false;
                self.smart_zoomed_paths.clear();
                self.smart_fit_scales.clear();
                for path in paths {
                    app.viewports.view_mut(Some(path)).reset();
                }
                app.preferences.preview_quality = PreviewMode::Smart;
                self.model.select((0..paths.len()).collect());
                // Change viewport editing targets through their ordinary command API.
                for (path, gesture) in [
                    (&paths[0], crate::viewport_selection::Gesture::Replace),
                    (
                        &paths[paths.len() - 1],
                        crate::viewport_selection::Gesture::Range,
                    ),
                ] {
                    app.edit_selection.apply(
                        crate::viewport_selection::Request {
                            path: path.clone(),
                            gesture,
                        },
                        app.preferences.multi_edit_limit,
                    );
                }
                app.sync_multi_edit();
                app.submit(None);
            }
            Step::SmartGroupZoom | Step::SmartGroupZoomOther => {
                let active = self.smart_active.ok_or("no active Smart photo")?;
                let index = if step == Step::SmartGroupZoom {
                    active
                } else {
                    (active + 1) % paths.len()
                };
                let path = &paths[index];
                let view = app.viewports.view_mut(Some(path));
                let percent = view
                    .zoom_percent(ctx.pixels_per_point())
                    .ok_or("no Smart view geometry")?;
                view.zoom_by(200. / percent);
                self.smart_zoomed = true;
                self.smart_zoomed_paths.push(path.clone());
            }
            Step::SmartGroupEdit => {
                self.smart_observed.clear();
                self.smart_began = Some(std::time::Instant::now());
                let event = serde_json::json!({"phase":"begin","zoomed":self.smart_zoomed,"editing_count":app.edit_selection.paths().len(),"minimum_revision":app.revision+1});
                crate::timing::emit("scenario.smart_edit", || event.clone());
                self.events.push(event);
                self.model.adjust_exposure(0.125);
                super::scenarios::apply_exposure(app, 0.125);
            }
            Step::SmartGroupSelectPending => {
                let active = app
                    .selected
                    .as_ref()
                    .and_then(|path| paths.iter().position(|candidate| candidate == path))
                    .ok_or("no active Smart photo for held selection")?;
                self.smart_active = Some((active + 1) % paths.len());
                self.hold = Some(
                    app.worker
                        .hold_delivery(paths[active].clone(), app.revision + 1),
                );
                self.model.adjust_exposure(0.125);
                super::scenarios::apply_exposure(app, 0.125);
            }
            Step::SmartGroupSolo => {
                let index = self.smart_active.ok_or("no active Smart photo")?;
                self.model.select(vec![index]);
                app.edit_selection.apply(
                    crate::viewport_selection::Request {
                        path: paths[index].clone(),
                        gesture: crate::viewport_selection::Gesture::Replace,
                    },
                    app.preferences.multi_edit_limit,
                );
                app.sync_multi_edit();
            }
            Step::SmartGroupEnd => {
                for path in paths {
                    app.viewports.view_mut(Some(path)).reset();
                }
                let index = self.smart_active.ok_or("no original Smart selection")?;
                self.model.select(vec![index]);
                crate::photo_tools::select(ctx, crate::photo_tools::Tool::Pointer);
                app.smoke_multi_click(ctx, smoke, index, egui::Modifiers::NONE);
                smoke.direct_events.push_back(vec![]);
            }
            Step::TileZoom | Step::TilePointerZoom => {
                let active = app
                    .selected
                    .as_ref()
                    .and_then(|path| paths.iter().position(|v| v == path))
                    .ok_or("no active tile")?;
                let index = (active + 1) % paths.len();
                let path = &paths[index];
                let frame = app
                    .multi_edit
                    .frame(path)
                    .ok_or("inactive tile has no frame")?;
                let view = app.viewports.view(Some(path));
                self.tile_view = Some(TileView {
                    index,
                    zoom: view
                        .zoom_percent(ctx.pixels_per_point())
                        .ok_or("tile has no geometry")?,
                    pan: view.pan,
                    task: (frame.diagnostic_task.id, frame.diagnostic_task.revision),
                });
                if step == Step::TilePointerZoom {
                    self.model.select(vec![index]);
                } else {
                    let pos = view.response().ok_or("tile is not drawn")?.rect.center();
                    smoke
                        .direct_events
                        .push_back(vec![egui::Event::PointerMoved(pos), space_key(true)]);
                }
                crate::photo_tools::pointer(ctx);
                for _ in 0..2 {
                    app.smoke_multi_click(ctx, smoke, index, egui::Modifiers::NONE);
                }
                if step != Step::TilePointerZoom {
                    smoke.direct_events.push_back(vec![space_key(false)]);
                }
                smoke.direct_events.push_back(vec![]);
            }
            Step::TilePan => {
                let expected = self.tile_view.as_ref().ok_or("no tile view")?;
                let rect = ctx
                    .read_response(app.viewports.view(Some(&paths[expected.index])).id())
                    .ok_or("tile is not drawn")?
                    .rect;
                let start = rect.center();
                let end = start + egui::vec2(40., 25.);
                smoke.direct_events.extend([
                    vec![
                        egui::Event::PointerMoved(start),
                        egui::Event::PointerButton {
                            pos: start,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: Default::default(),
                        },
                    ],
                    vec![egui::Event::PointerMoved(end)],
                    vec![egui::Event::PointerButton {
                        pos: end,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    }],
                    vec![],
                ]);
            }
            Step::TileSelect => {
                let index = self.tile_view.as_ref().ok_or("no tile view")?.index;
                self.model.select(vec![index]);
                crate::photo_tools::select(ctx, crate::photo_tools::Tool::Pointer);
                app.smoke_multi_click(ctx, smoke, index, egui::Modifiers::NONE);
                smoke.direct_events.push_back(vec![]);
            }
            Step::TileFit => {
                let index = self.tile_view.as_ref().ok_or("no tile view")?.index;
                crate::photo_tools::select(ctx, crate::photo_tools::Tool::Pointer);
                for _ in 0..2 {
                    app.smoke_multi_click(ctx, smoke, index, egui::Modifiers::NONE);
                }
                smoke.direct_events.push_back(vec![]);
            }
            Step::SpaceClick => {
                let active = app
                    .selected
                    .as_ref()
                    .and_then(|p| paths.iter().position(|v| v == p))
                    .ok_or("active photo outside journey")?;
                let other = (active + 1) % paths.len();
                crate::photo_tools::select(ctx, crate::photo_tools::Tool::Pointer);
                let pos = app
                    .viewports
                    .view(Some(&paths[other]))
                    .response()
                    .ok_or("Space-click tile is not drawn")?
                    .rect
                    .center();
                smoke
                    .direct_events
                    .push_back(vec![egui::Event::PointerMoved(pos), space_key(true)]);
                app.smoke_multi_click(ctx, smoke, other, egui::Modifiers::NONE);
                smoke.direct_events.push_back(vec![space_key(false)]);
                smoke.direct_events.push_back(vec![]);
            }
            Step::Single => {
                let active = app
                    .selected
                    .as_ref()
                    .and_then(|p| paths.iter().position(|v| v == p))
                    .ok_or("active photo outside journey")?;
                let next = if active + 1 < paths.len() {
                    active + 1
                } else {
                    active - 1
                };
                self.single = Some(next);
                let key = if next > active {
                    egui::Key::ArrowRight
                } else {
                    egui::Key::ArrowLeft
                };
                app.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::Arrow(key),
                    egui::Modifiers::NONE,
                )]);
            }
            Step::Quality(which) => {
                app.viewport_mut().reset();
                app.preferences.preview_quality = [
                    PreviewMode::Full,
                    PreviewMode::Fast,
                    PreviewMode::Smart,
                    PreviewMode::Draft,
                    PreviewMode::Thumb,
                ][which as usize];
                app.submit(None);
            }
            Step::Zoom => {
                let (_, image) = app
                    .viewport()
                    .image_rectangles()
                    .ok_or("viewport image was not drawn")?;
                self.fit_image = Some(image);
                let frame = app.frame.as_ref().ok_or("missing frame")?;
                self.view_request = Some((
                    frame.requested.clone(),
                    frame.artifact.settings.working_scale,
                ));
                app.viewport_mut().zoom_by(4.);
            }
            Step::Pan => app.viewport_mut().pan_to(egui::vec2(0.7, 0.3)),
            Step::ViewportScopes => app.preferences.scope_viewport = true,
            Step::Fit => app.viewport_mut().reset(),
            Step::WholeScopes => app.preferences.scope_viewport = false,
            Step::HeldSupersession => {
                let index = self.single.ok_or("no single photo for delivery hold")?;
                let revision = app.revision + 1;
                self.hold = Some(app.worker.hold_delivery(paths[index].clone(), revision));
                self.held_delta = if self.model.exposures()[index] > 3.5 {
                    -0.125
                } else {
                    0.125
                };
                self.model.adjust_exposure(self.held_delta);
                self.events.push(serde_json::json!({"action":"hold delivery and edit", "photo":index, "minimum_revision":revision, "delta":self.held_delta, "coverage":"controlled harness wait"}));
                super::scenarios::apply_exposure(app, f64::from(self.held_delta));
            }
            Step::Compose => {
                let index = self.single.ok_or("no single photo for composition")?;
                let crop = foveonic::crop::Crop {
                    bounds: [0.125, 0.125, 0.875, 0.875],
                    aspect: foveonic::crop::Aspect::Unconstrained,
                    flipped: false,
                };
                let rotation = foveonic::rotation::Rotation {
                    turns: 1,
                    degrees: 2.0,
                };
                self.composition = Some((index, crop, rotation));
                app.settings.crop = crop;
                app.settings.rotation = rotation;
                app.sidecars.edited(&app.settings);
                app.multi_edit
                    .observe_active(&paths[index], &app.settings, app.metadata.as_ref());
                app.submit(None);
            }
            Step::Before => {
                app.toggle_before(crate::editor::Rendition::Camera);
                if app.before.active != crate::editor::Rendition::Camera {
                    return Err("Before did not activate".into());
                }
            }
            Step::After => {
                app.toggle_before(crate::editor::Rendition::Camera);
                if app.before.active != crate::editor::Rendition::Current {
                    return Err("After did not restore".into());
                }
            }
            Step::BeforeZoom => {
                app.preferences.preview_quality = PreviewMode::Smart;
                app.viewport_mut().zoom_by(20.);
                app.submit(None);
            }
            Step::RefreshOverviews => {
                app.multi_edit.invalidate();
                app.submit_photos();
            }
            Step::Inspection => {
                let frame = app
                    .frame
                    .as_ref()
                    .ok_or("missing photograph before inspection")?;
                self.photograph_task =
                    Some((frame.diagnostic_task.id, frame.diagnostic_task.revision));
                let output =
                    foveonic::inspection::image_outputs(&app.settings, app.metadata.as_deref())
                        .into_iter()
                        .find(|output| {
                            output.name != "final"
                                && output.channels == foveonic::inspection::Channels::Rgb
                        })
                        .ok_or("no RGB inspection output for this photo")?;
                self.inspection_output = Some(output.name.into());
                app.output = output.name.into();
                app.channel = -1;
                app.submit(None);
            }
            Step::RefreshInspection => {
                app.multi_edit.invalidate();
                app.submit_photos();
            }
            Step::Photograph => {
                app.output = "final".into();
                app.submit(None);
            }
            Step::Restore => {
                app.preferences.preview_quality = PreviewMode::Thumb;
                app.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::SelectAll,
                    egui::Modifiers::COMMAND,
                )]);
            }
        }
        self.pending = true;
        Ok(false)
    }
}

fn space_key(pressed: bool) -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seeded_sequences_are_bounded_and_replayable() {
        assert_eq!(steps(0, 1, 2), steps(0, 1, 2));
        assert_ne!(steps(0, 8, 2), steps(1, 8, 2));
        for count in [1, 8, 64] {
            let sequence = steps(u64::MAX, count, 8);
            assert_eq!(
                sequence
                    .iter()
                    .filter(|step| matches!(step, Step::Select(_)))
                    .count(),
                count
            );
            assert_eq!(
                sequence
                    .iter()
                    .filter(|step| matches!(step, Step::Edit(_)))
                    .count(),
                count
            );
            assert_eq!(
                sequence
                    .iter()
                    .filter(|step| **step == Step::HeldSupersession)
                    .count(),
                1
            );
            assert_eq!(
                sequence
                    .iter()
                    .filter(|step| **step == Step::SmartGroupEdit)
                    .count(),
                12
            );
            assert_eq!(sequence.last(), Some(&Step::Restore));
            assert!(sequence
                .iter()
                .all(|step| !matches!(step, Step::Select(index) if *index >= 8)));
        }
    }
    #[test]
    fn rejects_invalid_bounds_and_seed_types() {
        assert!(!valid_count(0));
        assert!(valid_count(1));
        assert!(valid_count(64));
        assert!(!valid_count(65));
        assert!(serde_json::from_str::<u64>("-1").is_err());
        assert!(serde_json::from_str::<u64>("1.5").is_err());
    }
    #[test]
    fn independent_model_rejects_wrong_photo_or_selection() {
        let mut model = super::super::journey::ExposureJourney::new(vec![0., 1.]);
        model.select(vec![1]);
        model.adjust_exposure(0.125);
        assert!(model.check_selection(&[0]).is_err());
        assert!(model.check_exposures(&[0.125, 1.]).is_err());
        assert!(model.check_exposures(&[0., 1.125]).is_ok());
    }
}
