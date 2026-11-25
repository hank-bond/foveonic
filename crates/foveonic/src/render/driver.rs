//! Drives every render through one resumable recipe execution that steps its graph traversal with a tick wait after each step.

use super::{
    allocation::NodeCost,
    artifacts::ArtifactPlan,
    execute::{report_graph_failure, Input, SourceRequired, Traversal, Value},
    recipes::{PreparedRecipe, Recipe, RecipeInput},
    working_set::{Guard, WorkingPhoto},
    Engine, MemoryReservation, MemoryResources, NeedsOriginal, PhotoKey, PhotoRender, Purpose,
    RenderProgress, Resolution, Settings, WorkingSet,
};
use crate::{
    diagnostics::{self, Category, Context},
    inspection::Artifact,
    interactive::{Cache, Execution, Superseded},
    profile::Profile,
    source::Result,
};
use std::{sync::Arc, time::Duration};

#[cfg(test)]
#[path = "execution_test.rs"]
mod execution_tests;
#[cfg(test)]
#[path = "driver_named_test.rs"]
mod named_tests;
#[cfg(test)]
#[path = "driver_test.rs"]
pub(crate) mod tests;

/// The steps submitted since the tick began.
pub struct Tick<'a>(crate::gpu::Tick<'a>);

impl Tick<'_> {
    /// Wait for the previous tick's work, or for everything after a job's
    /// `last` step. Returns how long the wait took.
    pub fn complete(self, last: bool) -> Result<Duration> {
        self.0.complete(last)
    }
}

impl Engine {
    pub fn tick(&self) -> Tick<'_> {
        Tick(self.gpu.tick())
    }

    pub(super) fn run_recipe(
        &self,
        source: RecipeInput<'_>,
        settings: &Settings,
        recipe: Recipe<'_>,
        profile: &mut Profile,
        execution: &mut Execution<'_>,
    ) -> Result<Vec<Artifact>> {
        if matches!(recipe, Recipe::Photographic([])) {
            execution.check()?;
            profile.mode.check_gpu_features(self.device().features())?;
            settings.validate_values()?;
            return Ok(Vec::new());
        }
        let mut render =
            RecipeExecution::begin(self, source, settings, recipe, profile, execution)?;
        let _scope = render.scope(execution.cache.as_deref(), "graph.execute");
        let progress = execution.progress;
        let outcome = render.run_alone(self, progress, |render| {
            render.step(self, source, execution)
        });
        render.finish(self, source, outcome, profile)
    }

    /// Runs a bare graph, such as preparing preparation inputs, one step per tick.
    /// Cache commit and finalization remain the input owner's transaction.
    pub(super) fn run_graph(
        &self,
        context: Input<'_>,
        graph: &crate::graph::Graph,
        targets: &[usize],
        profile: &mut Profile,
        execution: &mut Execution<'_>,
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
            let mut traversal =
                self.start_graph(context, graph, targets, profile, execution, graph_id)?;
            loop {
                let tick = self.tick();
                let more = traversal.step(self, context, graph, profile, execution)?;
                traversal.complete_tick(tick.complete(!more)?, profile, execution.progress);
                if !more {
                    break;
                }
            }
            traversal.finish(self, targets, profile)
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

/// Borrow a working photo.s input and cache only for one call.
struct PhotoInput<'a> {
    source: RecipeInput<'a>,
    cache: &'a mut Cache,
}

