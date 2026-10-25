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
const ERROR_PANEL_HEIGHT: f32 = 120.;

pub struct Review {
    scan_destination: Option<PathBuf>,
    generation: u64,
    scanning: bool,
    sources: HashMap<PathBuf, batch::SourceInfo>,
    browse_selection: Vec<PathBuf>,
    errors: Vec<String>,
    worker: batch::Worker,
    received_imports: usize,
}
impl Review {
    pub fn new(engine: Engine, cache: Arc<Cache>, context: egui::Context) -> Self {
        Self {
            scan_destination: None,
            generation: 0,
            scanning: false,
            sources: HashMap::new(),
            browse_selection: Vec::new(),
            errors: Vec::new(),
            worker: batch::Worker::new(engine, cache, context),
            received_imports: 0,
        }
    }
    pub fn rescan(&mut self, browser: &mut Browser, destination: Option<&std::path::Path>) {
        self.scan_destination = destination.map(std::path::Path::to_owned);
        self.generation += 1;
        self.scanning = browser.import_mode;
        self.sources.clear();
        self.browse_selection.clear();
        self.errors.clear();
        browser.recorded_marks.clear();
        if browser.import_mode {
            self.worker.scan(batch::Scan {
                generation: self.generation,
                destination: self.scan_destination.clone(),
                files: browser.files.clone(),
            });
        }
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
                generation: self.generation,
                destination: None,
                files,
            });
        }
    }
    pub fn update(&mut self, browser: &mut Browser) {
        let mut replay_from = self.received_imports;
        if let Some(scanned) = self.worker.receive_scan() {
            if scanned.generation == self.generation {
                self.scanning = false;
                self.sources.extend(scanned.sources);
                // A completion already received by the UI may postdate this scan.
                // Replay its recorded status without undoing later checkbox changes.
                replay_from = scanned.completed_imports;
                self.errors = scanned.errors;
                if browser.import_mode {
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
        apply_imports(
            browser,
            &progress,
            self.scan_destination.as_deref(),
            replay_from,
            self.received_imports,
        );
        self.received_imports = progress.imported.len();
        browser.refresh_filter();
    }
    pub fn ready(&self, browser: &Browser) -> bool {
        !self.worker.progress().running
            && !self.scanning
            && self.scan_destination.is_some()
            && if browser.import_mode {
                browser.review_counts().0 > 0 || !browser.marks.is_empty()
            } else {
                !self.browse_selection.is_empty()
            }
    }
    pub fn show_status(&self, ui: &mut egui::Ui) {
        let progress = self.worker.progress();
        if self.scanning && self.browse_selection.is_empty() {
            ui.weak("Checking import history…");
        }
        // Batch status remains visible when the owner returns to ordinary browsing.
        if progress.running || progress.total > 0 || !progress.errors.is_empty() {
            ui.horizontal(|ui| {
                let label = if progress.running {
                    "Processing"
                } else {
                    "Processing finished"
                };
                ui.label(format!(
                    "{label} · {}/{} photos · {} errors",
                    progress.completed,
                    progress.total,
                    progress.errors.len()
                ));
                if progress.running {
                    ui.ctx().request_repaint_after(STATUS_POLL);
                }
            });
        }
        let errors: Vec<_> = self.errors.iter().chain(&progress.errors).collect();
        if !errors.is_empty() {
            ui.collapsing(format!("Processing errors ({})", errors.len()), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(ERROR_PANEL_HEIGHT)
                    .show(ui, |ui| {
                        for error in errors {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                    });
            });
        }
    }
    pub fn submit(
        &mut self,
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
            let Some(source) = self.sources.get(path) else {
                if !browser.import_mode || mark == Mark::Pick || browser.marks.contains_key(path) {
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
                        settings: settings.clone(),
                    }),
                    Err(error) => self.errors.push(format!("{}: {error}", path.display())),
                }
            } else if let Some(mark) = browser.marks.get(path) {
                decisions.push(batch::Decision {
                    path: path.clone(),
                    identity: source.identity.clone(),
                    hidden: *mark == Mark::Hidden,
                });
            }
        }
        if self.worker.submit(batch::Batch {
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
                conflict: settings.conflict,
            },
        }) {
            self.received_imports = 0;
        }
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

/// History replacement and completion receipt have different cursors. Replaying
/// an old completion restores recorded status, while only new receipts uncheck picks.
fn apply_imports(
    browser: &mut Browser,
    progress: &batch::Progress,
    destination: Option<&std::path::Path>,
    replay_from: usize,
    received: usize,
) {
    if !browser.import_mode || progress.destination.as_deref() != destination {
        return;
    }
    for path in &progress.imported[replay_from.min(progress.imported.len())..] {
        browser.recorded_marks.insert(path.clone(), Mark::Imported);
    }
    for path in &progress.imported[received.min(progress.imported.len())..] {
        if browser.marks.get(path) == Some(&Mark::Pick) {
            browser.marks.remove(path);
        }
    }
}

#[cfg(test)]
#[path = "import_review_test.rs"]
mod tests;
