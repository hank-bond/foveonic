//! Defers background submission waits to the executor's existing completed-stage boundary.

use super::Gpu;
use std::cell::Cell;

thread_local! {
    static STAGE: Cell<Option<u64>> = const { Cell::new(None) };
}

pub(super) fn stage_controls(gpu: &Gpu) -> bool {
    STAGE.with(|stage| stage.get() == Some(gpu.diagnostics.clock))
}

pub(crate) struct BackgroundStage<'a> {
    gpu: &'a Gpu,
    previous: Option<Option<u64>>,
}
impl Gpu {
    /// Operation and window completion already bound unfinished graph work.
    /// Avoid an additional wait after each dispatch within that boundary.
    pub(crate) fn background_stage(&self, managed_completion: bool) -> BackgroundStage<'_> {
        let previous = (managed_completion && crate::background::context().is_some())
            .then(|| STAGE.with(|stage| stage.replace(Some(self.diagnostics.clock))));
        BackgroundStage {
            gpu: self,
            previous,
        }
    }
}
impl BackgroundStage<'_> {
    /// The executor has completed the submitted operation through its usual wait.
    pub(crate) fn complete(&mut self) {
        if let Some(previous) = self.previous.take() {
            STAGE.with(|stage| stage.set(previous));
        }
    }
}
impl Drop for BackgroundStage<'_> {
    fn drop(&mut self) {
        if self.previous.is_some() {
            // An early error must still bound the unfinished background work
            // before the worker can serve a newer foreground request.
            let _scope = crate::diagnostics::scope_with(
                crate::diagnostics::Category::Gpu,
                "gpu.wait",
                |fields| {
                    fields
                        .str("reason", "background-stage-unwind")
                        .bool("blocking_poll", false);
                },
            );
            let _ = super::completion::submitted(&self.gpu.device, &self.gpu.queue);
            self.complete();
        }
    }
}

/// Owns completion for the opt-in worker experiment's bounded window.
#[cfg(feature = "wide-experiment")]
pub(crate) struct SubmissionWindow<'a> {
    gpu: &'a Gpu,
    background: BackgroundStage<'a>,
    initial_submissions: u64,
    drained: bool,
}
#[cfg(feature = "wide-experiment")]
impl Gpu {
    pub(crate) fn submission_window(&self) -> SubmissionWindow<'_> {
        SubmissionWindow {
            gpu: self,
            background: self.background_stage(true),
            initial_submissions: self.activity().submissions,
            drained: false,
        }
    }
}
#[cfg(feature = "wide-experiment")]
impl SubmissionWindow<'_> {
    pub(crate) fn complete(mut self) -> crate::source::Result<bool> {
        let submitted = self.gpu.activity().submissions != self.initial_submissions;
        if submitted {
            self.gpu.wait_for_submitted_work()?;
        }
        self.background.complete();
        self.drained = true;
        Ok(submitted)
    }
}
#[cfg(feature = "wide-experiment")]
impl Drop for SubmissionWindow<'_> {
    fn drop(&mut self) {
        if !self.drained {
            // Errors cannot leave an unfinished window behind a later request.
            if self.gpu.activity().submissions != self.initial_submissions {
                let _ = self.gpu.wait_for_submitted_work();
            }
            self.background.complete();
        }
    }
}
