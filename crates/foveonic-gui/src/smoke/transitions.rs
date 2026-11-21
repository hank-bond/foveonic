//! Runs replayable transitions through the existing native scenario.

use crate::{rendering::worker::RenderResult, App};
use eframe::egui;
use foveonic::render::ProcessingQuality;
use std::path::{Path, PathBuf};

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
    GroupBeforeSetup,
    GroupCamera,
    GroupColdEvict,
    GroupColdAddFocus,
    GroupColdReturn,
    GroupDefaults,
    GroupFocus,
    GroupBeforeZoom,
    GroupBeforePan,
    GroupAfter,
    GroupViewPan,
    GroupViewWheel,
    GroupViewFit,
    GroupViewSnap,
    GroupBeforeEnd,
    SmartGroupSolo,
    SmartGroupEnd,
    TileZoom,
    TilePointerZoom,
    TilePan,
    TileSelect,
    TileFit,
    Loupe,
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
    result.push(Step::SmartGroupFit);
    if photos >= 3 {
        result.extend([Step::GroupBeforeSetup, Step::GroupCamera]);
        if photos == 4 {
            result.extend([
                Step::GroupColdEvict,
                Step::GroupColdAddFocus,
                Step::GroupColdReturn,
            ]);
        }
        result.extend([
            Step::GroupDefaults,
            Step::GroupFocus,
            Step::GroupBeforeZoom,
            Step::GroupBeforePan,
            Step::GroupAfter,
            Step::GroupViewPan,
            Step::GroupViewWheel,
            Step::GroupViewFit,
            Step::GroupViewSnap,
            Step::GroupBeforeEnd,
        ]);
    }
    result.extend([
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
        Step::Quality(0),
        Step::Quality(0),
        Step::Quality(0),
        Step::Quality(0),
        Step::Before,
        Step::RefreshOverviews,
        Step::After,
        Step::Quality(1),
        Step::Inspection,
        Step::RefreshInspection,
        Step::Before,
        Step::After,
        Step::Photograph,
        Step::Quality(0),
        Step::TileZoom,
        Step::TilePan,
        Step::TileSelect,
        Step::TileFit,
        Step::TilePointerZoom,
        Step::TileFit,
        Step::SpaceClick,
        Step::Loupe,
        Step::Single,
        Step::Quality(0),
        Step::Quality(1),
        Step::Quality(0),
        Step::Quality(0),
        Step::Quality(0),
        Step::Quality(0),
        Step::Zoom,
        Step::Pan,
        Step::Fit,
        Step::Quality(0),
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

struct ViewSnapshot {
    task: (u64, u64),
    texture: std::sync::Weak<foveonic::image::ImageTexture>,
    scale: foveonic::render::Scale,
    demanded_scale: foveonic::render::Scale,
    settings: foveonic::render::Settings,
    policy: ProcessingQuality,
    origin: foveonic::source::SourceOrigin,
    native_extent: [u32; 2],
    detail: Option<((u64, u64), [u32; 4])>,
}

struct GroupPhoto {
    path: PathBuf,
    authored: foveonic::render::Settings,
    current: std::sync::Weak<foveonic::image::ImageTexture>,
    browser_revision_ceiling: u64,
    origin: foveonic::source::SourceOrigin,
    camera: foveonic::render::Settings,
    defaults: foveonic::render::Settings,
}
struct GroupBefore {
    original_b: foveonic::render::Settings,
    photos: Vec<GroupPhoto>,
    idle_completion: bool,
    editing_limit: usize,
    cold_began: Option<std::time::Instant>,
    zoom_pan: egui::Vec2,
}

struct GroupViewing {
    views: Vec<(f32, egui::Vec2)>,
    focus: Option<PathBuf>,
    began: std::time::Instant,
    pan_delta: egui::Vec2,
}

struct TileView {
    index: usize,
    zoom: f32,
    pan: egui::Vec2,
    snapshot: ViewSnapshot,
}

pub(super) struct TransitionJourney {
    pub(super) model: super::journey::ExposureJourney,
    steps: Vec<Step>,
    index: usize,
    pending: bool,
    single: Option<usize>,
    tile_view: Option<TileView>,
    group_before: Option<GroupBefore>,
    group_viewing: Option<GroupViewing>,
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
    single_view: Option<ViewSnapshot>,
    inspection_output: Option<String>,
    inspection_task: Option<(u64, u64)>,
    photograph_task: Option<(u64, u64)>,
    hold: Option<crate::rendering::worker::DeliveryHold>,
    held_delta: f32,
    loupe: Option<super::loupe::Driver>,
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
            group_before: None,
            group_viewing: None,
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
            single_view: None,
            inspection_output: None,
            inspection_task: None,
            photograph_task: None,
            hold: None,
            held_delta: 0.,
            loupe: None,
        }
    }
    pub(super) fn evidence(&self) -> serde_json::Value {
        serde_json::json!({"seed":self.seed,"sequence":self.steps,"events":self.events,"expected":self.model,"composition":self.composition,"loupe":self.loupe.as_ref().map(super::loupe::Driver::evidence)})
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
        if let Some(loupe) = &mut self.loupe {
            loupe.release();
        }
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
    /// Current regional feedback is complete without optional Whole continuation.
    pub(super) fn smart_feedback<'a>(
        app: &'a App,
        path: &Path,
        native: bool,
    ) -> Option<&'a RenderResult> {
        let settings = if app.selected.as_deref() == Some(path) {
            &app.settings
        } else {
            app.multi_edit.settings(path)?
        };
        let base = app.current_image(path)?;
        let current = |frame: &RenderResult| {
            crate::rendering::matching::same_authored_edits(&frame.request.settings, settings)
                && app.worker.is_settled_and_admitted(frame)
                && app
                    .worker
                    .cache
                    .source_origin(path)
                    .is_none_or(|origin| origin == frame.origin)
        };
        app.photo_patch(path)
            .filter(|detail| {
                let Some(visible) = app.viewports.view(Some(path)).visible_pixels() else {
                    return false;
                };
                let window = base.display_window(false);
                let visible = visible.native_bounds(window);
                current(detail)
                    && detail.artifact.settings.working_scale == foveonic::render::Scale::NATIVE
                    && crate::rendering::smart::contains(detail.detail_region(), visible)
            })
            .or_else(|| {
                (current(base)
                    && (!native
                        || base.artifact.settings.working_scale == foveonic::render::Scale::NATIVE))
                    .then_some(base)
            })
    }

    fn observe_smart_edit(&mut self, app: &App, _paths: &[PathBuf]) -> Result<(), String> {
        for path in app.edit_selection.paths() {
            if self.smart_observed.contains(path) {
                continue;
            }
            if let Some(frame) =
                Self::smart_feedback(app, path, self.smart_zoomed_paths.contains(path))
            {
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

    fn view_snapshot(
        app: &App,
        ctx: &egui::Context,
        path: &std::path::Path,
    ) -> Result<ViewSnapshot, String> {
        let frame = if app.selected.as_deref() == Some(path) {
            app.active_image()
        } else {
            app.photo_layers(path)[0]
        }
        .ok_or("missing photograph")?;
        let view = app.viewports.view(Some(path));
        Ok(ViewSnapshot {
            task: (frame.diagnostic_task.id, frame.diagnostic_task.revision),
            texture: std::sync::Arc::downgrade(&frame.texture),
            scale: frame.artifact.settings.working_scale,
            demanded_scale: super::automatic_scale_bound(
                frame,
                view,
                app.preferences.processing_quality,
                ctx.pixels_per_point(),
            )?,
            settings: frame.request.settings.clone(),
            policy: app.preferences.processing_quality,
            origin: frame.origin.clone(),
            native_extent: frame.native_display_extent(),
            detail: app.photo_patch(path).map(|detail| {
                (
                    (detail.diagnostic_task.id, detail.diagnostic_task.revision),
                    detail
                        .artifact
                        .settings
                        .preview_region
                        .unwrap_or_else(|| detail.display_window(false)),
                )
            }),
        })
    }

    fn check_view(
        app: &App,
        ctx: &egui::Context,
        path: &std::path::Path,
        expected: &mut ViewSnapshot,
    ) -> Result<(bool, bool), String> {
        let view = app.viewports.view(Some(path));
        let frame = if app.selected.as_deref() == Some(path) {
            app.active_image()
        } else {
            app.photo_layers(path)[0]
        }
        .ok_or("missing photograph")?;
        if frame.request.path != *path
            || frame.request.rendition != crate::editor::Rendition::Current
            || !frame.is_photographic()
            || frame.processing_quality() != expected.policy
            || app.preferences.processing_quality != expected.policy
            || frame.origin != expected.origin
            || frame.native_display_extent() != expected.native_extent
            || !crate::rendering::matching::same_authored_edits(
                &frame.request.settings,
                &expected.settings,
            )
        {
            return Err(
                "viewing changed photographic edits, policy, source or native geometry".into(),
            );
        }
        let demanded_scale =
            super::automatic_scale_bound(frame, view, expected.policy, ctx.pixels_per_point())?;
        let demanded = demanded_scale.divisor();
        let visible = view.visible_pixels().ok_or("missing visible pixels")?;
        let window = frame.display_window(false);
        let visible = visible.native_bounds(window);
        let detail = app.photo_patch(path);
        if let Some(detail) = detail {
            if detail.processing_quality() != expected.policy
                || detail.origin != expected.origin
                || !crate::rendering::matching::same_authored_edits(
                    &detail.request.settings,
                    &expected.settings,
                )
                || detail.artifact.settings.working_scale != foveonic::render::Scale::NATIVE
            {
                return Err(
                    "detail does not match its photographic policy, source and edits".into(),
                );
            }
        }
        let detail_snapshot = detail.map(|detail| {
            (
                (detail.diagnostic_task.id, detail.diagnostic_task.revision),
                detail
                    .artifact
                    .settings
                    .preview_region
                    .unwrap_or_else(|| detail.display_window(false)),
            )
        });
        if frame.artifact.settings.working_scale.divisor() > demanded
            && !detail_snapshot
                .is_some_and(|(_, coverage)| crate::rendering::smart::contains(coverage, visible))
        {
            return Err(
                "viewing settled without sufficient sampling and visible native coverage".into(),
            );
        }
        let task = (frame.diagnostic_task.id, frame.diagnostic_task.revision);
        let same_pixels = expected
            .texture
            .upgrade()
            .is_some_and(|texture| std::sync::Arc::ptr_eq(&texture, &frame.texture));
        let prior_refinement = expected.scale.divisor() > expected.demanded_scale.divisor()
            || expected.detail.is_some();
        let finer_needed = frame.artifact.settings.working_scale.divisor()
            < expected.scale.divisor()
            && (expected.scale.divisor() > demanded || prior_refinement);
        // Relaxing demand reuses immutable finer pixels. A prior regional pass may
        // still finish its Whole continuation, and zoom may need finer pixels.
        if !same_pixels && !finer_needed {
            return Err("viewing recomputed existing sufficient Whole pixels".into());
        }
        if let (Some((old_task, old_coverage)), Some((new_task, _))) =
            (expected.detail, detail_snapshot)
        {
            if old_task != new_task
                && crate::rendering::smart::contains(old_coverage, visible)
                && detail.is_some_and(|detail| detail.artifact.settings.preview_region.is_some())
            {
                return Err(
                    "viewing recomputed native detail already covering the visible region".into(),
                );
            }
        }
        expected.task = task;
        expected.texture = std::sync::Arc::downgrade(&frame.texture);
        expected.scale = frame.artifact.settings.working_scale;
        expected.demanded_scale = demanded_scale;
        expected.detail = detail_snapshot;
        Ok((same_pixels, !same_pixels && finer_needed))
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
        let (same_pixels, completed_refinement) =
            Self::check_view(app, ctx, path, &mut expected.snapshot)?;
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
        let snapshot = &expected.snapshot;
        self.events.push(
            serde_json::json!({"tile_view":path,"zoom":zoom,"pan":[view.pan.x,view.pan.y],
                "processing_quality":snapshot.policy,"demanded_scale_divisor":snapshot.demanded_scale.divisor(),
                "overview_scale_divisor":snapshot.scale.divisor(),"overview_task":snapshot.task,
                "reused_overview_pixels":same_pixels,"completed_refinement":completed_refinement,
                "detail":snapshot.detail}),
        );
        Ok(())
    }

    fn check_saved_currents(&self, app: &App, after: bool) -> Result<(), String> {
        let group = self
            .group_before
            .as_ref()
            .ok_or("missing group expectations")?;
        Self::check_current_photos(app, after, &group.photos)
    }

    fn check_current_photos(app: &App, after: bool, photos: &[GroupPhoto]) -> Result<(), String> {
        for (index, photo) in photos.iter().enumerate() {
            let authored = if app.selected.as_ref() == Some(&photo.path) {
                Some(&app.settings)
            } else {
                app.multi_edit.settings(&photo.path)
            }
            .ok_or("missing authored group settings")?;
            if authored != &photo.authored {
                return Err(format!(
                    "group Before changed authored settings for {}",
                    photo.path.display()
                ));
            }
            let current = app
                .current_image(&photo.path)
                .ok_or("group Before lost Current pixels")?;
            let same_pixels = photo
                .current
                .upgrade()
                .is_some_and(|saved| std::sync::Arc::ptr_eq(&saved, &current.texture));
            let native_replacement = after
                && index == 1
                && current.artifact.settings.working_scale == foveonic::render::Scale::NATIVE;
            if !same_pixels && !native_replacement {
                return Err(format!(
                    "group Before replaced saved Current pixels for {}",
                    photo.path.display()
                ));
            }
            if !after
                && app
                    .browser
                    .rendered_revision(&photo.path)
                    .is_some_and(|revision| revision > photo.browser_revision_ceiling)
            {
                return Err(format!(
                    "group Before published alternate browser pixels for {}",
                    photo.path.display()
                ));
            }
        }
        Ok(())
    }

    fn check_group_before(
        &mut self,
        app: &App,
        paths: &[PathBuf],
        step: Step,
    ) -> Result<bool, String> {
        use crate::editor::Rendition;
        let rendition = match step {
            Step::GroupCamera => Rendition::Camera,
            Step::GroupAfter => Rendition::Current,
            _ => Rendition::Defaults,
        };
        if app.before.active != rendition || app.edit_selection.paths() != &paths[..2] {
            return Err("group rendition or editing targets changed during Before viewing".into());
        }
        if step == Step::GroupFocus && app.selected.as_ref() != Some(&paths[1]) {
            return Err("technical focus did not move to the other blue target".into());
        }
        self.check_saved_currents(app, step == Step::GroupAfter)?;
        let group = self
            .group_before
            .as_ref()
            .ok_or("missing group expectations")?;
        if !app.multi_edit.multiple() || app.scope_key.is_some() {
            return Err("group viewing left scopes enabled".into());
        }
        let mut observations = Vec::new();
        for (index, photo) in group.photos.iter().enumerate() {
            let expected_rendition = if index < 2 {
                rendition
            } else {
                Rendition::Current
            };
            if app.photo_rendition(&photo.path) != expected_rendition {
                return Err("a white peer inherited the blue targets' Before rendition".into());
            }
            let frame = if app.selected.as_ref() == Some(&photo.path) {
                app.active_image()
            } else {
                app.photo_layers(&photo.path)[0]
            };
            let Some(frame) = frame.filter(|frame| frame.request.rendition == expected_rendition)
            else {
                return Ok(false);
            };
            let expected = match expected_rendition {
                Rendition::Camera => &photo.camera,
                Rendition::Defaults => &photo.defaults,
                Rendition::Current => &photo.authored,
            };
            if frame.request.path != photo.path
                || frame.request.rendition != expected_rendition
                || frame.processing_quality() != app.preferences.processing_quality
                || frame.origin != photo.origin
                || !crate::rendering::matching::same_authored_edits(
                    &frame.request.settings,
                    expected,
                )
            {
                return Err(format!(
                    "group rendition did not use its own settings/source/geometry for {}",
                    photo.path.display()
                ));
            }
            let mut coverage = None;
            if index == 1
                && matches!(
                    step,
                    Step::GroupBeforeZoom | Step::GroupBeforePan | Step::GroupAfter
                )
            {
                let view = app.viewports.view(Some(&photo.path));
                if !view.explicit_zoom() {
                    return Err("group view lost explicit zoom".into());
                }
                let Some(visible) = view.visible_pixels() else {
                    return Ok(false);
                };
                let window = frame.display_window(false);
                let visible = visible.native_bounds(window);
                let detail = app.photo_patch(&photo.path);
                if let Some(detail) = detail {
                    if detail.request.path != photo.path
                        || detail.request.rendition != expected_rendition
                        || detail.origin != photo.origin
                        || detail.processing_quality() != app.preferences.processing_quality
                        || detail.artifact.settings.working_scale != foveonic::render::Scale::NATIVE
                        || !crate::rendering::matching::same_authored_edits(
                            &detail.request.settings,
                            expected,
                        )
                    {
                        return Err("group native patch has the wrong source, rendition, sampling or authored settings".into());
                    }
                }
                coverage = detail.map(|detail| detail.detail_region());
                if frame.artifact.settings.working_scale != foveonic::render::Scale::NATIVE
                    && !coverage.is_some_and(|coverage| {
                        crate::rendering::smart::contains(coverage, visible)
                    })
                {
                    return Ok(false);
                }
                if step == Step::GroupBeforePan && (view.pan - group.zoom_pan).length() < 1. {
                    return Err("group Before image did not pan".into());
                }
            }
            observations.push(serde_json::json!({"source":photo.path,"rendition":format!("{expected_rendition:?}"),"task":frame.diagnostic_task.id,"crop":frame.request.settings.crop,"rotation":frame.request.settings.rotation,"scale_divisor":frame.artifact.settings.working_scale.divisor(),"detail_coverage":coverage}));
        }
        let event = serde_json::json!({"group_before":step,"photos":observations,"scopes_suppressed":true,"multiple_editing_targets":true,"authored_settings_preserved":true});
        crate::timing::emit("scenario.group_before", || event.clone());
        self.events.push(event);
        Ok(true)
    }

    fn group_view_geometry(
        app: &App,
        path: &std::path::Path,
        pixels_per_point: f32,
    ) -> Result<(f32, egui::Vec2), String> {
        let view = app.viewports.view(Some(path));
        let zoom = view
            .zoom_percent(pixels_per_point)
            .ok_or("missing group view geometry")?;
        let frame = app
            .current_image(path)
            .ok_or("missing group Current frame")?;
        let extent = frame.display_extent(false);
        let display =
            egui::vec2(extent[0] as f32, extent[1] as f32) * (zoom / 100. / pixels_per_point);
        Ok((zoom, egui::vec2(0.5, 0.5) - view.pan / display))
    }

    fn check_group_viewing(
        &mut self,
        app: &App,
        ctx: &egui::Context,
        paths: &[PathBuf],
        step: Step,
    ) -> Result<bool, String> {
        let saved = self
            .group_viewing
            .as_ref()
            .ok_or("missing group viewing expectation")?;
        if app.edit_selection.paths() != &paths[..2]
            || app.selected != saved.focus
            || app.before.active != crate::editor::Rendition::Current
            || crate::photo_tools::selected(ctx) != crate::photo_tools::Tool::Pointer
        {
            return Err(
                "group viewing changed editing targets, technical focus, rendition or tool".into(),
            );
        }
        let geometry = paths
            .iter()
            .map(|path| Self::group_view_geometry(app, path, ctx.pixels_per_point()))
            .collect::<Result<Vec<_>, _>>()?;
        let factor = geometry[1].0 / saved.views[1].0;
        if step == Step::GroupViewWheel
            && (factor <= 1.001 || (geometry[0].0 - geometry[1].0).abs() < 1.)
        {
            return Err(
                "Shift wheel did not zoom the blue group relatively from different magnifications"
                    .into(),
            );
        }
        let group = self
            .group_before
            .as_ref()
            .ok_or("missing group photographs")?;
        let mut observations = Vec::new();
        for (index, photo) in group.photos.iter().enumerate() {
            let view = app.viewports.view(Some(&photo.path));
            let (zoom, center) = geometry[index];
            let authored = if app.selected.as_ref() == Some(&photo.path) {
                Some(&app.settings)
            } else {
                app.multi_edit.settings(&photo.path)
            };
            let frame = app
                .current_image(&photo.path)
                .ok_or("missing group viewing Current")?;
            if authored != Some(&photo.authored)
                || frame.request.path != photo.path
                || frame.origin != photo.origin
                || frame.request.rendition != crate::editor::Rendition::Current
                || frame.processing_quality() != app.preferences.processing_quality
                || !crate::rendering::matching::same_authored_edits(
                    &frame.request.settings,
                    &photo.authored,
                )
            {
                return Err(
                    "group viewing changed authored settings, source or photographic policy".into(),
                );
            }
            let mut coverage = None;
            if index >= 2 {
                if (zoom - saved.views[index].0).abs() > 0.01
                    || (center - saved.views[index].1).length() > 1e-4
                    || !photo
                        .current
                        .upgrade()
                        .is_some_and(|texture| std::sync::Arc::ptr_eq(&texture, &frame.texture))
                {
                    return Err(
                        "blue group gesture changed a white peer's view or Current pixels".into(),
                    );
                }
            } else {
                let expected_center = match step {
                    Step::GroupViewPan => {
                        let size = view
                            .current_image_size()
                            .ok_or("missing group pan geometry")?;
                        saved.views[index].1 - saved.pan_delta / egui::vec2(size[0], size[1])
                    }
                    Step::GroupViewFit | Step::GroupViewSnap => egui::vec2(0.5, 0.5),
                    Step::GroupViewWheel => saved.views[index].1,
                    _ => unreachable!(),
                };
                if (center - expected_center).length() > 1e-4 {
                    return Err(format!("group {step:?} center mismatch for {index}: {center:?} vs {expected_center:?}"));
                }
                match step {
                    Step::GroupViewPan if (zoom - saved.views[index].0).abs() > 0.01 => {
                        return Err("Shift pan changed magnification".into())
                    }
                    Step::GroupViewWheel if (zoom / saved.views[index].0 - factor).abs() > 1e-4 => {
                        return Err(
                            "Shift wheel copied absolute zoom instead of its multiplier".into()
                        )
                    }
                    Step::GroupViewFit if view.explicit_zoom() || view.pan.length() > 0.1 => {
                        return Err(
                            "group double-click did not return both blue peers to centered Fit"
                                .into(),
                        )
                    }
                    Step::GroupViewSnap
                        if (zoom - app.preferences.zoom_snap.multiple() * 100.).abs() > 0.01 =>
                    {
                        return Err(
                            "group double-click did not use the common configured snap".into()
                        )
                    }
                    _ => {}
                }
                if step != Step::GroupViewFit {
                    let Some(visible) = view.visible_pixels() else {
                        return Ok(false);
                    };
                    let window = frame.display_window(false);
                    let visible = visible.native_bounds(window);
                    let detail = app.photo_patch(&photo.path);
                    if let Some(detail) = detail {
                        if detail.request.path != photo.path
                            || detail.origin != photo.origin
                            || detail.request.rendition != crate::editor::Rendition::Current
                            || detail.processing_quality() != app.preferences.processing_quality
                            || detail.artifact.settings.working_scale
                                != foveonic::render::Scale::NATIVE
                            || !crate::rendering::matching::same_authored_edits(
                                &detail.request.settings,
                                &photo.authored,
                            )
                        {
                            return Err("group gesture native detail has incompatible source, settings or policy".into());
                        }
                    }
                    coverage = detail.map(|detail| detail.detail_region());
                    if frame.artifact.settings.working_scale != foveonic::render::Scale::NATIVE
                        && !coverage.is_some_and(|region| {
                            crate::rendering::smart::contains(region, visible)
                        })
                    {
                        return Ok(false);
                    }
                }
            }
            observations.push(serde_json::json!({"source":photo.path,"zoom":zoom,"center":[center.x,center.y],"native_coverage":coverage,"white":index>=2}));
        }
        self.next_double_click = None;
        let event = serde_json::json!({"group_viewing":step,"photos":observations,"zoom_multiplier":factor,"authored_settings_preserved":true,"selection_preserved":true});
        crate::timing::emit("scenario.group_viewing", || event.clone());
        self.events.push(event);
        Ok(true)
    }

    fn cold_group_state(&self, app: &App, paths: &[PathBuf]) -> serde_json::Value {
        serde_json::json!({"group_before_cold":self.steps[self.index],
            "opening":app.opening.is_some(),"worker_busy":app.view_busy(),
            "selected":app.selected,"editing_targets":app.edit_selection.paths(),
            "cold_photos":paths[2..4].iter().map(|path| {
                let frame = if app.selected.as_ref() == Some(path) {
                    app.active_image().filter(|frame| frame.request.path == *path)
                } else {
                    app.photo_layers(path)[0]
                };
                serde_json::json!({"source":path,"settings_known":app.multi_edit.settings(path).is_some(),
                    "metadata_known":app.multi_edit.metadata(path).is_some(),
                    "current_pixels":app.current_image(path).is_some(),
                    "camera_pixels":frame.is_some_and(|frame| frame.request.rendition == crate::editor::Rendition::Camera)})
            }).collect::<Vec<_>>()})
    }

    fn check_cold_group(
        &mut self,
        app: &App,
        paths: &[PathBuf],
        step: Step,
    ) -> Result<bool, String> {
        use crate::editor::Rendition;
        let group = self
            .group_before
            .as_mut()
            .ok_or("missing cold group expectations")?;
        Self::check_current_photos(app, false, &group.photos[..2])?;
        if app.before.active != Rendition::Camera {
            return Err("cold selection left group Camera mode".into());
        }
        if step == Step::GroupColdEvict {
            if app.edit_selection.paths() != &paths[..2] {
                return Err("cold eviction changed the A/B editing targets".into());
            }
            for path in &paths[2..4] {
                if app.multi_edit.metadata(path).is_some()
                    || app.multi_edit.settings(path).is_some()
                    || app.current_image(path).is_some()
                    || app.images.images().any(|frame| frame.request.path == *path)
                    || app.multi_edit.rendering(path)
                {
                    return Err("retention did not evict the cold target's application metadata/pixels/demand".into());
                }
            }
            // The published plan lists only retained photos as history.
            // The common readiness gate has waited for that plan to be processed.
            let event = serde_json::json!({"group_before_cold":step,
                "evicted":&paths[2..4],"retained":app.multi_edit.retained_paths(),
                "render_paths":app.multi_edit.render_paths(),"worker_busy":app.view_busy(),
                "current_demand_retirement":"processed plan history excludes C/D"});
            crate::timing::emit("scenario.group_before_cold", || event.clone());
            self.events.push(event);
            return Ok(true);
        }
        if step == Step::GroupColdAddFocus {
            if app.edit_selection.paths() != &paths[..4] || app.selected.as_ref() != Some(&paths[2])
            {
                return Err("cold targets were not added with C as technical focus".into());
            }
            for (index, path) in paths.iter().enumerate().take(4).skip(2) {
                let photo = &group.photos[index];
                if app.multi_edit.metadata(path).is_none() {
                    return Ok(false);
                }
                let frame = if index == 2 {
                    app.active_image()
                } else {
                    app.photo_layers(path)[0]
                };
                let Some(frame) =
                    frame.filter(|frame| frame.request.rendition == Rendition::Camera)
                else {
                    return Ok(false);
                };
                if frame.request.path != *path
                    || frame.origin != photo.origin
                    || frame.processing_quality() != app.preferences.processing_quality
                    || !crate::rendering::matching::same_authored_edits(
                        &frame.request.settings,
                        &photo.camera,
                    )
                    || app.multi_edit.settings(path) != Some(&photo.authored)
                {
                    return Err(format!(
                        "cold target {index} mismatch: path={}, source={}, policy={}, baseline={}, authored={}, actual_request={:?}, expected_baseline={:?}, authored_settings={:?}, expected_authored={:?}",
                        frame.request.path == *path, frame.origin == photo.origin,
                        frame.processing_quality() == app.preferences.processing_quality,
                        crate::rendering::matching::same_authored_edits(&frame.request.settings, &photo.camera),
                        app.multi_edit.settings(path) == Some(&photo.authored),
                        frame.request.settings, photo.camera, app.multi_edit.settings(path), photo.authored,
                    ));
                }
            }
            if app.opening.is_some() {
                return Ok(false);
            }
        } else {
            if app.edit_selection.paths() != &paths[..2]
                || app.selected.as_ref() != Some(&paths[0])
                || app.preferences.multi_edit_limit != group.editing_limit
            {
                return Err(
                    "cold journey did not restore A/B selection, retention and technical focus"
                        .into(),
                );
            }
            for photo in &mut group.photos[2..] {
                let Some(current) = app.current_image(&photo.path) else {
                    return Ok(false);
                };
                if !crate::rendering::matching::same_authored_edits(
                    &current.request.settings,
                    &photo.authored,
                ) {
                    return Ok(false);
                }
                // These white owners were deliberately evicted and legitimately rendered again.
                photo.current = std::sync::Arc::downgrade(&current.texture);
                photo.browser_revision_ceiling = current
                    .request
                    .id
                    .0
                    .max(app.browser.rendered_revision(&photo.path).unwrap_or(0));
            }
        }
        let event = self.cold_group_state(app, paths);
        crate::timing::emit("scenario.group_before_cold", || event.clone());
        self.events.push(event);
        Ok(true)
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
                Step::GroupViewPan
                    | Step::GroupViewWheel
                    | Step::GroupViewFit
                    | Step::GroupViewSnap
            )
            && self
                .group_viewing
                .as_ref()
                .is_some_and(|saved| saved.began.elapsed() > std::time::Duration::from_secs(25))
        {
            return Err(format!(
                "group viewing {:?} did not settle within 25 seconds",
                self.steps[self.index]
            ));
        }
        if self.pending
            && self.steps[self.index] == Step::GroupColdAddFocus
            && self
                .group_before
                .as_ref()
                .and_then(|group| group.cold_began)
                .is_some_and(|began| began.elapsed() > std::time::Duration::from_secs(25))
        {
            let state = self.cold_group_state(app, paths);
            self.events.push(state.clone());
            return Err(format!(
                "cold group Before did not settle within 25 seconds: {state}"
            ));
        }
        if self.pending
            && self.steps[self.index] == Step::Loupe
            && !self
                .loupe
                .as_mut()
                .unwrap()
                .advance(app, ctx, smoke, paths)?
        {
            return Ok(false);
        }
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
            app.preferences.processing_quality = ProcessingQuality::Speed;
            self.model.adjust_exposure(self.held_delta);
            super::scenarios::apply_exposure(app, f64::from(self.held_delta));
            self.events.push(serde_json::json!({"action":"newer exposure and Speed", "delta":self.held_delta, "coverage":"direct-command"}));
            self.release();
            self.events.push(serde_json::json!({"action":"release delivery", "coverage":"controlled harness wait"}));
            return Ok(false);
        }
        if self.pending && self.steps[self.index] == Step::SmartGroupEdit {
            self.observe_smart_edit(app, paths)?;
        }
        if !smoke.direct_events.is_empty() || !app.sidecars.ready() || app.view_busy() {
            return Ok(false);
        }
        if self.pending {
            let step = self.steps[self.index];
            if step == Step::SmartGroupFit
                && (app.edit_selection.paths() != paths || !app.multi_edit.editable_ready())
            {
                return Ok(false);
            }
            if step == Step::GroupBeforeSetup {
                for path in &paths[..2] {
                    let settings = app
                        .multi_edit
                        .settings(path)
                        .ok_or("missing group settings")?;
                    if app.current_image(path).is_none_or(|frame| {
                        !crate::rendering::matching::same_authored_edits(
                            &frame.request.settings,
                            settings,
                        )
                    }) {
                        return Ok(false);
                    }
                }
            }
            if matches!(
                step,
                Step::GroupColdEvict | Step::GroupColdAddFocus | Step::GroupColdReturn
            ) && !self.check_cold_group(app, paths, step)?
            {
                return Ok(false);
            }
            if matches!(
                step,
                Step::GroupCamera
                    | Step::GroupDefaults
                    | Step::GroupFocus
                    | Step::GroupBeforeZoom
                    | Step::GroupBeforePan
                    | Step::GroupAfter
            ) && !self.check_group_before(app, paths, step)?
            {
                return Ok(false);
            }
            if step == Step::GroupBeforeEnd {
                let original = &self
                    .group_before
                    .as_ref()
                    .ok_or("missing group restore")?
                    .original_b;
                if app.multi_edit.settings(&paths[1]) != Some(original)
                    || app.current_image(&paths[1]).is_none_or(|frame| {
                        !crate::rendering::matching::same_authored_edits(
                            &frame.request.settings,
                            original,
                        )
                    })
                {
                    return Ok(false);
                }
            }
            if matches!(
                step,
                Step::GroupViewPan
                    | Step::GroupViewWheel
                    | Step::GroupViewFit
                    | Step::GroupViewSnap
            ) && !self.check_group_viewing(app, ctx, paths, step)?
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
                                .active_image()
                                .filter(|frame| frame.request.path == *path)
                                .or_else(|| app.photo_layers(path)[0])
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
                if Self::smart_feedback(app, path, true).is_none() {
                    return Ok(false);
                }
            }
            if step == Step::SmartGroupSelectPending {
                for path in paths {
                    if Self::smart_feedback(app, path, self.smart_zoomed_paths.contains(path))
                        .is_none()
                    {
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
                let index = self
                    .tile_view
                    .as_ref()
                    .ok_or("missing tile view expectation")?
                    .index;
                let view = app.viewports.view(Some(&paths[index]));
                // Selection invalidates screen bounds until the next draw. Check
                // the new observed geometry, rather than the previous layout.
                if view.image_rectangles().is_none() || view.visible_pixels().is_none() {
                    return Ok(false);
                }
                self.check_tile_view(app, ctx, paths, step)?;
            }
            if step == Step::Restore {
                if !app.multi_photo_overview() {
                    return Ok(false);
                }
                self.model.select((0..paths.len()).collect());
            }
            if let Step::Quality(which) = step {
                let frame = match app.active_image() {
                    Some(frame) if frame.request.rendition == crate::editor::Rendition::Current => {
                        frame
                    }
                    _ => return Ok(false),
                };
                let _ = which;
                let divisor = crate::rendering::request::camera_preview_scale(
                    app.preview_scale_for(&frame.request.path, app.preferences.processing_quality),
                    &frame.artifact.metadata,
                )
                .map_err(|e| e.to_string())?
                .divisor();
                if frame.processing_quality() != app.preferences.processing_quality
                    || frame.artifact.settings.working_scale.divisor() > divisor
                    || !crate::rendering::matching::same_authored_edits(
                        &frame.request.settings,
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
                        let Some(frame) = app.photo_layers(path)[0] else {
                            return Ok(false);
                        };
                        let expected = crate::rendering::request::camera_preview_scale(
                            app.preview_scale_for(path, app.preferences.processing_quality),
                            &frame.artifact.metadata,
                        )
                        .map_err(|e| e.to_string())?
                        .divisor();
                        let actual = frame.artifact.settings.working_scale.divisor();
                        if frame.processing_quality() != app.preferences.processing_quality
                            || actual > expected
                            || !app.multi_edit.settings(path).is_some_and(|settings| {
                                crate::rendering::matching::same_authored_edits(
                                    &frame.request.settings,
                                    settings,
                                )
                            })
                        {
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
                let index = self.single.ok_or("missing single photo")?;
                if app.selected.as_ref() != Some(&paths[index]) || app.multi_photo_overview() {
                    return Err("single viewing changed the active photograph or layout".into());
                }
                let snapshot = self
                    .single_view
                    .as_mut()
                    .ok_or("missing viewport snapshot")?;
                Self::check_view(app, ctx, &paths[index], snapshot)?;
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
                let path = app.selected.as_deref().ok_or("missing scope photo")?;
                let base = app.active_image().ok_or("missing scope overview")?;
                if app.multi_edit.multiple()
                    || app.before.active != crate::editor::Rendition::Current
                {
                    return Err("single-photo Current scopes did not re-enable".into());
                }
                let calibration = app
                    .noise_calibration
                    .selected(app.preferences.noise_profile);
                let signature = calibration.as_ref().map(|profile| profile.signature());
                let origin = app.worker.cache.source_origin(path);
                let compatible = |frame: &crate::rendering::worker::RenderResult| {
                    frame.request.path.as_path() == path
                        && frame.request.rendition == crate::editor::Rendition::Current
                        && frame.request.output == "final"
                        && !frame.request.framing_preview
                        && frame.comparison.is_none()
                        && frame.processing_quality() == app.preferences.processing_quality
                        && frame.processing_quality() == app.preferences.processing_quality
                        && frame.calibration_signature() == signature
                        && origin.as_ref().is_none_or(|origin| &frame.origin == origin)
                        && crate::rendering::matching::same_authored_edits(
                            &frame.request.settings,
                            &app.settings,
                        )
                };
                // The viewport reads native whole-photo coordinates even when its
                // overview texture is reduced. A patch stores samples relative to its origin.
                let window = base.display_window(false);
                let pixels = app
                    .viewport()
                    .visible_pixels_for([window[2] - window[0], window[3] - window[1]])
                    .ok_or("missing native scope bounds")?;
                let visible = pixels.native_bounds(window);
                let detail = app.photo_patch(path).filter(|frame| {
                    step == Step::ViewportScopes
                        && compatible(frame)
                        && frame.request.detail
                        && frame.artifact.settings.working_scale == foveonic::render::Scale::NATIVE
                        && frame
                            .artifact
                            .settings
                            .preview_region
                            .is_some_and(|region| {
                                crate::rendering::smart::contains(region, visible)
                            })
                });
                // A complete finer image wins over a reduced whole-photo preview.
                // Neither a stale whole nor a regional patch can supply whole-photo scopes.
                let whole = [Some(base), app.photo_layers(path)[0]]
                    .into_iter()
                    .flatten()
                    .filter(|frame| compatible(frame) && frame.is_photographic())
                    .min_by_key(|frame| frame.artifact.settings.working_scale.divisor());
                let Some(source) = detail.or(whole) else {
                    return Ok(false);
                };
                let region = if step == Step::WholeScopes {
                    None
                } else if let Some(patch) = source.artifact.settings.preview_region {
                    Some([
                        visible[0] - patch[0],
                        visible[1] - patch[1],
                        visible[2] - patch[0],
                        visible[3] - patch[1],
                    ])
                } else {
                    let pixels = app
                        .viewport()
                        .visible_pixels_for(source.texture.description().extent)
                        .ok_or("missing scope sampling bounds")?;
                    Some(pixels.bounds())
                };
                let foveonic::inspection::Content::Image(image) = &source.artifact.content else {
                    return Err("scope source is not photographic samples".into());
                };
                if let Some([left, top, right, bottom]) = region {
                    let [width, height] = image.view().extent;
                    if left >= right || top >= bottom || right > width || bottom > height {
                        return Err(
                            "scope sampling coordinates lie outside their source image".into()
                        );
                    }
                }
                let key = crate::rendering::worker::ScopeKey {
                    request_id: source.request.id,
                    region,
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
                    serde_json::json!({"scope_revision":key.request_id,"scope_region":key.region,
                    "scope_source":source.request.path,"scope_source_task":source.diagnostic_task.id,
                    "scope_source_scale":source.artifact.settings.working_scale.divisor(),
                    "scope_source_patch":source.artifact.settings.preview_region}),
                );
            }
            if step == Step::HeldSupersession {
                let frame = match app.active_image() {
                    Some(frame) if frame.request.rendition == crate::editor::Rendition::Current => {
                        frame
                    }
                    _ => return Ok(false),
                };
                if frame.processing_quality() != app.preferences.processing_quality
                    || !crate::rendering::matching::same_authored_edits(
                        &frame.request.settings,
                        &app.settings,
                    )
                {
                    return Ok(false);
                }
            }
            if step == Step::Compose
                && app.active_image().is_none_or(|frame| {
                    !crate::rendering::matching::same_authored_edits(
                        &frame.request.settings,
                        &app.settings,
                    )
                })
            {
                return Ok(false);
            }
            if step == Step::Before
                && app.active_image().is_none_or(|frame| {
                    frame.request.rendition != crate::editor::Rendition::Camera
                        || frame.processing_quality() != app.preferences.processing_quality
                })
            {
                return Ok(false);
            }
            if step == Step::BeforeZoom {
                let frame = app.active_image().ok_or("missing Before zoom image")?;
                if app.before.active != crate::editor::Rendition::Camera
                    || frame.request.rendition != crate::editor::Rendition::Camera
                {
                    return Err("Smart display demand left the Before rendition".into());
                }
                if frame.artifact.settings.working_scale != foveonic::render::Scale::NATIVE {
                    return Ok(false);
                }
            }
            if step == Step::RefreshOverviews {
                if app.before.active != crate::editor::Rendition::Camera
                    || app.active_image().is_none_or(|frame| {
                        frame.request.rendition != crate::editor::Rendition::Camera
                    })
                {
                    return Err("overview refresh replaced the active Before view".into());
                }
                if app.multi_edit.render_paths().len() != paths.len() {
                    return Err("overview refresh lost displayed retained photos".into());
                }
                self.events.push(serde_json::json!({"background_overviews":app.multi_edit.render_paths(),"editing_targets":app.edit_selection.paths(),"active_rendition":"Camera"}));
            }
            if matches!(step, Step::Inspection | Step::RefreshInspection) {
                let Some(frame) = app.active_image() else {
                    return Ok(false);
                };
                if Some(frame.request.output.as_str()) != self.inspection_output.as_deref() {
                    return Ok(false);
                }
                if frame.processing_quality() != ProcessingQuality::Quality
                    || frame.artifact.settings.working_scale != foveonic::render::Scale::NATIVE
                {
                    return Err("pipeline inspection did not use native Quality".into());
                }
                if step == Step::RefreshInspection {
                    for path in app.multi_edit.render_paths() {
                        let Some(overview) = app.current_image(path) else {
                            return Ok(false);
                        };
                        if overview.processing_quality() != ProcessingQuality::Speed {
                            return Err(
                                "inspector refresh changed a background overview's Speed policy"
                                    .into(),
                            );
                        }
                    }
                    self.events.push(serde_json::json!({"observed":"Quality inspector with Speed background overviews"}));
                }
                if !app.worker.is_settled_and_admitted(frame) {
                    return Err("overview refresh retired the active inspection request".into());
                }
            }
            if step == Step::Photograph
                && app
                    .active_image()
                    .is_none_or(|frame| frame.request.output != "final")
            {
                return Ok(false);
            }
            if step == Step::Photograph {
                let frame = app.active_image().unwrap();
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
                && app.active_image().is_none_or(|frame| {
                    frame.request.rendition != crate::editor::Rendition::Current
                })
            {
                return Ok(false);
            }
            if step == Step::After && app.output != "final" {
                let frame = app.active_image().ok_or("missing restored inspector")?;
                if app.preferences.processing_quality != ProcessingQuality::Speed
                    || frame.processing_quality() != ProcessingQuality::Quality
                    || frame.artifact.settings.working_scale != foveonic::render::Scale::NATIVE
                    || Some(frame.request.output.as_str()) != self.inspection_output.as_deref()
                    || Some((frame.diagnostic_task.id, frame.diagnostic_task.revision))
                        != self.inspection_task
                {
                    return Err("Before did not restore its owned native Quality inspector in a Speed workspace".into());
                }
                self.events.push(serde_json::json!({"observed":"Speed Before restored existing Quality inspector", "task":self.inspection_task}));
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
            if !matches!(step, Step::GroupColdEvict | Step::GroupColdAddFocus) {
                self.check(app, paths)?;
            }
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
        if matches!(
            step,
            Step::TileZoom
                | Step::TilePointerZoom
                | Step::TileFit
                | Step::GroupViewFit
                | Step::GroupViewSnap
        ) {
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
        self.events.push(serde_json::json!({"action":step,"coverage": if matches!(step, Step::Select(_) | Step::SpaceClick | Step::TileZoom | Step::TilePointerZoom | Step::TilePan | Step::TileSelect | Step::TileFit | Step::GroupViewPan | Step::GroupViewWheel | Step::GroupViewFit | Step::GroupViewSnap) {"egui-input"} else {"direct-command"}}));
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
                app.preferences.processing_quality = ProcessingQuality::Speed;
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
                app.submit_photos();
            }
            Step::GroupBeforeSetup => {
                let original_b = app
                    .multi_edit
                    .settings(&paths[1])
                    .ok_or("missing B settings")?
                    .clone();
                let mut distinct_b = original_b.clone();
                distinct_b.crop.bounds = [0.1, 0.15, 0.85, 0.9];
                distinct_b.rotation.turns = (app
                    .multi_edit
                    .settings(&paths[0])
                    .ok_or("missing A settings")?
                    .rotation
                    .turns
                    + 1)
                    % 4;
                app.sidecars.apply(paths[1].clone(), distinct_b.clone());
                app.multi_edit.observe_requested(&paths[1], &distinct_b);
                for (index, gesture) in [
                    (0, crate::viewport_selection::Gesture::Replace),
                    (1, crate::viewport_selection::Gesture::Toggle),
                ] {
                    app.edit_selection.apply(
                        crate::viewport_selection::Request {
                            path: paths[index].clone(),
                            gesture,
                        },
                        app.preferences.multi_edit_limit,
                    );
                }
                self.model.select(vec![0, 1]);
                app.select(paths[0].clone(), false);
                app.sync_multi_edit();
                self.group_before = Some(GroupBefore {
                    original_b,
                    photos: Vec::new(),
                    idle_completion: app.preferences.background_full_image_completion,
                    editing_limit: app.preferences.multi_edit_limit,
                    cold_began: None,
                    zoom_pan: egui::Vec2::ZERO,
                });
                app.preferences.background_full_image_completion = false;
                app.submit_photos();
            }
            Step::GroupCamera => {
                // A prior Current preview may still be queued after its viewport settles.
                app.previews.invalidate();
                let mut photos = Vec::new();
                for path in paths {
                    let frame = app
                        .current_image(path)
                        .ok_or("missing saved Current photograph")?;
                    let current = if app.selected.as_ref() == Some(path) {
                        &app.settings
                    } else {
                        app.multi_edit
                            .settings(path)
                            .ok_or("missing saved authored settings")?
                    };
                    let mut camera =
                        foveonic::render::camera_settings(&frame.artifact.metadata, None)
                            .map_err(|e| e.to_string())?;
                    let mut defaults = app
                        .preferences
                        .editor
                        .defaults
                        .for_metadata(&frame.artifact.metadata)
                        .map_err(|e| e.to_string())?;
                    for baseline in [&mut camera, &mut defaults] {
                        baseline.working_scale = current.working_scale;
                        baseline.output_size = current.output_size;
                        baseline.crop = current.crop;
                        baseline.rotation = current.rotation;
                    }
                    photos.push(GroupPhoto {
                        path: path.clone(),
                        authored: current.clone(),
                        current: std::sync::Arc::downgrade(&frame.texture),
                        browser_revision_ceiling: frame
                            .request
                            .id
                            .0
                            .max(app.browser.rendered_revision(path).unwrap_or(0)),
                        origin: frame.origin.clone(),
                        camera: app.preferences.with_workspace_settings(&camera),
                        defaults: app.preferences.with_workspace_settings(&defaults),
                    });
                }
                if photos[0].authored.crop == photos[1].authored.crop
                    && photos[0].authored.rotation == photos[1].authored.rotation
                {
                    return Err("group baselines require distinct authored geometry".into());
                }
                let group = self.group_before.as_mut().unwrap();
                group.photos = photos;
                app.toggle_before(crate::editor::Rendition::Camera);
            }
            Step::GroupColdEvict => {
                app.preferences.multi_edit_limit = 2;
                app.sync_multi_edit();
                app.sync_render_demand();
            }
            Step::GroupColdAddFocus => {
                app.preferences.multi_edit_limit = 4;
                for path in &paths[2..4] {
                    app.edit_selection.apply(
                        crate::viewport_selection::Request {
                            path: path.clone(),
                            gesture: crate::viewport_selection::Gesture::Toggle,
                        },
                        4,
                    );
                }
                self.model.select(vec![0, 1, 2, 3]);
                app.sync_multi_edit();
                // Sidecars resolve asynchronously through ordinary target admission.
                for path in &paths[2..4] {
                    if app.multi_edit.metadata(path).is_some() || app.current_image(path).is_some()
                    {
                        return Err(
                            "cold-add precondition already has metadata or Current pixels".into(),
                        );
                    }
                }
                app.select(paths[2].clone(), false);
                if app.opening.is_none() {
                    return Err("cold technical focus did not create an opening".into());
                }
                self.group_before.as_mut().unwrap().cold_began = Some(std::time::Instant::now());
                let event = self.cold_group_state(app, paths);
                crate::timing::emit("scenario.group_before_cold", || event.clone());
                self.events.push(event);
                app.submit_photos();
            }
            Step::GroupColdReturn => {
                let limit = self.group_before.as_ref().unwrap().editing_limit;
                app.preferences.multi_edit_limit = limit;
                // Restore evicted white owners through ordinary selection/retention, then
                // leave only A/B blue before the next frame can request their renditions.
                for (index, gesture) in [
                    (0, crate::viewport_selection::Gesture::Replace),
                    (paths.len() - 1, crate::viewport_selection::Gesture::Range),
                ] {
                    app.edit_selection.apply(
                        crate::viewport_selection::Request {
                            path: paths[index].clone(),
                            gesture,
                        },
                        limit,
                    );
                }
                app.sync_multi_edit();
                for (index, gesture) in [
                    (0, crate::viewport_selection::Gesture::Replace),
                    (1, crate::viewport_selection::Gesture::Toggle),
                ] {
                    app.edit_selection.apply(
                        crate::viewport_selection::Request {
                            path: paths[index].clone(),
                            gesture,
                        },
                        limit,
                    );
                }
                self.model.select(vec![0, 1]);
                app.select(paths[0].clone(), false);
                app.sync_multi_edit();
                app.submit_photos();
            }
            Step::GroupDefaults => app.toggle_before(crate::editor::Rendition::Defaults),
            Step::GroupFocus => app.select(paths[1].clone(), false),
            Step::GroupBeforeZoom => {
                let view = app.viewports.view_mut(Some(&paths[1]));
                let percent = view
                    .zoom_percent(ctx.pixels_per_point())
                    .ok_or("missing group zoom geometry")?;
                view.zoom_by(200. / percent);
            }
            Step::GroupBeforePan => {
                let view = app.viewports.view_mut(Some(&paths[1]));
                self.group_before.as_mut().unwrap().zoom_pan = view.pan;
                view.pan_to(egui::vec2(0.8, 0.2));
            }
            Step::GroupAfter => {
                app.toggle_before(crate::editor::Rendition::Defaults);
                self.check_saved_currents(app, false)?;
            }
            Step::GroupViewPan
            | Step::GroupViewWheel
            | Step::GroupViewFit
            | Step::GroupViewSnap => {
                crate::photo_tools::pointer(ctx);
                if step == Step::GroupViewPan {
                    for (index, percent) in [(0, 200.), (1, 100.)] {
                        let view = app.viewports.view_mut(Some(&paths[index]));
                        let old = view
                            .zoom_percent(ctx.pixels_per_point())
                            .ok_or("missing group pan setup geometry")?;
                        view.zoom_by(percent / old);
                        view.pan_to(if index == 0 {
                            egui::vec2(0.55, 0.45)
                        } else {
                            egui::vec2(0.45, 0.55)
                        });
                    }
                } else if step == Step::GroupViewSnap {
                    // A is still Fit. B starts explicitly zoomed so independent toggles would diverge.
                    app.viewports.view_mut(Some(&paths[1])).zoom_by(2.);
                }
                let views = paths
                    .iter()
                    .map(|path| Self::group_view_geometry(app, path, ctx.pixels_per_point()))
                    .collect::<Result<Vec<_>, _>>()?;
                let pan_delta = egui::vec2(41., 25.);
                self.group_viewing = Some(GroupViewing {
                    views,
                    focus: app.selected.clone(),
                    began: std::time::Instant::now(),
                    pan_delta,
                });
                let source = if matches!(step, Step::GroupViewWheel | Step::GroupViewFit) {
                    1
                } else {
                    0
                };
                let point = ctx
                    .read_response(app.viewports.view(Some(&paths[source])).id())
                    .ok_or("group source tile is not drawn")?
                    .rect
                    .center();
                match step {
                    Step::GroupViewPan => {
                        let end = point + egui::vec2(41., 25.);
                        smoke.direct_events.extend([
                            vec![
                                egui::Event::PointerMoved(point),
                                egui::Event::PointerButton {
                                    pos: point,
                                    button: egui::PointerButton::Primary,
                                    pressed: true,
                                    modifiers: egui::Modifiers::SHIFT,
                                },
                            ],
                            vec![egui::Event::PointerMoved(end)],
                            vec![egui::Event::PointerButton {
                                pos: end,
                                button: egui::PointerButton::Primary,
                                pressed: false,
                                modifiers: egui::Modifiers::SHIFT,
                            }],
                        ]);
                    }
                    Step::GroupViewWheel => {
                        smoke.direct_events.extend([
                            vec![
                                egui::Event::PointerMoved(point),
                                egui::Event::MouseWheel {
                                    unit: egui::MouseWheelUnit::Point,
                                    delta: egui::vec2(0., 80.),
                                    modifiers: egui::Modifiers::SHIFT,
                                },
                            ],
                            vec![egui::Event::PointerGone],
                        ]);
                    }
                    Step::GroupViewFit | Step::GroupViewSnap => {
                        for _ in 0..2 {
                            app.smoke_multi_click(ctx, smoke, source, egui::Modifiers::SHIFT);
                        }
                    }
                    _ => unreachable!(),
                }
                smoke.direct_events.push_back(vec![]);
            }
            Step::GroupBeforeEnd => {
                let group = self.group_before.as_ref().ok_or("missing group restore")?;
                app.sidecars
                    .apply(paths[1].clone(), group.original_b.clone());
                app.multi_edit
                    .observe_requested(&paths[1], &group.original_b);
                if app.selected.as_ref() == Some(&paths[1]) {
                    app.settings = group.original_b.clone();
                }
                app.preferences.background_full_image_completion = group.idle_completion;
                for path in paths {
                    app.viewports.view_mut(Some(path)).reset();
                }
                for (index, gesture) in [
                    (0, crate::viewport_selection::Gesture::Replace),
                    (paths.len() - 1, crate::viewport_selection::Gesture::Range),
                ] {
                    app.edit_selection.apply(
                        crate::viewport_selection::Request {
                            path: paths[index].clone(),
                            gesture,
                        },
                        app.preferences.multi_edit_limit,
                    );
                }
                self.model.select((0..paths.len()).collect());
                app.select(
                    paths[self
                        .smart_active
                        .ok_or("missing original technical focus")?]
                    .clone(),
                    false,
                );
                app.sync_multi_edit();
                app.submit_photos();
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
                let event = serde_json::json!({"phase":"begin","zoomed":self.smart_zoomed,"editing_count":app.edit_selection.paths().len(),"minimum_revision":app.last_request_id.next().0});
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
                        .hold_delivery(paths[active].clone(), app.last_request_id.next().0),
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
                let view = app.viewports.view(Some(path));
                self.tile_view = Some(TileView {
                    index,
                    zoom: view
                        .zoom_percent(ctx.pixels_per_point())
                        .ok_or("tile has no geometry")?,
                    pan: view.pan,
                    snapshot: Self::view_snapshot(app, ctx, path)?,
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
            Step::Loupe => {
                self.loupe = Some(super::loupe::Driver::new());
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
                app.preferences.processing_quality =
                    [ProcessingQuality::Quality, ProcessingQuality::Speed][which as usize];
                app.submit_photos();
            }
            Step::Zoom => {
                let (_, image) = app
                    .viewport()
                    .image_rectangles()
                    .ok_or("viewport image was not drawn")?;
                self.fit_image = Some(image);
                let index = self.single.ok_or("missing single photo")?;
                if app.selected.as_ref() != Some(&paths[index]) || app.multi_photo_overview() {
                    return Err("single viewing changed the active photograph or layout".into());
                }
                self.single_view = Some(Self::view_snapshot(app, ctx, &paths[index])?);
                app.viewport_mut().zoom_by(4.);
            }
            Step::Pan => app.viewport_mut().pan_to(egui::vec2(0.7, 0.3)),
            Step::ViewportScopes => app.preferences.scope_viewport = true,
            Step::Fit => app.viewport_mut().reset(),
            Step::WholeScopes => app.preferences.scope_viewport = false,
            Step::HeldSupersession => {
                let index = self.single.ok_or("no single photo for delivery hold")?;
                let revision = app.last_request_id.next().0;
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
                app.submit_photos();
            }
            Step::Before => {
                self.inspection_task = if app.output != "final" {
                    app.active_image()
                        .map(|frame| (frame.diagnostic_task.id, frame.diagnostic_task.revision))
                } else {
                    None
                };
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
                app.preferences.processing_quality = ProcessingQuality::Speed;
                app.viewport_mut().zoom_by(20.);
                app.submit_photos();
            }
            Step::RefreshOverviews => {
                app.submit_photos();
            }
            Step::Inspection => {
                let frame = app
                    .active_image()
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
                app.submit_photos();
            }
            Step::RefreshInspection => {
                app.submit_photos();
            }
            Step::Photograph => {
                app.output = "final".into();
                app.submit_photos();
            }
            Step::Restore => {
                app.preferences.processing_quality = ProcessingQuality::Speed;
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
            assert_eq!(
                sequence.iter().filter(|step| **step == Step::Loupe).count(),
                1
            );
            let loupe = sequence
                .iter()
                .position(|step| *step == Step::Loupe)
                .unwrap();
            let single = sequence
                .iter()
                .position(|step| *step == Step::Single)
                .unwrap();
            assert!(
                loupe < single,
                "loupe must run while inactive tiles are displayed"
            );
            assert_eq!(sequence.last(), Some(&Step::Restore));
            assert!(sequence
                .iter()
                .all(|step| !matches!(step, Step::Select(index) if *index >= 8)));
        }
    }
    #[test]
    fn speed_inspection_before_restoration_is_part_of_the_native_journey() {
        let sequence = steps(0, 1, 2);
        assert!(sequence.windows(6).any(|steps| steps
            == [
                Step::Quality(1),
                Step::Inspection,
                Step::RefreshInspection,
                Step::Before,
                Step::After,
                Step::Photograph,
            ]));
    }

    #[test]
    fn group_before_requires_two_blue_targets_and_a_visible_white_peer() {
        assert!(!steps(0, 1, 2).contains(&Step::GroupCamera));
        let sequence = steps(0, 1, 3);
        assert!(sequence.windows(12).any(|steps| steps
            == [
                Step::GroupBeforeSetup,
                Step::GroupCamera,
                Step::GroupDefaults,
                Step::GroupFocus,
                Step::GroupBeforeZoom,
                Step::GroupBeforePan,
                Step::GroupAfter,
                Step::GroupViewPan,
                Step::GroupViewWheel,
                Step::GroupViewFit,
                Step::GroupViewSnap,
                Step::GroupBeforeEnd,
            ]));
        assert!(
            sequence
                .iter()
                .position(|step| *step == Step::GroupBeforeEnd)
                .unwrap()
                < sequence
                    .iter()
                    .position(|step| *step == Step::SmartGroupEdit)
                    .unwrap()
        );
    }

    #[test]
    fn cold_before_addition_requires_four_photos() {
        for photos in [2, 3, 5, 6, 7, 8] {
            assert!(!steps(0, 1, photos).contains(&Step::GroupColdEvict));
        }
        assert!(steps(0, 1, 4).windows(5).any(|steps| steps
            == [
                Step::GroupCamera,
                Step::GroupColdEvict,
                Step::GroupColdAddFocus,
                Step::GroupColdReturn,
                Step::GroupDefaults,
            ]));
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
