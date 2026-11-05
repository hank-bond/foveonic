//! Prepares frozen processing selections and tracks destination-scoped import review and batch status.

use crate::{
    batch,
    browser::{Browser, Mark},
    processing, sidecars,
};
use eframe::egui;
use foveonic::{decode_cache::Cache, importing::Disposition, render::Engine};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

const STATUS_POLL: std::time::Duration = std::time::Duration::from_millis(150);
const DESTINATION_POLL: std::time::Duration = std::time::Duration::from_secs(2);

pub struct Review {
    calibration: Option<Arc<foveonic::noise_calibration::Calibration>>,
    defaults: foveonic::render::RecipeDefaults,
    scan_destination: Option<PathBuf>,
    generation: u64,
    scanning: bool,
    sources: HashMap<PathBuf, batch::SourceInfo>,
    browse_selection: Vec<PathBuf>,
    errors: Vec<String>,
    worker: batch::Worker,
    received_imports: usize,
    submitted_decisions: Vec<(PathBuf, bool)>,
    destination_check: Option<(PathBuf, std::time::Instant)>,
    destination_unavailable: bool,
    events: Vec<crate::activity::Event>,
    batch_report: bool,
    received_errors: usize,
    received_operations: usize,
}
impl Review {
    pub fn new(engine: Engine, cache: Arc<Cache>, context: egui::Context) -> Self {
        Self {
            calibration: None,
            defaults: Default::default(),
            scan_destination: None,
            generation: 0,
            scanning: false,
            sources: HashMap::new(),
            browse_selection: Vec::new(),
            errors: Vec::new(),
            worker: batch::Worker::new(engine, cache, context),
            received_imports: 0,
            submitted_decisions: Vec::new(),
            destination_check: None,
            destination_unavailable: false,
            events: Vec::new(),
            batch_report: false,
            received_errors: 0,
            received_operations: 0,
        }
    }
    pub fn set_noise_calibration(
        &mut self,
        calibration: Option<Arc<foveonic::noise_calibration::Calibration>>,
    ) {
        self.calibration = calibration;
    }
    pub fn rescan(&mut self, browser: &mut Browser, destination: Option<&std::path::Path>) {
        self.scan_destination = destination.map(std::path::Path::to_owned);
        self.generation += 1;
        self.scanning = browser.import_mode;
        self.sources.clear();
        self.browse_selection.clear();
        self.errors.clear();
        browser.recorded_marks.clear();
        browser.refresh_filter();
        if browser.import_mode {
            self.worker.scan(batch::Scan {
                defaults: self.defaults.clone(),
                generation: self.generation,
                destination: self.scan_destination.clone(),
                files: browser.files.clone(),
            });
        }
    }

