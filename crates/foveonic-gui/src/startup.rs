//! Presents a responsive loading window while one worker prepares the photographic application.

use crate::{diagnostics, display, preferences, timing, widgets, App};
use eframe::{egui, egui_wgpu};
use foveonic::diagnostics as trace;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[cfg(test)]
#[path = "startup_test.rs"]
mod tests;

pub const WINDOW_SIZE: egui::Vec2 = egui::vec2(560., 200.);

// Eframe 0.32 writes this opaque value before App::save. Preserve it while the
// loading window is visible so its temporary geometry cannot replace the workspace.
const WINDOW_STORAGE_KEY: &str = "window";
struct Window {
    workspace: egui::ViewportBuilder,
    saved: String,
    positioned: bool,
    workspace_position: Option<egui::Pos2>,
    resizing_from: Option<egui::Vec2>,
}
impl Window {
    fn new(workspace: egui::ViewportBuilder, storage: Option<&dyn eframe::Storage>) -> Self {
        Self {
            workspace,
            // Storage has no removal API. An empty value reloads as no settings.
            saved: storage
                .and_then(|storage| storage.get_string(WINDOW_STORAGE_KEY))
                .unwrap_or_default(),
            positioned: false,
            workspace_position: None,
            resizing_from: None,
        }
    }
    fn place(&mut self, context: &egui::Context) {
        if std::mem::replace(&mut self.positioned, true) {
            return;
        }
        if let Some(command) = egui::ViewportCommand::center_on_screen(context) {
            if self.workspace.position.is_some() {
                // Snapshot the actual outer position. The builder contains an
                // inner position on macOS and cannot be replayed as an outer one.
                self.workspace_position =
                    context.input(|input| input.viewport().outer_rect.map(|rect| rect.min));
            }
            context.send_viewport_cmd(command);
        }
    }
    fn commands(&self) -> Vec<egui::ViewportCommand> {
        use egui::ViewportCommand as Command;
        let mut commands = vec![
            Command::Resizable(self.workspace.resizable.unwrap_or(true)),
            Command::MinInnerSize(self.workspace.min_inner_size.unwrap_or_default()),
            Command::InnerSize(
                self.workspace
                    .inner_size
                    .unwrap_or(egui::vec2(1440., 1000.)),
            ),
        ];
        if let Some(position) = self.workspace_position {
            commands.push(Command::OuterPosition(position));
        }
        commands.extend([
            Command::Maximized(self.workspace.maximized.unwrap_or(false)),
            Command::Fullscreen(self.workspace.fullscreen.unwrap_or(false)),
        ]);
        commands
    }
    fn restore(&mut self, context: &egui::Context) {
        self.resizing_from =
            context.input(|input| input.viewport().inner_rect.map(|rect| rect.size()));
        if self.workspace.position.is_none() {
            // On first launch, expand around the loading window's center rather
            // than keeping its small-window origin and extending offscreen.
            self.workspace_position = context.input(|input| {
                let viewport = input.viewport();
                Some(
                    viewport.outer_rect?.min
                        + (viewport.inner_rect?.size() - self.workspace.inner_size?) / 2.,
                )
            });
        }
        self.fit(context, |outer| {
            work_area(context.pixels_per_point(), outer)
        });
        for command in self.commands() {
            context.send_viewport_cmd(command);
        }
    }
    /// Keep the workspace inside the monitor area that the taskbar leaves free.
    /// Eframe and winit place windows against the whole monitor.
    fn fit(
        &mut self,
        context: &egui::Context,
        work_area: impl FnOnce(egui::Rect) -> Option<egui::Rect>,
    ) {
        let frame = context.input(|input| {
            let viewport = input.viewport();
            Some(viewport.outer_rect?.size() - viewport.inner_rect?.size())
        });
        let (Some(frame), Some(position), Some(inner_size)) =
            (frame, self.workspace_position, self.workspace.inner_size)
        else {
            return;
        };
        let outer = egui::Rect::from_min_size(position, inner_size + frame);
        let Some(area) = work_area(outer) else {
            return;
        };
        let fitted = fit_within(outer, area);
        self.workspace_position = Some(fitted.min);
        self.workspace.inner_size = Some(fitted.size() - frame);
    }
    fn observe_size(&mut self, size: Option<egui::Vec2>) {
        if size.is_some() && size != self.resizing_from {
            self.resizing_from = None;
        }
    }
    fn preserve(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string(WINDOW_STORAGE_KEY, self.saved.clone());
    }
}

