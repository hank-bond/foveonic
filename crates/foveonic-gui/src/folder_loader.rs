//! Discovers folder snapshots away from the UI and publishes only the newest request.

use crate::preferences::FolderMode;
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
};

pub struct Discovery {
    pub folder: PathBuf,
    pub files: Vec<PathBuf>,
    pub selected: Option<PathBuf>,
    pub canonical: Option<PathBuf>,
}

pub fn discover(path: &Path) -> std::io::Result<Discovery> {
    let info = std::fs::metadata(path)?;
    let (folder, selected) = if info.is_dir() {
        (path.to_owned(), None)
    } else if info.is_file() {
        (
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .to_owned(),
            Some(path.to_owned()),
        )
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a file or folder",
        ));
    };
    let files = crate::browser::folders::discover(&folder)?;
    let canonical = std::fs::canonicalize(&folder).ok();
    Ok(Discovery {
        folder,
        files,
        selected,
        canonical,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Fixed(FolderMode),
    Explicit {
        previous_folder: Option<PathBuf>,
        previous_mode: FolderMode,
    },
}
impl Mode {
    fn resolve(&self, canonical: Option<&Path>) -> FolderMode {
        match self {
            Self::Fixed(mode) => *mode,
            Self::Explicit {
                previous_folder,
                previous_mode,
            } => {
                if canonical.is_some_and(|folder| {
                    previous_folder
                        .as_ref()
                        .is_some_and(|root| folder.starts_with(root))
                }) {
                    *previous_mode
                } else {
                    FolderMode::Browse
                }
            }
        }
    }
}
#[derive(Clone)]
struct Request {
    generation: u64,
    path: PathBuf,
    mode: Mode,
}
pub struct Ready {
    pub path: PathBuf,
    pub result: std::io::Result<(Discovery, FolderMode)>,
}
#[derive(Default)]
struct State {
    current: u64,
    pending: Option<Request>,
    ready: Option<Ready>,
    stop: bool,
    loading: bool,
}
pub struct Loader {
    state: Arc<(Mutex<State>, Condvar)>,
}
impl Loader {
    pub fn new(context: egui::Context) -> Self {
        Self::with_reader(context, discover)
    }
    fn with_reader(
        context: egui::Context,
        read: impl Fn(&Path) -> std::io::Result<Discovery> + Send + 'static,
    ) -> Self {
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker = state.clone();
        thread::spawn(move || loop {
            let request = {
                let mut state = worker.0.lock().unwrap();
                while !state.stop && state.pending.is_none() {
                    state = worker.1.wait(state).unwrap();
                }
                if state.stop {
                    return;
                }
                state.pending.take().unwrap()
            };
            let result = read(&request.path).map(|discovery| {
                let mode = request.mode.resolve(discovery.canonical.as_deref());
                (discovery, mode)
            });
            let mut state = worker.0.lock().unwrap();
            if state.stop {
                return;
            }
            if request.generation == state.current {
                state.ready = Some(Ready {
                    path: request.path,
                    result,
                });
                context.request_repaint();
            }
        });
        Self { state }
    }
    pub fn request(&self, path: PathBuf, mode: Mode) {
        let mut state = self.state.0.lock().unwrap();
        state.current = state.current.wrapping_add(1);
        state.pending = Some(Request {
            generation: state.current,
            path,
            mode,
        });
        state.ready = None;
        state.loading = true;
        self.state.1.notify_one();
    }
    pub fn receive(&self) -> Option<Ready> {
        let mut state = self.state.0.lock().unwrap();
        let ready = state.ready.take();
        if ready.is_some() {
            state.loading = false;
        }
        ready
    }
    pub fn loading(&self) -> bool {
        self.state.0.lock().unwrap().loading
    }
}
impl Drop for Loader {
    fn drop(&mut self) {
        let mut state = self.state.0.lock().unwrap();
        state.stop = true;
        self.state.1.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    fn snapshot(path: &Path) -> Discovery {
        Discovery {
            folder: path.to_owned(),
            files: vec![path.join("image.x3f")],
            selected: None,
            canonical: Some(path.to_owned()),
        }
    }
    fn receive(loader: &Loader) -> Ready {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(ready) = loader.receive() {
                return ready;
            }
            assert!(Instant::now() < deadline, "folder loader did not reply");
            thread::yield_now();
        }
    }
    #[test]
    fn latest_request_replaces_pending_and_discards_running_result() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Mutex::new(release_rx);
        let loader = Loader::with_reader(egui::Context::default(), move |path| {
            if path == Path::new("old") {
                started_tx.send(()).unwrap();
                release_rx.lock().unwrap().recv().unwrap();
            }
            Ok(snapshot(path))
        });
        loader.request("old".into(), Mode::Fixed(FolderMode::Browse));
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        loader.request("middle".into(), Mode::Fixed(FolderMode::Browse));
        loader.request("new".into(), Mode::Fixed(FolderMode::Import));
        release_tx.send(()).unwrap();
        let ready = receive(&loader);
        assert_eq!(ready.path, Path::new("new"));
        let (discovery, mode) = ready.result.unwrap();
        assert_eq!(discovery.folder, Path::new("new"));
        assert_eq!(mode, FolderMode::Import);
        assert!(loader.receive().is_none());
    }
    #[test]
    fn failed_request_returns_error_without_publishing_a_snapshot() {
        let loader = Loader::with_reader(egui::Context::default(), |_| {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"))
        });
        loader.request("missing".into(), Mode::Fixed(FolderMode::Import));
        let ready = receive(&loader);
        assert_eq!(ready.path, Path::new("missing"));
        assert_eq!(
            ready.result.err().unwrap().kind(),
            std::io::ErrorKind::NotFound
        );
    }
    #[test]
    fn explicit_file_uses_containing_folder_and_inherits_saved_mode() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("card");
        std::fs::create_dir(&nested).unwrap();
        let file = nested.join("photo.x3f");
        std::fs::write(&file, []).unwrap();
        let discovery = discover(&file).unwrap();
        let mode = Mode::Explicit {
            previous_folder: Some(std::fs::canonicalize(dir.path()).unwrap()),
            previous_mode: FolderMode::Import,
        };
        assert_eq!(discovery.folder, nested);
        assert_eq!(discovery.selected.as_deref(), Some(file.as_path()));
        assert_eq!(
            mode.resolve(discovery.canonical.as_deref()),
            FolderMode::Import
        );
        assert_eq!(
            mode.resolve(Some(Path::new("/unrelated"))),
            FolderMode::Browse
        );
    }
    #[test]
    fn applying_failed_discovery_preserves_open_browser_until_success() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("old");
        let new = root.path().join("new");
        std::fs::create_dir(&old).unwrap();
        std::fs::create_dir(&new).unwrap();
        let old_file = old.join("old.x3f");
        let new_file = new.join("new.x3f");
        std::fs::write(&old_file, []).unwrap();
        std::fs::write(&new_file, []).unwrap();
        let mut browser = crate::browser::Browser::new(egui::Context::default());
        browser.open(&old_file).unwrap();
        browser.import_mode = true;
        browser
            .marks
            .insert(old_file.clone(), crate::browser::Mark::Pick);
        browser.refresh_filter();
        let fail = Ready {
            path: root.path().join("missing"),
            result: Err(std::io::Error::new(std::io::ErrorKind::NotFound, "missing")),
        };
        assert!(browser.finish_open(fail).is_err());
        assert_eq!(browser.root.as_deref(), Some(old.as_path()));
        assert_eq!(browser.files, vec![old_file.clone()]);
        assert!(browser.import_mode);
        assert_eq!(browser.mark(&old_file), crate::browser::Mark::Pick);
        assert_eq!(browser.review_counts(), (1, 0));
        let success = Ready {
            path: new_file.clone(),
            result: Ok((discover(&new_file).unwrap(), FolderMode::Browse)),
        };
        let (selected, mode, _) = browser.finish_open(success).unwrap();
        assert_eq!(selected, Some(new_file));
        assert_eq!(mode, FolderMode::Browse);
        assert_eq!(browser.root.as_deref(), Some(new.as_path()));
        assert!(browser.marks.is_empty());
    }
}
