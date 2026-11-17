//! Drives shared graph execution and claimed session renders through preparation and settlement.

use super::{
    allocation::GpuAllocation,
    artifacts::ArtifactPlan,
    execute::{report_graph_failure, Completion, GraphExecution, Input, SourceRequired, Value},
    recipes::{self, PreparedRecipe, Recipe, RecipeInput},
    working_set::{ExecutionClaim, Session},
    Engine, ExecutionBudget, MemoryReservation, MemoryResources, NeedsOriginal,
    ProcessingWorkingSet, RenderProgress, SessionKey, SessionKind, SessionRender, SessionUse,
    Settings,
};
use crate::{
    diagnostics::{self, Category, Context},
    inspection::Artifact,
    interactive::{Cache, Execution, Superseded},
    profile::Profile,
    source::Result,
};
use std::sync::Arc;

#[cfg(feature = "wide-experiment")]
#[path = "experimental.rs"]
pub(super) mod experimental;
#[cfg(test)]
#[path = "driver_named_test.rs"]
mod named_tests;
#[cfg(test)]
#[path = "driver_test.rs"]
mod tests;

impl Engine {
    pub(super) fn run_recipe(
        &self,
        source: RecipeInput<'_>,
        settings: &Settings,
        recipe: Recipe<'_>,
        profile: &mut Profile,
        execution: &mut Execution<'_>,
        completion: Completion,
    ) -> Result<Vec<Artifact>> {
        let names = match recipe {
            Recipe::Photographic(names) => names,
            Recipe::DngBase => &["final"],
        };
        if names.is_empty() {
            execution.check()?;
            profile.mode.check_gpu_features(self.device().features())?;
            settings.validate_values()?;
            return Ok(Vec::new());
        }
        let mut prepared = self.prepare_recipe(source, settings, recipe, profile, execution)?;
        let plan = self.plan_artifacts(
            &prepared.settings,
            &mut prepared.built.graph,
            prepared.built.final_id,
            names,
            profile,
            execution,
        )?;
        let values = self.run_graph(
            source.context(),
            &prepared.built.graph,
            &plan.roots,
            profile,
            execution,
            completion,
        )?;
        let mut artifacts = self.package_artifacts(
            source,
            &prepared.settings,
            &prepared.built.graph,
            plan,
            values,
        )?;
        prepared.compose(&mut artifacts);
        Ok(artifacts)
    }

    /// Borrowed inputs use the same cursor as claimed, resumable sessions.
    /// Cache commit and finalization remain the input owner's transaction.
    pub(super) fn run_graph(
        &self,
        context: Input<'_>,
        graph: &crate::graph::Graph,
        targets: &[usize],
        profile: &mut Profile,
        execution: &mut Execution<'_>,
        completion: Completion,
    ) -> Result<Vec<Value>> {
        let graph_id =
            (profile.mode.enabled() || diagnostics::enabled()).then(crate::profile::next_graph_id);
        let _scope = graph_scope(
            graph,
            targets,
            execution.cache.as_deref(),
            graph_id,
            "graph.execute",
        );
        (|| {
            let mut state =
                self.start_graph(context, graph, targets, profile, execution, graph_id)?;
            while state.advance(self, context, graph, profile, execution, completion)? {}
            state.finish(self, targets, profile)
        })()
        .map_err(|error| report_graph_failure(error, context.metadata(), graph, targets))
    }
}

fn graph_scope(
    graph: &crate::graph::Graph,
    roots: &[usize],
    cache: Option<&Cache>,
    graph_id: Option<u64>,
    name: &'static str,
) -> diagnostics::Scope {
    diagnostics::scope_with(Category::Graph, name, |fields| {
        fields
            .u64("nodes", graph.nodes.len() as u64)
            .u64("targets", roots.len() as u64)
            .u64("outputs", graph.outputs.len() as u64)
            .str(
                "requested_roots",
                &serde_json::to_string(roots).expect("graph targets"),
            );
        if let Some(id) = graph_id {
            fields.u64("graph_id", id);
        }
        if let Some(cache) = cache {
            fields.u64("cache_id", cache.diagnostic_id());
        }
    })
}