    pub fn set_defaults(
        &mut self,
        defaults: &foveonic::render::RecipeDefaults,
        browser: &mut Browser,
        destination: Option<&std::path::Path>,
    ) {
        if &self.defaults == defaults {
            return;
        }
        self.defaults = defaults.clone();
        // A default recipe changes future processing snapshots, not import history.
        let recorded = browser.recorded_marks.clone();
        self.rescan(browser, destination);
        browser.recorded_marks = recorded;
        browser.refresh_filter();
    }
    /// Prepare only the highlighted Browse photos, away from the UI thread.
    /// Recipes are ready before Process is enabled so later edits cannot alter a submitted job.
    pub fn prepare_browse(&mut self, browser: &Browser, active: Option<&std::path::Path>) {
        if browser.import_mode {
            return;
        }
        let paths = browser.processing_selection(active);
        if paths == self.browse_selection {
            return;
        }
        self.browse_selection = paths;
        self.generation += 1;
        let files: Vec<_> = self
            .browse_selection
            .iter()
            .filter(|path| !self.sources.contains_key(*path))
            .cloned()
            .collect();
        self.scanning = !files.is_empty();
        self.errors.clear();
        if self.scanning {
            self.worker.scan(batch::Scan {
                defaults: self.defaults.clone(),
                generation: self.generation,
                destination: None,
                files,
            });
        }
    }
    pub fn update(&mut self, browser: &mut Browser) {
        let mut replay_from = self.received_imports;
        let mut filter_changed = false;
        if let Some(scanned) = self.worker.receive_scan() {
            if scanned.generation == self.generation {
                self.scanning = false;
                self.sources.extend(scanned.sources);
                // A completion already received by the UI may postdate this scan.
                // Replay its recorded status without undoing later checkbox changes.
                replay_from = scanned.completed_imports;
                self.errors = scanned.errors;
                if browser.import_mode {
                    filter_changed = true;
                    browser.recorded_marks = scanned
                        .dispositions
                        .into_iter()
                        .map(|(path, disposition)| {
                            (
                                path,
                                if matches!(disposition, Disposition::Hidden) {
                                    Mark::Hidden
                                } else {
                                    Mark::Imported
                                },
                            )
                        })
                        .collect();
                }
            }
        }
        let progress = self.worker.progress();
        if progress.decisions_saved {
            let decisions = std::mem::take(&mut self.submitted_decisions);
            if browser.import_mode && progress.destination == self.scan_destination {
                filter_changed |= acknowledge_decisions(browser, decisions);
            }
        }
        self.receive_events(&progress);
        if self.batch_report && !progress.running {
            self.events.push(crate::activity::Event::Operation(format!(
                "Processing finished: {} of {} photos, {} errors. Destination: {}",
                progress.completed,
                progress.total,
                progress.errors.len(),
                progress
                    .destination
                    .as_ref()
                    .map_or_else(String::new, |p| p.display().to_string())
            )));
            self.batch_report = false;
        }
        filter_changed |= apply_imports(
            browser,
            &progress,
            self.scan_destination.as_deref(),
            replay_from,
            self.received_imports,
        );
        self.received_imports = progress.imported.len();
        if filter_changed {
            browser.refresh_filter();
        }
    }
    pub fn ready(&self, browser: &Browser) -> bool {
        self.process_blocker(browser).is_none()
    }
    pub fn process_blocker(&self, browser: &Browser) -> Option<&'static str> {
        if self.worker.summary().running {
            Some("Wait for the current processing batch to finish.")
        } else if self.scan_destination.is_none() {
            Some("Choose a destination folder first.")
        } else if self.scanning {
            Some("Preparing photo settings and checking import history…")
        } else if browser.import_mode {
            (!has_import_work(browser)).then_some(
                "Pick photos for import using their checkboxes or Space. Highlighting a photo only selects it for viewing.",
            )
        } else {
            self.browse_selection
                .is_empty()
                .then_some("Select a photo to process.")
        }
    }
    /// Directory probes share the background worker. Drawing never touches a removable drive.
    pub fn destination_unavailable(
        &mut self,
        ctx: &egui::Context,
        path: Option<&std::path::Path>,
    ) -> bool {
        let Some(path) = path else {
            self.destination_check = None;
            self.destination_unavailable = false;
            return false;
        };
        let changed = self
            .destination_check
            .as_ref()
            .is_none_or(|(old, _)| old != path);
        if changed {
            self.destination_unavailable = false;
        }
        if let Some((checked, available)) = self.worker.receive_directory() {
            if checked == path {
                self.destination_unavailable = !available;
            }
        }
        if changed
            || self
                .destination_check
                .as_ref()
                .is_some_and(|(_, at)| at.elapsed() >= DESTINATION_POLL)
        {
            self.worker.check_directory(path.to_owned());
            self.destination_check = Some((path.to_owned(), std::time::Instant::now()));
        }
        ctx.request_repaint_after(DESTINATION_POLL);
        self.destination_unavailable
    }
    pub fn status(
        &mut self,
        context: &egui::Context,
        log: &mut crate::activity::Log,
    ) -> Option<String> {
        let progress = self.worker.summary();
        for event in self.events.drain(..) {
            log.record(event);
        }
        log.observe(
            crate::activity::Origin::Processing,
            self.errors.iter().cloned().chain(
                self.destination_check
                    .as_ref()
                    .filter(|_| self.destination_unavailable)
                    .map(|(path, _)| format!("Destination folder unavailable: {}", path.display())),
            ),
        );
        if progress.running {
            context.request_repaint_after(STATUS_POLL);
            return Some(format!(
                "Processing {} / {} photos",
                progress.completed, progress.total
            ));
        }
        if self.scanning && self.browse_selection.is_empty() {
            return Some("Checking import history…".into());
        }
        None
    }
    pub fn submit(
        &mut self,
        limits: foveonic::render::AdaptiveToneTuning,
        browser: &Browser,
        sidecars: &sidecars::Session,
        settings: &processing::Settings,
    ) {
        if !self.ready(browser) {
            return;
        }
        let Some(destination) = self.scan_destination.clone() else {
            return;
        };
        let mut photos = Vec::new();
        let mut decisions = Vec::new();
        self.errors.clear();
        let paths = if browser.import_mode {
            &browser.files
        } else {
            &self.browse_selection
        };
        for path in paths {
            let mark = browser.mark(path);
            let hidden = hidden_change(browser, path, mark);
            let Some(source) = self.sources.get(path) else {
                if !browser.import_mode || mark == Mark::Pick || hidden.is_some() {
                    self.errors.push(format!(
                        "{}: source metadata is unavailable",
                        path.display()
                    ));
                }
                continue;
            };
            if !browser.import_mode || mark == Mark::Pick {
                let edits = sidecars
                    .requested_settings(path)
                    .map(Ok)
                    .unwrap_or(source.settings.as_ref());
                match edits {
                    Ok(settings) => photos.push(batch::Photo {
                        path: path.clone(),
                        identity: source.identity.clone(),
                        settings: {
                            let mut frozen = settings.clone();
                            frozen.adaptive_tone_tuning = limits;
                            frozen
                        },
                        rating: browser.changed_rating(path).unwrap_or(source.rating),
                    }),
                    Err(error) => self.errors.push(format!("{}: {error}", path.display())),
                }
            } else if let Some(hidden) = hidden {
                decisions.push(batch::Decision {
                    path: path.clone(),
                    identity: source.identity.clone(),
                    hidden,
                });
            }
        }
        self.receive_events(&self.worker.progress());
        let submitted_decisions = decisions
            .iter()
            .map(|d| (d.path.clone(), d.hidden))
            .collect();
        if self.worker.submit(batch::Batch {
            calibration: self.calibration.clone(),
            destination,
            photos,
            operation: if browser.import_mode {
                batch::Operation::Import { decisions }
            } else {
                batch::Operation::Browse
            },
            output: batch::Output {
                formats: if browser.import_mode {
                    settings.formats()
                } else {
                    vec![settings.format]
                },
                tiff_color_space: settings.tiff_color_space,
                encoding: settings.encoding,
                sections: settings.sections(),
                conflict: settings.conflict,
            },
        }) {
            self.submitted_decisions = submitted_decisions;
            let progress = self.worker.progress();
            self.events.push(crate::activity::Event::Operation(format!(
                "Started {} {} photos. Destination: {}",
                if browser.import_mode {
                    "importing"
                } else {
                    "processing"
                },
                progress.total,
                progress
                    .destination
                    .as_ref()
                    .map_or_else(String::new, |p| p.display().to_string())
            )));
            self.batch_report = true;
            self.received_errors = 0;
            self.received_operations = 0;
            self.received_imports = 0;
        }
    }

    fn receive_events(&mut self, progress: &batch::Progress) {
        self.events.extend(
            progress
                .operations
                .iter()
                .skip(self.received_operations)
                .cloned()
                .map(crate::activity::Event::Operation),
        );
        self.received_operations = progress.operations.len();
        self.events.extend(
            progress
                .errors
                .iter()
                .skip(self.received_errors)
                .cloned()
                .map(crate::activity::Event::Error),
        );
        self.received_errors = progress.errors.len();
    }

    /// Used only by the opt-in native smoke sequence with an isolated destination.
    pub fn smoke_prepare(&mut self, destination: &std::path::Path, browser: &mut Browser) {
        assert!(browser.import_mode);
        browser.hide_reviewed = false;
        if let Some(path) = browser.files.first() {
            browser.marks.insert(path.clone(), Mark::Pick);
        }
        if let Some(path) = browser.files.get(1) {
            browser.marks.insert(path.clone(), Mark::Hidden);
        }
        self.rescan(browser, Some(destination));
    }
    pub fn smoke_ready(&self, copied: bool) -> bool {
        if self.scanning {
            return false;
        }
        if !copied {
            return true;
        }
        let progress = self.worker.progress();
        !progress.running && progress.imported.len() == 1 && progress.errors.is_empty()
    }
}

