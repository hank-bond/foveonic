//! Owns desired photo renders, handles their results and publishes immutable execution plans.

use super::{
    delivery::{self, Destination, ViewContext},
    request::{Purpose, Refinement, Request},
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
    pub image_revision: u64,
    pub origin: Option<SourceOrigin>,
    pub request: Arc<ColorPickRequest>,
}
#[derive(Clone, Default)]
pub struct WorkPlan {
    pub tasks: Vec<Arc<Task>>,
    pub scopes: Option<Arc<ScopeTask>>,
    pub pick: Option<Arc<PickTask>>,
    pub preview_active: Option<PathBuf>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Viewport,
    Auxiliary,
    Thumbnail,
    Idle,
}
impl WorkPlan {
    pub fn task(&self, target: &Target) -> Option<&Arc<Task>> {
        self.tasks.iter().find(|task| &task.stamp.target == target)
    }
    pub fn priority(&self, task: &Task) -> TaskPriority {
        if task.speculative {
            TaskPriority::Idle
        } else if task.stamp.target.purpose != Purpose::Preview {
            TaskPriority::Viewport
        } else if self.preview_active.as_ref() == Some(&task.request.path) {
            TaskPriority::Auxiliary
        } else {
            TaskPriority::Thumbnail
        }
    }
}
impl Task {
    pub fn viewport(&self) -> bool {
        !self.speculative && self.stamp.target.purpose != Purpose::Preview
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
#[derive(Clone)]
struct RefinementState {
    first: Stamp,
    request: Arc<Refinement>,
    pending: Option<PendingRegion>,
}
#[derive(Clone)]
struct PendingRegion {
    request: Arc<Refinement>,
    queued_at: Instant,
}
struct Desired {
    task: Arc<Task>,
    refinement: Option<RefinementState>,
    settled: bool,
    admitted: u64,
    // Coverage of the admitted result. Admission must match the current stamp.
    regional: bool,
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
    scopes: Option<Arc<ScopeTask>>,
    pick: Option<Arc<PickTask>>,
    active: Option<PathBuf>,
    before_overviews: Vec<(PathBuf, crate::editor::Rendition)>,
    background_completion_disabled: bool,
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
        self.desire_inner(request, origin, queued_at, false)
    }

    fn desire_inner(
        &mut self,
        request: Arc<Request>,
        origin: Option<SourceOrigin>,
        queued_at: Instant,
        speculative: bool,
    ) -> TaskId {
        let target = Target::of(&request);
        if speculative || target.purpose == Purpose::Preview {
            // Keep the admitted region and its intent independently settled.
        } else if target.purpose == Purpose::Detail {
            // A moving lens replaces its detail, not a useful Whole continuation.
            if let Some(entry) = self
                .desired
                .iter_mut()
                .find(|entry| entry.task.stamp.target == target)
            {
                entry.refinement = None;
            }
        } else if !matches!(target.purpose, Purpose::Inspection(_, _) | Purpose::Export) {
            self.cancel_refinement_for(&request.path, Some(request.rendition), Some(&target));
        }
        self.desired.retain(|entry| {
            if !speculative
                && target.purpose == Purpose::Current
                && entry.task.stamp.target.path == target.path
                && entry.task.stamp.target.purpose == Purpose::Detail
                && (entry.settled
                    || entry.task.request.processing_quality != request.processing_quality
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
                        Purpose::Camera | Purpose::Defaults | Purpose::Inspection(_, _)
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
            revision: request.revision,
        };
        let origin =
            origin.or_else(|| existing.and_then(|index| self.desired[index].task.origin.clone()));
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
                refinement: None,
                settled: false,
                admitted: 0,
                regional: false,
                delivered: None,
            });
        }
        id
    }
    pub fn refine(
        &mut self,
        request: Refinement,
        origin: Option<SourceOrigin>,
        queued_at: Instant,
    ) -> Option<Arc<Task>> {
        if let Some(index) = self.desired.iter().position(|entry| {
            entry.refinement.as_ref().is_some_and(|state| {
                state.first.target == entry.task.stamp.target
                    && state.request.same_sequence(&request)
                    && (entry.task.origin.is_none()
                        || origin.is_none()
                        || entry.task.origin == origin)
            })
        }) {
            if let Some(origin) = origin {
                let stamp = self.desired[index].task.stamp.clone();
                self.bind_origin(&stamp, &origin);
            }
            let entry = &self.desired[index];
            let state = entry.refinement.as_ref().unwrap();
            let whole_admitted = self.whole_admitted(state, entry.task.origin.as_ref());
            let first_admitted_whole =
                entry.settled && entry.admitted == entry.task.stamp.revision && !entry.regional;
            if whole_admitted || first_admitted_whole || state.request.covers(&request) {
                self.desired[index].refinement.as_mut().unwrap().pending = None;
                return None;
            }
            let pending = PendingRegion {
                request: Arc::new(request),
                queued_at,
            };
            if !entry.settled && entry.task.stamp == state.first {
                // Finish useful work already in flight, retaining only the latest
                // uncovered viewport rather than restarting on every pan event.
                self.desired[index].refinement.as_mut().unwrap().pending = Some(pending);
                return None;
            }
            let state = state.clone();
            let origin = entry.task.origin.clone();
            return self.start_region(state, pending, origin);
        }
        // A loupe can replace and then retire the primary patch while its
        // same-edit preview/whole work remains useful. Recover that sequence's
        // frozen siblings, but do not mistake its retired patch for coverage.
        if let Some(index) = self.desired.iter().position(|entry| {
            entry.refinement.as_ref().is_some_and(|state| {
                state.request.same_sequence(&request)
                    && (entry.task.origin.is_none()
                        || origin.is_none()
                        || entry.task.origin == origin)
            })
        }) {
            if let Some(origin) = &origin {
                let stamp = self.desired[index].task.stamp.clone();
                self.bind_origin(&stamp, origin);
            }
            let entry = &self.desired[index];
            let state = entry.refinement.as_ref().unwrap();
            if self.whole_admitted(state, entry.task.origin.as_ref()) {
                return None;
            }
            return self.start_region(
                state.clone(),
                PendingRegion {
                    request: Arc::new(request),
                    queued_at,
                },
                entry.task.origin.clone(),
            );
        }
        self.cancel_refinement_for(&request.first.path, Some(request.first.rendition), None);
        // A new edit's regional job replaces obsolete whole-image work for that
        // photo. Completed whole images remain useful presentation resources.
        self.desired.retain(|entry| {
            let keep = request.first.rendition != crate::editor::Rendition::Current
                || entry.settled
                || entry.task.stamp.target.path != request.first.path
                || entry.task.stamp.target.purpose != Purpose::Current;
            if !keep {
                retired(entry, "refinement_replaced");
            }
            keep
        });
        let request = Arc::new(request);
        self.desire(Arc::new(request.first.clone()), origin, queued_at);
        let entry = self
            .desired
            .iter_mut()
            .find(|entry| entry.task.stamp.target == Target::of(&request.first))
            .unwrap();
        entry.refinement = Some(RefinementState {
            first: entry.task.stamp.clone(),
            request,
            pending: None,
        });
        if !entry.settled {
            return Some(entry.task.clone());
        }
        // The first image can already be admitted, for example when a
        // region-only request acquires a whole-image continuation later.
        if entry.admitted == entry.task.stamp.revision {
            let stamp = entry.task.stamp.clone();
            let origin = entry.task.origin.clone();
            let regional = entry.regional;
            return self.advance_refinement(
                &stamp,
                ResultUpdate {
                    current: true,
                    update_photo: true,
                    destination: Some(Destination::Active),
                },
                origin.as_ref().map(|origin| (origin, regional)),
            );
        }
        None
    }