/// Borrow the session's input and cache only for one preparation, step or finish.
struct SessionInput<'a> {
    source: RecipeInput<'a>,
    cache: &'a mut Cache,
}

impl<'a> SessionInput<'a> {
    fn new(session: &'a mut Session) -> Self {
        match session {
            Session::Native(session) => Self {
                source: RecipeInput::Original(&session.source),
                cache: &mut session.cache,
            },
            Session::Reduced(session) => Self {
                source: RecipeInput::Metadata {
                    metadata: &session.metadata,
                    origin: &session.origin,
                    preparation: &session.preparation,
                },
                cache: &mut session.cache,
            },
        }
    }

    fn prepare(
        session: &'a mut Session,
        engine: &Engine,
        settings: &Settings,
        profile: &mut Profile,
    ) -> Result<Option<Self>> {
        match session {
            Session::Native(session) => {
                let preparation = profile.mode.enabled().then(std::time::Instant::now);
                session.cache.bind(&engine.owner);
                session.cache.bind_source(session.source.origin());
                // Native change tracking records attempted settings, including
                // cancellation and validation failure.
                session.last_settings = Some(settings.clone());
                if let Some(start) = preparation {
                    profile.record("prepare session cache", start);
                }
            }
            Session::Reduced(session) => {
                settings.validate_values()?;
                if !Arc::ptr_eq(&session.owner, &engine.owner) {
                    return Ok(None);
                }
                session.cache.bind(&engine.owner);
                session.cache.bind_source(&session.origin);
            }
        }
        Ok(Some(Self::new(session)))
    }
}

struct Job {
    claim: ExecutionClaim,
    key: SessionKey,
    requested: Settings,
    prepared: PreparedRecipe,
    plan: ArtifactPlan,
    graph: Option<GraphExecution>,
    graph_id: Option<u64>,
    profile: Profile,
    context: Option<Context>,
    submissions_before: u64,
    complete: bool,
    reservation: Option<Reservation>,
    // Operations own their prepared parameters. Retain the request's immutable
    // calibration snapshot for the entire execution as well.
    _calibration: Option<Arc<crate::noise_calibration::Calibration>>,
}

/// Fresh outputs transfer to the actual inventory when submitted. Uploaded
/// tables and uniforms remain reserved until the window completion boundary.
struct Reservation {
    operations: Vec<GpuAllocation>,
    submitted_temporary_bytes: u64,
    delivery_bytes: u64,
}

impl Reservation {
    fn bytes(&self) -> Option<u64> {
        let operations = self
            .operations
            .iter()
            .try_fold(GpuAllocation::default(), |total, allocation| {
                total.checked_add(*allocation)
            })?;
        operations
            .total_bytes()?
            .checked_add(self.delivery_bytes)?
            .checked_add(self.submitted_temporary_bytes)
    }

    fn submitted(&mut self, node: usize) -> Option<()> {
        let allocation = std::mem::take(self.operations.get_mut(node)?);
        self.submitted_temporary_bytes = self
            .submitted_temporary_bytes
            .checked_add(allocation.temporary_bytes)?;
        Some(())
    }

    fn completed(&mut self) {
        self.submitted_temporary_bytes = 0;
    }
}

enum Begin {
    Ready(RenderExecution),
    Missing,
    NeedsOriginal,
}

/// Owns one claimed session render without borrowing the engine or working set.
/// Every started execution must be finished, including failed or cancelled work.
pub struct RenderExecution {
    job: Option<Box<Job>>,
    owner: Arc<()>,
    retirement: crate::gpu::Retirement,
    submitted: bool,
}

impl RenderExecution {
    fn check_owner(&self, engine: &Engine) -> Result<()> {
        if Arc::ptr_eq(&self.owner, &engine.owner) {
            Ok(())
        } else {
            Err("render execution belongs to another engine".into())
        }
    }

    #[cfg(feature = "wide-experiment")]
    pub fn finish(
        self,
        engine: &Engine,
        working: &mut ProcessingWorkingSet,
        outcome: Result<()>,
        profile: &mut Profile,
    ) -> Result<SessionRender> {
        self.check_owner(engine)?;
        let scope = self.graph_scope_named(working, "graph.finish");
        self.finish_with_scope(engine, working, outcome, profile, scope)
    }

