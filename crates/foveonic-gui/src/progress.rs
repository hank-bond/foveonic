//! Shows request-to-image time and discrete completed loading and rendering stages.

use crate::rendering::{coordinator::Target, view_plan::ImageState};
use eframe::egui;
use foveonic::{render::RenderProgress, source::LoadStage};
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

// Equal completed sections: read, decode, prepare, execute, display delivery.
// These describe work boundaries, not estimates of elapsed-time proportions.
const SECTION_COUNT: f32 = 5.;
const READ_COMPLETE: f32 = 1. / SECTION_COUNT;
const DECODE_COMPLETE: f32 = 2. / SECTION_COUNT;
const PREPARE_COMPLETE: f32 = 3. / SECTION_COUNT;
const EXECUTION_SHARE: f32 = 1. / SECTION_COUNT;
const ELAPSED_REFRESH: std::time::Duration = std::time::Duration::from_millis(50);
/// The widest elapsed time the footer reserves room for.
const ELAPSED_TEMPLATE: &str = "99999 ms";

#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    /// The pass's first image: the selected photo's, when it is in the pass.
    pub request_id: u64,
    /// The whole pass: the average of its images' progress.
    pub fraction: f32,
    /// How many images the pass strings together, each marked on the bar.
    pub checkpoints: usize,
    started: Instant,
    finished: Option<Instant>,
    pub failed: bool,
}
impl Snapshot {
    pub fn milliseconds(self) -> f64 {
        self.finished
            .unwrap_or_else(Instant::now)
            .saturating_duration_since(self.started)
            .as_secs_f64()
            * 1000.
    }
    pub fn running(self) -> bool {
        self.finished.is_none()
    }
}

/// One update's images, which the bar follows together in one pass. Each
/// section belongs to one image target, so the sections stay fixed while a newer
/// request for the same target takes its section over.
struct Pass {
    images: Vec<Image>,
    started: Instant,
    finished: Option<Instant>,
    failed: bool,
}
struct Image {
    target: Target,
    request_id: u64,
    fraction: f32,
    /// Received or failed.
    done: bool,
    /// Not wanted yet: a whole image waiting for patches to finish.
    waiting: bool,
}
/// One image of an update for the progress pass to follow.
pub struct Section {
    pub target: Target,
    pub request_id: u64,
    /// A ready image's section starts complete; a waiting one keeps the pass open.
    pub state: ImageState,
}
impl Pass {
    fn snapshot(&self) -> Snapshot {
        let total = self.images.len().max(1) as f32;
        Snapshot {
            request_id: self.images.first().map_or(0, |image| image.request_id),
            fraction: self.images.iter().map(|image| image.fraction).sum::<f32>() / total,
            checkpoints: self.images.len(),
            started: self.started,
            finished: self.finished,
            failed: self.failed,
        }
    }
    fn image(&mut self, request_id: u64) -> Option<&mut Image> {
        if self.finished.is_some() {
            return None;
        }
        self.images
            .iter_mut()
            .find(|image| image.request_id == request_id)
    }
}

