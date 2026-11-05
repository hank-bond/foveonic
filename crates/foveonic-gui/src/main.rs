//! Runs a three-pane photographic workspace with current-buffer inspection, separate UI preferences and per-X3F editing recipes.

mod actions;
mod activity;
mod application_settings;
mod batch;
mod browser;
mod capture;
mod chromaticity;
mod comparison_view;
mod controls;
mod crop_tool;
mod display;
mod display_image;
mod editor;
mod export;
mod folder_loader;
mod help;
mod help_examples;
mod icon;
mod import_review;
mod information;
mod inspect;
mod metadata;
mod mixer;
mod navigator;
mod noise_calibration;
mod photo_tools;
mod preferences;
mod preview_image;
mod previews;
mod processing;
mod progress;
mod scopes;
mod sections;
mod shortcuts;
mod sidecars;
mod smart;
mod smoke;
mod timing;
mod toolbar;
mod viewport;
mod widgets;
mod worker;

use eframe::{egui, egui_wgpu, wgpu};
use foveonic::{
    render::{Engine, Settings},
    source::Metadata,
};
use preferences::{FolderMode, Section};
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
    noise_calibration: noise_calibration::State,
    comparison_selection: Option<PathBuf>,
    comparison_zoom: f32,
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
    detail: Option<worker::Frame>,
    smart: smart::Controller,
    scope_images: Option<scopes::Images>,
    scope_snapshot: Option<worker::ScopeSnapshot>,
    scope_key: Option<worker::ScopeKey>,
    viewport: viewport::Viewport,
    navigator: navigator::Navigator,
    previews: previews::Previews,
    revision: u64,
    busy: bool,
    exporting: bool,
    preferences: preferences::Preferences,
    status: String,
    activity: activity::Log,
    srgb_target: bool,
    display: display::Host,
    render_state: egui_wgpu::RenderState,
    smoke: Option<smoke::Smoke>,
    last_frame: Option<std::time::Instant>,
    startup_ready: Option<std::time::Instant>,
}
impl App {
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
        timing::emit(
            "session",
            serde_json::json!({"frontend":"gui","adapter":rs.adapter.get_info().name,"gpu_supported":rs.adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY),"gpu_enabled":rs.device.features().contains(wgpu::Features::TIMESTAMP_QUERY)}),
        );
        let display = display::Host::new(cc)?;
        timing::emit(
            "display",
            serde_json::json!({
                "format": format!("{:?}", rs.target_format),
                "encoding": format!("{:?}", display.target),
                "managed": display.target.managed(),
            }),
        );
        viewport::install(rs);
        widgets::install_style(&cc.egui_ctx);
        if smoke::screenshot_active() {
            cc.egui_ctx.set_theme(egui::Theme::Dark);
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
        editor::direct_color::configure(&cc.egui_ctx, preferences.direct_color);
        if std::env::var_os("FOVEONIC_GUI_SMOKE_SMART").is_some() {
            preferences.preview_quality = preferences::PreviewMode::Smart;
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
            noise_calibration,
            comparison_selection: None,
            comparison_zoom: 1.,
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
                std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").is_none(),
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
            detail: None,
            smart: Default::default(),
            scope_images: None,
            scope_snapshot: None,
            scope_key: None,
            viewport: Default::default(),
            navigator: navigator::Navigator::new(rs.clone(), cc.egui_ctx.clone()),
            revision: 0,
            busy: false,
            exporting: false,
            preferences,
            status: "Open a folder of X3F photographs".into(),
            activity: Default::default(),
            srgb_target: rs.target_format.is_srgb(),
            display,
            render_state: rs.clone(),
            smoke: smoke::Smoke::from_env(),
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
            .set_thumbnail_budget(app.preferences.thumbnail_cache_bytes);
        app.sidecars.set_defaults(&app.preferences.editor.defaults);
        app.worker
            .cache
            .configure_photo_defaults(&app.preferences.editor.defaults);
        app.import.set_defaults(
            &app.preferences.editor.defaults,
            &mut app.browser,
            app.preferences.processing_destination.as_deref(),
        );
        app.viewport.zoom_snap = app.preferences.zoom_snap;
        if let Some((path, mode)) = app.preferences.startup_request(path) {
            app.folder_loader.request(path, mode);
        }
        if let Some(began) = startup_began {
            timing::emit(
                "startup",
                serde_json::json!({"phase":"app_constructed", "elapsed_ms": began.elapsed().as_secs_f64() * 1000., "profile": startup_profile}),
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
                    self.status = "No X3F files in this folder".into();
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
        self.opening = None;
        self.sidecars.clear();
        self.capture.select(None);
        self.metadata = None;
        self.frame = None;
        self.detail = None;
        self.smart.reset();
        self.scope_images = None;
        self.scope_snapshot = None;
        self.scope_key = None;
        self.busy = false;
        self.worker.clear();
        self.viewport.reset();
        viewport::clear(&self.render_state);
    }
    fn select(&mut self, path: PathBuf, preserve_viewport: bool) {
        self.before = Default::default();
        self.browser.invalidate_prefetch();
        timing::selected(self.revision + 1, &path);
        let began = std::time::Instant::now();
        self.opening = Some(Opening {
            _priority: self.worker.cache.foreground(),
            began,
        });
        self.capture.select(Some(path.clone()));
        self.sidecars.select(path.clone());
        self.selected = Some(path);
        self.settings = Settings::new("", "");
        self.revision += 1;
        self.worker.clear();
        self.worker.progress.begin(self.revision, began);
        self.busy = false;
        self.status = "Loading editing recipe…".into();
        self.metadata = None;
        self.output = "final".into();
        self.channel = -1;
        self.frame = None;
        self.detail = None;
        self.smart.reset();
        self.scope_key = None;
        self.viewport.begin_image(preserve_viewport);
        viewport::clear(&self.render_state);
    }
    fn submit(&mut self, export: Option<export::Export>) {
        if export.is_some() {
            self.exit_before();
        }
        self.detail = None;
        self.smart.reset();
        let request = if export.is_none() {
            self.smart_edit_request()
        } else {
            None
        };
        let (quality, region) = request.unwrap_or((self.preferences.preview_quality, None));
        self.submit_with_quality(export, region, quality);
    }
    fn submit_request(&mut self, export: Option<export::Export>, region: Option<[u32; 4]>) {
        self.submit_with_quality(export, region, self.preferences.preview_quality);
    }
    fn submit_with_quality(
        &mut self,
        export: Option<export::Export>,
        region: Option<[u32; 4]>,
        quality: preferences::PreviewMode,
    ) {
        if !self.sidecars.ready() {
            return;
        }
        if let Some(path) = &self.selected {
            self.revision += 1;

            self.busy = true;
            self.exporting = export.is_some();
            self.status = if let Some(export) = &export {
                format!("Saving {}…", export.format.label())
            } else if self.frame.is_some() {
                "Updating photo…".into()
            } else {
                "Loading photo…".into()
            };
            timing::requested(self.revision, path, &self.output, &self.settings);
            let mut requested = self.view_settings();
            requested.preview_region = region;
            let request = worker::Request {
                crop_preview: export.is_none() && self.crop_preview_active(),
                calibration: self
                    .noise_calibration
                    .selected(self.preferences.noise_profile),
                rendition: self.before.active,
                revision: self.revision,
                path: path.clone(),
                settings: requested,
                preview_quality: quality,
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
            };
            if let Some(opening) = &self.opening {
                self.worker.submit_at(request, opening.began);
            } else {
                self.worker.submit(request);
            }
            self.opening = None;
        }
    }
    fn receive(&mut self, ctx: &egui::Context) {
        self.browser.receive(ctx);
        while let Ok(preview) = self.previews.replies.try_recv() {
            if self.frame.as_ref().is_some_and(|frame| {
                frame.path == preview.path
                    && frame.revision == preview.revision
                    && preview.calibration_signature.as_deref()
                        == self
                            .noise_calibration
                            .selected(self.preferences.noise_profile)
                            .as_ref()
                            .map(|p| p.signature())
            }) {
                self.browser
                    .rendered(ctx, preview.path, preview.revision, preview.image);
            }
        }
        while let Ok(update) = self.worker.replies.try_recv() {
            let reply = match update {
                worker::Update::Image(reply) => *reply,
                worker::Update::ColorPick(reply) => {
                    if !editor::dehaze::receive(ctx, &reply) {
                        editor::direct_color::receive(ctx, reply);
                    }
                    continue;
                }
                worker::Update::Scopes { key, result } => {
                    if self
                        .frame
                        .as_ref()
                        .into_iter()
                        .chain(self.detail.as_ref())
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
            if !self.exporting
                && reply
                    .result
                    .as_ref()
                    .ok()
                    .and_then(Option::as_ref)
                    .is_some_and(|frame| {
                        frame.calibration_signature.as_deref()
                            != self
                                .noise_calibration
                                .selected(self.preferences.noise_profile)
                                .as_ref()
                                .map(|p| p.signature())
                    })
            {
                continue;
            }
            if reply
                .result
                .as_ref()
                .ok()
                .and_then(Option::as_ref)
                .is_some_and(|frame| frame.crop_preview != self.crop_preview_active())
            {
                continue;
            }
            let current = reply.revision == self.revision;
            if reply.rendition != self.before.active {
                continue;
            }
            if !current
                && reply
                    .result
                    .as_ref()
                    .ok()
                    .and_then(Option::as_ref)
                    .is_some_and(|frame| frame.artifact.settings.preview_region.is_some())
            {
                continue;
            }
            if !current
                && !reply
                    .result
                    .as_ref()
                    .ok()
                    .and_then(Option::as_ref)
                    .is_some_and(|frame| {
                        !self.exporting
                            && self.selected.as_ref() == Some(&frame.path)
                            && self.output == frame.output
                            && (!self.comparison.requested() || frame.comparison.is_some())
                            && self
                                .frame
                                .as_ref()
                                .is_none_or(|displayed| frame.revision > displayed.revision)
                    })
            {
                continue;
            }
            if current {
                self.worker
                    .progress
                    .finish(reply.revision, reply.result.is_err());
                self.busy = false;
                self.exporting = false;
            }
            if let Some(metadata) = reply.metadata {
                self.metadata = Some(metadata);
                if current && reply.rendition == editor::Rendition::Current {
                    // Authored As Shot choices must keep following each photograph.
                    // Render replies carry concrete camera choices for the GPU recipe.
                    let white_balance = std::mem::take(&mut self.settings.white_balance);
                    let color_mode = std::mem::take(&mut self.settings.color_mode);
                    self.settings = reply.settings;
                    self.settings.white_balance = white_balance;
                    self.settings.color_mode = color_mode;
                }
            }
            match reply.result {
                Ok(Some(frame)) => {
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
                        self.before
                            .retain_current(&mut self.frame, &mut self.detail);
                    }
                    timing::event(frame.revision, "image_received");
                    self.status.clear();
                    if self.smoke.is_some() {
                        eprintln!("{}: {:.0} ms", frame.artifact.name, frame.milliseconds);
                    }
                    if !current {
                        self.status = "Updating photo…".into();
                    }
                    timing::image_received(
                        frame.revision,
                        current,
                        frame.queued_at,
                        frame.milliseconds,
                    );
                    // Keep the last completed scopes until this frame has replacements.
                    self.scope_key = None;
                    if frame.artifact.settings.preview_region.is_some() {
                        self.detail = Some(frame);
                    } else {
                        viewport::clear(&self.render_state);
                        if self.preferences.preview_quality != preferences::PreviewMode::Smart
                            || frame.artifact.settings.preview_quality
                                != foveonic::render::PreviewQuality::Full
                        {
                            self.detail = None;
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
                    assert!(self.smoke.is_none(), "native smoke processing failed: {e}");
                    self.fail(e);
                }
            }
        }
    }
    fn update_scopes(&mut self, ctx: &egui::Context) {
        let window = self
            .frame
            .as_ref()
            .map_or([0; 4], |f| f.display_window(false));
        let visible = self.viewport.visible_pixels().map(|p| {
            [
                p.top_left[0] + window[0],
                p.top_left[1] + window[1],
                p.bottom_right[0] + 1 + window[0],
                p.bottom_right[1] + 1 + window[1],
            ]
        });
        let detail = self.smart_detail(ctx.pixels_per_point()).filter(|detail| {
            self.preferences.scope_viewport
                && visible.is_some_and(|visible| {
                    smart::contains(detail.artifact.settings.preview_region.unwrap(), visible)
                })
        });
        let Some(frame) = detail.or(self.frame.as_ref()) else {
            self.scope_key = None;
            return;
        };
        let region = if self.preferences.scope_viewport {
            if let Some(rect) = frame.artifact.settings.preview_region {
                let visible = visible.unwrap();
                Some([
                    visible[0] - rect[0],
                    visible[1] - rect[1],
                    visible[2] - rect[0],
                    visible[3] - rect[1],
                ])
            } else {
                let Some(pixels) = self
                    .viewport
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
            image: if frame.crop_preview {
                image.without_crop()
            } else {
                (**image).clone()
            },
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
                let process_ready = self.import.ready(&self.browser)
                    && (self.browser.import_mode || !only_current || export_ready);
                let actions = toolbar::show(
                    ui,
                    toolbar::State {
                        enabled: !self.exporting,
                        process_ready,
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
        self.status = "Operation failed".into();
    }
    fn toolbar_status(&mut self, ui: &mut egui::Ui) {
        let processing = self.import.status(ui.ctx(), &mut self.activity);
        self.activity
            .observe(activity::Origin::Edits, self.sidecars.warnings());
        self.activity.button(ui);
        if self.sidecars.warning().is_some()
            && self.sidecars.can_retry()
            && widgets::button(ui, "Retry saving edits", false, egui::Vec2::ZERO).clicked()
        {
            self.sidecars.retry();
        }
        let message = if self.sidecars.warning().is_some() {
            "Edits need attention"
        } else if self.folder_loader.loading() {
            "Opening folder…"
        } else {
            processing.as_deref().unwrap_or(&self.status)
        };
        toolbar::status(ui, message);
    }

    fn crop_preview_active(&self) -> bool {
        crop_tool::active(&self.context)
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
            })
            .map(worker::Frame::uncropped_extent);
        photo_tools::show(
            ui,
            !self.exporting,
            &mut self.settings.crop,
            extent,
            &mut self.preferences.crop_ratios,
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
                self.viewport.navigator(ui, texture, preview.as_ref());
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
                ui.add_enabled_ui(!self.exporting && self.sidecars.ready(), |ui| {
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
                });
                changed
            }
            Section::Metadata => {
                metadata::show(ui, self.metadata.as_deref());
                false
            }
        }
    }
    fn tools(&mut self, ui: &mut egui::Ui) -> bool {
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
        let mut changed = false;
        // Reserve both groups before drawing so later dropdowns cannot shift Preview vertically.
        let group_height = widgets::button_group_height(ui);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), group_height),
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
            |ui| {
                changed |= inspect::preview_quality(
                    ui,
                    !self.exporting,
                    if self.output != "final" || self.comparison.requested() {
                        Some(false)
                    } else {
                        inspect::preview_support(
                            &self.settings,
                            self.metadata.as_deref(),
                            self.capture.properties.as_ref(),
                        )
                    },
                    &mut self.preferences.preview_quality,
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
        );
        if crop_tool::active(ui.ctx()) && !self.crop_preview_active() {
            crop_tool::exit(ui.ctx());
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
                let paths = self.browser.processing_selection(self.selected.as_deref());
                if paths.len() > 1
                    && self
                        .selected
                        .as_ref()
                        .is_some_and(|active| paths.contains(active))
                {
                    changed |= self.comparison_view(ui, &paths);
                    return;
                }
                let canvas = egui::Rect::from_min_size(
                    ui.cursor().min,
                    egui::vec2(ui.available_width(), height),
                );
                changed |= self.show_photo(ui, mapping);
                self.before_badge(ui, canvas);
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
        information::show(ui, properties, &filename, self.viewport.visible_pixels());
        changed
    }

    fn color_tool_available(&self, mapping: viewport::Mapping, ctx: &egui::Context) -> bool {
        !self.exporting
            && self.sidecars.ready()
            && (editor::dehaze::active(ctx)
                || editor::dehaze::pending(ctx)
                || self.settings.color_editor_enabled)
            && self.before.active == editor::Rendition::Current
            && self.output == "final"
            && !mapping.input
            && mapping.difference_gain == 0.
            && self.frame.as_ref().is_some_and(|f| {
                !f.crop_preview
                    && self.selected.as_ref() == Some(&f.path)
                    && f.output == "final"
                    && f.rendition == editor::Rendition::Current
            })
    }

    fn show_photo(&mut self, ui: &mut egui::Ui, mapping: viewport::Mapping) -> bool {
        let allowed = self.color_tool_available(mapping, ui.ctx());
        let crop_active = self.crop_preview_active() && !self.exporting && self.sidecars.ready();
        self.viewport.editing = crop_active
            || (editor::direct_color::active(ui.ctx()) || editor::dehaze::active(ui.ctx()))
                && allowed;
        if let Some(frame) = &self.frame {
            self.viewport.show(
                ui,
                frame.view(mapping.input).clone(),
                frame.display_extent(mapping.input),
                frame.comparison.as_ref().map(|c| c.texture.clone()),
                mapping,
                self.srgb_target,
                frame.revision,
            );
            if let Some(detail) = self.smart_detail(ui.ctx().pixels_per_point()) {
                self.viewport.show_region(
                    ui,
                    detail.texture.clone(),
                    detail.artifact.settings.preview_region.unwrap(),
                    frame.display_window(false),
                    mapping,
                    self.srgb_target,
                    detail.revision,
                );
            }
        } else if let Some(path) = &self.selected {
            if let Some(preview) = self.browser.preview(path) {
                self.viewport.show_preview(
                    ui,
                    &preview.texture,
                    preview.native_extent,
                    if crop_active {
                        Default::default()
                    } else {
                        self.settings.crop
                    },
                );
            }
        } else {
            ui.centered_and_justified(|ui| {
                ui.weak(if self.browser.files.is_empty() {
                    "Open a folder of X3F photographs"
                } else {
                    "No visible photographs"
                });
            });
        }
        if let (Some(path), Some((response, uv))) = (
            &self.selected,
            self.viewport
                .pointer(ui.input(|i| i.pointer.interact_pos())),
        ) {
            if crop_active {
                if let (Some(frame), Some((canvas, image))) = (
                    self.frame
                        .as_ref()
                        .filter(|f| f.crop_preview && &f.path == path),
                    self.viewport.image_rectangles(),
                ) {
                    return crop_tool::photo(
                        ui,
                        &response,
                        canvas,
                        image,
                        frame.uncropped_extent(),
                        &mut self.settings.crop,
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
            let (changed, request) = if editor::dehaze::active(ui.ctx())
                || editor::dehaze::pending(ui.ctx())
            {
                editor::dehaze::photo(ui, &response, uv, path, &mut self.settings, allowed)
            } else {
                editor::direct_color::photo(ui, &response, uv, path, &mut self.settings, allowed)
            };
            if let Some(request) = request {
                self.worker.pick_color(request);
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
        self.capture_actions(ctx, input);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.preferences.zoom_snap = self.viewport.zoom_snap;
        self.preferences.save(storage);
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(ready) = self.startup_ready.take() {
            timing::emit(
                "startup",
                serde_json::json!({"phase":"first_update", "since_constructed_ms": ready.elapsed().as_secs_f64() * 1000.}),
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
        editor::dehaze::source(ctx, self.selected.as_deref());
        crop_tool::source(ctx, self.selected.as_deref());
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
                self.submit(None);
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
            self.smart.reset();
            self.detail = None;
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
        application_settings::show(
            ctx,
            &mut self.settings_open,
            &mut self.preferences,
            &self.worker.cache,
            self.display.target.managed(),
        );
        application_settings::adaptive(ctx, &mut self.preferences);
        self.browser
            .set_thumbnail_budget(self.preferences.thumbnail_cache_bytes);
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
            self.detail = None;
            self.browser.invalidate_previews();
            self.previews.invalidate();
        }
        let mut changed = normalized || actions_changed || tuning_changed || toolbar_changed;
        let tools_limit = (ctx.available_rect().width() - 240.).clamp(TOOLS_MIN_WIDTH, 600.);
        egui::SidePanel::right("settings")
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
        let mut viewport_frame = egui::Frame::central_panel(&ctx.style());
        // The information row owns its vertical spacing down to the window edge.
        viewport_frame.inner_margin.bottom = 0;
        egui::CentralPanel::default()
            .frame(viewport_frame)
            .show(ctx, |ui| {
                changed |= self.central(ui);
            });
        self.browser.finish_previews();
        if crop_tool::take_view_change(ctx) {
            self.viewport.reset();
            changed = true;
        }
        if previous_selection == self.selected && (normalized || previous_settings != self.settings)
        {
            self.exit_before();
            self.sidecars.edited(&self.settings);
        }
        self.preferences.direct_color = editor::direct_color::configuration(ctx);
        if std::mem::take(&mut self.browser.clear_selection) {
            self.clear_selection();
            self.status = "No visible photographs".into();
        } else if let Some(path) = self.comparison_selection.take() {
            self.select(path, true);
        } else if let Some(selection) = select {
            self.select(selection.path, selection.preserve_viewport);
        } else if changed && !self.exporting {
            self.exit_before();
            self.submit(None);
        }
        if let Some(paths) = self.browser.take_prefetch_order(self.selected.as_deref()) {
            self.worker
                .cache
                .prefetch_window(paths, self.selected.clone());
        }
        self.editor_ui.dialog(ctx, &mut self.preferences.editor);
        self.activity.show(ctx);
        self.update_smart(ctx);
        self.update_scopes(ctx);
        self.smoke_check_scope_retention();
        self.previews.update(self.frame.as_ref(), self.busy, ctx);
        self.navigator.update(
            self.frame.as_ref(),
            self.displayed_mapping(),
            self.busy,
            ctx,
        );
        if timing::enabled() {
            timing::emit(
                "ui_update",
                serde_json::json!({"elapsed_ms":began.elapsed().as_secs_f64()*1000.}),
            );
        }
    }
}
fn main() -> eframe::Result {
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
                    eprintln!("unexpected argument: {arg}");
                    std::process::exit(2);
                }
                path = Some(PathBuf::from(arg));
            }
        }
    }
    let setup = egui_wgpu::WgpuSetupCreateNew {
        device_descriptor: Arc::new(|adapter| wgpu::DeviceDescriptor {
            label: Some("Shared GUI/image device"),
            required_features: timing::required_features(adapter.features()),
            required_limits: adapter.limits(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        persistence_path: std::env::var_os("FOVEONIC_GUI_SMOKE_DIR")
            .map(PathBuf::from)
            .map(|d| d.join("app.ron")),
        viewport: egui::ViewportBuilder::default()
            .with_app_id(preferences::APP_ID)
            .with_icon(icon::window_icon())
            .with_inner_size(if smoke::screenshot_active() {
                smoke::screenshot_recipe().window_size
            } else {
                [1440., 1000.]
            })
            .with_min_inner_size([1000., 600.]),
        wgpu_options: display::configure(egui_wgpu::WgpuConfiguration {
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup),
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
