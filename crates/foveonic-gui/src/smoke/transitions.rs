//! Runs replayable transitions through the existing native scenario.

use crate::{preferences::PreviewMode, App};
use eframe::egui;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
enum Step {
    Select(usize),
    Edit(bool),
    HandClick,
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
        Step::HandClick,
        Step::Single,
        Step::Quality(0),
        Step::Quality(1),
        Step::Quality(2),
        Step::Quality(3),
        Step::Quality(4),
        Step::Quality(1),
        Step::Zoom,
        Step::Pan,
        Step::ViewportScopes,
        Step::Fit,
        Step::WholeScopes,
        Step::HeldSupersession,
        Step::Compose,
        Step::Before,
        Step::After,
        Step::Restore,
    ]);
    result
}

pub(super) struct TransitionJourney {
    pub(super) model: super::journey::ExposureJourney,
    steps: Vec<Step>,
    index: usize,
    pending: bool,
    single: Option<usize>,
    events: Vec<serde_json::Value>,
    seed: u64,
    fit_image: Option<egui::Rect>,
    zoom_image: Option<egui::Rect>,
    view_request: Option<(foveonic::render::Settings, foveonic::render::Scale)>,
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
            events: Vec::new(),
            seed,
            composition: None,
            fit_image: None,
            zoom_image: None,
            view_request: None,
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
    fn advance_inner(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        smoke: &mut super::Smoke,
        paths: &[PathBuf],
    ) -> Result<bool, String> {
        if self.pending && self.steps[self.index] == Step::HeldSupersession && self.hold.is_some() {
            match self.hold.as_ref().unwrap().reached.try_recv() {
                Ok(()) => {}
                Err(crossbeam_channel::TryRecvError::Empty) => return Ok(false),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    return Err("delivery hold disconnected before reaching an image".into())
                }
            }
            self.events.push(serde_json::json!({"observation":"delivery held", "coverage":"controlled harness wait"}));
            app.preferences.preview_quality = PreviewMode::Draft;
            self.model.adjust_exposure(self.held_delta);
            super::scenarios::apply_exposure(app, f64::from(self.held_delta));
            self.events.push(serde_json::json!({"action":"newer exposure and Draft", "delta":self.held_delta, "coverage":"direct-command"}));
            self.release();
            self.events.push(serde_json::json!({"action":"release delivery", "coverage":"controlled harness wait"}));
            return Ok(false);
        }
        if !smoke.direct_events.is_empty() || !app.sidecars.ready() || app.worker.busy() {
            return Ok(false);
        }
        if self.pending {
            let step = self.steps[self.index];
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
                    1 | 2 => 2,
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
            }
            if matches!(step, Step::Zoom | Step::Pan | Step::Fit) {
                let (_, image) = app
                    .viewport
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
                        .viewport
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
            if step == Step::HandClick {
                if crate::photo_tools::selected(ctx) != crate::photo_tools::Tool::Hand {
                    return Err("Hand tool changed after pointer input".into());
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
        self.events.push(serde_json::json!({"action":step,"coverage": if matches!(step, Step::Select(_) | Step::HandClick) {"egui-input"} else {"direct-command"}}));
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
            Step::HandClick => {
                let active = app
                    .selected
                    .as_ref()
                    .and_then(|p| paths.iter().position(|v| v == p))
                    .ok_or("active photo outside journey")?;
                let other = (active + 1) % paths.len();
                crate::photo_tools::select(ctx, crate::photo_tools::Tool::Hand);
                app.smoke_multi_click(ctx, smoke, other, egui::Modifiers::NONE);
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
                app.viewport.reset();
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
                    .viewport
                    .image_rectangles()
                    .ok_or("viewport image was not drawn")?;
                self.fit_image = Some(image);
                let frame = app.frame.as_ref().ok_or("missing frame")?;
                self.view_request = Some((
                    frame.requested.clone(),
                    frame.artifact.settings.working_scale,
                ));
                app.viewport.zoom_by(4.);
            }
            Step::Pan => app.viewport.pan_to(egui::vec2(0.7, 0.3)),
            Step::ViewportScopes => app.preferences.scope_viewport = true,
            Step::Fit => app.viewport.reset(),
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