    fn whole_admitted(&self, state: &RefinementState, origin: Option<&SourceOrigin>) -> bool {
        state.request.whole.as_ref().is_some_and(|whole| {
            self.desired.iter().any(|entry| {
                entry.settled
                    && entry.admitted == entry.task.stamp.revision
                    && !entry.regional
                    && origin.is_none_or(|origin| entry.task.origin.as_ref() == Some(origin))
                    && crate::rendering::matching::same_delivery_request(&entry.task.request, whole)
            })
        })
    }

    fn start_region(
        &mut self,
        mut state: RefinementState,
        pending: PendingRegion,
        origin: Option<SourceOrigin>,
    ) -> Option<Arc<Task>> {
        let mut request = (*state.request).clone();
        request.first = pending.request.first.clone();
        request.visible_region = pending.request.visible_region;
        // Before/Defaults share one target across both phases, so their whole
        // result needs a revision newer than the replacement regional result.
        if let Some((whole, next_whole)) =
            request.whole.as_mut().zip(pending.request.whole.as_ref())
        {
            if Target::of(whole) == Target::of(&request.first) {
                whole.revision = next_whole.revision;
            }
        }
        state.request = Arc::new(request);
        state.pending = None;
        self.desire_inner(
            Arc::new(state.request.first.clone()),
            origin,
            pending.queued_at,
            false,
        );
        let entry = self
            .desired
            .iter_mut()
            .find(|entry| entry.task.stamp.target == Target::of(&state.request.first))?;
        state.first = entry.task.stamp.clone();
        entry.refinement = Some(state);
        (!entry.settled).then(|| entry.task.clone())
    }

