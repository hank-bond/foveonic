//! Groups shared session executions behind opt-in submission windows.

use super::*;

pub enum RenderExecutionStart {
    Ready(RenderExecution),
    Missing,
    NeedsOriginal(NeedsOriginal),
    Superseded,
}

/// Owns completion of the current queue prefix, including racing UI work.
/// Drop drains an unfinished window before its caller can release live jobs.
pub struct SubmissionWindow<'a> {
    inner: crate::gpu::SubmissionWindow<'a>,
}

impl SubmissionWindow<'_> {
    pub fn complete(self) -> Result<bool> {
        self.inner.complete()
    }
}

impl Engine {
    pub fn submission_window(&self) -> SubmissionWindow<'_> {
        SubmissionWindow {
            inner: self.gpu.submission_window(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_render_execution(
        &self,
        working: &mut ProcessingWorkingSet,
        key: SessionKey,
        settings: &Settings,
        usage: SessionUse,
        cancel: &dyn Fn() -> bool,
        profile: &mut Profile,
        progress: &dyn Fn(RenderProgress),
    ) -> Result<RenderExecutionStart> {
        let mut execution = match RenderExecution::begin(
            self,
            working,
            key,
            settings,
            &["final"],
            usage,
            profile,
            cancel,
            Some(progress),
        ) {
            Ok(Begin::Missing) => return Ok(RenderExecutionStart::Missing),
            Ok(Begin::NeedsOriginal) => {
                return Ok(RenderExecutionStart::NeedsOriginal(
                    NeedsOriginal::EngineChanged,
                ))
            }
            Err(error) if key.kind == SessionKind::Reduced && error.is::<SourceRequired>() => {
                return Ok(RenderExecutionStart::NeedsOriginal(
                    NeedsOriginal::UpstreamChanged,
                ))
            }
            Err(error) if error.is::<Superseded>() => return Ok(RenderExecutionStart::Superseded),
            Err(error) => return Err(error),
            Ok(Begin::Ready(execution)) => execution,
        };
        let scope = execution.graph_scope_named(working, "graph.prepare_execution");
        if let Err(error) = execution.initialize(self, working, cancel, Some(progress)) {
            return match execution.finish_with_scope(self, working, Err(error), profile, scope)? {
                SessionRender::NeedsOriginal(reason) => {
                    Ok(RenderExecutionStart::NeedsOriginal(reason))
                }
                SessionRender::Superseded => Ok(RenderExecutionStart::Superseded),
                SessionRender::Rendered(_) => unreachable!("initialization failed"),
            };
        }
        // Speed exploration uses coarse safeguards. Do not imply that the
        // partial allocation audit has become a production admission contract.
        let job = execution.job.as_mut().unwrap();
        job.reservation = None;
        execution.submitted = self.gpu_activity().submissions != job.submissions_before;
        drop(scope);
        Ok(RenderExecutionStart::Ready(execution))
    }
}

impl RenderExecution {
    /// Encode at most one required operation, buffering completed-stage progress
    /// until the caller drains its enclosing submission window.
    pub fn advance(
        &mut self,
        engine: &Engine,
        working: &mut ProcessingWorkingSet,
        cancel: &dyn Fn() -> bool,
    ) -> Result<bool> {
        self.check_owner(engine)?;
        let before = engine.gpu_activity().submissions;
        let result = self.advance_graph(engine, working, cancel, None, Completion::Window);
        self.submitted |= engine.gpu_activity().submissions != before;
        result
    }

    /// Call after the enclosing window completes and before publishing progress
    /// or finishing this image. No additional GPU wait is introduced here.
    pub fn complete_window(
        &mut self,
        engine: &Engine,
        progress: &dyn Fn(RenderProgress),
    ) -> Result<()> {
        self.check_owner(engine)?;
        let job = self.job.as_mut().unwrap();
        diagnostics::with_context(job.context, || {
            if let Some(graph) = &mut job.graph {
                graph.complete_window(engine, Some(progress));
            }
        });
        job.submissions_before = engine.gpu_activity().submissions;
        self.submitted = false;
        Ok(())
    }
}

#[cfg(test)]
#[path = "experimental_test.rs"]
mod tests;
