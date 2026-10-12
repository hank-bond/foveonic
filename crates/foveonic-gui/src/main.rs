//! Runs a three-pane Merrill workspace with current-buffer inspection, reorderable tools, and UI-only persistence.

mod advanced;
mod browser;
mod controls;
mod help;
mod icon;
mod information;
mod inspect;
mod metadata;
mod navigator;
mod preferences;
mod scopes;
mod sections;
mod smoke;
mod viewport;
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
    metadata: Option<Arc<Metadata>>,
    selected: Option<PathBuf>,
    settings: Settings,
    camera_overrides: [String; 2],
    output: String,
    channel: i32,
    frame: Option<worker::Frame>,
    scope_images: Option<scopes::Images>,
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
    measure_timing: bool,
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
        viewport::install(rs);
        install_style(&cc.egui_ctx);
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
            metadata: None,
            selected: None,
            settings,
            camera_overrides: overrides,
            output: "final".into(),
            channel: -1,
            frame: None,
            scope_images: None,
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
            measure_timing: std::env::var_os("FOVEONIC_UI_TIMING").is_some(),
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
        self.selected = Some(path);
        reset_camera_selection(&mut self.settings, &self.camera_overrides);
        self.metadata = None;
        self.frame = None;
        self.scope_images = None;
        self.viewport.reset();
        viewport::clear(&self.render_state);
        self.submit(None);
    }
    fn submit(&mut self, export: Option<PathBuf>) {
        if let Some(path) = &self.selected {
            self.revision += 1;
            self.busy = true;
            self.exporting = export.is_some();
            self.status = if self.exporting {
                "Exporting final rendering…".into()
            } else if let Some(frame) = &self.frame {
                format!(
                    "Updating {}; displaying {}",
                    self.output, frame.artifact.name
                )
            } else {
                format!("Loading {}…", self.output)
            };
            self.worker.submit(worker::Request {
                revision: self.revision,
                path: path.clone(),
                settings: self.settings.clone(),
                output: self.output.clone(),
                export,
            });
        }
    }
    fn receive(&mut self, ctx: &egui::Context) {
        self.browser.receive(ctx);
        while let Ok(update) = self.worker.replies.try_recv() {
            let reply = match update {
                worker::Update::Image(reply) => *reply,
                worker::Update::Scopes { revision, result } => {
                    if let Some(frame) = self
                        .frame
                        .as_mut()
                        .filter(|frame| frame.revision == revision)
                    {
                        match result {
                            Ok(data) => {
                                self.scope_images = Some(scopes::Images::new(ctx, &data));
                                frame.scopes = Some(data);
                                if self.measure_timing {
                                    eprintln!(
                                        "UI-SCOPES revision={revision} elapsed_ms={:.3}",
                                        frame.queued_at.elapsed().as_secs_f64() * 1000.
                                    );
                                }
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
                    if self.measure_timing {
                        eprintln!(
                            "UI-IMAGE revision={} current={} input_to_ui_ms={:.3} worker_ms={:.3}",
                            frame.revision,
                            current,
                            frame.queued_at.elapsed().as_secs_f64() * 1000.,
                            frame.milliseconds
                        );
                    }
                    self.scope_images = None;
                    viewport::clear(&self.render_state);
                    self.frame = Some(frame);
                }
                Ok(None) => {
                    self.status = if reply.exported {
                        "TIFF exported"
                    } else {
                        "Request superseded"
                    }
                    .into();
                    if reply.exported {
                        self.submit(None);
                    }
                }
                Err(e) => self.status = e,
            }
        }
    }
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!self.exporting, |ui| {
                if ui.button("Open folder").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.open(path);
                    }
                }
                if ui.button("Open X3F").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Merrill X3F", &["x3f"])
                        .pick_file()
                    {
                        self.open(path);
                    }
                }
                if ui.button("Fit").clicked() {
                    self.viewport.reset();
                }
                if ui.button("100%").clicked() {
                    self.viewport.one_to_one = true;
                }
                if ui
                    .add_enabled(
                        self.selected.is_some() && self.settings.validate().is_ok(),
                        egui::Button::new("Export TIFF"),
                    )
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("TIFF", &["tiff"])
                        .set_file_name("render.tiff")
                        .save_file()
                    {
                        self.submit(Some(path));
                    }
                }
            });
            if self.busy {
                ui.spinner();
            }
            ui.add(egui::Label::new(&self.status).truncate())
                .on_hover_text(&self.status);
        });
    }
    fn section_body(&mut self, ui: &mut egui::Ui, section: Section) -> bool {
        match section {
            Section::Navigator => {
                let channel = self.displayed_channel();
                let texture = self
                    .frame
                    .as_ref()
                    .map(|frame| self.navigator.image(frame, channel));
                let preview = self
                    .selected
                    .as_ref()
                    .and_then(|p| self.browser.thumbnail(p));
                self.viewport.navigator(ui, texture, preview.as_ref());
                false
            }
            Section::Scopes => {
                egui::ComboBox::from_id_salt("scope mode")
                    .width(ui.available_width() - 8.)
                    .selected_text(match self.preferences.scope {
                        Scope::Histogram => "Histogram",
                        Scope::Parade => "RGB / layer parade",
                        Scope::Waveform => "Combined waveform",
                    })
                    .show_ui(ui, |ui| {
                        for (value, name) in [
                            (Scope::Histogram, "Histogram"),
                            (Scope::Parade, "RGB / layer parade"),
                            (Scope::Waveform, "Combined waveform"),
                        ] {
                            ui.selectable_value(&mut self.preferences.scope, value, name)
                                .on_hover_text(help::scope(value));
                        }
                    })
                    .response
                    .on_hover_text(help::scope(self.preferences.scope));
                scopes::show(
                    ui,
                    self.preferences.scope,
                    self.frame.as_ref().and_then(|f| f.scopes.as_ref()),
                    self.scope_images.as_ref(),
                );
                false
            }
            Section::Editing => {
                let mut changed = false;
                ui.add_enabled_ui(!self.exporting, |ui| {
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
        // A pending root's component must not reinterpret the artifact still being displayed.
        if self.frame.as_ref().is_some_and(|f| f.output == self.output) {
            self.channel
        } else {
            -1
        }
    }
    fn central(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.weak("Inspect");
            ui.add_enabled_ui(!self.exporting, |ui| {
                changed |= inspect::show(ui, &self.settings, &mut self.output, &mut self.channel);
            });
        });
        ui.add_space(3.);
        let height =
            (ui.available_height() - information::HEIGHT - ui.spacing().item_spacing.y).max(0.);
        let channel = self.displayed_channel();
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_min_height(height);
                if let Some(frame) = &self.frame {
                    self.viewport
                        .show(ui, frame.texture.clone(), channel, self.srgb_target);
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
        let metadata = self
            .frame
            .as_ref()
            .map(|f| f.artifact.metadata.as_ref())
            .or(self.metadata.as_deref());
        information::show(ui, metadata, &filename, self.viewport.visible_pixels());
        changed
    }
}
impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.preferences.zoom_snap = self.viewport.zoom_snap;
        self.preferences.save(storage);
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let began = std::time::Instant::now();
        if self.measure_timing {
            if let Some(last) = self.last_frame.replace(began) {
                eprintln!(
                    "UI-FRAME gap_ms={:.3} busy={}",
                    began.duration_since(last).as_secs_f64() * 1000.,
                    self.busy
                );
            }
            if self.busy {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
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
                    ui.selectable_value(&mut self.preferences.organizer, Organizer::Icons, "Icons");
                    ui.selectable_value(&mut self.preferences.organizer, Organizer::List, "List");
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
        if let Some(path) = select {
            self.select(path);
        } else if changed && !self.exporting {
            self.submit(None);
        }
        self.navigator.update(
            self.frame.as_ref(),
            self.displayed_channel(),
            self.busy,
            ctx,
        );
        if self.measure_timing {
            eprintln!(
                "UI-UPDATE cpu_ms={:.3}",
                began.elapsed().as_secs_f64() * 1000.
            );
        }
    }
}
fn install_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = egui::Color32::from_gray(22);
    visuals.window_fill = egui::Color32::from_gray(22);
    visuals.extreme_bg_color = egui::Color32::from_gray(12);
    visuals.selection.bg_fill = egui::Color32::from_rgb(35, 65, 100);
    visuals.selection.stroke = egui::Stroke::new(1_f32, egui::Color32::from_rgb(100, 160, 245));
    visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(34);
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_gray(29);
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(7., 6.);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.));
    ctx.set_style(style);
}
fn reset_camera_selection(settings: &mut Settings, overrides: &[String; 2]) {
    settings.white_balance.clone_from(&overrides[0]);
    settings.color_mode.clone_from(&overrides[1]);
}
fn main() -> eframe::Result {
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
    fn image_changes_reset_camera_choices_but_keep_other_controls() {
        let mut settings = foveonic::render::Settings::new("Shade", "Vivid");
        settings.exposure = 1.;
        super::reset_camera_selection(&mut settings, &[String::new(), String::new()]);
        assert!(settings.white_balance.is_empty() && settings.color_mode.is_empty());
        assert_eq!(settings.exposure, 1.);
        super::reset_camera_selection(&mut settings, &["Auto".into(), "Neutral".into()]);
        assert_eq!(settings.white_balance, "Auto");
        assert_eq!(settings.color_mode, "Neutral");
    }
}