    pub fn cancel_refinement(&mut self, path: &Path, rendition: crate::editor::Rendition) {
        self.cancel_refinement_for(path, Some(rendition), None);
    }

    pub fn retire_detail(&mut self, path: &Path) {
        self.desired.retain(|entry| {
            let keep = entry.task.stamp.target.path != path
                || entry.task.stamp.target.purpose != Purpose::Detail;
            if !keep {
                retired(entry, "detail_retired");
            }
            keep
        });
    }

    fn cancel_refinement_for(
        &mut self,
        path: &Path,
        rendition: Option<crate::editor::Rendition>,
        replacing: Option<&Target>,
    ) {
        self.desired.retain_mut(|entry| {
            if entry.task.stamp.target.path != path
                || !entry.refinement.as_ref().is_some_and(|state| {
                    rendition.is_none_or(|rendition| state.request.first.rendition == rendition)
                })
            {
                return true;
            }
            entry.refinement = None;
            let keep = replacing == Some(&entry.task.stamp.target)
                || (!entry.task.request.detail
                    && !entry.task.speculative
                    && entry.task.stamp.target.purpose != Purpose::Preview)
                || entry.settled;
            if !keep {
                retired(entry, "refinement_cancelled");
            }
            keep
        });
    }

    fn advance_refinement(
        &mut self,
        stamp: &Stamp,
        update: ResultUpdate,
        image: Option<(&SourceOrigin, bool)>,
    ) -> Option<Arc<Task>> {
        let index = self.desired.iter().position(|entry| {
            entry.task.stamp == *stamp
                && entry
                    .refinement
                    .as_ref()
                    .is_some_and(|state| state.first == *stamp)
        })?;
        let mut state = self.desired[index].refinement.as_ref().unwrap().clone();
        let pending = state.pending.take();
        self.desired[index].refinement.as_mut().unwrap().pending = None;
        if !update.current || update.destination.is_none() || image.is_none() {
            let origin = self.desired[index].task.origin.clone();
            self.desired[index].refinement = None;
            // A failed region must not swallow a later pan. A rejected source,
            // however, cannot authorize work from the previous source identity.
            if update.current && image.is_none() {
                if let Some(pending) = pending {
                    return self.start_region(state, pending, origin);
                }
            }
            return None;
        }
        let (origin, regional) = image.unwrap();
        self.desired[index].regional = regional;
        if !regional {
            return None;
        }
        if let Some(preview) = &state.request.preview {
            self.desire_inner(
                Arc::new(preview.clone()),
                Some(origin.clone()),
                Instant::now(),
                false,
            );
            if let Some(entry) = self
                .desired
                .iter_mut()
                .find(|entry| entry.task.stamp.target == Target::of(preview))
            {
                entry.refinement = Some(state.clone());
            }
        }
        if let Some(pending) = pending.filter(|_| !self.whole_admitted(&state, Some(origin))) {
            return self.start_region(state, pending, Some(origin.clone()));
        }
        if self.background_completion_disabled {
            return None;
        }
        let whole = state.request.whole.as_ref()?;
        let target = Target::of(whole);
        if self.desired.iter().any(|entry| {
            entry.task.stamp.target == target
                && !entry.settled
                && !entry.task.speculative
                && !crate::rendering::matching::same_delivery_request(&entry.task.request, whole)
        }) {
            return None;
        }
        let foreground = self.desired.iter().any(|entry| {
            entry.task.stamp.target == target
                && !entry.settled
                && !entry.task.speculative
                && entry
                    .task
                    .origin
                    .as_ref()
                    .is_none_or(|current| current == origin)
                && crate::rendering::matching::same_delivery_request(&entry.task.request, whole)
        });
        self.desire_inner(
            Arc::new(whole.clone()),
            Some(origin.clone()),
            Instant::now(),
            !foreground,
        );
        let entry = self
            .desired
            .iter_mut()
            .find(|entry| entry.task.stamp.target == target)
            .unwrap();
        if !foreground {
            let mut task = (*entry.task).clone();
            task.speculative = true;
            entry.task = Arc::new(task);
        }
        entry.refinement = Some(state);
        (!entry.settled).then(|| entry.task.clone())
    }

