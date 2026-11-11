//! Owns desired photo renders, handles their results and publishes immutable execution plans.

use super::{
    delivery::{self, Destination, ViewContext},
    request::{Purpose, Request},
    worker::{ColorPickRequest, Reply, ScopeRequest},
};
use foveonic::source::SourceOrigin;
use serde_json::json;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Target {
    pub path: PathBuf,
    pub purpose: Purpose,
}
impl Target {
    pub fn of(request: &Request) -> Self {
        Self {
            path: request.path.clone(),
            purpose: request.purpose(),
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
    pub seed: Option<Arc<crate::rendering::worker::presentation::Snapshot>>,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResultUpdate {
    pub current: bool,
    pub update_photo: bool,
    pub destination: Option<Destination>,
}
struct Desired {
    task: Arc<Task>,
    settled: bool,
    admitted: u64,
    delivered: Option<(
        TaskId,
        Arc<crate::rendering::worker::presentation::Snapshot>,
    )>,
}
fn retired(entry: &Desired, reason: &'static str) {
    crate::timing::task(
        &entry.task.stamp,
        if entry.settled {
            "retired"
        } else {
            "cancelled"
        },
        || json!({"reason":reason}),
    );
}
#[derive(Default)]
pub struct RenderCoordinator {
    next_id: u64,
    desired: Vec<Desired>,
    scopes: Option<Arc<ScopeTask>>,
    pick: Option<Arc<PickTask>>,
    active: Option<PathBuf>,
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
                    || !crate::rendering::matching::same_pixel_settings(
                        &entry.task.request.settings,
                        &request.settings,
                    ))
            {
                retired(entry, "detail_replaced");
                return false;
            }
            let keep = entry.task.stamp.target.path != target.path
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
                };
            if !keep {
                retired(entry, "purpose_replaced");
            }
            keep
        });
        let existing = self
            .desired
            .iter()
            .position(|entry| entry.task.stamp.target == target);
        if let Some(index) = existing {
            let old = &self.desired[index].task;
            if (old.origin.is_none() || origin.is_none() || old.origin == origin)
                && crate::rendering::matching::same_delivery_request(&old.request, &request)
            {
                let id = old.stamp.id;
                crate::timing::task(
                    &old.stamp,
                    "coalesced",
                    || json!({"desired_revision":request.revision,"settled":self.desired[index].settled}),
                );
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
                && crate::rendering::matching::can_share_pixel_work(&old.request, &request)
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
        crate::timing::task(&task.stamp, "desired", || {
            json!({"nominal_scale":task.request.preview_quality.working_scale().divisor(),"preview_quality":format!("{:?}",task.request.preview_quality),
            "output":task.request.output,"settings":task.request.settings,"pixel_reuse":compatible})
        });
        if let Some(index) = existing {
            let entry = &mut self.desired[index];
            let same_delivery = entry.task.stamp == task.stamp;
            if !same_delivery {
                crate::timing::task(
                    &entry.task.stamp,
                    "replaced",
                    || json!({"by_task":task.stamp.id.0,"by_revision":task.stamp.revision,"pixel_reuse":compatible}),
                );
            }
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
        self.desired.retain(|entry| {
            let keep = entry.task.stamp.target.purpose != Purpose::Current
                || paths.contains(&entry.task.stamp.target.path)
                || (entry.settled && retained.contains(&entry.task.stamp.target.path));
            if !keep {
                retired(entry, "target_removed");
            }
            keep
        });
    }
    pub fn activate(&mut self, path: &Path) {
        if self.active.as_deref() == Some(path) {
            return;
        }
        self.active = Some(path.to_owned());
        self.desired.retain(|entry| {
            let keep = entry.task.stamp.target.purpose == Purpose::Current
                || entry.task.stamp.target.purpose == Purpose::Export
                || entry.task.stamp.target.path == path;
            if !keep {
                retired(entry, "active_photo_changed");
            }
            keep
        });
        self.scopes = None;
        self.pick = None;
    }
    pub fn retire_before(&mut self, path: &Path) {
        self.desired.retain(|entry| {
            let keep = entry.task.stamp.target.path != path
                || !matches!(
                    entry.task.stamp.target.purpose,
                    Purpose::Camera | Purpose::Defaults
                );
            if !keep {
                retired(entry, "before_retired");
            }
            keep
        });
    }
    pub fn clear(&mut self) {
        for entry in &self.desired {
            retired(entry, "clear");
        }
        self.desired.clear();
        self.scopes = None;
        self.pick = None;
    }
    pub fn frozen_export(
        &mut self,
        request: Arc<Request>,
        origin: Option<SourceOrigin>,
        queued_at: Instant,
    ) -> Arc<Task> {
        let task = Arc::new(Task {
            stamp: Stamp {
                id: self.id(),
                target: Target::of(&request),
                revision: request.revision,
            },
            request,
            origin,
            queued_at,
            seed: None,
        });
        crate::timing::task(
            &task.stamp,
            "desired",
            || json!({"frozen_export":true,"output":task.request.output}),
        );
        task
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
    #[cfg(test)]
    pub fn is_settled_and_admitted(&self, id: TaskId, revision: u64) -> bool {
        self.admitted_images().contains(&(id, revision))
    }
    pub fn admitted_images(&self) -> Vec<(TaskId, u64)> {
        self.desired
            .iter()
            .filter(|entry| entry.settled && entry.admitted == entry.task.stamp.revision)
            .map(|entry| (entry.task.stamp.id, entry.admitted))
            .collect()
    }
    /// Decide reuse and view updates together. Photo feedback can remain useful
    /// even when the image is rejected. UI application and acknowledgement follow.
    pub fn handle_result(&mut self, reply: &Reply, view: &ViewContext<'_>) -> ResultUpdate {
        let completion = if reply.exported.is_some() {
            crate::timing::task(
                &reply.stamp,
                "completed",
                || json!({"frozen_export":true,"success":reply.result.is_ok()}),
            );
            Completion::Current
        } else {
            self.finish(&reply.stamp)
        };
        let decision = delivery::classify(completion, &reply.into(), view);
        let image = reply.result.as_ref().ok().and_then(Option::as_ref);
        if decision.retain_pixels {
            if let Some(frame) = image {
                self.note_pixels(&reply.stamp, frame);
            }
        }
        let destination = match decision.destination {
            Ok(destination) => {
                if image.is_some_and(|frame| !self.admit(&reply.stamp, &frame.origin)) {
                    None
                } else {
                    Some(destination)
                }
            }
            Err(rejection) => {
                crate::timing::task(
                    &reply.stamp,
                    "image_rejected",
                    || json!({"reason":rejection.reason()}),
                );
                None
            }
        };
        ResultUpdate {
            current: completion == Completion::Current,
            update_photo: decision.update_photo,
            destination,
        }
    }
    pub fn finish(&mut self, stamp: &Stamp) -> Completion {
        let Some(entry) = self
            .desired
            .iter_mut()
            .find(|entry| entry.task.stamp.target == stamp.target)
        else {
            crate::timing::task(stamp, "completion_removed", || json!({}));
            return Completion::Removed;
        };
        if entry.task.stamp == *stamp {
            entry.settled = true;
            crate::timing::task(stamp, "completed", || json!({}));
            Completion::Current
        } else {
            crate::timing::task(
                stamp,
                "completion_superseded",
                || json!({"current_revision":entry.task.stamp.revision}),
            );
            Completion::Superseded
        }
    }
    pub fn admit(&mut self, stamp: &Stamp, origin: &SourceOrigin) -> bool {
        let Some(entry) = self
            .desired
            .iter_mut()
            .find(|entry| entry.task.stamp.target == stamp.target)
        else {
            crate::timing::task(stamp, "rejected", || json!({"reason":"target_removed"}));
            return false;
        };
        if entry
            .task
            .origin
            .as_ref()
            .is_some_and(|expected| expected != origin)
        {
            crate::timing::task(stamp, "rejected", || json!({"reason":"source_changed"}));
            return false;
        }
        if stamp.revision <= entry.admitted {
            crate::timing::task(
                stamp,
                "rejected",
                || json!({"reason":"out_of_order","admitted_revision":entry.admitted}),
            );
            return false;
        }
        entry.admitted = stamp.revision;
        crate::timing::task(
            stamp,
            "admitted",
            || json!({"desired_revision":entry.task.stamp.revision}),
        );
        true
    }
    pub fn note_pixels(&mut self, stamp: &Stamp, frame: &crate::rendering::worker::Frame) {
        self.bind_origin(stamp, &frame.origin);
        let Some(entry) = self.desired.iter_mut().find(|entry| {
            entry.task.stamp.target == stamp.target && entry.task.stamp.id == stamp.id
        }) else {
            return;
        };
        if let Some(snapshot) =
            crate::rendering::worker::presentation::Snapshot::from_frame(frame, &entry.task.request)
        {
            let snapshot = Arc::new(snapshot);
            entry.delivered = Some((stamp.id, snapshot.clone()));
            let mut task = (*entry.task).clone();
            task.seed = Some(snapshot);
            entry.task = Arc::new(task);
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
    pub fn retained_images(&self) -> Vec<Arc<crate::rendering::worker::presentation::Snapshot>> {
        self.desired
            .iter()
            .filter_map(|entry| {
                entry
                    .delivered
                    .as_ref()
                    .map(|(_, snapshot)| snapshot.clone())
            })
            .collect()
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
            crate::timing::emit(
                "scope_coalesced",
                || json!({"task":self.scopes.as_ref().unwrap().id.0,"revision":request.key.revision}),
            );
            return;
        }
        if let Some(old) = &self.scopes {
            crate::timing::emit(
                "scope_replaced",
                || json!({"task":old.id.0,"revision":old.request.key.revision,"by_revision":request.key.revision}),
            );
        }
        self.scopes = Some(Arc::new(ScopeTask {
            id: self.id(),
            request: Arc::new(request),
        }));
        crate::timing::emit(
            "scope_desired",
            || json!({"task":self.scopes.as_ref().unwrap().id.0,"revision":self.scopes.as_ref().unwrap().request.key.revision}),
        );
    }
    pub fn pick(
        &mut self,
        request: ColorPickRequest,
        image_revision: u64,
        origin: Option<SourceOrigin>,
    ) {
        if let Some(old) = &self.pick {
            crate::timing::emit(
                "pick_replaced",
                || json!({"task":old.id.0,"revision":old.image_revision}),
            );
        }
        self.pick = Some(Arc::new(PickTask {
            id: self.id(),
            image_revision,
            origin,
            request: Arc::new(request),
        }));
        crate::timing::emit(
            "pick_desired",
            || json!({"task":self.pick.as_ref().unwrap().id.0,"revision":image_revision,"source":self.pick.as_ref().unwrap().request.path}),
        );
    }
    pub fn finish_scope(&mut self, id: TaskId) -> bool {
        crate::timing::emit(
            "scope_completed",
            || json!({"task":id.0,"current":self.scopes.as_ref().is_some_and(|task|task.id==id)}),
        );
        if self.scopes.as_ref().is_some_and(|task| task.id == id) {
            self.scopes = None;
            true
        } else {
            false
        }
    }
    pub fn finish_pick(&mut self, id: TaskId) -> bool {
        crate::timing::emit(
            "pick_completed",
            || json!({"task":id.0,"current":self.pick.as_ref().is_some_and(|task|task.id==id)}),
        );
        if self.pick.as_ref().is_some_and(|task| task.id == id) {
            self.pick = None;
            true
        } else {
            false
        }
    }
}

/// Physical service order survives replacement plans while the coordinator owns request lifecycle.
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
#[path = "coordinator_test.rs"]
mod tests;
