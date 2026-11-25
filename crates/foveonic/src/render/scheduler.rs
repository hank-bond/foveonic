//! Owns the job queue, the working set and admission, and advances admitted jobs together one step per tick.

use super::{
    image_cache::ImageCache,
    queue::{Queue, Queued},
    Engine, MemoryReservation, MemoryTrim, PhotoKey, PhotoRender, Purpose, RenderExecution,
    RenderExecutionStart, RenderProgress, Settings, WorkingSet,
};
use crate::{
    inspection::Artifact, interactive::Superseded, priority::Priority, profile::Profile,
    source::Result,
};

/// Work the scheduler orders. The host defines what a job means.
pub trait Job {
    type Target: PartialEq + Clone;
    fn target(&self) -> &Self::Target;
    fn purpose(&self) -> Purpose;
    /// Exclusive jobs, such as color picks and exports, start only when no job is admitted.
    fn exclusive(&self) -> bool {
        false
    }
    /// The photo a job renders. Only one admitted job uses a photo at a time.
    fn photo(&self) -> Option<&std::path::Path> {
        None
    }
    /// The job's place within its tier. Lower ranks start first, and equal ranks
    /// start in submission order.
    fn rank(&self) -> usize {
        0
    }
}

/// What starting a job needs next.
pub enum Start {
    /// The host completed the job itself, for example by reusing a visible image.
    Finished,
    /// The job waits for an input the host is acquiring until `Host::ready` reports it.
    Waiting,
    /// The job cannot start without changing the working set beside other jobs, or
    /// cannot get its input at all. It retries after a job finishes or retires,
    /// memory is freed, or the target changes.
    Deferred,
    Render(Box<Render>),
}

/// A render of one working photo.
pub struct Render {
    pub key: PhotoKey,
    pub settings: Settings,
    pub names: Vec<String>,
    pub profile: Profile,
}

