//! Runs a three-pane photographic workspace with current-buffer inspection, separate UI preferences and per-X3F editing recipes.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

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
use rendering::{delivery, matching, request, smart, worker};
use std::{path::PathBuf, sync::Arc};

// Keep folder readiness responsive during prefetch without polling idle folders at frame rate.
const CACHE_ACTIVE_POLL: std::time::Duration = std::time::Duration::from_millis(150);
const CACHE_IDLE_POLL: std::time::Duration = std::time::Duration::from_secs(1);
const TOOLS_MIN_WIDTH: f32 = 300.;
const TOOLS_DEFAULT_WIDTH: f32 = 330.;

/// Opening starts at selection, including recipe reads before worker submission.
struct Opening {
    _priority: foveonic::decode_cache::Activity,
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
    viewport_paths: Arc<[PathBuf]>,
    displayed_paths: Vec<PathBuf>,
    viewport_paths_key: Option<(u64, Option<PathBuf>)>,
    viewport_paths_limit: usize,
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
    camera_overrides: [String; 2],
    output: String,
    channel: i32,
    comparison: inspect::Comparison,
    frame: Option<worker::Frame>,
    preview_scales: std::collections::BTreeMap<PathBuf, foveonic::render::Scale>,
    preview_densities: std::collections::BTreeMap<PathBuf, f32>,
    preview_demand_changed: bool,
    scope_images: Option<scopes::Images>,
    scope_snapshot: Option<worker::ScopeSnapshot>,
    scope_key: Option<worker::ScopeKey>,
    viewports: photo_views::PhotoViews,
    navigator: navigator::Navigator,
    previews: previews::Previews,
    revision: u64,
    busy: bool,
    memory_key: Vec<u64>,
    memory_reply_epoch: u64,
    exporting: bool,
    preferences: preferences::Preferences,
    activity: activity::Log,
    srgb_target: bool,
    display: display::Host,
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
                serde_json::json!({"source":path,
                    "photo":timing::context_for(0,0,path,"snapshot").photo,
                    "settings":settings})
            })
            .collect();
        serde_json::json!({"workspace_mode":if self.browser.import_mode {"import"} else {"browse"},
            "active_photo":self.selected,"targets":self.edit_selection.paths(),"photos":photos,
            "settings":self.settings,"processing_quality":self.preferences.processing_quality,
            "displayed":self.frame.as_ref().map(|frame|serde_json::json!({"source":frame.path,
                "task":frame.diagnostic_task.id,"revision":frame.diagnostic_task.revision,
                "presentation_revision":frame.revision,"artifact":frame.artifact.name,
                "scale":frame.artifact.settings.working_scale.divisor(),"purpose":frame.diagnostic_task.purpose})),
            "output":self.output,"rendition":format!("{:?}",self.before.active),
            "render_pending":self.worker.busy(),"disk_cache":self.preferences.decode_cache,
            "memory_cache_bytes":self.preferences.memory_cache_bytes,"multi_edit_limit":self.preferences.multi_edit_limit,
            "hardware":self.diagnostic_hardware()})
    }
    pub(crate) fn viewport(&self) -> &viewport::Viewport {
        self.viewports.view(self.selected.as_deref())
    }
    pub(crate) fn viewport_mut(&mut self) -> &mut viewport::Viewport {
        self.viewports.view_mut(self.selected.as_deref())
    }

    fn multi_photo_overview(&self) -> bool {
        multi_photo_overview(&self.viewport_paths, self.selected.as_deref())
    }

    fn sync_viewport_paths(&mut self) {
        let key = (self.browser.selection_generation(), self.selected.clone());
        let changed = self.viewport_paths_key.as_ref() != Some(&key);
        if changed {
            self.viewport_paths = self
                .browser
                .processing_selection(self.selected.as_deref())
                .into();
            self.viewport_paths_key = Some(key);
            self.preview_scales
                .retain(|path, _| self.viewport_paths.contains(path));
            self.preview_densities
                .retain(|path, _| self.viewport_paths.contains(path));
        }
        if changed || self.viewport_paths_limit != self.preferences.multi_edit_limit {
            self.edit_selection
                .sync_browser(&self.viewport_paths, self.preferences.multi_edit_limit);
            self.viewport_paths_limit = self.preferences.multi_edit_limit;
        }
    }
    fn new(
        cc: &eframe::CreationContext<'_>,
        path: Option<PathBuf>,
        settings: Settings,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let startup_began = timing::enabled().then(std::time::Instant::now);
        let mut startup_profile = foveonic::profile::Profile::new(timing::mode());
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .ok_or("wgpu renderer unavailable")?;
        timing::check_gpu_features(rs.device.features())?;
        let mut diagnostics = diagnostics::Controller::default();
        if let Some(requested) = timing::requested_mode() {
            let mode = if requested == foveonic::diagnostics::Mode::Gpu && !timing::gpu_available()
            {
                foveonic::diagnostics::Mode::Wall
            } else {
                requested
            };
            diagnostics.start(mode, serde_json::json!({"entry":"environment",
                "adapter":rs.adapter.get_info().name,"gpu_timestamps_available":timing::gpu_available()}))?;
        }
        timing::emit(
            "session",
            || serde_json::json!({"frontend":"gui","adapter":rs.adapter.get_info().name,"gpu_supported":rs.adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY),"gpu_enabled":rs.device.features().contains(wgpu::Features::TIMESTAMP_QUERY)}),
        );
        let display = display::Host::new(cc)?;
        timing::emit("display", || {
            serde_json::json!({
                "format": format!("{:?}", rs.target_format),
                "encoding": format!("{:?}", display.target),
                "managed": display.target.managed(),
            })
        });
        viewport::install(rs);
        widgets::install_style(&cc.egui_ctx);
        if smoke::screenshot_active() {
            cc.egui_ctx.set_theme(egui::Theme::Dark);
        } else if std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").is_some()
            && std::env::var_os("FOVEONIC_GUI_SMOKE_LIGHT").is_some()
        {
            cc.egui_ctx.set_theme(egui::Theme::Light);
        }
        let engine = Engine::from_device_profiled(
            rs.device.clone(),
            rs.queue.clone(),
            rs.adapter.get_info().name,
            &mut startup_profile,
        )?;
        let engine = if let Some(directory) = std::env::var_os("FOVEONIC_CALIBRATION_RESOURCES") {
            engine.with_calibration_resources(directory)?
        } else {
            engine
        };
        let overrides = [settings.white_balance.clone(), settings.color_mode.clone()];
        let mut preferences = preferences::Preferences::load(cc.storage);
        let smoke = smoke::Smoke::from_env();
        if let Some(smoke) = &smoke {
            smoke.configure_cache(&mut preferences.decode_cache);
        }
        editor::direct_color::configure(&cc.egui_ctx, preferences.direct_color);
        if std::env::var_os("FOVEONIC_GUI_SMOKE_SMART").is_some() {
            preferences.processing_quality = ProcessingQuality::Quality;
        }
        viewport::set_gamut(rs, preferences.display_gamut);
        let training_engine = engine.executor();
        let batch_engine = engine.executor();
        let worker = worker::Worker::new(engine, cc.egui_ctx.clone());
        let import =
            import_review::Review::new(batch_engine, worker.cache.clone(), cc.egui_ctx.clone());
        let previews =
            previews::Previews::new(rs.clone(), cc.egui_ctx.clone(), worker.cache.clone());
        let browser = browser::Browser::with_cache(
            cc.egui_ctx.clone(),
            Some(worker.cache.clone()),
            rs.clone(),
        );
        let noise_calibration = noise_calibration::State::new(
            training_engine,
            worker.cache.clone(),
            cc.egui_ctx.clone(),
            &mut preferences.noise_profile,
        );
        worker.cache.configure_noise_calibration(
            noise_calibration
                .selected(preferences.noise_profile)
                .as_ref()
                .map(|p| p.signature().to_owned()),
        );
        let mut app = Self {
            context: cc.egui_ctx.clone(),
            diagnostics,
            noise_calibration,
            comparison_selection: None,
            edit_selection: Default::default(),
            multi_edit: Default::default(),
            loupe: Default::default(),
            comparison_layout: Default::default(),
            viewport_paths: Arc::default(),
            displayed_paths: Vec::new(),
            viewport_paths_key: None,
            viewport_paths_limit: 0,
            editor_ui: Default::default(),
            before: Default::default(),
            actions: Vec::new(),
            import,
            settings_open: false,
            processing_open: false,
            opening: None,
            browser,
            folder_loader: folder_loader::Loader::new(cc.egui_ctx.clone()),
            previews,
            capture: capture::Loader::new(cc.egui_ctx.clone(), worker.cache.clone()),
            sidecars: sidecars::Session::new(
                cc.egui_ctx.clone(),
                std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").is_none()
                    || std::env::var_os("FOVEONIC_GUI_SCENARIO_MANIFEST").is_some(),
                worker.cache.clone(),
            ),
            worker,
            metadata: None,
            selected: None,
            settings,
            camera_overrides: overrides,
            output: "final".into(),
            channel: -1,
            comparison: Default::default(),
            frame: None,
            preview_scales: Default::default(),
            preview_densities: Default::default(),
            preview_demand_changed: false,
            scope_images: None,
            scope_snapshot: None,
            scope_key: None,
            viewports: Default::default(),
            navigator: navigator::Navigator::new(rs.clone(), cc.egui_ctx.clone()),
            revision: 0,
            busy: false,
            memory_key: Vec::new(),
            memory_reply_epoch: 0,
            exporting: false,
            preferences,
            activity: Default::default(),
            srgb_target: rs.target_format.is_srgb(),
            display,
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
        if timing::requested_mode().is_some() {
            let state = app.diagnostic_state();
            app.diagnostics.set_start_state(state);
        }
        if let Some((path, mode)) = app.preferences.startup_request(path) {
            app.folder_loader.request(path, mode);
        }
        if let Some(began) = startup_began {
            timing::emit(
                "startup",
                || serde_json::json!({"phase":"app_constructed", "elapsed_ms": began.elapsed().as_secs_f64() * 1000.}),
            );
            app.startup_ready = Some(std::time::Instant::now());
        }
        Ok(app)
    }
    fn open(&mut self, path: PathBuf, mode: FolderMode) {
        self.folder_loader
            .request(path, folder_loader::Mode::Fixed(mode));
    }
    fn receive_folder(&mut self) {
        if self.exporting {
            return;
        }
        let Some(reply) = self.folder_loader.receive() else {
            return;
        };
        let path = reply.path.clone();
        match self.browser.finish_open(reply) {
            Ok((selected, mode, canonical)) => {
                // The old source-write policy stays in force until discovery succeeds.
                self.browser.import_mode = mode == FolderMode::Import;
                self.sidecars.set_memory_only(self.browser.import_mode);
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
        self.revision += 1;
        self.selected = None;
        self.edit_selection
            .sync_browser(&[], self.preferences.multi_edit_limit);
        self.multi_edit.clear();
        self.opening = None;
        self.sidecars.clear();
        self.sidecars.resolve_settings(&[]);
        self.capture.select(None);
        self.metadata = None;
        self.frame = None;
        self.cancel_smart();
        self.scope_images = None;
        self.scope_snapshot = None;
        self.scope_key = None;
        self.busy = false;
        self.worker.clear_all();
        self.viewport_mut().reset();
        viewport::clear(&self.render_state);
    }
    fn reusable_photograph(
        &self,
        path: &std::path::Path,
        frame: &worker::Frame,
        settings: &Settings,
    ) -> bool {
        let settings = self.preferences.render_settings(settings);
        frame.path == path
            && frame.is_photographic()
            && frame.requested.processing_quality == self.preferences.processing_quality
            && matching::same_authored_edits(&frame.requested, &settings)
            && frame.calibration_signature.as_deref()
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

    pub(crate) fn restore_photograph(&mut self, presentation_revision: u64) {
        if self.before.active != editor::Rendition::Current
            || self.output != "final"
            || self.comparison.requested()
            || self.framing_preview_active()
            || self
                .frame
                .as_ref()
                .is_some_and(worker::Frame::is_photographic)
        {
            return;
        }
        let Some(path) = self.selected.clone() else {
            return;
        };
        if !self
            .multi_edit
            .frame(&path)
            .is_some_and(|frame| self.reusable_photograph(&path, frame, &self.view_settings()))
        {
            return;
        }
        let mut frame = self.multi_edit.take_frame(&path).unwrap();
        frame.revision = presentation_revision;
        self.worker.delivered(presentation_revision);
        self.scope_key = None;
        viewport::clear(&self.render_state);
        self.frame = Some(frame);
    }

    fn select(&mut self, path: PathBuf, preserve_viewport: bool) {
        let carried = preserve_viewport.then(|| self.viewport().transform());
        if self.selected.as_ref() != Some(&path) || carried.is_some() {
            self.viewports.select(&path, carried);
        }
        self.multi_edit.selected(&path);
        if let Some(frame) = self.frame.take() {
            self.multi_edit.retain(frame);
        }
        let cached_frame = self.multi_edit.take_frame(&path);
        let cached_metadata = self.multi_edit.metadata(&path);
        let cached_settings = self.multi_edit.settings(&path).cloned();
        let cached_selection = cached_frame.as_ref().is_some_and(|frame| {
            cached_settings
                .as_ref()
                .is_some_and(|settings| self.reusable_photograph(&path, frame, settings))
        });
        let retain_current = self.selected.as_ref() == Some(&path) && cached_selection;
        if let Some(previous) = &self.selected {
            self.worker.retire_before(previous);
        }
        self.worker.activate(&path);
        self.before = Default::default();
        self.browser.invalidate_prefetch();
        timing::selected(self.revision + 1, &path);
        let began = std::time::Instant::now();
        self.opening = (!cached_selection).then(|| Opening {
            _priority: self.worker.cache.foreground(),
            began,
        });
        self.capture.select(Some(path.clone()));
        self.sidecars.select(path.clone());
        self.selected = Some(path);
        self.settings = cached_settings.unwrap_or_else(|| Settings::new("", ""));
        self.revision += 1;
        if !self.multi_photo_overview() && !retain_current {
            self.worker.clear();
        }
        if !cached_selection {
            self.worker.progress.begin(self.revision, began);
        }
        self.busy = self.worker.busy();
        self.metadata = cached_metadata;
        self.output = "final".into();
        self.channel = -1;
        if cached_selection {
            if let Some(frame) = &cached_frame {
                self.update_browser_preview(frame);
            }
        }
        self.frame = cached_frame;
        if cached_selection {
            if let Some(frame) = &mut self.frame {
                // Reusing pixels starts a new presentation, while their delivered
                // task identity continues to describe the original work.
                frame.revision = self.revision;
            }
        }
        self.scope_key = None;
        viewport::clear(&self.render_state);
    }
    fn update_browser_preview(&mut self, frame: &worker::Frame) {
        // Selection can change presentation revision without changing pixels.
        let revision = frame.diagnostic_task.revision;
        if self.browser.has_rendered_preview(&frame.path, revision) {
            return;
        }
        if let Some((thumbnail, preview)) = self.previews.snapshot(frame) {
            self.browser
                .rendered_texture(frame.path.clone(), revision, thumbnail, preview);
        }
    }
    fn submit(&mut self, export: Option<export::Export>) {
        if export.is_some() {
            self.exit_before();
        }
        if export.is_none()
            && self.before.active == editor::Rendition::Current
            && self.output == "final"
            && !self.comparison.requested()
        {
            if let Some(path) = &self.selected {
                self.multi_edit.invalidate_photo(path);
            }
            self.submit_photos();
            return;
        }
        if self.before.active != editor::Rendition::Current {
            self.cancel_smart();
        }
        if export.is_none() && self.smart_refinement_active() {
            self.submit_smart_refinement();
        } else {
            self.submit_with_quality(export, None, self.preferences.processing_quality);
        }
    }
    fn submit_request(&mut self, export: Option<export::Export>, region: Option<[u32; 4]>) {
        self.submit_with_quality(export, region, self.preferences.processing_quality);
    }
    fn submit_with_quality(
        &mut self,
        export: Option<export::Export>,
        region: Option<[u32; 4]>,
        quality: ProcessingQuality,
    ) {
        if !self.sidecars.ready() {
            return;
        }
        self.restore_photograph(self.revision + 1);
        self.publish_render_view();
        if let Some(path) = &self.selected {
            self.revision += 1;

            self.busy = true;
            self.exporting = export.is_some();
            timing::requested(self.revision, path, &self.output, &self.settings);
            let request = self
                .active_render_request(self.revision, export, region, quality)
                .unwrap();
            if let Some(opening) = &self.opening {
                self.worker.submit_at(request, opening.began);
            } else {
                self.worker.submit(request);
            }
            self.opening = None;
            self.busy = self.worker.busy() || self.exporting;
        }
    }
    fn active_render_request(
        &self,
        revision: u64,
        export: Option<export::Export>,
        region: Option<[u32; 4]>,
        quality: ProcessingQuality,
    ) -> Option<request::Request> {
        let path = self.selected.as_ref()?;
        let quality = if export.is_some() {
            ProcessingQuality::Quality
        } else {
            rendering::quality::for_view(
                quality,
                self.before.active,
                &self.output,
                self.comparison.requested(),
            )
        };
        let mut requested = self.view_settings();
        requested.processing_quality = quality;
        requested.working_scale = foveonic::render::Scale::NATIVE;
        requested.preview_region = region;
        let preview_scale = if region.is_some() {
            foveonic::render::Scale::NATIVE
        } else {
            self.preview_scale_for(path, quality)
        };
        Some(request::Request {
            detail: region.is_some(),
            framing_preview: export.is_none() && self.framing_preview_active(),
            calibration: self
                .noise_calibration
                .selected(self.preferences.noise_profile),
            rendition: self.before.active,
            revision,
            path: path.clone(),
            settings: requested,
            preview_scale,
            processing_quality: quality,
            output: if self.before.active == editor::Rendition::Current {
                self.output.clone()
            } else {
                "final".into()
            },
            comparison: self.before.active == editor::Rendition::Current
                && self.comparison.requested()
                && foveonic::inspection::comparison_input(
                    &self.settings,
                    &self.output,
                    self.metadata.as_deref(),
                )
                .is_some(),
            export,
        })
    }
    fn sync_render_progress(&mut self, failed_revision: Option<u64>) {
        self.busy = self.worker.busy() || self.exporting;
        self.worker
            .progress
            .settle(self.busy || self.opening.is_some(), failed_revision);
    }

    fn receive(&mut self, ctx: &egui::Context) {
        self.publish_render_view();
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
                    if self
                        .frame
                        .as_ref()
                        .into_iter()
                        .chain(self.detail_frame())
                        .any(|frame| frame.revision == key.revision)
                        && self.scope_key == Some(key)
                    {
                        match result {
                            Ok(data) => {
                                self.scope_images = Some(scopes::Images::new(&data));
                                self.scope_snapshot = Some(worker::ScopeSnapshot { key, data });
                                timing::event(key.revision, "scopes_received");
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
            if reply.exported.is_some() {
                self.exporting = false;
            }
            let update = self.render_result_for_view(&reply);
            let current = update.current;
            self.sync_render_progress((current && reply.result.is_err()).then_some(reply.revision));
            if update.update_photo {
                self.multi_edit.complete(
                    &reply.path,
                    reply.metadata.clone(),
                    reply.numeric_settings.clone(),
                    reply.reset_settings.clone(),
                    &reply.settings,
                );
            }
            let Some(destination) = update.destination else {
                timing::task(&reply.stamp, "result_view_skipped", || {
                    serde_json::json!({
                        "coordinator_had_destination":reply.handling.is_some_and(|handling|handling.destination.is_some()),
                        "current_after_recheck":update.current,
                        "active_photo":self.selected,
                        "overview_visible":self.multi_edit.rendering(&reply.path),
                    })
                });
                // A photo can leave the viewport while its completed edit is in
                // transit. Keep the valid result for an unchanged re-add.
                if update.update_photo {
                    if let Ok(Some(frame)) = reply.result {
                        self.update_browser_preview(&frame);
                        self.multi_edit.retain(frame);
                    }
                }
                continue;
            };
            if self.smoke.is_some() {
                if let Ok(Some(frame)) = &reply.result {
                    let visible = self.current_photo_frame(&frame.path).and_then(|base| {
                        let window = base.display_window(false);
                        self.viewports
                            .view(Some(&frame.path))
                            .visible_pixels()
                            .map(|pixels| {
                                [
                                    pixels.top_left[0] + window[0],
                                    pixels.top_left[1] + window[1],
                                    pixels.bottom_right[0] + 1 + window[0],
                                    pixels.bottom_right[1] + 1 + window[1],
                                ]
                            })
                    });
                    if let Some(smoke) = &mut self.smoke {
                        smoke.observe_admitted(frame, current, destination, visible);
                    }
                }
            }
            if destination == delivery::Destination::Overview {
                match reply.result {
                    Ok(Some(frame)) => {
                        timing::image_received(
                            &reply.stamp,
                            current,
                            frame.queued_at,
                            frame.milliseconds,
                        );
                        timing::task(&reply.stamp, "image_accepted", || {
                            serde_json::json!({"current":current,
                            "destination":"overview","scale":frame.artifact.settings.working_scale.divisor(),
                            "artifact":frame.artifact.name,"extent":frame.texture.description().extent})
                        });
                        self.update_browser_preview(&frame);
                        self.multi_edit.retain(frame);
                    }
                    Err(error) if current => {
                        self.fail(format!("{}: {error}", reply.path.display()))
                    }
                    _ => {}
                }
                continue;
            }
            if current {
                if let Some(metadata) = reply.metadata {
                    self.metadata = Some(metadata);
                }
            }
            match reply.result {
                Ok(Some(frame)) => {
                    self.worker.delivered(frame.revision);
                    if current
                        && frame.rendition == editor::Rendition::Current
                        && frame.artifact.settings.dehaze_estimate
                    {
                        editor::dehaze::set_estimate(
                            ctx,
                            &self.settings,
                            frame.artifact.dehaze_estimate,
                        );
                    }
                    if frame.rendition != editor::Rendition::Current {
                        self.before.retain_current(&mut self.frame);
                    }
                    if frame.rendition == editor::Rendition::Current
                        && !frame.detail
                        && !frame.is_photographic()
                    {
                        if let Some(previous) = self.frame.take() {
                            self.multi_edit.retain(previous);
                        }
                    }
                    timing::image_received(
                        &reply.stamp,
                        current,
                        frame.queued_at,
                        frame.milliseconds,
                    );
                    timing::task(&reply.stamp, "image_accepted", || {
                        serde_json::json!({"current":current,
                        "scale":frame.artifact.settings.working_scale.divisor(),"artifact":frame.artifact.name,
                        "extent":frame.texture.description().extent})
                    });
                    self.update_browser_preview(&frame);
                    // Keep the last completed scopes until this frame has replacements.
                    if !self.loupe_inspecting(&frame.path) {
                        self.scope_key = None;
                    }
                    if frame.detail {
                        if frame.rendition == editor::Rendition::Current {
                            self.multi_edit.retain(frame);
                        } else {
                            self.before.detail = Some(frame);
                        }
                    } else {
                        viewport::clear(&self.render_state);
                        if frame.rendition == editor::Rendition::Current {
                            self.multi_edit.supersede_detail(&frame);
                        }
                        if self
                            .before
                            .detail
                            .as_ref()
                            .is_some_and(|detail| frame.supersedes_detail(detail))
                        {
                            self.before.detail = None;
                        }
                        self.frame = Some(frame);
                    }
                }
                Ok(None) => {
                    if let Some(export) = &reply.exported {
                        self.activity.operation(format!(
                            "Saved {}: {}",
                            export.format.label(),
                            export.path.display()
                        ));
                        self.submit(None);
                    }
                }
                Err(e) => {
                    assert!(
                        self.smoke
                            .as_ref()
                            .is_none_or(|smoke| smoke.expects_failure(&reply.path)),
                        "native smoke processing failed: {e}"
                    );
                    self.fail(e);
                }
            }
        }
    }
    fn update_scopes(&mut self) {
        let window = self
            .frame
            .as_ref()
            .map_or([0; 4], |f| f.display_window(false));
        let visible = self.viewport().visible_pixels().map(|p| {
            [
                p.top_left[0] + window[0],
                p.top_left[1] + window[1],
                p.bottom_right[0] + 1 + window[0],
                p.bottom_right[1] + 1 + window[1],
            ]
        });
        let detail = self.smart_detail().filter(|detail| {
            self.preferences.scope_viewport
                && visible.is_some_and(|visible| smart::contains(detail.detail_region(), visible))
        });
        let Some(frame) = detail.or(self.frame.as_ref()) else {
            self.scope_key = None;
            return;
        };
        let region = if self.preferences.scope_viewport {
            if frame.framing_preview {
                // Drafts only move the texture. Analyze the visible part of the saved framing.
                let foveonic::inspection::Content::Image(image) = &frame.artifact.content else {
                    return;
                };
                let [l, t, r, b] = frame
                    .requested
                    .rotation
                    .framing_bounds(frame.camera_extent(), frame.requested.crop)
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
                Some([
                    pixels.top_left[0],
                    pixels.top_left[1],
                    pixels.bottom_right[0] + 1,
                    pixels.bottom_right[1] + 1,
                ])
            }
        } else {
            None
        };
        let key = worker::ScopeKey {
            revision: frame.revision,
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
            photo: foveonic::diagnostics::photo_id(&frame.path),
            parent_render_task: frame.diagnostic_task.id,
            parent_render_revision: frame.diagnostic_task.revision,
        });
        self.scope_key = Some(key);
    }
    fn process(&mut self) {
        // Freeze edits before either executor receives the job.
        self.sidecars.refresh(&self.settings);
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
                let export_ready = self.sidecars.ready()
                    && self.selected.is_some()
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
                        enabled: !self.exporting,
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
            .frame
            .as_ref()
            .filter(|f| {
                self.selected.as_ref() == Some(&f.path)
                    && f.output == "final"
                    && f.rendition == editor::Rendition::Current
                    && self.output == "final"
                    && self.before.active == editor::Rendition::Current
                    && !self.comparison.requested()
                    && self.sidecars.ready()
                    && self.edit_selection.contains(&f.path)
            })
            .map(|frame| self.settings.rotation.extent(frame.camera_extent()));
        photo_tools::show(
            ui,
            !self.exporting,
            &mut self.settings.crop,
            &mut self.settings.rotation,
            extent,
            &mut self.preferences.crop_ratios,
            &mut self.preferences.loupe,
        )
    }
    fn section_body(&mut self, ui: &mut egui::Ui, section: Section) -> bool {
        match section {
            Section::Navigator => {
                let mapping = self.displayed_mapping();
                let texture = self
                    .frame
                    .as_ref()
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
                    !self.exporting
                        && self.sidecars.ready()
                        && !self.edit_selection.paths().is_empty()
                        && (!self.multi_edit.multiple() || self.multi_edit.editable_ready()),
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
                                    .filter(|_| !self.exporting && self.sidecars.ready()),
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
    fn displayed_channel(&self) -> i32 {
        if self.before.active != editor::Rendition::Current {
            return -1;
        }
        // The displayed image may belong to an older request. Using the new
        // component choice now could show its samples with the wrong meaning.
        if self.frame.as_ref().is_some_and(|f| f.output == self.output) {
            self.channel
        } else {
            -1
        }
    }
    fn displayed_mapping(&self) -> viewport::Mapping {
        if self.before.active != editor::Rendition::Current {
            return viewport::Mapping::normal(-1);
        }
        let channel = self.displayed_channel();
        let has_pair = self
            .frame
            .as_ref()
            .is_some_and(|frame| frame.output == self.output && frame.comparison.is_some());
        let mut mapping = self.comparison.mapping(channel, has_pair);
        if let Some(frame) = &self.frame {
            let description = frame.texture.description();
            if matches!(description.space, foveonic::image::Space::LinearRgb(_)) {
                mapping.signal_scale = 4095. / description.linear_white as f32;
            }
        }
        mapping
    }
    fn central(&mut self, ui: &mut egui::Ui) -> bool {
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
                changed |= inspect::processing_quality(
                    ui,
                    !self.exporting,
                    &mut self.preferences.processing_quality,
                );
                widgets::button_group(ui, |ui| {
                    widgets::centered_control_row(ui, |ui| {
                        ui.weak("Inspect");
                        ui.add_enabled_ui(!self.exporting, |ui| {
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
                            let has_pair = self.frame.as_ref().is_some_and(|frame| {
                                frame.output == self.output && frame.comparison.is_some()
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
            let frame = self.frame.as_ref().unwrap();
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
                    frame.output,
                    range / mapping.difference_gain,
                ))
                .wrap(),
            );
        } else {
            let name = self
                .frame
                .as_ref()
                .map_or("preview", |frame| frame.output.as_str());
            let label = if self.comparison.difference {
                Some(format!("Difference pending. Showing {name}."))
            } else if self.comparison.input {
                Some(if mapping.input {
                    let input = self
                        .frame
                        .as_ref()
                        .unwrap()
                        .comparison
                        .as_ref()
                        .unwrap()
                        .input;
                    format!("Input · {input} · scopes: output")
                } else {
                    format!("Input pending. Showing {name}.")
                })
            } else if self.frame.as_ref().is_some_and(|frame| {
                foveonic::inspection::comparison_input(
                    &frame.artifact.settings,
                    &frame.output,
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
                let paths = self.viewport_paths.clone();
                self.displayed_paths.clear();
                if self.multi_photo_overview() {
                    changed |= self.comparison_view(ui, &paths);
                } else {
                    self.displayed_paths.extend(self.selected.iter().cloned());
                    changed |= self.show_photo(ui, mapping);
                    self.before_badge(ui, canvas);
                }
                self.show_loupe(ui, mapping);
            },
        );
        let path = self
            .frame
            .as_ref()
            .map(|f| &f.path)
            .or(self.selected.as_ref());
        let filename = path
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        let properties = self
            .frame
            .as_ref()
            .map(|f| &f.artifact.metadata.properties)
            .or_else(|| self.metadata.as_ref().map(|metadata| &metadata.properties))
            .or(self.capture.properties.as_ref());
        information::show(ui, properties, &filename, self.viewport().visible_pixels());
        changed
    }

    fn photo_interaction_available(&self, mapping: viewport::Mapping) -> bool {
        !self.exporting
            && self
                .selected
                .as_deref()
                .is_some_and(|path| self.edit_selection.contains(path))
            && self.sidecars.ready()
            && self.before.active == editor::Rendition::Current
            && self.output == "final"
            && !mapping.input
            && mapping.difference_gain == 0.
            && self.frame.as_ref().is_some_and(|f| {
                !f.framing_preview
                    && self.selected.as_ref() == Some(&f.path)
                    && f.output == "final"
                    && f.rendition == editor::Rendition::Current
            })
    }

    fn show_photo(&mut self, ui: &mut egui::Ui, mapping: viewport::Mapping) -> bool {
        let allowed = self.photo_interaction_available(mapping);
        let framing_active =
            self.framing_preview_active() && !self.exporting && self.sidecars.ready();
        let crop_active = framing_active && crop_tool::active(ui.ctx());
        self.viewport_mut().framing = self
            .frame
            .as_ref()
            .filter(|f| framing_active && f.framing_preview)
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
        if let Some(frame) = &self.frame {
            let detail = self.smart_detail().map(|detail| viewport::ImageLayer {
                texture: detail.texture.clone(),
                coverage: detail.detail_region(),
                identity: detail.diagnostic_identity(false),
            });
            let viewport = self.viewports.view_mut(self.selected.as_deref());
            viewport.zoom_snap = self.preferences.zoom_snap;
            viewport.show(
                ui,
                frame.view(mapping.input).clone(),
                viewport.framing.map_or_else(
                    || frame.display_extent(mapping.input),
                    framing::Presentation::extent,
                ),
                frame.comparison.as_ref().map(|c| c.texture.clone()),
                mapping,
                self.srgb_target,
                frame.diagnostic_identity(mapping.input),
            );
            if let Some(layer) = detail {
                viewport.show_region(
                    ui,
                    layer,
                    frame.display_window(false),
                    mapping,
                    self.srgb_target,
                );
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
        if self.output == "final"
            && !self.comparison.requested()
            && self
                .frame
                .as_ref()
                .is_none_or(|frame| frame.output == "final")
            && (!framing_active || self.viewport().explicit_zoom_pending())
        {
            let density = self
                .frame
                .as_ref()
                .filter(|frame| frame.output == "final")
                .and_then(|frame| {
                    self.viewport().image_rectangles().map(|(_, image)| {
                        let extent = frame.native_display_extent();
                        rendering::quality::display_density(
                            [image.width(), image.height()],
                            extent,
                            ui.ctx().pixels_per_point(),
                        )
                    })
                })
                .or_else(|| {
                    self.viewport()
                        .zoom_percent(ui.ctx().pixels_per_point())
                        .map(|percent| percent / 100.)
                });
            if let Some((path, density)) = self.selected.clone().zip(density) {
                self.observe_preview_density(&path, density);
            }
        }
        if let (Some(path), Some((response, uv))) = (
            &self.selected,
            self.viewport()
                .pointer(ui.input(|i| i.pointer.interact_pos())),
        ) {
            if framing_active {
                if let (Some(frame), Some((canvas, image))) = (
                    self.frame
                        .as_ref()
                        .filter(|f| f.framing_preview && &f.path == path),
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
                self.frame
                    .as_ref()
                    .is_some_and(|f| f.requested == self.settings)
            });
            let (changed, request) =
                photo_tools::pick(ui, &response, uv, path, &mut self.settings, allowed);
            if let Some(request) = request {
                if let Some(frame) = &self.frame {
                    self.worker
                        .pick_color(request, frame.revision, Some(frame.origin.clone()));
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
        let _update = timing::Span::new(self.revision, "ui_update");
        if let Some(ready) = self.startup_ready.take() {
            timing::emit(
                "startup",
                || serde_json::json!({"phase":"first_update", "since_constructed_ms": ready.elapsed().as_secs_f64() * 1000.}),
            );
        }
        self.activity.observe(
            activity::Origin::Display,
            self.display
                .refresh(frame)
                .err()
                .map(|error| format!("Display color management: {error}")),
        );
        let began = std::time::Instant::now();
        if timing::enabled() {
            let gap = self
                .last_frame
                .replace(began)
                .map(|last| began.duration_since(last).as_secs_f64() * 1000.);
            timing::native_frame(
                self.frame.as_ref().map_or(0, |image| image.revision),
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
        if let Some(saved) = self.sidecars.receive() {
            if let Some(path) = &self.selected {
                if let Some(opening) = &self.opening {
                    timing::selection_ready(self.revision, path, opening.began);
                }
                self.settings = sidecars::selection_settings(saved, &self.camera_overrides);
                inspect::normalize(
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
                let retained_current = self.frame.as_ref().is_some_and(|frame| {
                    frame.path == *path
                        && frame.rendition == editor::Rendition::Current
                        && frame.output == "final"
                        && self.output == "final"
                        && frame.framing_preview == self.framing_preview_active()
                        && matching::same_authored_edits(&frame.requested, &self.view_settings())
                        && self
                            .worker
                            .cache
                            .source_origin(path)
                            .is_none_or(|origin| origin == frame.origin)
                        && self.worker.is_settled_and_admitted(frame)
                });
                if !retained_current {
                    self.submit(None);
                }
            }
        }
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
        if !self.exporting && self.noise_calibration.take_changed() {
            self.multi_edit.invalidate();
            self.multi_edit.clear_details();
            self.before.detail = None;
            self.cancel_smart();
            let selected = self
                .noise_calibration
                .selected(self.preferences.noise_profile);
            self.worker
                .cache
                .configure_noise_calibration(selected.as_ref().map(|p| p.signature().to_owned()));
            self.import.set_noise_calibration(selected);
            self.browser.invalidate_previews();
            self.previews.invalidate();
            self.submit_request(None, None);
        }
        self.smoke_tick(ctx);
        automation::begin_frame(ctx);
        let normalized = controls::normalize_with_capture(
            &mut self.settings,
            self.metadata.as_deref(),
            self.capture.properties.as_ref(),
        );
        self.sidecars.refresh(&self.settings);
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
            self.display.target.managed(),
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
                        serde_json::json!({"source":self.selected,"revision":self.revision}),
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
                self.browser.header(
                    ui,
                    &mut self.preferences.organizer,
                    !self.exporting,
                    browser_focus,
                );
                ui.separator();
                ui.add_enabled_ui(!self.exporting, |ui| {
                    select = self.browser.organizer(
                        ui,
                        self.selected.as_deref(),
                        self.preferences.organizer,
                    );
                });
                self.preferences.browser_sort = self.browser.sort;
                self.preferences.browser_show_ratings = self.browser.show_ratings;
            });
        let actions_changed = self.dispatch_actions(ctx);
        let tuning_changed = previous_limits != self.preferences.adaptive_tone_tuning;
        if tuning_changed {
            self.multi_edit.invalidate();
            self.multi_edit.clear_details();
            self.before.detail = None;
            self.browser.invalidate_previews();
            self.previews.invalidate();
        }
        let mut changed = normalized || actions_changed || tuning_changed || toolbar_changed;
        self.sync_viewport_paths();
        if self
            .selected
            .as_deref()
            .is_some_and(|path| !self.edit_selection.contains(path))
        {
            if let Some(path) = self.edit_selection.paths().first().cloned() {
                self.select(path, false);
            }
        }
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
        if self.multi_edit.multiple() && self.multi_edit.editable_ready() {
            let assignments =
                multi_edit_controls::assignments(&editor_before, &self.settings, &edits);
            edits.extend(assignments);
            if self.multi_edit.apply_edits(&edits, &mut self.sidecars) {
                if let Some(settings) = self
                    .selected
                    .as_deref()
                    .and_then(|path| self.multi_edit.settings(path))
                {
                    self.settings = settings.clone();
                }
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
                    self.settings = settings.clone();
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
        if self.sidecars.ready() {
            if let Some(path) = &self.selected {
                self.multi_edit
                    .observe_active(path, &self.settings, self.metadata.as_ref());
            }
        }
        if previous_selection == self.selected && (normalized || previous_settings != self.settings)
        {
            self.exit_before();
            self.sidecars.edited(&self.settings);
        }
        self.preferences.direct_color = editor::direct_color::configuration(ctx);
        let preview_changed = std::mem::take(&mut self.preview_demand_changed);
        let mut submit_active = false;
        if std::mem::take(&mut self.browser.clear_selection) {
            self.clear_selection();
        } else if let Some(request) = self.comparison_selection.take() {
            let clicked = request.path.clone();
            let selection_changed = self.edit_selection.apply_displayed(
                request,
                self.preferences.multi_edit_limit,
                &self.displayed_paths,
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
        } else if (changed || preview_changed) && !self.exporting {
            if changed {
                self.exit_before();
            }
            submit_active = true;
        }
        if submit_active {
            self.submit(None);
        }
        self.submit_photos();
        self.update_loupe();
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
                overrides: self.camera_overrides.clone(),
                calibration: self
                    .noise_calibration
                    .selected(self.preferences.noise_profile),
            });
        self.editor_ui.dialog(ctx, &mut self.preferences.editor);
        self.activity.show(ctx);
        if !self.multi_photo_overview() {
            self.update_smart();
        }
        self.update_scopes();
        self.smoke_check_scope_retention();
        self.sync_render_progress(None);
        self.previews.update(self.frame.as_ref(), self.busy, ctx);
        self.navigator.update(
            self.frame.as_ref(),
            self.displayed_mapping(),
            self.busy,
            ctx,
        );
        self.update_memory();
        self.publish_render_view();
    }
}
fn multi_photo_overview(paths: &[PathBuf], active: Option<&std::path::Path>) -> bool {
    paths.len() > 1 && active.is_some_and(|active| paths.iter().any(|path| path == active))
}
#[cfg(target_os = "windows")]
fn main() {
    if let Err(error) = run() {
        show_startup_error(&error.to_string());
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "windows"))]
fn main() -> eframe::Result {
    run()
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
    timing::configure().map_err(eframe::Error::AppCreation)?;
    let mut args = std::env::args().skip(1);
    let mut path = None;
    let mut settings = Settings::new("", "");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--wb" => settings.white_balance = args.next().unwrap_or_default(),
            "--mode" => settings.color_mode = args.next().unwrap_or_default(),
            "--help" => {
                println!(
                    "foveonic-gui [X3F_OR_FOLDER] [--wb NAME --mode NAME]\nWB and color mode default to the loaded X3F's recorded settings; --wb/--mode override them."
                );
                return Ok(());
            }
            _ => {
                if path.is_some() || arg.starts_with('-') {
                    let message = format!("Unexpected argument: {arg}");
                    eprintln!("{message}");
                    #[cfg(target_os = "windows")]
                    show_startup_error(&message);
                    std::process::exit(2);
                }
                path = Some(PathBuf::from(arg));
            }
        }
    }
    #[cfg(target_os = "windows")]
    let instance_descriptor = wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
        backend_options: wgpu::BackendOptions {
            dx12: wgpu::Dx12BackendOptions {
                shader_compiler: wgpu::Dx12Compiler::default_dynamic_dxc(),
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let setup = egui_wgpu::WgpuSetupCreateNew {
        #[cfg(target_os = "windows")]
        instance_descriptor,
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
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        persistence_path: std::env::var_os("FOVEONIC_GUI_SMOKE_DIR")
            .map(PathBuf::from)
            .map(|d| d.join("app.ron")),
        viewport,
        event_loop_builder,
        wgpu_options: display::configure(egui_wgpu::WgpuConfiguration {
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup),
            on_paint_stage: Some(Arc::new(timing::paint_stage)),
            ..Default::default()
        }),
        ..Default::default()
    };
    eframe::run_native(
        preferences::APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, path, settings)?))),
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
    #[test]
    fn image_defaults_and_saved_recipes_do_not_carry_other_photo_edits() {
        let mut saved = foveonic::render::Settings::new("Shade", "Vivid");
        saved.exposure = 1.;
        let overrides = [String::new(), String::new()];
        assert_eq!(
            super::sidecars::selection_settings(saved.clone(), &overrides),
            saved
        );
        let fresh = super::sidecars::selection_settings(
            foveonic::render::Settings::new("", ""),
            &overrides,
        );
        assert_eq!(fresh.exposure, 0.);
        assert!(fresh.white_balance.is_empty() && fresh.color_mode.is_empty());
        let explicit =
            super::sidecars::selection_settings(saved, &["Auto".into(), "Neutral".into()]);
        assert_eq!(explicit.white_balance, "Auto");
        assert_eq!(explicit.color_mode, "Neutral");
        assert_eq!(explicit.exposure, 1.);
    }
}