    /// Policy changes retire optional work without rerendering admitted regions.
    pub fn set_background_completion(&mut self, enabled: bool) {
        if self.background_completion_disabled == !enabled {
            return;
        }
        self.background_completion_disabled = !enabled;
        if !enabled {
            self.desired.retain_mut(|entry| {
                let keep = !entry.task.speculative || entry.settled;
                if !keep {
                    retired(entry, "background_completion_disabled");
                    // Before uses the same target for both phases. Restore its
                    // admitted region instead of losing foreground settlement.
                    if let Some(state) = entry
                        .refinement
                        .as_ref()
                        .filter(|state| state.first.target == entry.task.stamp.target)
                    {
                        entry.task = Arc::new(Task {
                            stamp: state.first.clone(),
                            request: Arc::new(state.request.first.clone()),
                            origin: entry.task.origin.clone(),
                            queued_at: entry.task.queued_at,
                            seed: None,
                            speculative: false,
                        });
                        entry.settled = true;
                        entry.admitted = state.first.revision;
                        entry.regional = true;
                        entry.delivered = None;
                        return true;
                    }
                }
                keep
            });
            return;
        }
        let ready = self
            .desired
            .iter()
            .filter(|entry| {
                entry.settled
                    && entry.regional
                    && entry.admitted == entry.task.stamp.revision
                    && entry
                        .refinement
                        .as_ref()
                        .is_some_and(|state| state.first == entry.task.stamp)
            })
            .map(|entry| (entry.task.stamp.clone(), entry.task.origin.clone()))
            .collect::<Vec<_>>();
        for (stamp, origin) in ready {
            self.advance_refinement(
                &stamp,
                ResultUpdate {
                    current: true,
                    update_photo: true,
                    destination: Some(Destination::Active),
                },
                origin.as_ref().map(|origin| (origin, true)),
            );
        }
    }

