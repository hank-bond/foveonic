//! Owns render coordination on a dedicated thread and publishes bounded UI updates.

use super::{
    coordinator::{RenderCoordinator, TaskId},
    delivery::ViewState,
    request::{Refinement, Request},
    worker::{
        self, ColorPickRequest, MemoryBudget, Pending, PreparationConfig, ScopeRequest, Update,
    },
};
use crossbeam_channel::{Receiver, Select, Sender};
use eframe::egui;
use foveonic::{decode_cache::Cache, source::SourceOrigin};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

type Slot = Arc<(Mutex<Pending>, Condvar)>;

enum Command {
    View(Arc<ViewState>),
    Activate(PathBuf),
    RetireBefore(PathBuf),
    RetireDetail(PathBuf),
    Clear(bool),
    Submit(Box<Request>, Instant),
    Refine(Box<Refinement>, Option<SourceOrigin>, Instant),
    CancelRefinement(PathBuf, crate::editor::Rendition),
    Overviews(Vec<Request>, Vec<PathBuf>, Vec<PathBuf>, Instant),
    Retain(Vec<PathBuf>, Vec<PathBuf>),
    Scopes(Box<ScopeRequest>),
    Pick(Box<ColorPickRequest>, u64, Option<SourceOrigin>),
    Delivered(u64),
    Memory(MemoryBudget),
    Preparation(Arc<PreparationConfig>),
    Stop,
}

#[derive(Default)]
struct Status {
    processed: u64,
    busy: bool,
    admitted: Vec<(TaskId, u64)>,
}

pub(super) struct Handle {
    commands: Sender<(u64, Command)>,
    sequence: Cell<u64>,
    pending: Cell<u64>,
    status: Arc<Mutex<Arc<Status>>>,
    observed: RefCell<Arc<Status>>,
    view: RefCell<Option<Arc<ViewState>>>,
    refinements: RefCell<Vec<(Refinement, Option<SourceOrigin>)>>,
    preparation: RefCell<Option<Arc<PreparationConfig>>>,
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
                    view: None,
                    loader,
                    exporting: None,
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
                view: Default::default(),
                refinements: Default::default(),
                preparation: Default::default(),
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

    pub fn is_settled_and_admitted(&self, frame: &worker::Frame) -> bool {
        let status = self.status();
        self.pending.get() <= status.processed
            && status.admitted.contains(&(
                TaskId(frame.diagnostic_task.id),
                frame.diagnostic_task.revision,
            ))
    }

    pub fn set_view(&self, view: ViewState) {
        let mut previous = self.view.borrow_mut();
        if previous.as_deref() != Some(&view) {
            let view = Arc::new(view);
            *previous = Some(view.clone());
            self.send(Command::View(view), false);
        }
    }

