//! Shares viewport geometry between JPEG previews and GPU images, with diagnostic mapping and manual filtering.

use eframe::{
    egui,
    egui_wgpu::{self, CallbackResources, CallbackTrait},
    wgpu,
};
use foveonic::image::{ColorSpace, ImageTexture, Space};
use std::{collections::HashMap, sync::Arc};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    rect: [f32; 4],
    options: [f32; 4],
}

impl Params {
    fn new(
        canvas: egui::Rect,
        image: egui::Rect,
        space: Space,
        channel: i32,
        srgb_target: bool,
    ) -> Self {
        let origin = image.min - canvas.min;
        let sensor = matches!(space, Space::SensorLayers | Space::CalibratedLayers);
        Self {
            rect: [
                origin.x / canvas.width(),
                origin.y / canvas.height(),
                image.width() / canvas.width(),
                image.height() / canvas.height(),
            ],
            options: [
                if sensor && channel < 0 {
                    0.
                } else {
                    channel as f32
                },
                if sensor || matches!(space, Space::LinearRgb(_)) {
                    1. / 4095.
                } else if space == Space::LuminanceRatio {
                    0.5
                } else if space == Space::SignedDetail {
                    1. / 2000.
                } else {
                    1.
                },
                if space == Space::SignedDetail {
                    -2.
                } else if matches!(space, Space::ScalarField | Space::LuminanceRatio) {
                    -1.
                } else {
                    f32::from(matches!(
                        space,
                        Space::LinearRgb(ColorSpace::AdobeRgb)
                            | Space::ToneMappedRgb(ColorSpace::AdobeRgb)
                    ))
                },
                f32::from(srgb_target),
            ],
        }
    }
}

struct Resources {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    binds: HashMap<&'static str, wgpu::BindGroup>,
}

pub fn install(rs: &egui_wgpu::RenderState) {
    let device = &rs.device;
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Image view"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Image presentation"),
        source: wgpu::ShaderSource::Wgsl(include_str!("view.wgsl").into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Image presentation"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: rs.target_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview: None,
        cache: None,
    });
    rs.renderer.write().callback_resources.insert(Resources {
        pipeline,
        layout,
        binds: HashMap::new(),
    });
}

pub fn clear(rs: &egui_wgpu::RenderState) {
    rs.renderer
        .write()
        .callback_resources
        .get_mut::<Resources>()
        .unwrap()
        .binds
        .clear();
}

struct Draw {
    key: &'static str,
    texture: Arc<ImageTexture>,
    params: Params,
}
impl CallbackTrait for Draw {
    fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        use wgpu::util::DeviceExt;
        let r = resources.get_mut::<Resources>().unwrap();
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("View parameters"),
            contents: bytemuck::bytes_of(&self.params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let view = self.texture.texture().create_view(&Default::default());
        r.binds.insert(
            self.key,
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &r.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            }),
        );
        Vec::new()
    }
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        let r = resources.get::<Resources>().unwrap();
        pass.set_pipeline(&r.pipeline);
        pass.set_bind_group(0, r.binds.get(self.key).unwrap(), &[]);
        pass.draw(0..6, 0..1);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRegion {
    pub top_left: [u32; 2],
    pub bottom_right: [u32; 2],
    pub extent: [u32; 2],
}

