//! Compares extended photographic colors, sRGB clipping, neutral ramps and ordinary egui output in a managed window.

#[path = "../src/display.rs"]
mod display;
use eframe::{egui, egui_wgpu, wgpu};

struct Probe {
    host: display::Host,
    began: std::time::Instant,
}
struct Paint;
struct Resources(wgpu::RenderPipeline);
impl egui_wgpu::CallbackTrait for Paint {
    fn paint(
        &self,
        _: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        pass.set_pipeline(&resources.get::<Resources>().unwrap().0);
        pass.draw(0..3, 0..1);
    }
}
impl Probe {
    fn new(
        cc: &eframe::CreationContext<'_>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let host = display::Host::new(cc)?;
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .ok_or("missing display device")?;
        let shader = rs
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Managed color probe"),
                source: wgpu::ShaderSource::Wgsl(include_str!("managed_color_probe.wgsl").into()),
            });
        let pipeline = rs
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Managed color probe"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    targets: &[Some(rs.target_format.into())],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
                cache: None,
            });
        rs.renderer
            .write()
            .callback_resources
            .insert(Resources(pipeline));
        eprintln!(
            "display probe: format={:?}, target={:?}",
            rs.target_format, host.target
        );
        if !host.target.managed() {
            return Err("extended floating presentation is unavailable".into());
        }
        Ok(Self {
            host,
            began: std::time::Instant::now(),
        })
    }
}
impl eframe::App for Probe {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.host
            .refresh(frame)
            .expect("managed layer must remain tagged");
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Managed color experiment");
            ui.label("Top: Adobe RGB colors retained. Middle: the same colors clipped to sRGB. Bottom: neutral ramp.");
            ui.label("The first two rows should differ in saturated colors on a wide-gamut display. Gray and white should stay neutral.");
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::from_rgb(70,130,180), "sRGB steelblue · 70, 130, 180");
                ui.checkbox(&mut false, "Ordinary UI");
                ui.button("Text and edges").on_hover_text("Check text in both light and dark themes.");
                egui::widgets::global_theme_preference_buttons(ui);
            });
            let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(rect, Paint));
        });
        if let Some(milliseconds) = std::env::var("FOVEONIC_DISPLAY_PROBE_EXIT_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
        {
            if self.began.elapsed() >= std::time::Duration::from_millis(milliseconds) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
    }
}
fn main() -> eframe::Result {
    let isolation =
        tempfile::tempdir().map_err(|error| eframe::Error::AppCreation(Box::new(error)))?;
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        persistence_path: Some(isolation.path().join("app.ron")),
        viewport: egui::ViewportBuilder::default()
            .with_app_id("foveonic-color-probe")
            .with_inner_size([1000., 600.]),
        wgpu_options: display::configure(Default::default()),
        ..Default::default()
    };
    eframe::run_native(
        "Foveonic color probe",
        options,
        Box::new(|cc| Ok(Box::new(Probe::new(cc)?))),
    )
}