    #[allow(clippy::too_many_arguments)]
    fn begin(
        engine: &Engine,
        working: &mut ProcessingWorkingSet,
        key: SessionKey,
        settings: &Settings,
        names: &[&str],
        usage: SessionUse,
        profile: &mut Profile,
        cancel: &dyn Fn() -> bool,
        progress: Option<&dyn Fn(RenderProgress)>,
    ) -> Result<Begin> {
        if !working.contains(key) {
            return Ok(Begin::Missing);
        }
        let claim = working.claim_execution(key)?;
        working.activate(key, usage);
        let mut cache_started = false;
        let submissions_before = engine.gpu_activity().submissions;
        let prepared = (|| -> Result<Option<(PreparedRecipe, ArtifactPlan)>> {
            let Some(session) =
                SessionInput::prepare(working.execution_mut(&claim), engine, settings, profile)?
            else {
                return Ok(None);
            };
            cache_started = true;
            let mut execution = Execution {
                cache: Some(session.cache),
                cancel: Some(cancel),
                progress,
                ..Default::default()
            };
            let mut prepared = engine.prepare_recipe(
                session.source,
                settings,
                recipes::Recipe::Photographic(names),
                profile,
                &mut execution,
            )?;
            let plan = engine.plan_artifacts(
                &prepared.settings,
                &mut prepared.built.graph,
                prepared.built.final_id,
                names,
                profile,
                &mut execution,
            )?;
            Ok(Some((prepared, plan)))
        })();
        match prepared {
            Ok(Some((prepared, plan))) => {
                let job = Job {
                    claim,
                    key,
                    requested: settings.clone(),
                    prepared,
                    plan,
                    graph: None,
                    graph_id: (profile.mode.enabled() || diagnostics::enabled())
                        .then(crate::profile::next_graph_id),
                    profile: std::mem::take(profile),
                    context: diagnostics::current_context(),
                    submissions_before,
                    complete: false,
                    reservation: None,
                    _calibration: engine.noise_calibration.clone(),
                };
                Ok(Begin::Ready(Self {
                    job: Some(Box::new(job)),
                    owner: engine.owner.clone(),
                    retirement: engine.gpu.retirement(),
                    submitted: false,
                }))
            }
            result => {
                let result = match result {
                    Ok(None) => Ok(Begin::NeedsOriginal),
                    Err(error) => {
                        // Recipe preparation may submit helper work. Retire it
                        // before finalizing cache state and releasing the claim.
                        let drained = if engine.gpu_activity().submissions != submissions_before {
                            engine.wait_for_submitted_work()
                        } else {
                            Ok(())
                        };
                        if cache_started {
                            match working.execution_mut(&claim) {
                                Session::Native(session) => {
                                    profile
                                        .measure("finish session cache", || session.cache.finish());
                                }
                                Session::Reduced(session) => session.cache.finish(),
                            }
                        }
                        drained.and(Err(error))
                    }
                    Ok(Some(_)) => unreachable!(),
                };
                drop(claim);
                working.retire_executions();
                result
            }
        }
    }

    fn graph_scope(&self, working: &ProcessingWorkingSet) -> diagnostics::Scope {
        self.graph_scope_named(working, "graph.execute")
    }

    fn graph_scope_named(
        &self,
        working: &ProcessingWorkingSet,
        name: &'static str,
    ) -> diagnostics::Scope {
        let job = self.job.as_ref().unwrap();
        graph_scope(
            &job.prepared.built.graph,
            &job.plan.roots,
            Some(working.execution(&job.claim).cache()),
            job.graph_id,
            name,
        )
    }