#[derive(Clone, Default)]
pub struct Tracker(Arc<Mutex<Option<Pass>>>);
impl Tracker {
    /// Follow an update's images. While a pass runs, an image for a target it
    /// already follows takes that section over and other images join it, so one
    /// pass covers the whole update; otherwise a new pass starts. A single
    /// request is a pass of one.
    pub fn begin(&self, sections: Vec<Section>, started: Instant) {
        let image = |section: Section| {
            let ready = section.state == ImageState::Ready;
            Image {
                target: section.target,
                request_id: section.request_id,
                fraction: if ready { 1. } else { 0. },
                done: ready,
                waiting: section.state == ImageState::Waiting,
            }
        };
        let mut pass = self.0.lock().unwrap();
        if let Some(pass) = pass.as_mut().filter(|pass| pass.finished.is_none()) {
            for section in sections {
                match pass
                    .images
                    .iter_mut()
                    .find(|image| image.target == section.target)
                {
                    // A section whose image started or is now current keeps its progress.
                    Some(current)
                        if current.request_id == section.request_id
                            || section.state == ImageState::Ready =>
                    {
                        current.waiting &= section.state == ImageState::Waiting;
                    }
                    Some(current) => *current = image(section),
                    None => pass.images.push(image(section)),
                }
            }
            return;
        }
        *pass = Some(Pass {
            images: sections.into_iter().map(image).collect(),
            started,
            finished: None,
            failed: false,
        });
    }
    pub fn snapshot(&self) -> Option<Snapshot> {
        self.0.lock().unwrap().as_ref().map(Pass::snapshot)
    }
    pub fn clear(&self) {
        *self.0.lock().unwrap() = None;
    }
    fn advance(&self, request_id: u64, fraction: f32) -> bool {
        let mut pass = self.0.lock().unwrap();
        let Some(image) = pass.as_mut().and_then(|pass| pass.image(request_id)) else {
            return false;
        };
        if fraction > image.fraction && fraction.is_finite() {
            image.fraction = fraction.min(1.);
            return true;
        }
        false
    }
    /// Finish an image on UI receipt, including worker queueing, GPU completion
    /// and reply delivery. Other images keep rendering after one fails, so the
    /// pass finishes once every image is done.
    pub fn finish(&self, request_id: u64, failed: bool) {
        let mut pass = self.0.lock().unwrap();
        let Some(pass) = pass.as_mut() else {
            return;
        };
        let Some(image) = pass.image(request_id) else {
            return;
        };
        if !failed {
            image.fraction = 1.;
        }
        image.done = true;
        pass.failed |= failed;
        if pass.images.iter().all(|image| image.done) {
            pass.finished = Some(Instant::now());
        }
    }
    /// Keep image-stage reporting separate from settlement of the desired render
    /// set. Idle work ends the pass unless an image still waits its turn.
    pub fn settle(&self, pending: bool, failed_request_id: Option<u64>) {
        let mut pass = self.0.lock().unwrap();
        let Some(pass) = pass.as_mut().filter(|pass| pass.finished.is_none()) else {
            return;
        };
        if failed_request_id
            .is_some_and(|id| pass.images.iter().any(|image| image.request_id == id))
        {
            pass.failed = true;
        }
        if !pending && !pass.images.iter().any(|image| image.waiting) {
            if !pass.failed {
                for image in &mut pass.images {
                    image.fraction = 1.;
                }
            }
            pass.finished = Some(Instant::now());
        }
    }
}

pub struct Reporter {
    tracker: Tracker,
    request_id: u64,
}
impl Reporter {
    pub fn new(tracker: Tracker, request_id: u64) -> Self {
        Self {
            tracker,
            request_id,
        }
    }
    fn advance(&self, fraction: f32) {
        // The UI polls the latest value for its elapsed-time label. Reporting
        // must never wait on the UI context or on the image-reply channel.
        self.tracker.advance(self.request_id, fraction);
    }
    pub fn loaded(&self, stage: LoadStage) {
        self.advance(match stage {
            LoadStage::FileRead => READ_COMPLETE,
            LoadStage::Decoded => DECODE_COMPLETE,
        });
    }
    pub fn rendered(&self, progress: RenderProgress) {
        match progress {
            RenderProgress::Prepared => self.advance(PREPARE_COMPLETE),
            RenderProgress::Stage { completed, total } => {
                let fraction = if total == 0 {
                    1.
                } else {
                    completed.min(total) as f32 / total as f32
                };
                self.advance(PREPARE_COMPLETE + EXECUTION_SHARE * fraction);
            }
        }
    }
}