pub struct Viewport {
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub one_to_one: bool,
    last_canvas: Option<(egui::Vec2, [u32; 2])>,
    navigator_offset: egui::Vec2,
}
impl Default for Viewport {
    fn default() -> Self {
        Self {
            zoom: 1.,
            pan: egui::Vec2::ZERO,
            one_to_one: false,
            last_canvas: None,
            navigator_offset: egui::Vec2::ZERO,
        }
    }
}
impl Viewport {
    pub fn reset(&mut self) {
        self.zoom = 1.;
        self.pan = egui::Vec2::ZERO;
        self.one_to_one = false;
        self.last_canvas = None;
    }
    /// Allocate the same canvas and interaction geometry for JPEG and pipeline presentation.
    fn layout(&mut self, ui: &mut egui::Ui, extent: [u32; 2]) -> (egui::Rect, egui::Rect) {
        let size = extent.map(|v| v as f32);
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::drag());
        let fit = (rect.width() / size[0]).min(rect.height() / size[1]);
        if self.one_to_one {
            let next = 1. / (fit * ui.ctx().pixels_per_point());
            self.pan *= next / self.zoom;
            self.zoom = next;
            self.one_to_one = false;
        }
        if response.dragged() {
            self.pan += ui.input(|i| i.pointer.delta());
        }
        if response.double_clicked() {
            self.reset();
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0. {
                let old = self.zoom;
                self.zoom = (self.zoom * (scroll * 0.005).exp()).clamp(0.05, 64.);
                if let Some(mouse) = response.hover_pos() {
                    let delta = mouse - rect.center();
                    self.pan = delta - (delta - self.pan) * (self.zoom / old);
                }
            }
        }
        self.last_canvas = Some((rect.size(), extent));
        (rect, self.image_rect(rect, extent))
    }

    fn image_rect(&self, canvas: egui::Rect, extent: [u32; 2]) -> egui::Rect {
        let size = egui::vec2(extent[0] as f32, extent[1] as f32);
        let fit = (canvas.width() / size.x).min(canvas.height() / size.y);
        egui::Rect::from_center_size(canvas.center() + self.pan, size * fit * self.zoom)
    }

    pub fn show_preview(
        &mut self,
        ui: &mut egui::Ui,
        texture: &egui::TextureHandle,
        native_extent: [u32; 2],
    ) {
        let (canvas, image) = self.layout(ui, native_extent);
        let painter = ui
            .painter()
            .with_clip_rect(canvas.intersect(ui.clip_rect()));
        painter.rect_filled(canvas, 0., egui::Color32::from_gray(14));
        painter.image(
            texture.id(),
            image,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            egui::Color32::WHITE,
        );
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        texture: Arc<ImageTexture>,
        channel: i32,
        srgb_target: bool,
    ) {
        let (rect, image) = self.layout(ui, texture.description().extent);
        let params = Params::new(
            rect,
            image,
            texture.description().space,
            channel,
            srgb_target,
        );
        ui.painter().add(egui_wgpu::Callback::new_paint_callback(
            rect,
            Draw {
                key: "main",
                texture,
                params,
            },
        ));
    }

    pub fn visible_uv(&self) -> egui::Rect {
        let Some((size, extent)) = self.last_canvas else {
            return egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.));
        };
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let image = self.image_rect(canvas, extent);
        let visible = canvas.intersect(image);
        egui::Rect::from_min_max(
            egui::pos2(
                (visible.left() - image.left()) / image.width(),
                (visible.top() - image.top()) / image.height(),
            ),
            egui::pos2(
                (visible.right() - image.left()) / image.width(),
                (visible.bottom() - image.top()) / image.height(),
            ),
        )
    }
    /// Inclusive pixel bounds in the cropped, oriented image grid; includes partly visible pixels.
    pub fn visible_pixels(&self) -> Option<PixelRegion> {
        let (_, extent) = self.last_canvas?;
        let uv = self.visible_uv();
        if !uv.is_positive() || !uv.min.is_finite() || !uv.max.is_finite() {
            return None;
        }
        let top_left: [u32; 2] =
            std::array::from_fn(|i| (uv.min[i].clamp(0., 1.) * extent[i] as f32).floor() as u32);
        let end: [u32; 2] =
            std::array::from_fn(|i| (uv.max[i].clamp(0., 1.) * extent[i] as f32).ceil() as u32);
        if end[0] <= top_left[0] || end[1] <= top_left[1] {
            return None;
        }
        Some(PixelRegion {
            top_left,
            bottom_right: end.map(|v| v - 1),
            extent: [end[0] - top_left[0], end[1] - top_left[1]],
        })
    }
    pub fn pan_to(&mut self, uv: egui::Vec2) {
        if let Some((size, extent)) = self.last_canvas {
            let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let display = self.image_rect(canvas, extent).size();
            let half = (size / display * 0.5).min(egui::vec2(0.5, 0.5));
            let uv = uv.max(half).min(egui::vec2(1., 1.) - half);
            self.pan = (egui::vec2(0.5, 0.5) - uv) * display;
        }
    }
    pub fn navigator(
        &mut self,
        ui: &mut egui::Ui,
        texture: Option<Arc<ImageTexture>>,
        preview: Option<&crate::browser::Thumbnail>,
        channel: i32,
        srgb_target: bool,
    ) {
        let extent = texture
            .as_ref()
            .map(|t| t.description().extent)
            .or_else(|| preview.map(|p| p.native_extent));
        let (canvas, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 150.),
            egui::Sense::click_and_drag(),
        );
        ui.painter()
            .rect_filled(canvas, 3., egui::Color32::from_gray(14));
        let Some(extent) = extent else {
            return;
        };
        let size = egui::vec2(extent[0] as f32, extent[1] as f32);
        let fit = (canvas.width() / size.x).min(canvas.height() / size.y);
        let image = egui::Rect::from_center_size(canvas.center(), size * fit);
        if let Some(texture) = texture {
            let params = Params::new(
                canvas,
                image,
                texture.description().space,
                channel,
                srgb_target,
            );
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                canvas,
                Draw {
                    key: "navigator",
                    texture,
                    params,
                },
            ));
        } else if let Some(preview) = preview {
            ui.painter().image(
                preview.texture.id(),
                image,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
        }
        let uv = self.visible_uv();
        if response.drag_started() || response.clicked() {
            let mouse = if response.drag_started() {
                ui.input(|i| i.pointer.press_origin())
            } else {
                response.interact_pointer_pos()
            };
            if let Some(mouse) = mouse {
                let point = (mouse - image.min) / image.size();
                let inside = uv.contains(egui::pos2(point.x, point.y));
                self.navigator_offset = if response.drag_started() && inside {
                    point - uv.center().to_vec2()
                } else {
                    egui::Vec2::ZERO
                };
            }
        }
        if response.dragged() || response.clicked() {
            if let Some(mouse) = response.interact_pointer_pos() {
                self.pan_to((mouse - image.min) / image.size() - self.navigator_offset);
                ui.ctx().request_repaint();
            }
        }
        if response.double_clicked() {
            self.reset();
            ui.ctx().request_repaint();
        }
        let visible = self.visible_uv();
        if visible.is_positive() {
            let overlay = egui::Rect::from_min_max(
                image.min + visible.min.to_vec2() * image.size(),
                image.min + visible.max.to_vec2() * image.size(),
            );
            ui.painter().rect_filled(
                overlay,
                0.,
                egui::Color32::from_rgba_unmultiplied(85, 145, 245, 25),
            );
            ui.painter().rect_stroke(
                overlay,
                0.,
                egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(95, 155, 245)),
                egui::StrokeKind::Inside,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visible_pixel_bounds_follow_fit_zoom_pan_and_oriented_extent() {
        let mut viewport = Viewport {
            last_canvas: Some((egui::vec2(800., 800.), [400, 200])),
            ..Default::default()
        };
        assert_eq!(
            viewport.visible_pixels().unwrap(),
            PixelRegion {
                top_left: [0, 0],
                bottom_right: [399, 199],
                extent: [400, 200]
            }
        );
        viewport.zoom = 2.;
        assert_eq!(
            viewport.visible_pixels().unwrap(),
            PixelRegion {
                top_left: [100, 0],
                bottom_right: [299, 199],
                extent: [200, 200]
            }
        );
        viewport.pan_to(egui::vec2(0.75, 0.5));
        assert_eq!(viewport.visible_pixels().unwrap().top_left, [200, 0]);
        viewport.pan = egui::vec2(10000., 10000.);
        assert!(viewport.visible_pixels().is_none());
        viewport.reset();
        assert!(viewport.visible_pixels().is_none());
        viewport.last_canvas = Some((egui::vec2(800., 800.), [200, 400]));
        assert_eq!(viewport.visible_pixels().unwrap().bottom_right, [199, 399]);
    }

    #[test]
    fn presentation_mapping_respects_buffer_meaning_and_canvas_geometry() {
        let canvas = egui::Rect::from_min_size(egui::pos2(20., 30.), egui::vec2(400., 200.));
        let image = egui::Rect::from_min_size(egui::pos2(120., 80.), egui::vec2(200., 100.));
        for (space, channel, expected) in [
            (Space::SensorLayers, -1, [0., 1. / 4095., 0., 1.]),
            (Space::CalibratedLayers, 2, [2., 1. / 4095., 0., 1.]),
            (Space::ScalarField, -1, [-1., 1., -1., 1.]),
            (Space::LuminanceRatio, -1, [-1., 0.5, -1., 1.]),
            (Space::SignedDetail, -1, [-1., 1. / 2000., -2., 1.]),
            (
                Space::LinearRgb(ColorSpace::Srgb),
                -1,
                [-1., 1. / 4095., 0., 1.],
            ),
            (
                Space::ToneMappedRgb(ColorSpace::AdobeRgb),
                -1,
                [-1., 1., 1., 1.],
            ),
        ] {
            let params = Params::new(canvas, image, space, channel, true);
            assert_eq!(params.rect, [0.25, 0.25, 0.5, 0.5]);
            assert_eq!(params.options, expected);
            assert_eq!(
                Params::new(canvas, image, space, channel, false).options[3],
                0.
            );
        }
    }

    #[test]
    fn navigator_pointer_click_drag_and_double_click_control_the_main_view() {
        let ctx = egui::Context::default();
        let preview = crate::browser::Thumbnail {
            texture: ctx.load_texture(
                "navigator fixture",
                egui::ColorImage::new([2, 2], egui::Color32::WHITE),
                Default::default(),
            ),
            native_extent: [900, 600],
        };
        let mut viewport = Viewport {
            zoom: 2.,
            last_canvas: Some((egui::vec2(600., 400.), [900, 600])),
            ..Default::default()
        };
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400., 200.),
            )),
            ..Default::default()
        };
        let run = |viewport: &mut Viewport, time: f64, events: Vec<egui::Event>| {
            let mut input = input.clone();
            input.time = Some(time);
            input.events = events;
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    viewport.navigator(ui, None, Some(&preview), -1, false)
                });
            });
        };
        run(&mut viewport, 0., vec![]);
        let point = egui::pos2(233.75, 83.); // miniature coordinate (.65,.5)
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run(
            &mut viewport,
            0.1,
            vec![egui::Event::PointerMoved(point), button(point, true)],
        );
        run(&mut viewport, 0.2, vec![button(point, false)]);
        assert!((viewport.pan.x + 180.).abs() < 0.01);
        let start = point + egui::vec2(10., 0.);
        let end = start + egui::vec2(18., 0.);
        run(
            &mut viewport,
            0.5,
            vec![egui::Event::PointerMoved(start), button(start, true)],
        );
        run(&mut viewport, 0.6, vec![egui::Event::PointerMoved(end)]);
        run(&mut viewport, 0.7, vec![button(end, false)]);
        assert!((viewport.pan.x + 276.).abs() < 0.01); // retains grab offset; 18 miniature pixels = 96 canvas pixels
        viewport.one_to_one = true;
        for (time, pressed) in [(1., true), (1.05, false), (1.1, true), (1.15, false)] {
            run(
                &mut viewport,
                time,
                vec![egui::Event::PointerMoved(end), button(end, pressed)],
            );
        }
        assert_eq!(viewport.zoom, 1.);
        assert_eq!(viewport.pan, egui::Vec2::ZERO);
        assert!(!viewport.one_to_one);
        assert_eq!(
            viewport.visible_uv(),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.))
        );
    }

    #[test]
    fn navigator_pan_tracks_visible_normalized_region() {
        let mut viewport = Viewport {
            zoom: 2.,
            last_canvas: Some((egui::vec2(600., 400.), [900, 600])),
            ..Default::default()
        };
        assert_eq!(
            viewport.visible_uv(),
            egui::Rect::from_min_max(egui::pos2(0.25, 0.25), egui::pos2(0.75, 0.75))
        );
        viewport.pan_to(egui::vec2(0.75, 0.5));
        assert_eq!(viewport.pan, egui::vec2(-300., 0.));
        assert_eq!(
            viewport.visible_uv(),
            egui::Rect::from_min_max(egui::pos2(0.5, 0.25), egui::pos2(1., 0.75))
        );
        viewport.pan_to(egui::vec2(2., -1.));
        assert_eq!(
            viewport.visible_uv(),
            egui::Rect::from_min_max(egui::pos2(0.5, 0.), egui::pos2(1., 0.5))
        );
    }

    #[test]
    fn fit_centers_portrait_and_landscape_in_the_same_canvas() {
        let viewport = Viewport::default();
        let canvas = egui::Rect::from_min_size(egui::pos2(20., 30.), egui::vec2(800., 600.));
        for extent in [[3136, 4704], [4704, 3136]] {
            let image = viewport.image_rect(canvas, extent);
            assert_eq!(image.center(), canvas.center());
            assert!(image.width() <= canvas.width() + 0.001);
            assert!(image.height() <= canvas.height() + 0.001);
            assert!(
                (image.width() - canvas.width()).abs() < 0.001
                    || (image.height() - canvas.height()).abs() < 0.001
            );
        }
    }

    #[test]
    fn thumbnail_and_pipeline_share_bounds_and_native_pixel_zoom() {
        let ctx = egui::Context::default();
        let thumb = ctx.load_texture(
            "tiny preview",
            egui::ColorImage::new([147, 221], egui::Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 600.),
            )),
            ..Default::default()
        };
        let extent = [3136, 4704];
        for native_zoom in [false, true] {
            let mut viewport = Viewport {
                zoom: 2.,
                pan: egui::vec2(47., -31.),
                one_to_one: native_zoom,
                ..Default::default()
            };
            let preview = ctx.run(input.clone(), |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| viewport.show_preview(ui, &thumb, extent));
            });
            let mesh = preview
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == thumb.id() => Some(mesh),
                    _ => None,
                })
                .unwrap();
            let mut preview_bounds = egui::Rect::NOTHING;
            for vertex in &mesh.vertices {
                preview_bounds.extend_with(vertex.pos);
            }
            let mut pipeline_bounds = egui::Rect::NOTHING;
            let _ = ctx.run(input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    pipeline_bounds = viewport.layout(ui, extent).1;
                });
            });
            assert_eq!(preview_bounds, pipeline_bounds);
            if native_zoom {
                assert!(
                    (pipeline_bounds.width() - extent[0] as f32 / ctx.pixels_per_point()).abs()
                        < 0.001
                );
                assert!(
                    (pipeline_bounds.height() - extent[1] as f32 / ctx.pixels_per_point()).abs()
                        < 0.001
                );
            } else {
                assert_eq!(viewport.zoom, 2.);
                assert_eq!(viewport.pan, egui::vec2(47., -31.));
            }
        }
    }
}
