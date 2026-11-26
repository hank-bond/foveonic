//! Turns the coordinator's work plan into scheduler jobs and gives each job its application meaning.

use super::{
    interruption, memory_inventory, memory_snapshot, presentation::Presentation, processing,
    processing::Photos, resolve_settings, ColorPickReply, Comparison, MemoryStatus, MemoryTarget,
    Pending, RenderOutcome, RenderResult, Reply, Update,
};
use crate::rendering::coordinator::{PickTask, ScopeTask, Stamp, Target, Task};
use eframe::egui;
use foveonic::{
    inspection::{Artifact, Content},
    priority::Priority,
    profile::Profile,
    render::{
        Engine, Interruption, Job, Outcome, PhotoKey, Purpose, Render, Resolution, Scale,
        Scheduler, Settings, Start, WorkingSet,
    },
    source::Metadata,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

type Slot = Arc<(Mutex<Pending>, Condvar)>;

#[derive(Clone, PartialEq)]
pub(super) enum JobTarget {
    Render(Target),
    Pick,
    Scopes,
    Prepare,
}

pub(super) enum Work {
    Render(RenderWork),
    Pick(Arc<PickTask>),
    Scopes(Arc<ScopeTask>),
    Prepare(Arc<super::PreparationConfig>),
}

/// Picks, scopes and idle preparation follow the planned renders of their tier.
const AFTER_PLAN: usize = usize::MAX;
/// A planned task's rank: patches before whole images. Tasks of one kind share
/// a rank, so the queue serves a group's photos in turn. A rank per photo
/// would render the first photos on every edit and the rest only once edits stop.
fn plan_rank(task: &Task) -> usize {
    usize::from(!task.request.detail)
}

pub(super) struct WorkJob {
    target: JobTarget,
    purpose: Purpose,
    /// The render's place in the latest work plan, which lists the planner's order.
    rank: usize,
    work: Work,
}

impl Job for WorkJob {
    type Target = JobTarget;
    fn target(&self) -> &JobTarget {
        &self.target
    }
    fn purpose(&self) -> Purpose {
        self.purpose
    }
    fn exclusive(&self) -> bool {
        !matches!(self.work, Work::Render(_))
    }
    fn photo(&self) -> Option<&std::path::Path> {
        match &self.work {
            Work::Render(render) => Some(&render.task.request.path),
            _ => None,
        }
    }
    fn rank(&self) -> usize {
        self.rank
    }
}

pub(super) struct RenderWork {
    task: Arc<Task>,
    /// The reduced working photo's sensor data was stale. Prepare it again from the file.
    refresh: bool,
    started: Option<Box<Started>>,
}

/// Context resolved when a render job starts, used while it runs and on delivery.
struct Started {
    metadata: Arc<Metadata>,
    origin: foveonic::source::SourceOrigin,
    settings: Settings,
    edit: Vec<&'static str>,
    input: Option<&'static str>,
    began: Instant,
    #[cfg(test)]
    dispatches_before: u64,
}

pub(super) struct WorkerHost {
    pub(super) photos: Photos,
    pub(super) presented: Presentation,
    pub(super) target: MemoryTarget,
    pub(super) pending_images: Vec<crate::memory::PendingImage>,
    preparation: super::preparation::Preparation,
    preparation_exhausted: Option<super::preparation::Exhausted>,
    pub(super) sources: crate::rendering::source_loader::Handle,
    slot: Slot,
    tx: crossbeam_channel::Sender<Update>,
    cache: Arc<foveonic::decode_cache::Cache>,
    progress: crate::progress::Tracker,
    memory_updates: Arc<Mutex<MemoryStatus>>,
    context: egui::Context,
    pub(super) diagnostic_work: super::diagnostics::Service,
    /// Render and export stamps already delivered for the current plan.
    served: HashSet<Stamp>,
    served_pick: Option<crate::rendering::coordinator::TaskId>,
    served_scopes: Option<crate::rendering::coordinator::TaskId>,
    resubmit: Vec<WorkJob>,
    /// The plan and pending source seen by the last sync, to notice changes before waiting.
    observed: (
        Arc<crate::rendering::coordinator::WorkPlan>,
        Option<std::path::PathBuf>,
    ),
    pub(super) disconnected: bool,
    /// A job finished since the last trim, so memory may need restoring to the target.
    finished_since_trim: bool,
    /// The working photo each target last rendered from, kept while it is loaded.
    rendered_from: HashMap<crate::rendering::coordinator::Target, PhotoKey>,
    image_cache: foveonic::render::ImageCache,
    #[cfg(test)]
    pub(super) observations: Arc<super::Checks>,
}

impl WorkerHost {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        sources: crate::rendering::source_loader::Handle,
        slot: Slot,
        tx: crossbeam_channel::Sender<Update>,
        cache: Arc<foveonic::decode_cache::Cache>,
        progress: crate::progress::Tracker,
        memory_updates: Arc<Mutex<MemoryStatus>>,
        context: egui::Context,
        diagnostic_work: super::diagnostics::Service,
        image_cache: foveonic::render::ImageCache,
        #[cfg(test)] observations: Arc<super::Checks>,
    ) -> Self {
        Self {
            photos: Photos::default(),
            presented: Presentation::default(),
            target: MemoryTarget::default(),
            pending_images: Vec::new(),
            preparation: Default::default(),
            preparation_exhausted: None,
            sources,
            slot,
            tx,
            cache,
            progress,
            memory_updates,
            context,
            diagnostic_work,
            served: HashSet::new(),
            rendered_from: HashMap::new(),
            served_pick: None,
            served_scopes: None,
            resubmit: Vec::new(),
            observed: Default::default(),
            disconnected: false,
            finished_since_trim: false,
            image_cache,
            #[cfg(test)]
            observations,
        }
    }

    /// Forget delivered work and presentation state after the coordinator cleared its plan.
    pub(super) fn reset(&mut self) {
        self.presented.clear();
        self.served.clear();
        self.served_pick = None;
        self.served_scopes = None;
        self.resubmit.clear();
    }

    pub(super) fn reset_preparation(&mut self) {
        self.preparation = Default::default();
        self.preparation_exhausted = None;
    }

    /// Submit new or changed work from the current plan and drop work it no longer wants.
    pub(super) fn sync(&mut self, scheduler: &mut Scheduler<WorkJob>) {
        let state = self.slot.0.lock().unwrap();
        let plan = state.plan.clone();
        let photos = &self.photos;
        self.rendered_from
            .retain(|target, _| photos.id(&target.path).is_some());
        self.protect(scheduler, &plan);
        self.served
            .retain(|stamp| plan.tasks.iter().any(|task| task.stamp == *stamp));
        let served = &self.served;
        scheduler.retain_queued(|job| match &job.work {
            Work::Render(render) => {
                plan.task(&render.task.stamp.target).is_some()
                    && !served.contains(&render.task.stamp)
            }
            Work::Pick(pick) => plan
                .pick
                .as_ref()
                .is_some_and(|current| current.id == pick.id),
            Work::Scopes(scopes) => plan
                .scopes
                .as_ref()
                .is_some_and(|current| current.id == scopes.id),
            Work::Prepare(config) => state
                .preparation
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, config)),
        });
        // Queued renders take their place in the new plan without losing
        // whether they wait for an original or were deferred.
        scheduler.update_queued(|job| {
            if let Work::Render(render) = &job.work {
                if let Some(task) = plan
                    .tasks
                    .iter()
                    .find(|task| task.stamp == render.task.stamp)
                {
                    job.rank = plan_rank(task);
                }
            }
        });
        for task in &plan.tasks {
            if self.served.contains(&task.stamp) {
                continue;
            }
            let purpose = plan.purpose(task);
            let is_task = |job: &WorkJob| matches!(&job.work, Work::Render(render) if render.task.stamp == task.stamp);
            // A queued task is resubmitted when its purpose changes. A running one continues.
            let known = scheduler.admitted().any(is_task)
                || scheduler
                    .queued()
                    .any(|job| is_task(job) && job.purpose == purpose);
            if !known {
                scheduler.submit(WorkJob {
                    target: JobTarget::Render(task.stamp.target.clone()),
                    purpose,
                    rank: plan_rank(task),
                    work: Work::Render(RenderWork {
                        task: task.clone(),
                        refresh: false,
                        started: None,
                    }),
                });
            }
        }
        if let Some(pick) = plan
            .pick
            .as_ref()
            .filter(|pick| self.served_pick != Some(pick.id))
        {
            if !scheduler.queued().any(|job| job.target == JobTarget::Pick) {
                scheduler.submit(WorkJob {
                    target: JobTarget::Pick,
                    purpose: Purpose::View,
                    rank: AFTER_PLAN,
                    work: Work::Pick(pick.clone()),
                });
            }
        }
        if let Some(scopes) = plan
            .scopes
            .as_ref()
            .filter(|scopes| self.served_scopes != Some(scopes.id))
        {
            if !scheduler
                .queued()
                .any(|job| job.target == JobTarget::Scopes)
            {
                scheduler.submit(WorkJob {
                    target: JobTarget::Scopes,
                    purpose: Purpose::Auxiliary,
                    rank: AFTER_PLAN,
                    work: Work::Scopes(scopes.clone()),
                });
            }
        }
        if let Some(config) = state.preparation.clone() {
            let window = self.cache.preparation_window();
            let exhausted = self
                .preparation_exhausted
                .as_ref()
                .is_some_and(|done| done.covers(&config, &window));
            if !exhausted
                && !scheduler
                    .queued()
                    .any(|job| job.target == JobTarget::Prepare)
            {
                scheduler.submit(WorkJob {
                    target: JobTarget::Prepare,
                    purpose: Purpose::Speculative,
                    rank: AFTER_PLAN,
                    work: Work::Prepare(config),
                });
            }
        }
        self.observed = (
            plan.clone(),
            state.source.as_ref().map(|source| source.path.clone()),
        );
        drop(state);
        for job in self.resubmit.drain(..) {
            scheduler.submit(job);
        }
    }

    /// Whether new work may have arrived since the last sync.
    pub(super) fn plan_changed(&self, state: &Pending) -> bool {
        !Arc::ptr_eq(&state.plan, &self.observed.0)
            || state.source.as_ref().map(|source| &source.path) != self.observed.1.as_ref()
    }

    /// Protect the working photos the plan's editing targets render from.
    /// Admission and trimming both release unprotected working photos, so
    /// this runs before either can act on a new plan.
    fn protect(
        &self,
        scheduler: &mut Scheduler<WorkJob>,
        plan: &crate::rendering::coordinator::WorkPlan,
    ) {
        scheduler.set_eviction_protected(
            self.rendered_from
                .iter()
                .filter(|(target, _)| plan.eviction_protected.contains(target))
                .map(|(_, key)| *key),
        );
    }

    /// Trim to the target and publish memory status after a job finishes or while
    /// nothing is admitted. Protection is refreshed first, so a photo loaded during
    /// this tick is already protected.
    pub(super) fn trim(&mut self, scheduler: &mut Scheduler<WorkJob>) {
        if !std::mem::take(&mut self.finished_since_trim) && scheduler.admitted().next().is_some() {
            return;
        }
        let plan = self.slot.0.lock().unwrap().plan.clone();
        self.protect(scheduler, &plan);
        let target = self.target.bytes;
        let inventory = memory_inventory(
            &self.presented,
            &self.target,
            &mut self.pending_images,
            &self.slot,
        );
        if foveonic::diagnostics::enabled() {
            let held = scheduler.with_cached_images(inventory.reservation().clone());
            let (engine, working) = scheduler.engine_and_working();
            let resources = engine.memory_resources(working, &held);
            memory_snapshot("before_trim", resources, target, working);
        }
        let (_, trim) = scheduler.release_cache(inventory.reservation(), target);
        let (_, working) = scheduler.engine_and_working();
        self.photos.prune(working);
        memory_snapshot("after_trim", trim.resources, target, working);
        let status = MemoryStatus {
            resources: trim.resources,
            over_target_bytes: trim.over_target_bytes,
            reduced_sources: self.photos.reduced_sources(working),
        };
        let (changed, freed) = {
            let mut previous = self.memory_updates.lock().unwrap();
            let changed = *previous != status;
            let freed = status.resources.total_bytes < previous.resources.total_bytes;
            *previous = status;
            (changed, freed)
        };
        if freed {
            scheduler.memory_changed();
        }
        if changed {
            if crate::timing::enabled() {
                crate::timing::emit(
                    "memory",
                    || serde_json::json!({"target_bytes": target, "resources": trim.resources, "over_target_bytes": trim.over_target_bytes, "evicted_photos": trim.evicted_photos}),
                );
            }
            self.context.request_repaint();
        }
    }

    fn acquire<'a>(
        sources: &'a mut crate::rendering::source_loader::Handle,
        priority: Priority,
        speculative: bool,
    ) -> impl FnMut(
        &std::path::Path,
        u64,
    ) -> foveonic::source::Result<Option<Arc<foveonic::source::Source>>>
           + 'a {
        move |path, revision| {
            if speculative {
                Ok(None)
            } else {
                sources.open_at(path, revision, priority)
            }
        }
    }

    fn start_render(
        &mut self,
        engine: &mut Engine,
        working: &mut WorkingSet,
        purpose: Purpose,
        render: &mut RenderWork,
        alone: bool,
    ) -> foveonic::source::Result<Option<Render>> {
        let task = render.task.clone();
        let request = &task.request;
        if let Some(seed) = &task.seed {
            self.presented.seed(seed);
        }
        let composition_quality = self.presented.quality_for_view_change(request, &self.cache);
        let reporter = crate::progress::Reporter::new(self.progress.clone(), request.id.0);
        let began = Instant::now();
        crate::timing::task(
            &task.stamp,
            "execution_started",
            || serde_json::json!({"has_pixel_seed":task.seed.is_some(), "speculative":task.speculative}),
        );
        crate::timing::worker_started(request.id.0, task.queued_at, began);
        engine.set_noise_calibration(request.calibration.clone());
        if let Some(composed) = self.presented.prepare(request, &self.cache)? {
            reporter.loaded(foveonic::source::LoadStage::Decoded);
            reporter.rendered(foveonic::render::RenderProgress::Prepared);
            reporter.rendered(foveonic::render::RenderProgress::Stage {
                completed: 1,
                total: 1,
            });
            let settings =
                resolve_settings(request, &composed.metadata, Some(composed.working_scale))?;
            let started = Started {
                metadata: composed.metadata.clone(),
                origin: composed.origin.clone(),
                settings,
                edit: Vec::new(),
                input: None,
                began,
                #[cfg(test)]
                dispatches_before: engine.gpu_activity().dispatches,
            };
            #[cfg(test)]
            self.record_dispatches(engine, &task, &started);
            let result = self.deliver(
                engine,
                &task,
                &started,
                composed.artifacts,
                composed.display,
            );
            self.reply(&task, Some(&started), result);
            return Ok(None);
        }
        let reduced = request.uses_reduced_photo()
            && composition_quality.is_none_or(|scale| scale.is_reduced());
        let resolution = if reduced {
            Resolution::Reduced
        } else {
            Resolution::Native
        };
        if !alone && (render.refresh || !self.photos.resident(working, &request.path, resolution)) {
            return Err(Box::new(LoadsAlone));
        }
        let mut acquire = Self::acquire(&mut self.sources, purpose.priority(), task.speculative);
        let mut reduced_loaded = None;
        let (key, metadata, origin) = if reduced {
            let Some(loaded) = self.photos.load_reduced(
                working,
                &request.path,
                request.id.0,
                &self.cache,
                engine,
                &mut acquire,
            )?
            else {
                return Err(Box::new(NeedsSource));
            };
            let identity = (
                loaded.key(Resolution::Reduced),
                loaded.metadata.clone(),
                loaded.origin().clone(),
            );
            reduced_loaded = Some(loaded);
            identity
        } else {
            if !self.photos.load_native(
                working,
                &request.path,
                request.id.0,
                &self.cache,
                engine,
                purpose,
                &mut acquire,
            )? {
                return Err(Box::new(NeedsSource));
            }
            let native = self.photos.native(working, &request.path).unwrap();
            (
                self.photos.key(&request.path, Resolution::Native).unwrap(),
                native.metadata().clone(),
                native.source().origin().clone(),
            )
        };
        self.rendered_from.insert(task.stamp.target.clone(), key);
        reporter.loaded(foveonic::source::LoadStage::Decoded);
        let mut settings = resolve_settings(request, &metadata, None)?;
        if let Some(quality) = composition_quality {
            settings.working_scale = quality;
            settings.preview_region = None;
        }
        if let Some(loaded) = reduced_loaded {
            let slot = self.slot.clone();
            let cancel = || {
                let state = slot.0.lock().unwrap();
                state.stop || state.clear
            };
            match self.photos.ready_reduced(
                working,
                engine,
                loaded,
                &settings,
                purpose,
                std::mem::take(&mut render.refresh),
                &cancel,
                &mut Profile::new(crate::timing::mode()),
                &mut acquire,
            )? {
                processing::Readiness::Ready => {}
                processing::Readiness::NeedsSource => return Err(Box::new(NeedsSource)),
                processing::Readiness::Superseded => return Err(Box::new(Obsolete)),
            }
        }
        let edit = match key.resolution {
            Resolution::Reduced => working
                .reduced_settings(key.photo)
                .map(|previous| previous.changed_parameters(&settings))
                .or_else(|| {
                    self.photos
                        .native(working, &request.path)
                        .map(|native| native.changed_parameters(&settings))
                })
                .unwrap_or_default(),
            Resolution::Native => self
                .photos
                .native(working, &request.path)
                .unwrap()
                .changed_parameters(&settings),
        };
        let root = request.output.clone();
        let input = if request.comparison {
            Some(
                foveonic::inspection::comparison_input(&settings, &root, Some(&metadata))
                    .ok_or("selected output has no operation-input comparison")?,
            )
        } else {
            None
        };
        let mut names = vec![root];
        names.extend(input.map(str::to_owned));
        render.started = Some(Box::new(Started {
            metadata,
            origin,
            settings: settings.clone(),
            edit,
            input,
            began,
            #[cfg(test)]
            dispatches_before: engine.gpu_activity().dispatches,
        }));
        Ok(Some(Render {
            key,
            settings,
            names,
            profile: Profile::new(crate::timing::mode()),
        }))
    }

    /// Tests check how much photographic work each delivered revision dispatched.
    #[cfg(test)]
    fn record_dispatches(&self, engine: &Engine, task: &Task, started: &Started) {
        self.observations.recipes.lock().unwrap().push((
            task.request.id.0,
            engine.gpu_activity().dispatches - started.dispatches_before,
        ));
    }

    /// Turn rendered artifacts into a frame for the UI.
    fn deliver(
        &mut self,
        engine: &Engine,
        task: &Task,
        started: &Started,
        mut artifacts: Vec<Artifact>,
        reused_display: Option<super::presentation::Display>,
    ) -> foveonic::source::Result<Option<RenderResult>> {
        let request = &task.request;
        let mut profile = Profile::new(crate::timing::mode());
        let root = request.output.as_str();
        let retain_whole =
            self.presented
                .retain(request, &started.settings, &artifacts, &started.origin);
        let view_before = crate::timing::enabled().then(|| engine.gpu_activity());
        let comparison = if let Some(input) = started.input {
            let baseline = artifacts.pop().ok_or("missing comparison input")?;
            let Content::Image(image) = baseline.content else {
                return Err("comparison input must be an image".into());
            };
            let _span = crate::timing::Span::new(request.id.0, "comparison_view_prepare_host");
            Some(Comparison {
                input,
                texture: if let Some(display) = &reused_display {
                    display.comparison.as_ref().unwrap().texture.clone()
                } else {
                    Arc::new(engine.view_texture(&image, None, &mut profile)?)
                },
            })
        } else {
            None
        };
        let artifact = artifacts.pop().ok_or("missing requested artifact")?;
        let Content::Image(image) = &artifact.content else {
            return Err(
                "GUI inspection requires a visual image buffer; node-data diagnostics are deferred"
                    .into(),
            );
        };
        let native_extent = if root == "final" {
            foveonic::render::native_extent(&artifact.metadata)?
        } else {
            image.uncropped_view().extent
        };
        let texture = {
            let _span = crate::timing::Span::new(request.id.0, "view_prepare_host");
            if let Some(display) = &reused_display {
                display.texture.clone()
            } else {
                let uncropped = request.framing_preview.then(|| image.without_composition());
                Arc::new(engine.view_texture(
                    uncropped.as_ref().unwrap_or(image.as_ref()),
                    None,
                    &mut profile,
                )?)
            }
        };
        if comparison
            .as_ref()
            .is_some_and(|c| c.texture.description().extent != texture.description().extent)
        {
            return Err("comparison views have different geometry".into());
        }
        if reused_display.is_none() {
            let _span = crate::timing::Span::new(request.id.0, "view_completion_wait");
            engine.wait_for_submitted_work()?;
        }
        if let Some(before) = view_before {
            foveonic::diagnostics::event(
                foveonic::diagnostics::Category::Presentation,
                "view",
                |fields| {
                    fields
                        .u64("revision", request.id.0)
                        .str("source", &request.path.to_string_lossy())
                        .str("output", root);
                    crate::timing::activity(fields, engine.gpu_activity().since(before));
                },
            );
        }
        if retain_whole {
            self.presented
                .display(texture.clone(), comparison.clone(), request.framing_preview);
        }
        Ok(Some(RenderResult {
            request: task.request.clone(),
            diagnostic_task: crate::timing::DeliveredTask::from(&task.stamp),
            origin: started.origin.clone(),
            native_extent,
            artifact,
            texture,
            comparison,
            milliseconds: started.began.elapsed().as_secs_f64() * 1000.,
            queued_at: task.queued_at,
        }))
    }

    /// Send one result to the coordinator. `Ok(None)` is a cancelled task.
    fn reply(
        &mut self,
        task: &Task,
        started: Option<&Started>,
        result: foveonic::source::Result<Option<RenderResult>>,
    ) {
        let request = &task.request;
        self.served.insert(task.stamp.clone());
        let result = result.map_err(|error| {
            foveonic::support::report_error(
                "render.execute",
                error.as_ref(),
                foveonic::support::OperationContext {
                    operation_id: Some(format!("render-{}", task.stamp.id.0)),
                    component: Some("render worker".into()),
                    source: Some(request.path.to_string_lossy().into_owned()),
                    revision: Some(request.id.to_string()),
                    purpose: Some(format!("{:?}", request.purpose())),
                    settings: Some(serde_json::json!({
                        "requested":foveonic::support::settings_snapshot(&request.settings),
                        "effective":started.map(|started| foveonic::support::settings_snapshot(&started.settings)),
                        "output":request.output,"quality":request.processing_quality})),
                    ..Default::default()
                },
            );
            error.to_string()
        });
        let result = match result {
            Ok(Some(image)) => RenderOutcome::Rendered(Box::new(image)),
            Ok(None) => RenderOutcome::Cancelled,
            Err(error) => RenderOutcome::Failed(error),
        };
        let metadata = started.map(|started| started.metadata.clone());
        // Reply settings populate photo edits. Preview policy stays in UI preferences.
        let mut settings = started.map_or_else(
            || request.settings.clone(),
            |started| started.settings.clone(),
        );
        settings.working_scale = Scale::NATIVE;
        settings.preview_region = None;
        settings.dehaze_estimate = false;
        let prepare_defaults = matches!(
            task.stamp.target.purpose,
            crate::rendering::request::Purpose::Current
        );
        let dehaze_estimate = result
            .rendered()
            .and_then(|image| image.artifact.dehaze_estimate);
        let (mut numeric_settings, mut reset_settings) = if prepare_defaults {
            metadata
                .as_deref()
                .and_then(|metadata| foveonic::render::numeric_settings(&settings, metadata).ok())
                .map_or((None, None), |(numeric, reset)| {
                    (Some(numeric), Some(reset))
                })
        } else {
            (None, None)
        };
        if let Some(numeric) = &mut numeric_settings {
            if numeric.dehaze.shadow.is_none() {
                numeric.dehaze.shadow = dehaze_estimate;
            }
        }
        if let Some(defaults) = &mut reset_settings {
            defaults.dehaze.shadow = dehaze_estimate;
        }
        let publication = crate::timing::Span::new(request.id.0, "reply_publication");
        crate::timing::task(&task.stamp, "execution_finished", || {
            serde_json::json!({
            "outcome":match &result {RenderOutcome::Rendered(_)=>"rendered",RenderOutcome::Cancelled=>"cancelled",RenderOutcome::Failed(_)=>"failed"},
            "error":match &result {RenderOutcome::Failed(error)=>Some(error),_=>None}})
        });
        if let RenderOutcome::Rendered(frame) = &result {
            self.pending_images
                .push(crate::memory::PendingImage::new(frame));
            self.diagnostic_work
                .before_delivery(&request.path, request.id.0);
        }
        crate::timing::task(
            &task.stamp,
            "reply_ready",
            || serde_json::json!({"mailbox":"image_replies","capacity":2}),
        );
        if self
            .tx
            .send(Update::Image(Box::new(Reply {
                handling: None,
                stamp: task.stamp.clone(),
                request: task.request.clone(),
                metadata,
                settings,
                numeric_settings,
                reset_settings,
                result,
            })))
            .is_err()
        {
            self.disconnected = true;
        }
        drop(publication);
        crate::timing::task(
            &task.stamp,
            "reply_send_returned",
            || serde_json::json!({"mailbox":"image_replies"}),
        );
    }

    fn start_pick(
        &mut self,
        engine: &mut Engine,
        working: &mut WorkingSet,
        pick_task: &Arc<PickTask>,
    ) -> Start {
        let pick = &pick_task.request;
        let _diagnostic_context = crate::timing::enabled().then(|| {
            foveonic::diagnostics::enter(crate::timing::context_for(
                pick_task.id.0,
                pick_task.image_request_id.0,
                &pick.path,
                "color_pick",
            ))
        });
        let _pick_scope =
            foveonic::diagnostics::scope(foveonic::diagnostics::Category::Cpu, "color_pick_task");
        let slot = self.slot.clone();
        let cancel = || {
            let state = slot.0.lock().unwrap();
            state.stop
                || state.clear
                || state
                    .plan
                    .pick
                    .as_ref()
                    .is_none_or(|current| current.id != pick_task.id)
        };
        let sample = (|| {
            if cancel() {
                return Err("color selection canceled".into());
            }
            if !self.photos.load_native(
                working,
                &pick.path,
                0,
                &self.cache,
                engine,
                Purpose::View,
                &mut |path, revision| self.sources.open(path, revision),
            )? {
                return Ok(None);
            }
            if pick_task.origin.as_ref().is_some_and(|origin| {
                origin != self.photos.native(working, &pick.path).unwrap().source().origin()
            }) {
                return Err("photo changed while selecting a color".into());
            }
            self.photos
                .with_native(working, &pick.path, |session| {
                    super::pick_color(engine, session, pick, cancel)
                })
                .map(Some)
        })()
        .map_err(|error: Box<dyn std::error::Error + Send + Sync>| {
            if !cancel() {
                foveonic::support::report_error(
                    "render.color_pick",
                    error.as_ref(),
                    foveonic::support::OperationContext {
                        component: Some("color selection".into()),
                        source: Some(pick.path.to_string_lossy().into_owned()),
                        settings: Some(serde_json::json!({"tool":format!("{:?}",pick.tool),"uv":pick.uv,"settings":foveonic::support::settings_snapshot(&pick.settings)})),
                        ..Default::default()
                    },
                );
            }
            error.to_string()
        });
        let sample = match sample {
            Ok(Some(sample)) => Ok(sample),
            Ok(None) => return Start::Waiting,
            Err(error) => Err(error),
        };
        self.served_pick = Some(pick_task.id);
        if self
            .tx
            .send(Update::ColorPick {
                id: pick_task.id,
                reply: ColorPickReply {
                    tool: pick.tool,
                    token: pick.token,
                    path: pick.path.clone(),
                    sample,
                },
            })
            .is_err()
        {
            self.disconnected = true;
        }
        Start::Finished
    }

    fn start_prepare(
        &mut self,
        engine: &mut Engine,
        working: &mut WorkingSet,
        config: &Arc<super::PreparationConfig>,
    ) -> Start {
        let mut reservation = memory_inventory(
            &self.presented,
            &self.target,
            &mut self.pending_images,
            &self.slot,
        )
        .reservation()
        .clone();
        reservation.merge(&self.image_cache.reservation());
        let slot = self.slot.clone();
        let pending = || {
            let state = slot.0.lock().unwrap();
            state.stop
                || state.clear
                || state.memory.is_some()
                || state
                    .preparation
                    .as_ref()
                    .is_none_or(|current| !Arc::ptr_eq(current, config))
        };
        let window = self.cache.preparation_window();
        if !self.preparation.run_one(
            config,
            &self.cache,
            engine,
            working,
            &mut self.photos,
            &reservation,
            self.target.bytes,
            &pending,
        ) {
            self.preparation_exhausted =
                Some(super::preparation::Exhausted::new(config.clone(), window));
        }
        Start::Finished
    }

    fn defer(&self, stamp: &Stamp) {
        crate::timing::task(
            stamp,
            "execution_deferred",
            || serde_json::json!({"reason":"source-loading"}),
        );
        #[cfg(test)]
        {
            self.observations
                .deferred
                .0
                .lock()
                .unwrap()
                .push(stamp.clone());
            self.observations.deferred.1.notify_all();
        }
    }

    fn source_path(job: &WorkJob) -> Option<&std::path::Path> {
        match &job.work {
            Work::Render(render) => Some(&render.task.request.path),
            Work::Pick(pick) => Some(&pick.request.path),
            Work::Scopes(_) | Work::Prepare(_) => None,
        }
    }
}