    fn advance_graph(
        &mut self,
        engine: &Engine,
        working: &mut ProcessingWorkingSet,
        cancel: &dyn Fn() -> bool,
        progress: Option<&dyn Fn(RenderProgress)>,
        completion: Completion,
    ) -> Result<bool> {
        self.initialize(engine, working, cancel, progress)?;
        let job = self.job.as_mut().unwrap();
        if diagnostics::enabled() {
            job.reservation = None;
        }
        let result = diagnostics::with_context(job.context, || {
            let session = SessionInput::new(working.execution_mut(&job.claim));
            let mut execution = Execution {
                cache: Some(session.cache),
                cancel: Some(cancel),
                progress,
                ..Default::default()
            };
            let context = session.source.context();
            let graph = job.graph.as_mut().unwrap();
            graph
                .advance(
                    engine,
                    context,
                    &job.prepared.built.graph,
                    &mut job.profile,
                    &mut execution,
                    completion,
                )
                .map_err(|error| {
                    report_graph_failure(
                        error,
                        session.source.metadata(),
                        &job.prepared.built.graph,
                        &job.plan.roots,
                    )
                })
        });
        if matches!(result, Ok(false)) {
            job.complete = true;
        } else if completion == Completion::Operation && matches!(result, Ok(true)) {
            // Per-operation completion retires temporary charges immediately.
            // Window completion retains them until the caller's shared fence.
            let node = job.graph.as_ref().unwrap().next_node() - 1;
            if let Some(reservation) = &mut job.reservation {
                if reservation.submitted(node).is_some() {
                    reservation.completed();
                } else {
                    job.reservation = None;
                }
            }
        }
        result
    }

    fn initialize(
        &mut self,
        engine: &Engine,
        working: &mut ProcessingWorkingSet,
        cancel: &dyn Fn() -> bool,
        progress: Option<&dyn Fn(RenderProgress)>,
    ) -> Result<()> {
        let job = self.job.as_mut().unwrap();
        if job.graph.is_some() {
            return Ok(());
        }
        diagnostics::with_context(job.context, || {
            let session = SessionInput::new(working.execution_mut(&job.claim));
            let mut execution = Execution {
                cache: Some(session.cache),
                cancel: Some(cancel),
                progress,
                ..Default::default()
            };
            job.graph = Some(
                engine
                    .start_graph(
                        session.source.context(),
                        &job.prepared.built.graph,
                        &job.plan.roots,
                        &mut job.profile,
                        &mut execution,
                        job.graph_id,
                    )
                    .map_err(|error| {
                        report_graph_failure(
                            error,
                            session.source.metadata(),
                            &job.prepared.built.graph,
                            &job.plan.roots,
                        )
                    })?,
            );
            job.reservation = estimate_job(job);
            Ok(())
        })
    }

    fn account(&self, retained: &mut crate::recipe::RetainedBytes) -> bool {
        retained.add_bytes(size_of::<Self>() + size_of::<Job>());
        let job = self.job.as_ref().unwrap();
        job.requested.account(retained);
        let mut complete = job.prepared.account(retained).is_some();
        job.plan.account(retained);
        job.profile.account(retained);
        if let Some(calibration) = &job._calibration {
            retained.owner(calibration.clone(), calibration.retained_bytes());
        }
        if let Some(graph) = &job.graph {
            complete &= graph.account(retained).is_some();
        }
        if let Some(reservation) = &job.reservation {
            retained.add_bytes(reservation.operations.capacity() * size_of::<GpuAllocation>());
        }
        complete
    }

    fn future_bytes(&self) -> Option<u64> {
        if diagnostics::enabled() {
            return None;
        }
        self.job.as_ref()?.reservation.as_ref()?.bytes()
    }

    fn reservation(&self, frontend: Option<&MemoryReservation>) -> MemoryReservation {
        let mut reservation = frontend.cloned().unwrap_or_default();
        if let Some(graph) = &self.job.as_ref().unwrap().graph {
            for buffer in graph.buffers() {
                reservation.add_buffer(buffer);
            }
        }
        reservation
    }

    fn memory(
        &self,
        engine: &Engine,
        working: &ProcessingWorkingSet,
        budget: Option<ExecutionBudget<'_>>,
    ) -> MemoryResources {
        let reservation = self.reservation(budget.map(|budget| budget.retained));
        let mut complete = true;
        let resources = engine.memory_resources_with(working, &reservation, |retained| {
            complete = self.account(retained);
        });
        let future = complete.then(|| self.future_bytes()).flatten();
        diagnostics::event(Category::Memory, "execution.memory", |fields| {
            fields
                .u64("executions", 1)
                .u64("retained_bytes", resources.total_bytes)
                .bool("retained_job_accounted", complete)
                .bool("future_gpu_estimated", future.is_some())
                .bool("future_cpu_estimated", false)
                .bool("complete_execution_accounting", false)
                .bool("extra_admission_allowed", false);
            if let Some(future) = future {
                fields.u64("execution_reserved_bytes", future);
            }
            if let Some(budget) = budget {
                fields.u64("limit_bytes", budget.limit_bytes).u64(
                    "retained_over_budget_bytes",
                    resources.total_bytes.saturating_sub(budget.limit_bytes),
                );
            }
        });
        resources
    }

