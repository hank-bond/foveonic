//! Runs declared native diagnostic cases through the existing application and capture controller.

use super::Smoke;
use crate::{
    multi_edit_controls::{self, Edit, Mapping, NumberChange},
    preferences::{FolderMode, PreviewMode},
    App,
};
use eframe::egui;
use foveonic::{profile::Mode, render::Settings};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum Case {
    Initial,
    WarmDisk,
    PrefetchedSelection,
    WarmEdit,
    CachedSwitch,
    RapidSupersession,
    MemoryPressure,
    SliderDrag,
    SelectionJourney,
    SelectionPressure,
    BackgroundEdit,
    TransitionJourney,
    LifecycleJourney,
    BrowserJourney,
    CollectionJourney,
    ChoiceJourney,
    SourceLoading,
}
impl Case {
    fn selection_journey(self) -> bool {
        matches!(self, Self::SelectionJourney | Self::SelectionPressure)
    }
    fn manual_exposure_journey(self) -> bool {
        self.selection_journey()
            || matches!(
                self,
                Self::BackgroundEdit
                    | Self::TransitionJourney
                    | Self::LifecycleJourney
                    | Self::BrowserJourney
                    | Self::CollectionJourney
                    | Self::ChoiceJourney
                    | Self::SourceLoading
            )
    }
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Quality {
    Full,
    Fast,
    Draft,
    Thumb,
}
fn default_folder_mode() -> FolderMode {
    FolderMode::Import
}
fn read_folder_mode<'de, D: serde::Deserializer<'de>>(input: D) -> Result<FolderMode, D::Error> {
    match String::deserialize(input)?.as_str() {
        "browse" => Ok(FolderMode::Browse),
        "import" => Ok(FolderMode::Import),
        _ => Err(serde::de::Error::custom(
            "folder_mode must be browse or import",
        )),
    }
}
impl Quality {
    fn preview(self) -> PreviewMode {
        match self {
            Self::Full => PreviewMode::Full,
            Self::Fast => PreviewMode::Fast,
            Self::Draft => PreviewMode::Draft,
            Self::Thumb => PreviewMode::Thumb,
        }
    }
    fn divisor(self) -> u32 {
        self.preview().working_scale().divisor()
    }
    fn expected_scale(
        self,
        metadata: &foveonic::source::Metadata,
    ) -> Option<foveonic::render::Scale> {
        if matches!(self, Self::Thumb) {
            foveonic::render::ControlCapabilities::from_metadata(metadata)
                .ok()
                .map(|caps| caps.thumb_scale())
        } else {
            Some(self.preview().working_scale())
        }
    }
}
#[derive(Deserialize)]
struct Config {
    name: Case,
    photo_count: usize,
    quality: Quality,
    #[serde(deserialize_with = "read_mode")]
    mode: Mode,
    directory: PathBuf,
    sources: Vec<PathBuf>,
    capture_directory: PathBuf,
    repeats: usize,
    rapid_edits: usize,
    pressure_bytes: u64,
    timeout_seconds: u64,
    #[serde(default = "default_disk_cache_bytes")]
    disk_cache_bytes: u64,
    #[serde(default = "default_memory_cache_bytes")]
    memory_cache_bytes: u64,
    #[serde(default = "default_control_pointer")]
    control_pointer: String,
    #[serde(default = "default_drag_steps")]
    drag_steps: usize,
    #[serde(default = "default_drag_fraction")]
    drag_fraction: f32,
    #[serde(default)]
    drag_pattern: super::slider::Pattern,
    #[serde(default)]
    light_theme: bool,
    #[serde(default = "default_true")]
    mixed_values: bool,
    #[serde(default = "default_editing_section")]
    editing_section: crate::preferences::EditingSection,
    #[serde(default)]
    capture_images: bool,
    #[serde(default)]
    save_on_exit: bool,
    #[serde(default = "default_folder_mode", deserialize_with = "read_folder_mode")]
    folder_mode: FolderMode,
    #[serde(default)]
    exit_with_pending: bool,
    #[serde(default)]
    seed: u64,
    #[serde(default = "default_action_count")]
    action_count: usize,
}
fn default_action_count() -> usize {
    8
}
fn default_editing_section() -> crate::preferences::EditingSection {
    crate::preferences::EditingSection::Tone
}
fn default_true() -> bool {
    true
}
fn default_control_pointer() -> String {
    "/exposure".into()
}
fn default_drag_steps() -> usize {
    12
}
fn default_drag_fraction() -> f32 {
    0.2
}
fn default_memory_cache_bytes() -> u64 {
    crate::preferences::DEFAULT_MEMORY_CACHE_BYTES
}
fn default_disk_cache_bytes() -> u64 {
    foveonic::decode_cache::Settings::default().max_bytes
}
fn read_mode<'de, D: serde::Deserializer<'de>>(input: D) -> Result<Mode, D::Error> {
    String::deserialize(input)?
        .parse()
        .map_err(serde::de::Error::custom)
}
#[derive(Clone, Copy)]
enum Action {
    Initial,
    Edit(f64),
    Switch(usize),
    Remove,
    Readd,
    Rapid,
    Pressure,
    SliderDrag,
    SelectionJourney,
    Restore,
    Evict,
    Rebuild,
    BackgroundEdit,
    BackgroundWait,
    TransitionJourney,
    ChoiceJourney,
    SourceLoading,
    LifecycleJourney,
    BrowserJourney,
    CollectionJourney,
}
impl Action {
    fn name(self) -> &'static str {
        match self {
            Self::Initial => "initial-selection",
            Self::Edit(_) => "warm-edit",
            Self::Switch(_) => "cached-switch",
            Self::Remove => "remove-pending",
            Self::Readd => "readd-pending",
            Self::Rapid => "rapid-supersession",
            Self::Pressure => "memory-pressure",
            Self::SliderDrag => "slider-drag",
            Self::SelectionJourney => "selection-journey",
            Self::Restore => "restore-selection",
            Self::Evict => "cache-eviction",
            Self::Rebuild => "evicted-edit",
            Self::BackgroundEdit => "background-edit",
            Self::BackgroundWait => "background-finish",
            Self::TransitionJourney => "transition-journey",
            Self::ChoiceJourney => "choice-journey",
            Self::SourceLoading => "source-loading",
            Self::LifecycleJourney => "lifecycle-journey",
            Self::BrowserJourney => "browser-journey",
            Self::CollectionJourney => "collection-journey",
        }
    }
    fn coverage(self) -> &'static str {
        match self {
            Self::Remove
            | Self::SourceLoading
            | Self::SelectionJourney
            | Self::Rebuild
            | Self::TransitionJourney
            | Self::LifecycleJourney
            | Self::BrowserJourney => "mixed",
            Self::Switch(_)
            | Self::Readd
            | Self::SliderDrag
            | Self::Restore
            | Self::ChoiceJourney => "input-dispatch",
            _ => "direct-command",
        }
    }
}
enum Phase {
    Setup,
    Folder,
    PrimingDisk,
    Starting,
    Running,
    Saving,
    Done,
}

struct ImageCapture {
    evidence: serde_json::Value,
    result: crossbeam_channel::Receiver<Result<(), String>>,
    finished: bool,
}