/// Shrink a window that is larger than the area, then move it inside.
fn fit_within(window: egui::Rect, area: egui::Rect) -> egui::Rect {
    let size = window.size().min(area.size());
    let min = window.min.clamp(area.min, area.max - size);
    egui::Rect::from_min_size(min, size)
}

/// The free area, in points, of the monitor nearest to a window rectangle.
/// Tests supply their own area so results do not depend on the test machine's monitor.
#[cfg(all(target_os = "windows", not(test)))]
fn work_area(pixels_per_point: f32, window: egui::Rect) -> Option<egui::Rect> {
    use windows_sys::Win32::{
        Foundation::RECT,
        Graphics::Gdi::{GetMonitorInfoW, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    };
    let pixels = |points: f32| (points * pixels_per_point).round() as i32;
    let window = RECT {
        left: pixels(window.left()),
        top: pixels(window.top()),
        right: pixels(window.right()),
        bottom: pixels(window.bottom()),
    };
    let empty = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut monitor = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        rcMonitor: empty,
        rcWork: empty,
        dwFlags: 0,
    };
    // SAFETY: both pointers refer to live locals, and `cbSize` states the
    // size of the structure the call fills.
    let found = unsafe {
        GetMonitorInfoW(
            MonitorFromRect(&window, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        )
    };
    let points = |pixels: i32| pixels as f32 / pixels_per_point;
    let work = monitor.rcWork;
    (found != 0).then(|| {
        egui::Rect::from_min_max(
            egui::pos2(points(work.left), points(work.top)),
            egui::pos2(points(work.right), points(work.bottom)),
        )
    })
}

/// Placement on other platforms is unchanged.
#[cfg(any(not(target_os = "windows"), test))]
fn work_area(_pixels_per_point: f32, _window: egui::Rect) -> Option<egui::Rect> {
    None
}

/// The native hook receives the already restored and monitor-clamped workspace.
pub fn compact_window(workspace: egui::ViewportBuilder) -> egui::ViewportBuilder {
    workspace
        .with_inner_size(WINDOW_SIZE)
        .with_min_inner_size(WINDOW_SIZE)
        .with_fullscreen(false)
        .with_maximized(false)
        .with_resizable(false)
}

pub(crate) struct Inputs {
    pub context: egui::Context,
    pub render_state: egui_wgpu::RenderState,
    pub preferences: preferences::Preferences,
    pub target: display::Target,
    pub path: Option<PathBuf>,
}

#[derive(Clone, Copy, Default)]
struct Progress {
    completed: usize,
    total: usize,
}

type Prepare<T> = Box<dyn FnOnce(Arc<Mutex<Progress>>) -> Result<T, String> + Send>;
struct Preparation<T> {
    work: Option<Prepare<T>>,
    receiver: Option<mpsc::Receiver<Result<T, String>>>,
    progress: Arc<Mutex<Progress>>,
}
impl<T: Send + 'static> Preparation<T> {
    fn new(work: impl FnOnce(Arc<Mutex<Progress>>) -> Result<T, String> + Send + 'static) -> Self {
        Self {
            work: Some(Box::new(work)),
            receiver: None,
            progress: Arc::default(),
        }
    }
    fn start_if_ready(
        &mut self,
        presented: bool,
        recorder_ready: bool,
        context: &egui::Context,
    ) -> Result<(), String> {
        if !presented || !recorder_ready {
            return Ok(());
        }
        let Some(work) = self.work.take() else {
            return Ok(());
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let progress = self.progress.clone();
        let context = context.clone();
        std::thread::Builder::new()
            .name("graphics-startup".into())
            .spawn(move || {
                let result = work(progress);
                let _ = sender.send(result);
                context.request_repaint();
            })
            .map_err(|error| {
                foveonic::support::report_error(
                    "startup.spawn_preparation",
                    &error,
                    Default::default(),
                );
                format!("Cannot start graphics preparation: {error}")
            })?;
        self.receiver = Some(receiver);
        Ok(())
    }
    fn receive(&mut self) -> Option<Result<T, String>> {
        match self.receiver.as_ref()?.try_recv() {
            Ok(result) => {
                self.receiver = None;
                Some(result)
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                foveonic::support::event(
                    foveonic::support::Level::Error,
                    "startup.worker_disconnected",
                    "Graphics preparation stopped unexpectedly",
                    serde_json::Value::Null,
                );
                self.receiver = None;
                Some(Err(
                    "Graphics engine preparation stopped unexpectedly.".into()
                ))
            }
            Err(mpsc::TryRecvError::Empty) => None,
        }
    }
}

struct Observation {
    name: &'static str,
    started: Instant,
    finished: Instant,
}
pub(crate) struct Observations {
    began: Instant,
    requested: bool,
    pending: Mutex<Vec<Observation>>,
    presented: AtomicBool,
    attached: AtomicBool,
}
impl Observations {
    pub fn new(began: Instant) -> Arc<Self> {
        Arc::new(Self {
            began,
            requested: timing::requested_mode().is_some(),
            pending: Mutex::new(Vec::new()),
            presented: AtomicBool::new(false),
            attached: AtomicBool::new(false),
        })
    }
    pub(super) fn record(&self, name: &'static str, started: Instant, finished: Instant) {
        if self.requested && !self.attached.load(Ordering::Acquire) {
            self.pending.lock().unwrap().push(Observation {
                name,
                started,
                finished,
            });
        }
    }
    pub fn setup(&self, value: egui_wgpu::SetupTiming) {
        let name = match value.stage {
            egui_wgpu::SetupStage::Instance => "startup.graphics_instance",
            egui_wgpu::SetupStage::AdapterEnumeration => "startup.adapter_enumeration",
            egui_wgpu::SetupStage::AdapterSelection => "startup.adapter_selection",
            egui_wgpu::SetupStage::Device => "startup.graphics_device",
            egui_wgpu::SetupStage::Renderer => "startup.ui_renderer",
            egui_wgpu::SetupStage::SurfaceCreation => "startup.surface_create",
            egui_wgpu::SetupStage::SurfaceConfiguration => "startup.surface_configure",
        };
        self.record(name, value.started, value.finished);
    }
    pub fn paint(&self, value: egui_wgpu::PaintTiming) {
        timing::paint_stage(value);
        if matches!(value.stage, egui_wgpu::PaintStage::Present)
            && value.viewport == egui::ViewportId::ROOT
            && !self.presented.swap(true, Ordering::AcqRel)
        {
            self.record("startup.entry_to_first_present", self.began, value.finished);
            foveonic::support::event(
                foveonic::support::Level::Info,
                "startup.first_present",
                "First application window presented",
                serde_json::Value::Null,
            );
        }
    }
    fn flush(&self) {
        if !trace::enabled() {
            return;
        }
        for value in self.pending.lock().unwrap().drain(..) {
            if let (Some(start), Some(end)) = (
                trace::timestamp_at(value.started),
                trace::timestamp_at(value.finished),
            ) {
                trace::interval(trace::Category::Ui, value.name, start, end, |fields| {
                    fields.str("measurement", "host_elapsed");
                });
            }
        }
    }
}

/// Keeps one startup recording owned even when native framework setup fails.
pub(crate) struct Capture {
    controller: Option<diagnostics::Controller>,
    observations: Arc<Observations>,
}
impl Capture {
    pub fn new(observations: Arc<Observations>) -> Result<Self, String> {
        let mut controller = diagnostics::Controller::default();
        if let Some(mode) = timing::requested_mode() {
            controller.start(
                mode,
                serde_json::json!({"entry":"process_startup", "pre_main_included":false}),
            )?;
        }
        Ok(Self {
            controller: Some(controller),
            observations,
        })
    }
    fn controller(&mut self) -> &mut diagnostics::Controller {
        self.controller.as_mut().unwrap()
    }
    fn take(&mut self) -> diagnostics::Controller {
        self.controller.take().unwrap()
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(controller) = self
            .controller
            .as_mut()
            .filter(|controller| controller.save_on_exit_enabled())
        {
            controller.set_end_state(serde_json::json!({"startup":"closed_before_ready"}));
            let saved = controller.wait_for_start().and_then(|()| {
                self.observations.flush();
                controller.save_on_exit()
            });
            if let Err(error) = saved {
                eprintln!("Cannot save startup capture: {error}");
            }
        }
    }
}

pub(crate) struct Startup {
    preparation: Preparation<Box<App>>,
    app: Option<Box<App>>,
    error: Option<String>,
    display: display::Host,
    window: Window,
    icons: [egui::TextureHandle; 2],
    preferences: preferences::Preferences,
    capture: Capture,
    observations: Arc<Observations>,
}
impl Startup {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        path: Option<PathBuf>,
        mut capture: Capture,
        observations: Arc<Observations>,
        workspace: egui::ViewportBuilder,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        observations.record(
            "startup.entry_to_app_creator",
            observations.began,
            Instant::now(),
        );
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .ok_or("wgpu renderer unavailable")?;
        crate::support::selected_gpu(rs);
        foveonic::support::event(
            foveonic::support::Level::Info,
            "startup.graphics_ready",
            "Native window and GPU device ready",
            serde_json::Value::Null,
        );
        timing::check_gpu_features(rs.device.features())?;
        crate::display_image::install_upload_error_handler(&rs.device);
        let display = display::Host::new(cc)?;
        let window = Window::new(workspace, cc.storage);
        let icons = crate::icon::startup_icons(&cc.egui_ctx);
        let preferences = preferences::Preferences::load(cc.storage);
        foveonic::support::update_context("preferences", preferences.support_context());
        widgets::install_style(&cc.egui_ctx);
        if crate::smoke::screenshot_active() {
            cc.egui_ctx.set_theme(egui::Theme::Dark);
        } else if std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").is_some()
            && std::env::var_os("FOVEONIC_GUI_SMOKE_LIGHT").is_some()
        {
            cc.egui_ctx.set_theme(egui::Theme::Light);
        }
        capture.controller().set_configuration(serde_json::json!({"entry":"process_startup", "pre_main_included":false,
            "adapter":rs.adapter.get_info().name,"gpu_timestamps_available":timing::gpu_available(),"display_format":format!("{:?}",rs.target_format)}));
        let inputs = Inputs {
            context: cc.egui_ctx.clone(),
            render_state: rs.clone(),
            preferences: preferences.clone(),
            target: display.target,
            path,
        };
        let observed = observations.clone();
        let preparation = Preparation::new(move |progress| {
            foveonic::support::event(
                foveonic::support::Level::Info,
                "startup.prepare",
                "Preparing photographic workspace",
                serde_json::Value::Null,
            );
            let began = Instant::now();
            let mut reported_quarter = None;
            let app = App::prepare(inputs, |completed, total| {
                *progress.lock().unwrap() = Progress { completed, total };
                let quarter = completed.saturating_mul(4) / total.max(1);
                if reported_quarter != Some(quarter) {
                    reported_quarter = Some(quarter);
                    foveonic::support::event(
                        foveonic::support::Level::Info,
                        "startup.preparation_progress",
                        "Preparing graphics programs",
                        serde_json::json!({"completed":completed,"total":total}),
                    );
                }
            });
            observed.record("startup.prepare_workspace", began, Instant::now());
            // Keep App behind a pointer on Windows' smaller main-thread stack.
            app.map(Box::new)
        });
        Ok(Self {
            preparation,
            app: None,
            error: None,
            display,
            window,
            icons,
            preferences,
            capture,
            observations,
        })
    }
}
impl eframe::App for Startup {
    fn raw_input_hook(&mut self, context: &egui::Context, input: &mut egui::RawInput) {
        if let Some(app) = &mut self.app {
            app.raw_input_hook(context, input);
        }
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(app) = &mut self.app {
            app.save(storage);
        } else {
            self.preferences.save(storage);
        }
        if self.app.is_none() || self.window.resizing_from.is_some() {
            self.window.preserve(storage);
        }
    }
    fn update(&mut self, context: &egui::Context, frame: &mut eframe::Frame) {
        self.window.place(context);
        let display_error = self
            .display
            .refresh(frame)
            .err()
            .map(|error| format!("Display color management: {error}"));
        if let Some(app) = &mut self.app {
            let size = context.input(|input| input.viewport().inner_rect.map(|rect| rect.size()));
            self.window.observe_size(size);
            // Window managers may ignore or cap resize requests. The workspace
            // uses the available geometry and never waits for a resize event.
            if !self.observations.attached.load(Ordering::Acquire) {
                self.observations.record(
                    "startup.entry_to_ready",
                    self.observations.began,
                    Instant::now(),
                );
                self.observations.flush();
                self.observations.attached.store(true, Ordering::Release);
                if app.diagnostics.active() {
                    app.diagnostics.set_start_state(app.diagnostic_state());
                }
            }
            app.activity
                .observe_reported(crate::activity::Origin::Display, display_error);
            app.update(context, frame);
            return;
        }
        self.capture.controller().poll(context);
        self.observations.flush();
        if self.error.is_none() {
            if let Err(error) = self.preparation.start_if_ready(
                self.observations.presented.load(Ordering::Acquire),
                !self.capture.controller().starting(),
                context,
            ) {
                crate::support::startup_failure(&error);
                self.error = Some(error);
            }
            if let Some(result) = self.preparation.receive() {
                match result {
                    Ok(mut app) => {
                        // Control observations are thread-local and must start on the UI thread.
                        app.enable_smoke_automation();
                        app.diagnostics = self.capture.take();
                        app.startup_ready = Some(Instant::now());
                        self.window.restore(context);
                        self.app = Some(app);
                        context.request_repaint();
                    }
                    Err(error) => {
                        crate::support::startup_failure(&error);
                        self.error = Some(error);
                    }
                }
            }
        }
        self.show_loading(context);
    }
}

