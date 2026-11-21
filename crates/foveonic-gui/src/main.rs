//! Runs a three-pane photographic workspace with current-buffer inspection, separate UI preferences and per-X3F editing recipes.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use crate::rendering::request::RequestId;
use foveonic::priority::Priority;
use foveonic::render::ProcessingQuality;
mod actions;
mod activity;
mod application_settings;
mod automation;
mod batch;
mod browser;
mod capture;
mod chromaticity;
mod comparison_view;
mod controls;
mod crop_tool;
mod diagnostics;
mod display;
mod display_image;
mod editor;
mod export;
mod folder_loader;
mod framing;
mod help;
mod help_examples;
mod icon;
mod import_review;
mod information;
mod inspect;
mod loupe_tool;
mod loupe_view;
mod memory;
mod metadata;
mod mixer;
mod multi_edit_controls;
mod multi_edit_runtime;
mod navigator;
mod noise_calibration;
mod photo_tools;
mod photo_views;
mod preferences;
mod preview_image;
mod previews;
mod processing;
mod progress;
mod rendering;
mod rotate_tool;
mod scopes;
mod sections;
mod shortcuts;
mod sidecars;
mod smoke;
mod startup;
mod support;
mod timing;
mod toolbar;
mod viewport;
mod viewport_selection;
mod widgets;

use eframe::{egui, egui_wgpu, wgpu};
use foveonic::{
    render::{Engine, Settings},
    source::Metadata,
};
use preferences::{FolderMode, Section};
use rendering::{request, smart, worker};
use std::{path::PathBuf, sync::Arc};

// Keep folder readiness responsive during prefetch without polling idle folders at frame rate.
const CACHE_ACTIVE_POLL: std::time::Duration = std::time::Duration::from_millis(150);
const CACHE_IDLE_POLL: std::time::Duration = std::time::Duration::from_secs(1);
const TOOLS_MIN_WIDTH: f32 = 300.;
const TOOLS_DEFAULT_WIDTH: f32 = 330.;

/// Opening starts at selection, including recipe reads before worker submission.
struct Opening {
    _priority: foveonic::priority::Demand,
    began: std::time::Instant,
}

struct App {
    context: egui::Context,
    diagnostics: diagnostics::Controller,
    noise_calibration: noise_calibration::State,
    comparison_selection: Option<viewport_selection::Request>,
    edit_selection: viewport_selection::Selection,
    multi_edit: multi_edit_runtime::Runtime,
    loupe: loupe_view::Loupe,
    comparison_layout: comparison_view::State,
    browser_selection: Arc<[PathBuf]>,
    viewport_selection: Vec<PathBuf>,
    /// Each viewport photo's view as last planned.
    views: std::collections::HashMap<PathBuf, rendering::views::PhotoView>,
    browser_selection_key: Option<(u64, Option<PathBuf>)>,
    browser_selection_limit: usize,
    editor_ui: editor::State,
    before: editor::Before,
    actions: Vec<actions::Edit>,
    import: import_review::Review,
    settings_open: bool,
    processing_open: bool,
    opening: Option<Opening>,
    browser: browser::Browser,
    folder_loader: folder_loader::Loader,
    worker: worker::Worker,
    capture: capture::Loader,
    sidecars: sidecars::Session,
    metadata: Option<Arc<Metadata>>,
    selected: Option<PathBuf>,
    settings: Settings,
    /// The photo whose loaded settings `settings` holds. Until the selected
    /// photo's settings load, `settings` is a placeholder that nothing saves.
    settings_photo: Option<PathBuf>,
    output: String,
    channel: i32,
    comparison: inspect::Comparison,
    /// Every rendered image the app holds, keyed by photo, look and output.
    images: rendering::rendered::RenderedImages<worker::RenderResult>,
    preview_densities: std::collections::BTreeMap<PathBuf, f32>,
    fit_densities: std::collections::BTreeMap<PathBuf, f32>,
    /// Zoomed photos' moving regions. Planning reads views, so motion uses interior mutability.
    view_motion:
        std::cell::RefCell<std::collections::BTreeMap<PathBuf, rendering::view_plan::Motion>>,
    scope_images: Option<scopes::Images>,
    scope_snapshot: Option<worker::ScopeSnapshot>,
    scope_key: Option<worker::ScopeKey>,
    viewports: photo_views::PhotoViews,
    navigator: navigator::Navigator,
    previews: previews::Previews,
    /// The last request id issued.
    last_request_id: RequestId,
    busy: bool,
    memory_key: Vec<u64>,
    memory_reply_epoch: u64,
    preferences: preferences::Preferences,
    support_preferences: Option<preferences::Preferences>,
    support_photo: Option<(Option<PathBuf>, RequestId)>,
    support_context_due: Option<std::time::Instant>,
    support_log_open: bool,
    activity: activity::Log,
    srgb_target: bool,
    display_target: display::Target,
    render_state: egui_wgpu::RenderState,
    smoke: Option<smoke::Smoke>,
    last_frame: Option<std::time::Instant>,
    startup_ready: Option<std::time::Instant>,
}
impl Drop for App {
    fn drop(&mut self) {
        if std::thread::panicking() {
            if let Some(directory) = std::env::var_os("FOVEONIC_GUI_SMOKE_DIR") {
                self.diagnostics
                    .save_smoke_failure(PathBuf::from(directory).join("capture"));
            }
        } else if self.diagnostics.save_on_exit_enabled() {
            let state = self.diagnostic_state();
            self.diagnostics.set_end_state(state);
            if let Err(error) = self.diagnostics.save_on_exit() {
                eprintln!("Cannot save diagnostic capture on exit: {error}");
            }
        }
    }
}
impl App {
    fn diagnostic_hardware(&self) -> serde_json::Value {
        let adapter = self.render_state.adapter.get_info();
        serde_json::json!({"adapter":adapter.name,"vendor":adapter.vendor,"device":adapter.device,
            "device_type":format!("{:?}",adapter.device_type),"backend":format!("{:?}",adapter.backend),
            "gpu_timestamps_available":timing::gpu_available(),"os":std::env::consts::OS,"arch":std::env::consts::ARCH})
    }
    fn diagnostic_state(&self) -> serde_json::Value {
        let mut photos = self.multi_edit.retained_paths();
        if let Some(path) = &self.selected {
            if !photos.contains(path) {
                photos.push(path.clone());
            }
        }
        let photos: Vec<_> = photos
            .iter()
            .map(|path| {
                let settings = if self.selected.as_ref() == Some(path) {
                    Some(&self.settings)
                } else {
                    self.sidecars
                        .requested_settings(path)
                        .or_else(|| self.multi_edit.settings(path))
                };
                serde_json::json!({"source":path.to_string_lossy(),
                    "photo":timing::context_for(0,0,path,"snapshot").photo,
                    "settings":settings})
            })
            .collect();
        serde_json::json!({"workspace_mode":if self.browser.import_mode {"import"} else {"browse"},
            "active_photo":self.selected.as_deref().map(|path|path.to_string_lossy()),"targets":self.edit_selection.paths().iter().map(|path|path.to_string_lossy()).collect::<Vec<_>>(),"photos":photos,
            "settings":self.settings,"processing_quality":self.preferences.processing_quality,
            "displayed":self.active_image().map(|frame|serde_json::json!({"source":frame.request.path.to_string_lossy(),
                "task":frame.diagnostic_task.id,"revision":frame.diagnostic_task.revision,
                "presentation_revision":frame.request.id,"artifact":frame.artifact.name,
                "scale":frame.artifact.settings.working_scale.divisor(),"purpose":frame.diagnostic_task.purpose})),
            "output":self.output,"rendition":format!("{:?}",self.before.active),
            "render_pending":self.worker.busy(),"opening":self.opening.is_some(),
            "disk_cache":self.preferences.decode_cache,
            "background_full_image_completion":self.preferences.background_full_image_completion,"memory_cache_bytes":self.preferences.memory_cache_bytes,"multi_edit_limit":self.preferences.multi_edit_limit,
            "render_concurrency":"automatic",
            "hardware":self.diagnostic_hardware()})
    }
    pub(crate) fn viewport(&self) -> &viewport::Viewport {
        self.viewports.view(self.selected.as_deref())
    }
    pub(crate) fn viewport_mut(&mut self) -> &mut viewport::Viewport {
        self.viewports.view_mut(self.selected.as_deref())
    }

    fn multi_photo_overview(&self) -> bool {
        multi_photo_overview(&self.browser_selection, self.selected.as_deref())
    }

