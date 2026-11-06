//! Owns desired photo renders and publishes one immutable plan to the serialized executor.

use crate::editor::Rendition;
use crate::worker::{ColorPickRequest, Request, ScopeRequest};
use foveonic::source::SourceOrigin;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Purpose {
    Current,
    Camera,
    Defaults,
    Detail,
    Inspection(String, bool),
    Export,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Target {
    pub path: PathBuf,
    pub purpose: Purpose,
}
impl Target {
    pub fn of(request: &Request) -> Self {
        let purpose = if request.export.is_some() {
            Purpose::Export
        } else {
            match request.rendition {
                Rendition::Camera => Purpose::Camera,
                Rendition::Defaults => Purpose::Defaults,
                Rendition::Current if request.output != "final" || request.comparison => {
                    Purpose::Inspection(request.output.clone(), request.comparison)
                }
                Rendition::Current if request.settings.preview_region.is_some() => Purpose::Detail,
                Rendition::Current => Purpose::Current,
            }
        };
        Self {
            path: request.path.clone(),
            purpose,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Stamp {
    pub id: TaskId,
    pub target: Target,
    pub revision: u64,
}
#[derive(Clone)]
pub struct Task {
    pub stamp: Stamp,
    pub request: Arc<Request>,
    pub origin: Option<SourceOrigin>,
    pub queued_at: Instant,
    pub seed: Option<Arc<crate::worker::presentation::Snapshot>>,
}
#[derive(Clone)]
pub struct ScopeTask {
    pub id: TaskId,
    pub request: Arc<ScopeRequest>,
}
#[derive(Clone)]
pub struct PickTask {
    pub id: TaskId,
    pub image_revision: u64,
    pub origin: Option<SourceOrigin>,
    pub request: Arc<ColorPickRequest>,
}
#[derive(Clone, Default)]
pub struct WorkPlan {
    pub tasks: Vec<Arc<Task>>,
    pub scopes: Option<Arc<ScopeTask>>,
    pub pick: Option<Arc<PickTask>>,
}
impl WorkPlan {
    pub fn task(&self, target: &Target) -> Option<&Arc<Task>> {
        self.tasks.iter().find(|task| &task.stamp.target == target)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    Current,
    Superseded,
    Removed,
}
struct Desired {
    task: Arc<Task>,
    settled: bool,
    admitted: u64,
    delivered: Option<(TaskId, Arc<crate::worker::presentation::Snapshot>)>,
}
#[derive(Default)]
pub struct RenderCoordinator {
    next_id: u64,
    desired: Vec<Desired>,
    scopes: Option<Arc<ScopeTask>>,
    pick: Option<Arc<PickTask>>,
    active: Option<PathBuf>,
    resource_revision: u64,
}
impl RenderCoordinator {
    fn id(&mut self) -> TaskId {
        self.next_id += 1;
        TaskId(self.next_id)
    }
    pub fn desire(
        &mut self,
        request: Arc<Request>,
        origin: Option<SourceOrigin>,
        queued_at: Instant,
    ) -> TaskId {
        let target = Target::of(&request);
        self.desired.retain(|entry| {
            if target.purpose == Purpose::Current
                && entry.task.stamp.target.path == target.path
                && entry.task.stamp.target.purpose == Purpose::Detail
                && (entry.settled
                    || entry.task.request.preview_policy != request.preview_policy
                    || !crate::worker::presentation::same_pixels(
                        &entry.task.request.settings,
                        &request.settings,
                    ))
            {
                return false;
            }
            entry.task.stamp.target.path != target.path
                || entry.task.stamp.target == target
                || match &target.purpose {
                    Purpose::Camera | Purpose::Defaults => !matches!(
                        entry.task.stamp.target.purpose,
                        Purpose::Camera
                            | Purpose::Defaults
                            | Purpose::Detail
                            | Purpose::Inspection(_, _)
                    ),
                    Purpose::Current | Purpose::Inspection(_, _) => {
                        !matches!(entry.task.stamp.target.purpose, Purpose::Inspection(_, _))
                    }
                    _ => true,
                }
        });
        let existing = self
            .desired
            .iter()
            .position(|entry| entry.task.stamp.target == target);
        if let Some(index) = existing {
            let old = &self.desired[index].task;
            if (old.origin.is_none() || origin.is_none() || old.origin == origin)
                && crate::worker::same_delivery_request(&old.request, &request)
            {
                let id = old.stamp.id;
                if old.origin.is_none() && origin.is_some() {
                    let mut task = (**old).clone();
                    task.origin = origin;
                    self.desired[index].task = Arc::new(task);
                }
                return id;
            }
        }
        let compatible = existing.is_some_and(|index| {
            let old = &self.desired[index].task;
            (old.origin.is_none() || origin.is_none() || old.origin == origin)
                && crate::worker::same_pixel_request(&old.request, &request)
        });
        let id = if compatible {
            self.desired[existing.unwrap()].task.stamp.id
        } else {
            self.id()
        };
        let stamp = Stamp {
            id,
            target,
            revision: request.revision,
        };
        let origin =
            origin.or_else(|| existing.and_then(|index| self.desired[index].task.origin.clone()));
        let seed = existing
            .and_then(|index| self.desired[index].delivered.as_ref())
            .filter(|(pixel, _)| *pixel == id)
            .map(|(_, seed)| seed.clone());
        let task = Arc::new(Task {
            stamp,
            request,
            origin,
            queued_at,
            seed,
        });
        if let Some(index) = existing {
            let entry = &mut self.desired[index];
            let same_delivery = entry.task.stamp == task.stamp;
            entry.task = task;
            if !same_delivery {
                entry.settled = false;
            }
        } else {
            self.desired.push(Desired {
                task,
                settled: false,
                admitted: 0,
                delivered: None,
            });
        }
        id
    }
    pub fn retain_overviews(&mut self, paths: &[PathBuf], retained: &[PathBuf]) {
        let before = self
            .desired
            .iter()
            .filter(|entry| entry.delivered.is_some())
            .count();
        self.desired.retain(|entry| {
            entry.task.stamp.target.purpose != Purpose::Current
                || paths.contains(&entry.task.stamp.target.path)
                || (entry.settled && retained.contains(&entry.task.stamp.target.path))
        });
        if self
            .desired
            .iter()
            .filter(|entry| entry.delivered.is_some())
            .count()
            != before
        {
            self.resource_revision = self.resource_revision.wrapping_add(1);
        }
    }
    pub fn activate(&mut self, path: &Path) {
        if self.active.as_deref() == Some(path) {
            return;
        }
        self.active = Some(path.to_owned());
        self.desired.retain(|entry| {
            entry.task.stamp.target.purpose == Purpose::Current
                || entry.task.stamp.target.purpose == Purpose::Export
                || entry.task.stamp.target.path == path
        });
        self.scopes = None;
        self.pick = None;
    }
    pub fn retire_before(&mut self, path: &Path) {
        let before = self
            .desired
            .iter()
            .filter(|entry| entry.delivered.is_some())
            .count();
        self.desired.retain(|entry| {
            entry.task.stamp.target.path != path
                || !matches!(
                    entry.task.stamp.target.purpose,
                    Purpose::Camera | Purpose::Defaults
                )
        });
        if self
            .desired
            .iter()
            .filter(|entry| entry.delivered.is_some())
            .count()
            != before
        {
            self.resource_revision = self.resource_revision.wrapping_add(1);
        }
    }
    pub fn clear(&mut self) {
        self.desired.clear();
        self.scopes = None;
        self.pick = None;
        self.resource_revision = self.resource_revision.wrapping_add(1);
    }
    pub fn frozen_export(
        &mut self,
        request: Arc<Request>,
        origin: Option<SourceOrigin>,
        queued_at: Instant,
    ) -> Arc<Task> {
        Arc::new(Task {
            stamp: Stamp {
                id: self.id(),
                target: Target::of(&request),
                revision: request.revision,
            },
            request,
            origin,
            queued_at,
            seed: None,
        })
    }
    pub fn plan(&self) -> Arc<WorkPlan> {
        Arc::new(WorkPlan {
            tasks: self
                .desired
                .iter()
                .filter(|entry| !entry.settled)
                .map(|entry| entry.task.clone())
                .collect(),
            scopes: self.scopes.clone(),
            pick: self.pick.clone(),
        })
    }
    pub fn busy(&self) -> bool {
        self.desired.iter().any(|entry| !entry.settled)
    }
    pub fn finish(&mut self, stamp: &Stamp) -> Completion {
        let Some(entry) = self
            .desired
            .iter_mut()
            .find(|entry| entry.task.stamp.target == stamp.target)
        else {
            return Completion::Removed;
        };
        if entry.task.stamp == *stamp {
            entry.settled = true;
            Completion::Current
        } else {
            Completion::Superseded
        }
    }
    pub fn admit(&mut self, stamp: &Stamp) -> bool {
        let Some(entry) = self
            .desired
            .iter_mut()
            .find(|entry| entry.task.stamp.target == stamp.target)
        else {
            return false;
        };
        if stamp.revision <= entry.admitted {
            return false;
        }
        entry.admitted = stamp.revision;
        true
    }
    pub fn note_pixels(&mut self, stamp: &Stamp, frame: &crate::worker::Frame) {
        self.bind_origin(stamp, &frame.origin);
        let Some(entry) = self.desired.iter_mut().find(|entry| {
            entry.task.stamp.target == stamp.target && entry.task.stamp.id == stamp.id
        }) else {
            return;
        };
        if let Some(snapshot) =
            crate::worker::presentation::Snapshot::from_frame(frame, &entry.task.request)
        {
            let snapshot = Arc::new(snapshot);
            entry.delivered = Some((stamp.id, snapshot.clone()));
            let mut task = (*entry.task).clone();
            task.seed = Some(snapshot);
            entry.task = Arc::new(task);
            self.resource_revision = self.resource_revision.wrapping_add(1);
        }
    }
    pub fn bind_origin(&mut self, stamp: &Stamp, origin: &SourceOrigin) {
        if let Some(entry) = self.desired.iter_mut().find(|entry| {
            entry.task.stamp.id == stamp.id && entry.task.stamp.target == stamp.target
        }) {
            if entry.task.origin.is_none() {
                let mut task = (*entry.task).clone();
                task.origin = Some(origin.clone());
                entry.task = Arc::new(task);
            }
        }
    }
    pub fn resource_revision(&self) -> u64 {
        self.resource_revision
    }
    pub fn delivered(&mut self, revision: u64) {
        if self
            .scopes
            .as_ref()
            .is_some_and(|task| task.request.key.revision != revision)
        {
            self.scopes = None;
        }
        if self
            .pick
            .as_ref()
            .is_some_and(|task| task.image_revision != revision)
        {
            self.pick = None;
        }
    }
    pub fn memory_resources(&self, inventory: &mut crate::memory::Inventory) {
        for entry in &self.desired {
            if let Some((_, snapshot)) = &entry.delivered {
                snapshot.memory_resources(inventory);
            }
        }
    }
    pub fn desired(&self, target: &Target) -> Option<&Arc<Task>> {
        self.desired
            .iter()
            .find(|entry| &entry.task.stamp.target == target)
            .map(|entry| &entry.task)
    }
    pub fn scopes(&mut self, request: ScopeRequest) {
        if self
            .scopes
            .as_ref()
            .is_some_and(|task| task.request.key == request.key)
        {
            return;
        }
        self.scopes = Some(Arc::new(ScopeTask {
            id: self.id(),
            request: Arc::new(request),
        }));
    }
    pub fn pick(
        &mut self,
        request: ColorPickRequest,
        image_revision: u64,
        origin: Option<SourceOrigin>,
    ) {
        self.pick = Some(Arc::new(PickTask {
            id: self.id(),
            image_revision,
            origin,
            request: Arc::new(request),
        }));
    }
    pub fn finish_scope(&mut self, id: TaskId) -> bool {
        if self.scopes.as_ref().is_some_and(|task| task.id == id) {
            self.scopes = None;
            true
        } else {
            false
        }
    }
    pub fn finish_pick(&mut self, id: TaskId) -> bool {
        if self.pick.as_ref().is_some_and(|task| task.id == id) {
            self.pick = None;
            true
        } else {
            false
        }
    }
}

/// Physical service order survives replacement plans; desired lifecycle stays in the UI coordinator.
#[derive(Default)]
pub struct Cursor {
    served: HashSet<Stamp>,
    last: Option<Target>,
    scopes: Option<TaskId>,
    pick: Option<TaskId>,
    followed: Option<TaskId>,
}
impl Cursor {
    pub fn next(&mut self, plan: &WorkPlan) -> Option<Arc<Task>> {
        self.served
            .retain(|stamp| plan.tasks.iter().any(|task| task.stamp == *stamp));
        let len = plan.tasks.len();
        if len == 0 {
            return None;
        }
        let start = self
            .last
            .as_ref()
            .and_then(|last| {
                plan.tasks
                    .iter()
                    .position(|task| &task.stamp.target == last)
            })
            .map_or(0, |index| (index + 1) % len);
        for offset in 0..len {
            let task = &plan.tasks[(start + offset) % len];
            if self.served.insert(task.stamp.clone()) {
                self.last = Some(task.stamp.target.clone());
                self.followed = None;
                return Some(task.clone());
            }
        }
        None
    }
    pub fn followup(&mut self, plan: &WorkPlan, previous: &Stamp) -> Option<Arc<Task>> {
        let next = plan.task(&previous.target)?;
        if self.followed != Some(previous.id)
            && next.stamp.id == previous.id
            && next.stamp.revision != previous.revision
            && self.served.insert(next.stamp.clone())
        {
            self.followed = Some(previous.id);
            Some(next.clone())
        } else {
            None
        }
    }
    pub fn scope(&mut self, plan: &WorkPlan) -> Option<Arc<ScopeTask>> {
        let task = plan.scopes.as_ref()?;
        if self.scopes == Some(task.id) {
            None
        } else {
            self.scopes = Some(task.id);
            Some(task.clone())
        }
    }
    pub fn pick(&mut self, plan: &WorkPlan) -> Option<Arc<PickTask>> {
        let task = plan.pick.as_ref()?;
        if self.pick == Some(task.id) {
            None
        } else {
            self.pick = Some(task.id);
            Some(task.clone())
        }
    }
}

#[cfg(test)]
#[path = "render_coordinator_test.rs"]
mod tests;