/// The job must load or prepare its working photo, which happens only when it runs alone.
#[derive(Debug)]
struct LoadsAlone;
impl std::fmt::Display for LoadsAlone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("loading a working photo waits until no other job runs")
    }
}
impl std::error::Error for LoadsAlone {}

/// A job's original is being acquired. It waits in the queue until the source loader has it.
#[derive(Debug)]
struct NeedsSource;
impl std::fmt::Display for NeedsSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("waiting for the original")
    }
}
impl std::error::Error for NeedsSource {}

/// Preparation for a job was superseded by newer work.
#[derive(Debug)]
struct Obsolete;
impl std::fmt::Display for Obsolete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("render superseded")
    }
}
impl std::error::Error for Obsolete {}

impl foveonic::render::Host<WorkJob> for WorkerHost {
    /// Higher-tier demand from other threads holds every job back. A waiting job
    /// retries once the loader has its original.
    fn ready(&self, _: &WorkingSet, job: &WorkJob, waiting: bool) -> bool {
        self.cache.priority().ready(job.purpose.priority())
            && (!waiting
                || Self::source_path(job)
                    .is_none_or(|path| self.sources.can_retry(path, &self.slot.0.lock().unwrap())))
    }

    fn start(
        &mut self,
        engine: &mut Engine,
        working: &mut WorkingSet,
        job: &mut WorkJob,
        alone: bool,
    ) -> Start {
        let purpose = job.purpose;
        match &mut job.work {
            Work::Render(render) => {
                let task = render.task.clone();
                let _context = crate::timing::enabled()
                    .then(|| foveonic::diagnostics::enter(crate::timing::context(&task.stamp)));
                match self.start_render(engine, working, purpose, render, alone) {
                    Ok(Some(render)) => Start::Render(Box::new(render)),
                    Ok(None) => Start::Finished,
                    Err(error) if error.is::<LoadsAlone>() => Start::Deferred,
                    // Speculative jobs never request originals, so they wait for
                    // another job to make the photo resident.
                    Err(error) if error.is::<NeedsSource>() && task.speculative => Start::Deferred,
                    Err(error) if error.is::<NeedsSource>() => {
                        self.defer(&task.stamp);
                        Start::Waiting
                    }
                    Err(error) if error.is::<Obsolete>() => {
                        self.reply(&task, None, Ok(None));
                        Start::Finished
                    }
                    Err(error) => {
                        self.reply(&task, render.started.as_deref(), Err(error));
                        Start::Finished
                    }
                }
            }
            Work::Pick(pick) => {
                let pick = pick.clone();
                self.start_pick(engine, working, &pick)
            }
            Work::Scopes(scopes) => {
                self.served_scopes = Some(scopes.id);
                if !super::analyze_scopes(engine, &self.slot, &self.tx, scopes.clone()) {
                    self.disconnected = true;
                }
                Start::Finished
            }
            Work::Prepare(config) => {
                let config = config.clone();
                self.start_prepare(engine, working, &config)
            }
        }
    }