    fn sync_browser_selection(&mut self) {
        let key = (self.browser.selection_generation(), self.selected.clone());
        let changed = self.browser_selection_key.as_ref() != Some(&key);
        if changed {
            self.browser_selection = self
                .browser
                .processing_selection(self.selected.as_deref())
                .into();
            self.browser_selection_key = Some(key);
            self.preview_densities
                .retain(|path, _| self.browser_selection.contains(path));
            self.fit_densities
                .retain(|path, _| self.browser_selection.contains(path));
        }
        if changed || self.browser_selection_limit != self.preferences.multi_edit_limit {
            self.edit_selection
                .sync_browser(&self.browser_selection, self.preferences.multi_edit_limit);
            self.browser_selection_limit = self.preferences.multi_edit_limit;
        }
    }
    fn prepare(
        input: startup::Inputs,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<Self, String> {
        let startup::Inputs {
            context,
            render_state: rs,
            mut preferences,
            target,
            path,
        } = input;
        let _context = foveonic::diagnostics::enter(foveonic::diagnostics::Context {
            purpose: "startup",
            ..Default::default()
        });
        let _prepare = foveonic::diagnostics::scope(
            foveonic::diagnostics::Category::Cpu,
            "startup.prepare_app",
        );
        let mut startup_profile = foveonic::profile::Profile::new(timing::mode());
        viewport::install(&rs);
        let engine = Engine::from_device_with_progress(
            rs.device.clone(),
            rs.queue.clone(),
            rs.adapter.get_info().name,
            &mut startup_profile,
            &mut progress,
        )
        .map_err(|error| {
            foveonic::support::report_error("startup.engine", error.as_ref(), Default::default());
            error.to_string()
        })?;
        let engine = if let Some(directory) = std::env::var_os("FOVEONIC_CALIBRATION_RESOURCES") {
            engine
                .with_calibration_resources(directory)
                .map_err(|error| {
                    foveonic::support::report_error(
                        "startup.calibration_resources",
                        error.as_ref(),
                        Default::default(),
                    );
                    error.to_string()
                })?
        } else {
            engine
        };
        let smoke = smoke::Smoke::from_env();
        if let Some(smoke) = &smoke {
            smoke.configure_cache(&mut preferences.decode_cache);
        }
        editor::direct_color::configure(&context, preferences.direct_color);
        if std::env::var_os("FOVEONIC_GUI_SMOKE_SMART").is_some() {
            preferences.processing_quality = ProcessingQuality::Quality;
        }
        viewport::set_gamut(&rs, preferences.display_gamut);
        let training_engine = engine.executor();
        let batch_engine = engine.executor();
        let worker = worker::Worker::new(engine, context.clone());
        let navigator =
            navigator::Navigator::new(rs.clone(), context.clone(), worker.cache.clone());
        let import =
            import_review::Review::new(batch_engine, worker.cache.clone(), context.clone());
        let previews = previews::Previews::new(rs.clone(), context.clone(), worker.cache.clone());
        let browser =
            browser::Browser::with_cache(context.clone(), Some(worker.cache.clone()), rs.clone());
        let noise_calibration = noise_calibration::State::new(
            training_engine,
            worker.cache.clone(),
            context.clone(),
            &mut preferences.noise_profile,
        );
        worker.cache.configure_noise_calibration(
            noise_calibration
                .selected(preferences.noise_profile)
                .as_ref()
                .map(|p| p.signature().to_owned()),
        );
        let mut app = Self {
            context: context.clone(),
            diagnostics: diagnostics::Controller::default(),
            noise_calibration,
            comparison_selection: None,
            edit_selection: Default::default(),
            multi_edit: Default::default(),
            loupe: Default::default(),
            comparison_layout: Default::default(),
            browser_selection: Arc::default(),
            viewport_selection: Vec::new(),
            views: Default::default(),
            browser_selection_key: None,
            browser_selection_limit: 0,
            editor_ui: Default::default(),
            before: Default::default(),
            actions: Vec::new(),
            import,
            settings_open: false,
            processing_open: false,
            opening: None,
            browser,
            folder_loader: folder_loader::Loader::new(context.clone()),
            previews,
            capture: capture::Loader::new(context.clone(), worker.cache.clone()),
            sidecars: sidecars::Session::new(
                context.clone(),
                std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").is_none()
                    || std::env::var_os("FOVEONIC_GUI_SCENARIO_MANIFEST").is_some(),
                worker.cache.clone(),
            ),
            worker,
            metadata: None,
            selected: None,
            settings: Settings::new("", ""),
            settings_photo: None,
            output: "final".into(),
            channel: -1,
            comparison: Default::default(),
            images: Default::default(),
            preview_densities: Default::default(),
            fit_densities: Default::default(),
            view_motion: Default::default(),
            scope_images: None,
            scope_snapshot: None,
            scope_key: None,
            viewports: Default::default(),
            navigator,
            last_request_id: RequestId::default(),
            busy: false,
            memory_key: Vec::new(),
            memory_reply_epoch: 0,
            preferences,
            support_preferences: None,
            support_photo: None,
            support_context_due: None,
            support_log_open: false,
            activity: Default::default(),
            srgb_target: rs.target_format.is_srgb(),
            display_target: target,
            render_state: rs.clone(),
            smoke,
            last_frame: None,
            startup_ready: None,
        };
        app.import.set_noise_calibration(
            app.noise_calibration
                .selected(app.preferences.noise_profile),
        );
        app.worker
            .cache
            .configure(app.preferences.decode_cache.clone());
        app.browser
            .set_thumbnail_budget(app.preferences.thumbnail_cache_bytes());
        app.sidecars.set_defaults(&app.preferences.editor.defaults);
        app.worker
            .cache
            .configure_photo_defaults(&app.preferences.editor.defaults);
        app.import.set_defaults(
            &app.preferences.editor.defaults,
            &mut app.browser,
            app.preferences.processing_destination.as_deref(),
        );
        app.viewport_mut().zoom_snap = app.preferences.zoom_snap;
        app.update_memory();
        if let Some((path, mode)) = app.preferences.startup_request(path) {
            app.folder_loader.request(path, mode);
        }
        Ok(app)
    }
    fn open(&mut self, path: PathBuf, mode: FolderMode) {
        self.folder_loader
            .request(path, folder_loader::Mode::Fixed(mode));
    }
    fn receive_folder(&mut self) {
        let Some(reply) = self.folder_loader.receive() else {
            return;
        };
        let path = reply.path.clone();
        match self.browser.finish_open(reply) {
            Ok((selected, mode, canonical)) => {
                // The old source-write policy stays in force until discovery succeeds.
                let review_ended = self.browser.import_mode;
                self.browser.import_mode = mode == FolderMode::Import;
                self.sidecars.set_memory_only(self.browser.import_mode);
                if review_ended {
                    // Import review's edits were held only in memory. The session
                    // dropped them above, so the reload reads the saved recipes.
                    self.multi_edit.forget_settings(&mut self.sidecars);
                    self.settings_photo = None;
                }
                if let Some(folder) = canonical {
                    self.preferences.last_folder = Some(folder);
                    self.preferences.last_folder_mode = mode;
                }
                if let Some(path) = selected {
                    self.select(path, false);
                } else {
                    self.clear_selection();
                    self.activity.operation("No X3F files in this folder");
                }
                self.import.rescan(
                    &mut self.browser,
                    self.preferences.processing_destination.as_deref(),
                );
            }
            Err(error) => self.fail(format!("Cannot open {}: {error}", path.display())),
        }
    }
    fn clear_selection(&mut self) {
        self.before = Default::default();
        self.browser.invalidate_prefetch();
        self.last_request_id = self.last_request_id.next();
        self.selected = None;
        self.edit_selection
            .sync_browser(&[], self.preferences.multi_edit_limit);
        self.multi_edit.clear();
        self.opening = None;
        self.settings_photo = None;
        self.sidecars.resolve_settings(&[]);
        self.capture.select(None);
        self.metadata = None;
        self.scope_images = None;
        self.scope_snapshot = None;
        self.scope_key = None;
        self.busy = false;
        self.worker.clear_all();
        self.images.clear();
        self.viewport_mut().reset();
        viewport::clear(&self.render_state);
    }
    fn reusable_photograph(
        &self,
        path: &std::path::Path,
        frame: &worker::RenderResult,
        settings: &Settings,
    ) -> bool {
        frame.request.path == path
            && frame.is_photographic()
            && frame.processing_quality() == self.preferences.processing_quality
            && self
                .preferences
                .renders_edits(&frame.request.settings, settings)
            && frame.calibration_signature()
                == self
                    .noise_calibration
                    .selected(self.preferences.noise_profile)
                    .as_ref()
                    .map(|profile| profile.signature())
            && self
                .worker
                .cache
                .source_origin(path)
                .is_none_or(|origin| origin == frame.origin)
            && request::camera_preview_scale(
                self.preview_scale_for(path, self.preferences.processing_quality),
                &frame.artifact.metadata,
            )
            .is_ok_and(|scale| {
                rendering::quality::accepts_scale(scale, frame.artifact.settings.working_scale)
            })
    }

    /// Select a photo. Its view is planned like any other, so existing images for
    /// its look are reused and work for photos that left the view stops by reconciling.
    fn select(&mut self, path: PathBuf, preserve_viewport: bool) {
        let carried = preserve_viewport.then(|| self.viewport().transform());
        if self.selected.as_ref() != Some(&path) || carried.is_some() {
            self.viewports.select(&path, carried);
        }
        self.multi_edit.selected(&path);
        if !self.edit_selection.contains(&path) {
            self.before.active = editor::Rendition::Current;
        }
        self.browser.invalidate_prefetch();
        timing::selected(self.last_request_id.next().0, &path);
        let began = std::time::Instant::now();
        self.opening = Some(Opening {
            _priority: self.worker.cache.priority().demand(Priority::Viewport),
            began,
        });
        self.capture.select(Some(path.clone()));
        self.metadata = self.multi_edit.metadata(&path);
        self.settings = Settings::new("", "");
        self.settings_photo = None;
        self.selected = Some(path);
        self.last_request_id = self.last_request_id.next();
        self.output = "final".into();
        self.channel = -1;
        self.scope_key = None;
        viewport::clear(&self.render_state);
        self.adopt_selected_settings();
    }
    /// The selected photo, once `settings` holds its loaded settings.
    pub(crate) fn loaded_selection(&self) -> Option<&std::path::Path> {
        self.selected
            .as_deref()
            .filter(|path| self.settings_photo.as_deref() == Some(*path))
    }
    /// Whether `settings` holds the selected photo's loaded settings.
    pub(crate) fn selection_loaded(&self) -> bool {
        self.loaded_selection().is_some()
    }
    /// Record the selected photo's edited `settings` in the editing runtime and
    /// save them, once they are its loaded settings.
    #[must_use]
    pub(crate) fn commit_settings(&mut self) -> bool {
        let Some(path) = self.loaded_selection().map(std::path::Path::to_path_buf) else {
            return false;
        };
        self.multi_edit
            .observe_active(&path, &self.settings, self.metadata.as_ref());
        self.sidecars.apply(path, self.settings.clone());
        true
    }
    /// Take the selected photo's settings from the editing runtime once they load.
    fn adopt_selected_settings(&mut self) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        if self.settings_photo.as_ref() == Some(&path) {
            return;
        }
        let Some(settings) = self.multi_edit.settings(&path) else {
            return;
        };
        if let Some(opening) = &self.opening {
            timing::selection_ready(self.last_request_id.0, &path, opening.began);
        }
        self.settings = settings.clone();
        self.settings_photo = Some(path);
        inspect::normalize(
            &self.settings,
            &mut self.output,
            &mut self.channel,
            self.metadata.as_deref(),
            self.capture
                .properties
                .as_ref()
                .and_then(|p| p.get("CAMMODEL"))
                .and_then(|model| foveonic::source::ProcessingFamily::from_camera_model(model)),
        );
    }
    pub(crate) fn navigator_source(&self) -> Option<&worker::RenderResult> {
        self.active_image()
    }
    /// The browser shows each photo's held photographic image. Following what is
    /// held, not what arrives, restores thumbnails after the browser forgets them,
    /// as when a folder is reopened while its images stay current.
    fn sync_browser_thumbnails(&mut self) {
        for frame in self.images.images().filter(|frame| {
            let origin = self.worker.cache.source_origin(&frame.request.path);
            frame.is_photographic()
                && rendering::views::usable(Some(*frame), origin.as_ref()).is_some()
        }) {
            let revision = frame.request.id.0;
            if !self.browser.needs_rendered(&frame.request.path, revision) {
                continue;
            }
            if let Some((thumbnail, preview)) = self.previews.snapshot(frame) {
                self.browser.rendered_texture(
                    frame.request.path.clone(),
                    revision,
                    thumbnail,
                    preview,
                );
            }
        }
    }
    fn sync_busy(&mut self) {
        self.busy = self.view_busy();
    }

