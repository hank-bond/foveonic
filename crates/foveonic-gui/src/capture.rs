//! Shows capture details without waiting for rendering or publishing an older selection.

use eframe::egui;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
};

type Properties = BTreeMap<String, String>;
type Result = std::result::Result<Properties, String>;

#[derive(Clone, PartialEq, Eq)]
struct Request {
    generation: u64,
    path: PathBuf,
}
#[derive(Default)]
struct State {
    selected: Option<Request>,
    pending: Option<Request>,
    ready: Option<Result>,
    stop: bool,
}

pub struct Loader {
    state: Arc<(Mutex<State>, Condvar)>,
    generation: u64,
    pub properties: Option<Properties>,
}
impl Loader {
    pub fn new(context: egui::Context) -> Self {
        Self::with_reader(context, |path| {
            foveonic::source::read_capture_properties(path).map_err(|error| error.to_string())
        })
    }

    fn with_reader(
        context: egui::Context,
        read: impl Fn(&Path) -> Result + Send + 'static,
    ) -> Self {
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let slot = state.clone();
        thread::spawn(move || loop {
            let request = {
                let (lock, wake) = &*slot;
                let mut state = lock.lock().unwrap();
                while !state.stop && state.pending.is_none() {
                    state = wake.wait(state).unwrap();
                }
                if state.stop {
                    break;
                }
                state.pending.take().unwrap()
            };
            let result = read(&request.path);
            let mut state = slot.0.lock().unwrap();
            if state.stop {
                break;
            }
            // Returning to the same path is still a new selection. A path check
            // alone would let an earlier load publish into that newer selection.
            if state.selected.as_ref() == Some(&request) {
                state.ready = Some(result);
                context.request_repaint();
            }
        });
        Self {
            state,
            generation: 0,
            properties: None,
        }
    }

    pub fn select(&mut self, path: Option<PathBuf>) {
        self.generation += 1;
        self.properties = None;
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap();
        state.selected = path.map(|path| Request {
            generation: self.generation,
            path,
        });
        state.pending = state.selected.clone();
        state.ready = None;
        wake.notify_one();
    }

    pub fn receive(&mut self) {
        if let Some(result) = self.state.0.lock().unwrap().ready.take() {
            // Capture details are optional. Their read error must not hide the
            // render worker's loading message or processing error.
            self.properties = result.ok();
        }
    }
}
impl Drop for Loader {
    fn drop(&mut self) {
        let (lock, wake) = &*self.state;
        lock.lock().unwrap().stop = true;
        wake.notify_one();
    }
}

#[cfg(test)]
#[path = "capture_test.rs"]
mod tests;
