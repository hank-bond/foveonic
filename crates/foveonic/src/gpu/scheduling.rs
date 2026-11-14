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
    /// Cancellable graph execution already completes every operation before
    /// advancing. Do not also block after each dispatch within that operation.
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