pub fn show(ui: &mut egui::Ui, snapshot: Option<Snapshot>, show_time: bool) -> egui::Response {
    let (rect, response) = crate::widgets::footer(ui);
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            let elapsed = format!("{:.0} ms", snapshot.map_or(0., Snapshot::milliseconds));
            let text = ui.painter().layout_no_wrap(
                elapsed,
                egui::TextStyle::Monospace.resolve(ui.style()),
                ui.visuals().text_color(),
            );
            // A fixed width for the label keeps the bar, and its section lines,
            // from moving as the elapsed time gains digits.
            let widest = ui
                .painter()
                .layout_no_wrap(
                    ELAPSED_TEMPLATE.into(),
                    egui::TextStyle::Monospace.resolve(ui.style()),
                    ui.visuals().text_color(),
                )
                .size()
                .x;
            let reserved = if show_time {
                widest + ui.spacing().item_spacing.x
            } else {
                0.
            };
            let width = (rect.width() - reserved).max(1.);
            let mut bar = egui::ProgressBar::new(snapshot.map_or(0., |s| s.fraction))
                .desired_width(width)
                .desired_height(14.)
                .animate(false);
            if snapshot.is_some_and(|s| s.failed) {
                bar = bar.fill(ui.visuals().error_fg_color);
            }
            let bar = ui.add(bar);
            if let Some(snapshot) = snapshot.filter(|snapshot| snapshot.checkpoints > 1) {
                // Mark where each image of a multi-photo pass completes.
                let stroke = egui::Stroke::new(1_f32, ui.visuals().text_color());
                for checkpoint in 1..snapshot.checkpoints {
                    let x = egui::lerp(
                        bar.rect.left()..=bar.rect.right(),
                        checkpoint as f32 / snapshot.checkpoints as f32,
                    );
                    ui.painter().vline(x, bar.rect.y_range(), stroke);
                }
            }
            if show_time {
                ui.label(text);
            }
        },
    );
    if snapshot.is_some_and(Snapshot::running) && show_time {
        // Only the elapsed label ticks. Progress changes solely at completed stages.
        ui.ctx().request_repaint_after(ELAPSED_REFRESH);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::request::RequestId;

    /// A section following request `id` for the whole image of `photo`.
    fn section(photo: &str, id: u64) -> Section {
        Section {
            target: Target {
                path: photo.into(),
                purpose: crate::rendering::request::Purpose::Current,
                patch: false,
            },
            request_id: id,
            state: ImageState::Render,
        }
    }

    fn desire(
        coordinator: &mut crate::rendering::coordinator::RenderCoordinator,
        path: &str,
        request_id: u64,
    ) -> crate::rendering::coordinator::Stamp {
        let request = Arc::new(crate::rendering::request::RenderRequest {
            detail: false,
            framing_preview: false,
            calibration: None,
            rendition: crate::editor::Rendition::Current,
            id: RequestId(request_id),
            path: path.into(),
            settings: foveonic::render::Settings::new("Sunlight", "Standard"),
            preview_scale: foveonic::render::Scale::new(8).unwrap(),
            processing_quality: foveonic::render::ProcessingQuality::Speed,
            output: "final".into(),
            comparison: false,
        });
        let target = crate::rendering::coordinator::Target::of(&request);
        coordinator.desire(request, None, Instant::now(), false);
        coordinator.desired(&target).unwrap().stamp.clone()
    }

    #[test]
    fn sibling_completion_order_cannot_leave_settled_progress_running() {
        let mut coordinator = crate::rendering::coordinator::RenderCoordinator::default();
        let a = desire(&mut coordinator, "a.x3f", 1);
        let b = desire(&mut coordinator, "b.x3f", 2);
        let tracker = Tracker::default();
        let began = Instant::now();
        tracker.begin(vec![section("b.x3f", b.request_id.0)], began);
        Reporter::new(tracker.clone(), b.request_id.0).rendered(RenderProgress::Stage {
            completed: 1,
            total: 1,
        });

        coordinator.finish(&b);
        tracker.settle(coordinator.busy(), None);
        assert!(tracker.snapshot().unwrap().running());
        assert_eq!(tracker.snapshot().unwrap().fraction, 0.8);

        coordinator.finish(&a);
        tracker.settle(coordinator.busy(), None);
        let done = tracker.snapshot().unwrap();
        assert!(!done.running());
        assert_eq!(done.fraction, 1.);
        assert_eq!(done.request_id, b.request_id.0);
        assert_eq!(done.started, began);
    }

    #[test]
    fn retiring_last_desire_settles_without_another_image_reply() {
        let mut coordinator = crate::rendering::coordinator::RenderCoordinator::default();
        let a = desire(&mut coordinator, "a.x3f", 1);
        let b = desire(&mut coordinator, "b.x3f", 2);
        let tracker = Tracker::default();
        tracker.begin(vec![section("b.x3f", b.request_id.0)], Instant::now());
        coordinator.finish(&a);
        tracker.settle(coordinator.busy(), None);
        assert!(tracker.snapshot().unwrap().running());

        coordinator.reconcile(
            &crate::rendering::coordinator::Photos {
                loading: vec!["a.x3f".into()],
                history: vec![],
                ..Default::default()
            },
            &[],
        );
        tracker.settle(coordinator.busy(), None);
        assert!(!tracker.snapshot().unwrap().running());
        assert_eq!(tracker.snapshot().unwrap().fraction, 1.);
    }

    #[test]
    fn stages_are_discrete_and_older_jobs_cannot_advance_or_finish_a_new_request() {
        let tracker = Tracker::default();
        tracker.begin(vec![section("a.x3f", 1)], Instant::now());
        let reporter = Reporter::new(tracker.clone(), 1);
        reporter.loaded(LoadStage::FileRead);
        assert_eq!(tracker.snapshot().unwrap().fraction, 0.2);
        reporter.loaded(LoadStage::Decoded);
        assert_eq!(tracker.snapshot().unwrap().fraction, 0.4);
        reporter.rendered(RenderProgress::Prepared);
        assert_eq!(tracker.snapshot().unwrap().fraction, 0.6);
        reporter.rendered(RenderProgress::Stage {
            completed: 2,
            total: 4,
        });
        assert!((tracker.snapshot().unwrap().fraction - 0.7).abs() < 1e-6);
        reporter.loaded(LoadStage::FileRead);
        assert!((tracker.snapshot().unwrap().fraction - 0.7).abs() < 1e-6);
        // Request 2 replaces request 1 in the same section.
        tracker.begin(vec![section("a.x3f", 2)], Instant::now());
        reporter.rendered(RenderProgress::Stage {
            completed: 4,
            total: 4,
        });
        tracker.finish(1, false);
        let latest = tracker.snapshot().unwrap();
        assert_eq!(latest.fraction, 0.);
        assert!(latest.running());
        tracker.finish(2, false);
        let done = tracker.snapshot().unwrap();
        assert_eq!(done.fraction, 1.);
        assert!(!done.running());
        assert_eq!(
            done.milliseconds(),
            tracker.snapshot().unwrap().milliseconds()
        );
    }
    #[test]
    fn cached_render_and_failure_keep_truthful_completion_and_total_time() {
        let tracker = Tracker::default();
        tracker.begin(
            vec![section("a.x3f", 1)],
            Instant::now() - std::time::Duration::from_millis(250),
        );
        let reporter = Reporter::new(tracker.clone(), 1);
        reporter.loaded(LoadStage::Decoded);
        reporter.rendered(RenderProgress::Stage {
            completed: 0,
            total: 0,
        });
        assert!((tracker.snapshot().unwrap().fraction - 0.8).abs() < 1e-6);
        tracker.finish(1, true);
        let failed = tracker.snapshot().unwrap();
        assert!(failed.failed);
        assert!(!failed.running());
        assert!(failed.fraction < 1.);
        assert!(failed.milliseconds() >= 250.);
        tracker.clear();
        assert!(tracker.snapshot().is_none());
    }

    #[test]
    fn a_multi_photo_update_is_one_pass_with_a_checkpoint_per_image() {
        let tracker = Tracker::default();
        tracker.begin(
            vec![
                section("a.x3f", 1),
                section("b.x3f", 2),
                section("c.x3f", 3),
            ],
            Instant::now(),
        );
        let snapshot = tracker.snapshot().unwrap();
        assert_eq!(snapshot.checkpoints, 3);
        assert_eq!(
            snapshot.request_id, 1,
            "the selected photo's image leads the pass"
        );
        tracker.finish(2, false);
        assert!((tracker.snapshot().unwrap().fraction - 1. / 3.).abs() < 1e-6);
        assert!(tracker.snapshot().unwrap().running());
        Reporter::new(tracker.clone(), 1).loaded(LoadStage::Decoded);
        tracker.finish(1, false);
        tracker.finish(3, false);
        let done = tracker.snapshot().unwrap();
        assert!(!done.running());
        assert_eq!(done.fraction, 1.);
    }

    #[test]
    fn a_failed_image_marks_the_pass_but_the_others_keep_it_running() {
        let tracker = Tracker::default();
        tracker.begin(
            vec![section("a.x3f", 1), section("b.x3f", 2)],
            Instant::now(),
        );
        tracker.finish(1, true);
        let snapshot = tracker.snapshot().unwrap();
        assert!(snapshot.failed && snapshot.running());
        tracker.begin(vec![section("c.x3f", 3)], Instant::now());
        assert_eq!(
            tracker.snapshot().unwrap().checkpoints,
            3,
            "work starting during the pass joins it"
        );
        tracker.finish(2, false);
        tracker.finish(3, false);
        assert!(!tracker.snapshot().unwrap().running());
    }

    #[test]
    fn sections_stay_fixed_while_newer_requests_and_waiting_images_take_them_over() {
        let tracker = Tracker::default();
        let mut waiting = section("a.x3f", 2);
        waiting.state = ImageState::Waiting;
        let mut patch = section("a.x3f", 1);
        patch.target.patch = true;
        tracker.begin(vec![patch, waiting], Instant::now());
        tracker.finish(1, false);
        tracker.settle(false, None);
        assert!(
            tracker.snapshot().unwrap().running(),
            "the whole image waiting for patches keeps the pass open"
        );
        tracker.begin(vec![section("a.x3f", 5)], Instant::now());
        assert_eq!(tracker.snapshot().unwrap().checkpoints, 2);
        tracker.finish(5, false);
        assert!(!tracker.snapshot().unwrap().running());
    }

    #[test]
    fn photos_already_current_count_as_complete_sections_of_the_pass() {
        let tracker = Tracker::default();
        let mut current = section("a.x3f", 1);
        current.state = ImageState::Ready;
        tracker.begin(vec![current, section("b.x3f", 2)], Instant::now());
        let snapshot = tracker.snapshot().unwrap();
        assert_eq!(snapshot.checkpoints, 2);
        assert_eq!(snapshot.fraction, 0.5);
        tracker.finish(2, false);
        assert!(!tracker.snapshot().unwrap().running());
    }
}
