//! Measures Smart detail viewports while two photos receive shared exposure edits.

use crate::{preferences::PreviewMode, rendering::worker::Frame, App};
use eframe::egui;
use std::{path::PathBuf, time::Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Start,
    Fit,
    Zoom,
    Edit(usize),
    ReturnFit,
    Cleanup,
    Done,
}

// Relative to each photograph's initial value: 0.125, 0.25, 0.125, 0.375.
const EDITS: [f32; 4] = [0.125, 0.125, -0.125, 0.25];

fn is_current_admission(
    current: bool,
    rendition: crate::editor::Rendition,
    exposure: f32,
    expected: f32,
    demand_known: bool,
) -> bool {
    demand_known
        && current
        && rendition == crate::editor::Rendition::Current
        && exposure == expected
}

fn covers_demand(
    scale: u32,
    maximum_divisor: u32,
    native: bool,
    coverage: [u32; 4],
    visible: Option<[u32; 4]>,
) -> bool {
    scale <= maximum_divisor
        && (!native
            || (scale == 1
                && visible.is_some_and(|rect| crate::rendering::smart::contains(coverage, rect))))
}

fn native_demand(count: usize, index: usize, step: Step) -> bool {
    index < count && matches!(step, Step::Zoom | Step::Edit(_))
}

fn referenced_resources(frame: &Frame) -> serde_json::Value {
    let backing = match &frame.artifact.content {
        foveonic::inspection::Content::Image(image) => {
            let description = image.description();
            serde_json::json!({"extent":description.extent,"layout":format!("{:?}",description.layout),
                "byte_len":description.byte_len()})
        }
        _ => serde_json::Value::Null,
    };
    let display = frame.texture.description();
    serde_json::json!({"accounting":"logical references; may alias other frames; not additive or GPU residency",
        "backing":backing,"display":{"extent":display.extent,"layout":format!("{:?}",display.layout),
            "byte_len":crate::memory::texture_bytes(frame.texture.texture())}})
}

pub(super) struct Driver {
    pub model: super::journey::ExposureJourney,
    step: Step,
    started: Instant,
    trace_origin_ns: Option<u64>,
    action_started: Instant,
    minimum_revision: u64,
    native_photo_count: usize,
    first: [bool; 2],
    regional: [bool; 2],
    foreground_ready: bool,
    fit_scales: Option<[u32; 2]>,
    initial_quality: Option<PreviewMode>,
    rows: Vec<serde_json::Value>,
}

impl Driver {
    pub fn new(exposures: Vec<f32>, native_photo_count: usize) -> Self {
        let now = Instant::now();
        Self {
            model: super::journey::ExposureJourney::new(exposures),
            step: Step::Start,
            started: now,
            trace_origin_ns: None,
            action_started: now,
            minimum_revision: 0,
            native_photo_count,
            first: [false; 2],
            regional: [false; 2],
            foreground_ready: false,
            fit_scales: None,
            initial_quality: None,
            rows: Vec::new(),
        }
    }

    pub fn evidence(&self) -> serde_json::Value {
        serde_json::json!({"schema":1,"clock":"monotonic","native_photo_count":self.native_photo_count,"trace_origin_ns":self.trace_origin_ns,"rows":self.rows,"expected_exposures":self.model.exposures(),
            "draw_milestone":"trace-only native command encoding; not physical presentation"})
    }

    fn begin(&mut self, name: &str, app: &App) {
        self.action_started = Instant::now();
        self.minimum_revision = app.revision + 1;
        self.first = [false; 2];
        self.regional = [false; 2];
        self.foreground_ready = false;
        self.rows
            .push(serde_json::json!({"event":"action","action":name,
            "elapsed_ms":self.started.elapsed().as_secs_f64()*1000.,
            "minimum_revision":self.minimum_revision,"coverage":"direct-command"}));
    }

    fn action(&self) -> String {
        match self.step {
            Step::Start | Step::Fit => "fit-smart".into(),
            Step::Zoom => self.zoom_action().into(),
            Step::Edit(index) => format!("exposure-{index}"),
            Step::ReturnFit | Step::Cleanup | Step::Done => "return-fit".into(),
        }
    }

