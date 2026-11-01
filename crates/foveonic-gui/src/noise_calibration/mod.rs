//! Coordinates cap review and one user calibration through explicit background jobs and immutable profile selection.

use eframe::egui;
use foveonic::{
    noise_calibration::{self as calibration, Calibration, Capture, Condition, TrainingProgress},
    render::Engine,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
};

mod captures;
mod view;
use captures::{Availability, Dataset};
type Properties = BTreeMap<String, String>;
const PROFILE_FILE: &str = "noise-calibration.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Slot {
    #[default]
    Provided,
    User,
}
impl Slot {
    fn label(self) -> &'static str {
        match self {
            Self::Provided => "Provided",
            Self::User => "User",
        }
    }
}

enum Event {
    Capture(Capture, Properties),
    ImportError(String),
    Imported,
    Saved(Result<(), String>),
    Progress(TrainingProgress),
    Trained(Result<Arc<Calibration>, String>),
}
enum Job {
    Import(mpsc::Receiver<Event>),
    Save(mpsc::Receiver<Event>),
    Train {
        receiver: mpsc::Receiver<Event>,
        cancel: Arc<AtomicBool>,
    },
}
impl Job {
    fn receiver(&self) -> &mpsc::Receiver<Event> {
        match self {
            Self::Import(rx) | Self::Save(rx) => rx,
            Self::Train { receiver, .. } => receiver,
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        if let Self::Train { cancel, .. } = self {
            cancel.store(true, Ordering::Release);
        }
    }
}

pub struct State {
    pub open: bool,
    engine: Engine,
    cache: Arc<foveonic::decode_cache::Cache>,
    context: egui::Context,
    directory: Option<PathBuf>,
    dataset: Dataset,
    availability: Availability,
    user: Option<Arc<Calibration>>,
    job: Option<Job>,
    trained: bool,
    completed: BTreeSet<Condition>,
    progress: Option<TrainingProgress>,
    error: Option<String>,
    changed: bool,
}
impl State {
    pub fn new(
        engine: Engine,
        cache: Arc<foveonic::decode_cache::Cache>,
        context: egui::Context,
        slot: &mut Slot,
    ) -> Self {
        let directory = eframe::storage_dir(crate::preferences::APP_ID);
        let mut error = None;
        let user = directory.as_ref().and_then(|d| {
            let path = d.join(PROFILE_FILE);
            match Calibration::load(&path) {
                Ok(p) => Some(Arc::new(p)),
                Err(e)
                    if e.downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
                {
                    None
                }
                Err(e) => {
                    error = Some(format!("Cannot load User calibration: {e}"));
                    None
                }
            }
        });
        if *slot == Slot::User && user.is_none() {
            *slot = Slot::Provided;
        }
        let (dataset, dataset_error) = Dataset::load(
            directory.as_deref(),
            user.as_ref().map_or(&[], |p| p.captures()),
        );
        if let Some(e) = dataset_error {
            error = Some(error.map_or_else(|| e.clone(), |old| format!("{old}\n{e}")));
        }
        let trained = user
            .as_ref()
            .is_some_and(|p| p.captures() == dataset.captures);
        let mut state = Self {
            open: false,
            engine,
            cache,
            context,
            directory,
            dataset,
            availability: Availability::default(),
            user,
            job: None,
            trained,
            completed: BTreeSet::new(),
            progress: None,
            error,
            changed: false,
        };
        let missing = state.dataset.missing_metadata();
        state.read_metadata(missing);
        state
    }
    pub fn selected(&self, slot: Slot) -> Option<Arc<Calibration>> {
        if slot == Slot::User {
            self.user.clone()
        } else {
            None
        }
    }
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
    fn is_importing(&self) -> bool {
        matches!(self.job, Some(Job::Import(_)))
    }
    fn is_training(&self) -> bool {
        matches!(self.job, Some(Job::Train { .. }))
    }
    fn cancel_training(&self) {
        if let Some(Job::Train { cancel, .. }) = &self.job {
            cancel.store(true, Ordering::Release);
        }
    }
    fn dataset_changed(&mut self) {
        self.trained = false;
        self.completed.clear();
        self.availability.invalidate();
    }
    fn can_train(&self) -> bool {
        self.job.is_none()
            && self.availability.ready()
            && !self.dataset.captures.is_empty()
            && calibration::groups(&self.dataset.captures)
                .values()
                .all(|paths| {
                    paths.len() >= calibration::MINIMUM_CAPTURES
                        && paths.iter().all(|p| !self.availability.missing.contains(p))
                })
    }
    fn save_dataset(&mut self) {
        let Some(directory) = self.directory.clone() else {
            self.error = Some("Application storage is unavailable".into());
            return;
        };
        let dataset = self.dataset.clone();
        let cache = self.cache.clone();
        let ctx = self.context.clone();
        let (tx, rx) = mpsc::channel();
        self.job = Some(Job::Save(rx));
        std::thread::spawn(move || {
            let result = cache
                .background(|| dataset.save(&directory))
                .map_err(|e| format!("Cannot save cap list: {e}"));
            let _ = tx.send(Event::Saved(result));
            ctx.request_repaint();
        });
    }
    fn import(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() || self.job.is_some() {
            return;
        }
        self.error = None;
        self.read_metadata(paths);
    }
    fn read_metadata(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() || self.job.is_some() {
            return;
        }
        let cache = self.cache.clone();
        let ctx = self.context.clone();
        let (tx, rx) = mpsc::channel();
        self.job = Some(Job::Import(rx));
        std::thread::spawn(move || {
            cache.background(|| {
                captures::visit(paths, &cache, |result| {
                    let event = match result {
                        Ok((capture, properties)) => Event::Capture(capture, properties),
                        Err(e) => Event::ImportError(e),
                    };
                    let sent = tx.send(event).is_ok();
                    ctx.request_repaint();
                    sent
                })
            });
            let _ = tx.send(Event::Imported);
            ctx.request_repaint();
        });
    }
    fn train(&mut self) {
        if !self.can_train() {
            return;
        }
        let Some(directory) = self.directory.clone() else {
            self.error = Some("Application storage is unavailable".into());
            return;
        };
        self.trained = false;
        self.completed.clear();
        self.progress = None;
        self.error = None;
        let engine = self.engine.executor();
        let captures = self.dataset.captures.clone();
        let cache = self.cache.clone();
        let ctx = self.context.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancellation = cancel.clone();
        let (tx, rx) = mpsc::channel();
        self.job = Some(Job::Train {
            receiver: rx,
            cancel,
        });
        std::thread::spawn(move || {
            let result = cache
                .processing(|| {
                    let profile = engine.train_noise_calibration(
                        &captures,
                        |p| {
                            let _ = tx.send(Event::Progress(p));
                            ctx.request_repaint();
                        },
                        || cancellation.load(Ordering::Acquire),
                    )?;
                    if cancellation.load(Ordering::Acquire) {
                        return Err("Calibration training cancelled".into());
                    }
                    profile.save(directory.join(PROFILE_FILE))?;
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(Arc::new(profile))
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(Event::Trained(result));
            ctx.request_repaint();
        });
    }
    pub fn poll(&mut self, slot: &mut Slot) {
        let mut events = Vec::new();
        const MAX_EVENTS_PER_FRAME: usize = 32;
        let disconnected = self.job.as_ref().is_some_and(|job| loop {
            if events.len() == MAX_EVENTS_PER_FRAME {
                self.context.request_repaint();
                break false;
            }
            match job.receiver().try_recv() {
                Ok(event) => events.push(event),
                Err(mpsc::TryRecvError::Empty) => break false,
                Err(mpsc::TryRecvError::Disconnected) => break true,
            }
        });
        for event in events {
            match event {
                Event::Capture(capture, properties) => {
                    if self.dataset.insert(capture, properties) {
                        self.dataset_changed();
                    }
                }
                Event::ImportError(e) => self.error = Some(e),
                Event::Imported => {
                    self.job = None;
                    self.availability.invalidate();
                    self.save_dataset();
                    return;
                }
                Event::Saved(result) => {
                    self.job = None;
                    if let Err(e) = result {
                        self.error = Some(e);
                    }
                    return;
                }
                Event::Progress(p) => {
                    if p.completed == p.total {
                        self.completed.insert(p.condition.clone());
                    }
                    self.progress = Some(p);
                }
                Event::Trained(result) => {
                    self.job = None;
                    self.progress = None;
                    match result {
                        Ok(profile) => {
                            self.user = Some(profile);
                            self.trained = true;
                            *slot = Slot::User;
                            self.changed = true;
                        }
                        Err(e) => {
                            self.error = Some(e);
                            self.completed.clear();
                        }
                    }
                    return;
                }
            }
        }
        if disconnected {
            self.job = None;
            self.completed.clear();
            self.progress = None;
            self.error = Some("Calibration worker stopped before completing the request".into());
        }
    }
}
