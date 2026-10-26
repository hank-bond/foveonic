//! Runs a three-pane photographic workspace with current-buffer inspection, separate UI preferences and per-X3F editing recipes.

mod actions;
mod application_settings;
mod batch;
mod browser;
mod capture;
mod controls;
mod display;
mod display_image;
mod export;
mod help;
mod help_examples;
mod icon;
mod import_review;
mod information;
mod inspect;
mod metadata;
mod mixer;
mod navigator;
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

/// Opening starts at selection, including recipe reads before worker submission.
struct Opening {
    _priority: foveonic::decode_cache::Activity,
    began: std::time::Instant,
}

struct App {
    actions: Vec<actions::Edit>,
    import: import_review::Review,
    settings_open: bool,
    processing_open: bool,
    opening: Option<Opening>,
    browser: browser::Browser,
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
    status_error: bool,
    srgb_target: bool,
    display: display::Host,
    render_state: egui_wgpu::RenderState,
    smoke: Option<smoke::Smoke>,
    last_frame: Option<std::time::Instant>,
}
impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        path: Option<PathBuf>,
        settings: Settings,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
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
        let engine = Engine::from_device(
            rs.device.clone(),
            rs.queue.clone(),
            rs.adapter.get_info().name,
        )?;
        let engine = if let Some(directory) = std::env::var_os("FOVEONIC_CALIBRATION_RESOURCES") {
            engine.with_calibration_resources(directory)?
        } else {
            engine
        };
        let overrides = [settings.white_balance.clone(), settings.color_mode.clone()];
        let mut preferences = preferences::Preferences::load(cc.storage);
        if std::env::var_os("FOVEONIC_GUI_SMOKE_SMART").is_some() {
            preferences.preview_quality = preferences::PreviewMode::Smart;
        }
        viewport::set_gamut(rs, preferences.display_gamut);
        let folder = preferences.startup_folder(path);
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
        let mut app = Self {
            actions: Vec::new(),
            import,
            settings_open: false,
            processing_open: false,
            opening: None,
            browser,
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
            status_error: false,
            srgb_target: rs.target_format.is_srgb(),
            display,
            render_state: rs.clone(),
            smoke: smoke::Smoke::from_env(),
            last_frame: None,
        };
        app.worker
            .cache
            .configure(app.preferences.decode_cache.clone());
        app.viewport.zoom_snap = app.preferences.zoom_snap;
        if let Some((path, mode)) = folder {
            app.open(path, mode);
        }
        Ok(app)
    }
    fn open(&mut self, path: PathBuf, mode: FolderMode) {
        self.status_error = false;
        match self.browser.open(&path) {
            Ok(()) => {
                // Commit the policy only after the folder can be opened, and
                // before any of its photographs can load or queue edits.
                self.browser.import_mode = mode == FolderMode::Import;
                self.sidecars.set_memory_only(self.browser.import_mode);
                let folder = if path.is_dir() {
                    path.as_path()
                } else {
                    path.parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .unwrap_or(std::path::Path::new("."))
                };
                self.preferences.remember_folder(folder, mode);
                let selected = if path.is_file() {
                    Some(path)
                } else {
                    self.browser.files.first().cloned()
                };
                if let Some(path) = selected {
                    self.select(path, false);
                } else {
                    self.clear_selection();
                    self.status = "No X3F files in this folder".into();
                    self.status_error = true;
                }
                self.import.rescan(
                    &mut self.browser,
                    self.preferences.processing_destination.as_deref(),
                );
            }
            Err(e) => {
                self.status = format!("Cannot open {}: {e}", path.display());
                self.status_error = true;
            }
        }
    }
    fn clear_selection(&mut self) {
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
        self.detail = None;
        self.smart.reset();
        self.submit_request(export, None);
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
            self.status_error = false;
            self.busy = true;
            self.exporting = export.is_some();
            self.status = if let Some(export) = &export {
                format!("Processing {}…", export.format.label())
            } else if let Some(frame) = &self.frame {
                format!(
                    "Updating {}; displaying {} · {}",
                    self.output,
                    frame.quality_label(),
                    frame.artifact.name
                )
            } else {
                format!("Loading {}…", self.output)
            };
            timing::requested(self.revision, path, &self.output, &self.settings);
            let mut requested = self.settings.clone();
            requested.preview_region = region;
            let request = worker::Request {
                revision: self.revision,
                path: path.clone(),
                settings: requested,
                preview_quality: quality,
                output: self.output.clone(),
                comparison: self.comparison.requested()
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
                frame.path == preview.path && frame.revision == preview.revision
            }) {
                self.browser
                    .rendered(ctx, preview.path, preview.revision, preview.image);
            }
        }
        while let Ok(update) = self.worker.replies.try_recv() {
            let reply = match update {
                worker::Update::Image(reply) => *reply,
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
                                    self.status = format!("Scope analysis: {error}");
                                    self.status_error = true;
                                }
                            }
                        }
                    }
                    continue;
                }
            };
            let current = reply.revision == self.revision;
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
                if current {
                    self.settings = reply.settings;
                } else {
                    if self.settings.white_balance.is_empty() {
                        self.settings.white_balance = reply.settings.white_balance;
                    }
                    if self.settings.color_mode.is_empty() {
                        self.settings.color_mode = reply.settings.color_mode;
                    }
                }
            }
            match reply.result {
                Ok(Some(frame)) => {
                    timing::event(frame.revision, "image_received");
                    self.status = if smoke::screenshot_active() {
                        format!("Displaying {}", frame.artifact.name)
                    } else {
                        format!(
                            "Displaying {} · {} · {:.0} ms",
                            frame.quality_label(),
                            frame.artifact.name,
                            frame.milliseconds
                        )
                    };
                    if self.smoke.is_some() {
                        eprintln!("{}: {:.0} ms", frame.artifact.name, frame.milliseconds);
                    }
                    if !current {
                        self.status = format!(
                            "Updating {}; displaying {} · {}",
                            self.output,
                            frame.quality_label(),
                            frame.artifact.name
                        );
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
                    self.status = match reply.exported {
                        Some(format) => format!("{} saved", format.label()),
                        None => "Request superseded".into(),
                    };
                    if reply.exported.is_some() {
                        self.submit(None);
                    }
                }
                Err(e) => {
                    assert!(self.smoke.is_none(), "native smoke processing failed: {e}");
                    self.status = e;
                    self.status_error = true;
                }
            }
        }
    }
    fn update_scopes(&mut self, ctx: &egui::Context) {
        let visible = self.viewport.visible_pixels().map(|p| {
            [
                p.top_left[0],
                p.top_left[1],
                p.bottom_right[0] + 1,
                p.bottom_right[1] + 1,
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
            image: (**image).clone(),
        });
        self.scope_key = Some(key);
    }
    fn process(&mut self) {
        // Freeze edits before either executor receives the job.
        self.sidecars.refresh(&self.settings);
        self.import.submit(
            &self.browser,
            &self.sidecars,
            if self.browser.import_mode {
                &self.preferences.import_processing
            } else {
                &self.preferences.browse_processing
            },
        );
    }
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        self.import
            .prepare_browse(&self.browser, self.selected.as_deref());
        toolbar::container(ui, |ui| {
            toolbar::row(ui, |ui| {
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
                progress::show(
                    ui,
                    self.worker.progress.snapshot(),
                    !smoke::screenshot_active(),
                )
                .on_hover_text(&self.status);
                if widgets::button(ui, "Settings", self.settings_open, egui::Vec2::ZERO).clicked() {
                    self.settings_open = !self.settings_open;
                }
                if self.status_error {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&self.status).color(ui.visuals().error_fg_color),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&self.status);
                }
            });
            self.sidecars.refresh(&self.settings);
            self.import.show_status(ui);
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
        if let Some(warning) = self.sidecars.warning() {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(egui::Color32::YELLOW, warning);
                if self.sidecars.can_retry()
                    && widgets::button(ui, "Retry saving edits", false, egui::Vec2::ZERO).clicked()
                {
                    self.sidecars.retry();
                }
            });
        }
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
                    changed = controls::processing(
                        ui,
                        &mut self.settings,
                        self.metadata.as_deref(),
                        self.capture.properties.as_ref(),
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
                        if response.clicked() {
                            self.preferences.toggle(section);
                        }
                        if !self.preferences.collapsed.contains(&section) {
                            ui.add_space(6.);
                            changed |= self.section_body(ui, section);
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
        changed
    }
    fn displayed_channel(&self) -> i32 {
        // The displayed image may belong to an older request. Using the new
        // component choice now could show its samples with the wrong meaning.
        if self.frame.as_ref().is_some_and(|f| f.output == self.output) {
            self.channel
        } else {
            -1
        }
    }
    fn displayed_mapping(&self) -> viewport::Mapping {
        let channel = self.displayed_channel();
        let has_pair = self
            .frame
            .as_ref()
            .is_some_and(|frame| frame.output == self.output && frame.comparison.is_some());
        self.comparison.mapping(channel, has_pair)
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
                (4095., "codes")
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
                            frame.display_extent(false),
                            mapping,
                            self.srgb_target,
                            detail.revision,
                        );
                    }
                } else if let Some(path) = &self.selected {
                    if let Some(preview) = self.browser.preview(path) {
                        self.viewport
                            .show_preview(ui, &preview.texture, preview.native_extent);
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
}
impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.capture_actions(ctx, input);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.preferences.zoom_snap = self.viewport.zoom_snap;
        self.preferences.save(storage);
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Err(error) = self.display.refresh(frame) {
            self.status = format!("Display color management: {error}");
            self.status_error = true;
        }
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
        self.smoke_tick(ctx);
        let normalized = controls::normalize_with_capture(
            &mut self.settings,
            self.metadata.as_deref(),
            self.capture.properties.as_ref(),
        );
        self.sidecars.refresh(&self.settings);
        let previous_settings = self.settings.clone();
        let previous_selection = self.selected.clone();
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| self.toolbar(ui));
        let previous_gamut = self.preferences.display_gamut;
        application_settings::show(
            ctx,
            &mut self.settings_open,
            &mut self.preferences.decode_cache,
            &self.worker.cache,
            &mut self.preferences.display_gamut,
            self.display.target.managed(),
        );
        if previous_gamut != self.preferences.display_gamut {
            viewport::set_gamut(&self.render_state, self.preferences.display_gamut);
        }
        let mut select = None;
        let screenshot = smoke::screenshot_active().then(smoke::screenshot_recipe);
        let organizer_minimum = browser::minimum_panel_width(&ctx.style());
        let organizer_limit = (ctx.available_rect().width() - 540.).clamp(organizer_minimum, 650.);
        egui::SidePanel::left("organizer")
            .default_width(
                screenshot
                    .as_ref()
                    .map_or(230., |recipe| recipe.organizer_width),
            )
            .width_range(organizer_minimum..=organizer_limit)
            .resizable(true)
            .show(ctx, |ui| {
                let browser_focus = ui.make_persistent_id("organizer focus");
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
            });
        let actions_changed = self.dispatch_actions(ctx);
        let mut changed = normalized || actions_changed;
        let tools_limit = (ctx.available_rect().width() - 240.).clamp(300., 600.);
        egui::SidePanel::right("settings")
            .default_width(
                screenshot
                    .as_ref()
                    .map_or(330., |recipe| recipe.tools_width),
            )
            .width_range(300. ..=tools_limit)
            .resizable(true)
            .show(ctx, |ui| {
                changed |= self.tools(ui);
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            changed |= self.central(ui);
        });
        if previous_selection == self.selected && (normalized || previous_settings != self.settings)
        {
            self.sidecars.edited(&self.settings);
        }
        if std::mem::take(&mut self.browser.clear_selection) {
            self.clear_selection();
            self.status = "No visible photographs".into();
            self.status_error = false;
        } else if let Some(selection) = select {
            self.select(selection.path, selection.preserve_viewport);
        } else if changed && !self.exporting {
            self.submit(None);
        }
        if let Some(paths) = self.browser.take_prefetch_order(self.selected.as_deref()) {
            self.worker
                .cache
                .prefetch_window(paths, self.selected.clone());
        }
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
        "Foveonic",
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
