//! Captures immutable app images and controls delivery timing for opt-in native checks.

use super::Pending;
use crossbeam_channel::{bounded, Receiver, Sender};
use foveonic::{image::RenderedImage, render::Engine};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
};

struct Capture {
    image: RenderedImage,
    destination: PathBuf,
    completed: Sender<Result<(), String>>,
}

struct Hold {
    path: PathBuf,
    revision: u64,
    reached: Sender<()>,
    release: Receiver<()>,
}

/// Dropping this handle releases the worker, including when a scenario fails.
pub(crate) struct DeliveryHold {
    pub reached: Receiver<()>,
    _release: Sender<()>,
}

pub(super) struct Handle {
    captures: Sender<Capture>,
    holds: Sender<Hold>,
    wake: Arc<(Mutex<Pending>, Condvar)>,
}

pub(super) struct Service {
    captures: Receiver<Capture>,
    holds: Receiver<Hold>,
    hold: Option<Hold>,
}

pub(super) fn channel(wake: Arc<(Mutex<Pending>, Condvar)>) -> (Handle, Service) {
    let (captures, capture_requests) = bounded(8);
    let (holds, hold_requests) = bounded(1);
    (
        Handle {
            captures,
            holds,
            wake,
        },
        Service {
            captures: capture_requests,
            holds: hold_requests,
            hold: None,
        },
    )
}

impl Handle {
    pub fn capture(
        &self,
        image: RenderedImage,
        destination: PathBuf,
    ) -> Receiver<Result<(), String>> {
        let (completed, result) = bounded(1);
        if let Err(error) = self.captures.try_send(Capture {
            image,
            destination,
            completed,
        }) {
            let _ = error
                .into_inner()
                .completed
                .try_send(Err("image capture worker unavailable or full".into()));
        }
        self.wake.1.notify_one();
        result
    }

    pub fn hold_delivery(&self, path: PathBuf, revision: u64) -> DeliveryHold {
        let (reached, arrival) = bounded(1);
        let (release, resume) = bounded(1);
        let _ = self.holds.try_send(Hold {
            path,
            revision,
            reached,
            release: resume,
        });
        DeliveryHold {
            reached: arrival,
            _release: release,
        }
    }
}

impl Service {
    pub fn pending_capture(&self) -> bool {
        !self.captures.is_empty()
    }

    pub fn capture_one(&self, engine: &Engine) -> bool {
        let Ok(capture) = self.captures.try_recv() else {
            return false;
        };
        let result = engine
            .readback_image(&capture.image)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                use std::io::Write;
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&capture.destination)
                    .and_then(|mut file| file.write_all(&bytes))
                    .map_err(|error| error.to_string())
            });
        let _ = capture.completed.send(result);
        true
    }

    pub fn before_delivery(&mut self, path: &Path, revision: u64) {
        if self.hold.is_none() {
            self.hold = self.holds.try_recv().ok();
        }
        if self
            .hold
            .as_ref()
            .is_some_and(|hold| hold.path == path && revision >= hold.revision)
        {
            let hold = self.hold.take().unwrap();
            let _span = foveonic::diagnostics::scope(
                foveonic::diagnostics::Category::Scenario,
                "scenario.delivery_pause",
            );
            let _ = hold.reached.try_send(());
            let _ = hold.release.recv();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delivery_hold_is_path_and_revision_specific_and_releases_on_drop() {
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let (handle, mut service) = channel(pending);
        let hold = handle.hold_delivery(PathBuf::from("photo"), 2);
        service.before_delivery(Path::new("other"), 2);
        service.before_delivery(Path::new("photo"), 1);
        assert!(hold.reached.is_empty());
        let worker = std::thread::spawn(move || service.before_delivery(Path::new("photo"), 2));
        hold.reached
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        drop(hold);
        worker.join().unwrap();
    }
}
