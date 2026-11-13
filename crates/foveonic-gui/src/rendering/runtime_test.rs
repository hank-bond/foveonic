//! Exercises real coordinator handoffs with a controlled executor and bounded image queues.

use super::*;
use crate::{preferences::PreviewMode, rendering::coordinator::Task};
use foveonic::render::Settings;
use std::time::Duration;

struct Rig {
    _sources: super::super::source_loader::Handle,
    handle: Handle,
    slot: Slot,
    results: Sender<Update>,
    updates: Receiver<Update>,
    progress: crate::progress::Tracker,
}
impl Rig {
    fn new() -> Self {
        let slot = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let progress = crate::progress::Tracker::default();
        let (results, input) = crossbeam_channel::bounded(2);
        let cache = Arc::new(Cache::new());
        let (sources, loader, _) =
            super::super::source_loader::start(cache.clone(), progress.clone(), slot.clone());
        let (handle, updates) = Handle::start(
            slot.clone(),
            input,
            cache,
            progress.clone(),
            Default::default(),
            loader,
        );
        Self {
            _sources: sources,
            handle,
            slot,
            results,
            updates,
            progress,
        }
    }
    fn requests(&self, requests: Vec<Request>) {
        self.handle.set_view(view(&requests));
        let paths = requests
            .iter()
            .map(|request| request.path.clone())
            .collect();
        self.handle
            .overviews(requests, paths, vec![], Instant::now());
        self.applied();
    }
    fn applied(&self) {
        let sequence = self.handle.sequence.get();
        wait(|| self.handle.status().processed >= sequence);
    }
    fn tasks(&self) -> Vec<Arc<Task>> {
        self.slot.0.lock().unwrap().plan.tasks.clone()
    }
    fn receive(&self) -> worker::Reply {
        let Update::Image(reply) = self.updates.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("expected an image result");
        };
        *reply
    }
}

fn wait(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "coordinator failed to make progress"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn request(path: &str, revision: u64, exposure: f32) -> Request {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.exposure = exposure;
    Request {
        path: path.into(),
        revision,
        settings,
        framing_preview: false,
        calibration: None,
        rendition: Default::default(),
        preview_quality: PreviewMode::Thumb,
        preview_policy: PreviewMode::Thumb,
        output: "final".into(),
        comparison: false,
        export: None,
    }
}
fn view(requests: &[Request]) -> ViewState {
    let active = &requests[0];
    ViewState {
        active: Some(active.path.clone()),
        rendition: active.rendition,
        output: active.output.clone(),
        comparison: active.comparison,
        framing_preview: active.framing_preview,
        exporting: false,
        calibration: None,
        settings: active.settings.clone(),
        overviews: requests
            .iter()
            .map(|request| crate::rendering::delivery::PhotoView {
                path: request.path.clone(),
                settings: request.settings.clone(),
                displayed_revision: None,
                detail_revision: None,
            })
            .collect(),
        displayed_revision: None,
        detail_revision: None,
    }
}
fn result(task: &Task) -> Update {
    Update::Image(Box::new(worker::Reply {
        request: task.request.clone(),
        handling: None,
        stamp: task.stamp.clone(),
        path: task.request.path.clone(),
        rendition: task.request.rendition,
        revision: task.request.revision,
        metadata: None,
        settings: task.request.settings.clone(),
        numeric_settings: None,
        reset_settings: None,
        result: Err("test source failure".into()),
        exported: None,
    }))
}

#[test]
fn inactive_photo_delivery_uses_workspace_processing_preferences() {
    let rig = Rig::new();
    let mut preferences = crate::preferences::Preferences::default();
    preferences.adaptive_tone_tuning.natural.shadows = 0.4;
    let mut inactive = request("b.x3f", 2, 0.);
    let authored = inactive.settings.clone();
    inactive.settings = preferences.render_settings(&authored);
    rig.requests(vec![request("a.x3f", 1, 0.), inactive]);
    let task = rig
        .tasks()
        .into_iter()
        .find(|task| task.request.path == std::path::Path::new("b.x3f"))
        .unwrap();
    rig.results.send(result(&task)).unwrap();
    let delivered = rig.receive();
    let snapshot = view(&[request("a.x3f", 1, 0.), (*task.request).clone()]);
    let context = snapshot.context(&delivered.path);
    assert!(!super::super::delivery::recheck(&delivered, &context, Some(&authored)).current);
    let effective = preferences.render_settings(&authored);
    let update = super::super::delivery::recheck(&delivered, &context, Some(&effective));
    assert!(update.current && update.update_photo);
    assert_ne!(
        authored.adaptive_tone_tuning,
        effective.adaptive_tone_tuning
    );
}

#[test]
fn coordinator_replaces_each_photo_and_keeps_siblings_without_ui_receipt() {
    let rig = Rig::new();
    for revision in 1..=12 {
        rig.requests(
            (0..8)
                .map(|i| request(&format!("{i}.x3f"), revision * 8 + i, revision as f32))
                .collect(),
        );
    }
    let tasks = rig.tasks();
    assert_eq!(tasks.len(), 8);
    assert!(tasks
        .iter()
        .all(|task| task.request.settings.exposure == 12.));
    rig.handle.retain(vec!["0.x3f".into()], vec![]);
    rig.applied();
    assert_eq!(rig.tasks().len(), 1);
    rig.handle.clear(false);
    rig.applied();
    assert!(rig.tasks().is_empty());
    assert!(!rig.handle.busy());
}

#[test]
fn handled_results_settle_siblings_and_cannot_settle_a_newer_request() {
    let rig = Rig::new();
    rig.requests(vec![request("a.x3f", 1, 0.), request("b.x3f", 2, 0.)]);
    let old = rig.tasks();
    rig.requests(vec![request("a.x3f", 3, 1.), request("b.x3f", 4, 0.)]);
    rig.results.send(result(&old[0])).unwrap();
    assert!(!rig.receive().handling.unwrap().current);
    assert_eq!(rig.tasks().len(), 2);
    rig.results.send(result(&old[1])).unwrap();
    assert!(rig.receive().handling.unwrap().current);
    assert!(rig.handle.busy());
    let latest = rig.tasks();
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].request.revision, 3);
    rig.results.send(result(&latest[0])).unwrap();
    assert!(rig.receive().handling.unwrap().current);
    assert!(!rig.handle.busy());
}