impl<'a> PhotoInput<'a> {
    fn new(session: &'a mut WorkingPhoto) -> Self {
        match session {
            WorkingPhoto::Native(session) => Self {
                source: RecipeInput::Original(&session.source),
                cache: &mut session.cache,
            },
            WorkingPhoto::Reduced(session) => Self {
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
        session: &'a mut WorkingPhoto,
        engine: &Engine,
        settings: &Settings,
        profile: &mut Profile,
    ) -> Result<Option<Self>> {
        match session {
            WorkingPhoto::Native(session) => {
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
            WorkingPhoto::Reduced(session) => {
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

    fn execution<'b>(
        &'b mut self,
        cancel: &'b dyn Fn() -> bool,
        progress: Option<&'b dyn Fn(RenderProgress)>,
    ) -> Execution<'b> {
        Execution {
            cache: Some(&mut *self.cache),
            cancel: Some(cancel),
            progress,
            ..Default::default()
        }
    }
}

/// Estimated GPU allocations for the remaining operations and the delivered view.
struct Reservation {
    costs: Vec<NodeCost>,
    delivery_bytes: u64,
}

impl Reservation {
    /// Peak memory the remaining nodes and the delivered view need beyond what is live.
    fn remaining_bytes(&self, next_node: usize) -> Option<u64> {
        super::allocation::peak_bytes(&self.costs[next_node.min(self.costs.len())..])?
            .checked_add(self.delivery_bytes)
    }
}

/// Unfinished submitted storage must survive abandonment or a panic.
struct RetireOnDrop {
    retirement: crate::gpu::Retirement,
    armed: bool,
}

impl Drop for RetireOnDrop {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.retirement.wait();
        }
    }
}

/// One render's prepared recipe and graph traversal. It borrows its input for
/// each call, so the same steps serve fresh sources, borrowed working photos and the
/// working set. Every started execution must be finished.
pub(super) struct RecipeExecution {
    // Fields drop in declaration order, so this waits before the traversal and
    // prepared recipe release their storage.
    unretired: RetireOnDrop,
    owner: Arc<()>,
    prepared: PreparedRecipe,
    plan: ArtifactPlan,
    traversal: Traversal,
    profile: Profile,
    context: Option<Context>,
    complete: bool,
    reservation: Option<Reservation>,
    /// GPU submissions when the last tick completed. Later ones are not yet
    /// retired, and the last tick's own may still be in flight.
    retired_submissions: u64,
    // Operations own their prepared parameters. Retain the request's immutable
    // calibration snapshot for the entire execution as well.
    _calibration: Option<Arc<crate::noise_calibration::Calibration>>,
}

impl RecipeExecution {
    fn begin(
        engine: &Engine,
        source: RecipeInput<'_>,
        settings: &Settings,
        recipe: Recipe<'_>,
        profile: &mut Profile,
        execution: &mut Execution<'_>,
    ) -> Result<Self> {
        let submissions = engine.gpu_activity().submissions;
        let names = match recipe {
            Recipe::Photographic(names) => names,
            Recipe::DngBase => &["final"],
        };
        let graph_id =
            (profile.mode.enabled() || diagnostics::enabled()).then(crate::profile::next_graph_id);
        let started = (|| {
            let mut prepared =
                engine.prepare_recipe(source, settings, recipe, profile, execution)?;
            let plan = engine.plan_artifacts(
                &prepared.settings,
                &mut prepared.built.graph,
                prepared.built.final_id,
                names,
                profile,
                execution,
            )?;
            let traversal = engine
                .start_graph(
                    source.context(),
                    &prepared.built.graph,
                    &plan.roots,
                    profile,
                    execution,
                    graph_id,
                )
                .map_err(|error| {
                    report_graph_failure(
                        error,
                        source.metadata(),
                        &prepared.built.graph,
                        &plan.roots,
                    )
                })?;
            Ok((prepared, plan, traversal))
        })();
        let (prepared, plan, traversal) = match started {
            Ok(started) => started,
            Err(error) => {
                // Recipe preparation may submit helper work. Retire it before the
                // caller finalizes cache state and releases its input.
                if engine.gpu_activity().submissions != submissions {
                    engine.wait_for_submitted_work()?;
                }
                return Err(error);
            }
        };
        let reservation = estimate(&prepared, &traversal, source.context());
        Ok(Self {
            owner: engine.owner.clone(),
            prepared,
            plan,
            traversal,
            profile: std::mem::take(profile),
            context: diagnostics::current_context(),
            complete: false,
            reservation,
            retired_submissions: submissions,
            unretired: RetireOnDrop {
                retirement: engine.gpu.retirement(),
                armed: true,
            },
            _calibration: engine.noise_calibration.clone(),
        })
    }

    fn check_owner(&self, engine: &Engine) -> Result<()> {
        if Arc::ptr_eq(&self.owner, &engine.owner) {
            Ok(())
        } else {
            Err("render execution belongs to another engine".into())
        }
    }

    fn scope(&self, cache: Option<&Cache>, name: &'static str) -> diagnostics::Scope {
        graph_scope(
            &self.prepared.built.graph,
            &self.plan.roots,
            cache,
            self.traversal.graph_id(),
            name,
        )
    }

    /// Run skipped nodes and at most one required node without waiting for the GPU.
    fn step(
        &mut self,
        engine: &Engine,
        source: RecipeInput<'_>,
        execution: &mut Execution<'_>,
    ) -> Result<bool> {
        self.check_owner(engine)?;
        let Self {
            prepared,
            plan,
            traversal,
            profile,
            context,
            complete,
            ..
        } = self;
        let more = diagnostics::with_context(*context, || {
            traversal
                .step(
                    engine,
                    source.context(),
                    &prepared.built.graph,
                    profile,
                    execution,
                )
                .map_err(|error| {
                    report_graph_failure(
                        error,
                        source.metadata(),
                        &prepared.built.graph,
                        &plan.roots,
                    )
                })
        })?;
        *complete = !more;
        Ok(more)
    }

    /// Call after the tick's GPU wait, before stepping again or finishing.
    fn complete_tick(
        &mut self,
        engine: &Engine,
        waited: Duration,
        progress: Option<&dyn Fn(RenderProgress)>,
    ) {
        let Self {
            traversal,
            profile,
            context,
            ..
        } = self;
        diagnostics::with_context(*context, || {
            traversal.complete_tick(waited, profile, progress)
        });
        self.retired_submissions = engine.gpu_activity().submissions;
    }

    /// Step alone, with a tick around each step.
    fn run_alone(
        &mut self,
        engine: &Engine,
        progress: Option<&dyn Fn(RenderProgress)>,
        mut step: impl FnMut(&mut Self) -> Result<bool>,
    ) -> Result<()> {
        loop {
            let tick = engine.tick();
            let more = step(self)?;
            self.complete_tick(engine, tick.complete(!more)?, progress);
            if !more {
                return Ok(());
            }
        }
    }

    fn finish(
        self,
        engine: &Engine,
        source: RecipeInput<'_>,
        outcome: Result<()>,
        profile: &mut Profile,
    ) -> Result<Vec<Artifact>> {
        self.check_owner(engine)?;
        let Self {
            prepared,
            plan,
            traversal,
            profile: mut own_profile,
            context,
            complete,
            retired_submissions,
            mut unretired,
            ..
        } = self;
        let mut outcome = outcome.and_then(|()| {
            if complete {
                Ok(())
            } else {
                Err("graph execution has not completed".into())
            }
        });
        // Keep the traversal's storage alive until submitted work has retired.
        if engine.gpu_activity().submissions != retired_submissions
            || engine.gpu.has_work_in_flight()
        {
            if let Err(error) = engine.gpu.settle() {
                outcome = Err(error);
            }
        }
        // Finishing profiling or packaging can itself submit GPU work. Retain
        // graph storage across those fallible, consuming operations as well.
        let retained: Vec<_> = traversal.buffers().cloned().collect();
        let finalizing_submissions = engine.gpu_activity().submissions;
        let values = diagnostics::with_context(context, || {
            outcome?;
            traversal
                .finish(engine, &plan.roots, &mut own_profile)
                .map_err(|error| {
                    report_graph_failure(
                        error,
                        source.metadata(),
                        &prepared.built.graph,
                        &plan.roots,
                    )
                })
        });
        let mut result = diagnostics::with_context(context, || -> Result<Vec<Artifact>> {
            let mut artifacts = engine.package_artifacts(
                source,
                &prepared.settings,
                &prepared.built.graph,
                plan,
                values?,
            )?;
            prepared.compose(&mut artifacts);
            Ok(artifacts)
        });
        if result.is_err() && engine.gpu_activity().submissions != finalizing_submissions {
            if let Err(error) = engine.wait_for_submitted_work() {
                result = Err(error);
            }
        }
        drop(retained);
        unretired.armed = false;
        *profile = own_profile;
        result
    }

    fn account(&self, retained: &mut crate::recipe::RetainedBytes) -> bool {
        retained.add_bytes(size_of::<Self>());
        let mut complete = self.prepared.account(retained).is_some();
        self.plan.account(retained);
        self.profile.account(retained);
        if let Some(calibration) = &self._calibration {
            retained.owner(calibration.clone(), calibration.retained_bytes());
        }
        complete &= self.traversal.account(retained).is_some();
        if let Some(reservation) = &self.reservation {
            retained.add_bytes(reservation.costs.capacity() * size_of::<NodeCost>());
        }
        complete
    }

    fn future_bytes(&self) -> Option<u64> {
        self.reservation
            .as_ref()?
            .remaining_bytes(self.traversal.next_node())
    }

    fn buffers(&self, frontend: Option<&MemoryReservation>) -> MemoryReservation {
        let mut reservation = frontend.cloned().unwrap_or_default();
        for buffer in self.traversal.buffers() {
            reservation.add_buffer(buffer);
        }
        reservation
    }
}

fn estimate(
    prepared: &PreparedRecipe,
    traversal: &Traversal,
    context: Input<'_>,
) -> Option<Reservation> {
    let costs = traversal.allocation_plan(&prepared.built.graph, context)?;
    // The caller creates one unresized view. Its texture and uniform are a
    // future charge, separate from the graph's retained photographic buffer.
    let extent = prepared.built.params.output[..2].try_into().ok()?;
    let delivery_bytes = super::allocation::image_bytes(extent, 4)?.checked_add(
        super::allocation::uploaded_bytes::<crate::nodes::sensor::calibration::EncodeParams>(1)?,
    )?;
    let reservation = Reservation {
        costs,
        delivery_bytes,
    };
    reservation.remaining_bytes(0)?;
    Some(reservation)
}

/// A recipe execution on a guarded working photo.
pub struct RenderExecution {
    // Fields drop in order: submitted work retires before the guard is released.
    recipe: Box<RecipeExecution>,
    guard: Guard,
    key: PhotoKey,
    requested: Box<Settings>,
    /// Stamps this render's restart inputs when its use counts as an edit.
    pub(super) edit_use: Option<u64>,
}

pub enum RenderExecutionStart {
    Ready(RenderExecution),
    Missing,
    NeedsOriginal(NeedsOriginal),
    Superseded,
}

impl Engine {
    #[allow(clippy::too_many_arguments)]
    pub fn begin_render_execution(
        &self,
        working: &mut WorkingSet,
        key: PhotoKey,
        settings: &Settings,
        names: &[&str],
        cancel: &dyn Fn() -> bool,
        profile: &mut Profile,
        progress: Option<&dyn Fn(RenderProgress)>,
    ) -> Result<RenderExecutionStart> {
        if !working.contains(key) {
            return Ok(RenderExecutionStart::Missing);
        }
        let guard = working.guard(key)?;
        let mut cache_started = false;
        let begun = (|| {
            let Some(mut session) =
                PhotoInput::prepare(working.execution_mut(&guard), self, settings, profile)?
            else {
                return Ok(None);
            };
            cache_started = true;
            let source = session.source;
            let mut execution = session.execution(cancel, progress);
            RecipeExecution::begin(
                self,
                source,
                settings,
                Recipe::Photographic(names),
                profile,
                &mut execution,
            )
            .map(Some)
        })();
        let error = match begun {
            Ok(Some(recipe)) => {
                return Ok(RenderExecutionStart::Ready(RenderExecution {
                    recipe: Box::new(recipe),
                    guard,
                    key,
                    requested: Box::new(settings.clone()),
                    edit_use: None,
                }))
            }
            Ok(None) => None,
            Err(error) => Some(error),
        };
        if cache_started {
            match working.execution_mut(&guard) {
                WorkingPhoto::Native(session) => {
                    profile.measure("finish session cache", || session.cache.finish());
                }
                WorkingPhoto::Reduced(session) => session.cache.finish(),
            }
        }
        drop(guard);
        let Some(error) = error else {
            return Ok(RenderExecutionStart::NeedsOriginal(
                NeedsOriginal::EngineChanged,
            ));
        };
        match classify(Err(error), key.resolution)? {
            PhotoRender::NeedsOriginal(reason) => Ok(RenderExecutionStart::NeedsOriginal(reason)),
            PhotoRender::Superseded => Ok(RenderExecutionStart::Superseded),
            PhotoRender::Rendered(_) => unreachable!("preparation failed"),
        }
    }

    /// Count retained processing, frontend and live execution owners together.
    /// Shared storage and repeated execution handles are counted once. This is
    /// a known subtotal. Future allocations and operation temporaries need headroom.
    pub fn execution_memory_resources(
        &self,
        working: &WorkingSet,
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
            reservation.extend_shared(&execution.recipe.buffers(None));
        }
        self.memory_resources_with(working, &reservation, |retained| {
            for execution in unique {
                execution.recipe.account(retained);
            }
        })
    }
}

impl RenderExecution {
    /// Run skipped nodes and at most one required node. The caller completes the tick.
    pub fn step(
        &mut self,
        engine: &Engine,
        working: &mut WorkingSet,
        cancel: &dyn Fn() -> bool,
    ) -> Result<bool> {
        let mut session = PhotoInput::new(working.execution_mut(&self.guard));
        let source = session.source;
        self.recipe
            .step(engine, source, &mut session.execution(cancel, None))
    }