    fn zoom_action(&self) -> &'static str {
        if self.native_photo_count == 2 {
            "zoom-both-200"
        } else {
            "zoom-a-200"
        }
    }

    pub fn observe_admitted(
        &mut self,
        paths: &[PathBuf],
        frame: &Frame,
        current: bool,
        destination: crate::rendering::delivery::Destination,
        visible: Option<[u32; 4]>,
    ) {
        if matches!(self.step, Step::Cleanup | Step::Done) {
            return;
        }
        let Some(index) = paths.iter().position(|path| path == &frame.path) else {
            return;
        };
        let now = self.started.elapsed().as_secs_f64() * 1000.;
        let action_ms = self.action_started.elapsed().as_secs_f64() * 1000.;
        let action = self.action();
        let row = serde_json::json!({"event":"ui-accepted","action":action,"photo":index,
            "source":frame.path,"elapsed_ms":now,"action_elapsed_ms":action_ms,
            "task":frame.diagnostic_task.id,"revision":frame.diagnostic_task.revision,
            "presentation_revision":frame.revision,"rendition":format!("{:?}",frame.rendition),
            "effective_scale":frame.artifact.settings.working_scale.divisor(),
            "requested_region":frame.requested.preview_region,"actual_region":frame.artifact.settings.preview_region,
            "available_region":frame.detail_region(),
            "exposure":frame.artifact.settings.exposure,"current":current,
            "destination":format!("{destination:?}"),"visible_region":visible,"reuse":null,
            "referenced_resources":referenced_resources(frame)});
        self.rows.push(row.clone());
        if !is_current_admission(
            current,
            frame.rendition,
            frame.artifact.settings.exposure,
            self.model.exposures()[index],
            self.fit_scales.is_some(),
        ) || !self.frame_covers(index, frame, visible)
        {
            return;
        }
        if !self.first[index] {
            let mut milestone = row.clone();
            milestone["event"] = "first-current".into();
            self.rows.push(milestone);
            self.first[index] = true;
        }
        if index < self.native_photo_count
            && !self.regional[index]
            && frame.artifact.settings.preview_region.is_some()
        {
            let mut milestone = row.clone();
            milestone["event"] = "regional-accepted".into();
            self.rows.push(milestone);
            self.regional[index] = true;
        }
        if frame.artifact.settings.preview_region.is_none() {
            let mut milestone = row;
            milestone["event"] = "whole-current-accepted".into();
            self.rows.push(milestone);
        }
        if matches!(self.step, Step::Edit(_))
            && self.first.iter().all(|seen| *seen)
            && !self.foreground_ready
        {
            self.rows.push(
                serde_json::json!({"event":"all-foreground-ready","action":self.action(),
                "elapsed_ms":now,"action_elapsed_ms":action_ms}),
            );
            self.foreground_ready = true;
        }
    }

    fn frame_covers(&self, index: usize, frame: &Frame, visible: Option<[u32; 4]>) -> bool {
        let native = native_demand(self.native_photo_count, index, self.step);
        let maximum = if native {
            1
        } else {
            self.fit_scales.map_or(u32::MAX, |scales| scales[index])
        };
        covers_demand(
            frame.artifact.settings.working_scale.divisor(),
            maximum,
            native,
            frame.detail_region(),
            visible,
        )
    }

    fn visible(app: &App, path: &std::path::Path) -> Option<[u32; 4]> {
        let window = app.current_photo_frame(path)?.display_window(false);
        let pixels = app.viewports.view(Some(path)).visible_pixels()?;
        Some([
            pixels.top_left[0] + window[0],
            pixels.top_left[1] + window[1],
            pixels.bottom_right[0] + 1 + window[0],
            pixels.bottom_right[1] + 1 + window[1],
        ])
    }

    fn observation(&mut self, name: &str, index: usize, frame: &Frame, visible: Option<[u32; 4]>) {
        self.rows.push(serde_json::json!({"event":name,"action":self.action(),"photo":index,"source":frame.path,
            "elapsed_ms":self.started.elapsed().as_secs_f64()*1000.,
            "action_elapsed_ms":self.action_started.elapsed().as_secs_f64()*1000.,
            "task":frame.diagnostic_task.id,"revision":frame.diagnostic_task.revision,
            "presentation_revision":frame.revision,"rendition":format!("{:?}",frame.rendition),
            "effective_scale":frame.artifact.settings.working_scale.divisor(),
            "requested_region":frame.requested.preview_region,
            "actual_region":frame.artifact.settings.preview_region,
            "available_region":frame.detail_region(),"visible_region":visible,
            "exposure":frame.artifact.settings.exposure,
            "referenced_resources":referenced_resources(frame),
            "reuse":null,"observation":"retained UI frame"}));
    }

    fn current<'a>(&self, app: &'a App, path: &std::path::Path, index: usize) -> Option<&'a Frame> {
        let settings = app.multi_edit.settings(path)?;
        let base = app.current_photo_frame(path);
        let detail = app.photo_detail(path, base, settings, crate::editor::Rendition::Current);
        [
            detail,
            base.filter(|frame| app.reusable_photograph(path, frame, settings)),
        ]
        .into_iter()
        .flatten()
        .filter(|frame| {
            frame.rendition == crate::editor::Rendition::Current
                && frame.artifact.settings.exposure == self.model.exposures()[index]
                && crate::rendering::matching::same_authored_edits(&frame.requested, settings)
                && self.frame_covers(index, frame, Self::visible(app, path))
        })
        .max_by_key(|frame| frame.content_revision())
    }

    fn observe(&mut self, app: &App, paths: &[PathBuf]) {
        for (index, path) in paths.iter().enumerate() {
            let Some(frame) = self.current(app, path, index) else {
                continue;
            };
            if !self.first[index] {
                self.observation(
                    if frame.diagnostic_task.revision < self.minimum_revision {
                        "reuse-observed"
                    } else {
                        "first-current-observed"
                    },
                    index,
                    frame,
                    Self::visible(app, path),
                );
                self.first[index] = true;
            }
            if index < self.native_photo_count
                && !self.regional[index]
                && frame.artifact.settings.preview_region.is_some()
            {
                self.observation("regional-observed", index, frame, Self::visible(app, path));
                self.regional[index] = true;
            }
        }
        if matches!(self.step, Step::Edit(_))
            && self.first.iter().all(|seen| *seen)
            && !self.foreground_ready
        {
            self.rows.push(
                serde_json::json!({"event":"all-foreground-observed","action":self.action(),
                "elapsed_ms":self.started.elapsed().as_secs_f64()*1000.,
                "action_elapsed_ms":self.action_started.elapsed().as_secs_f64()*1000.,
                "observation":"retained UI frames"}),
            );
            self.foreground_ready = true;
        }
    }

    fn whole_ready(&self, app: &App, paths: &[PathBuf]) -> bool {
        paths.iter().enumerate().all(|(index, path)| {
            let Some(settings) = app.multi_edit.settings(path) else {
                return false;
            };
            app.current_photo_frame(path).is_some_and(|frame| {
                frame.artifact.settings.exposure == self.model.exposures()[index]
                    && app.reusable_photograph(path, frame, settings)
                    && self.frame_covers(index, frame, Self::visible(app, path))
            })
        })
    }

    fn observe_whole_settlement(&mut self, app: &App, paths: &[PathBuf]) {
        for (index, path) in paths.iter().take(self.native_photo_count).enumerate() {
            let frame = app.current_photo_frame(path).expect("checked native Whole");
            if frame.diagnostic_task.revision < self.minimum_revision {
                self.observation(
                    "whole-reused-observed",
                    index,
                    frame,
                    Self::visible(app, path),
                );
            }
            self.observation("whole-settled", index, frame, Self::visible(app, path));
            self.rows.last_mut().unwrap()["regional_observed"] = self.regional[index].into();
        }
    }

    pub fn advance(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        paths: &[PathBuf],
    ) -> Result<bool, String> {
        match self.step {
            Step::Start => {
                self.started = Instant::now();
                self.trace_origin_ns = foveonic::diagnostics::timestamp_at(self.started)
                    .map(|timestamp| timestamp.nanoseconds());
                self.begin("fit-smart", app);
                self.initial_quality = Some(app.preferences.preview_quality);
                for path in paths {
                    app.viewports.view_mut(Some(path)).reset();
                }
                app.preferences.preview_quality = PreviewMode::Smart;
                app.multi_edit.invalidate();
                app.submit(None);
                self.step = Step::Fit;
            }
            Step::Fit if !app.worker.busy() && self.whole_ready(app, paths) => {
                let mut demanded_scales = [0; 2];
                for (index, path) in paths.iter().enumerate() {
                    let frame = app
                        .current_photo_frame(path)
                        .ok_or("missing Smart Fit image")?;
                    let quality = app.preview_quality_for(path, PreviewMode::Smart);
                    let scale = crate::rendering::request::camera_preview_scale(
                        quality,
                        &frame.artifact.metadata,
                    )
                    .map_err(|error| error.to_string())?;
                    demanded_scales[index] = scale.divisor();
                    if !crate::rendering::quality::accepts_scale(
                        PreviewMode::Smart,
                        scale,
                        frame.artifact.settings.working_scale,
                    ) {
                        return Err(format!(
                            "Smart Fit used an insufficient scale for {}",
                            path.display()
                        ));
                    }
                }
                let mut percentages = [0.; 2];
                for (index, path) in paths.iter().take(self.native_photo_count).enumerate() {
                    let Some(percent) = app
                        .viewports
                        .view(Some(path))
                        .zoom_percent(ctx.pixels_per_point())
                    else {
                        return Ok(false);
                    };
                    percentages[index] = percent;
                }
                self.fit_scales = Some(demanded_scales);
                for (index, path) in paths.iter().enumerate() {
                    let frame = app.current_photo_frame(path).expect("checked Fit frame");
                    self.observation("fit-overview", index, frame, Self::visible(app, path));
                    self.rows.last_mut().unwrap()["demanded_scale"] = demanded_scales[index].into();
                }
                self.begin(self.zoom_action(), app);
                for (index, path) in paths.iter().take(self.native_photo_count).enumerate() {
                    app.viewports
                        .view_mut(Some(path))
                        .zoom_by(200. / percentages[index]);
                }
                self.step = Step::Zoom;
            }
            Step::Zoom => {
                self.observe(app, paths);
                if !app.worker.busy() && self.whole_ready(app, paths) {
                    self.observe_whole_settlement(app, paths);
                    self.start_edit(app, 0);
                }
            }
            Step::Edit(index) => {
                self.observe(app, paths);
                if self.first.iter().all(|seen| *seen)
                    && !app.worker.busy()
                    && self.whole_ready(app, paths)
                {
                    self.observe_whole_settlement(app, paths);
                    if index + 1 < EDITS.len() {
                        self.start_edit(app, index + 1);
                    } else {
                        self.begin("return-fit", app);
                        for path in paths.iter().take(self.native_photo_count) {
                            app.viewports.view_mut(Some(path)).reset();
                        }
                        app.submit(None);
                        self.step = Step::ReturnFit;
                    }
                }
            }
            Step::ReturnFit if !app.worker.busy() => {
                if !self.whole_ready(app, paths) {
                    return Ok(false);
                }
                self.rows.push(
                    serde_json::json!({"event":"fit-current","action":"return-fit",
                    "elapsed_ms":self.started.elapsed().as_secs_f64()*1000.,
                    "action_elapsed_ms":self.action_started.elapsed().as_secs_f64()*1000.}),
                );
                app.preferences.preview_quality =
                    self.initial_quality.ok_or("missing initial quality")?;
                app.submit(None);
                self.step = Step::Cleanup;
            }
            Step::Cleanup if !app.worker.busy() => {
                let quality = self.initial_quality.ok_or("missing initial quality")?;
                for (index, path) in paths.iter().enumerate() {
                    let Some(frame) = app.current_photo_frame(path) else {
                        return Ok(false);
                    };
                    let Some(settings) = app.multi_edit.settings(path) else {
                        return Ok(false);
                    };
                    let scale = crate::rendering::request::camera_preview_scale(
                        quality,
                        &frame.artifact.metadata,
                    )
                    .map_err(|error| error.to_string())?;
                    if frame.artifact.settings.exposure != self.model.exposures()[index]
                        || !app.reusable_photograph(path, frame, settings)
                        || frame.artifact.settings.working_scale != scale
                    {
                        return Ok(false);
                    }
                }
                self.step = Step::Done;
            }
            Step::Done => return Ok(true),
            _ => {}
        }
        Ok(false)
    }

    fn start_edit(&mut self, app: &mut App, index: usize) {
        self.begin(&format!("exposure-{index}"), app);
        self.rows.last_mut().unwrap()["delta"] = EDITS[index].into();
        self.model.adjust_exposure(EDITS[index]);
        super::scenarios::apply_exposure(app, f64::from(EDITS[index]));
        self.step = Step::Edit(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Rendition;

    #[test]
    fn native_demand_tracks_both_zoomed_photos_and_ends_at_fit() {
        assert!(native_demand(1, 0, Step::Zoom));
        assert!(!native_demand(1, 1, Step::Zoom));
        assert!(native_demand(2, 0, Step::Edit(0)));
        assert!(native_demand(2, 1, Step::Edit(0)));
        assert!(!native_demand(2, 1, Step::ReturnFit));
    }

    #[test]
    fn current_content_does_not_require_a_new_task_revision() {
        assert!(is_current_admission(
            true,
            Rendition::Current,
            0.375,
            0.375,
            true
        ));
        assert!(
            !is_current_admission(true, Rendition::Current, 0.375, 0.375, false),
            "an initial admission cannot satisfy an unresolved Fit demand"
        );
        assert!(!is_current_admission(
            false,
            Rendition::Current,
            0.375,
            0.375,
            true
        ));
        assert!(!is_current_admission(
            true,
            Rendition::Camera,
            0.375,
            0.375,
            true
        ));
        assert!(!is_current_admission(
            true,
            Rendition::Current,
            0.25,
            0.375,
            true
        ));
        let visible = Some([100, 100, 300, 300]);
        assert!(covers_demand(1, 1, true, [0, 0, 400, 400], visible));
        assert!(!covers_demand(2, 1, true, [0, 0, 400, 400], visible));
        assert!(!covers_demand(1, 1, true, [150, 0, 400, 400], visible));
        assert!(!covers_demand(1, 1, true, [0, 0, 400, 400], None));
        // An initially retained Full frame does not raise a Draft Fit demand.
        assert!(covers_demand(1, 4, false, [0, 0, 400, 400], None));
        assert!(covers_demand(4, 4, false, [0, 0, 400, 400], None));
        assert!(!covers_demand(8, 4, false, [0, 0, 400, 400], None));
    }
}
