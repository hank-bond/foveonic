//! Groups submitted graph steps into ticks. Each tick waits for the tick before it, so one
//! tick of work stays in flight and the GPU does not idle while the next step is prepared.

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
    /// The enclosing tick already bounds unfinished graph work.
    /// Avoid an additional wait after each dispatch within that boundary.
    pub(crate) fn background_stage(&self) -> BackgroundStage<'_> {
        let previous = crate::priority::context()
            .is_some()
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

/// The steps submitted since the tick began. Completing a tick waits for the
/// previous tick's work, which bounds unfinished work and the storage it holds
/// to one tick. Drop waits for everything before its caller can release live jobs.
pub(crate) struct Tick<'a> {
    gpu: &'a Gpu,
    background: BackgroundStage<'a>,
    initial_submissions: u64,
    completed: bool,
}
impl Gpu {
    pub(crate) fn tick(&self) -> Tick<'_> {
        Tick {
            gpu: self,
            background: self.background_stage(),
            initial_submissions: self.activity().submissions,
            completed: false,
        }
    }

    /// Whether a tick left work in flight that nothing has waited for since.
    pub(crate) fn has_work_in_flight(&self) -> bool {
        self.in_flight.lock().unwrap().is_some()
    }

    /// Wait for all submitted work, including the tick left in flight.
    pub(crate) fn settle(&self) -> crate::source::Result<()> {
        self.in_flight.lock().unwrap().take();
        self.wait_for_submitted_work()
    }
}
impl Tick<'_> {
    /// Returns how long the wait took. The `last` tick of a job waits for
    /// everything, so nothing outlives the job.
    pub(crate) fn complete(mut self, last: bool) -> crate::source::Result<std::time::Duration> {
        let started = std::time::Instant::now();
        if last {
            self.gpu.settle()?;
        } else if self.gpu.activity().submissions != self.initial_submissions {
            let submitted = super::completion::marker(&self.gpu.queue);
            let previous = self.gpu.in_flight.lock().unwrap().replace(submitted);
            if let Some(previous) = previous {
                super::completion::wait(&self.gpu.device, previous, || Ok(()))?;
            }
        }
        self.background.complete();
        self.completed = true;
        Ok(started.elapsed())
    }
}
impl Drop for Tick<'_> {
    fn drop(&mut self) {
        if !self.completed {
            // Errors cannot leave unfinished work behind a later request.
            let _ = self.gpu.settle();
            self.background.complete();
        }
    }
}