    pub fn retain_overviews(&mut self, paths: &[PathBuf], retained: &[PathBuf]) {
        self.desired.retain(|entry| {
            let keep = !matches!(
                entry.task.stamp.target.purpose,
                Purpose::Current | Purpose::Detail | Purpose::Preview
            ) || paths.contains(&entry.task.stamp.target.path)
                || (entry.settled && retained.contains(&entry.task.stamp.target.path));
            if !keep {
                retired(entry, "target_removed");
            }
            keep
        });
    }
    pub fn set_view(&mut self, view: &super::delivery::ViewState) {
        self.before_overviews = view
            .overviews
            .iter()
            .filter(|photo| photo.rendition != crate::editor::Rendition::Current)
            .map(|photo| (photo.path.clone(), photo.rendition))
            .collect();
        self.desired.retain(|entry| {
            let request = &entry.task.request;
            let keep = !matches!(
                entry.task.stamp.target.purpose,
                Purpose::Camera | Purpose::Defaults
            ) || (view.active.as_ref() == Some(&request.path)
                && view.rendition == request.rendition)
                || self.before_overviews.iter().any(|(path, rendition)| {
                    path == &request.path && *rendition == request.rendition
                });
            if !keep {
                retired(entry, "before_view_changed");
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
            let keep = matches!(
                entry.task.stamp.target.purpose,
                Purpose::Current | Purpose::Detail | Purpose::Preview | Purpose::Export
            ) || entry.task.stamp.target.path == path
                || self.before_overviews.iter().any(|(path, rendition)| {
                    path == &entry.task.request.path && *rendition == entry.task.request.rendition
                });
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
            speculative: false,
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
            preview_active: self.active.clone(),
        })
    }
    pub fn busy(&self) -> bool {
        self.desired
            .iter()
            .any(|entry| !entry.settled && entry.task.viewport())
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
    pub fn handle_result(
        &mut self,
        reply: &Reply,
        view: &ViewContext<'_>,
    ) -> (ResultUpdate, Option<Arc<Task>>) {
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
        let update = ResultUpdate {
            current: completion == Completion::Current,
            update_photo: decision.update_photo,
            destination,
        };
        if update.current && update.destination.is_some() {
            if let Some(image) = image {
                if let Some(entry) = self
                    .desired
                    .iter_mut()
                    .find(|entry| entry.task.stamp == reply.stamp)
                {
                    entry.regional = image.artifact.settings.preview_region.is_some();
                }
            }
        }
        // Freeze regional admission before replacing desired work. Camera and
        // Defaults use the same target for their regional and whole phases.
        let started = self.advance_refinement(
            &reply.stamp,
            update,
            image.map(|frame| {
                (
                    &frame.origin,
                    frame.artifact.settings.preview_region.is_some(),
                )
            }),
        );
        (update, started)
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
            entry.delivered = Some(snapshot.clone());
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
            .filter_map(|entry| entry.delivered.clone())
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
    claimed: HashSet<Stamp>,
    last: Option<Target>,
    scopes: Option<TaskId>,
    pick: Option<TaskId>,
    followed: Option<TaskId>,
    waiting: HashSet<Stamp>,
    waiting_pick: Option<TaskId>,
}
impl Cursor {
    /// Keep a photo exclusive while an admitted task executes or awaits delivery.
    /// Replacement plans do not retire physical work already owned by the worker.
    #[cfg(any(feature = "wide-experiment", test))]
    pub fn claim(&mut self, stamp: &Stamp) -> bool {
        if self.path_claimed(&stamp.target.path) {
            return false;
        }
        self.claimed.insert(stamp.clone());
        self.served.insert(stamp.clone());
        true
    }
    /// Release only the retired task, never a newer claim for the same photo.
    pub fn release(&mut self, stamp: &Stamp) {
        self.claimed.remove(stamp);
    }
    fn path_claimed(&self, path: &Path) -> bool {
        self.claimed.iter().any(|stamp| stamp.target.path == path)
    }
    pub fn should_yield(&self, plan: &WorkPlan, task: &Task, export_pending: bool) -> bool {
        let Some(current) = plan.task(&task.stamp.target) else {
            return false;
        };
        let priority = plan.priority(current);
        priority != TaskPriority::Viewport
            && (export_pending
                || plan.tasks.iter().any(|other| {
                    other.stamp.target != current.stamp.target && plan.priority(other) < priority
                })
                || plan
                    .pick
                    .as_ref()
                    .is_some_and(|pick| self.pick != Some(pick.id))
                || (priority > TaskPriority::Auxiliary
                    && plan
                        .scopes
                        .as_ref()
                        .is_some_and(|scope| self.scopes != Some(scope.id))))
    }
    pub fn defer(&mut self, stamp: &Stamp) {
        self.release(stamp);
        self.served.remove(stamp);
        self.waiting.insert(stamp.clone());
        self.followed = None;
    }
    /// A yielded continuation remains desired and can run as soon as foreground work settles.
    pub fn yield_task(&mut self, stamp: &Stamp) {
        self.release(stamp);
        self.served.remove(stamp);
        self.waiting.remove(stamp);
        self.followed = None;
    }
    pub fn retry_pick(&mut self) {
        self.waiting_pick = self.pick.take();
    }
    pub fn ready(&self, stamp: &Stamp, can_retry_source: bool) -> bool {
        // An untried job may reuse presentation or session data. Wait only
        // after execution actually discovers that this job needs a source.
        !self.waiting.contains(stamp) || can_retry_source
    }
    pub fn available(&self, task: &Task, can_retry_source: bool) -> bool {
        !self.path_claimed(&task.stamp.target.path)
            && !self.served.contains(&task.stamp)
            && self.ready(&task.stamp, can_retry_source)
    }
    pub fn retain_waiting(&mut self, plan: &WorkPlan, export: Option<&Stamp>) {
        self.waiting.retain(|stamp| {
            export == Some(stamp) || plan.tasks.iter().any(|task| task.stamp == *stamp)
        });
    }
    pub fn pick_ready(&mut self, plan: &WorkPlan, can_retry_source: bool) -> Option<Arc<PickTask>> {
        if plan
            .pick
            .as_ref()
            .is_some_and(|task| self.waiting_pick == Some(task.id))
            && !can_retry_source
        {
            return None;
        }
        let task = self.pick(plan);
        if task.is_some() {
            self.waiting_pick = None;
        }
        task
    }
    #[cfg(test)]
    pub fn next(&mut self, plan: &WorkPlan) -> Option<Arc<Task>> {
        self.next_ready(plan, |_| true)
    }
    #[cfg(test)]
    pub fn next_ready(
        &mut self,
        plan: &WorkPlan,
        ready: impl Fn(&Task) -> bool,
    ) -> Option<Arc<Task>> {
        self.next_ready_matching(plan, ready, |_| true)
    }
    /// Eligibility filters every candidate. Source readiness only gates tasks
    /// that previously discovered a missing source.
    pub fn next_ready_matching(
        &mut self,
        plan: &WorkPlan,
        ready: impl Fn(&Task) -> bool,
        eligible: impl Fn(&Task) -> bool,
    ) -> Option<Arc<Task>> {
        self.served
            .retain(|stamp| plan.tasks.iter().any(|task| task.stamp == *stamp));
        let len = plan.tasks.len();
        if len == 0 {
            return None;
        }
        // Even a foreground render waiting for source loading precedes background work.
        let priority = plan
            .tasks
            .iter()
            .map(|task| plan.priority(task))
            .min()
            .unwrap();
        if priority != TaskPriority::Viewport
            && (plan
                .scopes
                .as_ref()
                .is_some_and(|task| self.scopes != Some(task.id))
                || plan
                    .pick
                    .as_ref()
                    .is_some_and(|task| self.pick != Some(task.id)))
        {
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
            if plan.priority(task) != priority
                || self.path_claimed(&task.stamp.target.path)
                || !eligible(task)
            {
                continue;
            }
            if self.available(task, ready(task)) && self.served.insert(task.stamp.clone()) {
                self.waiting.remove(&task.stamp);
                self.last = Some(task.stamp.target.clone());
                self.followed = None;
                return Some(task.clone());
            }
        }
        None
    }
    pub fn followup(&mut self, plan: &WorkPlan, previous: &Stamp) -> Option<Arc<Task>> {
        let next = plan.task(&previous.target)?;
        if !self.path_claimed(&next.stamp.target.path)
            && !self.should_yield(plan, next, false)
            && self.followed != Some(previous.id)
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
        if plan.tasks.iter().any(|task| task.viewport()) {
            return None;
        }
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
