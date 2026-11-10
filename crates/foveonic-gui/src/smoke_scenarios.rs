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
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Quality {
    Full,
    Fast,
    Draft,
    Thumb,
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
        }
    }
    fn coverage(self) -> &'static str {
        match self {
            Self::Switch(_) | Self::Remove | Self::Readd => "input-dispatch",
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
}
impl Scenario {
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
        assert!(config.photo_count==1 || matches!(config.quality,Quality::Thumb),"native multi-photo overviews support Thumb; use the headless runner for multi-photo Draft");
        assert!(
            config.name != Case::CachedSwitch || config.photo_count >= 2,
            "cached switching needs at least two photographs"
        );
        assert!(
            config.repeats > 0 && config.rapid_edits > 0 && config.timeout_seconds > 0,
            "scenario repetition and timeout values must be positive"
        );
        Some(Self {
            paths: config.sources.clone(),
            config,
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
                app.open(self.config.directory.clone(), FolderMode::Import);
                self.phase = Phase::Folder;
            }
            Phase::Folder => {
                if app.folder_loader.loading()
                    || !app.browser.import_mode
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
                self.results.push(serde_json::json!({"step":self.step,"action":self.action.unwrap().name(),"coverage":self.action.unwrap().coverage(),"baseline_elapsed_ms":if self.config.mode==Mode::Off {Some(self.began.elapsed().as_secs_f64()*1000.)}else{None},"photos":self.targets.iter().map(|path|serde_json::json!({"source":path,"task":self.frame(app,path).unwrap().diagnostic_task.id,"revision":self.frame(app,path).unwrap().diagnostic_task.revision,"presentation_revision":self.frame(app,path).unwrap().revision,"scale_divisor":self.frame(app,path).unwrap().artifact.settings.working_scale.divisor(),"settings":self.settings(app,path)})).collect::<Vec<_>>(),"retained":app.worker.memory_status().resources}));
                if matches!(self.action, Some(Action::Initial)) {
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
                if self.config.mode == Mode::Off
                    || app.diagnostics.last_saved().is_some()
                    || app.diagnostics.error().is_some()
                    || (!app.diagnostics.active()
                        && !app.diagnostics.starting()
                        && !app.diagnostics.saving()
                        && self.failed.is_some())
                {
                    let result = serde_json::json!({"schema_version":1,"input_kind":if self.config.mode==Mode::Off {"baseline"}else{"native-perfetto-scenario"},"scenario":self.manifest["scenario"],"hardware":app.diagnostic_hardware(),"disk_cache":app.preferences.decode_cache,"memory_cache_bytes":app.preferences.memory_cache_bytes,"passed":self.failed.is_none(),"failure":self.failed,"steps":self.results,"numerical_equality":"not measured in native scenario; associate separate headless cached-versus-fresh verification","coverage":"egui input dispatch and declared direct application commands; physical input and screen presentation not measured"});
                    std::fs::create_dir_all(&smoke.directory).unwrap();
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
        let edits = [Edit::Number {
            pointer: "/exposure".into(),
            change: NumberChange::Relative(delta),
            range: [-4., 4.],
            mapping: Mapping::Identity,
        }];
        if app.multi_edit.multiple() {
            assert!(app.multi_edit.apply_edits(&edits, &mut app.sidecars));
            app.settings = app
                .multi_edit
                .settings(app.selected.as_ref().unwrap())
                .unwrap()
                .clone();
        } else {
            assert!(multi_edit_controls::apply(&mut app.settings, &edits));
            app.sidecars.edited(&app.settings);
            if let Some(path) = &app.selected {
                app.multi_edit
                    .observe_active(path, &app.settings, app.metadata.as_ref());
            }
        }
        for path in &changed {
            self.expected
                .insert(path.clone(), self.settings(app, path).clone());
        }
        if !app.submit_multi_edit() {
            app.submit(None);
        }
    }
    fn next(&mut self, app: &mut App, ctx: &egui::Context, smoke: &mut Smoke) {
        let Some(action) = self.actions.pop_front() else {
            self.finish(app);
            return;
        };
        self.action = Some(action);
        self.step += 1;
        self.observed.clear();
        self.ready_time = None;
        self.began = Instant::now();
        self.minimum_revision = 0;
        match action {
            Action::Initial => self.targets = self.paths.clone(),
            Action::Switch(index) => self.targets = vec![self.paths[index].clone()],
            Action::Remove => self.targets = vec![self.paths[0].clone()],
            Action::Readd => self.targets = self.paths[..2].to_vec(),
            _ => {}
        }
        let mut expectation = serde_json::json!({});
        if !matches!(action, Action::Initial)
            && (self.config.name != Case::MemoryPressure || self.config.disk_cache_bytes > 0)
        {
            expectation["expect_original_reads"] = 0.into();
        }
        if !matches!(action, Action::Initial)
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
        if matches!(action, Action::Switch(_)) {
            expectation["expect_photographic_dispatches"] = 0.into();
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
    fn finish(&mut self, app: &mut App) {
        if let Some(error) = &self.failed {
            self.marker(
                "failed",
                serde_json::json!({"error":error,
                "expected_selection":self.targets,"actual_selection":app.edit_selection.paths(),
                "worker_busy":app.worker.busy()}),
            );
        }
        if self.config.mode != Mode::Off && app.diagnostics.active() {
            app.diagnostics.set_end_state(app.diagnostic_state());
            app.diagnostics
                .stop_to(self.config.capture_directory.clone())
                .expect("cannot save scenario capture");
        }
        self.phase = Phase::Saving;
    }
}