    fn reconcile(
        &self,
        engine: &Engine,
        working: &mut ProcessingWorkingSet,
        budget: Option<ExecutionBudget<'_>>,
    ) {
        if let Some(budget) = budget {
            let reservation = self.reservation(Some(budget.retained));
            let future = self.future_bytes().unwrap_or(0);
            engine.trim_memory_with(
                working,
                budget.limit_bytes.saturating_sub(future),
                &reservation,
                |retained| {
                    self.account(retained);
                },
            );
        }
        if diagnostics::enabled() {
            self.memory(engine, working, budget);
        }
    }

    fn finish_with_scope(
        mut self,
        engine: &Engine,
        working: &mut ProcessingWorkingSet,
        mut outcome: Result<()>,
        profile: &mut Profile,
        graph_scope: diagnostics::Scope,
    ) -> Result<SessionRender> {
        self.check_owner(engine)?;
        if self.submitted {
            if let Err(error) = self.retirement.wait() {
                outcome = Err(error);
            }
        }
        let Job {
            claim,
            key,
            requested,
            prepared,
            plan,
            graph,
            profile: mut job_profile,
            context,
            submissions_before,
            complete,
            ..
        } = *self.job.take().unwrap();
        let mut outcome = outcome.and_then(|()| {
            if complete {
                Ok(())
            } else {
                Err("graph execution has not completed".into())
            }
        });
        // Keep the cursor, scratch and cache claim alive until failed work has
        // retired. Early rejection without new submissions needs no queue wait.
        if outcome.is_err() && engine.gpu_activity().submissions != submissions_before {
            if let Err(error) = engine.wait_for_submitted_work() {
                outcome = Err(error);
            }
        }
        // Finishing profiling or packaging can itself submit GPU work. Retain
        // graph storage across those fallible, consuming operations as well.
        let retirement: Vec<_> = graph
            .iter()
            .flat_map(GraphExecution::buffers)
            .cloned()
            .collect();
        let finalizing_submissions = engine.gpu_activity().submissions;
        let values = diagnostics::with_context(context, || {
            outcome?;
            graph
                .expect("started execution")
                .finish(engine, &plan.roots, &mut job_profile)
                .map_err(|error| {
                    let session = SessionInput::new(working.execution_mut(&claim));
                    report_graph_failure(
                        error,
                        session.source.metadata(),
                        &prepared.built.graph,
                        &plan.roots,
                    )
                })
        });
        drop(graph_scope);
        let mut result = diagnostics::with_context(context, || -> Result<Vec<Artifact>> {
            let values = values?;
            let session = SessionInput::new(working.execution_mut(&claim));
            let mut artifacts = engine.package_artifacts(
                session.source,
                &prepared.settings,
                &prepared.built.graph,
                plan,
                values,
            )?;
            prepared.compose(&mut artifacts);
            Ok(artifacts)
        });
        if result.is_err() && engine.gpu_activity().submissions != finalizing_submissions {
            if let Err(error) = engine.wait_for_submitted_work() {
                result = Err(error);
            }
        }
        drop(retirement);
        match working.execution_mut(&claim) {
            Session::Native(session) => {
                job_profile.measure("finish session cache", || {
                    if result.is_ok() {
                        session.cache.commit();
                    }
                    session.cache.finish();
                });
            }
            Session::Reduced(session) => {
                if result.is_ok() {
                    session.cache.commit();
                    session.scale = requested.working_scale;
                    session.last_settings = Some(Box::new(requested));
                }
                session.cache.finish();
            }
        }
        drop(claim);
        working.retire_executions();
        *profile = job_profile;
        classify(result, key.kind)
    }
}