#[test]
fn full_ui_queue_does_not_prevent_new_intent_or_shutdown() {
    let rig = Rig::new();
    rig.requests(
        (0..8)
            .map(|i| request(&format!("{i}.x3f"), i + 1, 0.))
            .collect(),
    );
    let tasks = rig.tasks();
    for task in &tasks[..4] {
        rig.results.send(result(task)).unwrap();
    }
    wait(|| rig.updates.is_full() && rig.tasks().len() <= 5);
    rig.handle.submit(request("0.x3f", 20, 2.), Instant::now());
    rig.applied();
    assert!(rig.tasks().iter().any(|task| task.request.revision == 20));
    rig.handle.clear(true);
    rig.applied();
    assert!(rig.tasks().is_empty());
    assert!(!rig.handle.busy());
    let slot = rig.slot.clone();
    drop(rig);
    wait(|| slot.0.lock().unwrap().stop);
}

#[test]
fn coalescing_a_completed_request_preserves_its_finished_elapsed_time() {
    let rig = Rig::new();
    rig.requests(vec![request("a.x3f", 1, 0.)]);
    rig.results.send(result(&rig.tasks()[0])).unwrap();
    rig.receive();
    rig.progress.settle(rig.handle.busy(), Some(1));
    let finished = rig.progress.snapshot().unwrap();
    assert!(!finished.running());
    rig.requests(vec![request("a.x3f", 2, 0.)]);
    assert!(!rig.handle.busy());
    let coalesced = rig.progress.snapshot().unwrap();
    assert_eq!(finished.revision, coalesced.revision);
    assert_eq!(finished.milliseconds(), coalesced.milliseconds());
    rig.requests(vec![request("a.x3f", 3, 1.)]);
    assert!(rig.handle.busy());
    assert_eq!(rig.progress.snapshot().unwrap().revision, 3);
}