pub enum Outcome {
    Rendered(Vec<Artifact>),
    Cancelled,
    /// The reduced working photo's sensor data is stale and needs the file again.
    NeedsOriginal,
    Failed(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Interruption {
    Continue,
    /// The job is obsolete.
    Cancel,
    /// The job is still wanted but gives way to higher-priority work.
    Yield,
}

/// The application side of every job: preparing inputs, deciding whether it is
/// still wanted, and delivering what it produced.
pub trait Host<J: Job> {
    /// Whether a job can start now. A `waiting` job returned `Start::Waiting` and
    /// must not be ready again until its input has changed.
    fn ready(&self, working: &WorkingSet, job: &J, waiting: bool) -> bool;
    /// `alone` is true when no other job is admitted. Otherwise the host must not
    /// load or prepare a working photo and returns `Start::Deferred` instead.
    fn start(
        &mut self,
        engine: &mut Engine,
        working: &mut WorkingSet,
        job: &mut J,
        alone: bool,
    ) -> Start;
    fn interruption(&self, job: &J) -> Interruption;
    fn progress(&self, job: &J, progress: RenderProgress);
    /// A started job returned to the queue to give way to other work.
    fn yielded(&mut self, job: &J);
    fn finish(&mut self, engine: &mut Engine, job: J, outcome: Outcome);
    /// Memory the host holds outside the working set, counted against the same target.
    fn retained(&mut self) -> MemoryReservation;
}

struct Admitted<J> {
    job: J,
    /// When the target was first submitted.
    order: u64,
    execution: RenderExecution,
    profile: Profile,
    outcome: Option<Result<()>>,
}

pub struct Scheduler<J: Job> {
    engine: Engine,
    working: WorkingSet,
    target_bytes: u64,
    width_cap: Option<usize>,
    queue: Queue<J>,
    admitted: Vec<Admitted<J>>,
    image_cache: ImageCache,
    widest: usize,
}

impl<J: Job> Scheduler<J> {
    pub fn new(engine: Engine, target_bytes: u64, image_cache: ImageCache) -> Self {
        Self {
            engine,
            working: WorkingSet::new(),
            target_bytes,
            width_cap: None,
            queue: Queue::new(),
            admitted: Vec::new(),
            image_cache,
            widest: 0,
        }
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn engine_and_working(&mut self) -> (&mut Engine, &mut WorkingSet) {
        (&mut self.engine, &mut self.working)
    }

    /// Host memory plus cached images.
    pub fn with_cached_images(&self, mut retained: MemoryReservation) -> MemoryReservation {
        retained.merge(&self.image_cache.reservation());
        retained
    }

    /// The one way memory is released. Cached images go first, oldest first,
    /// then the working set in its trimming order, until held memory is at most
    /// `limit` or only what jobs and eviction-protected photos need is left.
    /// Returns host memory plus the cached images that remain, and the trim.
    pub fn release_cache(
        &mut self,
        retained: &MemoryReservation,
        limit: u64,
    ) -> (MemoryReservation, MemoryTrim) {
        let held = loop {
            let held = self.with_cached_images(retained.clone());
            let total = self
                .engine
                .memory_resources(&self.working, &held)
                .total_bytes;
            if total <= limit || !self.image_cache.drop_oldest() {
                break held;
            }
        };
        let trim = self.engine.trim_memory(&mut self.working, limit, &held);
        (held, trim)
    }

    pub fn set_target(&mut self, target_bytes: u64) {
        self.target_bytes = target_bytes;
        self.memory_changed();
    }

    /// Working photos that trimming never evicts.
    pub fn set_eviction_protected(&mut self, keys: impl IntoIterator<Item = PhotoKey>) {
        self.working.set_eviction_protected(keys);
    }

    /// The host freed memory, for example by trimming, so deferred jobs may fit now.
    pub fn memory_changed(&mut self) {
        self.queue.memory_changed();
    }

    /// Performance tests and profiling may cap how many jobs run together.
    pub fn set_width_cap(&mut self, cap: Option<usize>) {
        self.width_cap = cap;
    }

    /// The most jobs that have stepped together in one tick, for performance tests.
    pub fn widest(&self) -> usize {
        self.widest
    }

    pub fn is_idle(&self) -> bool {
        self.admitted.is_empty() && self.queue.is_empty()
    }

    pub fn admitted(&self) -> impl Iterator<Item = &J> {
        self.admitted.iter().map(|admitted| &admitted.job)
    }

    pub fn queued(&self) -> impl Iterator<Item = &J> {
        self.queue.jobs()
    }

    /// A newer job for a queued target replaces it and keeps its place in line.
    /// A newer job for a running target waits behind the targets still queued.
    /// Otherwise, when edits arrive faster than every target can run, the
    /// targets served first would be served every time and the rest never.
    pub fn submit(&mut self, job: J) {
        self.queue.submit(job);
    }

    /// Change queued jobs in place, such as their rank, keeping whether each is
    /// waiting for its input or deferred.
    pub fn update_queued(&mut self, update: impl FnMut(&mut J)) {
        self.queue.update(update);
    }

    /// Drop queued jobs the host no longer wants. Admitted jobs learn of it
    /// through `Host::interruption`.
    pub fn retain_queued(&mut self, keep: impl FnMut(&J) -> bool) {
        self.queue.retain(keep);
    }

    /// Drop every queued and admitted job without delivering them.
    pub fn drop_jobs(&mut self) {
        while let Some(admitted) = self.admitted.pop() {
            let mut profile = admitted.profile;
            let _ = admitted.execution.finish(
                &self.engine,
                &mut self.working,
                Err(Box::new(Superseded)),
                &mut profile,
            );
        }
        self.queue.clear();
    }

    /// Run one tick. Returns false when nothing could run.
    pub fn tick(&mut self, host: &mut impl Host<J>) -> bool {
        let mut progressed = self.apply_interruptions(host);
        let Some(tier) = self.best_tier() else {
            return progressed;
        };
        let lower = self
            .admitted
            .iter()
            .any(|admitted| admitted.job.purpose().priority() > tier);
        if lower {
            for admitted in &self.admitted {
                host.yielded(&admitted.job);
            }
            self.retire_all();
        }
        self.retire_overflow(host);
        progressed |= self.admit(tier, host);
        if self.admitted.is_empty() {
            return progressed;
        }
        self.step_all(host);
        true
    }

    fn apply_interruptions(&mut self, host: &mut impl Host<J>) -> bool {
        let mut changed = false;
        let mut index = 0;
        while index < self.admitted.len() {
            match host.interruption(&self.admitted[index].job) {
                Interruption::Continue => index += 1,
                Interruption::Cancel => {
                    let admitted = self.admitted.remove(index);
                    self.finish(admitted, Err(Box::new(Superseded)), host);
                    changed = true;
                }
                Interruption::Yield => {
                    host.yielded(&self.admitted[index].job);
                    self.retire(index);
                    changed = true;
                }
            }
        }
        changed
    }

    /// The highest tier with queued or admitted work.
    fn best_tier(&self) -> Option<Priority> {
        self.queue
            .best_tier()
            .into_iter()
            .chain(
                self.admitted
                    .iter()
                    .map(|admitted| admitted.job.purpose().priority()),
            )
            .min()
    }

    /// Start queued jobs of this tier, in order, while they fit.
    fn admit(&mut self, tier: Priority, host: &mut impl Host<J>) -> bool {
        let mut progressed = false;
        loop {
            let full = self.width_cap.is_some_and(|cap| self.admitted.len() >= cap)
                || self
                    .admitted
                    .iter()
                    .any(|admitted| admitted.job.exclusive());
            if full {
                return progressed;
            }
            let (admitted, working) = (&self.admitted, &self.working);
            let Some(next) = self.queue.next(tier, |queued| {
                let job = &queued.job;
                host.ready(working, job, queued.waiting)
                    && !admitted.iter().any(|admitted| {
                        admitted.job.target() == job.target()
                            || job.photo().is_some() && admitted.job.photo() == job.photo()
                    })
            }) else {
                return progressed;
            };
            let exclusive = next.job.exclusive();
            if exclusive && !self.admitted.is_empty() {
                return progressed;
            }
            let target = next.job.target().clone();
            let mut queued = self.queue.take(&target).expect("next job is queued");
            progressed = true;
            let alone = self.admitted.is_empty();
            let render =
                match host.start(&mut self.engine, &mut self.working, &mut queued.job, alone) {
                    Start::Finished if exclusive => return progressed,
                    Start::Finished => continue,
                    Start::Waiting => {
                        queued.waiting = true;
                        self.queue.put_back(queued);
                        continue;
                    }
                    Start::Deferred => {
                        queued.deferred = true;
                        self.queue.put_back(queued);
                        continue;
                    }
                    Start::Render(render) => *render,
                };
            if !self.begin(queued, render, host) || exclusive {
                return progressed;
            }
        }
    }

    /// Returns false when admission should stop for this tick.
    fn begin(&mut self, queued: Queued<J>, mut render: Render, host: &mut impl Host<J>) -> bool {
        let priority = queued.job.purpose().priority();
        let names: Vec<&str> = render.names.iter().map(String::as_str).collect();
        let started = {
            let job = &queued.job;
            let host = &*host;
            let cancel = || host.interruption(job) != Interruption::Continue;
            let progress = |stage| host.progress(job, stage);
            self.engine.begin_render_execution(
                &mut self.working,
                render.key,
                &render.settings,
                &names,
                &cancel,
                &mut render.profile,
                Some(&progress),
            )
        };
        let mut execution = match started {
            Ok(RenderExecutionStart::Ready(execution)) => execution,
            Ok(RenderExecutionStart::Missing | RenderExecutionStart::NeedsOriginal(_)) => {
                host.finish(&mut self.engine, queued.job, Outcome::NeedsOriginal);
                return true;
            }
            Ok(RenderExecutionStart::Superseded) => {
                self.settle_cancelled(queued, host);
                return true;
            }
            Err(error) => {
                host.finish(&mut self.engine, queued.job, Outcome::Failed(error));
                return true;
            }
        };
        let retained = host.retained();
        let speculative = priority == Priority::Speculative;
        let fits = if self.admitted.is_empty() {
            // The first job always runs, and trimming after the tick returns memory
            // to the target. A first speculative job runs only when its result fits
            // in free memory, so it never forces an eviction. The target describes
            // the steady state, so its transient peak is not counted.
            !speculative || self.result_fits(&self.with_cached_images(retained), &execution)
        } else {
            self.group_fits(priority, &retained, Some(&execution))
        };
        if !fits {
            self.abandon(execution, queued, render.profile);
            return false;
        }
        execution.edit_use = self.working.record_use(render.key, queued.job.purpose());
        self.admitted.push(Admitted {
            job: queued.job,
            order: queued.order,
            execution,
            profile: render.profile,
            outcome: None,
        });
        true
    }

    /// Whether admitted jobs and the candidate fit. Viewport jobs release cache
    /// down to the room they leave: cache has room only beside one render of
    /// every editing target together, so a group that still cannot fit leaves
    /// it none, and takes turns. Other work runs wider only in free memory.
    fn group_fits(
        &mut self,
        priority: Priority,
        retained: &MemoryReservation,
        candidate: Option<&RenderExecution>,
    ) -> bool {
        let held = self.with_cached_images(retained.clone());
        if priority != Priority::Viewport {
            return self.fits(&held, candidate);
        }
        let Some(needed) = self.needed(&held, candidate) else {
            return false;
        };
        let excess = needed.saturating_sub(self.target_bytes);
        if excess == 0 {
            return true;
        }
        // Cache that could not make the room stays: releasing it would cost
        // its next use and the group would take turns all the same.
        let cache =
            self.image_cache.reservation().bytes() + self.engine.releasable_bytes(&self.working);
        if excess > cache {
            return false;
        }
        let total = self
            .engine
            .memory_resources(&self.working, &held)
            .total_bytes;
        let (held, _) = self.release_cache(retained, total.saturating_sub(excess));
        self.fits(&held, candidate)
    }

    /// Whether every admitted execution, and the candidate, fit the target
    /// alongside retained memory without evicting anything.
    fn fits(&self, retained: &MemoryReservation, candidate: Option<&RenderExecution>) -> bool {
        self.needed(retained, candidate)
            .is_some_and(|needed| needed <= self.target_bytes)
    }

    /// What retained memory, every admitted execution and the candidate need
    /// at their peak. `None` when a job has no estimate.
    fn needed(
        &self,
        retained: &MemoryReservation,
        candidate: Option<&RenderExecution>,
    ) -> Option<u64> {
        let executions: Vec<&RenderExecution> = self
            .admitted
            .iter()
            .map(|admitted| &admitted.execution)
            .chain(candidate)
            .collect();
        let future = executions.iter().try_fold(0_u64, |total, execution| {
            total.checked_add(execution.future_bytes()?)
        })?;
        self.engine
            .execution_memory_resources(&self.working, retained, &executions)
            .total_bytes
            .checked_add(future)
    }

    /// Whether retained memory plus the execution's delivered image fit the target.
    /// A job without an estimate runs alone.
    fn result_fits(&self, retained: &MemoryReservation, execution: &RenderExecution) -> bool {
        let Some(delivery) = execution.delivery_bytes() else {
            return true;
        };
        let current = self
            .engine
            .execution_memory_resources(&self.working, retained, &[execution])
            .total_bytes;
        current
            .checked_add(delivery)
            .is_some_and(|total| total <= self.target_bytes)
    }

    /// Return the latest-submitted jobs to the queue while the admitted set no
    /// longer fits even after releasing cache.
    fn retire_overflow(&mut self, host: &mut impl Host<J>) {
        if self.admitted.len() <= 1 {
            return;
        }
        let retained = host.retained();
        while self.admitted.len() > 1 {
            let priority = self
                .admitted
                .iter()
                .map(|admitted| admitted.job.purpose().priority())
                .min()
                .unwrap();
            if self.group_fits(priority, &retained, None) {
                break;
            }
            let newest = self
                .admitted
                .iter()
                .enumerate()
                .max_by_key(|(_, admitted)| admitted.order)
                .map(|(index, _)| index)
                .unwrap();
            // It stays deferred so the same tick does not start it again.
            let admitted = self.admitted.remove(newest);
            self.requeue(admitted, true);
        }
    }

    fn step_all(&mut self, host: &mut impl Host<J>) {
        self.widest = self.widest.max(self.admitted.len());
        let tick = self.engine.tick();
        for admitted in &mut self.admitted {
            let job = &admitted.job;
            let host = &*host;
            let cancel = || host.interruption(job) != Interruption::Continue;
            match admitted
                .execution
                .step(&self.engine, &mut self.working, &cancel)
            {
                Ok(true) => {}
                Ok(false) => admitted.outcome = Some(Ok(())),
                Err(error) => admitted.outcome = Some(Err(error)),
            }
        }
        // A finished job's results must be complete before they are delivered.
        // Several jobs keep the GPU busy by themselves, so only a job running
        // alone leaves its step in flight.
        let settle = self.admitted.len() > 1
            || self
                .admitted
                .iter()
                .any(|admitted| admitted.outcome.is_some());
        let waited = tick.complete(settle);
        for admitted in &mut self.admitted {
            match &waited {
                Ok(waited) => {
                    let job = &admitted.job;
                    let host = &*host;
                    admitted
                        .execution
                        .complete_tick(&self.engine, *waited, &|stage| host.progress(job, stage));
                }
                Err(error) => admitted.outcome = Some(Err(error.to_string().into())),
            }
        }
        let mut index = 0;
        while index < self.admitted.len() {
            if self.admitted[index].outcome.is_none() {
                index += 1;
                continue;
            }
            let mut admitted = self.admitted.remove(index);
            let outcome = admitted.outcome.take().unwrap();
            if outcome.is_err() && host.interruption(&admitted.job) == Interruption::Yield {
                host.yielded(&admitted.job);
                self.requeue(admitted, false);
            } else {
                self.finish(admitted, outcome, host);
            }
        }
    }

    fn finish(&mut self, admitted: Admitted<J>, outcome: Result<()>, host: &mut impl Host<J>) {
        let Admitted {
            job,
            execution,
            mut profile,
            ..
        } = admitted;
        let rendered = execution.finish(&self.engine, &mut self.working, outcome, &mut profile);
        let outcome = match rendered {
            Ok(PhotoRender::Rendered(artifacts)) => Outcome::Rendered(artifacts),
            Ok(PhotoRender::Superseded) => Outcome::Cancelled,
            Ok(PhotoRender::NeedsOriginal(_)) => Outcome::NeedsOriginal,
            Err(error) => Outcome::Failed(error),
        };
        self.memory_changed();
        host.finish(&mut self.engine, job, outcome);
    }

    fn settle_cancelled(&mut self, queued: Queued<J>, host: &mut impl Host<J>) {
        if host.interruption(&queued.job) == Interruption::Yield {
            host.yielded(&queued.job);
            self.queue.put_back(queued);
        } else {
            host.finish(&mut self.engine, queued.job, Outcome::Cancelled);
        }
    }

    /// Release a started execution that does not fit now. Nothing is committed,
    /// and the job retries after memory changes.
    fn abandon(&mut self, execution: RenderExecution, mut queued: Queued<J>, mut profile: Profile) {
        let _ = execution.finish(
            &self.engine,
            &mut self.working,
            Err(Box::new(Superseded)),
            &mut profile,
        );
        queued.deferred = true;
        self.queue.put_back(queued);
    }

    fn retire(&mut self, index: usize) {
        let admitted = self.admitted.remove(index);
        self.requeue(admitted, false);
    }

    /// Return a started job to the queue. Its released memory may let deferred jobs fit.
    fn requeue(&mut self, admitted: Admitted<J>, deferred: bool) {
        let Admitted {
            job,
            order,
            execution,
            mut profile,
            ..
        } = admitted;
        let _ = execution.finish(
            &self.engine,
            &mut self.working,
            Err(Box::new(Superseded)),
            &mut profile,
        );
        self.memory_changed();
        self.queue.put_back(Queued {
            job,
            order,
            waiting: false,
            deferred,
        });
    }

    fn retire_all(&mut self) {
        while !self.admitted.is_empty() {
            self.retire(self.admitted.len() - 1);
        }
    }
}

#[cfg(test)]
#[path = "scheduler_test.rs"]
mod tests;