    fn receive(&mut self, ctx: &egui::Context) {
        let _scope = foveonic::diagnostics::scope(
            foveonic::diagnostics::Category::Coordinator,
            "ui.receive",
        );
        self.browser.receive(ctx);
        while let Ok(preview) = self.previews.replies.try_recv() {
            if self.previews.accepts(&preview)
                && preview.calibration_signature.as_deref()
                    == self
                        .noise_calibration
                        .selected(self.preferences.noise_profile)
                        .as_ref()
                        .map(|p| p.signature())
            {
                self.browser
                    .rendered(ctx, preview.path, preview.revision, preview.image);
            }
        }
        while let Ok(update) = self.worker.replies.try_recv() {
            let reply = match update {
                worker::Update::Image(reply) => {
                    self.memory_reply_epoch = self.memory_reply_epoch.wrapping_add(1);
                    *reply
                }
                worker::Update::ColorPick { reply, .. } => {
                    if !editor::white_balance::receive(ctx, &reply)
                        && !editor::dehaze::receive(ctx, &reply)
                    {
                        editor::direct_color::receive(ctx, reply);
                    }
                    continue;
                }
                worker::Update::Scopes { key, result, .. } => {
                    if !self.multi_edit.multiple()
                        && self
                            .navigator_source()
                            .into_iter()
                            .chain(self.active_patch())
                            .any(|frame| frame.request.id == key.request_id)
                        && self.scope_key == Some(key)
                    {
                        match result {
                            Ok(data) => {
                                self.scope_images = Some(scopes::Images::new(&data));
                                self.scope_snapshot = Some(worker::ScopeSnapshot { key, data });
                                timing::event(key.request_id.0, "scopes_received");
                            }
                            Err(error) => {
                                if !self.busy {
                                    self.fail(format!("Scope analysis: {error}"));
                                }
                            }
                        }
                    }
                    continue;
                }
            };
            timing::task(
                &reply.stamp,
                "reply_dequeued",
                || serde_json::json!({"mailbox":"image_replies"}),
            );
            self.receive_image(ctx, reply);
        }
    }
    /// Store a result the coordinator accepted as an image in use, unless an
    /// image of the same kind is newer, and apply its side effects.
    fn receive_image(&mut self, ctx: &egui::Context, reply: worker::Reply) {
        let handling = reply
            .handling
            .unwrap_or(rendering::coordinator::ResultUpdate {
                current: false,
                accepted: false,
            });
        self.sync_busy();
        if handling.current && reply.request.rendition == editor::Rendition::Current {
            self.multi_edit.complete(
                &reply.request.path,
                reply.metadata.clone(),
                reply.numeric_settings.clone(),
                reply.reset_settings.clone(),
                &reply.settings,
            );
            if self.selected.as_ref() == Some(&reply.request.path) {
                if let Some(metadata) = &reply.metadata {
                    self.metadata = Some(metadata.clone());
                }
            }
        }
        match reply.result {
            worker::RenderOutcome::Rendered(frame) if handling.accepted => {
                timing::image_received(
                    &reply.stamp,
                    handling.current,
                    frame.queued_at,
                    frame.milliseconds,
                );
                timing::task(&reply.stamp, "image_accepted", || {
                    serde_json::json!({"current":handling.current,
                    "scale":frame.artifact.settings.working_scale.divisor(),"artifact":frame.artifact.name,
                    "extent":frame.texture.description().extent})
                });
                if self.smoke.is_some() {
                    let visible = self.visible_region(&frame.request.path);
                    if let Some(smoke) = &mut self.smoke {
                        smoke.observe_admitted(&frame, handling.current, visible);
                    }
                }
                if handling.current
                    && self.selected.as_ref() == Some(&frame.request.path)
                    && frame.request.rendition == editor::Rendition::Current
                    && frame.artifact.settings.dehaze_estimate
                {
                    editor::dehaze::set_estimate(
                        ctx,
                        &self.settings,
                        frame.artifact.dehaze_estimate,
                    );
                }
                let active = self.selected.as_ref() == Some(&frame.request.path);
                let whole = !frame.request.detail;
                if self.images.accept(*frame) && active {
                    if whole {
                        self.worker.whole_delivered(reply.request.id);
                    }
                    self.scope_key = None;
                    viewport::clear(&self.render_state);
                }
            }
            worker::RenderOutcome::Rendered(_) | worker::RenderOutcome::Cancelled => {}
            worker::RenderOutcome::Failed(error) => {
                assert!(
                    self.smoke
                        .as_ref()
                        .is_none_or(|smoke| smoke.expects_failure(&reply.request.path)),
                    "native smoke processing failed: {error}"
                );
                if handling.accepted {
                    self.images.fail(reply.request.clone());
                }
                if handling.current {
                    self.activity.error_reported(error);
                }
            }
        }
    }
    fn update_scopes(&mut self) {
        if self.multi_edit.multiple() {
            self.scope_key = None;
            return;
        }
        let visible = self
            .selected
            .as_deref()
            .and_then(|path| self.visible_region(path));
        let detail = self.active_patch().filter(|detail| {
            self.preferences.scope_viewport
                && visible.is_some_and(|visible| smart::contains(detail.detail_region(), visible))
        });
        let Some(frame) = detail.or_else(|| self.navigator_source()) else {
            self.scope_key = None;
            return;
        };
        let region = if self.preferences.scope_viewport {
            if frame.request.framing_preview {
                // Drafts only move the texture. Analyze the visible part of the saved framing.
                let foveonic::inspection::Content::Image(image) = &frame.artifact.content else {
                    return;
                };
                let [l, t, r, b] = frame
                    .request
                    .settings
                    .rotation
                    .framing_bounds(frame.camera_extent(), frame.request.settings.crop)
                    .map(|v| v as f32);
                let framing = egui::Rect::from_min_max(egui::pos2(l, t), egui::pos2(r, b));
                let visible = self.viewport().visible_uv().intersect(framing);
                if !visible.is_positive() {
                    return;
                }
                let min = (visible.min - framing.min) / framing.size();
                let max = (visible.max - framing.min) / framing.size();
                let [w, h] = image.view().extent;
                Some([
                    (min.x * w as f32).floor().max(0.) as u32,
                    (min.y * h as f32).floor().max(0.) as u32,
                    ((max.x * w as f32).ceil() as u32).min(w),
                    ((max.y * h as f32).ceil() as u32).min(h),
                ])
            } else if let Some(rect) = frame.artifact.settings.preview_region {
                let visible = visible.unwrap();
                Some([
                    visible[0] - rect[0],
                    visible[1] - rect[1],
                    visible[2] - rect[0],
                    visible[3] - rect[1],
                ])
            } else {
                let Some(pixels) = self
                    .viewport()
                    .visible_pixels_for(frame.texture.description().extent)
                else {
                    return;
                };
                Some(pixels.bounds())
            }
        } else {
            None
        };
        let key = worker::ScopeKey {
            request_id: frame.request.id,
            region,
        };
        if self.scope_key == Some(key) {
            return;
        }
        let foveonic::inspection::Content::Image(image) = &frame.artifact.content else {
            return;
        };
        // Keep the previous plots visible while the worker prepares their replacement.
        self.worker.submit_scopes(worker::ScopeRequest {
            key,
            image: (**image).clone(),
            photo: foveonic::diagnostics::photo_id(&frame.request.path),
            parent_render_task: frame.diagnostic_task.id,
            parent_render_revision: frame.diagnostic_task.revision,
        });
        self.scope_key = Some(key);
    }
    fn process(&mut self) {
        self.import.submit(
            self.preferences.adaptive_tone_tuning,
            &self.browser,
            &self.sidecars,
            if self.browser.import_mode {
                &self.preferences.import_processing
            } else {
                &self.preferences.browse_processing
            },
        );
    }
    fn toolbar(&mut self, ui: &mut egui::Ui) -> bool {
        let _scope =
            foveonic::diagnostics::scope(foveonic::diagnostics::Category::Ui, "ui.toolbar");
        let mut changed = false;
        self.import
            .prepare_browse(&self.browser, self.selected.as_deref());
        toolbar::container(ui, |ui| {
            let pane_left =
                egui::containers::panel::PanelState::load(ui.ctx(), egui::Id::new("settings"))
                    .map_or(
                        ui.ctx().screen_rect().right() - TOOLS_DEFAULT_WIDTH,
                        |state| state.rect.left(),
                    );
            let tools_width = ui.available_rect_before_wrap().right() - pane_left;
            toolbar::split(ui, tools_width, |ui, area| {
                if area == toolbar::Area::Tools {
                    changed |= self.photo_toolbar(ui);
                    return;
                }
                let export_ready = self.selection_loaded()
                    && toolbar::can_export(&self.settings, self.metadata.as_deref());
                let only_current = self
                    .browser
                    .processing_selection(self.selected.as_deref())
                    .as_slice()
                    == self.selected.as_slice();
                let process_blocker = self.import.process_blocker(&self.browser).or_else(|| {
                    (!self.browser.import_mode && only_current && !export_ready).then_some(
                        "Waiting for a valid photo recipe. Check Log for edit or camera-setting errors.",
                    )
                });
                let actions = toolbar::show(
                    ui,
                    toolbar::State {
                        process_blocker,
                        importing: self.browser.import_mode,
                        destination: self.preferences.processing_destination.as_deref(),
                        destination_unavailable: self.import.destination_unavailable(
                            ui.ctx(),
                            self.preferences.processing_destination.as_deref(),
                        ),
                    },
                );
                if let Some(mode) = actions.open_folder {
                    let title = match mode {
                        FolderMode::Browse => "Browse folder",
                        FolderMode::Import => "Import from folder",
                    };
                    let mut dialog = rfd::FileDialog::new().set_title(title);
                    if let Some(folder) = &self.preferences.last_folder {
                        dialog = dialog.set_directory(folder);
                    }
                    if let Some(path) = dialog.pick_folder() {
                        self.open(path, mode);
                    }
                }
                if actions.destination {
                    let mut dialog = rfd::FileDialog::new().set_title("Processing destination");
                    if let Some(path) = self.preferences.processing_destination.as_deref() {
                        dialog = dialog.set_directory(path);
                    }
                    if let Some(path) = dialog.pick_folder() {
                        self.preferences.processing_destination = Some(path);
                        self.import.rescan(
                            &mut self.browser,
                            self.preferences.processing_destination.as_deref(),
                        );
                    }
                }
                if actions.settings {
                    self.processing_open = true;
                }
                if actions.process {
                    self.process();
                }
                if widgets::button(ui, "Settings", self.settings_open, egui::Vec2::ZERO).clicked() {
                    self.settings_open = !self.settings_open;
                }
                self.toolbar_status(ui);
            });
        });
        processing::show(
            ui.ctx(),
            &mut self.processing_open,
            if self.browser.import_mode {
                FolderMode::Import
            } else {
                FolderMode::Browse
            },
            if self.browser.import_mode {
                &mut self.preferences.import_processing
            } else {
                &mut self.preferences.browse_processing
            },
        );
        changed
    }
    fn fail(&mut self, message: impl Into<String>) {
        self.activity.error(message);
    }
    fn toolbar_status(&mut self, ui: &mut egui::Ui) {
        self.import.publish_activity(ui.ctx(), &mut self.activity);
        self.activity.receive(ui.ctx());
        self.activity
            .observe(activity::Origin::Edits, self.sidecars.warnings());
        if self.sidecars.warning().is_some()
            && self.sidecars.can_retry()
            && widgets::button(ui, "Retry saving edits", false, egui::Vec2::ZERO).clicked()
        {
            self.sidecars.retry();
        }
        self.activity.button(ui);
        self.activity.status(ui);
    }

