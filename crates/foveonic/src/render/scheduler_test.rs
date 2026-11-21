//! Checks admission rules with a recording host over real working photos.

use super::*;
use crate::render::{driver::tests::fixture, driver::tests::settings, NativePhoto, Resolution};
use std::path::{Path, PathBuf};

struct TestJob {
    target: &'static str,
    photo: PathBuf,
    key: PhotoKey,
    /// The photo is not resident until the job loads it alone.
    loads: bool,
}

impl Job for TestJob {
    type Target = &'static str;
    fn target(&self) -> &&'static str {
        &self.target
    }
    fn purpose(&self) -> Purpose {
        Purpose::Edit
    }
    fn photo(&self) -> Option<&Path> {
        Some(&self.photo)
    }
}

#[derive(Default)]
struct RecordingHost {
    /// Each start attempt with whether the job would run alone.
    starts: Vec<(&'static str, bool)>,
    rendered: Vec<&'static str>,
    waiting: Option<&'static str>,
}

impl Host<TestJob> for RecordingHost {
    fn ready(&self, _: &WorkingSet, _: &TestJob, waiting: bool) -> bool {
        !waiting
    }
    fn start(
        &mut self,
        _: &mut Engine,
        _: &mut WorkingSet,
        job: &mut TestJob,
        alone: bool,
    ) -> Start {
        self.starts.push((job.target, alone));
        if self.waiting == Some(job.target) {
            return Start::Waiting;
        }
        if job.loads && !alone {
            return Start::Deferred;
        }
        let mut settings = settings();
        settings.preview_region = Some([0, 0, 64, 64]);
        Start::Render(Box::new(Render {
            key: job.key,
            settings,
            names: vec!["final".into()],
            profile: Profile::default(),
        }))
    }
    fn interruption(&self, _: &TestJob) -> Interruption {
        Interruption::Continue
    }
    fn progress(&self, _: &TestJob, _: RenderProgress) {}
    fn yielded(&mut self, _: &TestJob) {}
    fn finish(&mut self, _: &mut Engine, job: TestJob, outcome: Outcome) {
        assert!(
            matches!(outcome, Outcome::Rendered(_)),
            "{} failed",
            job.target
        );
        self.rendered.push(job.target);
    }
    fn retained(&mut self) -> MemoryReservation {
        MemoryReservation::default()
    }
}

fn scheduler() -> Scheduler<TestJob> {
    let mut scheduler = Scheduler::new(Engine::new().unwrap(), u64::MAX, ImageCache::default());
    let (engine, working) = scheduler.engine_and_working();
    for photo in [1, 2] {
        working
            .insert_native(photo, NativePhoto::new(fixture()), engine, Purpose::Edit)
            .unwrap();
    }
    scheduler
}

fn job(target: &'static str, photo: u64, loads: bool) -> TestJob {
    TestJob {
        target,
        photo: PathBuf::from(photo.to_string()),
        key: PhotoKey {
            photo,
            resolution: Resolution::Native,
        },
        loads,
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn one_job_per_photo_and_loading_waits_until_alone() {
    let mut scheduler = scheduler();
    let mut host = RecordingHost::default();
    scheduler.submit(job("first", 1, false));
    scheduler.submit(job("same photo", 1, false));
    scheduler.submit(job("loads", 2, true));
    assert!(scheduler.tick(&mut host));
    let admitted: Vec<_> = scheduler.admitted().map(|job| job.target).collect();
    assert_eq!(admitted, ["first"]);
    assert_eq!(host.starts, [("first", true), ("loads", false)]);
    while scheduler.tick(&mut host) {}
    assert!(scheduler.is_idle());
    assert_eq!(host.rendered.len(), 3);
    // It retries once after each finished job, not on every tick, and loads alone.
    let loads: Vec<bool> = host
        .starts
        .iter()
        .filter(|(target, _)| *target == "loads")
        .map(|(_, alone)| *alone)
        .collect();
    assert_eq!(loads, [false, false, true]);
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn waiting_job_stays_queued_until_ready() {
    let mut scheduler = scheduler();
    let mut host = RecordingHost {
        waiting: Some("waiting"),
        ..Default::default()
    };
    scheduler.submit(job("waiting", 1, false));
    assert!(scheduler.tick(&mut host));
    assert!(!scheduler.tick(&mut host));
    assert_eq!(host.starts, [("waiting", true)]);
    assert_eq!(scheduler.queued().count(), 1);
}