    fn interruption(&self, job: &WorkJob) -> Interruption {
        if !self.cache.priority().ready(job.purpose.priority()) {
            return Interruption::Yield;
        }
        let Work::Render(render) = &job.work else {
            return Interruption::Continue;
        };
        let state = self.slot.0.lock().unwrap();
        let Some(started) = &render.started else {
            return if state.plan.task(&render.task.stamp.target).is_some() {
                Interruption::Continue
            } else {
                Interruption::Cancel
            };
        };
        interruption(
            &state,
            &render.task,
            &started.settings,
            &started.metadata,
            &started.edit,
        )
    }

    fn progress(&self, job: &WorkJob, progress: foveonic::render::RenderProgress) {
        if let Work::Render(render) = &job.work {
            #[cfg(test)]
            if render.task.speculative {
                let hold = self.observations.speculative_hold.lock().unwrap().take();
                if let Some((reached, release)) = hold {
                    let _ = reached.send(());
                    let _ = release.recv();
                }
            }
            crate::progress::Reporter::new(self.progress.clone(), render.task.request.id.0)
                .rendered(progress);
        }
    }

    fn yielded(&mut self, job: &WorkJob) {
        if let Work::Render(render) = &job.work {
            crate::timing::task(
                &render.task.stamp,
                "execution_yielded",
                || serde_json::json!({"reason":"foreground"}),
            );
            #[cfg(test)]
            {
                self.observations
                    .yielded
                    .0
                    .lock()
                    .unwrap()
                    .push(render.task.stamp.clone());
                self.observations.yielded.1.notify_all();
            }
        }
    }