    /// Apply a tile click from the last frame before anything reads the
    /// selection, so the photo it selects is laid out before it is planned.
    fn apply_tile_selection(&mut self) -> bool {
        let Some(request) = self.comparison_selection.take() else {
            return false;
        };
        let clicked = request.path.clone();
        let selection_changed = self.edit_selection.apply_viewport(
            request,
            self.preferences.multi_edit_limit,
            &self.viewport_selection,
        );
        timing::emit("viewport.selection.applied", || {
            serde_json::json!({"source":clicked,"photo":foveonic::diagnostics::photo_id(&clicked),
                "changed":selection_changed,"targets":self.edit_selection.paths()})
        });
        self.sync_multi_edit();
        let active = if self.edit_selection.contains(&clicked) {
            Some(clicked)
        } else if self
            .selected
            .as_deref()
            .is_some_and(|path| self.edit_selection.contains(path))
        {
            self.selected.clone()
        } else {
            self.edit_selection.paths().first().cloned()
        };
        if let Some(path) = active.filter(|path| self.selected.as_ref() != Some(path)) {
            self.select(path, false);
        }
        true
    }
    fn framing_preview_active(&self) -> bool {
        photo_tools::framing(&self.context)
            && self
                .selected
                .as_deref()
                .is_some_and(|path| self.edit_selection.contains(path))
            && self.output == "final"
            && self.before.active == editor::Rendition::Current
            && !self.comparison.requested()
    }

