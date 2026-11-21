//! Owns render coordination on a dedicated thread and publishes bounded UI updates.

use super::{
    coordinator::{Photos, RenderCoordinator, TaskId, WorkPlan},
    request::RenderRequest,
    view_plan::{ImageState, Planned},
    worker::{
        self, ColorPickRequest, MemoryTarget, Pending, PreparationConfig, ScopeRequest, Update,
    },
};
use crate::rendering::request::RequestId;
use crossbeam_channel::{Receiver, Select, Sender};
use eframe::egui;
use foveonic::priority::Priority;
use foveonic::{decode_cache::Cache, source::SourceOrigin};
use std::{
    cell::{Cell, RefCell},
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

type Slot = Arc<(Mutex<Pending>, Condvar)>;

enum Command {
    Clear(bool),
    Plan(Photos, Vec<Planned>, Instant),
    Scopes(Box<ScopeRequest>),
    Pick(Box<ColorPickRequest>, RequestId, Option<SourceOrigin>),
    WholeDelivered(RequestId),
    Memory(MemoryTarget),
    Preparation(Arc<PreparationConfig>),
    #[cfg(test)]
    RenderWidth(Option<usize>),
    Stop,
}

#[derive(Default)]
struct Status {
    processed: u64,
    busy: bool,
    admitted: Vec<(TaskId, RequestId)>,
}

pub(super) struct Handle {
    commands: Sender<(u64, Command)>,
    sequence: Cell<u64>,
    pending: Cell<u64>,
    status: Arc<Mutex<Arc<Status>>>,
    observed: RefCell<Arc<Status>>,
    /// The last published planned photos and images in use, to skip unchanged publication.
    planned: RefCell<(Photos, Vec<Planned>)>,
    preparation: RefCell<Option<Arc<PreparationConfig>>>,
    #[cfg(test)]
    render_width: Cell<Option<usize>>,
}

impl Handle {
    pub fn start(
        slot: Slot,
        results: Receiver<Update>,
        cache: Arc<Cache>,
        progress: crate::progress::Tracker,
        context: egui::Context,
        loader: super::source_loader::Service,
    ) -> (Self, Receiver<Update>) {
        let (commands, input) = crossbeam_channel::unbounded();
        let (output, updates) = crossbeam_channel::bounded(2);
        let status = Arc::new(Mutex::new(Arc::new(Status::default())));
        let published = status.clone();
        #[cfg(test)]
        let render_width = slot.0.lock().unwrap().render_width;
        std::thread::Builder::new()
            .name("render-coordinator".into())
            .spawn(move || {
                let mut runtime = Runtime {
                    coordinator: Default::default(),
                    slot,
                    cache,
                    progress,
                    context,
                    status: published,
                    processed: 0,
                    loader,
                    tracked: Vec::new(),
                    pass: None,
                };
                runtime.run(input, results, output);
            })
            .expect("start render-coordinator thread");
        (
            Self {
                commands,
                sequence: Cell::new(0),
                pending: Cell::new(0),
                status,
                observed: Default::default(),
                planned: Default::default(),
                preparation: Default::default(),
                #[cfg(test)]
                render_width: Cell::new(render_width),
            },
            updates,
        )
    }

    fn send(&self, command: Command, changes_work: bool) {
        let sequence = self.sequence.get() + 1;
        self.sequence.set(sequence);
        if changes_work {
            self.pending.set(sequence);
        }
        let _ = self.commands.send((sequence, command));
    }

    fn status(&self) -> Arc<Status> {
        // A paint can use its last snapshot while the coordinator publishes.
        if let Ok(status) = self.status.try_lock() {
            *self.observed.borrow_mut() = status.clone();
        }
        self.observed.borrow().clone()
    }

    pub fn busy(&self) -> bool {
        let status = self.status();
        self.pending.get() > status.processed || status.busy
    }

    #[cfg(test)]
    pub fn set_render_width(&self, width: Option<usize>) {
        let width = width.map(|value| value.clamp(1, 8));
        if self.render_width.replace(width) != width {
            self.send(Command::RenderWidth(width), false);
        }
    }

    pub fn is_settled_and_admitted(&self, frame: &worker::RenderResult) -> bool {
        let status = self.status();
        self.pending.get() <= status.processed
            && status
                .admitted
                .contains(&(TaskId(frame.diagnostic_task.id), frame.request.id))
    }

    pub fn clear(&self, all: bool) {
        *self.planned.borrow_mut() = Default::default();
        self.send(Command::Clear(all), true);
    }
    /// Publish the images in use for the planned photos, in priority order.
    /// A photo planned before that is neither planned nor `deferred` now uses no
    /// images, so its work stops. Unchanged publications are skipped; only `clear`
    /// drops the coordinator's planned work, and it forgets the last publication too.
    pub fn plan(&self, photos: Photos, planned: Vec<Planned>, queued_at: Instant) -> bool {
        let mut previous = self.planned.borrow_mut();
        let unchanged = previous.0 == photos
            && previous.1.len() == planned.len()
            && previous.1.iter().zip(&planned).all(|(old, new)| {
                old.speculative == new.speculative
                    && old.state == new.state
                    && super::matching::same_delivery_request(&old.request, &new.request)
            });
        if unchanged {
            return false;
        }
        foveonic::diagnostics::event(
            foveonic::diagnostics::Category::Coordinator,
            "view.plan",
            |fields| {
                let images: Vec<_> = planned
                    .iter()
                    .map(|planned| {
                        serde_json::json!({"source":planned.request.path,
                        "region":planned.request.settings.preview_region,
                        "scale":planned.request.preview_scale.divisor(),
                        "detail":planned.request.detail,
                        "speculative":planned.speculative,"state":format!("{:?}",planned.state)})
                    })
                    .collect();
                fields
                    .str(
                        "photos",
                        &serde_json::to_string(&photos.planned).expect("plan diagnostics"),
                    )
                    .str(
                        "images",
                        &serde_json::to_string(&images).expect("plan diagnostics"),
                    );
            },
        );
        *previous = (photos.clone(), planned.clone());
        self.send(Command::Plan(photos, planned, queued_at), true);
        true
    }
    /// The region of the patch last published for a photo, for scenario checks.
    pub fn planned_patch(&self, path: &std::path::Path) -> Option<[u32; 4]> {
        self.planned
            .borrow()
            .1
            .iter()
            .find(|planned| {
                planned.state == ImageState::Render
                    && planned.request.detail
                    && planned.request.path == path
            })
            .and_then(|planned| planned.request.settings.preview_region)
    }
    pub fn scopes(&self, request: ScopeRequest) {
        self.send(Command::Scopes(Box::new(request)), true);
    }
    pub fn pick(
        &self,
        request: ColorPickRequest,
        image_request_id: RequestId,
        origin: Option<SourceOrigin>,
    ) {
        self.send(
            Command::Pick(Box::new(request), image_request_id, origin),
            true,
        );
    }
    pub fn whole_delivered(&self, request_id: RequestId) {
        self.send(Command::WholeDelivered(request_id), false);
    }
    pub fn memory(&self, bytes: u64, frontend: crate::memory::Inventory) {
        self.send(Command::Memory(MemoryTarget { bytes, frontend }), false);
    }
    pub fn preparation(&self, config: PreparationConfig) {
        let mut previous = self.preparation.borrow_mut();
        if previous.as_deref() != Some(&config) {
            let config = Arc::new(config);
            *previous = Some(config.clone());
            self.send(Command::Preparation(config), false);
        }
    }
}

#[cfg(test)]
#[path = "runtime_test.rs"]
mod tests;

impl Drop for Handle {
    fn drop(&mut self) {
        self.send(Command::Stop, false);
    }
}

struct Runtime {
    coordinator: RenderCoordinator,
    slot: Slot,
    cache: Arc<Cache>,
    progress: crate::progress::Tracker,
    context: egui::Context,
    status: Arc<Mutex<Arc<Status>>>,
    processed: u64,
    loader: super::source_loader::Service,
    /// The latest plan's tracked images, which the progress pass follows.
    tracked: Vec<(super::coordinator::Target, RequestId, ImageState)>,
    /// When the progress pass started and finished.
    pass: Option<(Instant, Option<Instant>)>,
}

impl Runtime {
    fn run(
        &mut self,
        commands: Receiver<(u64, Command)>,
        results: Receiver<Update>,
        output: Sender<Update>,
    ) {
        let source_signals = self.loader.signals.clone();
        let loaded_sources = self.loader.results.clone();
        let mut waiting = None;
        let mut handled = false;
        loop {
            // Apply queued intent before exposing another plan or handling a result.
            let mut changed = false;
            for (sequence, command) in commands.try_iter().take(64) {
                if !self.command(sequence, command) {
                    self.stop();
                    return;
                }
                changed = true;
            }
            if changed {
                self.publish();
                self.context.request_repaint();
            }
            if !handled {
                if let Some(update) = waiting.take() {
                    waiting = self.handle(update);
                    handled = waiting.is_some();
                }
            }
            let mut select = Select::new();
            let command = select.recv(&commands);
            let source_request = select.recv(&source_signals);
            let source_result = select.recv(&loaded_sources);
            let receive = waiting.is_none().then(|| select.recv(&results));
            let send = waiting.is_some().then(|| select.send(&output));
            let operation = select.select();
            let index = operation.index();
            if index == command {
                match operation.recv(&commands) {
                    Ok((sequence, command)) => {
                        if !self.command(sequence, command) {
                            self.stop();
                            return;
                        }
                        self.publish();
                        self.context.request_repaint();
                    }
                    _ => {
                        self.stop();
                        return;
                    }
                }
            } else if index == source_request {
                match operation.recv(&source_signals) {
                    Ok(signal) => {
                        self.loader.signal(signal);
                        self.prioritize_sources();
                    }
                    Err(_) => {
                        self.stop();
                        return;
                    }
                }
            } else if index == source_result {
                match operation.recv(&loaded_sources) {
                    Ok(source) => {
                        self.loader.completed(source);
                        self.prioritize_sources();
                    }
                    Err(_) => {
                        self.stop();
                        return;
                    }
                }
            } else if Some(index) == receive {
                match operation.recv(&results) {
                    Ok(update) => {
                        waiting = Some(update);
                        handled = false;
                    }
                    Err(_) => {
                        self.stop();
                        return;
                    }
                }
            } else if Some(index) == send {
                if operation.send(&output, waiting.take().unwrap()).is_err() {
                    self.stop();
                    return;
                }
                handled = false;
                self.context.request_repaint();
            }
        }
    }

    fn command(&mut self, sequence: u64, command: Command) -> bool {
        let _scope = foveonic::diagnostics::scope(
            foveonic::diagnostics::Category::Coordinator,
            "coordinator.command",
        );
        self.processed = sequence;
        match command {
            Command::Clear(all) => {
                self.coordinator.clear();
                self.progress.clear();
                self.tracked.clear();
                self.pass = None;
                let mut pending = self.slot.0.lock().unwrap();
                pending.clear = true;
                pending.clear_all |= all;
            }
            Command::Plan(photos, planned, queued_at) => {
                // The progress pass follows the plan's tracked images; a pass
                // starts when new work begins after the last one finished.
                let mut tracked = Vec::new();
                let mut new_work = false;
                for planned in &planned {
                    let target = super::coordinator::Target::of(&planned.request);
                    if planned.state == ImageState::Render {
                        let origin = self.cache.source_origin(&planned.request.path);
                        new_work |= self.desire(
                            planned.request.clone(),
                            origin,
                            queued_at,
                            planned.speculative,
                        );
                    }
                    new_work |= planned.state == ImageState::Waiting;
                    if let Some(place) = planned.tracked {
                        tracked.push((place, target, planned.request.id, planned.state));
                    }
                }
                tracked.sort_by_key(|(place, ..)| *place);
                self.tracked = tracked
                    .into_iter()
                    .map(|(_, target, id, state)| (target, id, state))
                    .collect();
                if new_work && self.pass.is_none_or(|(_, finished)| finished.is_some()) {
                    self.pass = Some((queued_at, None));
                }
                let in_use: Vec<_> = planned.into_iter().map(|planned| planned.request).collect();
                self.coordinator.reconcile(&photos, &in_use);
            }
            Command::Scopes(request) => self.coordinator.scopes(*request),
            Command::Pick(request, image_request_id, origin) => {
                self.coordinator.pick(*request, image_request_id, origin)
            }
            Command::WholeDelivered(request_id) => self.coordinator.whole_delivered(request_id),
            Command::Memory(target) => self.slot.0.lock().unwrap().memory = Some(target),
            #[cfg(test)]
            Command::RenderWidth(width) => self.slot.0.lock().unwrap().render_width = width,
            Command::Preparation(config) => {
                let mut pending = self.slot.0.lock().unwrap();
                if pending.preparation.as_deref() != Some(&config) {
                    pending.preparation = Some(config);
                }
            }
            Command::Stop => return false,
        }
        true
    }

    /// Want this request; whether it starts new work rather than joining work
    /// already wanted for its target.
    fn desire(
        &mut self,
        request: RenderRequest,
        origin: Option<SourceOrigin>,
        queued_at: Instant,
        speculative: bool,
    ) -> bool {
        let target = super::coordinator::Target::of(&request);
        let previous = self
            .coordinator
            .desired(&target)
            .map(|task| task.stamp.clone());
        self.coordinator
            .desire(Arc::new(request), origin, queued_at, speculative);
        // New work, rather than a request joining work already wanted.
        let started = self
            .coordinator
            .desired(&target)
            .is_some_and(|task| Some(&task.stamp) != previous.as_ref());
        started
    }

    fn handle(&mut self, mut update: Update) -> Option<Update> {
        let _scope = foveonic::diagnostics::scope(
            foveonic::diagnostics::Category::Coordinator,
            "coordinator.result",
        );
        match &mut update {
            Update::Image(reply) if reply.handling.is_none() => {
                reply.handling = Some(self.coordinator.handle_result(reply));
            }
            Update::Scopes { id, .. } => {
                if !self.coordinator.finish_scope(*id) {
                    self.publish();
                    return None;
                }
            }
            Update::ColorPick { id, .. } => {
                if !self.coordinator.finish_pick(*id) {
                    self.publish();
                    return None;
                }
            }
            _ => return Some(update),
        }
        self.publish();
        Some(update)
    }

    fn work_plan(&self) -> Arc<WorkPlan> {
        self.coordinator.plan()
    }

    /// Publish the progress pass from the tracked images and the coordinator:
    /// it finishes once every section is current, done or failed.
    fn publish_progress(&mut self) {
        use crate::progress::{Section, SectionState};
        // A finished pass stays as it ended until new work starts another.
        let Some((started, finished @ None)) = self.pass.as_mut() else {
            return;
        };
        let sections: Vec<_> = self
            .tracked
            .iter()
            .map(|(target, planned_id, state)| {
                let (request_id, state) = match state {
                    ImageState::Ready => (*planned_id, SectionState::Ready),
                    ImageState::Waiting => (*planned_id, SectionState::Waiting),
                    ImageState::Render => match self.coordinator.target_progress(target) {
                        Some((id, _, true)) => (id, SectionState::Failed),
                        Some((id, true, false)) => (id, SectionState::Done),
                        Some((id, false, false)) => (id, SectionState::Running),
                        None => (*planned_id, SectionState::Done),
                    },
                };
                Section {
                    request_id: request_id.0,
                    state,
                }
            })
            .collect();
        let settled = sections.iter().all(|section| {
            matches!(
                section.state,
                SectionState::Ready | SectionState::Done | SectionState::Failed
            )
        });
        if finished.is_none() && settled {
            *finished = Some(Instant::now());
        }
        self.progress.publish(sections, *started, *finished);
    }

    fn publish(&mut self) {
        self.publish_progress();
        let plan = self.work_plan();
        let mut wanted = plan
            .tasks
            .iter()
            .map(|task| task.request.path.clone())
            .collect::<Vec<_>>();
        wanted.extend(plan.pick.iter().map(|task| task.request.path.clone()));
        self.loader.retain(wanted);
        self.prioritize_sources();
        self.loader.admit();
        let retained = self.coordinator.retained_images();
        foveonic::diagnostics::counter(
            foveonic::diagnostics::Category::Coordinator,
            "pending_render_targets",
            plan.tasks.len() as f64,
        );
        foveonic::diagnostics::event(
            foveonic::diagnostics::Category::Coordinator,
            "work_plan_published",
            |fields| {
                fields.u64("render_targets", plan.tasks.len() as u64);
                fields.bool("scopes", plan.scopes.is_some());
                fields.bool("pick", plan.pick.is_some());
            },
        );
        {
            let mut pending = self.slot.0.lock().unwrap();
            pending.activity = (plan.tasks.iter().any(|task| task.viewport())
                || plan.pick.is_some())
            .then(|| self.cache.priority().demand(Priority::Viewport));
            pending.auxiliary_activity = (plan.scopes.is_some()
                || plan
                    .tasks
                    .iter()
                    .any(|task| plan.priority(task) == Priority::Auxiliary))
            .then(|| self.cache.priority().demand(Priority::Auxiliary));
            pending.thumbnail_activity = plan
                .tasks
                .iter()
                .any(|task| plan.priority(task) == Priority::Thumbnail)
                .then(|| self.cache.priority().demand(Priority::Thumbnail));
            pending.plan = plan;
            pending.retained = retained.into();
        }
        *self.status.lock().unwrap() = Arc::new(Status {
            processed: self.processed,
            busy: self.coordinator.busy(),
            admitted: self.coordinator.admitted_images(),
        });
        self.slot.1.notify_one();
    }

    fn prioritize_sources(&mut self) {
        let plan = self.work_plan();
        let mut priorities = plan
            .tasks
            .iter()
            .map(|task| (task.request.path.clone(), plan.priority(task)))
            .collect::<Vec<_>>();
        priorities.extend(
            plan.pick
                .iter()
                .map(|task| (task.request.path.clone(), Priority::Viewport)),
        );
        self.loader.prioritize(&priorities);
    }

    fn stop(&self) {
        self.slot.0.lock().unwrap().stop = true;
        self.slot.1.notify_one();
    }
}