    /// Call after the tick's GPU wait, before stepping again or finishing.
    pub fn complete_tick(
        &mut self,
        engine: &Engine,
        waited: Duration,
        progress: &dyn Fn(RenderProgress),
    ) {
        self.recipe.complete_tick(engine, waited, Some(progress));
    }

    pub fn finish(
        self,
        engine: &Engine,
        working: &mut WorkingSet,
        outcome: Result<()>,
        profile: &mut Profile,
    ) -> Result<PhotoRender> {
        let Self {
            recipe,
            guard,
            key,
            requested,
            edit_use,
        } = self;
        let result = {
            let session = PhotoInput::new(working.execution_mut(&guard));
            recipe.finish(engine, session.source, outcome, profile)
        };
        match working.execution_mut(&guard) {
            WorkingPhoto::Native(session) => {
                profile.measure("finish session cache", || {
                    if result.is_ok() {
                        session
                            .cache
                            .commit(requested.preview_region.is_some(), edit_use);
                    }
                    session.cache.finish();
                });
            }
            WorkingPhoto::Reduced(session) => {
                if result.is_ok() {
                    session
                        .cache
                        .commit(requested.preview_region.is_some(), edit_use);
                    session.scale = requested.working_scale;
                    session.last_settings = Some(requested);
                }
                session.cache.finish();
            }
        }
        drop(guard);
        classify(result, key.resolution)
    }