impl Startup {
    fn show_loading(&self, context: &egui::Context) {
        let progress = *self.preparation.progress.lock().unwrap();
        loading_panel(context, &self.icons, progress, self.error.as_deref());
        if self.error.is_none() {
            context.request_repaint_after(Duration::from_millis(16));
        }
    }
}

fn loading_panel(
    context: &egui::Context,
    icons: &[egui::TextureHandle; 2],
    progress: Progress,
    error: Option<&str>,
) {
    egui::CentralPanel::default()
        .frame(egui::Frame::central_panel(&context.style()).inner_margin(24))
        .show(context, |ui| {
            ui.spacing_mut().item_spacing.x = 24.;
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let icon = &icons[usize::from(context.theme() == egui::Theme::Dark)];
                ui.add(
                    egui::Image::new(icon)
                        .fit_to_exact_size(egui::vec2(128., 128.))
                        .alt_text("Foveonic"),
                );
                let content_height = if error.is_some() { 128. } else { 96. };
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), content_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label(
                            egui::RichText::new(preferences::APP_NAME)
                                .size(24.)
                                .strong(),
                        );
                        ui.add_space(10.);
                        if let Some(error) = error {
                            egui::ScrollArea::vertical().max_height(64.).show(ui, |ui| {
                                ui.label("The graphics engine could not be prepared.");
                                ui.label(error);
                                if let Some(log) = foveonic::support::handle() {
                                    let status = log.status();
                                    ui.add(
                                        egui::Label::new(format!(
                                            "Support logs: {}",
                                            status.directory.display()
                                        ))
                                        .wrap()
                                        .selectable(true),
                                    );
                                    if let Some(error) = status.persistence_error {
                                        ui.label(format!("Logs could not be fully saved: {error}"));
                                    }
                                }
                            });
                            if ui.button("Close").clicked() {
                                context.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        } else {
                            ui.label(if progress.total == 0 {
                                "Preparing graphics engine…"
                            } else if progress.completed == progress.total {
                                "Finishing setup…"
                            } else {
                                "Preparing graphics programs…"
                            });
                            ui.add_space(8.);
                            let fraction = if progress.total == 0 {
                                0.
                            } else {
                                progress.completed as f32 / progress.total as f32
                            };
                            let mut bar = egui::ProgressBar::new(fraction)
                                .desired_width(ui.available_width());
                            if progress.total > 0 {
                                bar = bar
                                    .text(format!("{} / {}", progress.completed, progress.total));
                            }
                            ui.add(bar);
                        }
                    },
                );
            });
        });
}