    pub fn activate(&self, path: PathBuf) {
        self.refinements.borrow_mut().retain(|(request, _)| {
            request.first.rendition == crate::editor::Rendition::Current
                || request.first.path == path
        });
        self.send(Command::Activate(path), true);
    }
    pub fn retire_before(&self, path: PathBuf) {
        self.refinements.borrow_mut().retain(|(request, _)| {
            request.first.path != path
                || request.first.rendition == crate::editor::Rendition::Current
        });
        self.send(Command::RetireBefore(path), true);
    }
    pub fn clear(&self, all: bool) {
        self.refinements.borrow_mut().clear();
        self.send(Command::Clear(all), true);
    }
    pub fn retire_detail(&self, path: PathBuf) {
        self.refinements.borrow_mut().retain(|(old, _)| {
            old.first.path != path || old.first.rendition != crate::editor::Rendition::Current
        });
        self.send(Command::RetireDetail(path), true);
    }
    pub fn submit(&self, request: Request, queued_at: Instant) {
        self.refinements.borrow_mut().retain(|(old, _)| {
            old.first.path != request.path || old.first.rendition != request.rendition
        });
        self.send(Command::Submit(Box::new(request), queued_at), true);
    }
    /// Suppress identical intent publication, as with immutable view snapshots.
    /// Sequencing and completion remain exclusively with the coordinator.
    pub fn refine(
        &self,
        request: Refinement,
        origin: Option<SourceOrigin>,
        queued_at: Instant,
    ) -> bool {
        let mut previous = self.refinements.borrow_mut();
        if let Some((old, old_origin)) = previous.iter_mut().find(|(old, old_origin)| {
            old.same_sequence(&request)
                && (old_origin.is_none() || origin.is_none() || *old_origin == origin)
        }) {
            if old_origin.is_none() && origin.is_some() {
                *old_origin = origin.clone();
                // Binding identity changes the desired snapshot, not the work
                // or presentation revision already published for this sequence.
                self.send(
                    Command::Refine(Box::new(old.clone()), origin, queued_at),
                    false,
                );
            }
            return false;
        }
        previous.retain(|(old, _)| {
            old.first.path != request.first.path || old.first.rendition != request.first.rendition
        });
        previous.push((request.clone(), origin.clone()));
        self.send(Command::Refine(Box::new(request), origin, queued_at), true);
        true
    }
    pub fn cancel_refinement(&self, path: PathBuf, rendition: crate::editor::Rendition) {
        let mut previous = self.refinements.borrow_mut();
        let count = previous.len();
        previous.retain(|(request, _)| {
            request.first.path != path || request.first.rendition != rendition
        });
        if previous.len() != count {
            self.send(Command::CancelRefinement(path, rendition), true);
        }
    }
    pub fn overviews(
        &self,
        requests: Vec<Request>,
        paths: Vec<PathBuf>,
        retained: Vec<PathBuf>,
        queued_at: Instant,
    ) {
        self.refinements.borrow_mut().retain(|(old, _)| {
            !requests.iter().any(|request| {
                request.path == old.first.path
                    && old.first.rendition == crate::editor::Rendition::Current
            })
        });
        self.send(
            Command::Overviews(requests, paths, retained, queued_at),
            true,
        );
    }
    pub fn retain(&self, paths: Vec<PathBuf>, retained: Vec<PathBuf>) {
        self.refinements.borrow_mut().retain(|(request, _)| {
            request.first.rendition != crate::editor::Rendition::Current
                || paths.contains(&request.first.path)
        });
        self.send(Command::Retain(paths, retained), true);
    }
    pub fn scopes(&self, request: ScopeRequest) {
        self.send(Command::Scopes(Box::new(request)), true);
    }
    pub fn pick(&self, request: ColorPickRequest, revision: u64, origin: Option<SourceOrigin>) {
        self.send(Command::Pick(Box::new(request), revision, origin), true);
    }
    pub fn delivered(&self, revision: u64) {
        self.send(Command::Delivered(revision), false);
    }
    pub fn memory(&self, limit: u64, frontend: crate::memory::Inventory) {
        self.send(Command::Memory(MemoryBudget { limit, frontend }), false);
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
    view: Option<Arc<ViewState>>,
    loader: super::source_loader::Service,
    exporting: Option<PathBuf>,
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
            if self.view.is_some() && !handled {
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
            let send = (waiting.is_some() && self.view.is_some()).then(|| select.send(&output));
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
            Command::View(view) => self.view = Some(view),
            Command::Activate(path) => self.coordinator.activate(&path),
            Command::RetireBefore(path) => self.coordinator.retire_before(&path),
            Command::RetireDetail(path) => self.coordinator.retire_detail(&path),
            Command::Clear(all) => {
                self.coordinator.clear();
                self.progress.clear();
                let mut pending = self.slot.0.lock().unwrap();
                pending.clear = true;
                pending.clear_all |= all;
            }
            Command::Submit(request, queued_at) => {
                let origin = self.cache.source_origin(&request.path);
                if request.export.is_some() {
                    self.exporting = Some(request.path.clone());
                    self.progress.begin(request.revision, queued_at);
                    let task =
                        self.coordinator
                            .frozen_export(Arc::new(*request), origin, queued_at);
                    self.slot.0.lock().unwrap().export = Some(task);
                } else {
                    self.desire(*request, origin, queued_at);
                }
            }
            Command::Refine(request, origin, queued_at) => {
                let origin = self.cache.source_origin(&request.first.path).or(origin);
                if let Some(started) = self.coordinator.refine(*request, origin, queued_at) {
                    self.progress
                        .begin(started.stamp.revision, started.queued_at);
                }
            }
            Command::CancelRefinement(path, rendition) => {
                self.coordinator.cancel_refinement(&path, rendition)
            }
            Command::Overviews(requests, paths, retained, queued_at) => {
                self.coordinator.retain_overviews(&paths, &retained);
                for request in requests {
                    let origin = self.cache.source_origin(&request.path);
                    self.desire(request, origin, queued_at);
                }
            }
            Command::Retain(paths, retained) => {
                self.coordinator.retain_overviews(&paths, &retained)
            }
            Command::Scopes(request) => self.coordinator.scopes(*request),
            Command::Pick(request, revision, origin) => {
                self.coordinator.pick(*request, revision, origin)
            }
            Command::Delivered(revision) => self.coordinator.delivered(revision),
            Command::Memory(budget) => self.slot.0.lock().unwrap().memory = Some(budget),
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

    fn desire(&mut self, request: Request, origin: Option<SourceOrigin>, queued_at: Instant) {
        let target = super::coordinator::Target::of(&request);
        let previous = self
            .coordinator
            .desired(&target)
            .map(|task| task.stamp.clone());
        let revision = request.revision;
        self.coordinator
            .desire(Arc::new(request), origin, queued_at);
        if self.coordinator.desired(&target).map(|task| &task.stamp) != previous.as_ref() {
            self.progress.begin(revision, queued_at);
        }
    }

    fn handle(&mut self, mut update: Update) -> Option<Update> {
        let _scope = foveonic::diagnostics::scope(
            foveonic::diagnostics::Category::Coordinator,
            "coordinator.result",
        );
        match &mut update {
            Update::Image(reply) if reply.handling.is_none() => {
                if reply.exported.is_some() {
                    self.exporting = None;
                }
                let view = self.view.as_ref().unwrap().context(&reply.path);
                let (handling, started) = self.coordinator.handle_result(reply, &view);
                reply.handling = Some(handling);
                if let Some(started) = started {
                    self.progress
                        .begin(started.stamp.revision, started.queued_at);
                }
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

    fn publish(&mut self) {
        let plan = self.coordinator.plan();
        let mut wanted = plan
            .tasks
            .iter()
            .map(|task| task.request.path.clone())
            .collect::<Vec<_>>();
        wanted.extend(plan.pick.iter().map(|task| task.request.path.clone()));
        wanted.extend(self.exporting.iter().cloned());
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
            pending.activity = (!plan.tasks.is_empty()
                || plan.scopes.is_some()
                || plan.pick.is_some()
                || pending.export.is_some())
            .then(|| self.cache.foreground());
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
        let plan = self.coordinator.plan();
        let mut foreground = plan
            .tasks
            .iter()
            .filter(|task| !task.speculative)
            .map(|task| task.request.path.clone())
            .collect::<Vec<_>>();
        foreground.extend(plan.pick.iter().map(|task| task.request.path.clone()));
        foreground.extend(self.exporting.iter().cloned());
        let speculative = plan
            .tasks
            .iter()
            .filter(|task| task.speculative)
            .map(|task| task.request.path.clone())
            .collect::<Vec<_>>();
        self.loader.prioritize(&foreground, &speculative);
    }

    fn stop(&self) {
        self.slot.0.lock().unwrap().stop = true;
        self.slot.1.notify_one();
    }
}
