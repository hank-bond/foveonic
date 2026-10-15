//! Runs a three-pane Merrill workspace with current-buffer inspection, separate UI preferences and per-X3F editing recipes.

mod advanced;
mod browser;
mod capture;
mod controls;
mod help;
mod help_examples;
mod icon;
mod information;
mod inspect;
mod metadata;
mod navigator;
mod preferences;
mod scopes;
mod sections;
mod sidecars;
mod smoke;
mod timing;
mod viewport;
mod widgets;
mod worker;

use eframe::{egui, egui_wgpu, wgpu};
use foveonic::{
    render::{Engine, Settings},
    source::Metadata,
};
use preferences::{Organizer, Scope, Section};
use std::{path::PathBuf, sync::Arc};

struct App {
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
    scope_images: Option<scopes::Images>,
    scope_key: Option<worker::ScopeKey>,
    viewport: viewport::Viewport,
    navigator: navigator::Navigator,
    revision: u64,
    busy: bool,
    exporting: bool,
    preferences: preferences::Preferences,
    status: String,
    srgb_target: bool,
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
        viewport::install(rs);
        widgets::install_style(&cc.egui_ctx);
        let engine = Engine::from_device(
            rs.device.clone(),
            rs.queue.clone(),
            rs.adapter.get_info().name,
        )?;
        let overrides = [settings.white_balance.clone(), settings.color_mode.clone()];
        let preferences = preferences::Preferences::load(cc.storage);
        let path = preferences.startup_path(path);
        let mut app = Self {
            browser: browser::Browser::new(cc.egui_ctx.clone()),
            worker: worker::Worker::new(engine, cc.egui_ctx.clone()),
            capture: capture::Loader::new(cc.egui_ctx.clone()),
            sidecars: sidecars::Session::new(
                cc.egui_ctx.clone(),
                std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").is_none(),
            ),
            metadata: None,
            selected: None,
            settings,
            camera_overrides: overrides,
            output: "final".into(),
            channel: -1,
            comparison: Default::default(),
            frame: None,
            scope_images: None,
            scope_key: None,
            viewport: Default::default(),
            navigator: navigator::Navigator::new(rs.clone(), cc.egui_ctx.clone()),
            revision: 0,
            busy: false,
            exporting: false,
            preferences,
            status: "Open a folder or Merrill X3F".into(),
            srgb_target: rs.target_format.is_srgb(),
            render_state: rs.clone(),
            smoke: smoke::Smoke::from_env(),
            last_frame: None,
        };
        app.viewport.zoom_snap = app.preferences.zoom_snap;
        if let Some(path) = path {
            app.open(path);
        }
        Ok(app)
    }
    fn open(&mut self, path: PathBuf) {
        match self.browser.open(&path) {
            Ok(()) => {
                let folder = if path.is_dir() {
                    path.as_path()
                } else {
                    path.parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .unwrap_or(std::path::Path::new("."))
                };
                self.preferences.remember_folder(folder);
                let selected = if path.is_file() {
                    Some(path)
                } else {
                    self.browser.files.first().cloned()
                };
                if let Some(path) = selected {
                    self.select(path);
                } else {
                    self.revision += 1;
                    self.selected = None;
                    self.sidecars.clear();
                    self.capture.select(None);
                    self.metadata = None;
                    self.frame = None;
                    self.scope_images = None;
                    self.busy = false;
                    self.worker.clear();
                    self.viewport.reset();
                    viewport::clear(&self.render_state);
                    self.status = "No X3F files in this folder".into();
                }
            }
            Err(e) => self.status = format!("Cannot open {}: {e}", path.display()),
        }
    }
    fn select(&mut self, path: PathBuf) {
        self.capture.select(Some(path.clone()));
        self.sidecars.select(path.clone());
        self.selected = Some(path);
        self.settings = Settings::new("", "");
        self.revision += 1;
        self.worker.clear();
        self.busy = false;
        self.status = "Loading editing recipe…".into();
        self.metadata = None;
        self.frame = None;
        self.scope_images = None;
        self.viewport.reset();
        viewport::clear(&self.render_state);
    }
    fn submit(&mut self, export: Option<worker::Export>) {
        if !self.sidecars.ready() {
            return;
        }
        if let Some(path) = &self.selected {
            self.revision += 1;
            self.busy = true;
            self.exporting = export.is_some();
            self.status = if let Some(export) = &export {
                format!("Exporting {}…", export.format.label())
            } else if let Some(frame) = &self.frame {
                format!(
                    "Updating {}; displaying {}",
                    self.output, frame.artifact.name
                )
            } else {
                format!("Loading {}…", self.output)
            };
            timing::requested(self.revision, path, &self.output, &self.settings);
            self.worker.submit(worker::Request {
                revision: self.revision,
                path: path.clone(),
                settings: self.settings.clone(),
                output: self.output.clone(),
                comparison: self.comparison.requested()
                    && foveonic::inspection::comparison_input(&self.settings, &self.output)
                        .is_some(),
                export,
            });
        }
    }
    fn receive(&mut self, ctx: &egui::Context) {
        self.browser.receive(ctx);
        while let Ok(update) = self.worker.replies.try_recv() {
            let reply = match update {
                worker::Update::Image(reply) => *reply,
                worker::Update::Scopes { key, result } => {
                    if let Some(frame) = self.frame.as_mut().filter(|frame| {
                        frame.revision == key.revision && self.scope_key == Some(key)
                    }) {
                        match result {
                            Ok(data) => {
                                self.scope_images = Some(scopes::Images::new(&data));
                                frame.scopes = Some(worker::ScopeSnapshot { key, data });
                                timing::event(key.revision, "scopes_received");
                            }
                            Err(error) => {
                                if !self.busy {
                                    self.status = format!("Scope analysis: {error}");
                                }
                            }
                        }
                    }
                    continue;
                }
            };
            let current = reply.revision == self.revision;
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
                    self.status = format!(
                        "Displaying {} · {:.0} ms",
                        frame.artifact.name, frame.milliseconds
                    );
                    if self.smoke.is_some() {
                        eprintln!("{}: {:.0} ms", frame.artifact.name, frame.milliseconds);
                    }
                    if !current {
                        self.status = format!(
                            "Updating {}; displaying {}",
                            self.output, frame.artifact.name
                        );
                    }
                    timing::image_received(
                        frame.revision,
                        current,
                        frame.queued_at,
                        frame.milliseconds,
                    );
                    self.scope_images = None;
                    self.scope_key = None;
                    viewport::clear(&self.render_state);
                    self.frame = Some(frame);
                }
                Ok(None) => {
                    self.status = match reply.exported {
                        Some(format) => format!("{} exported", format.label()),
                        None => "Request superseded".into(),
                    };
                    if reply.exported.is_some() {
                        self.submit(None);
                    }
                }
                Err(e) => self.status = e,
            }
        }
    }
    fn update_scopes(&mut self) {
        let Some(frame) = &self.frame else {
            self.scope_key = None;
            return;
        };
        let region = if self.preferences.scope_viewport {
            let Some(pixels) = self.viewport.visible_pixels() else {
                return;
            };
            Some([
                pixels.top_left[0],
                pixels.top_left[1],
                pixels.bottom_right[0] + 1,
                pixels.bottom_right[1] + 1,
            ])
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
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!self.exporting, |ui| {
                if widgets::button(ui, "Open folder", false, egui::Vec2::ZERO).clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.open(path);
                    }
                }
                if widgets::button(ui, "Open X3F", false, egui::Vec2::ZERO).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Merrill X3F", &["x3f"])
                        .pick_file()
                    {
                        self.open(path);
                    }
                }
                for format in [worker::ExportFormat::Tiff, worker::ExportFormat::Dng] {
                    if ui
                        .add_enabled_ui(
                            self.sidecars.ready()
                                && self.selected.is_some()
                                && self.settings.validate().is_ok(),
                            |ui| {
                                widgets::button(
                                    ui,
                                    &format!("Export {}", format.label()),
                                    false,
                                    egui::Vec2::ZERO,
                                )
                            },
                        )
                        .inner
                        .clicked()
                    {
                        let name = self
                            .selected
                            .as_ref()
                            .and_then(|p| p.file_stem())
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "render".into());
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter(format.label(), &[format.extension()])
                            .set_file_name(format!("{name}.{}", format.extension()))
                            .save_file()
                        {
                            self.submit(Some(worker::Export { path, format }));
                        }
                    }
                }
            });
            if self.busy {
                ui.spinner();
            }
            ui.add(egui::Label::new(&self.status).truncate())
                .on_hover_text(&self.status);
        });
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
                let combo = egui::ComboBox::from_id_salt("scope mode")
                    .width(ui.available_width() - 8.)
                    .selected_text(match self.preferences.scope {
                        Scope::Histogram => "Histogram",
                        Scope::Parade => "RGB / layer parade",
                        Scope::Waveform => "Combined waveform",
                    });
                widgets::dropdown(ui, combo, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.;
                    for (value, name) in [
                        (Scope::Histogram, "Histogram"),
                        (Scope::Parade, "RGB / layer parade"),
                        (Scope::Waveform, "Combined waveform"),
                    ] {
                        widgets::menu_value(ui, &mut self.preferences.scope, value, name)
                            .on_hover_text(help::scope(value));
                    }
                })
                .response
                .on_hover_text(help::scope(self.preferences.scope));
                widgets::checkbox(ui, &mut self.preferences.scope_viewport, "Viewport only")
                    .on_hover_text("Show scopes for the visible part of the image. Updates as you pan or zoom. Channel, Input/Output, and Difference views do not change scope values.");
                scopes::show(
                    ui,
                    self.preferences.scope,
                    self.frame
                        .as_ref()
                        .and_then(|f| f.scopes.as_ref())
                        .map(|snapshot| &snapshot.data),
                    self.scope_images.as_mut(),
                );
                false
            }
            Section::Editing => {
                let mut changed = false;
                ui.add_enabled_ui(!self.exporting && self.sidecars.ready(), |ui| {
                    changed =
                        controls::processing(ui, &mut self.settings, self.metadata.as_deref());
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
                    ui.add_space(7.);
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
        ui.horizontal_wrapped(|ui| {
            ui.weak("Inspect");
            ui.add_enabled_ui(!self.exporting, |ui| {
                changed |= inspect::show(ui, &self.settings, &mut self.output, &mut self.channel);
                let requested =
                    self.comparison
                        .show(ui, &self.settings, &self.output, self.channel);
                let has_pair = self
                    .frame
                    .as_ref()
                    .is_some_and(|frame| frame.output == self.output && frame.comparison.is_some());
                changed |= requested && (self.busy || !has_pair);
            });
        });
        let mapping = self.displayed_mapping();
        if mapping.difference_gain > 0. {
            let frame = self.frame.as_ref().unwrap();
            let input = frame.comparison.as_ref().unwrap().input;
            let (range, units) = if matches!(
                frame.texture.description().space,
                foveonic::image::Space::ToneMappedRgb(_)
            ) {
                (1., "linear")
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
                foveonic::inspection::comparison_input(&frame.artifact.settings, &frame.output)
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
                        frame.comparison.as_ref().map(|c| c.texture.clone()),
                        mapping,
                        self.srgb_target,
                        frame.revision,
                    );
                } else if let Some(path) = &self.selected {
                    if let Some(preview) = self.browser.thumbnail(path) {
                        self.viewport
                            .show_preview(ui, &preview.texture, preview.native_extent);
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.weak("Open a folder or Merrill X3F");
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
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.preferences.zoom_snap = self.viewport.zoom_snap;
        self.preferences.save(storage);
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
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
        if let Some(saved) = self.sidecars.receive() {
            if self.selected.is_some() {
                self.settings = sidecars::selection_settings(saved, &self.camera_overrides);
                inspect::normalize(&self.settings, &mut self.output, &mut self.channel);
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
        self.sidecars.refresh(&self.settings);
        let previous_settings = self.settings.clone();
        let previous_selection = self.selected.clone();
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| self.toolbar(ui));
        let mut select = None;
        let organizer_limit = (ctx.available_rect().width() - 540.).clamp(160., 650.);
        egui::SidePanel::left("organizer")
            .default_width(230.)
            .width_range(160. ..=organizer_limit)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.weak(format!("{} files", self.browser.files.len()));
                    widgets::selectable_value(
                        ui,
                        &mut self.preferences.organizer,
                        Organizer::Icons,
                        "Icons",
                    );
                    widgets::selectable_value(
                        ui,
                        &mut self.preferences.organizer,
                        Organizer::List,
                        "List",
                    );
                });
                ui.separator();
                if let Some(path) = &self.selected {
                    if let Some(folder) = path.parent().and_then(|p| p.file_name()) {
                        ui.add(egui::Label::new(folder.to_string_lossy()).truncate());
                    }
                }
                ui.add_enabled_ui(!self.exporting, |ui| {
                    select = self.browser.organizer(
                        ui,
                        self.selected.as_deref(),
                        self.preferences.organizer,
                    );
                });
            });
        let mut changed = false;
        let tools_limit = (ctx.available_rect().width() - 240.).clamp(300., 600.);
        egui::SidePanel::right("settings")
            .default_width(330.)
            .width_range(300. ..=tools_limit)
            .resizable(true)
            .show(ctx, |ui| {
                changed |= self.tools(ui);
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            changed |= self.central(ui);
        });
        if previous_selection == self.selected && previous_settings != self.settings {
            self.sidecars.edited(&self.settings);
        }
        if let Some(path) = select {
            self.select(path);
        } else if changed && !self.exporting {
            self.submit(None);
        }
        self.update_scopes();
        self.smoke_check_scope_retention();
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
                println!("foveonic-gui [X3F_OR_FOLDER] [--wb NAME --mode NAME]\nWB and color mode default to the loaded X3F's recorded settings; --wb/--mode override them.");
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
            .with_inner_size([1440., 1000.])
            .with_min_inner_size([1000., 600.]),
        wgpu_options: egui_wgpu::WgpuConfiguration {
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup),
            ..Default::default()
        },
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
        super::inspect::normalize(&settings, &mut output, &mut channel);
        assert_eq!((&*output, channel), ("final", -1));
        output = "calibration.layers".into();
        channel = 1;
        super::inspect::normalize(&settings, &mut output, &mut channel);
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
