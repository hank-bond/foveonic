//! Shows request-to-image time and discrete completed loading and rendering stages.

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

/// Where one image of the update stands, as the runtime publishes it from the
/// plan and the coordinator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionState {
    /// Already current when the update began.
    Ready,
    /// A whole image waiting for patches to finish.
    Waiting,
    Running,
    Done,
    Failed,
}
#[derive(Clone, Copy, Debug)]
pub struct Section {
    /// The request whose work the section follows.
    pub request_id: u64,
    pub state: SectionState,
}

/// The update the bar follows and how far each running request has come. The
/// runtime publishes the sections and timing; the worker reports the fractions.
/// The bar's snapshot is computed from both, so it keeps no lifecycle of its own.
#[derive(Default)]
struct Shared {
    sections: Vec<Section>,
    started: Option<Instant>,
    finished: Option<Instant>,
    fractions: std::collections::HashMap<u64, f32>,
}

#[derive(Clone, Default)]
pub struct Tracker(Arc<Mutex<Shared>>);
impl Tracker {
    /// The runtime publishes the update's sections, started when its work began
    /// and finished once every section is done.
    pub fn publish(&self, sections: Vec<Section>, started: Instant, finished: Option<Instant>) {
        let mut shared = self.0.lock().unwrap();
        // Fractions only matter for requests still in the pass.
        shared
            .fractions
            .retain(|id, _| sections.iter().any(|section| section.request_id == *id));
        shared.sections = sections;
        shared.started = Some(started);
        shared.finished = finished;
    }
    pub fn snapshot(&self) -> Option<Snapshot> {
        let shared = self.0.lock().unwrap();
        let started = shared.started?;
        let reported = |section: &Section| {
            shared
                .fractions
                .get(&section.request_id)
                .copied()
                .unwrap_or(0.)
        };
        let fraction = |section: &Section| match section.state {
            SectionState::Ready | SectionState::Done => 1.,
            SectionState::Waiting => 0.,
            SectionState::Running | SectionState::Failed => reported(section),
        };
        let total = shared.sections.len().max(1) as f32;
        Some(Snapshot {
            request_id: shared
                .sections
                .first()
                .map_or(0, |section| section.request_id),
            fraction: shared.sections.iter().map(fraction).sum::<f32>() / total,
            checkpoints: shared.sections.len(),
            started,
            finished: shared.finished,
            failed: shared
                .sections
                .iter()
                .any(|section| section.state == SectionState::Failed),
        })
    }
    pub fn clear(&self) {
        *self.0.lock().unwrap() = Shared::default();
    }
    fn advance(&self, request_id: u64, fraction: f32) {
        if !fraction.is_finite() {
            return;
        }
        let mut shared = self.0.lock().unwrap();
        let current = shared.fractions.entry(request_id).or_insert(0.);
        *current = current.max(fraction.min(1.));
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

    fn section(request_id: u64, state: SectionState) -> Section {
        Section { request_id, state }
    }

    #[test]
    fn stages_are_discrete_and_the_bar_reads_each_running_requests_fraction() {
        let tracker = Tracker::default();
        let started = Instant::now();
        tracker.publish(vec![section(1, SectionState::Running)], started, None);
        let reporter = Reporter::new(tracker.clone(), 1);
        reporter.loaded(LoadStage::FileRead);
        assert_eq!(tracker.snapshot().unwrap().fraction, 0.2);
        reporter.loaded(LoadStage::Decoded);
        reporter.rendered(RenderProgress::Prepared);
        assert_eq!(tracker.snapshot().unwrap().fraction, 0.6);
        reporter.rendered(RenderProgress::Stage {
            completed: 2,
            total: 4,
        });
        assert!((tracker.snapshot().unwrap().fraction - 0.7).abs() < 1e-6);
        reporter.loaded(LoadStage::FileRead);
        assert!(
            (tracker.snapshot().unwrap().fraction - 0.7).abs() < 1e-6,
            "a stage never moves the bar back"
        );
    }

    #[test]
    fn ready_and_done_sections_are_full_waiting_ones_empty_and_failures_mark_the_bar() {
        let tracker = Tracker::default();
        tracker.publish(
            vec![
                section(1, SectionState::Ready),
                section(2, SectionState::Done),
                section(3, SectionState::Waiting),
                section(4, SectionState::Failed),
            ],
            Instant::now(),
            None,
        );
        let snapshot = tracker.snapshot().unwrap();
        assert_eq!(snapshot.checkpoints, 4);
        assert_eq!(snapshot.fraction, 0.5);
        assert!(snapshot.failed && snapshot.running());
        assert_eq!(snapshot.request_id, 1, "the first section leads the pass");
    }

    #[test]
    fn a_finished_pass_keeps_its_elapsed_time() {
        let tracker = Tracker::default();
        let started = Instant::now() - std::time::Duration::from_millis(250);
        let finished = Instant::now();
        tracker.publish(
            vec![section(1, SectionState::Done)],
            started,
            Some(finished),
        );
        let snapshot = tracker.snapshot().unwrap();
        assert!(!snapshot.running());
        assert!(snapshot.milliseconds() >= 250.);
        assert_eq!(
            snapshot.milliseconds(),
            tracker.snapshot().unwrap().milliseconds()
        );
    }
}
