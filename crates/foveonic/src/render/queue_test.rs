//! Checks tier order, rank, submission order, replacement and waiting jobs.

use super::*;
use crate::render::Purpose;

#[derive(Debug, PartialEq)]
struct Work {
    target: &'static str,
    priority: Priority,
    revision: u32,
    rank: usize,
}

impl Job for Work {
    type Target = &'static str;
    fn target(&self) -> &&'static str {
        &self.target
    }
    fn purpose(&self) -> Purpose {
        match self.priority {
            Priority::Viewport => Purpose::Edit,
            Priority::Auxiliary => Purpose::Auxiliary,
            Priority::Thumbnail => Purpose::Thumbnail,
            Priority::Export => Purpose::Export,
            Priority::Speculative => Purpose::Speculative,
        }
    }
    fn rank(&self) -> usize {
        self.rank
    }
}

fn work(target: &'static str, priority: Priority, revision: u32) -> Work {
    Work {
        target,
        priority,
        revision,
        rank: 0,
    }
}

fn next(queue: &Queue<Work>) -> Option<(&'static str, u32)> {
    let tier = queue.best_tier()?;
    queue
        .next(tier, |queued| !queued.waiting)
        .map(|queued| (queued.job.target, queued.job.revision))
}

#[test]
fn higher_tiers_come_first_then_submission_order() {
    let mut queue = Queue::new();
    queue.submit(work("thumb", Priority::Thumbnail, 1), None);
    queue.submit(work("b", Priority::Viewport, 1), None);
    queue.submit(work("a", Priority::Viewport, 1), None);
    assert_eq!(next(&queue), Some(("b", 1)));
    queue.take(&"b");
    assert_eq!(next(&queue), Some(("a", 1)));
    queue.take(&"a");
    assert_eq!(next(&queue), Some(("thumb", 1)));
}

#[test]
fn rank_orders_a_tier_before_submission_and_follows_replacement() {
    let ranked = |target, revision, rank| Work {
        rank,
        ..work(target, Priority::Viewport, revision)
    };
    let mut queue = Queue::new();
    queue.submit(ranked("whole", 1, 1), None);
    queue.submit(ranked("patch", 1, 0), None);
    assert_eq!(
        next(&queue),
        Some(("patch", 1)),
        "the better rank starts first"
    );
    queue.submit(ranked("whole", 2, 0), None);
    queue.submit(ranked("patch", 2, 1), None);
    assert_eq!(
        next(&queue),
        Some(("whole", 2)),
        "a resubmission takes its new rank"
    );
}

#[test]
fn reranking_in_place_keeps_a_waiting_job_waiting() {
    let mut queue = Queue::new();
    queue.submit(work("a", Priority::Viewport, 1), None);
    queue.submit(work("b", Priority::Viewport, 1), None);
    let mut a = queue.take(&"a").unwrap();
    a.waiting = true;
    queue.put_back(a);
    queue.update(|job| job.rank = if job.target == "a" { 0 } else { 1 });
    assert_eq!(next(&queue), Some(("b", 1)), "a still waits for its input");
}

#[test]
fn replacement_keeps_its_place_and_takes_the_new_priority() {
    let mut queue = Queue::new();
    queue.submit(work("a", Priority::Viewport, 1), None);
    queue.submit(work("b", Priority::Viewport, 1), None);
    queue.submit(work("a", Priority::Viewport, 2), None);
    assert_eq!(next(&queue), Some(("a", 2)), "a slider drag keeps its turn");
    queue.submit(work("a", Priority::Speculative, 3), None);
    assert_eq!(next(&queue), Some(("b", 1)));
    assert_eq!(queue.jobs().count(), 2);
}

#[test]
fn a_running_target_keeps_its_place_and_put_back_never_overwrites_newer_work() {
    let mut queue = Queue::new();
    queue.submit(work("a", Priority::Viewport, 1), None);
    queue.submit(work("b", Priority::Viewport, 1), None);
    let running = queue.take(&"a").unwrap();
    queue.submit(work("a", Priority::Viewport, 2), Some(running.order));
    assert_eq!(next(&queue), Some(("a", 2)));
    queue.put_back(running);
    assert_eq!(next(&queue), Some(("a", 2)), "the newer job stays");
    assert_eq!(queue.jobs().count(), 2);
}

#[test]
fn waiting_jobs_hold_back_lower_tiers_but_not_ready_peers() {
    let mut queue = Queue::new();
    queue.submit(work("loading", Priority::Viewport, 1), None);
    queue.submit(work("ready", Priority::Viewport, 1), None);
    queue.submit(work("thumb", Priority::Thumbnail, 1), None);
    let mut loading = queue.take(&"loading").unwrap();
    loading.waiting = true;
    queue.put_back(loading);
    assert_eq!(next(&queue), Some(("ready", 1)));
    queue.take(&"ready");
    assert_eq!(queue.best_tier(), Some(Priority::Viewport));
    assert_eq!(
        next(&queue),
        None,
        "thumbnails wait behind a loading viewport job"
    );
}

#[test]
fn deferred_speculative_work_neither_runs_nor_holds_a_tier_until_memory_changes() {
    let mut queue = Queue::new();
    queue.submit(work("whole", Priority::Speculative, 1), None);
    let mut queued = queue.take(&"whole").unwrap();
    queued.deferred = true;
    queue.put_back(queued);
    assert_eq!(queue.best_tier(), None);
    queue.memory_changed();
    assert_eq!(next(&queue), Some(("whole", 1)));
}