fn has_import_work(browser: &Browser) -> bool {
    browser.review_counts().0 > 0
        || browser
            .marks
            .iter()
            .any(|(path, &mark)| hidden_change(browser, path, mark).is_some())
}

fn acknowledge_decisions(browser: &mut Browser, decisions: Vec<(PathBuf, bool)>) -> bool {
    let mut changed = false;
    for (path, hidden) in decisions {
        if !browser.files.contains(&path) {
            continue;
        }
        let mark = if hidden { Mark::Hidden } else { Mark::Unmarked };
        if browser.recorded_marks.get(&path) != Some(&Mark::Imported) {
            if hidden {
                browser.recorded_marks.insert(path.clone(), mark);
            } else {
                browser.recorded_marks.remove(&path);
            }
        }
        // A later checkbox change belongs to the next batch.
        if browser.marks.get(&path) == Some(&mark) {
            browser.marks.remove(&path);
        }
        changed = true;
    }
    changed
}

/// Picks and their reversals are temporary. Only changed hidden decisions need saving.
fn hidden_change(browser: &Browser, path: &std::path::Path, mark: Mark) -> Option<bool> {
    let recorded = browser
        .recorded_marks
        .get(path)
        .copied()
        .unwrap_or_default();
    // Hiding an imported photo must retain the link to its copied original.
    if recorded == Mark::Imported || mark == Mark::Pick {
        return None;
    }
    let hidden = mark == Mark::Hidden;
    (hidden != (recorded == Mark::Hidden)).then_some(hidden)
}

/// History replacement and completion receipt have different cursors. Replaying
/// an old completion restores recorded status, while only new receipts uncheck picks.
fn apply_imports(
    browser: &mut Browser,
    progress: &batch::Progress,
    destination: Option<&std::path::Path>,
    replay_from: usize,
    received: usize,
) -> bool {
    if !browser.import_mode || progress.destination.as_deref() != destination {
        return false;
    }
    let mut changed = false;
    for path in &progress.imported[replay_from.min(progress.imported.len())..] {
        changed |=
            browser.recorded_marks.insert(path.clone(), Mark::Imported) != Some(Mark::Imported);
    }
    for path in &progress.imported[received.min(progress.imported.len())..] {
        if browser.marks.get(path) == Some(&Mark::Pick) {
            browser.marks.remove(path);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
#[path = "import_review_test.rs"]
mod tests;