    fn photo_toolbar(&mut self, ui: &mut egui::Ui) -> bool {
        let extent = self
            .active_image()
            .filter(|f| {
                self.selected.as_ref() == Some(&f.request.path)
                    && f.request.output == "final"
                    && f.request.rendition == editor::Rendition::Current
                    && self.output == "final"
                    && self.before.active == editor::Rendition::Current
                    && !self.comparison.requested()
                    && self.selection_loaded()
                    && self.edit_selection.contains(&f.request.path)
            })
            .map(|frame| self.settings.rotation.extent(frame.camera_extent()));
        photo_tools::show(
            ui,
            &mut self.settings.crop,
            &mut self.settings.rotation,
            extent,
            &mut self.preferences.crop_ratios,
            &mut self.preferences.loupe,
        )
    }
    fn section_body(&mut self, ui: &mut egui::Ui, section: Section) -> bool {
        if self.multi_edit.multiple() && matches!(section, Section::Navigator | Section::Scopes) {
            ui.add_enabled(false, egui::Label::new("Available when editing one photo."));
            return false;
        }
        match section {
            Section::Navigator => {
                let mapping = self.displayed_mapping();
                let texture = self
                    .navigator_source()
                    .map(|frame| self.navigator.image(frame, mapping));
                let preview = self
                    .selected
                    .as_ref()
                    .and_then(|p| self.browser.thumbnail(p));
                let viewport = self.viewports.view_mut(self.selected.as_deref());
                viewport.zoom_snap = self.preferences.zoom_snap;
                viewport.navigator(ui, texture, preview.as_ref());
                self.preferences.zoom_snap = viewport.zoom_snap;
                false
            }
            Section::Scopes => {
                scopes::controls(
                    ui,
                    &mut self.preferences.scope,
                    &mut self.preferences.scope_viewport,
                );
                scopes::show(
                    ui,
                    self.preferences.scope,
                    self.scope_snapshot.as_ref().map(|snapshot| &snapshot.data),
                    self.scope_images.as_mut(),
                );
                false
            }
            Section::Editing => {
                let mut changed = false;
                ui.add_enabled_ui(
                    self.selection_loaded()
                        && !self.edit_selection.paths().is_empty()
                        && self.multi_edit.editable_ready(),
                    |ui| {
                        let exposure_auto_compensation = self.settings.exposure_auto_compensation;
                        let prevent_color_clipping = self.settings.prevent_color_clipping;
                        changed = controls::processing(
                            ui,
                            &mut self.settings,
                            self.metadata.as_deref(),
                            self.capture.properties.as_ref(),
                            &mut self.preferences.editing_sections,
                            &mut editor::Menus {
                                exposure_auto_compensation,
                                prevent_color_clipping,
                                family: self.metadata.as_deref().and_then(|m| {
                                    foveonic::render::ControlCapabilities::from_metadata(m)
                                        .ok()
                                        .map(|c| c.family)
                                }),
                                library: &self.preferences.editor,
                                state: &mut self.editor_ui,
                            },
                        );
                    },
                );
                changed
            }
            Section::Metadata => {
                metadata::show(ui, self.metadata.as_deref());
                false
            }
        }
    }
    fn tools(&mut self, ui: &mut egui::Ui) -> bool {
        let _scope = foveonic::diagnostics::scope(foveonic::diagnostics::Category::Ui, "ui.editor");
        let mut changed = false;
        let mut movement = None;
        egui::ScrollArea::vertical()
            .id_salt("tool sections")
            .show(ui, |ui| {
                for section in self.preferences.sections.clone() {
                    ui.add_space(sections::SECTION_GAP);
                    ui.push_id(section.title(), |ui| {
                        let collapsed = self.preferences.collapsed.contains(&section);
                        let mut response = sections::header(ui, section, collapsed);
                        if section == Section::Navigator {
                            response = help::attach(response, &help::navigator(ui.ctx().os()));
                        }
                        let header = response.rect;
                        if section == Section::Editing {
                            editor::Menus {
                                prevent_color_clipping: self.settings.prevent_color_clipping,
                                exposure_auto_compensation: self
                                    .settings
                                    .exposure_auto_compensation,
                                family: self
                                    .metadata
                                    .as_deref()
                                    .and_then(|m| {
                                        foveonic::render::ControlCapabilities::from_metadata(m)
                                            .ok()
                                            .map(|c| c.family)
                                    })
                                    .filter(|_| self.selection_loaded()),
                                library: &self.preferences.editor,
                                state: &mut self.editor_ui,
                            }
                            .header(ui, editor::Scope::All, header);
                        }
                        if response.clicked() {
                            self.preferences.toggle(section);
                        }
                        if !self.preferences.collapsed.contains(&section) {
                            ui.add_space(6.);
                            changed |= egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(
                                    sections::CONTENT_HORIZONTAL_PADDING,
                                    0,
                                ))
                                .show(ui, |ui| self.section_body(ui, section))
                                .inner;
                        }
                        if let Some((source, after)) = sections::drop(ui, header) {
                            movement = Some((source, section, after));
                        }
                    });
                }
            });
        if let Some((source, target, after)) = movement {
            self.preferences.move_section(source, target, after);
        }
        changed |= self.apply_editor_actions(ui.ctx());
        changed
    }
    fn displayed_mapping(&self) -> viewport::Mapping {
        self.mapping_for(self.active_image())
    }
    /// How a photo's image is mapped for display. The chosen channel and
    /// comparison apply only once the image shows the chosen output: an image
    /// of an older request would show its samples with the wrong meaning.
    fn mapping_for(&self, image: Option<&worker::RenderResult>) -> viewport::Mapping {
        if self.before.active != editor::Rendition::Current {
            return viewport::Mapping::normal(-1);
        }
        let shows_output = image.is_some_and(|image| image.request.output == self.output);
        let channel = if shows_output { self.channel } else { -1 };
        let has_pair = shows_output && image.is_some_and(|image| image.comparison.is_some());
        let mut mapping = self.comparison.mapping(channel, has_pair);
        if let Some(image) = image {
            let description = image.texture.description();
            if matches!(description.space, foveonic::image::Space::LinearRgb(_)) {
                mapping.signal_scale = 4095. / description.linear_white as f32;
            }
        }
        mapping
    }
    fn central(&mut self, ui: &mut egui::Ui) -> bool {
        let shown_pair = self
            .active_image()
            .map(|frame| (frame.request.output.clone(), frame.comparison.is_some()));
        let _scope =
            foveonic::diagnostics::scope(foveonic::diagnostics::Category::Ui, "ui.viewport");
        let mut changed = false;
        let background_width = 2. * widgets::ControlSize::Medium.side()
            + widgets::MEDIUM_CONTROL_GAP
            + 2. * f32::from(widgets::BUTTON_GROUP_MARGIN);
        toolbar::row_with_trailing(
            ui,
            background_width,
            |ui| {
                changed |=
                    inspect::processing_quality(ui, &mut self.preferences.processing_quality);
                widgets::button_group(ui, |ui| {
                    widgets::centered_control_row(ui, |ui| {
                        ui.weak("Inspect");
                        ui.scope(|ui| {
                            changed |= inspect::show(
                                ui,
                                &self.settings,
                                &mut self.output,
                                &mut self.channel,
                                self.metadata.as_deref(),
                                self.capture
                                    .properties
                                    .as_ref()
                                    .and_then(|p| p.get("CAMMODEL"))
                                    .and_then(|model| {
                                        foveonic::source::ProcessingFamily::from_camera_model(model)
                                    }),
                            );
                            let requested = self.comparison.show(
                                ui,
                                &self.settings,
                                &self.output,
                                self.channel,
                                self.metadata.as_deref(),
                            );
                            let has_pair =
                                shown_pair.as_ref().is_some_and(|(output, comparison)| {
                                    *output == self.output && *comparison
                                });
                            changed |= requested && (self.busy || !has_pair);
                        });
                    });
                });
            },
            |ui| {
                inspect::background(ui, &mut self.preferences.white_viewport_background);
            },
        );
        if crop_tool::active(ui.ctx()) && !self.framing_preview_active() {
            crop_tool::exit(ui.ctx());
        }
        if rotate_tool::active(ui.ctx()) && !self.framing_preview_active() {
            rotate_tool::exit(ui.ctx());
        }
        let mapping = self.displayed_mapping();
        if mapping.difference_gain > 0. {
            let frame = self.active_image().unwrap();
            let input = frame.comparison.as_ref().unwrap().input;
            let (range, units) = if matches!(
                frame.texture.description().space,
                foveonic::image::Space::ToneMappedRgb(_)
            ) {
                (1., "linear")
            } else if matches!(
                frame.texture.description().space,
                foveonic::image::Space::SensorRgb
                    | foveonic::image::Space::SensorBlue
                    | foveonic::image::Space::CorrectedSensorRgb
                    | foveonic::image::Space::CorrectedSensorBlue
            ) {
                (16383., "codes")
            } else {
                (frame.texture.description().linear_white as f32, "codes")
            };
            ui.add(
                egui::Label::new(format!(
                    "Δ {} − {input} · dark − / gray 0 / light + · ±{:.3} {units} · scopes: output",
                    frame.request.output,
                    range / mapping.difference_gain,
                ))
                .wrap(),
            );
        } else {
            let name = self
                .active_image()
                .map_or("preview", |frame| frame.request.output.as_str());
            let label = if self.comparison.difference {
                Some(format!("Difference pending. Showing {name}."))
            } else if self.comparison.input {
                Some(if mapping.input {
                    let input = self
                        .active_image()
                        .unwrap()
                        .comparison
                        .as_ref()
                        .unwrap()
                        .input;
                    format!("Input · {input} · scopes: output")
                } else {
                    format!("Input pending. Showing {name}.")
                })
            } else if self.active_image().is_some_and(|frame| {
                foveonic::inspection::comparison_input(
                    &frame.artifact.settings,
                    &frame.request.output,
                    Some(&frame.artifact.metadata),
                )
                .is_some()
            }) {
                Some(format!("Output · {name} · scopes: output"))
            } else {
                None
            };
            if let Some(label) = label {
                // Keep one line on both sides so flickering cannot resize the canvas.
                ui.add(egui::Label::new(label).truncate());
            }
        }
        ui.add_space(3.);
        let height =
            (ui.available_height() - information::HEIGHT - ui.spacing().item_spacing.y).max(0.);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_min_height(height);
                let canvas = egui::Rect::from_min_size(
                    ui.cursor().min,
                    egui::vec2(ui.available_width(), height),
                );
                photo_tools::set_viewport_rect(ui.ctx(), canvas);
                ui.painter().rect_filled(
                    canvas,
                    0.,
                    if self.preferences.white_viewport_background {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::BLACK
                    },
                );
                let paths = self.browser_selection.clone();
                self.viewport_selection.clear();
                if self.multi_photo_overview() {
                    changed |= self.comparison_view(ui, &paths);
                } else {
                    self.viewport_selection
                        .extend(self.selected.iter().cloned());
                    changed |= self.show_photo(ui, mapping);
                    self.before_badge(ui, canvas);
                }
                self.show_loupe(ui, mapping);
            },
        );
        let path = self
            .active_image()
            .map(|f| &f.request.path)
            .or(self.selected.as_ref());
        let filename = path
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        let properties = self
            .active_image()
            .map(|f| &f.artifact.metadata.properties)
            .or_else(|| self.metadata.as_ref().map(|metadata| &metadata.properties))
            .or(self.capture.properties.as_ref());
        information::show(ui, properties, &filename, self.viewport().visible_pixels());
        changed
    }

    fn photo_interaction_available(&self, mapping: viewport::Mapping) -> bool {
        self.selected
            .as_deref()
            .is_some_and(|path| self.edit_selection.contains(path))
            && self.selection_loaded()
            && self.before.active == editor::Rendition::Current
            && self.output == "final"
            && !mapping.input
            && mapping.difference_gain == 0.
            && self.active_image().is_some_and(|f| {
                !f.request.framing_preview
                    && self.selected.as_ref() == Some(&f.request.path)
                    && f.request.output == "final"
                    && f.request.rendition == editor::Rendition::Current
            })
    }

    fn show_photo(&mut self, ui: &mut egui::Ui, mapping: viewport::Mapping) -> bool {
        let group_target = self.edit_selection.paths().len() > 1
            && self
                .selected
                .as_ref()
                .is_some_and(|path| self.edit_selection.contains(path));
        self.viewport_mut().group_target = group_target;
        let allowed = self.photo_interaction_available(mapping);
        let framing_active = self.framing_preview_active() && self.selection_loaded();
        let crop_active = framing_active && crop_tool::active(ui.ctx());
        self.viewport_mut().framing = self
            .active_image()
            .filter(|f| framing_active && f.request.framing_preview)
            .map(|frame| framing::Presentation {
                rotation: rotate_tool::displayed(ui.ctx(), self.settings.rotation),
                native_extent: frame.camera_extent(),
            });
        self.viewport_mut().editing = photo_tools::takes_pointer(ui.ctx())
            && if photo_tools::framing(ui.ctx()) {
                framing_active
            } else {
                allowed
            };
        let content = self.active_image().map(|frame| {
            (
                frame.view(mapping.input).clone(),
                frame.display_extent(mapping.input),
                frame.comparison.as_ref().map(|c| c.texture.clone()),
                frame.diagnostic_identity(mapping.input),
                frame.display_window(false),
            )
        });
        if let Some((texture, extent, comparison, identity, window)) = content {
            let detail = self.active_patch().map(|detail| viewport::ImageLayer {
                texture: detail.texture.clone(),
                coverage: detail.detail_region(),
                identity: detail.diagnostic_identity(false),
            });
            let viewport = self.viewports.view_mut(self.selected.as_deref());
            viewport.zoom_snap = self.preferences.zoom_snap;
            viewport.show(
                ui,
                texture,
                viewport
                    .framing
                    .map_or(extent, framing::Presentation::extent),
                comparison,
                mapping,
                self.srgb_target,
                identity,
            );
            if let Some(layer) = detail {
                viewport.show_region(ui, layer, window, mapping, self.srgb_target);
            }
        } else if let Some(path) = &self.selected {
            if let Some(preview) = self.browser.preview(path) {
                let viewport = self.viewports.view_mut(Some(path));
                viewport.zoom_snap = self.preferences.zoom_snap;
                viewport.show_preview(
                    ui,
                    &preview.texture,
                    preview.native_extent,
                    if crop_active {
                        Default::default()
                    } else {
                        self.settings.crop
                    },
                );
            } else {
                self.viewport_mut().show_empty(ui);
            }
        } else {
            ui.centered_and_justified(|ui| {
                let message = if self.browser.files.is_empty() {
                    "Open a folder of X3F photographs"
                } else {
                    "No visible photographs"
                };
                ui.label(egui::RichText::new(message).color(
                    if self.preferences.white_viewport_background {
                        egui::Color32::DARK_GRAY
                    } else {
                        egui::Color32::LIGHT_GRAY
                    },
                ));
            });
        }
        if let Some(path) = self.selected.clone() {
            self.observe_photo_view_density(&path, ui.ctx().pixels_per_point());
        }
        if let (Some(path), Some((response, uv))) = (
            &self.selected,
            self.viewport()
                .pointer(ui.input(|i| i.pointer.interact_pos())),
        ) {
            if framing_active {
                if let (Some(frame), Some((canvas, image))) = (
                    self.active_image()
                        .filter(|f| f.request.framing_preview && &f.request.path == path),
                    self.viewport().image_rectangles(),
                ) {
                    return framing::photo(
                        ui,
                        &response,
                        canvas,
                        image,
                        frame.camera_extent(),
                        &mut self.settings.crop,
                        &mut self.settings.rotation,
                    );
                }
                return false;
            }
            // Existing gestures can continue while their render catches up. Start a new
            // selection only from a frame that represents the current authored recipe.
            let uv = uv.filter(|_| {
                self.active_image().is_some_and(|image| {
                    self.preferences
                        .renders_edits(&image.request.settings, &self.settings)
                })
            });
            let (changed, request) =
                photo_tools::pick(ui, &response, uv, path, &mut self.settings, allowed);
            if let Some(request) = request {
                if let Some(frame) = self.active_image() {
                    self.worker
                        .pick_color(request, frame.request.id, Some(frame.origin.clone()));
                }
            }
            return changed;
        }
        false
    }
}
impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if let Some(smoke) = &mut self.smoke {
            smoke.inject_direct_color(input, ctx);
        }
        timing::pointer_input(input, photo_tools::selects_photos(ctx));
        self.capture_actions(ctx, input);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.preferences.save(storage);
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.diagnostics.poll(ctx);
        let _update = timing::Span::new(self.last_request_id.0, "ui_update");
        if let Some(ready) = self.startup_ready.take() {
            foveonic::support::event(
                foveonic::support::Level::Info,
                "startup.ready",
                "Workspace ready",
                serde_json::Value::Null,
            );
            timing::emit(
                "startup",
                || serde_json::json!({"phase":"first_update", "since_constructed_ms": ready.elapsed().as_secs_f64() * 1000.}),
            );
        }
        let began = std::time::Instant::now();
        if timing::enabled() {
            let gap = self
                .last_frame
                .replace(began)
                .map(|last| began.duration_since(last).as_secs_f64() * 1000.);
            timing::native_frame(
                self.active_image().map_or(0, |image| image.request.id.0),
                frame.info().cpu_usage,
                gap,
                self.busy,
            );
        }
        self.receive_folder();
        self.capture.receive();
        self.import.update(&mut self.browser);
        let cache_status = self.worker.cache.status();
        self.browser.decode_states = cache_status.files;
        self.browser.cache_enabled =
            self.preferences.decode_cache.enabled && self.preferences.decode_cache.max_bytes > 0;
        if self.preferences.decode_cache.enabled && !self.browser.files.is_empty() {
            ctx.request_repaint_after(if !cache_status.prefetching.is_empty() {
                CACHE_ACTIVE_POLL
            } else {
                CACHE_IDLE_POLL
            });
        }
        photo_tools::source(ctx, self.selected.as_deref());
        self.sidecars.receive();
        self.adopt_selected_settings();
        if self
            .smoke
            .as_ref()
            .is_some_and(smoke::Smoke::preview_pending)
        {
            self.browser.receive(ctx);
        } else {
            self.receive(ctx);
        }
        self.noise_calibration
            .poll(&mut self.preferences.noise_profile);
        self.noise_calibration
            .show(ctx, &mut self.preferences.noise_profile);
        if self.noise_calibration.take_changed() {
            let selected = self
                .noise_calibration
                .selected(self.preferences.noise_profile);
            self.worker
                .cache
                .configure_noise_calibration(selected.as_ref().map(|p| p.signature().to_owned()));
            self.import.set_noise_calibration(selected);
            self.browser.invalidate_previews();
            self.previews.invalidate();
        }
        let tile_selected = self.apply_tile_selection();
        self.smoke_tick(ctx);
        automation::begin_frame(ctx);
        let normalized = controls::normalize_with_capture(
            &mut self.settings,
            self.metadata.as_deref(),
            self.capture.properties.as_ref(),
        );
        editor::advanced_color::sync_photo(ctx, self.selected.as_deref());
        let previous_settings = self.settings.clone();
        let previous_selection = self.selected.clone();
        let toolbar_changed = egui::TopBottomPanel::top("toolbar")
            .show(ctx, |ui| self.toolbar(ui))
            .inner;
        let previous_gamut = self.preferences.display_gamut;
        let previous_limits = self.preferences.adaptive_tone_tuning;
        let capture_action = application_settings::show(
            ctx,
            &mut self.settings_open,
            &mut self.preferences,
            &self.worker.cache,
            self.display_target.managed(),
            self.worker.memory_status(),
            &mut self.diagnostics,
        );
        if let Some(action) = capture_action {
            let result = match action {
                diagnostics::Action::Start(mode) => {
                    let state = self.diagnostic_state();
                    self.diagnostics
                        .start(mode, serde_json::json!({"entry":"manual"}))
                        .map(|()| self.diagnostics.set_start_state(state))
                }
                diagnostics::Action::Mark(label) => {
                    self.diagnostics.mark(
                        &label,
                        serde_json::json!({"source":self.selected,"revision":self.last_request_id}),
                    );
                    Ok(())
                }
                diagnostics::Action::StopSave(directory) => {
                    let state = self.diagnostic_state();
                    self.diagnostics.set_end_state(state);
                    self.diagnostics.stop_manual_to(directory)
                }
            };
            if let Err(error) = result {
                self.diagnostics.fail(error);
            }
        }
        application_settings::adaptive(ctx, &mut self.preferences);
        self.browser
            .set_thumbnail_budget(self.preferences.thumbnail_cache_bytes());
        if previous_gamut != self.preferences.display_gamut {
            viewport::set_gamut(&self.render_state, self.preferences.display_gamut);
        }
        let mut select = None;
        let screenshot = smoke::screenshot_active().then(smoke::screenshot_recipe);
        let organizer_minimum = browser::minimum_panel_width(&ctx.style());
        let organizer_limit = (ctx.available_rect().width() - 540.).clamp(organizer_minimum, 650.);
        let mut organizer_frame = egui::Frame::side_top_panel(&ctx.style());
        organizer_frame.inner_margin.bottom = 0;
        egui::SidePanel::left("organizer")
            .frame(organizer_frame)
            .default_width(
                screenshot
                    .as_ref()
                    .map_or(230., |recipe| recipe.organizer_width),
            )
            .width_range(organizer_minimum..=organizer_limit)
            .resizable(true)
            .show(ctx, |ui| {
                egui::TopBottomPanel::bottom("browser progress")
                    .frame(egui::Frame::NONE)
                    .exact_height(information::HEIGHT)
                    .show_inside(ui, |ui| {
                        progress::show(
                            ui,
                            self.worker.progress.snapshot(),
                            !smoke::screenshot_active(),
                        );
                    });
                let browser_focus = ui.make_persistent_id("organizer focus");
                self.browser.sort = self.preferences.browser_sort;
                self.browser.show_ratings = self.preferences.browser_show_ratings;
                self.browser
                    .header(ui, &mut self.preferences.organizer, browser_focus);
                ui.separator();
                select = self.browser.organizer(
                    ui,
                    self.selected.as_deref(),
                    self.preferences.organizer,
                );
                self.preferences.browser_sort = self.browser.sort;
                self.preferences.browser_show_ratings = self.browser.show_ratings;
            });
        let actions_changed = self.dispatch_actions(ctx);
        let tuning_changed = previous_limits != self.preferences.adaptive_tone_tuning;
        if tuning_changed {
            self.browser.invalidate_previews();
            self.previews.invalidate();
        }
        let mut changed = normalized || actions_changed || tuning_changed || toolbar_changed;
        self.sync_browser_selection();
        if self
            .selected
            .as_deref()
            .is_some_and(|path| !self.edit_selection.contains(path))
        {
            if let Some(path) = self.edit_selection.paths().first().cloned() {
                self.select(path, false);
            }
        }
        // Apply selection before the viewport draws, so a newly selected photo
        // is laid out, and its visible region known, before it is planned.
        let selection_applied = if std::mem::take(&mut self.browser.clear_selection) {
            self.clear_selection();
            true
        } else if let Some(selection) = select {
            let path = if self.edit_selection.contains(&selection.path) {
                selection.path
            } else {
                self.edit_selection
                    .paths()
                    .first()
                    .cloned()
                    .unwrap_or(selection.path)
            };
            self.select(path, selection.preserve_viewport);
            true
        } else {
            false
        };
        self.sync_multi_edit();
        let editor_before = self.settings.clone();
        multi_edit_controls::begin_with_numeric(
            ctx,
            self.multi_edit.snapshots(),
            self.multi_edit.numeric_snapshots(),
        );
        multi_edit_controls::set_metadata(ctx, self.multi_edit.metadata_snapshots());
        for (pointer, ready) in self.multi_edit.optional_readiness() {
            multi_edit_controls::set_optional_ready(ctx, pointer, ready);
        }
        for (pointer, range) in self.multi_edit.control_ranges() {
            multi_edit_controls::set_range(ctx, &pointer, range);
        }
        let tools_limit = (ctx.available_rect().width() - 240.).clamp(TOOLS_MIN_WIDTH, 600.);
        let settings_panel = egui::SidePanel::right("settings")
            .default_width(
                screenshot
                    .as_ref()
                    .map_or(TOOLS_DEFAULT_WIDTH, |recipe| recipe.tools_width),
            )
            .width_range(TOOLS_MIN_WIDTH..=tools_limit)
            .resizable(true)
            .show(ctx, |ui| {
                changed |= self.tools(ui);
            });
        automation::panel(settings_panel.response.rect);
        let mut edits = multi_edit_controls::finish(ctx);
        if self.multi_edit.editable_ready() {
            let assignments =
                multi_edit_controls::assignments(&editor_before, &self.settings, &edits);
            edits.extend(assignments);
            if self.multi_edit.apply_edits(&edits, &mut self.sidecars) {
                for path in self.edit_selection.paths() {
                    self.previews.invalidate_path(path);
                }
                changed = true;
            }
            if !edits.is_empty() {
                if let Some(settings) = self
                    .selected
                    .as_deref()
                    .and_then(|path| self.multi_edit.settings(path))
                {
                    // The range preview is view state, not an edit, so this frame's
                    // choice in the panel stands.
                    let preview = self.settings.color_range_preview;
                    self.settings = settings.clone();
                    self.settings.color_range_preview = preview;
                }
            }
        }
        let mut viewport_frame = egui::Frame::central_panel(&ctx.style());
        // The information row owns its vertical spacing down to the window edge.
        viewport_frame.inner_margin.bottom = 0;
        egui::CentralPanel::default()
            .frame(viewport_frame)
            .show(ctx, |ui| {
                changed |= self.central(ui);
            });
        self.browser.finish_previews();
        if photo_tools::take_view_change(ctx) {
            self.viewport_mut().reset();
            changed = true;
        }
        let edited = previous_selection == self.selected
            && (normalized || previous_settings != self.settings);
        if edited && self.commit_settings() {
            self.exit_before();
        }
        self.preferences.direct_color = editor::direct_color::configuration(ctx);
        // A tile click this frame selects next frame; it must not leave Before now.
        let tile_clicked = self.comparison_selection.is_some();
        if changed && !selection_applied && !tile_selected && !tile_clicked {
            self.exit_before();
        }
        self.submit_photos();
        if let Some(paths) = self.browser.take_prefetch_order(self.selected.as_deref()) {
            self.worker
                .cache
                .prefetch_window(paths, self.selected.clone());
        }
        let (_, preparation_paths) = self.worker.cache.preparation_window();
        self.worker
            .configure_preparation(worker::PreparationConfig {
                processing_quality: self.preferences.processing_quality,
                recipes: self.sidecars.cached_recipes(&preparation_paths),
                calibration: self
                    .noise_calibration
                    .selected(self.preferences.noise_profile),
            });
        self.editor_ui.dialog(ctx, &mut self.preferences.editor);
        let support_photo = (self.selected.clone(), self.last_request_id);
        let source_changed = self
            .support_photo
            .as_ref()
            .is_none_or(|previous| previous.0 != self.selected);
        if self.support_preferences.as_ref() != Some(&self.preferences)
            || self.support_photo.as_ref() != Some(&support_photo)
        {
            self.support_preferences = Some(self.preferences.clone());
            self.support_photo = Some(support_photo);
            self.support_context_due =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(500));
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        let opened = self.activity.is_open() && !self.support_log_open;
        if source_changed
            || opened
            || self
                .support_context_due
                .is_some_and(|due| std::time::Instant::now() >= due)
        {
            foveonic::support::update_context("preferences", self.preferences.support_context());
            foveonic::support::update_context(
                "requested_workspace",
                serde_json::json!({
                    "source":self.selected.as_deref().map(crate::support::path),"revision":self.last_request_id,
                    "settings":foveonic::support::settings_snapshot(&self.settings),"quality":self.preferences.processing_quality,"output":self.output,
                }),
            );
            self.support_context_due = None;
        }
        self.support_log_open = self.activity.is_open();
        self.activity.show(ctx);
        self.update_scopes();
        self.smoke_check_scope_retention();
        self.sync_busy();
        self.sync_browser_thumbnails();
        let mapping = self.displayed_mapping();
        let active = self.active_key();
        let frame = active.as_ref().and_then(|(key, origin)| {
            rendering::views::whole_image(&self.images, key, origin.as_ref())
        });
        self.previews.update(frame, self.busy, ctx);
        let navigator_source = frame.filter(|_| !self.multi_edit.multiple());
        self.navigator
            .update(navigator_source, mapping, self.busy, ctx);
        self.update_memory();
    }
}
fn multi_photo_overview(paths: &[PathBuf], active: Option<&std::path::Path>) -> bool {
    paths.len() > 1 && active.is_some_and(|active| paths.iter().any(|path| path == active))
}
fn main() -> std::process::ExitCode {
    let guard = support::initialize();
    let result = run();
    if let Err(error) = &result {
        foveonic::support::report_error("application.run", error, Default::default());
        let message = format!(
            "{error}\n\nSupport logs: {}",
            guard.log_directory().display()
        );
        #[cfg(target_os = "windows")]
        show_startup_error(&message);
        #[cfg(not(target_os = "windows"))]
        eprintln!("{message}");
    }
    let success = result.is_ok() && !support::startup_failed();
    support::shutdown(guard, success);
    if success {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

#[cfg(target_os = "windows")]
fn show_startup_error(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "Foveonic".encode_utf16().chain(Some(0)).collect();
    // The pointers remain valid for the duration of this synchronous call.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn run() -> eframe::Result {
    let startup_began = std::time::Instant::now();
    timing::configure().map_err(eframe::Error::AppCreation)?;
    let mut path = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--help" => {
                println!("foveonic-gui [X3F_OR_FOLDER]");
                return Ok(());
            }
            _ => {
                if path.is_some() || arg.starts_with('-') {
                    let message = format!("Unexpected argument: {arg}");
                    return Err(eframe::Error::AppCreation(
                        std::io::Error::new(std::io::ErrorKind::InvalidInput, message).into(),
                    ));
                }
                path = Some(PathBuf::from(arg));
            }
        }
    }
    let startup_observations = startup::Observations::new(startup_began);
    foveonic::support::update_context(
        "startup_request",
        serde_json::json!({"source":path.as_deref().map(|path|path.to_string_lossy())}),
    );
    startup_observations.record(
        "startup.entry_to_arguments",
        startup_began,
        std::time::Instant::now(),
    );
    let startup_capture = startup::Capture::new(startup_observations.clone())
        .map_err(|error| eframe::Error::AppCreation(error.into()))?;
    let options_began = std::time::Instant::now();
    let setup = egui_wgpu::WgpuSetupCreateNew {
        #[cfg(target_os = "windows")]
        instance_descriptor: foveonic::gpu_instance_descriptor(),
        device_descriptor: Arc::new(|adapter| wgpu::DeviceDescriptor {
            label: Some("Shared GUI/image device"),
            required_features: timing::required_features(adapter.features()),
            required_limits: adapter.limits(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let background = std::env::var_os("FOVEONIC_GUI_SCENARIO_BACKGROUND").is_some();
    let mut viewport = egui::ViewportBuilder::default()
        .with_app_id(preferences::APP_ID)
        .with_icon(icon::window_icon())
        .with_inner_size(if smoke::screenshot_active() {
            smoke::screenshot_recipe().window_size
        } else {
            [1440., 1000.]
        })
        .with_min_inner_size([1000., 600.]);
    if background {
        viewport = viewport
            .with_active(false)
            .with_window_level(egui::WindowLevel::AlwaysOnBottom);
    }
    #[cfg(target_os = "macos")]
    let event_loop_builder = background.then(|| {
        Box::new(|builder: &mut eframe::EventLoopBuilder<_>| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            builder.with_activate_ignoring_other_apps(false);
        }) as eframe::EventLoopBuilderHook
    });
    #[cfg(not(target_os = "macos"))]
    let event_loop_builder = None;
    let workspace_viewport = Arc::new(std::sync::Mutex::new(None));
    let options = eframe::NativeOptions {
        window_builder: Some({
            let workspace = workspace_viewport.clone();
            Box::new(move |viewport| {
                *workspace.lock().unwrap() = Some(viewport.clone());
                startup::compact_window(viewport)
            })
        }),
        renderer: eframe::Renderer::Wgpu,
        persistence_path: std::env::var_os("FOVEONIC_GUI_SMOKE_DIR")
            .map(PathBuf::from)
            .map(|d| d.join("app.ron")),
        viewport,
        event_loop_builder,
        wgpu_options: display::configure(egui_wgpu::WgpuConfiguration {
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup),
            on_paint_stage: Some({
                let observations = startup_observations.clone();
                Arc::new(move |value| observations.paint(value))
            }),
            on_setup_stage: timing::requested_mode().map(|_| {
                let observations = startup_observations.clone();
                Arc::new(move |value| observations.setup(value))
                    as Arc<dyn Fn(egui_wgpu::SetupTiming) + Send + Sync>
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    startup_observations.record(
        "startup.window_options",
        options_began,
        std::time::Instant::now(),
    );
    foveonic::support::event(
        foveonic::support::Level::Info,
        "startup.graphics",
        "Creating native window and GPU device",
        serde_json::Value::Null,
    );
    eframe::run_native(
        preferences::APP_NAME,
        options,
        Box::new(move |cc| {
            let workspace = workspace_viewport
                .lock()
                .unwrap()
                .take()
                .expect("native workspace viewport");
            Ok(Box::new(startup::Startup::new(
                cc,
                path,
                startup_capture,
                startup_observations,
                workspace,
            )?))
        }),
    )
}
#[cfg(test)]
mod tests {
    #[test]
    fn restored_recipes_normalize_unavailable_inspection_before_rendering() {
        let settings = foveonic::render::Settings::new("Sunlight", "Standard");
        let mut output = "noise.layers".to_owned();
        let mut channel = 2;
        super::inspect::normalize(&settings, &mut output, &mut channel, None, None);
        assert_eq!((&*output, channel), ("final", -1));
        output = "calibration.layers".into();
        channel = 1;
        super::inspect::normalize(&settings, &mut output, &mut channel, None, None);
        assert_eq!((&*output, channel), ("calibration.layers", 1));
    }
}