    fn finish(&mut self, engine: &mut Engine, job: WorkJob, outcome: Outcome) {
        self.finished_since_trim = true;
        let WorkJob {
            target,
            purpose,
            rank,
            work,
        } = job;
        let Work::Render(mut render) = work else {
            return;
        };
        let task = render.task.clone();
        let started = render.started.take().map(|started| *started);
        match outcome {
            Outcome::Rendered(artifacts) => {
                let started = started.expect("a rendered job started");
                #[cfg(test)]
                self.record_dispatches(engine, &task, &started);
                let result = self.deliver(engine, &task, &started, artifacts, None);
                self.reply(&task, Some(&started), result);
            }
            Outcome::Cancelled => self.reply(&task, started.as_ref(), Ok(None)),
            Outcome::NeedsOriginal => {
                render.refresh = true;
                self.resubmit.push(WorkJob {
                    target,
                    purpose,
                    rank,
                    work: Work::Render(render),
                });
            }
            Outcome::Failed(error) => self.reply(&task, started.as_ref(), Err(error)),
        }
    }

    fn retained(&mut self) -> foveonic::render::MemoryReservation {
        memory_inventory(
            &self.presented,
            &self.target,
            &mut self.pending_images,
            &self.slot,
        )
        .reservation()
        .clone()
    }
}