fn classify(result: Result<Vec<Artifact>>, kind: SessionKind) -> Result<SessionRender> {
    match result {
        Ok(artifacts) => Ok(SessionRender::Rendered(artifacts)),
        Err(error) if kind == SessionKind::Reduced && error.is::<SourceRequired>() => {
            Ok(SessionRender::NeedsOriginal(NeedsOriginal::UpstreamChanged))
        }
        Err(error) if error.is::<Superseded>() => Ok(SessionRender::Superseded),
        Err(error) => Err(error),
    }
}

fn estimate_job(job: &Job) -> Option<Reservation> {
    if job.key.kind != SessionKind::Reduced
        || diagnostics::enabled()
        || !job.plan.final_image_only(job.prepared.built.final_id)
    {
        return None;
    }
    let operations = job.graph.as_ref()?.allocation_plan(
        &job.prepared.built.graph,
        job.profile.mode,
        job.prepared.settings.preview_region.is_some(),
    )?;
    // The current worker creates one unresized view. Its texture and uniform are
    // a future charge, separate from the graph's retained photographic buffer.
    let extent = job.prepared.built.params.output[..2].try_into().ok()?;
    let delivery_bytes = super::allocation::image_bytes(extent, 4)?.checked_add(
        super::allocation::uploaded_bytes::<crate::nodes::sensor::calibration::EncodeParams>(1)?,
    )?;
    let reservation = Reservation {
        operations,
        submitted_temporary_bytes: 0,
        delivery_bytes,
    };
    reservation.bytes()?;
    Some(reservation)
}

#[cfg(feature = "wide-experiment")]
impl Engine {
    /// Count retained processing, frontend and live execution owners together.
    /// Shared storage and repeated execution handles are counted once. This is
    /// a known subtotal. Future allocations and operation temporaries need headroom.
    pub fn execution_memory_resources(
        &self,
        working: &ProcessingWorkingSet,
        retained: &MemoryReservation,
        executions: &[&RenderExecution],
    ) -> MemoryResources {
        let mut reservation = retained.clone();
        let mut unique = Vec::<&RenderExecution>::with_capacity(executions.len());
        for execution in executions {
            if unique.iter().any(|prior| std::ptr::eq(*prior, *execution)) {
                continue;
            }
            unique.push(*execution);
            reservation.extend_shared(&execution.reservation(None));
        }
        self.memory_resources_with(working, &reservation, |retained| {
            for execution in unique {
                execution.account(retained);
            }
        })
    }
}

impl Drop for RenderExecution {
    fn drop(&mut self) {
        if self.job.is_some() {
            // Submitted storage must survive abandonment or a panic. Normal
            // callers finish cache state explicitly before releasing the claim.
            let _ = self.retirement.wait();
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_session(
    engine: &Engine,
    working: &mut ProcessingWorkingSet,
    key: SessionKey,
    settings: &Settings,
    names: &[&str],
    usage: SessionUse,
    cancel: &dyn Fn() -> bool,
    profile: &mut Profile,
    progress: Option<&dyn Fn(RenderProgress)>,
    budget: Option<ExecutionBudget<'_>>,
) -> Result<Option<SessionRender>> {
    let mut execution = match RenderExecution::begin(
        engine, working, key, settings, names, usage, profile, cancel, progress,
    ) {
        Ok(Begin::Missing) => return Ok(None),
        Ok(Begin::NeedsOriginal) => {
            return Ok(Some(SessionRender::NeedsOriginal(
                NeedsOriginal::EngineChanged,
            )))
        }
        Err(error) => return classify(Err(error), key.kind).map(Some),
        Ok(Begin::Ready(execution)) => execution,
    };
    let scope = execution.graph_scope(working);
    let outcome = execution
        .initialize(engine, working, cancel, progress)
        .and_then(|()| {
            execution.reconcile(engine, working, budget);
            loop {
                match execution.advance_graph(
                    engine,
                    working,
                    cancel,
                    progress,
                    Completion::Operation,
                ) {
                    Ok(true) => {
                        if diagnostics::enabled() {
                            execution.memory(engine, working, budget);
                        }
                    }
                    Ok(false) => break Ok(()),
                    Err(error) => break Err(error),
                }
            }
        });
    execution
        .finish_with_scope(engine, working, outcome, profile, scope)
        .map(Some)
}