    pub(super) fn future_bytes(&self) -> Option<u64> {
        self.recipe.future_bytes()
    }

    pub(super) fn delivery_bytes(&self) -> Option<u64> {
        Some(self.recipe.reservation.as_ref()?.delivery_bytes)
    }
}

fn classify(result: Result<Vec<Artifact>>, kind: Resolution) -> Result<PhotoRender> {
    match result {
        Ok(artifacts) => Ok(PhotoRender::Rendered(artifacts)),
        Err(error) if kind == Resolution::Reduced && error.is::<SourceRequired>() => {
            Ok(PhotoRender::NeedsOriginal(NeedsOriginal::UpstreamChanged))
        }
        Err(error) if error.is::<Superseded>() => Ok(PhotoRender::Superseded),
        Err(error) => Err(error),
    }
}

/// Render a working photo alone, one step per tick.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_working_photo(
    engine: &Engine,
    working: &mut WorkingSet,
    key: PhotoKey,
    settings: &Settings,
    names: &[&str],
    purpose: Purpose,
    cancel: &dyn Fn() -> bool,
    profile: &mut Profile,
    progress: Option<&dyn Fn(RenderProgress)>,
) -> Result<Option<PhotoRender>> {
    let edit_use = working.record_use(key, purpose);
    let mut render = match engine
        .begin_render_execution(working, key, settings, names, cancel, profile, progress)?
    {
        RenderExecutionStart::Ready(mut render) => {
            render.edit_use = edit_use;
            render
        }
        RenderExecutionStart::Missing => return Ok(None),
        RenderExecutionStart::NeedsOriginal(reason) => {
            return Ok(Some(PhotoRender::NeedsOriginal(reason)))
        }
        RenderExecutionStart::Superseded => return Ok(Some(PhotoRender::Superseded)),
    };
    let scope = render.recipe.scope(
        Some(working.execution(&render.guard).cache()),
        "graph.execute",
    );
    let outcome = render.recipe.run_alone(engine, progress, |recipe| {
        let mut session = PhotoInput::new(working.execution_mut(&render.guard));
        let source = session.source;
        recipe.step(engine, source, &mut session.execution(cancel, None))
    });
    drop(scope);
    render.finish(engine, working, outcome, profile).map(Some)
}
