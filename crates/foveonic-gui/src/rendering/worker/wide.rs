//! Interleaves opt-in warm photo executions while ordinary worker code owns delivery.

#[path = "admission.rs"]
mod admission;

use super::{
    interruption, presentation::Presentation, processing::Photos, Interruption, Pending, Ready,
};
use crate::rendering::{
    coordinator::{Cursor, Task, TaskPriority, WorkPlan},
    request::Purpose,
};
use foveonic::{
    decode_cache::Cache,
    profile::Profile,
    render::{
        Engine, RenderExecution, RenderExecutionStart, SessionKind, SessionRender, SessionUse,
    },
    source::{ProcessingFamily, Result},
};
use std::{
    cell::Cell,
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

type Slot = Arc<(Mutex<Pending>, Condvar)>;

struct Job {
    ready: Ready,
    execution: RenderExecution,
    profile: Profile,
    workspace: Option<admission::WorkspaceHint>,
    outcome: Option<Result<()>>,
}

pub(super) enum Start {
    Admitted,
    Deferred,
    Serial,
}

pub(super) enum Next {
    Ready(Box<Ready>),
    Serial(Arc<Task>),
}

pub(super) struct Wide {
    width: Option<usize>,
    jobs: Vec<Job>,
    ready: VecDeque<Ready>,
    serial: VecDeque<Arc<Task>>,
}

const MAX_CONCURRENT_PHOTOS: usize = 8;

pub(super) fn initial_width() -> Option<usize> {
    #[cfg(test)]
    if let Ok(value) = std::env::var("FOVEONIC_WIDE_RENDER_WIDTH") {
        return match value.as_str() {
            "serial" | "0" => None,
            _ => match value.parse::<usize>() {
                Ok(width @ (1 | 2 | 4 | 8)) => Some(width),
                _ => panic!("FOVEONIC_WIDE_RENDER_WIDTH requires serial, 0, 1, 2, 4 or 8"),
            },
        };
    }
    Some(MAX_CONCURRENT_PHOTOS)
}

fn at_priority<T>(
    cache: &Cache,
    priority: TaskPriority,
    work: impl FnOnce(&dyn Fn() -> bool) -> T,
) -> Option<T> {
    match priority {
        TaskPriority::Viewport => {
            let _activity = cache.foreground();
            Some(work(&|| false))
        }
        TaskPriority::Auxiliary => {
            let _activity = cache.auxiliary_demand();
            cache.auxiliary_execution(work)
        }
        TaskPriority::Thumbnail => {
            let _activity = cache.thumbnail_demand();
            cache.thumbnail_execution(work)
        }
        TaskPriority::Idle => None,
    }
}

impl Wide {
    pub fn new() -> Self {
        Self {
            width: None,
            jobs: Vec::new(),
            ready: VecDeque::new(),
            serial: VecDeque::new(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.width.is_some()
    }

    pub fn is_idle(&self) -> bool {
        self.jobs.is_empty() && self.ready.is_empty() && self.serial.is_empty()
    }

    fn inputs(
        task: &Task,
        photos: &Photos,
        cache: &Cache,
        presented: &Presentation,
    ) -> Option<(super::processing::Loaded, SessionKind)> {
        let request = &task.request;
        let kind = match task.stamp.target.purpose {
            Purpose::Current | Purpose::Preview | Purpose::Camera | Purpose::Defaults
                if request.uses_reduced_session()
                    && !request.detail
                    && request.settings.preview_region.is_none() =>
            {
                SessionKind::Reduced
            }
            Purpose::Detail | Purpose::Camera | Purpose::Defaults
                if request.detail
                    && !request.preview_scale.is_reduced()
                    && request.settings.preview_region.is_some() =>
            {
                SessionKind::Native
            }
            _ => return None,
        };
        if task.speculative
            || request.output != "final"
            || request.comparison
            || request.framing_preview
            || request.export.is_some()
            || (request.settings.output_size.is_some() && !request.auxiliary_preview)
            || request.settings.color_range_preview.is_some()
            || crate::timing::mode().gpu()
        {
            return None;
        }
        let mut seeded = Presentation::default();
        let presentation = if let Some(seed) = &task.seed {
            seeded.seed(seed);
            &seeded
        } else {
            presented
        };
        if presentation.prepare(request, cache).ok()?.is_some()
            || presentation
                .quality_for_view_change(request, cache)
                .is_some()
        {
            return None;
        }
        let loaded = photos.warm(&request.path, cache, kind)?;
        if loaded.metadata.processing_family().ok()? != ProcessingFamily::Merrill
            || task
                .origin
                .as_ref()
                .is_some_and(|origin| origin != loaded.origin())
        {
            return None;
        }
        Some((loaded, kind))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &mut self,
        task: Arc<Task>,
        engine: &mut Engine,
        photos: &mut Photos,
        cursor: &mut Cursor,
        slot: &Slot,
        cache: &Cache,
        presented: &Presentation,
        progress: &crate::progress::Tracker,
        budget: &super::MemoryBudget,
        pending_images: &mut Vec<crate::memory::PendingImage>,
    ) -> Start {
        if !self.enabled() || task.speculative {
            return Start::Serial;
        }
        let priority = slot.0.lock().unwrap().plan.priority(&task);
        let stamp = task.stamp.clone();
        let result = at_priority(cache, priority, |external_cancel| {
            let result = self.start_ready(
                task,
                engine,
                photos,
                cursor,
                slot,
                cache,
                presented,
                progress,
                budget,
                pending_images,
                external_cancel,
            );
            if external_cancel() {
                self.retire(engine, photos, cursor);
                cursor.yield_task(&stamp);
                Start::Deferred
            } else {
                result
            }
        });
        result.unwrap_or_else(|| {
            cursor.yield_task(&stamp);
            Start::Deferred
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn start_ready(
        &mut self,
        task: Arc<Task>,
        engine: &mut Engine,
        photos: &mut Photos,
        cursor: &mut Cursor,
        slot: &Slot,
        cache: &Cache,
        presented: &Presentation,
        progress: &crate::progress::Tracker,
        budget: &super::MemoryBudget,
        pending_images: &mut Vec<crate::memory::PendingImage>,
        external_cancel: &dyn Fn() -> bool,
    ) -> Start {
        if !self.enabled()
            || self
                .jobs
                .iter()
                .any(|job| job.ready.task.request.path == task.request.path)
        {
            return Start::Serial;
        }
        // A blocked pick/export can still let ordinary ready images run. Keep
        // those images serial until that priority work retires, rather than
        // repeatedly admitting and immediately yielding the same warm job.
        {
            let state = slot.0.lock().unwrap();
            if state.stop || state.clear || state.export.is_some() || state.plan.pick.is_some() {
                return Start::Serial;
            }
        }
        let Some((loaded, kind)) = Self::inputs(&task, photos, cache, presented) else {
            return Start::Serial;
        };
        if !cursor.claim(&task.stamp) {
            return Start::Serial;
        }
        let started = Instant::now();
        let reporter = crate::progress::Reporter::new(progress.clone(), task.request.revision);
        reporter.loaded(foveonic::source::LoadStage::Decoded);
        engine.set_noise_calibration(task.request.calibration.clone());
        let _context = crate::timing::enabled()
            .then(|| foveonic::diagnostics::enter(crate::timing::context(&task.stamp)));
        let Ok((settings, defaults_errors)) =
            super::resolve_settings(&task.request, &loaded.metadata, None)
        else {
            cursor.release(&task.stamp);
            return Start::Serial;
        };
        let supported = match kind {
            SessionKind::Reduced => {
                settings.working_scale.is_reduced() && settings.preview_region.is_none()
            }
            SessionKind::Native => {
                !settings.working_scale.is_reduced() && settings.preview_region.is_some()
            }
        };
        let Some(edit) = photos
            .warm_changes(&loaded, &settings, kind)
            .filter(|_| supported)
        else {
            cursor.release(&task.stamp);
            return Start::Serial;
        };
        let workspace = admission::workspace(&settings, &loaded.metadata);
        if !self.admit_extra(
            engine,
            photos,
            presented,
            budget,
            pending_images,
            slot,
            workspace,
        ) {
            cursor.yield_task(&task.stamp);
            return Start::Deferred;
        }
        // Recheck source validity before acquiring the execution claim.
        if photos.warm(&task.request.path, cache, kind).is_none() {
            return if self.jobs.is_empty() {
                cursor.release(&task.stamp);
                Start::Serial
            } else {
                cursor.yield_task(&task.stamp);
                Start::Deferred
            };
        }
        let yielded = Cell::new(false);
        let cancel = || {
            let interruption = interruption(
                cursor,
                &slot.0.lock().unwrap(),
                &task,
                &settings,
                &loaded.metadata,
                &edit,
            );
            if external_cancel() || interruption == Interruption::Yield {
                yielded.set(true);
                true
            } else {
                interruption == Interruption::Cancel
            }
        };
        let mut profile = Profile::new(crate::timing::mode());
        #[cfg(test)]
        let activity_before = engine.gpu_activity();
        let begin = engine.begin_render_execution(
            photos.experimental_working_set(),
            loaded.session_key(kind),
            &settings,
            SessionUse::Foreground,
            &cancel,
            &mut profile,
            &|stage| reporter.rendered(stage),
        );
        if yielded.get() || external_cancel() {
            if let Ok(RenderExecutionStart::Ready(execution)) = begin {
                let _ = execution.finish(
                    engine,
                    photos.experimental_working_set(),
                    Err("preview execution yielded before advancement".into()),
                    &mut profile,
                );
            }
            cursor.yield_task(&task.stamp);
            return Start::Deferred;
        }
        let ready = Ready {
            task: task.clone(),
            metadata: loaded.metadata.clone(),
            origin: loaded.origin().clone(),
            settings,
            defaults_errors,
            edit,
            started,
            input: None,
            display: None,
            #[cfg(test)]
            dispatches: engine.gpu_activity().since(activity_before).dispatches,
            artifacts: Ok(None),
        };
        match begin {
            Ok(RenderExecutionStart::Ready(execution)) => {
                crate::timing::task(
                    &task.stamp,
                    "execution_started",
                    || serde_json::json!({"wide_experiment":true,"requested_width":self.width}),
                );
                crate::timing::worker_started(task.request.revision, task.queued_at, started);
                self.jobs.push(Job {
                    ready,
                    execution,
                    profile,
                    workspace,
                    outcome: None,
                });
                #[cfg(test)]
                super::CHECKS.with(|checks| {
                    if let Some(checks) = checks.borrow().as_ref() {
                        use std::sync::atomic::Ordering::Relaxed;
                        checks.wide_admitted.fetch_add(1, Relaxed);
                        checks
                            .wide_max_live
                            .fetch_max(self.jobs.len() as u64, Relaxed);
                    }
                });
                Start::Admitted
            }
            Ok(RenderExecutionStart::Missing | RenderExecutionStart::NeedsOriginal(_)) => {
                cursor.release(&task.stamp);
                Start::Serial
            }
            Ok(RenderExecutionStart::Superseded) => {
                self.ready.push_back(ready);
                Start::Admitted
            }
            Err(error) => {
                self.ready.push_back(Ready {
                    artifacts: Err(error),
                    ..ready
                });
                Start::Admitted
            }
        }
    }

    /// Called only outside a submission window, before exclusive numerical work.
    pub fn retire(&mut self, engine: &Engine, photos: &mut Photos, cursor: &mut Cursor) {
        self.retire_jobs(0, engine, photos, cursor);
        for ready in self.ready.drain(..) {
            cursor.yield_task(&ready.task.stamp);
        }
        for task in self.serial.drain(..) {
            cursor.yield_task(&task.stamp);
        }
    }

    fn retire_jobs(
        &mut self,
        keep: usize,
        engine: &Engine,
        photos: &mut Photos,
        cursor: &mut Cursor,
    ) {
        for mut job in self.jobs.drain(keep..) {
            let _ = job.execution.finish(
                engine,
                photos.experimental_working_set(),
                Err("experimental execution retired before serial work".into()),
                &mut job.profile,
            );
            cursor.yield_task(&job.ready.task.stamp);
        }
    }

    fn inventory(
        &self,
        presented: &Presentation,
        budget: &super::MemoryBudget,
        pending_images: &mut Vec<crate::memory::PendingImage>,
        slot: &Slot,
    ) -> crate::memory::Inventory {
        let mut inventory = super::memory_inventory(presented, budget, pending_images, slot);
        for ready in &self.ready {
            if let Ok(Some(artifacts)) = &ready.artifacts {
                for artifact in artifacts {
                    inventory.metadata(&artifact.metadata);
                    if let foveonic::inspection::Content::Image(image) = &artifact.content {
                        inventory.image(image);
                    }
                }
            }
        }
        inventory
    }

    fn native_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|job| !job.ready.settings.working_scale.is_reduced())
            .count()
            + self
                .ready
                .iter()
                .filter(|ready| !ready.settings.working_scale.is_reduced())
                .count()
    }

    fn headroom(&self) -> Option<u64> {
        self.jobs
            .iter()
            .try_fold(0_u64, |total, job| total.checked_add(job.workspace?.bytes))
    }

    fn resources(
        &self,
        engine: &Engine,
        photos: &mut Photos,
        inventory: &crate::memory::Inventory,
    ) -> foveonic::render::MemoryResources {
        let executions = self
            .jobs
            .iter()
            .map(|job| &job.execution)
            .collect::<Vec<_>>();
        engine.execution_memory_resources(
            photos.experimental_working_set(),
            inventory.reservation(),
            &executions,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn admit_extra(
        &self,
        engine: &Engine,
        photos: &mut Photos,
        presented: &Presentation,
        budget: &super::MemoryBudget,
        pending_images: &mut Vec<crate::memory::PendingImage>,
        slot: &Slot,
        candidate: Option<admission::WorkspaceHint>,
    ) -> bool {
        if self.jobs.is_empty() && self.ready.is_empty() {
            return true;
        }
        let Some(candidate) = candidate else {
            return false;
        };
        let Some(headroom) = self
            .headroom()
            .and_then(|bytes| bytes.checked_add(candidate.bytes))
        else {
            return false;
        };
        let native = self.native_count() + usize::from(candidate.native);
        if !admission::fits(budget.limit, 0, 0, native) {
            return false;
        }
        let inventory = self.inventory(presented, budget, pending_images, slot);
        // Concurrency uses spare capacity. Retained inputs are never trimmed
        // merely to make another execution fit.
        admission::fits(
            budget.limit,
            self.resources(engine, photos, &inventory).total_bytes,
            headroom,
            native,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn reconsider(
        &mut self,
        engine: &Engine,
        photos: &mut Photos,
        cursor: &mut Cursor,
        presented: &Presentation,
        budget: &super::MemoryBudget,
        pending_images: &mut Vec<crate::memory::PendingImage>,
        slot: &Slot,
    ) {
        let maximum = self.width.unwrap_or(0);
        let minimum = usize::from(self.ready.is_empty() && maximum > 0);
        while self.jobs.len() > minimum {
            let inventory = self.inventory(presented, budget, pending_images, slot);
            let fits = self.headroom().is_some_and(|headroom| {
                admission::fits(
                    budget.limit,
                    self.resources(engine, photos, &inventory).total_bytes,
                    headroom,
                    self.native_count(),
                )
            });
            if self.jobs.len() + self.ready.len() <= maximum && fits {
                break;
            }
            self.retire_jobs(self.jobs.len() - 1, engine, photos, cursor);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn next(
        &mut self,
        engine: &mut Engine,
        photos: &mut Photos,
        cursor: &mut Cursor,
        slot: &Slot,
        cache: &Cache,
        presented: &Presentation,
        progress: &crate::progress::Tracker,
        sources: &crate::rendering::source_loader::Handle,
        budget: &mut super::MemoryBudget,
        pending_images: &mut Vec<crate::memory::PendingImage>,
    ) -> Option<Next> {
        let priority = {
            let state = slot.0.lock().unwrap();
            self.priority(&state.plan)
        };
        let Some(priority) = priority else {
            self.width = slot.0.lock().unwrap().render_width;
            return None;
        };
        at_priority(cache, priority, |external_cancel| {
            self.next_ready(
                engine,
                photos,
                cursor,
                slot,
                cache,
                presented,
                progress,
                sources,
                budget,
                pending_images,
                external_cancel,
            )
        })
        .unwrap_or_else(|| {
            self.retire(engine, photos, cursor);
            None
        })
    }

    fn priority(&self, plan: &WorkPlan) -> Option<TaskPriority> {
        self.jobs
            .first()
            .map(|job| job.ready.task.as_ref())
            .or_else(|| self.ready.front().map(|ready| ready.task.as_ref()))
            .or_else(|| self.serial.front().map(AsRef::as_ref))
            .map(|task| plan.priority(task))
    }

    #[allow(clippy::too_many_arguments)]
    fn next_ready(
        &mut self,
        engine: &mut Engine,
        photos: &mut Photos,
        cursor: &mut Cursor,
        slot: &Slot,
        cache: &Cache,
        presented: &Presentation,
        progress: &crate::progress::Tracker,
        sources: &crate::rendering::source_loader::Handle,
        budget: &mut super::MemoryBudget,
        pending_images: &mut Vec<crate::memory::PendingImage>,
        external_cancel: &dyn Fn() -> bool,
    ) -> Option<Next> {
        loop {
            let (width, memory) = {
                let mut state = slot.0.lock().unwrap();
                (state.render_width, state.memory.take())
            };
            if let Some(next) = memory {
                *budget = next;
            }
            self.width = width;
            // Retained frontend data and live storage can grow without a cache
            // setting change. Reduce peers at completed windows before result
            // delivery can run ordinary cache trimming.
            self.reconsider(
                engine,
                photos,
                cursor,
                presented,
                budget,
                pending_images,
                slot,
            );
            let (controls_pending, plan) = {
                let state = slot.0.lock().unwrap();
                (
                    state.stop
                        || state.clear
                        || state.export.is_some()
                        || state.plan.pick.is_some(),
                    state.plan.clone(),
                )
            };
            let preempt = external_cancel()
                || controls_pending
                || self
                    .jobs
                    .iter()
                    .any(|job| cursor.should_yield(&plan, &job.ready.task, false))
                || self
                    .ready
                    .iter()
                    .any(|ready| cursor.should_yield(&plan, &ready.task, false));
            if preempt {
                self.retire(engine, photos, cursor);
                return None;
            }
            if let Some(ready) = self.ready.pop_front() {
                return Some(Next::Ready(Box::new(ready)));
            }
            if let Some(task) = self.serial.pop_front() {
                return Some(Next::Serial(task));
            }
            // Presentation reuse and source deferral need no exclusive graph
            // execution. Let the worker attempt them between completed windows.
            if let Some(task) = cursor.next_ready_matching(
                &plan,
                |task| sources.can_retry(&task.request.path, &slot.0.lock().unwrap()),
                |task| task.viewport() && Self::inputs(task, photos, cache, presented).is_none(),
            ) {
                return Some(Next::Serial(task));
            }
            if self.jobs.is_empty() {
                return None;
            }
            while self.jobs.len() + self.ready.len() < self.width.unwrap_or(0) {
                let plan = {
                    let state = slot.0.lock().unwrap();
                    if state.stop
                        || state.clear
                        || state.export.is_some()
                        || state.plan.pick.is_some()
                    {
                        break;
                    }
                    state.plan.clone()
                };
                let priority = self.priority(&plan);
                let next = cursor.next_ready_matching(
                    &plan,
                    |task| sources.can_retry(&task.request.path, &slot.0.lock().unwrap()),
                    |task| {
                        Some(plan.priority(task)) == priority
                            && !self
                                .jobs
                                .iter()
                                .any(|job| job.ready.task.request.path == task.request.path)
                            && Self::inputs(task, photos, cache, presented).is_some()
                    },
                );
                let Some(task) = next else {
                    break;
                };
                match self.start(
                    task.clone(),
                    engine,
                    photos,
                    cursor,
                    slot,
                    cache,
                    presented,
                    progress,
                    budget,
                    pending_images,
                ) {
                    Start::Admitted => {}
                    Start::Deferred => break,
                    Start::Serial => {
                        self.serial.push_back(task);
                        break;
                    }
                }
            }
            if external_cancel() {
                self.retire(engine, photos, cursor);
                return None;
            }
            let yielded = Cell::new(false);
            let window = engine.submission_window();
            #[cfg(test)]
            let mut submitted = Vec::new();
            for job in &mut self.jobs {
                #[cfg(test)]
                let activity_before = engine.gpu_activity();
                let ready = &job.ready;
                let _context = crate::timing::enabled().then(|| {
                    foveonic::diagnostics::enter(crate::timing::context(&ready.task.stamp))
                });
                let cancel = || {
                    let interruption = interruption(
                        cursor,
                        &slot.0.lock().unwrap(),
                        &ready.task,
                        &ready.settings,
                        &ready.metadata,
                        &ready.edit,
                    );
                    if external_cancel() || interruption == Interruption::Yield {
                        yielded.set(true);
                        true
                    } else {
                        interruption == Interruption::Cancel
                    }
                };
                match job
                    .execution
                    .advance(engine, photos.experimental_working_set(), &cancel)
                {
                    Ok(true) => {}
                    Ok(false) => job.outcome = Some(Ok(())),
                    Err(error) => job.outcome = Some(Err(error)),
                }
                #[cfg(test)]
                {
                    let activity = engine.gpu_activity().since(activity_before);
                    if activity.submissions > 0 {
                        submitted.push(job.ready.task.stamp.clone());
                    }
                    job.ready.dispatches += activity.dispatches;
                }
            }
            let completed = window.complete();
            #[cfg(test)]
            if matches!(completed, Ok(true)) {
                super::CHECKS.with(|checks| {
                    if let Some(checks) = checks.borrow().as_ref() {
                        use std::sync::atomic::Ordering::Relaxed;
                        checks.wide_windows.fetch_add(1, Relaxed);
                        checks
                            .wide_max_submitted
                            .fetch_max(submitted.len() as u64, Relaxed);
                        if !submitted.is_empty() {
                            checks
                                .wide_submissions
                                .lock()
                                .unwrap()
                                .push(submitted.clone());
                        }
                        if submitted.len() >= 2 {
                            if let Some((reached, release)) =
                                checks.wide_window_hold.lock().unwrap().take()
                            {
                                let _ = reached.send(());
                                let _ = release.recv();
                            }
                        }
                    }
                });
            }
            for job in &mut self.jobs {
                let reporter = crate::progress::Reporter::new(
                    progress.clone(),
                    job.ready.task.request.revision,
                );
                match &completed {
                    Ok(_) => {
                        if let Err(error) = job
                            .execution
                            .complete_window(engine, &|stage| reporter.rendered(stage))
                        {
                            job.outcome = Some(Err(error));
                        }
                    }
                    Err(error) => job.outcome = Some(Err(error.to_string().into())),
                }
            }
            if yielded.get() || external_cancel() {
                self.retire(engine, photos, cursor);
                return None;
            }
            let mut index = 0;
            while index < self.jobs.len() {
                if self.jobs[index].outcome.is_none() {
                    index += 1;
                    continue;
                }
                let mut job = self.jobs.remove(index);
                #[cfg(test)]
                let activity_before = engine.gpu_activity();
                let result = job.execution.finish(
                    engine,
                    photos.experimental_working_set(),
                    job.outcome.take().unwrap(),
                    &mut job.profile,
                );
                #[cfg(test)]
                {
                    job.ready.dispatches += engine.gpu_activity().since(activity_before).dispatches;
                }
                match result {
                    Ok(SessionRender::Rendered(artifacts)) => {
                        job.ready.artifacts = Ok(Some(artifacts))
                    }
                    Ok(SessionRender::Superseded) => {}
                    Ok(SessionRender::NeedsOriginal(_)) => {
                        cursor.release(&job.ready.task.stamp);
                        self.serial.push_back(job.ready.task);
                        continue;
                    }
                    Err(error) => job.ready.artifacts = Err(error),
                }
                self.ready.push_back(job.ready);
            }
        }
    }
}
