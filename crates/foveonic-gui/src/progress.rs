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

#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub revision: u64,
    pub fraction: f32,
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

#[derive(Clone, Default)]
pub struct Tracker(Arc<Mutex<Option<Snapshot>>>);
impl Tracker {
    pub fn begin(&self, revision: u64, started: Instant) {
        *self.0.lock().unwrap() = Some(Snapshot {
            revision,
            fraction: 0.,
            started,
            finished: None,
            failed: false,
        });
    }
    pub fn snapshot(&self) -> Option<Snapshot> {
        *self.0.lock().unwrap()
    }
    pub fn clear(&self) {
        *self.0.lock().unwrap() = None;
    }
    fn advance(&self, revision: u64, fraction: f32) -> bool {
        let mut state = self.0.lock().unwrap();
        let Some(state) = state
            .as_mut()
            .filter(|s| s.revision == revision && s.finished.is_none())
        else {
            return false;
        };
        if fraction > state.fraction && fraction.is_finite() {
            state.fraction = fraction.min(1.);
            return true;
        }
        false
    }
    /// Finish on UI receipt, including worker queueing, GPU completion and reply delivery.
    pub fn finish(&self, revision: u64, failed: bool) {
        let mut state = self.0.lock().unwrap();
        if let Some(state) = state
            .as_mut()
            .filter(|s| s.revision == revision && s.finished.is_none())
        {
            state.finished = Some(Instant::now());
            state.failed = failed;
            if !failed {
                state.fraction = 1.;
            }
        }
    }
}

pub struct Reporter {
    tracker: Tracker,
    revision: u64,
}
impl Reporter {
    pub fn new(tracker: Tracker, revision: u64) -> Self {
        Self { tracker, revision }
    }
    fn advance(&self, fraction: f32) {
        // The UI polls the latest value for its elapsed-time label. Reporting
        // must never wait on the UI context or on the image-reply channel.
        self.tracker.advance(self.revision, fraction);
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
    let fraction = snapshot.map_or(0., |s| s.fraction);
    let mut bar = egui::ProgressBar::new(fraction)
        .desired_width(180.)
        .desired_height(ui.spacing().interact_size.y)
        .animate(false);
    if snapshot.is_some_and(|s| s.failed) {
        bar = bar.fill(ui.visuals().error_fg_color);
    }
    let response = ui.add(bar);
    if show_time {
        ui.monospace(format!(
            "{:.0} ms",
            snapshot.map_or(0., Snapshot::milliseconds)
        ));
    }
    if snapshot.is_some_and(Snapshot::running) && show_time {
        // Only the elapsed label ticks. Progress changes solely at completed stages.
        ui.ctx().request_repaint_after(ELAPSED_REFRESH);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stages_are_discrete_and_older_jobs_cannot_advance_or_finish_a_new_request() {
        let tracker = Tracker::default();
        tracker.begin(1, Instant::now());
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
        tracker.begin(2, Instant::now());
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
        tracker.begin(1, Instant::now() - std::time::Duration::from_millis(250));
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
}
