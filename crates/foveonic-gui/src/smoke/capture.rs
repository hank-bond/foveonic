//! Saves existing smoke checks through the shared capture controller without changing their actions.

use super::*;
use foveonic::diagnostics::{self as trace, Category};

#[derive(Default)]
pub(super) struct Capture {
    marked_step: Option<usize>,
    close_requested: bool,
    recorded: bool,
    saving_started: bool,
}
impl Capture {
    pub(super) fn request_close(&mut self, ctx: &egui::Context) {
        self.close_requested = true;
        ctx.request_repaint();
    }
}
impl Smoke {
    fn diagnostic_suite(&self) -> &'static str {
        [
            ("white-balance", WHITE_BALANCE_TARGETS),
            ("status", STATUS_TARGETS),
            ("rotate", rotate::TARGETS),
            ("crop", CROP_TARGETS),
            ("exposure-auto", EXPOSURE_AUTO_TARGETS),
            ("dehaze", DEHAZE_TARGETS),
            ("base-response", BASE_RESPONSE_TARGETS),
            ("advanced-color", ADVANCED_COLOR_TARGETS),
            ("direct-color", DIRECT_COLOR_TARGETS),
            ("perceptual", PERCEPTUAL_TARGETS),
            ("comparison", COMPARISON_TARGETS),
            ("multi-edit", multi_edit::TARGETS),
            ("workflow", WORKFLOW_TARGETS),
            ("display", DISPLAY_COLOR_TARGETS),
            ("browser", BROWSER_TARGETS),
            ("import", IMPORT_TARGETS),
            ("smart", SMART_TARGETS),
            ("fast", FAST_PREVIEW_TARGETS),
            ("tiger", TIGER_TARGETS),
            ("screenshot", screenshot::TARGETS),
            ("scopes", VIEWPORT_SCOPE_TARGETS),
            ("inspection", DIFFERENCE_TARGETS),
            ("detail", DETAIL_TARGETS),
            ("repair", REPAIR_TARGETS),
            ("despeckle", SPECKLE_TARGETS),
            ("editor", EDITOR_TARGETS),
            ("noise", NOISE_TARGETS),
        ]
        .into_iter()
        .find_map(|(name, targets)| (targets == self.targets).then_some(name))
        .unwrap_or("default")
    }
    pub(super) fn diagnostic_begin(&mut self) {
        if !trace::enabled()
            || self.index >= self.targets.len()
            || self.capture.marked_step == Some(self.index)
        {
            return;
        }
        self.capture.marked_step = Some(self.index);
        self.diagnostic_marker("begin", None, false);
    }
    pub(super) fn diagnostic_checked(
        &self,
        frame: Option<&crate::rendering::worker::RenderResult>,
        worker_busy: bool,
    ) {
        self.diagnostic_marker("checked", frame, worker_busy);
    }
    fn diagnostic_marker(
        &self,
        phase: &str,
        frame: Option<&crate::rendering::worker::RenderResult>,
        worker_busy: bool,
    ) {
        trace::event(Category::Scenario, "smoke.step", |fields| {
            fields.str("suite", self.diagnostic_suite());
            fields.str("target", self.targets[self.index]);
            fields.str("phase", phase);
            fields.str("coverage", "mixed-existing-smoke");
            fields.str(
                "mode",
                match trace::active_mode() {
                    trace::Mode::Off => "off",
                    trace::Mode::Wall => "wall",
                    trace::Mode::Gpu => "gpu",
                },
            );
            fields.u64("step", self.index as u64 + 1);
            fields.u64("step_count", self.targets.len() as u64);
            if phase == "begin" && self.targets[self.index] == "cache-reopen" {
                fields.u64("expect_original_reads", 0);
                fields.u64("expect_local_reads", 0);
                fields.u64("expect_photographic_dispatches", 0);
            }
            if phase == "checked" {
                fields.bool("worker_busy", worker_busy);
                if let Some(frame) = frame {
                    fields.u64("task", frame.diagnostic_task.id);
                    fields.u64("revision", frame.diagnostic_task.revision);
                    fields.u64("presentation_revision", frame.request.id.0);
                    fields.u64(
                        "scale_divisor",
                        frame.artifact.settings.working_scale.divisor() as u64,
                    );
                    fields.str("artifact", &frame.request.output);
                    if let Some(identity) = frame.diagnostic_identity(false) {
                        fields.u64("photo", identity.context.photo);
                    }
                }
            }
        });
    }
    pub(super) fn diagnostic_scope(&self) -> trace::Scope {
        trace::scope_with(Category::Scenario, "smoke.tick", |fields| {
            fields.str("suite", self.diagnostic_suite());
            fields.u64("step", self.index as u64 + 1);
            if let Some(target) = self.targets.get(self.index) {
                fields.str("target", target);
            }
        })
    }
}
impl App {
    pub(super) fn smoke_capture_tick(&mut self, ctx: &egui::Context) -> bool {
        if self
            .smoke
            .as_ref()
            .is_none_or(|smoke| smoke.scenario.is_some())
        {
            return false;
        }
        let mut smoke = self.smoke.take().unwrap();
        let hold = self.poll_smoke_capture(ctx, &mut smoke);
        self.smoke = Some(smoke);
        hold
    }
    fn poll_smoke_capture(&mut self, ctx: &egui::Context, smoke: &mut Smoke) -> bool {
        if let Some(error) = self.diagnostics.error() {
            panic!("Smoke diagnostic capture failed: {error}");
        }
        if self.diagnostics.starting() {
            ctx.request_repaint_after(std::time::Duration::from_millis(20));
            return true;
        }
        if self.diagnostics.active() && !smoke.capture.recorded {
            smoke.capture.recorded = true;
            let mut state = self.diagnostic_state();
            state["smoke"] = serde_json::json!({"suite":smoke.diagnostic_suite(),
                "targets":smoke.targets,"coverage":"existing assertions and mixed direct-command/egui-input actions",
                "measurement":"includes screenshot readback and artifact encoding; not a latency baseline"});
            self.diagnostics.set_start_state(state);
        }
        if smoke.capture.close_requested {
            if self.diagnostics.active() && !smoke.capture.saving_started {
                trace::event(Category::Scenario, "smoke.completed", |fields| {
                    fields.str("suite", smoke.diagnostic_suite());
                    fields.u64("checked_steps", smoke.index as u64);
                    fields.u64("step_count", smoke.targets.len() as u64);
                });
                self.diagnostics.set_end_state(self.diagnostic_state());
                self.diagnostics
                    .stop_to(smoke.directory.join("capture"))
                    .expect("cannot save smoke diagnostic capture");
                smoke.capture.saving_started = true;
            }
            if !self.diagnostics.saving() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(20));
            }
            return true;
        }
        smoke.diagnostic_begin();
        false
    }
}