#[test]
fn ui_uses_its_snapshot_while_the_coordinator_is_publishing() {
    let rig = Rig::new();
    rig.requests(vec![request("a.x3f", 1, 0.)]);
    assert!(rig.handle.busy());
    let locked = rig.handle.status.lock().unwrap();
    // A blocking read here would deadlock on the lock held by this thread.
    assert!(rig.handle.busy());
    drop(locked);
}

#[test]
fn a_failure_in_transit_cannot_overwrite_a_newer_ui_edit() {
    let rig = Rig::new();
    rig.requests(vec![request("a.x3f", 1, 0.)]);
    rig.results.send(result(&rig.tasks()[0])).unwrap();
    let reply = rig.receive();
    assert!(reply.handling.unwrap().current);
    let newer = view(&[request("a.x3f", 2, 1.)]);
    let update =
        super::super::delivery::recheck(&reply, &newer.context(&reply.path), Some(&newer.settings));
    assert!(!update.current);
    assert!(!update.update_photo);
    assert!(update.destination.is_none());
}

#[test]
fn refinement_publication_coalesces_source_binding_and_panning_without_phantom_revision() {
    let rig = Rig::new();
    let mut first = request("a.x3f", 1, 0.);
    first.preview_policy = PreviewMode::Smart;
    first.preview_quality = PreviewMode::Full;
    first.settings.preview_region = Some([0, 0, 128, 128]);
    rig.handle.set_view(view(&[first.clone()]));
    let initial = super::super::request::Refinement { first, whole: None };
    assert!(rig.handle.refine(initial.clone(), None, Instant::now()));
    rig.applied();
    assert_eq!(rig.progress.snapshot().unwrap().revision, 1);
    let mut pan = initial;
    pan.first.revision = 3;
    pan.first.settings.preview_region = Some([100, 100, 228, 228]);
    assert!(!rig
        .handle
        .refine(pan, Some(SourceOrigin::default()), Instant::now()));
    rig.applied();
    assert_eq!(rig.tasks()[0].stamp.revision, 1);
    assert_eq!(rig.progress.snapshot().unwrap().revision, 1);
    let mut replacement = request("a.x3f", 5, 0.);
    replacement.preview_policy = PreviewMode::Smart;
    replacement.preview_quality = PreviewMode::Full;
    replacement.settings.preview_region = Some([0, 0, 128, 128]);
    assert!(rig.handle.refine(
        super::super::request::Refinement {
            first: replacement,
            whole: None
        },
        Some(SourceOrigin::default()),
        Instant::now()
    ));
    rig.applied();
    assert_eq!(rig.tasks()[0].stamp.revision, 5);
}

#[test]
fn complete_demand_retains_current_detail_but_removal_forgets_its_suppressed_intent() {
    let rig = Rig::new();
    let mut first = request("a.x3f", 1, 0.);
    first.preview_policy = PreviewMode::Smart;
    first.preview_quality = PreviewMode::Full;
    first.settings.preview_region = Some([32, 32, 160, 160]);
    let mut whole = first.clone();
    whole.revision = 2;
    whole.settings.preview_region = None;
    let intent = Refinement {
        first: first.clone(),
        whole: Some(whole),
    };
    assert!(rig.handle.refine(intent.clone(), None, Instant::now()));
    rig.handle.activate("b.x3f".into());
    rig.handle.retire_before("a.x3f".into());
    rig.handle.overviews(
        vec![request("b.x3f", 3, 0.)],
        vec!["a.x3f".into(), "b.x3f".into()],
        vec![],
        Instant::now(),
    );
    rig.applied();
    assert_eq!(rig.tasks().len(), 2);
    assert!(rig
        .tasks()
        .iter()
        .any(|task| task.request.path == first.path
            && task.request.settings.preview_region.is_some()));
    assert!(
        !rig.handle.refine(intent.clone(), None, Instant::now()),
        "selection must preserve intent suppression along with work"
    );
    rig.handle
        .retain(vec!["b.x3f".into()], vec!["a.x3f".into()]);
    rig.applied();
    assert!(!rig
        .tasks()
        .iter()
        .any(|task| task.request.path == first.path));
    assert!(
        rig.handle.refine(intent, None, Instant::now()),
        "re-adding a removed photo must republish its intent"
    );
}