pub(super) struct Scenario {
    config: Config,
    manifest: serde_json::Value,
    phase: Phase,
    paths: Vec<PathBuf>,
    expected: BTreeMap<PathBuf, Settings>,
    targets: Vec<PathBuf>,
    cached: BTreeMap<PathBuf, Weak<foveonic::image::ImageTexture>>,
    actions: VecDeque<Action>,
    action: Option<Action>,
    step: u64,
    began: Instant,
    minimum_revision: u64,
    observed: BTreeMap<PathBuf, u64>,
    results: Vec<serde_json::Value>,
    rapid_left: usize,
    ready_time: Option<f64>,
    failed: Option<String>,
    drag: Option<super::slider::Drag>,
    drag_checked: bool,
    drag_reveal_frames: usize,
    drag_layout: Option<([f32; 4], [f32; 4], usize)>,
    failure_screenshot: Option<Instant>,
    screenshot_finished: bool,
    image_captures: Option<Vec<ImageCapture>>,
    journey: Option<super::selection::SelectionJourney>,
    background_edit: Option<super::background::BackgroundEdit>,
    transitions: Option<super::transitions::TransitionJourney>,
    choices: Option<super::choices::ChoiceJourney>,
    source_loading: Option<super::source_loading::SourceLoading>,
    lifecycle: Option<super::lifecycle::LifecycleJourney>,
    browser_journey: Option<super::browser_journey::BrowserJourney>,
    collection_journey: Option<super::collection::CollectionJourney>,
    feedback: Vec<super::journey::ImageFeedback>,
    retained_before: Vec<PathBuf>,
    eviction: Option<serde_json::Value>,
    rebuild_edited: bool,
    ui_started: Option<Instant>,
    ui_samples: Vec<serde_json::Value>,
    geometry: serde_json::Value,
    shutdown_exposures: Vec<f32>,
    shutdown_pending: Option<serde_json::Value>,
}
impl Scenario {
    pub(super) fn expects_failure(&self, path: &Path) -> bool {
        self.lifecycle
            .as_ref()
            .is_some_and(|journey| journey.expects_failure(path))
    }
    pub(super) fn from_env() -> Option<Self> {
        let path = std::env::var_os("FOVEONIC_GUI_SCENARIO_MANIFEST")?;
        // Explicit scenario configuration is cold application setup, never a widget filesystem probe.
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).expect("cannot read scenario manifest"))
                .expect("invalid scenario manifest");
        let config: Config = serde_json::from_value(manifest["scenario"].clone())
            .expect("invalid scenario configuration");
        assert!(
            (1..=8).contains(&config.photo_count),
            "native scenario photo count must be one through eight"
        );
        assert_eq!(
            config.sources.len(),
            config.photo_count,
            "scenario inputs must match its photo count"
        );
        assert!(
            config.name != Case::CachedSwitch || config.photo_count >= 2,
            "cached switching needs at least two photographs"
        );
        assert!(
            config.name != Case::SliderDrag
                || (config.photo_count >= 2
                    && config.drag_steps >= 3
                    && (0.0..0.45).contains(&config.drag_fraction)
                    && config.control_pointer.starts_with('/')),
            "slider drag requires two photos, a setting pointer and a usable gesture"
        );
        assert!(
            config.repeats > 0 && config.rapid_edits > 0 && config.timeout_seconds > 0,
            "scenario repetition and timeout values must be positive"
        );
        assert!(
            config.name != Case::BackgroundEdit || config.folder_mode == FolderMode::Import,
            "background-edit requires Import mode"
        );
        assert!(!config.exit_with_pending || (config.folder_mode == FolderMode::Browse && config.save_on_exit && config.mode != Mode::Off && !config.capture_images && (2..=8).contains(&config.photo_count) && matches!(config.name, Case::Initial | Case::WarmDisk)), "pending shutdown requires Browse, a saved capture, two to eight photos and an initial or warm-disk case");
        let shutdown_exposures = if config.exit_with_pending {
            manifest["inputs"]
                .as_array()
                .expect("declared inputs")
                .iter()
                .map(|input| {
                    assert_eq!(input["settings"]["exposure_auto"], false);
                    let exposure = input["settings"]["exposure"]
                        .as_f64()
                        .expect("declared manual exposure")
                        as f32;
                    assert!(
                        exposure.is_finite() && (-4.0..=3.75).contains(&exposure),
                        "pending shutdown exposure out of range"
                    );
                    exposure
                })
                .collect()
        } else {
            Vec::new()
        };
        assert!(
            !config.capture_images
                || config.name == Case::SliderDrag
                || config.name.manual_exposure_journey(),
            "image comparison requires an exposure or selection journey"
        );
        let exposures = if config.name.manual_exposure_journey() {
            assert!(
                config.photo_count >= 2,
                "selection journeys require at least two photographs"
            );
            let exposures = manifest["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|input| {
                    assert_eq!(input["settings"]["exposure_auto"], false);
                    input["settings"]["exposure"]
                        .as_f64()
                        .expect("declared exposure") as f32
                })
                .collect();
            exposures
        } else {
            Vec::new()
        };
        let journey = config
            .name
            .selection_journey()
            .then(|| super::selection::SelectionJourney::new(exposures.clone()));
        assert!(
            config.name != Case::TransitionJourney
                || super::transitions::valid_count(config.action_count),
            "transition action_count must be 1 through 64"
        );
        let transitions = (config.name == Case::TransitionJourney).then(|| {
            super::transitions::TransitionJourney::new(
                exposures.clone(),
                config.seed,
                config.action_count,
            )
        });
        let browser_journey = (config.name == Case::BrowserJourney).then(|| {
            super::browser_journey::BrowserJourney::new(exposures.clone(), config.sources.clone())
        });
        let collection_journey = (config.name == Case::CollectionJourney).then(|| {
            super::collection::CollectionJourney::new(
                config.sources.clone(),
                exposures.clone(),
                config
                    .capture_directory
                    .parent()
                    .unwrap()
                    .join("collection-inputs"),
            )
        });
        let choices = (config.name == Case::ChoiceJourney).then(|| {
            super::choices::ChoiceJourney::new(
                manifest["inputs"]
                    .as_array()
                    .expect("choice journey requires declared inputs"),
            )
            .expect("invalid choice journey fixture")
        });
        let background_edit = (config.name == Case::BackgroundEdit)
            .then(|| super::background::BackgroundEdit::new(exposures.clone()));
        let lifecycle = (config.name == Case::LifecycleJourney)
            .then(|| super::lifecycle::LifecycleJourney::new(exposures[0]));
        let source_loading = (config.name == Case::SourceLoading)
            .then(|| super::source_loading::SourceLoading::new(exposures.clone()));
        if matches!(
            config.name,
            Case::SliderDrag
                | Case::LifecycleJourney
                | Case::ChoiceJourney
                | Case::BackgroundEdit
                | Case::SourceLoading
        ) {
            crate::automation::enable();
        }
        let feedback = (0..config.photo_count)
            .map(|_| super::journey::ImageFeedback::default())
            .collect();
        Some(Self {
            paths: config.sources.clone(),
            config,
            source_loading,
            manifest,
            phase: Phase::Setup,
            expected: BTreeMap::new(),
            targets: vec![],
            cached: BTreeMap::new(),
            actions: VecDeque::new(),
            action: None,
            step: 0,
            began: Instant::now(),
            minimum_revision: 0,
            observed: BTreeMap::new(),
            results: vec![],
            rapid_left: 0,
            ready_time: None,
            failed: None,
            drag: None,
            drag_checked: false,
            drag_reveal_frames: 0,
            drag_layout: None,
            failure_screenshot: None,
            screenshot_finished: false,
            image_captures: None,
            journey,
            background_edit,
            transitions,
            choices,
            lifecycle,
            browser_journey,
            collection_journey,
            feedback,
            retained_before: Vec::new(),
            eviction: None,
            rebuild_edited: false,
            ui_started: None,
            ui_samples: Vec::new(),
            geometry: serde_json::Value::Null,
            shutdown_exposures,
            shutdown_pending: None,
        })
    }
    fn marker(&self, phase: &str, extra: serde_json::Value) {
        let mut fields = serde_json::json!({"scenario":self.manifest["scenario"]["name"],"action":self.action.map_or("setup",Action::name),"phase":phase,"step":self.step,"photo_count":self.targets.len(),"nominal_scale_divisor":self.config.quality.divisor()});
        fields
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        crate::timing::scenario_step(
            "scenario.step",
            self.action.map_or("direct-command", Action::coverage),
            fields,
        );
    }
    pub(super) fn configure_cache(&self, settings: &mut foveonic::decode_cache::Settings) {
        settings.classic = true;
        settings.merrill = true;
        settings.quattro = true;
        settings.enabled = self.config.disk_cache_bytes > 0;
        settings.max_bytes = self.config.disk_cache_bytes;
    }
    pub(super) fn tick(&mut self, app: &mut App, ctx: &egui::Context, smoke: &mut Smoke) {
        ctx.request_repaint_after(Duration::from_millis(20));
        if matches!(self.phase, Phase::Running) {
            self.observe_ui(app, ctx);
            if self.exposure_model().is_some() {
                for index in 0..self.paths.len() {
                    let desired = self.exposure_model().unwrap().exposures()[index];
                    let shown = self
                        .frame(app, &self.paths[index])
                        .filter(|frame| frame.rendition == crate::editor::Rendition::Current)
                        .map(|frame| frame.artifact.settings.exposure);
                    if let Err(error) = self.feedback[index].observe(desired, shown) {
                        self.failed = Some(format!("photo {index}: {error}"));
                    }
                }
            }
        }
        if let Some(error) = app.diagnostics.error() {
            self.failed.get_or_insert_with(|| error.to_owned());
        }
        if !matches!(self.phase, Phase::Saving | Phase::Done)
            && self.began.elapsed() > Duration::from_secs(self.config.timeout_seconds)
        {
            self.failed
                .get_or_insert_with(|| format!("scenario step {} did not settle", self.step));
        }
        if self.failed.is_some() && !matches!(self.phase, Phase::Saving | Phase::Done) {
            self.finish(app);
        }
        match self.phase {
            Phase::Setup => {
                if app.worker.busy() || !app.sidecars.ready() || app.frame.is_none() {
                    return;
                }
                app.preferences.multi_edit_limit = self.config.photo_count;
                app.preferences.memory_cache_bytes = self.config.memory_cache_bytes;
                app.preferences.preview_quality = self.config.quality.preview();
                if matches!(self.config.name, Case::ChoiceJourney | Case::BackgroundEdit) {
                    app.preferences.collapsed = [
                        crate::preferences::Section::Navigator,
                        crate::preferences::Section::Scopes,
                        crate::preferences::Section::Metadata,
                    ]
                    .into();
                    let section = if self.config.name == Case::ChoiceJourney {
                        crate::preferences::EditingSection::WhiteBalance
                    } else {
                        crate::preferences::EditingSection::Tone
                    };
                    app.preferences
                        .editing_sections
                        .retain(|current| *current != section);
                    app.preferences.editing_sections.insert(0, section);
                }
                if self.config.name == Case::SliderDrag {
                    let pointer = &self.config.control_pointer;
                    if let Some(index) = pointer
                        .strip_prefix("/photographic/color_editor/bands/")
                        .and_then(|rest| rest.split('/').next())
                        .and_then(|index| index.parse().ok())
                    {
                        crate::editor::direct_color::select(ctx, index);
                    } else if pointer.starts_with("/photographic/color_editor/master/") {
                        crate::editor::direct_color::select(ctx, 8);
                    } else if let Some(index) = pointer
                        .strip_prefix("/photographic/color_editor/advanced/")
                        .and_then(|rest| rest.split('/').next())
                        .and_then(|index| index.parse().ok())
                    {
                        crate::editor::advanced_color::select(ctx, index);
                    }
                    ctx.set_theme(if self.config.light_theme {
                        egui::Theme::Light
                    } else {
                        egui::Theme::Dark
                    });
                    app.preferences.collapsed = [
                        crate::preferences::Section::Navigator,
                        crate::preferences::Section::Scopes,
                        crate::preferences::Section::Metadata,
                    ]
                    .into();
                    app.preferences
                        .editing_sections
                        .retain(|section| *section != self.config.editing_section);
                    app.preferences
                        .editing_sections
                        .insert(0, self.config.editing_section);
                }
                app.open(self.config.directory.clone(), self.config.folder_mode);
                self.phase = Phase::Folder;
            }
            Phase::Folder => {
                if app.folder_loader.loading()
                    || app.browser.import_mode != (self.config.folder_mode == FolderMode::Import)
                    || !app.sidecars.ready()
                    || app.worker.busy()
                {
                    return;
                }
                if self.config.name != Case::Initial && self.config.disk_cache_bytes > 0 {
                    app.worker
                        .cache
                        .prefetch_window(self.paths.clone(), self.paths.first().cloned());
                }
                self.phase = Phase::PrimingDisk;
            }
            Phase::PrimingDisk => {
                if self.config.name == Case::PrefetchedSelection
                    && self.config.mode != Mode::Off
                    && !app.diagnostics.active()
                {
                    if !app.diagnostics.starting() {
                        app.diagnostics
                            .start(self.config.mode, self.manifest.clone())
                            .expect("cannot start preparation capture");
                        app.diagnostics.set_start_state(app.diagnostic_state());
                    }
                    return;
                }
                if app.worker.busy() || !app.sidecars.ready() {
                    return;
                }
                let ready = app.worker.cache.status().ready;
                if self.config.name != Case::Initial
                    && self.config.disk_cache_bytes > 0
                    && !self.paths.iter().all(|path| ready.contains(path))
                {
                    return;
                }
                if self.config.name == Case::PrefetchedSelection {
                    let processing = app.worker.memory_status();
                    if !self.paths.iter().all(|path| {
                        processing.reduced_sources.iter().any(|(cached, origin)| {
                            cached == path
                                && app.worker.cache.source_origin(path).as_ref() == Some(origin)
                        })
                    }) {
                        return;
                    }
                }
                smoke.multi_paths = self.paths.clone();
                self.targets = self.paths.clone();
                self.actions.push_back(Action::Initial);
                match self.config.name {
                    Case::Initial | Case::WarmDisk => {}
                    Case::PrefetchedSelection => self.actions.extend(
                        (0..self.config.repeats)
                            .map(|index| Action::Edit(if index % 2 == 0 { 0.25 } else { -0.25 })),
                    ),
                    Case::WarmEdit => self.actions.extend(
                        (0..self.config.repeats)
                            .map(|index| Action::Edit(if index % 2 == 0 { 0.25 } else { -0.25 })),
                    ),
                    Case::CachedSwitch => self.actions.extend([
                        Action::Switch(0),
                        Action::Switch(1),
                        Action::Switch(0),
                        Action::Readd,
                        Action::Remove,
                        Action::Readd,
                    ]),
                    Case::RapidSupersession => self.actions.push_back(Action::Rapid),
                    Case::MemoryPressure => {
                        self.actions.push_back(Action::Pressure);
                        self.actions
                            .extend((0..self.config.repeats).map(|_| Action::Edit(0.01)));
                    }
                    Case::SliderDrag => self.actions.push_back(Action::SliderDrag),
                    Case::SelectionJourney => self.actions.extend([
                        Action::SelectionJourney,
                        Action::Switch(0),
                        Action::Switch(self.paths.len() - 1),
                        Action::Restore,
                    ]),
                    Case::SelectionPressure => self.actions.extend([
                        Action::SelectionJourney,
                        Action::Switch(0),
                        Action::Evict,
                        Action::Rebuild,
                        Action::Restore,
                    ]),
                    Case::TransitionJourney => self.actions.push_back(Action::TransitionJourney),
                    Case::ChoiceJourney => self.actions.push_back(Action::ChoiceJourney),
                    Case::SourceLoading => self.actions.push_back(Action::SourceLoading),
                    Case::LifecycleJourney => self.actions.push_back(Action::LifecycleJourney),
                    Case::BrowserJourney => self.actions.push_back(Action::BrowserJourney),
                    Case::CollectionJourney => self.actions.push_back(Action::CollectionJourney),
                    Case::BackgroundEdit => self
                        .actions
                        .extend([Action::BackgroundEdit, Action::BackgroundWait]),
                }
                if self.config.mode == Mode::Off || self.config.name == Case::PrefetchedSelection {
                    self.next(app, ctx, smoke);
                } else {
                    app.diagnostics
                        .start(self.config.mode, self.manifest.clone())
                        .expect("cannot start scenario capture");
                    app.diagnostics.set_start_state(app.diagnostic_state());
                    self.phase = Phase::Starting;
                }
            }
            Phase::Starting => {
                if app.diagnostics.active() {
                    self.next(app, ctx, smoke);
                }
            }
            Phase::Running => {
                if matches!(self.action, Some(Action::SourceLoading)) {
                    let result =
                        self.source_loading
                            .as_mut()
                            .unwrap()
                            .advance(app, ctx, smoke, &self.paths);
                    self.update_journey_expectations();
                    match result {
                        Ok(false) => return,
                        Ok(true) => {}
                        Err(error) => {
                            self.failed = Some(error);
                            self.finish(app);
                            return;
                        }
                    }
                }
                if matches!(self.action, Some(Action::ChoiceJourney)) {
                    match self.choices.as_mut().unwrap().advance(
                        app,
                        smoke,
                        &self.paths,
                        &mut self.expected,
                    ) {
                        Ok(false) => return,
                        Ok(true) => {}
                        Err(error) => {
                            self.failed = Some(error);
                            self.finish(app);
                            return;
                        }
                    }
                }
                if matches!(self.action, Some(Action::CollectionJourney)) {
                    match self.collection_journey.as_mut().unwrap().advance(app, ctx) {
                        Ok(false) => return,
                        Ok(true) => {}
                        Err(error) => {
                            self.failed = Some(error);
                            self.finish(app);
                            return;
                        }
                    }
                }
                if matches!(self.action, Some(Action::BrowserJourney)) {
                    let completed = self
                        .browser_journey
                        .as_mut()
                        .unwrap()
                        .advance(app, ctx, smoke);
                    self.update_journey_expectations();
                    match completed {
                        Ok(false) => return,
                        Ok(true) => {}
                        Err(error) => {
                            self.failed = Some(error);
                            self.finish(app);
                            return;
                        }
                    }
                }
                if matches!(self.action, Some(Action::LifecycleJourney)) {
                    let trial = self.config.capture_directory.parent().unwrap();
                    match self.lifecycle.as_mut().unwrap().advance(
                        app,
                        ctx,
                        smoke,
                        &self.paths,
                        trial,
                        &mut self.expected,
                    ) {
                        Ok(false) => return,
                        Ok(true) => {}
                        Err(error) => {
                            self.failed = Some(error);
                            self.finish(app);
                            return;
                        }
                    }
                }
                if matches!(self.action, Some(Action::TransitionJourney)) {
                    let completed =
                        self.transitions
                            .as_mut()
                            .unwrap()
                            .advance(app, ctx, smoke, &self.paths);
                    self.update_journey_expectations();
                    match completed {
                        Ok(false) => return,
                        Ok(true) => {}
                        Err(error) => {
                            self.failed = Some(error);
                            self.finish(app);
                            return;
                        }
                    }
                }
                if matches!(self.action, Some(Action::BackgroundEdit)) {
                    let destination = self
                        .config
                        .capture_directory
                        .parent()
                        .unwrap()
                        .join("background-output");
                    let completed = self.background_edit.as_mut().unwrap().advance(
                        app,
                        ctx,
                        &self.paths,
                        &destination,
                    );
                    self.update_journey_expectations();
                    match completed {
                        Ok(false) => return,
                        Ok(true) => {}
                        Err(error) => {
                            self.failed = Some(error);
                            self.finish(app);
                            return;
                        }
                    }
                }
                if matches!(self.action, Some(Action::BackgroundWait)) {
                    let progress = app.import.smoke_batch_progress();
                    if !progress.errors.is_empty() {
                        self.failed =
                            Some(format!("background export failed: {:?}", progress.errors));
                        self.finish(app);
                        return;
                    }
                    if progress.running {
                        return;
                    }
                    if progress.completed != 1 {
                        self.failed =
                            Some("background export did not complete one photograph".into());
                        self.finish(app);
                        return;
                    }
                }
                if matches!(self.action, Some(Action::SelectionJourney)) {
                    let completed =
                        self.journey
                            .as_mut()
                            .unwrap()
                            .advance(app, ctx, smoke, &self.paths);
                    self.update_journey_expectations();
                    match completed {
                        Ok(false) => return,
                        Ok(true) => {}
                        Err(error) => {
                            self.failed = Some(error);
                            self.finish(app);
                            return;
                        }
                    }
                }
                if matches!(self.action, Some(Action::Evict)) && self.eviction.is_none() {
                    let after: Vec<_> = app
                        .worker
                        .memory_status()
                        .reduced_sources
                        .into_iter()
                        .map(|(path, _)| path)
                        .collect();
                    let source = self.retained_before.iter().find(|path| {
                        self.paths.contains(path)
                            && app.selected.as_ref() != Some(path)
                            && !after.contains(path)
                    });
                    let Some(source) = source else {
                        return;
                    };
                    self.eviction = Some(serde_json::json!({"evicted_source":source,
                        "retained_before":self.retained_before,"retained_after":after}));
                }
                if matches!(self.action, Some(Action::Rebuild)) && !self.rebuild_edited {
                    if !smoke.direct_events.is_empty() || app.edit_selection.paths() != self.targets
                    {
                        return;
                    }
                    self.minimum_revision = app.revision + 1;
                    self.journey.as_mut().unwrap().model.adjust_exposure(0.0625);
                    apply_exposure(app, 0.0625);
                    self.update_journey_expectations();
                    self.rebuild_edited = true;
                }
                if matches!(self.action, Some(Action::SliderDrag)) && self.drag.is_none() {
                    // Opening option groups and scrolling are navigation setup. Begin
                    // the gesture once its real drawn geometry has stopped moving.
                    if let Some(control) = crate::automation::snapshot().and_then(|snapshot| {
                        snapshot.controls.into_iter().find(|control| {
                            control.pointer.as_deref() == Some(self.config.control_pointer.as_str())
                                && control.visible
                        })
                    }) {
                        let stable =
                            self.drag_layout
                                .as_ref()
                                .map_or(0, |(slider, readout, count)| {
                                    if *slider == control.slider && *readout == control.readout {
                                        count + 1
                                    } else {
                                        0
                                    }
                                });
                        self.drag_layout = Some((control.slider, control.readout, stable));
                        if stable < 2 {
                            return;
                        }
                    }
                    let offscreen = crate::automation::snapshot().is_some_and(|snapshot| {
                        snapshot.controls.iter().any(|control| {
                            control.pointer.as_deref() == Some(self.config.control_pointer.as_str())
                                && !control.visible
                        })
                    });
                    if offscreen && self.drag_reveal_frames < 10 {
                        self.drag_reveal_frames += 1;
                        return;
                    }
                    crate::automation::reveal(None);
                    match super::slider::Drag::start(
                        app,
                        &self.config.control_pointer,
                        self.config.drag_steps,
                        self.config.drag_fraction,
                        self.config.mixed_values,
                        self.config.drag_pattern,
                        &mut smoke.direct_events,
                    ) {
                        Ok(drag) => self.drag = Some(drag),
                        Err(error) => {
                            self.failed = Some(error);
                            self.finish(app);
                        }
                    }
                    return;
                }
                if let Some(drag) = self.drag.as_mut() {
                    drag.observe(app);
                }
                if self.rapid_left > 0 {
                    for _ in 0..self.rapid_left.min(2) {
                        self.edit(app, 0.01);
                        self.rapid_left -= 1;
                    }
                    return;
                }
                if !smoke.direct_events.is_empty() {
                    return;
                }
                if matches!(self.action, Some(Action::SliderDrag)) && !self.drag_checked {
                    self.drag_checked = true;
                    if let Err(error) = self.drag.as_mut().expect("slider gesture started").check()
                    {
                        self.failed = Some(error);
                        self.finish(app);
                        return;
                    }
                    for path in &self.targets {
                        self.expected
                            .insert(path.clone(), self.settings(app, path).clone());
                    }
                }
                let targets_applied = app.edit_selection.paths() == self.targets;
                if !targets_applied {
                    return;
                }
                for path in &self.targets {
                    if !self.expected.contains_key(path) {
                        if let Some(settings) = app.multi_edit.settings(path) {
                            self.expected.insert(path.clone(), settings.clone());
                        }
                    }
                }
                for path in &self.targets {
                    if let Some(frame) = self
                        .frame(app, path)
                        .filter(|frame| self.matches(app, path, frame, self.minimum_revision))
                    {
                        if !self.observed.contains_key(path) {
                            self.marker("current",serde_json::json!({"source":path,"task":frame.diagnostic_task.id,"revision":frame.diagnostic_task.revision,"photo":frame.diagnostic_identity(false).map(|identity|identity.context.photo),"presentation_revision":frame.revision,"scale_divisor":frame.artifact.settings.working_scale.divisor(),"expected_scale_divisor":self.config.quality.expected_scale(&frame.artifact.metadata).map(|scale|scale.divisor())}));
                            self.observed.insert(path.clone(), frame.diagnostic_task.id);
                        }
                    }
                }
                if !self.ready(app, self.minimum_revision) {
                    self.ready_time = None;
                    return;
                }
                if let Some(model) = self.exposure_model() {
                    let observed: Vec<_> = self
                        .paths
                        .iter()
                        .map(|path| self.settings(app, path).exposure)
                        .collect();
                    if let Err(error) = model.check_exposures(&observed) {
                        self.failed = Some(error);
                        self.finish(app);
                        return;
                    }
                }
                // Permit the completed image to reach native command encoding
                // before declaring the step settled. This is not screen scanout.
                let input_time = ctx.input(|input| input.time);
                if self.ready_time.is_none() {
                    self.ready_time = Some(input_time);
                    return;
                }
                if self.ready_time == Some(input_time) {
                    return;
                }
                if matches!(self.action, Some(Action::SliderDrag)) {
                    if let Err(error) = self.drag.as_mut().expect("slider gesture started").check()
                    {
                        self.failed = Some(error);
                        self.finish(app);
                        return;
                    }
                }
                let progress = app.worker.progress.snapshot();
                assert!(
                    progress.is_none_or(|progress| !progress.running()),
                    "scenario progress did not settle with desired renders"
                );
                if matches!(self.action, Some(Action::Switch(_))) {
                    for path in &self.targets {
                        let cached = self.cached[path]
                            .upgrade()
                            .expect("cached selection discarded its image");
                        assert!(
                            Arc::ptr_eq(&cached, &self.frame(app, path).unwrap().texture),
                            "unchanged selection replaced cached pixels"
                        );
                    }
                }
                self.marker(
                    "settled",
                    serde_json::json!({"worker_busy":false,"progress_running":false}),
                );
                let photos = self.targets.iter().map(|path| {
                    let frame = self.frame(app, path).unwrap();
                    serde_json::json!({"source":path,"family":frame.artifact.metadata.processing_family().ok(),
                        "task":frame.diagnostic_task.id,"revision":frame.diagnostic_task.revision,
                        "presentation_revision":frame.revision,"scale_divisor":frame.artifact.settings.working_scale.divisor(),
                        "settings":self.settings(app,path)})
                }).collect::<Vec<_>>();
                self.results.push(serde_json::json!({"step":self.step,"action":self.action.unwrap().name(),"coverage":self.action.unwrap().coverage(),"baseline_elapsed_ms":if self.config.mode==Mode::Off {Some(self.began.elapsed().as_secs_f64()*1000.)}else{None},"photos":photos,"retained":app.worker.memory_status().resources}));
                self.results.last_mut().unwrap()["geometry"] = self.geometry.clone();
                if self.config.name == Case::BackgroundEdit {
                    let batch = app.import.smoke_batch_progress();
                    self.results.last_mut().unwrap()["background"] = serde_json::json!({
                        "running":batch.running,"completed":batch.completed,"total":batch.total,
                        "decisions_saved":batch.decisions_saved,"errors":batch.errors});
                }
                if matches!(
                    self.action,
                    Some(Action::Initial | Action::SelectionJourney)
                ) {
                    for path in &self.paths {
                        self.cached.insert(
                            path.clone(),
                            Arc::downgrade(&self.frame(app, path).unwrap().texture),
                        );
                    }
                }
                self.next(app, ctx, smoke);
            }
            Phase::Saving => {
                if self.failed.is_some()
                    && (self.config.name == Case::SliderDrag
                        || self.config.name.manual_exposure_journey())
                    && !self.screenshot_finished
                {
                    if self.failure_screenshot.is_none() {
                        self.failure_screenshot = Some(Instant::now());
                        ctx.send_viewport_cmd(
                            egui::ViewportCommand::Screenshot(Default::default()),
                        );
                        return;
                    }
                    let screenshot = ctx.input(|input| {
                        input.events.iter().find_map(|event| {
                            if let egui::Event::Screenshot { image, .. } = event {
                                Some(image.clone())
                            } else {
                                None
                            }
                        })
                    });
                    if let Some(image) = screenshot {
                        std::fs::create_dir_all(&smoke.directory).unwrap();
                        let bytes: Vec<_> = image
                            .pixels
                            .iter()
                            .flat_map(|pixel| pixel.to_array())
                            .collect();
                        let _ = image::save_buffer(
                            smoke.directory.join("slider-failure.png"),
                            &bytes,
                            image.width() as u32,
                            image.height() as u32,
                            image::ColorType::Rgba8,
                        );
                        self.screenshot_finished = true;
                    } else if self
                        .failure_screenshot
                        .is_some_and(|began| began.elapsed() > Duration::from_secs(3))
                    {
                        self.screenshot_finished = true;
                    } else {
                        return;
                    }
                }
                if self.config.save_on_exit
                    || self.config.mode == Mode::Off
                    || app.diagnostics.last_saved().is_some()
                    || app.diagnostics.error().is_some()
                    || (!app.diagnostics.active()
                        && !app.diagnostics.starting()
                        && !app.diagnostics.saving()
                        && self.failed.is_some())
                {
                    if self.config.capture_images && self.failed.is_none() {
                        match self.capture_images(app, &smoke.directory) {
                            Ok(false) => return,
                            Ok(true) => {}
                            Err(error) => {
                                self.failed = Some(error);
                                return;
                            }
                        }
                    }
                    let captures: Vec<_> = self
                        .image_captures
                        .as_deref()
                        .unwrap_or_default()
                        .iter()
                        .filter(|capture| capture.finished)
                        .map(|capture| capture.evidence.clone())
                        .collect();
                    let result = serde_json::json!({
                        "schema_version":1,
                        "input_kind":if self.config.mode == Mode::Off { "baseline" } else { "native-perfetto-scenario" },
                        "scenario":self.manifest["scenario"],
                        "hardware":app.diagnostic_hardware(),
                        "disk_cache":app.preferences.decode_cache,
                        "memory_cache_bytes":app.preferences.memory_cache_bytes,
                        "passed":self.failed.is_none(),
                        "failure":self.failed,
                        "pending":self.failed.as_ref().map(|_|serde_json::json!({
                            "application":app.diagnostic_state(),
                            "action":self.action.map(Action::name),
                            "minimum_revision":self.minimum_revision,
                            "targets":self.targets.iter().map(|path|serde_json::json!({
                                "source":path,"expected_final":self.expected.get(path),
                                "editor_requested":app.multi_edit.settings(path),
                                "sidecar_requested":app.sidecars.requested_settings(path),
                                "frame":self.frame(app,path).map(|frame|serde_json::json!({
                                    "task":frame.diagnostic_task.id,"revision":frame.diagnostic_task.revision,
                                    "presentation_revision":frame.revision,"requested":frame.requested,
                                    "scale_divisor":frame.artifact.settings.working_scale.divisor(),
                                    "admitted":app.worker.is_settled_and_admitted(frame),
                                    "matches_expected_final":self.matches(app,path,frame,self.minimum_revision),
                                })),
                            })).collect::<Vec<_>>(),
                        })),
                        "steps":self.results,
                        "image_captures":captures,
                        "geometry":self.geometry,
                        "ui_samples":self.ui_samples,
                        "journey":self.journey.as_ref().map(|journey|journey.evidence()),
                        "transitions":self.transitions.as_ref().map(|journey|journey.evidence()),
                        "choices":self.choices.as_ref().map(|journey|journey.evidence()),
                        "source_loading":self.source_loading.as_ref().map(|journey|journey.evidence()),
                        "browser_journey":self.browser_journey.as_ref().map(|journey|journey.evidence()),
                        "collection_journey":self.collection_journey.as_ref().map(|journey|journey.evidence()),
                        "lifecycle":self.lifecycle.as_ref().map(|journey| {
                            let mut evidence = journey.evidence();
                            evidence["expected"] = serde_json::json!({"exposures":self.paths.iter()
                                .map(|path|self.expected[path].exposure).collect::<Vec<_>>()});
                            evidence
                        }),
                        "background":self.background_edit.as_ref().map(|background|background.evidence()),
                        "shutdown_pending":self.shutdown_pending,
                        "eviction":self.eviction,
                        "numerical_equality":"app image captures require separate fresh-render comparison",
                        "coverage":"egui input dispatch and declared direct application commands; physical input and screen presentation not measured",
                    });
                    std::fs::create_dir_all(&smoke.directory).unwrap();
                    if let Some(drag) = &self.drag {
                        std::fs::write(
                            smoke.directory.join("slider-drag.json"),
                            serde_json::to_vec_pretty(drag).unwrap(),
                        )
                        .unwrap();
                    } else if self.config.name == Case::SliderDrag {
                        std::fs::write(
                            smoke.directory.join("slider-observation.json"),
                            serde_json::to_vec_pretty(&crate::automation::snapshot()).unwrap(),
                        )
                        .unwrap();
                    }
                    std::fs::write(
                        smoke.directory.join("scenario-result.json"),
                        serde_json::to_vec_pretty(&result).unwrap(),
                    )
                    .unwrap();
                    self.phase = Phase::Done;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Phase::Done => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }
    fn settings<'a>(&self, app: &'a App, path: &Path) -> &'a Settings {
        app.multi_edit.settings(path).unwrap_or(&app.settings)
    }

    fn observe_ui(&mut self, app: &App, ctx: &egui::Context) {
        let Some(started) = self.ui_started else {
            return;
        };
        let window = ctx.screen_rect().size();
        let viewport = crate::photo_tools::viewport_rect(ctx)
            .map(|rect| rect.size())
            .unwrap_or_default();
        self.geometry = serde_json::json!({"window_points":[window.x,window.y],
            "viewport_points":[viewport.x,viewport.y],"pixels_per_point":ctx.pixels_per_point()});
        let mut sample = serde_json::json!({"elapsed_ms":started.elapsed().as_secs_f64()*1000.,
            "action":self.action.map_or("initial-selection",Action::name),"worker_busy":app.worker.busy()});
        if self.config.name == Case::BackgroundEdit {
            let progress = app.import.smoke_batch_progress();
            sample["background_running"] = (progress.running && progress.decisions_saved).into();
        }
        self.ui_samples.push(sample);
    }

    fn exposure_model(&self) -> Option<&super::journey::ExposureJourney> {
        self.journey
            .as_ref()
            .map(|journey| &journey.model)
            .or_else(|| self.source_loading.as_ref().map(|journey| &journey.model))
            .or_else(|| self.transitions.as_ref().map(|journey| &journey.model))
            .or_else(|| self.browser_journey.as_ref().map(|journey| &journey.model))
            .or_else(|| {
                self.background_edit
                    .as_ref()
                    .map(|background| &background.model)
            })
    }

    fn update_journey_expectations(&mut self) {
        let model = self
            .journey
            .as_ref()
            .map(|journey| &journey.model)
            .or_else(|| self.source_loading.as_ref().map(|journey| &journey.model))
            .or_else(|| self.transitions.as_ref().map(|journey| &journey.model))
            .or_else(|| self.browser_journey.as_ref().map(|journey| &journey.model))
            .or_else(|| {
                self.background_edit
                    .as_ref()
                    .map(|background| &background.model)
            });
        if let Some((index, crop, rotation)) = self
            .transitions
            .as_ref()
            .and_then(|journey| journey.composition)
        {
            if let Some(settings) = self.expected.get_mut(&self.paths[index]) {
                settings.crop = crop;
                settings.rotation = rotation;
            }
        }
        if let Some(model) = model {
            for (path, exposure) in self.paths.iter().zip(model.exposures()) {
                if let Some(settings) = self.expected.get_mut(path) {
                    settings.exposure = *exposure;
                    settings.exposure_auto = false;
                }
            }
        }
    }

    fn capture_images(&mut self, app: &App, directory: &Path) -> Result<bool, String> {
        if self.image_captures.is_none() {
            let mut captures = Vec::new();
            for (index, path) in self.targets.iter().enumerate() {
                let frame = self
                    .frame(app, path)
                    .ok_or("missing settled frame for capture")?;
                let foveonic::inspection::Content::Image(image) = &frame.artifact.content else {
                    return Err("settled frame does not contain an image".into());
                };
                let description = image.description();
                let view = image.view();
                let destination = directory.join(format!("image-{index:02}.f32"));
                let evidence = serde_json::json!({
                    "source":path,"file":destination,"task":frame.diagnostic_task.id,
                    "revision":frame.diagnostic_task.revision,
                    "settings":foveonic::profile::RenderSettingsSnapshot::from(image.settings()),
                    "extent":description.extent,"layout":format!("{:?}",description.layout),
                    "space":format!("{:?}",description.space),"linear_white":description.linear_white,
                    "view_baked":description.view_baked,"view_extent":view.extent,
                    "view_crop":view.crop,"view_rotation":view.rotation,
                    "endianness":if cfg!(target_endian="little") {"little"} else {"big"},
                });
                captures.push(ImageCapture {
                    evidence,
                    result: app.worker.capture_image((**image).clone(), destination),
                    finished: false,
                });
            }
            self.image_captures = Some(captures);
        }
        let captures = self.image_captures.as_mut().unwrap();
        for capture in captures.iter_mut().filter(|capture| !capture.finished) {
            match capture.result.try_recv() {
                Ok(Ok(())) => capture.finished = true,
                Ok(Err(error)) => return Err(format!("image capture failed: {error}")),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    return Err("image capture worker stopped".into());
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
        }
        Ok(captures.iter().all(|capture| capture.finished))
    }
    fn frame<'a>(
        &self,
        app: &'a App,
        path: &PathBuf,
    ) -> Option<&'a crate::rendering::worker::Frame> {
        app.frame
            .as_ref()
            .filter(|frame| frame.path == *path)
            .or_else(|| app.multi_edit_frame(path))
    }
    fn matches(
        &self,
        app: &App,
        path: &PathBuf,
        frame: &crate::rendering::worker::Frame,
        minimum: u64,
    ) -> bool {
        frame.rendition == crate::editor::Rendition::Current
            && frame.diagnostic_task.revision >= minimum
            && Some(frame.artifact.settings.working_scale)
                == self.config.quality.expected_scale(&frame.artifact.metadata)
            && crate::rendering::matching::same_authored_edits(
                &frame.requested,
                self.expected
                    .get(path)
                    .unwrap_or_else(|| self.settings(app, path)),
            )
    }
    fn ready(&self, app: &App, minimum: u64) -> bool {
        !app.worker.busy()
            && app.sidecars.ready()
            && (self.targets.len() == 1 || app.multi_edit.editable_ready())
            && self.targets.iter().all(|path| {
                self.frame(app, path)
                    .is_some_and(|frame| self.matches(app, path, frame, minimum))
            })
    }
    fn edit(&mut self, app: &mut App, delta: f64) {
        let changed = app.edit_selection.paths().to_vec();
        apply_exposure(app, delta);
        for path in changed {
            self.expected
                .insert(path.clone(), self.settings(app, &path).clone());
        }
    }
    fn next(&mut self, app: &mut App, ctx: &egui::Context, smoke: &mut Smoke) {
        let Some(action) = self.actions.pop_front() else {
            if self.config.exit_with_pending && self.failed.is_none() {
                if let Err(error) = self.queue_shutdown_edit(app) {
                    self.failed = Some(error);
                }
                self.finish(app);
                self.tick(app, ctx, smoke);
                return;
            }
            self.finish(app);
            return;
        };
        self.action = Some(action);
        self.step += 1;
        self.observed.clear();
        self.ready_time = None;
        self.began = Instant::now();
        self.minimum_revision = 0;
        self.ui_started.get_or_insert_with(Instant::now);
        self.observe_ui(app, ctx);
        match action {
            Action::Initial => self.targets = self.paths.clone(),
            Action::SourceLoading => self.targets = self.paths[1..2].to_vec(),
            Action::Switch(index) => self.targets = vec![self.paths[index].clone()],
            Action::Remove => self.targets = vec![self.paths[0].clone()],
            Action::Readd => self.targets = self.paths[..2].to_vec(),
            Action::Restore => self.targets = self.paths.clone(),
            Action::Rebuild => {
                let source: PathBuf = serde_json::from_value(
                    self.eviction.as_ref().unwrap()["evicted_source"].clone(),
                )
                .unwrap();
                self.targets = vec![source];
            }
            _ => {}
        }
        if let Some(journey) = &mut self.journey {
            if !matches!(action, Action::SelectionJourney) {
                journey.model.select(
                    self.targets
                        .iter()
                        .map(|path| {
                            self.paths
                                .iter()
                                .position(|candidate| candidate == path)
                                .unwrap()
                        })
                        .collect(),
                );
            }
        }
        let mut expectation = serde_json::json!({});
        let may_rebuild = matches!(action, Action::Rebuild | Action::TransitionJourney)
            || (self.config.name == Case::SelectionPressure && matches!(action, Action::Restore))
            || matches!(
                action,
                Action::BackgroundEdit
                    | Action::BackgroundWait
                    | Action::TransitionJourney
                    | Action::LifecycleJourney
                    | Action::BrowserJourney
                    | Action::CollectionJourney
                    | Action::ChoiceJourney
            );
        let target_reuse = self.config.name.selection_journey()
            && (matches!(action, Action::Switch(_))
                || (matches!(action, Action::Restore) && !may_rebuild));
        if !matches!(action, Action::Initial)
            && !may_rebuild
            && !target_reuse
            && (self.config.name != Case::MemoryPressure || self.config.disk_cache_bytes > 0)
        {
            expectation["expect_original_reads"] = 0.into();
        }
        if !matches!(action, Action::Initial)
            && !may_rebuild
            && !target_reuse
            && (self.config.name != Case::MemoryPressure || self.config.disk_cache_bytes == 0)
        {
            expectation["expect_local_reads"] = 0.into();
        }
        if matches!(action, Action::Initial) && self.config.name != Case::Initial {
            let count = self.paths.len() as u64;
            let disk = self.config.disk_cache_bytes > 0;
            expectation["expect_original_reads"] = (if disk { 0 } else { count }).into();
            expectation["expect_local_reads"] = (if self.config.name == Case::PrefetchedSelection {
                0
            } else if disk {
                count
            } else {
                0
            })
            .into();
        }
        if matches!(action, Action::SourceLoading) {
            // A deliberately reacquires native input. B's edit must complete
            // before that held read is released.
            expectation["expect_original_reads"] =
                u64::from(self.config.disk_cache_bytes == 0).into();
            expectation["expect_local_reads"] = u64::from(self.config.disk_cache_bytes > 0).into();
        }
        if matches!(action, Action::Switch(_)) {
            if !target_reuse {
                expectation["expect_photographic_dispatches"] = 0.into();
            }
            expectation["expected_work"] = "reuse".into();
        }
        if matches!(action, Action::Restore) && !may_rebuild {
            expectation["expected_work"] = "reuse".into();
        }
        if matches!(action, Action::Rebuild) {
            expectation
                .as_object_mut()
                .unwrap()
                .extend(self.eviction.as_ref().unwrap().as_object().unwrap().clone());
            expectation["expected_work"] = "rebuild".into();
        }
        self.marker("begin", expectation);
        match action {
            Action::Initial => {
                self.minimum_revision = app.revision + 1;
                if self.config.name != Case::PrefetchedSelection {
                    app.worker.clear_all();
                }
                app.multi_edit.invalidate();
                app.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::SelectAll,
                    egui::Modifiers::COMMAND,
                )]);
                if self.paths.len() == 1 {
                    app.submit(None);
                }
            }
            Action::Edit(delta) => {
                self.minimum_revision = app.revision + 1;
                self.edit(app, delta);
            }
            Action::Switch(index) => self.click(app, ctx, smoke, index, egui::Modifiers::NONE),
            Action::Remove => {
                self.edit(app, 0.125);
                self.click(app, ctx, smoke, 1, egui::Modifiers::ALT);
            }
            Action::Readd => self.click(app, ctx, smoke, 1, egui::Modifiers::ALT),
            Action::Rapid => {
                self.minimum_revision = app.revision + 1;
                self.rapid_left = self.config.rapid_edits;
            }
            Action::Pressure => {
                app.preferences.memory_cache_bytes = self.config.pressure_bytes;
            }
            Action::SliderDrag => {
                crate::automation::reveal(Some(&self.config.control_pointer));
            }
            Action::SelectionJourney | Action::TransitionJourney | Action::ChoiceJourney => {}
            Action::Restore => {
                self.click(app, ctx, smoke, 0, egui::Modifiers::NONE);
                self.click(
                    app,
                    ctx,
                    smoke,
                    self.paths.len() - 1,
                    egui::Modifiers::SHIFT,
                );
            }
            Action::Evict => {
                self.retained_before = app
                    .worker
                    .memory_status()
                    .reduced_sources
                    .into_iter()
                    .map(|(path, _)| path)
                    .collect();
                app.preferences.memory_cache_bytes = self.config.pressure_bytes;
            }
            Action::Rebuild => {
                let index = self
                    .paths
                    .iter()
                    .position(|path| path == &self.targets[0])
                    .unwrap();
                self.click(app, ctx, smoke, index, egui::Modifiers::NONE);
            }
            Action::BackgroundEdit
            | Action::BackgroundWait
            | Action::LifecycleJourney
            | Action::BrowserJourney
            | Action::CollectionJourney => {}
            Action::SourceLoading => {}
        }
        self.phase = Phase::Running;
    }
    fn click(
        &self,
        app: &App,
        ctx: &egui::Context,
        smoke: &mut Smoke,
        index: usize,
        modifiers: egui::Modifiers,
    ) {
        app.smoke_multi_click(ctx, smoke, index, modifiers);
    }
    fn queue_shutdown_edit(&mut self, app: &mut App) -> Result<(), String> {
        if app.browser.import_mode || app.edit_selection.paths() != self.paths {
            return Err("shutdown edit requires the complete Browse selection".into());
        }
        let mut photos = Vec::new();
        for (path, initial) in self.paths.iter().zip(&self.shutdown_exposures) {
            let settings = self.settings(app, path);
            if settings.exposure_auto || settings.exposure != *initial {
                return Err(format!(
                    "shutdown starting exposure differs for {}",
                    path.display()
                ));
            }
            let frame = self
                .frame(app, path)
                .ok_or("shutdown requires an existing frame")?;
            photos.push(serde_json::json!({"source":path,"initial_exposure":initial,
                "expected_exposure":*initial + 0.25_f32,"old_presented_revision":frame.revision,
                "old_task_revision":frame.diagnostic_task.revision}));
        }
        let before = app.revision;
        apply_exposure(app, 0.25);
        let after = app.revision;
        if after <= before {
            return Err("shutdown edit did not advance the requested revision".into());
        }
        for (path, initial) in self.paths.iter().zip(&self.shutdown_exposures) {
            if self.settings(app, path).exposure != *initial + 0.25_f32 {
                return Err(format!(
                    "shutdown requested edit differs for {}",
                    path.display()
                ));
            }
        }
        let evidence = serde_json::json!({"delta":0.25,"requested_revision_before":before,
            "requested_revision_after":after,"photos":photos});
        crate::timing::scenario_step(
            "scenario.shutdown_pending",
            "direct-command",
            evidence.clone(),
        );
        self.shutdown_pending = Some(evidence);
        Ok(())
    }

    fn finish(&mut self, app: &mut App) {
        if let Some(journey) = &mut self.source_loading {
            journey.release();
        }
        if let Some(journey) = &mut self.transitions {
            journey.release();
        }
        if let Some(journey) = &mut self.journey {
            journey.release();
        }
        if let Some(error) = &self.failed {
            self.marker(
                "failed",
                serde_json::json!({"error":error,
                "expected_selection":self.targets,"actual_selection":app.edit_selection.paths(),
                "worker_busy":app.worker.busy()}),
            );
        }
        if !self.config.save_on_exit && self.config.mode != Mode::Off && app.diagnostics.active() {
            app.diagnostics.set_end_state(app.diagnostic_state());
            app.diagnostics
                .stop_to(self.config.capture_directory.clone())
                .expect("cannot save scenario capture");
        }
        self.phase = Phase::Saving;
    }
}

pub(super) fn apply_exposure(app: &mut App, delta: f64) {
    let edits = [Edit::Number {
        pointer: "/exposure".into(),
        change: NumberChange::Relative(delta),
        range: [-4., 4.],
        mapping: Mapping::Identity,
    }];
    if app.multi_edit.multiple() {
        if !app.multi_edit.apply_edits(&edits, &mut app.sidecars) {
            return;
        }
        app.settings = app
            .multi_edit
            .settings(app.selected.as_ref().unwrap())
            .unwrap()
            .clone();
    } else {
        if !multi_edit_controls::apply(&mut app.settings, &edits) {
            return;
        }
        app.sidecars.edited(&app.settings);
        if let Some(path) = &app.selected {
            app.multi_edit
                .observe_active(path, &app.settings, app.metadata.as_ref());
        }
    }
    app.submit(None);
}
