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
    purpose: Purpose,
    exclusive: bool,
    rank: usize,
}

impl Job for TestJob {
    type Target = &'static str;
    fn target(&self) -> &&'static str {
        &self.target
    }
    fn purpose(&self) -> Purpose {
        self.purpose
    }
    fn exclusive(&self) -> bool {
        self.exclusive
    }
    fn rank(&self) -> usize {
        self.rank
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
    yielded: Vec<&'static str>,
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
    fn yielded(&mut self, job: &TestJob) {
        self.yielded.push(job.target);
    }
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
        purpose: Purpose::Edit,
        exclusive: false,
        rank: 0,
    }
}

fn admitted(scheduler: &Scheduler<TestJob>) -> Vec<&'static str> {
    scheduler.admitted().map(|job| job.target).collect()
}

fn queued(scheduler: &Scheduler<TestJob>) -> Vec<&'static str> {
    scheduler.queued().map(|job| job.target).collect()
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
    assert_eq!(admitted(&scheduler), ["first"]);
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

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn lowering_the_target_retires_the_newest_submitted_job() {
    let mut scheduler = scheduler();
    let mut host = RecordingHost::default();
    // The older job ranks lower, so it is admitted after the newer one.
    scheduler.submit(TestJob {
        rank: 1,
        ..job("older", 1, false)
    });
    scheduler.submit(job("newer", 2, false));
    assert!(scheduler.tick(&mut host));
    assert_eq!(admitted(&scheduler), ["newer", "older"]);
    scheduler.set_target(0);
    assert!(scheduler.tick(&mut host));
    assert_eq!(admitted(&scheduler), ["older"]);
    assert_eq!(queued(&scheduler), ["newer"]);
    while scheduler.tick(&mut host) {}
    assert_eq!(host.rendered, ["older", "newer"]);
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn exclusive_job_starts_after_admitted_jobs_finish() {
    let mut scheduler = scheduler();
    let mut host = RecordingHost::default();
    scheduler.submit(job("first", 1, false));
    scheduler.submit(TestJob {
        exclusive: true,
        ..job("exclusive", 2, false)
    });
    assert!(scheduler.tick(&mut host));
    assert_eq!(admitted(&scheduler), ["first"]);
    while scheduler.tick(&mut host) {}
    assert_eq!(host.starts, [("first", true), ("exclusive", true)]);
    assert_eq!(host.rendered, ["first", "exclusive"]);
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn higher_tier_demand_retires_every_lower_tier_job() {
    let mut scheduler = scheduler();
    let mut host = RecordingHost::default();
    for (target, photo) in [("thumbnail 1", 1), ("thumbnail 2", 2)] {
        scheduler.submit(TestJob {
            purpose: Purpose::Thumbnail,
            ..job(target, photo, false)
        });
    }
    assert!(scheduler.tick(&mut host));
    assert_eq!(admitted(&scheduler), ["thumbnail 1", "thumbnail 2"]);
    scheduler.submit(job("edit", 1, false));
    assert!(scheduler.tick(&mut host));
    assert_eq!(admitted(&scheduler), ["edit"]);
    let mut retired = queued(&scheduler);
    retired.sort();
    assert_eq!(retired, ["thumbnail 1", "thumbnail 2"]);
    host.yielded.sort();
    assert_eq!(host.yielded, ["thumbnail 1", "thumbnail 2"]);
    while scheduler.tick(&mut host) {}
    assert_eq!(host.rendered[0], "edit");
    assert_eq!(host.rendered.len(), 3);
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn width_cap_limits_jobs_running_together() {
    let mut scheduler = scheduler();
    let mut host = RecordingHost::default();
    scheduler.set_width_cap(Some(1));
    scheduler.submit(job("first", 1, false));
    scheduler.submit(job("second", 2, false));
    while scheduler.tick(&mut host) {}
    assert_eq!(host.rendered, ["first", "second"]);
    assert_eq!(scheduler.widest(), 1);
}

/// What two edits of photos 1 and 2 need when nothing else is held.
fn needed_by_two_edits() -> u64 {
    let mut scheduler = scheduler();
    scheduler.submit(job("first", 1, false));
    scheduler.submit(job("second", 2, false));
    assert!(scheduler.tick(&mut RecordingHost::default()));
    assert_eq!(admitted(&scheduler), ["first", "second"]);
    scheduler
        .needed(&MemoryReservation::default(), None)
        .unwrap()
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn admission_releases_working_photos_nothing_needs_to_render_together() {
    let needed = needed_by_two_edits();
    let mut scheduler = scheduler();
    let mut host = RecordingHost::default();
    // Three photos that were rendered earlier and are no longer needed hold
    // their stages, as full-size copies do after zooming back out.
    let leftovers = [3, 4, 5];
    for photo in leftovers {
        let (engine, working) = scheduler.engine_and_working();
        working
            .insert_native(photo, NativePhoto::new(fixture()), engine, Purpose::View)
            .unwrap();
    }
    for (target, photo) in ["third", "fourth", "fifth"].into_iter().zip(leftovers) {
        scheduler.submit(job(target, photo, false));
    }
    while scheduler.tick(&mut host) {}
    scheduler.set_eviction_protected([1, 2].map(|photo| PhotoKey {
        photo,
        resolution: Resolution::Native,
    }));
    // The two edits fit only once leftovers go: the margin is less than the
    // leftovers hold.
    let held = scheduler
        .engine()
        .memory_resources(&scheduler.working, &MemoryReservation::default())
        .total_bytes;
    scheduler.set_target(needed + held / 2);
    scheduler.submit(job("first", 1, false));
    scheduler.submit(job("second", 2, false));
    assert!(scheduler.tick(&mut host));
    assert_eq!(admitted(&scheduler), ["first", "second"]);
    assert!(
        leftovers
            .iter()
            .any(|photo| !scheduler.working.contains(PhotoKey {
                photo: *photo,
                resolution: Resolution::Native,
            })),
        "a leftover was released to make room"
    );
    while scheduler.tick(&mut host) {}
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn admission_keeps_cache_that_could_not_make_room() {
    let needed = needed_by_two_edits();
    let mut scheduler = scheduler();
    let mut host = RecordingHost::default();
    let leftover = PhotoKey {
        photo: 3,
        resolution: Resolution::Native,
    };
    let (engine, working) = scheduler.engine_and_working();
    working
        .insert_native(
            leftover.photo,
            NativePhoto::new(fixture()),
            engine,
            Purpose::View,
        )
        .unwrap();
    scheduler.submit(job("third", leftover.photo, false));
    while scheduler.tick(&mut host) {}
    // The two edits need more than the target on their own, so they take
    // turns whether or not the leftover goes.
    scheduler.set_target(needed / 2);
    scheduler.submit(job("first", 1, false));
    scheduler.submit(job("second", 2, false));
    assert!(scheduler.tick(&mut host));
    assert_eq!(admitted(&scheduler), ["first"]);
    assert!(scheduler.working.contains(leftover));
    while scheduler.tick(&mut host) {}
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn newer_job_for_a_running_target_waits_behind_queued_targets() {
    let mut scheduler = scheduler();
    let mut host = RecordingHost::default();
    scheduler.set_width_cap(Some(1));
    scheduler.submit(job("first", 1, false));
    scheduler.submit(job("second", 2, false));
    assert!(scheduler.tick(&mut host));
    assert_eq!(admitted(&scheduler), ["first"]);
    // A slider keeps moving: the running target's next edit arrives before
    // the second target has run at all.
    scheduler.submit(job("first", 1, false));
    while scheduler.tick(&mut host) {}
    assert_eq!(host.rendered, ["first", "second", "first"]);
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn speculative_peer_never_drops_cached_images() {
    const CACHED_BYTES: u64 = 1 << 50;
    // An edit peer in the same state drops the cached image to run, which shows
    // the peer fits only without it.
    for (purpose, peer_runs) in [(Purpose::Speculative, false), (Purpose::Edit, true)] {
        let mut scheduler = scheduler();
        let mut host = RecordingHost::default();
        let peer = |target, photo| TestJob {
            purpose,
            ..job(target, photo, false)
        };
        scheduler.submit(peer("first", 1));
        assert!(scheduler.tick(&mut host));
        assert_eq!(admitted(&scheduler), ["first"]);
        scheduler.set_target(CACHED_BYTES / 2);
        let mut reservation = MemoryReservation::default();
        reservation.exclusive_bytes = CACHED_BYTES;
        let _cached = scheduler.image_cache.insert((), reservation);
        scheduler.submit(peer("second", 2));
        assert!(scheduler.tick(&mut host));
        assert_eq!(host.starts, [("first", true), ("second", false)]);
        let cached = scheduler.image_cache.reservation().exclusive_bytes;
        if peer_runs {
            assert_eq!(admitted(&scheduler), ["first", "second"]);
            assert_eq!(cached, 0);
        } else {
            assert_eq!(queued(&scheduler), ["second"]);
            assert_eq!(cached, CACHED_BYTES);
        }
    }
}
