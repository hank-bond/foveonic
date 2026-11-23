//! Exercises recipe persistence and import review in the running application.

use super::scenarios::number_edit;
use super::{App, Smoke};
use crate::multi_edit_controls::NumberChange;
use eframe::egui;
use foveonic::render::Settings;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
enum Phase {
    OpenBrowse,
    Browse,
    Edit,
    SwitchAway,
    SwitchBack,
    Saved,
    ReopenImport,
    Import,
    ImportAway,
    ImportBack,
    NoDestination,
    CopyAndDefault,
    CameraReset,
    DefaultReset,
    Paste,
    PasteHighlighted,
    MenuActions,
    MenuApplied,
    InvalidPaste,
    AcknowledgePaste,
    ImportDisk,
    PrepareOutput,
    OutputScan,
    Unpicked,
    Repicked,
    ProcessClicked,
    OutputWait,
    OutputVerify,
    Damaged,
    MissingOpen,
    AcknowledgeOpen,
    ReopenBrowse,
    Restored,
    SelectAll,
    Done,
}

pub(super) struct LifecycleJourney {
    phase: Phase,
    initial_exposure: f32,
    saved: Option<Settings>,
    review: Option<Settings>,
    clipboard: Option<String>,
    /// The photo's settings before the menu actions.
    expected_menu: Option<Settings>,
    read: Option<mpsc::Receiver<Result<Option<Settings>, String>>>,
    io: Option<mpsc::Receiver<Result<Value, String>>>,
    last_read: Option<Instant>,
    checks: Vec<Value>,
    damaged: super::damaged::DamagedRecovery,
}
impl LifecycleJourney {
    pub(super) fn expects_failure(&self, path: &std::path::Path) -> bool {
        matches!(self.phase, Phase::Damaged) && self.damaged.expects_failure(path)
    }
    pub(super) fn new(initial_exposure: f32) -> Self {
        Self {
            phase: Phase::OpenBrowse,
            initial_exposure,
            saved: None,
            review: None,
            clipboard: None,
            expected_menu: None,
            read: None,
            io: None,
            last_read: None,
            checks: Vec::new(),
            damaged: super::damaged::DamagedRecovery::new(),
        }
    }
    fn check(&mut self, name: &str, passed: bool, detail: Value) -> Result<(), String> {
        self.checks
            .push(json!({"check":name,"passed":passed,"detail":detail}));
        if passed {
            Ok(())
        } else {
            Err(format!("lifecycle check failed: {name}"))
        }
    }
    fn ready(app: &App, path: &PathBuf) -> bool {
        !app.folder_loader.loading()
            && app.selection_loaded()
            && !app.view_busy()
            && app.selected.as_ref() == Some(path)
    }
    fn matches(a: &Settings, b: &Settings) -> bool {
        crate::rendering::matching::same_authored_edits(a, b)
    }
    // Disk inspection belongs to a worker, including retries while a queued save completes.
    fn read_disk(&mut self, path: PathBuf) -> Option<Result<Option<Settings>, String>> {
        if let Some(rx) = &self.read {
            match rx.try_recv() {
                Ok(value) => {
                    self.read = None;
                    return Some(value);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.read = None;
                    return Some(Err("recipe inspection worker stopped".into()));
                }
                Err(mpsc::TryRecvError::Empty) => return None,
            }
        }
        if self
            .last_read
            .is_some_and(|t| t.elapsed() < Duration::from_millis(50))
        {
            return None;
        }
        self.last_read = Some(Instant::now());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(
                foveonic::sidecar::load(&path)
                    .map(foveonic::sidecar::Saved::settings)
                    .map_err(|e| e.to_string()),
            );
        });
        self.read = Some(rx);
        None
    }
    pub(super) fn advance(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        smoke: &mut Smoke,
        paths: &[PathBuf],
        trial: &std::path::Path,
        expected: &mut std::collections::BTreeMap<PathBuf, Settings>,
    ) -> Result<bool, String> {
        let directory = paths[0]
            .parent()
            .ok_or("missing lifecycle input directory")?;
        let destination = trial.join("lifecycle-output");
        let destination = destination.as_path();
        let first = &paths[0];
        match self.phase {
            Phase::OpenBrowse => {
                app.open(directory.to_owned(), crate::preferences::FolderMode::Browse);
                self.phase = Phase::Browse;
            }
            Phase::Browse => {
                if app.folder_loader.loading()
                    || app.browser.import_mode
                    || !app.selection_loaded()
                    || app.view_busy()
                {
                    return Ok(false);
                }
                app.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::Arrow(egui::Key::ArrowLeft),
                    egui::Modifiers::NONE,
                )]);
                self.phase = Phase::Edit;
            }
            Phase::Edit => {
                if !smoke.direct_events.is_empty()
                    || !Self::ready(app, first)
                    || app.edit_selection.paths() != [first.clone()]
                {
                    return Ok(false);
                }
                let baseline = expected
                    .get(first)
                    .ok_or("missing declared starting settings")?
                    .clone();
                self.check("browse-start", Self::matches(app.smoke_recipe(), &baseline) && baseline.exposure == self.initial_exposure && !baseline.exposure_auto && baseline.exposure <= 3.5, json!({"source":first,"expected_exposure":self.initial_exposure,"actual_exposure":app.smoke_recipe().exposure}))?;
                let mut saved = baseline.clone();
                saved.exposure += 0.125;
                saved.saturation = 0.75;
                self.saved = Some(saved);
                super::scenarios::apply_edits(
                    app,
                    &[
                        number_edit(
                            "/exposure",
                            NumberChange::Relative(0.125),
                            super::scenarios::EXPOSURE_RANGE,
                        ),
                        number_edit(
                            "/saturation",
                            NumberChange::Set(0.75),
                            reference_saturation_range(),
                        ),
                    ],
                );
                app.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::Arrow(egui::Key::ArrowRight),
                    egui::Modifiers::NONE,
                )]);
                self.phase = Phase::SwitchAway;
            }
            Phase::SwitchAway => {
                if !smoke.direct_events.is_empty() || !Self::ready(app, &paths[1]) {
                    return Ok(false);
                }
                app.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::Arrow(egui::Key::ArrowLeft),
                    egui::Modifiers::NONE,
                )]);
                self.phase = Phase::SwitchBack;
            }
            Phase::SwitchBack => {
                if !smoke.direct_events.is_empty() || !Self::ready(app, first) {
                    return Ok(false);
                }
                self.check("browse-switch-restores-latest", Self::matches(app.smoke_recipe(), self.saved.as_ref().unwrap()), json!({"actual_exposure":app.smoke_recipe().exposure,"expected_exposure":self.saved.as_ref().unwrap().exposure}))?;
                self.phase = Phase::Saved;
            }
            Phase::Saved => {
                let Some(result) = self.read_disk(first.clone()) else {
                    return Ok(false);
                };
                match result {
                    Ok(Some(settings))
                        if Self::matches(&settings, self.saved.as_ref().unwrap()) =>
                    {
                        self.check(
                            "browse-sidecar-saved",
                            true,
                            json!({"source":first,"exposure":settings.exposure}),
                        )?
                    }
                    Ok(_) => return Ok(false),
                    Err(error) => return Err(format!("read saved sidecar: {error}")),
                }
                app.open(directory.to_owned(), crate::preferences::FolderMode::Import);
                self.phase = Phase::ReopenImport;
            }
            Phase::ReopenImport => {
                if app.folder_loader.loading()
                    || !app.browser.import_mode
                    || !app.selection_loaded()
                    || app.view_busy()
                {
                    return Ok(false);
                }
                app.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::Arrow(egui::Key::ArrowLeft),
                    egui::Modifiers::NONE,
                )]);
                self.phase = Phase::Import;
            }
            Phase::Import => {
                if !smoke.direct_events.is_empty()
                    || !Self::ready(app, first)
                    || app.edit_selection.paths() != [first.clone()]
                {
                    return Ok(false);
                }
                self.check("import-restores-saved", Self::matches(app.smoke_recipe(), self.saved.as_ref().unwrap()), json!({"actual_exposure":app.smoke_recipe().exposure,"expected_exposure":self.saved.as_ref().unwrap().exposure}))?;
                let mut review = self.saved.as_ref().unwrap().clone();
                review.exposure += 0.25;
                self.review = Some(review);
                super::scenarios::apply_exposure(app, 0.25);
                app.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::Arrow(egui::Key::ArrowRight),
                    egui::Modifiers::NONE,
                )]);
                self.phase = Phase::ImportAway;
            }
            Phase::ImportAway => {
                if !smoke.direct_events.is_empty() || !Self::ready(app, &paths[1]) {
                    return Ok(false);
                }
                app.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::Arrow(egui::Key::ArrowLeft),
                    egui::Modifiers::NONE,
                )]);
                self.phase = Phase::ImportBack;
            }
            Phase::ImportBack => {
                if !smoke.direct_events.is_empty() || !Self::ready(app, first) {
                    return Ok(false);
                }
                self.check("import-switch-retains-review", Self::matches(app.smoke_recipe(), self.review.as_ref().unwrap()), json!({"actual_exposure":app.smoke_recipe().exposure,"expected_exposure":self.review.as_ref().unwrap().exposure}))?;
                app.preferences.processing_destination = None;
                app.import.rescan(&mut app.browser, None);
                self.phase = Phase::NoDestination;
            }
            Phase::NoDestination => {
                let Some(button) = process_button() else {
                    return Ok(false);
                };
                self.check(
                    "process-button-disabled-without-destination",
                    !button.enabled,
                    json!(button),
                )?;
                self.phase = Phase::CopyAndDefault;
            }
            Phase::CopyAndDefault => {
                let blocker = app.import.process_blocker(&app.browser);
                self.check(
                    "process-requires-destination",
                    blocker == Some("Choose a destination folder first."),
                    json!({"blocker":blocker}),
                )?;
                app.process();
                self.check(
                    "blocked-process-does-not-start-batch",
                    !app.import.smoke_batch_progress().running,
                    json!({"running":app.import.smoke_batch_progress().running}),
                )?;
                app.actions.push(crate::actions::Edit::CopySettings);
                app.dispatch_actions(ctx);
                let copied = ctx.output(|output| {
                    output.commands.iter().rev().find_map(|command| {
                        if let egui::OutputCommand::CopyText(text) = command {
                            Some(text.clone())
                        } else {
                            None
                        }
                    })
                });
                ctx.output_mut(|output| {
                    output
                        .commands
                        .retain(|command| !matches!(command, egui::OutputCommand::CopyText(_)))
                });
                let decoded = copied
                    .as_ref()
                    .and_then(|text| foveonic::sidecar::decode(text.as_bytes()).ok());
                self.check(
                    "copy-requested-settings",
                    decoded
                        .as_ref()
                        .is_some_and(|s| Self::matches(s, self.review.as_ref().unwrap())),
                    json!({"clipboard_output":copied.is_some()}),
                )?;
                self.clipboard = copied;
                let scope = crate::editor::Scope::Tool(crate::editor::Section::Tone);
                app.editor_ui
                    .actions
                    .push(crate::editor::Action::Default(scope));
                app.apply_editor_actions(ctx);
                self.phase = Phase::CameraReset;
            }
            Phase::CameraReset => {
                let metadata = app
                    .selected_metadata()
                    .ok_or("missing metadata for reset")?;
                let camera = foveonic::render::camera_settings(
                    &metadata,
                    Some(app.smoke_recipe().rendering),
                )
                .map_err(|e| e.to_string())?;
                app.editor_ui.actions.push(crate::editor::Action::Reset(
                    crate::editor::Scope::Tool(crate::editor::Section::Tone),
                    true,
                ));
                app.apply_editor_actions(ctx);
                self.check("camera-reset", app.smoke_recipe().exposure == camera.exposure && app.smoke_recipe().exposure_auto == camera.exposure_auto, json!({"expected_exposure":camera.exposure,"actual_exposure":app.smoke_recipe().exposure}))?;
                self.check(
                    "tone-camera-reset-preserves-saturation",
                    app.smoke_recipe().saturation == 0.75,
                    json!({"expected":0.75,"actual":app.smoke_recipe().saturation}),
                )?;
                app.submit_photos();
                self.phase = Phase::DefaultReset;
            }
            Phase::DefaultReset => {
                if !Self::ready(app, first) {
                    return Ok(false);
                }
                app.editor_ui.actions.push(crate::editor::Action::Reset(
                    crate::editor::Scope::Tool(crate::editor::Section::Tone),
                    false,
                ));
                app.apply_editor_actions(ctx);
                self.check("user-default-reset", app.smoke_recipe().exposure == self.review.as_ref().unwrap().exposure && app.smoke_recipe().exposure_auto == self.review.as_ref().unwrap().exposure_auto, json!({"expected_exposure":self.review.as_ref().unwrap().exposure,"actual_exposure":app.smoke_recipe().exposure}))?;
                self.check(
                    "tone-user-reset-preserves-saturation",
                    app.smoke_recipe().saturation == 0.75,
                    json!({"expected":0.75,"actual":app.smoke_recipe().saturation}),
                )?;
                app.submit_photos();
                self.phase = Phase::Paste;
            }
            Phase::Paste => {
                if !Self::ready(app, first) {
                    return Ok(false);
                }
                super::scenarios::apply_exposure(app, 0.125);
                app.actions.push(crate::actions::Edit::PasteSettings(
                    self.clipboard.clone().unwrap(),
                ));
                let changed = app.dispatch_actions(ctx);
                self.check("paste-copied-settings", changed && Self::matches(app.smoke_recipe(), self.review.as_ref().unwrap()), json!({"actual_exposure":app.smoke_recipe().exposure,"expected_exposure":self.review.as_ref().unwrap().exposure}))?;
                app.submit_photos();
                self.phase = Phase::PasteHighlighted;
            }
            Phase::PasteHighlighted => {
                if !Self::ready(app, first) {
                    return Ok(false);
                }
                // Highlighting every photo pastes to loaded and unloaded photos alike.
                app.browser.highlight_paths(paths);
                app.actions.push(crate::actions::Edit::PasteSettings(
                    self.clipboard.clone().unwrap(),
                ));
                app.dispatch_actions(ctx);
                let review = self.review.as_ref().unwrap();
                let photos = paths
                    .iter()
                    .map(|path| {
                        let loaded = app.multi_edit.settings(path);
                        let recorded = app.sidecars.requested_settings(path);
                        json!({"source":path,"loaded":loaded.is_some(),
                            "loaded_matches":loaded.is_none_or(|settings| Self::matches(settings, review)),
                            "recorded_matches":recorded.is_some_and(|settings| Self::matches(settings, review))})
                    })
                    .collect::<Vec<_>>();
                self.check(
                    "paste-reaches-every-highlighted-photo",
                    photos.iter().all(|photo| {
                        photo["loaded_matches"] == true && photo["recorded_matches"] == true
                    }),
                    json!({"photos":photos}),
                )?;
                app.browser.highlight_paths(std::slice::from_ref(first));
                self.phase = Phase::MenuActions;
            }
            Phase::MenuActions => {
                if !Self::ready(app, first)
                    || app.edit_selection.paths() != std::slice::from_ref(first)
                {
                    return Ok(false);
                }
                // Menu edits reach the photo through the frame's own action pass,
                // and an edit made while Before is shown returns to Current.
                app.toggle_before(crate::editor::Rendition::Camera);
                let current = app.smoke_recipe().clone();
                let mut preset = current.clone();
                preset.saturation_enabled = true;
                preset.saturation = 0.5;
                app.editor_ui.actions.extend([
                    crate::editor::Action::PreventSaturationClipping(
                        !current.prevent_color_clipping,
                    ),
                    crate::editor::Action::ExposureAutoCompensation(
                        foveonic::render::AutoExposureCompensation::Dial,
                    ),
                    crate::editor::Action::Apply(
                        crate::editor::Scope::Tool(crate::editor::Section::Saturation),
                        Box::new(preset),
                    ),
                ]);
                self.expected_menu = Some(current);
                self.phase = Phase::MenuApplied;
            }
            Phase::MenuApplied => {
                if !app.editor_ui.actions.is_empty() || !Self::ready(app, first) {
                    return Ok(false);
                }
                let before = self.expected_menu.take().unwrap();
                let settings = app.smoke_recipe();
                self.check(
                    "menu-actions-edit-the-photo",
                    settings.prevent_color_clipping != before.prevent_color_clipping
                        && settings.exposure_auto_compensation
                            == foveonic::render::AutoExposureCompensation::Dial
                        && settings.saturation == 0.5
                        && settings.crop == before.crop
                        && settings.rotation == before.rotation,
                    json!({"prevent_color_clipping":settings.prevent_color_clipping,
                        "exposure_auto_compensation":format!("{:?}",settings.exposure_auto_compensation),
                        "saturation":settings.saturation}),
                )?;
                self.check(
                    "menu-edit-leaves-before",
                    app.before.active == crate::editor::Rendition::Current,
                    json!({"rendition":format!("{:?}",app.before.active)}),
                )?;
                // Pasting the copied review recipe restores it for the steps after.
                app.actions.push(crate::actions::Edit::PasteSettings(
                    self.clipboard.clone().unwrap(),
                ));
                app.dispatch_actions(ctx);
                app.submit_photos();
                self.phase = Phase::InvalidPaste;
            }
            Phase::InvalidPaste => {
                if !Self::ready(app, first) {
                    return Ok(false);
                }
                let before = app.smoke_recipe().clone();
                app.actions
                    .push(crate::actions::Edit::PasteSettings("not a recipe".into()));
                let changed = app.dispatch_actions(ctx);
                self.check(
                    "invalid-paste-reports-error",
                    !changed
                        && Self::matches(app.smoke_recipe(), &before)
                        && app.activity.unread_errors(),
                    json!({"changed":changed,"unread_errors":app.activity.unread_errors()}),
                )?;
                app.activity.open();
                self.check(
                    "log-acknowledges-paste-error",
                    !app.activity.unread_errors() && app.activity.is_open(),
                    json!({"unread_errors":app.activity.unread_errors()}),
                )?;
                smoke.direct_events.push_back(vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
                self.phase = Phase::AcknowledgePaste;
            }
            Phase::AcknowledgePaste => {
                if !smoke.direct_events.is_empty() || app.activity.is_open() {
                    return Ok(false);
                }
                self.phase = Phase::ImportDisk;
            }
            Phase::ImportDisk => {
                let Some(result) = self.read_disk(first.clone()) else {
                    return Ok(false);
                };
                let actual = result.map_err(|error| format!("read source sidecar: {error}"))?;
                self.check("import-does-not-write-source", actual.as_ref().is_some_and(|s| Self::matches(s, self.saved.as_ref().unwrap())), json!({"source":first,"actual_exposure":actual.as_ref().map(|s| s.exposure),"expected_exposure":self.saved.as_ref().unwrap().exposure}))?;
                let destination = destination.to_owned();
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let result = std::fs::create_dir_all(destination)
                        .map(|_| json!({}))
                        .map_err(|e| e.to_string());
                    let _ = tx.send(result);
                });
                self.io = Some(rx);
                self.phase = Phase::PrepareOutput;
            }
            Phase::PrepareOutput => {
                let Some(result) = self.poll_io()? else {
                    return Ok(false);
                };
                self.check("destination-created", true, result)?;
                app.browser.hide_reviewed = false;
                app.browser
                    .marks
                    .insert(first.clone(), crate::browser::Mark::Pick);
                app.browser.refresh_filter();
                app.preferences.processing_destination = Some(destination.to_owned());
                app.preferences.import_processing.copy_only = true;
                app.import.rescan(&mut app.browser, Some(destination));
                self.phase = Phase::OutputScan;
            }
            Phase::OutputScan => {
                if !app.import.ready(&app.browser) {
                    return Ok(false);
                }
                let Some(button) = process_button().filter(|button| button.enabled) else {
                    return Ok(false);
                };
                self.check("process-button-enabled-with-pick", true, json!(button))?;
                app.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::TogglePick,
                    egui::Modifiers::NONE,
                )]);
                self.phase = Phase::Unpicked;
            }
            Phase::Unpicked => {
                if app.browser.mark(first) != crate::browser::Mark::Unmarked {
                    return Ok(false);
                }
                let Some(button) = process_button().filter(|button| !button.enabled) else {
                    return Ok(false);
                };
                self.check("unpick-disables-process-button", true, json!(button))?;
                app.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::TogglePick,
                    egui::Modifiers::NONE,
                )]);
                self.phase = Phase::Repicked;
            }
            Phase::Repicked => {
                if app.browser.mark(first) != crate::browser::Mark::Pick {
                    return Ok(false);
                }
                let Some(button) = process_button().filter(|button| button.enabled) else {
                    return Ok(false);
                };
                self.check("repick-enables-process-button", true, json!(button))?;
                let [left, top, right, bottom] = button.rect;
                let position = egui::pos2((left + right) * 0.5, (top + bottom) * 0.5);
                for pressed in [true, false] {
                    smoke.direct_events.push_back(vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]);
                }
                smoke.direct_events.push_back(vec![]);
                self.phase = Phase::ProcessClicked;
            }
            Phase::ProcessClicked => {
                if !smoke.direct_events.is_empty() {
                    return Ok(false);
                }
                let progress = app.import.smoke_batch_progress();
                self.check("process-button-click-starts-batch", (progress.running || progress.completed == 1) && progress.total == 1 && progress.errors.is_empty(), json!({"running":progress.running,"completed":progress.completed,"total":progress.total,"errors":progress.errors}))?;
                // A later edit must not change the already submitted copy recipe.
                super::scenarios::apply_exposure(app, 0.125);
                self.phase = Phase::OutputWait;
            }
            Phase::OutputWait => {
                let progress = app.import.smoke_batch_progress();
                if progress.running {
                    return Ok(false);
                }
                self.check(
                    "copy-batch-completed",
                    progress.completed == 1 && progress.errors.is_empty(),
                    json!({"completed":progress.completed,"errors":progress.errors}),
                )?;
                let source = first.clone();
                let output = destination.join(first.file_name().ok_or("source has no filename")?);
                let expected = self.review.as_ref().unwrap().clone();
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let result = (|| -> Result<Value, String> {
                        use std::io::Read;
                        let mut original = std::io::BufReader::new(
                            std::fs::File::open(&source).map_err(|e| e.to_string())?,
                        );
                        let mut copy = std::io::BufReader::new(
                            std::fs::File::open(&output).map_err(|e| e.to_string())?,
                        );
                        let mut a = [0; 65536];
                        let mut b = [0; 65536];
                        let bytes_equal = loop {
                            let n = original.read(&mut a).map_err(|e| e.to_string())?;
                            let m = copy.read(&mut b).map_err(|e| e.to_string())?;
                            if n != m || a[..n] != b[..m] {
                                break false;
                            }
                            if n == 0 {
                                break true;
                            }
                        };
                        let recipe = foveonic::sidecar::load(&output)
                            .map(foveonic::sidecar::Saved::settings)
                            .map_err(|e| e.to_string())?;
                        Ok(
                            json!({"source":source,"output":output,"bytes_equal":bytes_equal,"recipe_equal":recipe.as_ref().is_some_and(|s| Self::matches(s, &expected)),"expected_exposure":expected.exposure,"copied_exposure":recipe.as_ref().map(|s| s.exposure)}),
                        )
                    })();
                    let _ = tx.send(result);
                });
                self.io = Some(rx);
                self.phase = Phase::OutputVerify;
            }
            Phase::OutputVerify => {
                let Some(result) = self.poll_io()? else {
                    return Ok(false);
                };
                self.check(
                    "copied-original-and-frozen-recipe",
                    result["bytes_equal"] == true && result["recipe_equal"] == true,
                    result,
                )?;
                self.phase = Phase::Damaged;
            }
            Phase::Damaged => {
                if !self.damaged.advance(
                    app,
                    smoke,
                    first,
                    &destination.parent().unwrap().join("lifecycle-damaged"),
                    self.saved.as_ref().unwrap(),
                )? {
                    return Ok(false);
                }
                app.open(
                    directory.join("missing-lifecycle-folder"),
                    crate::preferences::FolderMode::Browse,
                );
                self.phase = Phase::MissingOpen;
            }
            Phase::MissingOpen => {
                if app.folder_loader.loading() {
                    return Ok(false);
                }
                self.check(
                    "missing-folder-reports-error",
                    app.activity.unread_errors(),
                    json!({"unread_errors":app.activity.unread_errors()}),
                )?;
                app.activity.open();
                self.check(
                    "log-acknowledges-open-error",
                    !app.activity.unread_errors() && app.activity.is_open(),
                    json!({"unread_errors":app.activity.unread_errors()}),
                )?;
                smoke.direct_events.push_back(vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
                self.phase = Phase::AcknowledgeOpen;
            }
            Phase::AcknowledgeOpen => {
                if !smoke.direct_events.is_empty() || app.activity.is_open() {
                    return Ok(false);
                }
                app.open(directory.to_owned(), crate::preferences::FolderMode::Browse);
                self.phase = Phase::ReopenBrowse;
            }
            Phase::ReopenBrowse => {
                if app.folder_loader.loading()
                    || app.browser.import_mode
                    || !app.selection_loaded()
                    || app.view_busy()
                {
                    return Ok(false);
                }
                app.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::Arrow(egui::Key::ArrowLeft),
                    egui::Modifiers::NONE,
                )]);
                self.phase = Phase::Restored;
            }
            Phase::Restored => {
                if !smoke.direct_events.is_empty() || !Self::ready(app, first) {
                    return Ok(false);
                }
                self.check("browse-reopen-restores-saved", Self::matches(app.smoke_recipe(), self.saved.as_ref().unwrap()), json!({"actual_exposure":app.smoke_recipe().exposure,"expected_exposure":self.saved.as_ref().unwrap().exposure}))?;
                expected.insert(first.clone(), self.saved.as_ref().unwrap().clone());
                app.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::SelectAll,
                    egui::Modifiers::COMMAND,
                )]);
                self.phase = Phase::SelectAll;
            }
            Phase::SelectAll => {
                if app.edit_selection.paths() != paths {
                    return Ok(false);
                }
                self.phase = Phase::Done;
            }
            Phase::Done => return Ok(true),
        }
        Ok(false)
    }
    fn poll_io(&mut self) -> Result<Option<Value>, String> {
        let Some(rx) = &self.io else {
            return Err("missing lifecycle file worker".into());
        };
        match rx.try_recv() {
            Ok(result) => {
                self.io = None;
                result.map(Some)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err("lifecycle file worker stopped".into()),
        }
    }
    pub(super) fn evidence(&self) -> Value {
        json!({"checks":self.checks,"damaged":self.damaged.evidence(),"actions":["open-browse","edit-exposure:+0.125","switch-away","switch-back","read-sidecar","open-import","edit-exposure:+0.25","switch-away","switch-back","process-without-destination","copy-settings","save-tone-default","reset-camera-tone","reset-user-tone","edit-and-paste","paste-to-highlighted","menu-actions-during-before","paste-invalid","acknowledge-log","read-sidecar","pick-and-copy-original","edit-after-submission","verify-copy-and-frozen-recipe","open-damaged-raw","recover-valid-raw","open-missing-folder","acknowledge-log","open-browse","select-all"],"scope":"recipe persistence, import review, clipboard handlers, scoped resets, copied originals and file error recovery; processed image encoding is not exercised"})
    }
}

fn process_button() -> Option<crate::automation::Button> {
    crate::automation::snapshot()?
        .buttons
        .into_iter()
        .find(|button| button.label == "Process")
}

fn reference_saturation_range() -> [f64; 2] {
    let range = crate::controls::saturation::REFERENCE_SATURATION_RANGE;
    [f64::from(*range.start()), f64::from(*range.end())]
}
