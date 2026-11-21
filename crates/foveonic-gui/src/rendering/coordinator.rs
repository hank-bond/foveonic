//! Owns desired photo renders, handles their results and publishes immutable execution plans.

use super::{
    request::{Purpose, RenderRequest},
    worker::{ColorPickRequest, Reply, ScopeRequest},
};
use crate::rendering::request::RequestId;
use foveonic::priority::Priority;
use foveonic::source::SourceOrigin;
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Instant};

/// The photos the app holds work for, by how their work is kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Photos {
    /// Planned photos: work must match an image in use.
    pub planned: Vec<PathBuf>,
    /// Loading photos: viewport photos whose settings have not been read yet.
    pub loading: Vec<PathBuf>,
    /// History photos: they left the viewport and keep finished images.
    pub history: Vec<PathBuf>,
    /// The viewed photo and the editing selection. Their working photos are
    /// protected from eviction between edits.
    pub eviction_protected: Vec<PathBuf>,
}

/// What a task renders for: a photo's whole image or patch for one purpose.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Target {
    pub path: PathBuf,
    pub purpose: Purpose,
    pub patch: bool,
}
impl Target {
    pub fn of(request: &RenderRequest) -> Self {
        Self {
            path: request.path.clone(),
            purpose: request.purpose(),
            patch: request.detail,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Stamp {
    pub id: TaskId,
    pub target: Target,
    /// The id of the request the task now serves.
    pub request_id: RequestId,
}
#[derive(Clone)]
pub struct Task {
    pub stamp: Stamp,
    pub request: Arc<RenderRequest>,
    pub origin: Option<SourceOrigin>,
    pub queued_at: Instant,
    pub seed: Option<Arc<crate::rendering::worker::presentation::Snapshot>>,
    /// Whole-image continuation created after regional detail, not explicit demand.
    pub speculative: bool,
}
#[derive(Clone)]
pub struct ScopeTask {
    pub id: TaskId,
    pub request: Arc<ScopeRequest>,
}
#[derive(Clone)]
pub struct PickTask {
    pub id: TaskId,
    pub image_request_id: RequestId,
    pub origin: Option<SourceOrigin>,
    pub request: Arc<ColorPickRequest>,
}
#[derive(Clone, Default)]
pub struct WorkPlan {
    pub tasks: Vec<Arc<Task>>,
    pub scopes: Option<Arc<ScopeTask>>,
    pub pick: Option<Arc<PickTask>>,
    /// Targets whose working photos are protected from eviction: the viewed
    /// and editing photos' images in use, and unfinished work. Another photo shown only
    /// with finished images may lose its working photo under memory pressure.
    /// Its next render reloads it.
    pub eviction_protected: Vec<Target>,
}
impl WorkPlan {
    pub fn task(&self, target: &Target) -> Option<&Arc<Task>> {
        self.tasks.iter().find(|task| &task.stamp.target == target)
    }
    /// What each task is for, which sets its priority and how it keeps its photo.
    pub fn purpose(&self, task: &Task) -> foveonic::render::Purpose {
        use foveonic::render::Purpose as Job;
        if task.speculative {
            return Job::Speculative;
        }
        match &task.stamp.target.purpose {
            Purpose::Current | Purpose::Inspection(_, false) => Job::Edit,
            Purpose::Camera | Purpose::Defaults | Purpose::Inspection(_, true) => Job::View,
        }
    }
    pub fn priority(&self, task: &Task) -> Priority {
        self.purpose(task).priority()
    }
}
impl Task {
    pub fn viewport(&self) -> bool {
        !self.speculative
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
    /// The result's task is still the desired work for its target.
    pub current: bool,
    /// The result is an image in use, newer than any admitted for its target.
    pub accepted: bool,
}
struct Desired {
    task: Arc<Task>,
    settled: bool,
    /// The current task's result was a failure.
    failed: bool,
    admitted: RequestId,
    delivered: Option<Arc<crate::rendering::worker::presentation::Snapshot>>,
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
    /// The images in use for the viewed and editing photos, whether or not
    /// they have work.
    protected_in_use: Vec<Target>,
    scopes: Option<Arc<ScopeTask>>,
    pick: Option<Arc<PickTask>>,
}
impl RenderCoordinator {
    fn id(&mut self) -> TaskId {
        self.next_id += 1;
        TaskId(self.next_id)
    }
    /// Coalesce with, replace or add the desired work for this request's target.
    pub fn desire(
        &mut self,
        request: Arc<RenderRequest>,
        origin: Option<SourceOrigin>,
        queued_at: Instant,
        speculative: bool,
    ) -> TaskId {
        self.place(request, origin, queued_at, speculative)
    }

    /// The only rule for work: a viewport photo keeps the work that matches an
    /// image in use, a loading photo keeps its work, a history photo keeps only
    /// finished images, and all other work stops.
    pub fn reconcile(&mut self, photos: &Photos, in_use: &[RenderRequest]) {
        self.protected_in_use = in_use
            .iter()
            .filter(|request| photos.eviction_protected.contains(&request.path))
            .map(Target::of)
            .collect();
        self.desired.retain(|entry| {
            let target = &entry.task.stamp.target;
            let (keep, reason) = if photos.loading.contains(&target.path) {
                (true, "")
            } else if photos.planned.contains(&target.path) {
                let used = in_use.iter().any(|request| {
                    &Target::of(request) == target
                        && crate::rendering::matching::same_delivery_request(
                            request,
                            &entry.task.request,
                        )
                });
                (used, "not_in_use")
            } else {
                (
                    entry.settled && photos.history.contains(&target.path),
                    "target_removed",
                )
            };
            if !keep {
                retired(entry, reason);
            }
            keep
        });
    }

    /// List work in the planner's order, which the executor follows within a
    /// priority tier. Work for loading photos, which the plan omits, goes last.
    pub fn order_as(&mut self, in_use: &[RenderRequest]) {
        self.desired.sort_by_key(|entry| {
            in_use
                .iter()
                .position(|request| Target::of(request) == entry.task.stamp.target)
                .unwrap_or(in_use.len())
        });
    }

    /// Coalesce with, replace or add the desired work for this request's target.
    fn place(
        &mut self,
        request: Arc<RenderRequest>,
        origin: Option<SourceOrigin>,
        queued_at: Instant,
        speculative: bool,
    ) -> TaskId {
        let target = Target::of(&request);
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
                    || json!({"desired_revision":request.id,"settled":self.desired[index].settled}),
                );
                if (old.origin.is_none() && origin.is_some()) || (!speculative && old.speculative) {
                    let mut task = (**old).clone();
                    if task.origin.is_none() {
                        task.origin = origin;
                    }
                    task.speculative &= speculative;
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
            request_id: request.id,
        };
        // A task knows only the origin its request knew. Inheriting the previous
        // task's origin would reject the first image of a replaced file as stale.
        let seed = existing
            .and_then(|index| self.desired[index].delivered.as_ref())
            // A new sampling demand has its own task identity, but completed finer
            // pixels can still satisfy it. The presenter checks their full contract.
            .filter(|snapshot| {
                origin
                    .as_ref()
                    .is_none_or(|origin| snapshot.origin() == origin)
            })
            .cloned();
        let task = Arc::new(Task {
            stamp,
            request,
            origin,
            queued_at,
            seed,
            speculative,
        });
        crate::timing::task(&task.stamp, "desired", || {
            json!({"nominal_scale":task.request.preview_scale.divisor(),"preview_scale":format!("{:?}",task.request.preview_scale),
            "output":task.request.output,"settings":task.request.settings,"preview_region":task.request.settings.preview_region,"pixel_reuse":compatible})
        });
        if let Some(index) = existing {
            let entry = &mut self.desired[index];
            let same_delivery = entry.task.stamp == task.stamp;
            if !same_delivery {
                crate::timing::task(
                    &entry.task.stamp,
                    "replaced",
                    || json!({"by_task":task.stamp.id.0,"by_revision":task.stamp.request_id,"pixel_reuse":compatible}),
                );
            }
            entry.task = task;
            if !same_delivery {
                entry.settled = false;
                entry.failed = false;
            }
        } else {
            self.desired.push(Desired {
                task,
                settled: false,
                failed: false,
                admitted: RequestId::default(),
                delivered: None,
            });
        }
        id
    }

    pub fn clear(&mut self) {
        for entry in &self.desired {
            retired(entry, "clear");
        }
        self.desired.clear();
        self.scopes = None;
        self.pick = None;
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
            eviction_protected: self
                .protected_in_use
                .iter()
                .cloned()
                .chain(
                    self.desired
                        .iter()
                        .filter(|entry| !entry.settled)
                        .map(|entry| entry.task.stamp.target.clone())
                        .filter(|target| !self.protected_in_use.contains(target)),
                )
                .collect(),
        })
    }
    pub fn busy(&self) -> bool {
        self.desired
            .iter()
            .any(|entry| !entry.settled && entry.task.viewport())
    }
    #[cfg(test)]
    pub fn is_settled_and_admitted(&self, id: TaskId, request_id: RequestId) -> bool {
        self.admitted_images().contains(&(id, request_id))
    }
    #[cfg(test)]
    pub fn admitted_images(&self) -> Vec<(TaskId, RequestId)> {
        self.desired
            .iter()
            .filter(|entry| entry.settled && entry.admitted == entry.task.stamp.request_id)
            .map(|entry| (entry.task.stamp.id, entry.admitted))
            .collect()
    }
    /// Settle a result and decide whether it is an image in use. A superseded
    /// result for a target still in use can advance feedback; a result whose target
    /// was removed is not in use.
    pub fn handle_result(&mut self, reply: &Reply) -> ResultUpdate {
        let completion = self.finish(&reply.stamp);
        if completion == Completion::Current && reply.result.failed() {
            if let Some(entry) = self
                .desired
                .iter_mut()
                .find(|entry| entry.task.stamp == reply.stamp)
            {
                entry.failed = true;
            }
        }
        let image = reply.result.rendered();
        let accepted = completion != Completion::Removed
            && image.is_some_and(|frame| {
                let admitted = self.admit(&reply.stamp, &frame.origin);
                if admitted {
                    self.note_pixels(&reply.stamp, frame);
                }
                admitted
            });
        ResultUpdate {
            current: completion == Completion::Current,
            accepted,
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
                || json!({"current_revision":entry.task.stamp.request_id}),
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
        if stamp.request_id <= entry.admitted {
            crate::timing::task(
                stamp,
                "rejected",
                || json!({"reason":"out_of_order","admitted_revision":entry.admitted}),
            );
            return false;
        }
        entry.admitted = stamp.request_id;
        crate::timing::task(
            stamp,
            "admitted",
            || json!({"desired_revision":entry.task.stamp.request_id}),
        );
        true
    }
    pub fn note_pixels(&mut self, stamp: &Stamp, frame: &crate::rendering::worker::RenderResult) {
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
            entry.delivered = Some(snapshot.clone());
            let mut task = (*entry.task).clone();
            task.seed = Some(snapshot);
            entry.task = Arc::new(task);
        }
    }
    /// Record the source an image came from on the task that made it. A later
    /// request sharing the task's pixel work may come from a replaced file, so
    /// only the exact request is bound.
    pub fn bind_origin(&mut self, stamp: &Stamp, origin: &SourceOrigin) {
        if let Some(entry) = self
            .desired
            .iter_mut()
            .find(|entry| &entry.task.stamp == stamp)
        {
            if entry.task.origin.is_none() {
                let mut task = (*entry.task).clone();
                task.origin = Some(origin.clone());
                entry.task = Arc::new(task);
            }
        }
    }
    /// A new whole image of the active photo makes a pending color pick from an
    /// older one obsolete. Scopes are requested again for each delivery instead.
    pub fn whole_delivered(&mut self, request_id: RequestId) {
        if self
            .pick
            .as_ref()
            .is_some_and(|task| task.image_request_id != request_id)
        {
            self.pick = None;
        }
    }
    pub fn retained_images(&self) -> Vec<Arc<crate::rendering::worker::presentation::Snapshot>> {
        self.desired
            .iter()
            .filter_map(|entry| entry.delivered.clone())
            .collect()
    }
    /// A target's current request and whether its work finished or failed.
    pub fn target_progress(&self, target: &Target) -> Option<(RequestId, bool, bool)> {
        self.desired
            .iter()
            .find(|entry| &entry.task.stamp.target == target)
            .map(|entry| (entry.task.stamp.request_id, entry.settled, entry.failed))
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
                || json!({"task":self.scopes.as_ref().unwrap().id.0,"revision":request.key.request_id}),
            );
            return;
        }
        if let Some(old) = &self.scopes {
            crate::timing::emit(
                "scope_replaced",
                || json!({"task":old.id.0,"revision":old.request.key.request_id,"by_revision":request.key.request_id}),
            );
        }
        self.scopes = Some(Arc::new(ScopeTask {
            id: self.id(),
            request: Arc::new(request),
        }));
        crate::timing::emit(
            "scope_desired",
            || json!({"task":self.scopes.as_ref().unwrap().id.0,"revision":self.scopes.as_ref().unwrap().request.key.request_id}),
        );
    }
    pub fn pick(
        &mut self,
        request: ColorPickRequest,
        image_request_id: RequestId,
        origin: Option<SourceOrigin>,
    ) {
        if let Some(old) = &self.pick {
            crate::timing::emit(
                "pick_replaced",
                || json!({"task":old.id.0,"revision":old.image_request_id}),
            );
        }
        self.pick = Some(Arc::new(PickTask {
            id: self.id(),
            image_request_id,
            origin,
            request: Arc::new(request),
        }));
        crate::timing::emit(
            "pick_desired",
            || json!({"task":self.pick.as_ref().unwrap().id.0,"revision":image_request_id,"source":self.pick.as_ref().unwrap().request.path}),
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

#[cfg(test)]
#[path = "coordinator_test.rs"]
mod tests;
