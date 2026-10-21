//! Shares viewport geometry between embedded previews and GPU images, with diagnostic mapping and manual filtering.

use crate::preferences::ZoomPreset;
use eframe::{
    egui,
    egui_wgpu::{self, CallbackResources, CallbackTrait},
    wgpu,
};
#[cfg(test)]
use foveonic::image::ColorSpace;
use foveonic::image::{ImageTexture, Space};
use std::{collections::HashMap, sync::Arc};

/// Display-only side, component, and difference amplification. Zero gain shows the selected side normally.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mapping {
    pub channel: i32,
    pub input: bool,
    pub difference_gain: f32,
}
impl Mapping {
    pub fn normal(channel: i32) -> Self {
        Self {
            channel,
            input: false,
            difference_gain: 0.,
        }
    }
    fn apply(self, mut params: Params) -> Params {
        if self.difference_gain > 0. {
            params.options[1] *= 0.5 * self.difference_gain;
            params.options[2] = -3.;
        }
        params
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    rect: [f32; 4],
    options: [f32; 4],
}

impl Params {
    /// Fit against the original image, but draw only inside the visible canvas.
    /// egui-wgpu clamps offscreen callback viewports, which would otherwise squeeze the image.
    fn clipped(
        canvas: egui::Rect,
        image: egui::Rect,
        clip_rect: egui::Rect,
        space: Space,
        channel: i32,
        srgb_target: bool,
    ) -> Option<(egui::Rect, Self)> {
        let visible = canvas.intersect(clip_rect);
        visible.is_positive().then(|| {
            (
                visible,
                Self::new(visible, image, space, channel, srgb_target),
            )
        })
    }

    fn new(
        canvas: egui::Rect,
        image: egui::Rect,
        space: Space,
        channel: i32,
        srgb_target: bool,
    ) -> Self {
        let origin = image.min - canvas.min;
        let sensor14 = matches!(
            space,
            Space::SensorRgb
                | Space::SensorBlue
                | Space::CorrectedSensorRgb
                | Space::CorrectedSensorBlue
        );
        let sensor = matches!(
            space,
            Space::SensorLayers
                | Space::CalibratedLayers
                | Space::SensorRgb
                | Space::CorrectedSensorRgb
        );
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
                if sensor14 {
                    1. / 16383.
                } else if sensor || matches!(space, Space::LinearRgb(_) | Space::Monochrome) {
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
                } else if space.buffer_layout() == foveonic::image::Layout::Scalar32Float {
                    -1.
                } else {
                    match space {
                        Space::LinearRgb(space) | Space::ToneMappedRgb(space) => {
                            space as u32 as f32
                        }
                        _ => 0.,
                    }
                },
                f32::from(srgb_target),
            ],
        }
    }
}

struct Resources {
    pipeline: wgpu::RenderPipeline,
    navigator_pipeline: wgpu::RenderPipeline,
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
                binding: 2,
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
    let pipeline_for = |format| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        })
    };
    let pipeline = pipeline_for(rs.target_format);
    let navigator_pipeline = pipeline_for(wgpu::TextureFormat::Rgba8UnormSrgb);
    rs.renderer.write().callback_resources.insert(Resources {
        pipeline,
        navigator_pipeline,
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

/// Reuse the main viewport's mapping so the navigator shows the same image.
/// Prepare the small tile only when content or mapping changes. Pan and zoom
/// should not put another preview conversion ahead of the next photo edit.
pub(crate) fn prepare_navigator(
    rs: &egui_wgpu::RenderState,
    source: &wgpu::Texture,
    space: Space,
    target: &wgpu::Texture,
    mapping: Mapping,
    input: Option<&wgpu::Texture>,
) {
    use wgpu::util::DeviceExt;
    let full = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.));
    let params = mapping.apply(Params::new(full, full, space, mapping.channel, true));
    let uniform = rs
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Navigator mapping"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
    let view = source.create_view(&Default::default());
    let input_view = input.unwrap_or(source).create_view(&Default::default());
    let target_view = target.create_view(&Default::default());
    let renderer = rs.renderer.write();
    let resources = renderer.callback_resources.get::<Resources>().unwrap();
    let bind = rs.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Navigator source"),
        layout: &resources.layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&input_view),
            },
        ],
    });
    let mut encoder = rs
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Small navigator presentation"),
        });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Small navigator presentation"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&resources.navigator_pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..6, 0..1);
    }
    rs.queue.submit(Some(encoder.finish()));
}

struct Draw {
    key: &'static str,
    texture: Arc<ImageTexture>,
    input: Option<Arc<ImageTexture>>,
    params: Params,
    probe: crate::timing::DrawProbe,
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
        let _span = self.probe.prepare();
        use wgpu::util::DeviceExt;
        let r = resources.get_mut::<Resources>().unwrap();
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("View parameters"),
            contents: bytemuck::bytes_of(&self.params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let view = self.texture.texture().create_view(&Default::default());
        let input_view = self
            .input
            .as_ref()
            .unwrap_or(&self.texture)
            .texture()
            .create_view(&Default::default());
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
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&input_view),
                    },
                ],
            }),
        );
        self.probe.prepared();
        Vec::new()
    }
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        let _span = self.probe.encode();
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
    /// None follows Fit. Some fixes the number of UI points per image pixel.
    scale: Option<f32>,
    pub pan: egui::Vec2,
    pub one_to_one: bool,
    pub zoom_snap: ZoomPreset,
    last_canvas: Option<(egui::Vec2, [u32; 2])>,
    navigator_offset: egui::Vec2,
}
impl Default for Viewport {
    fn default() -> Self {
        Self {
            scale: None,
            pan: egui::Vec2::ZERO,
            one_to_one: false,
            zoom_snap: ZoomPreset::default(),
            last_canvas: None,
            navigator_offset: egui::Vec2::ZERO,
        }
    }
}
impl Viewport {
    pub fn reset(&mut self) {
        self.scale = None;
        self.pan = egui::Vec2::ZERO;
        self.one_to_one = false;
        self.last_canvas = None;
    }
    fn fit_scale(size: egui::Vec2, extent: [u32; 2]) -> f32 {
        (size.x / extent[0] as f32).min(size.y / extent[1] as f32)
    }

    /// Zoom around the image currently at the canvas centre, leaving Fit mode.
    pub fn zoom_by(&mut self, factor: f32) {
        if let Some((size, extent)) = self.last_canvas {
            let scale = self.scale.unwrap_or_else(|| Self::fit_scale(size, extent));
            self.set_scale_at(scale * factor, egui::Vec2::ZERO);
        }
    }

    /// Set UI points per image pixel, preserving the image point at an offset from canvas centre.
    fn set_scale_at(&mut self, next: f32, anchor: egui::Vec2) {
        if let Some((size, extent)) = self.last_canvas {
            let old = self.scale.unwrap_or_else(|| Self::fit_scale(size, extent));
            if old > 0. {
                self.pan = anchor - (anchor - self.pan) * (next / old);
            }
            self.scale = Some(next);
            self.one_to_one = false;
        }
    }

    fn toggle_fit_snap(&mut self, pixels_per_point: f32, anchor: egui::Vec2) {
        if self.scale.is_none() && !self.one_to_one {
            self.set_scale_at(self.zoom_snap.multiple() / pixels_per_point, anchor);
        } else {
            let canvas = self.last_canvas;
            self.reset();
            self.last_canvas = canvas;
        }
    }

    fn zoom_percent(&self, pixels_per_point: f32) -> Option<f32> {
        if self.one_to_one {
            return Some(100.);
        }
        let (size, extent) = self.last_canvas?;
        let scale = self.scale.unwrap_or_else(|| Self::fit_scale(size, extent));
        Some(scale * pixels_per_point * 100.)
    }

    fn navigator_zoom_controls(&mut self, ui: &mut egui::Ui) {
        let pixels_per_point = ui.ctx().pixels_per_point();
        crate::widgets::control_row(ui, |ui| {
            let zoom = self
                .zoom_percent(pixels_per_point)
                .map(|percent| format!("Zoom {percent:.0}%"))
                .unwrap_or_else(|| "Zoom —".into());
            ui.label(zoom);
            for preset in ZoomPreset::ALL {
                let response = crate::help::attach(
                    crate::widgets::button(ui, preset.label(), false, egui::Vec2::ZERO),
                    &crate::help::navigator(ui.ctx().os()),
                );
                if response.clicked() {
                    if ui.input(|i| i.modifiers.command) {
                        self.zoom_snap = preset;
                    } else {
                        self.set_scale_at(preset.multiple() / pixels_per_point, egui::Vec2::ZERO);
                    }
                    ui.ctx().request_repaint();
                }
                if preset == self.zoom_snap {
                    let rect = response.rect;
                    let y = rect.bottom() - 2.;
                    ui.painter().line_segment(
                        [
                            egui::pos2(rect.left() + 5., y),
                            egui::pos2(rect.right() - 5., y),
                        ],
                        egui::Stroke::new(2_f32, ui.visuals().selection.stroke.color),
                    );
                }
            }
        });
    }

    fn reframe(&mut self, size: egui::Vec2, extent: [u32; 2]) {
        if let (Some(scale), Some((old_size, old_extent))) = (self.scale, self.last_canvas) {
            if size != old_size && extent == old_extent && size.x > 0. && size.y > 0. {
                let old_canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, old_size);
                let old_image = self.image_rect(old_canvas, extent);
                let display = egui::vec2(extent[0] as f32, extent[1] as f32) * scale;
                // Rounding to the information-bar coordinates would make the image jump
                // during resize. Keep the precise origin and center fully visible axes.
                for axis in 0..2 {
                    self.pan[axis] = if display[axis] <= size[axis] {
                        0.
                    } else {
                        old_image.min[axis].min(0.) + (display[axis] - size[axis]) * 0.5
                    };
                }
            }
        }
        if size.x > 0. && size.y > 0. {
            self.last_canvas = Some((size, extent));
        }
    }

    /// Allocate the same canvas and interaction geometry for JPEG and pipeline presentation.
    fn layout(&mut self, ui: &mut egui::Ui, extent: [u32; 2]) -> (egui::Rect, egui::Rect) {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        self.reframe(rect.size(), extent);
        let fit = Self::fit_scale(rect.size(), extent);
        if self.one_to_one {
            self.set_scale_at(1. / ui.ctx().pixels_per_point(), egui::Vec2::ZERO);
        }
        if response.dragged() {
            self.scale.get_or_insert(fit);
            self.pan += ui.input(|i| i.pointer.delta());
        }
        if response.double_clicked() {
            let anchor = response
                .interact_pointer_pos()
                .map(|mouse| mouse - rect.center())
                .unwrap_or(egui::Vec2::ZERO);
            self.toggle_fit_snap(ui.ctx().pixels_per_point(), anchor);
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0. {
                let old = self.scale.unwrap_or(fit);
                let pixels_per_point = ui.ctx().pixels_per_point();
                let next = (old * (scroll * 0.005).exp())
                    .clamp(0.05 / pixels_per_point, 64. / pixels_per_point);
                let anchor = response
                    .hover_pos()
                    .map(|mouse| mouse - rect.center())
                    .unwrap_or(egui::Vec2::ZERO);
                self.set_scale_at(next, anchor);
            }
        }
        if rect.width() > 0. && rect.height() > 0. {
            self.last_canvas = Some((rect.size(), extent));
        }
        (rect, self.image_rect(rect, extent))
    }

    fn image_rect(&self, canvas: egui::Rect, extent: [u32; 2]) -> egui::Rect {
        let size = egui::vec2(extent[0] as f32, extent[1] as f32);
        let scale = self
            .scale
            .unwrap_or_else(|| Self::fit_scale(canvas.size(), extent));
        egui::Rect::from_center_size(canvas.center() + self.pan, size * scale)
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
        input: Option<Arc<ImageTexture>>,
        mapping: Mapping,
        srgb_target: bool,
        revision: u64,
    ) {
        let (rect, image) = self.layout(ui, texture.description().extent);
        let Some((visible, params)) = Params::clipped(
            rect,
            image,
            ui.clip_rect().intersect(ui.ctx().screen_rect()),
            texture.description().space,
            mapping.channel,
            srgb_target,
        ) else {
            return;
        };
        ui.painter().add(egui_wgpu::Callback::new_paint_callback(
            visible,
            Draw {
                key: "main",
                texture,
                input,
                params: mapping.apply(params),
                probe: crate::timing::DrawProbe::new(revision),
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
    /// Include partly visible pixels so the readout describes everything on screen.
    /// Bounds are inclusive and use the cropped, oriented image grid.
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
            self.scale.get_or_insert(Self::fit_scale(size, extent));
            let display = self.image_rect(canvas, extent).size();
            let half = (size / display * 0.5).min(egui::vec2(0.5, 0.5));
            let uv = uv.max(half).min(egui::vec2(1., 1.) - half);
            self.pan = (egui::vec2(0.5, 0.5) - uv) * display;
        }
    }
    pub fn navigator(
        &mut self,
        ui: &mut egui::Ui,
        texture: Option<crate::navigator::Image>,
        preview: Option<&crate::browser::Thumbnail>,
    ) {
        let extent = texture
            .as_ref()
            .map(|t| t.extent)
            .or_else(|| preview.map(|p| p.native_extent));
        let (canvas, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 150.),
            egui::Sense::click_and_drag(),
        );
        ui.painter()
            .rect_filled(canvas, 3., egui::Color32::from_gray(14));
        let Some(extent) = extent else {
            self.navigator_zoom_controls(ui);
            return;
        };
        let size = egui::vec2(extent[0] as f32, extent[1] as f32);
        let fit = (canvas.width() / size.x).min(canvas.height() / size.y);
        let image = egui::Rect::from_center_size(canvas.center(), size * fit);
        if let Some(texture) = texture {
            if let Some(id) = texture.id {
                ui.painter().image(
                    id,
                    image,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    egui::Color32::WHITE,
                );
            }
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
        if response.double_clicked() {
            let fitted = self.scale.is_none() && !self.one_to_one;
            self.toggle_fit_snap(ui.ctx().pixels_per_point(), egui::Vec2::ZERO);
            if fitted {
                if let Some(mouse) = response.interact_pointer_pos() {
                    self.pan_to((mouse - image.min) / image.size());
                }
            }
            ui.ctx().request_repaint();
        } else if self.scale.is_some() && (response.dragged() || response.clicked()) {
            // A first click must leave Fit unchanged, or the following double-click
            // would start from the wrong zoom state.
            if let Some(mouse) = response.interact_pointer_pos() {
                self.pan_to((mouse - image.min) / image.size() - self.navigator_offset);
                ui.ctx().request_repaint();
            }
        }
        self.navigator_zoom_controls(ui);
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
    fn ordinary_navigator_clips_without_compression_and_overlay_tracks_original_geometry() {
        for offset in [0., -60., -140.] {
            let ctx = egui::Context::default();
            let mut viewport = Viewport {
                scale: Some(4. / 3.),
                last_canvas: Some((egui::vec2(600., 400.), [900, 600])),
                ..Default::default()
            };
            let clip = egui::Rect::from_min_max(egui::pos2(8., 8.), egui::pos2(392., 250.));
            let result = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 300.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.set_clip_rect(clip);
                        ui.add_space(offset);
                        viewport.navigator(
                            ui,
                            Some(crate::navigator::Image {
                                id: Some(egui::TextureId::User(7)),
                                extent: [900, 600],
                            }),
                            None,
                        );
                    });
                },
            );
            assert!(!result
                .shapes
                .iter()
                .any(|shape| matches!(shape.shape, egui::Shape::Callback(_))));
            let mesh = result
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == egui::TextureId::User(7) => {
                        Some(mesh)
                    }
                    _ => None,
                })
                .unwrap();
            let mut rect = egui::Rect::NOTHING;
            for vertex in &mesh.vertices {
                rect.extend_with(vertex.pos);
                assert!(vertex.uv.x == 0. || vertex.uv.x == 1.);
                assert!(vertex.uv.y == 0. || vertex.uv.y == 1.);
            }
            assert!((rect.width() - 225.).abs() < 0.001);
            assert!((rect.height() - 150.).abs() < 0.001);
            let uv = viewport.visible_uv();
            let overlay = egui::Rect::from_min_max(
                rect.min + uv.min.to_vec2() * rect.size(),
                rect.min + uv.max.to_vec2() * rect.size(),
            );
            assert!(result
                .shapes
                .iter()
                .any(|shape| matches!(&shape.shape,egui::Shape::Rect(r)if r.rect==overlay)));
            assert_eq!(viewport.pan, egui::Vec2::ZERO);
        }
    }
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
        viewport.zoom_by(2.);
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
    fn difference_mapping_is_signed_display_gray_with_space_aware_amplification() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200., 100.));
        for (space, expected) in [
            (Space::CalibratedLayers, 16. / 4095.),
            (Space::SensorRgb, 16. / 16383.),
            (Space::CorrectedSensorRgb, 16. / 16383.),
            (Space::LinearRgb(ColorSpace::AdobeRgb), 16. / 4095.),
            (Space::ToneMappedRgb(ColorSpace::Srgb), 16.),
        ] {
            let normal = Params::new(rect, rect, space, 1, true);
            let mapped = Mapping {
                channel: 1,
                input: false,
                difference_gain: 32.,
            }
            .apply(normal);
            assert_eq!(mapped.options, [1., expected, -3., 1.]);
            assert_eq!(Mapping::normal(1).apply(normal).options, normal.options);
        }
    }

    #[test]
    fn presentation_mapping_respects_buffer_meaning_and_canvas_geometry() {
        let canvas = egui::Rect::from_min_size(egui::pos2(20., 30.), egui::vec2(400., 200.));
        let image = egui::Rect::from_min_size(egui::pos2(120., 80.), egui::vec2(200., 100.));
        for (space, channel, expected) in [
            (Space::SensorLayers, -1, [0., 1. / 4095., 0., 1.]),
            (Space::SensorRgb, -1, [0., 1. / 16383., 0., 1.]),
            (Space::SensorRgb, 2, [2., 1. / 16383., 0., 1.]),
            (Space::CorrectedSensorRgb, 1, [1., 1. / 16383., 0., 1.]),
            (Space::SensorBlue, -1, [-1., 1. / 16383., -1., 1.]),
            (Space::CorrectedSensorBlue, -1, [-1., 1. / 16383., -1., 1.]),
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
                Space::ToneMappedRgb(ColorSpace::ProPhotoRgb),
                -1,
                [-1., 1., 2., 1.],
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
    fn clipped_gpu_canvas_preserves_image_scale_and_overlay_coordinates() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400., 300.));
        let clip = egui::Rect::from_min_max(egui::pos2(8., 10.), egui::pos2(392., 290.));
        for extent in [[900, 600], [600, 900]] {
            for origin in [
                egui::pos2(8., 20.),  // fully visible
                egui::pos2(8., -60.), // scrolled above the window
                egui::pos2(8., 230.), // partly below the window
                egui::pos2(-60., 20.),
                egui::pos2(300., 20.),
            ] {
                let canvas = egui::Rect::from_min_size(origin, egui::vec2(384., 150.));
                let image = Viewport::default().image_rect(canvas, extent);
                let (visible, params) = Params::clipped(
                    canvas,
                    image,
                    clip.intersect(screen),
                    Space::CalibratedLayers,
                    0,
                    false,
                )
                .unwrap();
                assert_eq!(visible, canvas.intersect(clip));
                assert!(
                    (egui::vec2(params.rect[2], params.rect[3]) * visible.size() - image.size())
                        .length()
                        < 0.001
                );
                // The shader's sampled UV must agree with the original image and navigator overlay.
                for point in [visible.min, visible.center(), visible.max] {
                    let canvas_uv = (point - visible.min) / visible.size();
                    let shader_uv = (canvas_uv - egui::vec2(params.rect[0], params.rect[1]))
                        / egui::vec2(params.rect[2], params.rect[3]);
                    let image_uv = (point - image.min) / image.size();
                    assert!((shader_uv - image_uv).length() < 1e-6);
                }
                for pixels_per_point in [1., 2.] {
                    let info = egui::PaintCallbackInfo {
                        viewport: visible,
                        clip_rect: clip,
                        pixels_per_point,
                        screen_size_px: [
                            (screen.width() * pixels_per_point) as u32,
                            (screen.height() * pixels_per_point) as u32,
                        ],
                    };
                    let viewport = info.viewport_in_pixels();
                    assert_eq!(viewport.width_px as f32, visible.width() * pixels_per_point);
                    assert_eq!(
                        viewport.height_px as f32,
                        visible.height() * pixels_per_point
                    );
                }
            }
        }
        for top in [-200., 310.] {
            let canvas = egui::Rect::from_min_size(egui::pos2(8., top), egui::vec2(384., 150.));
            assert!(Params::clipped(canvas, canvas, clip, Space::ScalarField, -1, false).is_none());
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
            scale: Some(4. / 3.),
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
                egui::CentralPanel::default()
                    .show(ctx, |ui| viewport.navigator(ui, None, Some(&preview)));
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
        assert!((viewport.pan.x + 276.).abs() < 0.01); // Keep the grab offset. Here 18 navigator pixels span 96 canvas pixels.
        viewport.one_to_one = true;
        for (time, pressed) in [(1., true), (1.05, false), (1.1, true), (1.15, false)] {
            run(
                &mut viewport,
                time,
                vec![egui::Event::PointerMoved(end), button(end, pressed)],
            );
        }
        assert_eq!(viewport.scale, None);
        assert_eq!(viewport.pan, egui::Vec2::ZERO);
        assert!(!viewport.one_to_one);
        assert_eq!(
            viewport.visible_uv(),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.))
        );
    }

    #[test]
    fn double_click_toggles_fit_and_snap_zoom_in_both_views() {
        for navigator in [false, true] {
            for pixels_per_point in [1., 2.] {
                for zoom_snap in ZoomPreset::ALL {
                    for initial_zoom in [None, Some(1.), Some(2.), Some(4.), Some(0.05)] {
                        let ctx = egui::Context::default();
                        ctx.set_pixels_per_point(pixels_per_point);
                        let extent = [900, 600];
                        let mut viewport = Viewport {
                            scale: initial_zoom.map(|zoom| zoom / pixels_per_point),
                            zoom_snap,
                            last_canvas: Some((egui::vec2(600., 400.), extent)),
                            ..Default::default()
                        };
                        let run = |viewport: &mut Viewport, time, events| {
                            let mut bounds = (egui::Rect::NOTHING, egui::Rect::NOTHING);
                            let _ = ctx.run(
                                egui::RawInput {
                                    screen_rect: Some(egui::Rect::from_min_size(
                                        egui::Pos2::ZERO,
                                        egui::vec2(400., 240.),
                                    )),
                                    time: Some(time),
                                    events,
                                    ..Default::default()
                                },
                                |ctx| {
                                    egui::CentralPanel::default().show(ctx, |ui| {
                                        if navigator {
                                            viewport.navigator(
                                                ui,
                                                Some(crate::navigator::Image {
                                                    id: Some(egui::TextureId::User(7)),
                                                    extent,
                                                }),
                                                None,
                                            );
                                        } else {
                                            bounds = viewport.layout(ui, extent);
                                        }
                                    });
                                },
                            );
                            bounds
                        };
                        run(&mut viewport, 0., vec![]); // Apply the display-density change first.
                        let (canvas, before) = run(&mut viewport, 0.01, vec![]);
                        let point = egui::pos2(190., 83.);
                        for (time, pressed) in
                            [(0.1, true), (0.15, false), (0.2, true), (0.25, false)]
                        {
                            run(
                                &mut viewport,
                                time,
                                vec![
                                    egui::Event::PointerMoved(point),
                                    egui::Event::PointerButton {
                                        pos: point,
                                        button: egui::PointerButton::Primary,
                                        pressed,
                                        modifiers: Default::default(),
                                    },
                                ],
                            );
                            if initial_zoom.is_none() && time == 0.15 {
                                assert_eq!(viewport.scale, None);
                            }
                        }
                        assert_eq!(
                            viewport.scale,
                            initial_zoom
                                .is_none()
                                .then_some(zoom_snap.multiple() / pixels_per_point),
                            "navigator={navigator}, density={pixels_per_point}, initial={initial_zoom:?}"
                        );
                        if initial_zoom.is_some() {
                            assert_eq!(viewport.pan, egui::Vec2::ZERO);
                        } else if !navigator {
                            let after = viewport.image_rect(canvas, extent);
                            let before_uv = (point - before.min) / before.size();
                            let after_uv = (point - after.min) / after.size();
                            assert!(
                                (after_uv - before_uv).length() < 1e-6,
                                "density={pixels_per_point}, before_uv={before_uv:?}, after_uv={after_uv:?}"
                            );
                            assert_ne!(viewport.pan, egui::Vec2::ZERO);
                        }
                        assert!(!viewport.one_to_one);
                        assert!(viewport.zoom_percent(pixels_per_point).is_some());
                    }
                }
            }
        }
    }

    #[test]
    fn navigator_double_click_zooms_into_clicked_image_location() {
        for pixels_per_point in [1., 2.] {
            for extent in [[4704, 3136], [3136, 4704]] {
                for uv in [egui::vec2(0.65, 0.35), egui::vec2(0.01, 0.99)] {
                    let ctx = egui::Context::default();
                    ctx.set_pixels_per_point(pixels_per_point);
                    let size = egui::vec2(600., 400.);
                    let mut viewport = Viewport {
                        last_canvas: Some((size, extent)),
                        ..Default::default()
                    };
                    let run = |viewport: &mut Viewport, time, events| {
                        ctx.run(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(400., 240.),
                                )),
                                time: Some(time),
                                events,
                                ..Default::default()
                            },
                            |ctx| {
                                egui::CentralPanel::default().show(ctx, |ui| {
                                    viewport.navigator(
                                        ui,
                                        Some(crate::navigator::Image {
                                            id: Some(egui::TextureId::User(7)),
                                            extent,
                                        }),
                                        None,
                                    );
                                });
                            },
                        )
                    };
                    run(&mut viewport, 0., vec![]); // Apply the display-density change first.
                    let output = run(&mut viewport, 0.01, vec![]);
                    let mesh = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Mesh(mesh)
                                if mesh.texture_id == egui::TextureId::User(7) =>
                            {
                                Some(mesh)
                            }
                            _ => None,
                        })
                        .unwrap();
                    let mut image = egui::Rect::NOTHING;
                    for vertex in &mesh.vertices {
                        image.extend_with(vertex.pos);
                    }
                    let point = image.min + uv * image.size();
                    for (time, pressed) in [(0.1, true), (0.15, false), (0.2, true), (0.25, false)]
                    {
                        run(
                            &mut viewport,
                            time,
                            vec![
                                egui::Event::PointerMoved(point),
                                egui::Event::PointerButton {
                                    pos: point,
                                    button: egui::PointerButton::Primary,
                                    pressed,
                                    modifiers: Default::default(),
                                },
                            ],
                        );
                    }
                    assert_eq!(
                        viewport.scale,
                        Some(2. / pixels_per_point),
                        "density={pixels_per_point}, extent={extent:?}, uv={uv:?}, image={image:?}, point={point:?}"
                    );
                    let display =
                        egui::vec2(extent[0] as f32, extent[1] as f32) * 2. / pixels_per_point;
                    let half = size / display * 0.5;
                    let expected = uv.max(half).min(egui::vec2(1., 1.) - half);
                    assert!((viewport.visible_uv().center().to_vec2() - expected).length() < 1e-6);
                    assert_ne!(viewport.pan, egui::Vec2::ZERO);
                }
            }
        }
    }

    #[test]
    fn navigator_zoom_buttons_and_readout_use_physical_pixels() {
        for pixels_per_point in [1., 2.] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(pixels_per_point);
            let mut viewport = Viewport {
                last_canvas: Some((egui::vec2(600., 400.), [900, 600])),
                ..Default::default()
            };
            let run = |viewport: &mut Viewport, time, events| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(400., 240.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            viewport.navigator(
                                ui,
                                Some(crate::navigator::Image {
                                    id: Some(egui::TextureId::User(7)),
                                    extent: [900, 600],
                                }),
                                None,
                            );
                        });
                    },
                )
            };
            let mut output = run(&mut viewport, 0., vec![]);
            let expected_fit = 600. / 900. * pixels_per_point * 100.;
            assert!(
                (viewport.zoom_percent(pixels_per_point).unwrap() - expected_fit).abs() < 0.001
            );
            for (index, percent) in [100., 200., 400.].into_iter().enumerate() {
                let label = format!("{percent:.0}%");
                let point = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => {
                            Some(text.pos + text.galley.size() * 0.5)
                        }
                        _ => None,
                    })
                    .unwrap();
                assert!(point.y > 158.); // Controls are below the 150-point miniature.
                viewport.pan = egui::vec2(-30., 12.);
                let old = viewport.scale.unwrap_or(600. / 900.);
                let time = index as f64 + 0.1;
                for (time, pressed) in [(time, true), (time + 0.05, false)] {
                    run(
                        &mut viewport,
                        time,
                        vec![
                            egui::Event::PointerMoved(point),
                            egui::Event::PointerButton {
                                pos: point,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            },
                        ],
                    );
                }
                let next = percent / 100. / pixels_per_point;
                assert_eq!(viewport.scale, Some(next));
                assert!((viewport.pan - egui::vec2(-30., 12.) * (next / old)).length() < 0.001);
                assert_eq!(viewport.zoom_percent(pixels_per_point), Some(percent));
                assert_eq!(viewport.zoom_snap, ZoomPreset::Percent200);
                output = run(&mut viewport, time + 0.1, vec![]);
                assert!(output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == format!("Zoom {percent:.0}%"))
                }));
            }
        }
    }

    #[test]
    fn modifier_click_selects_snap_without_changing_the_view() {
        for modifiers in [
            egui::Modifiers {
                mac_cmd: true,
                command: true,
                ..Default::default()
            },
            egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            },
        ] {
            for pixels_per_point in [1., 2.] {
                for (last_canvas, scale) in [
                    (None, None),
                    (Some((egui::vec2(600., 400.), [4704, 3136])), None),
                    (Some((egui::vec2(600., 400.), [4704, 3136])), Some(0.25)),
                ] {
                    let ctx = egui::Context::default();
                    ctx.set_pixels_per_point(pixels_per_point);
                    let mut viewport = Viewport {
                        last_canvas,
                        scale,
                        pan: egui::vec2(-23.5, 12.25),
                        ..Default::default()
                    };
                    let run = |viewport: &mut Viewport, time, events| {
                        ctx.run(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(400., 240.),
                                )),
                                time: Some(time),
                                modifiers,
                                events,
                                ..Default::default()
                            },
                            |ctx| {
                                egui::CentralPanel::default().show(ctx, |ui| {
                                    viewport.navigator_zoom_controls(ui);
                                });
                            },
                        )
                    };
                    run(&mut viewport, 0., vec![]);
                    let mut output = run(&mut viewport, 0.01, vec![]);
                    for (index, preset) in ZoomPreset::ALL.into_iter().enumerate() {
                        let point = output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                egui::Shape::Text(text) if text.galley.text() == preset.label() => {
                                    Some(text.pos + text.galley.size() * 0.5)
                                }
                                _ => None,
                            })
                            .unwrap();
                        let before = (
                            viewport.scale,
                            viewport.pan,
                            viewport.one_to_one,
                            viewport.last_canvas,
                        );
                        let time = index as f64 + 0.1;
                        for (time, pressed) in [(time, true), (time + 0.05, false)] {
                            run(
                                &mut viewport,
                                time,
                                vec![
                                    egui::Event::PointerMoved(point),
                                    egui::Event::PointerButton {
                                        pos: point,
                                        button: egui::PointerButton::Primary,
                                        pressed,
                                        modifiers,
                                    },
                                ],
                            );
                        }
                        assert_eq!(viewport.zoom_snap, preset);
                        assert_eq!(
                            (
                                viewport.scale,
                                viewport.pan,
                                viewport.one_to_one,
                                viewport.last_canvas,
                            ),
                            before
                        );
                        output = run(&mut viewport, time + 0.1, vec![]);
                        let lines: Vec<_> = output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::LineSegment { points, stroke }
                                    if stroke.width == 2.
                                        && stroke.color
                                            == ctx.style().visuals.selection.stroke.color =>
                                {
                                    Some(points)
                                }
                                _ => None,
                            })
                            .collect();
                        assert_eq!(lines.len(), 1);
                        assert!(((lines[0][0].x + lines[0][1].x) * 0.5 - point.x).abs() < 0.001);
                        assert!(lines[0][0].y > point.y);
                        viewport.reset();
                        assert_eq!(viewport.zoom_snap, preset);
                        viewport.scale = before.0;
                        viewport.pan = before.1;
                        viewport.last_canvas = before.3;
                    }
                }
            }
        }
    }

    #[test]
    fn navigator_pan_tracks_visible_normalized_region() {
        let mut viewport = Viewport {
            scale: Some(4. / 3.),
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
    fn fixed_scale_resize_preserves_fractional_crop_origin_for_both_orientations() {
        for extent in [[4704, 3136], [3136, 4704]] {
            let mut viewport = Viewport {
                scale: Some(0.5),
                pan: egui::vec2(-47.25, 31.125),
                last_canvas: Some((egui::vec2(800., 600.), extent)),
                ..Default::default()
            };
            let uv = viewport.visible_uv().min;
            let pixels = viewport.visible_pixels().unwrap().top_left;
            for size in [
                egui::vec2(1100., 600.), // pane width
                egui::vec2(1100., 900.), // window height
                egui::vec2(600., 400.),
                egui::vec2(800., 600.),
            ] {
                viewport.reframe(size, extent);
                let image =
                    viewport.image_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, size), extent);
                assert_eq!(
                    image.size(),
                    egui::vec2(extent[0] as f32, extent[1] as f32) * 0.5
                );
                assert!((viewport.visible_uv().min - uv).length() < 1e-6);
                assert_eq!(viewport.visible_pixels().unwrap().top_left, pixels);
                assert_eq!(viewport.scale, Some(0.5));
            }
        }
    }

    #[test]
    fn fit_continues_to_refit_and_letterboxed_fixed_scale_axes_stay_centred() {
        let extent = [900, 600];
        let mut viewport = Viewport::default();
        for size in [egui::vec2(800., 600.), egui::vec2(400., 900.)] {
            viewport.reframe(size, extent);
            let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let image = viewport.image_rect(canvas, extent);
            assert_eq!(image.center(), canvas.center());
            assert!(image.width() <= size.x && image.height() <= size.y);
            assert_eq!(viewport.visible_pixels().unwrap().extent, extent);
            assert_eq!(viewport.scale, None);
        }
        viewport.scale = Some(1.);
        viewport.pan = egui::vec2(-100., 0.);
        let origin = viewport.visible_uv().min.x;
        viewport.reframe(egui::vec2(600., 800.), extent);
        assert_eq!(viewport.pan.y, 0.);
        assert!((viewport.visible_uv().min.x - origin).abs() < 1e-6);
        viewport.reframe(egui::vec2(1000., 800.), extent);
        assert_eq!(viewport.pan, egui::Vec2::ZERO);
        assert_eq!(viewport.visible_pixels().unwrap().extent, extent);
        // Shrinking a fully visible axis starts cropping at image pixel zero.
        viewport.reframe(egui::vec2(500., 400.), extent);
        assert_eq!(viewport.visible_uv().min, egui::Pos2::ZERO);
        assert_eq!(viewport.scale, Some(1.));
        viewport.one_to_one = true;
        viewport.reset();
        assert_eq!(viewport.scale, None);
        assert!(!viewport.one_to_one);
    }

    #[test]
    fn translating_the_canvas_does_not_change_scale_or_crop() {
        let extent = [4704, 3136];
        let mut viewport = Viewport {
            scale: Some(0.5),
            pan: egui::vec2(-47.25, 31.125),
            last_canvas: Some((egui::vec2(800., 600.), extent)),
            ..Default::default()
        };
        let size = egui::vec2(800., 600.);
        let original =
            viewport.image_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, size), extent);
        let uv = viewport.visible_uv();
        let pan = viewport.pan;
        viewport.reframe(size, extent);
        let offset = egui::vec2(125., 73.);
        let moved = viewport.image_rect(egui::Rect::from_min_size(offset.to_pos2(), size), extent);
        assert_eq!(moved, original.translate(offset));
        assert_eq!(viewport.pan, pan);
        assert_eq!(viewport.visible_uv(), uv);
    }

    #[test]
    fn pointer_zoom_and_drag_leave_fit_and_keep_their_scale_on_resize() {
        let extent = [4704, 3136];
        for wheel_zoom in [false, true] {
            let ctx = egui::Context::default();
            let mut viewport = Viewport::default();
            let mut run = |size, time, events| {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    time: Some(time),
                    events,
                    ..Default::default()
                };
                let mut bounds = (egui::Rect::NOTHING, egui::Rect::NOTHING);
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        bounds = viewport.layout(ui, extent);
                    });
                });
                (bounds, viewport.scale, viewport.visible_uv())
            };
            let size = egui::vec2(800., 600.);
            let ((canvas, original_image), scale, _) = run(size, 0., vec![]);
            assert_eq!(scale, None);
            let point = canvas.center() + egui::vec2(40., -30.);
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            let (bounds, scale, uv) = if wheel_zoom {
                let (bounds, scale, uv) = run(
                    size,
                    0.1,
                    vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0., 40.),
                            modifiers: Default::default(),
                        },
                    ],
                );
                let before = (point - original_image.min) / original_image.size();
                let after = (point - bounds.1.min) / bounds.1.size();
                assert!((before - after).length() < 1e-6);
                assert!(bounds.1.width() > original_image.width());
                (bounds, scale, uv)
            } else {
                run(
                    size,
                    0.1,
                    vec![egui::Event::PointerMoved(point), button(point, true)],
                );
                let end = point - egui::vec2(60., 40.);
                run(size, 0.2, vec![egui::Event::PointerMoved(end)]);
                let (bounds, scale, uv) = run(size, 0.3, vec![button(end, false)]);
                assert_eq!(bounds.1.size(), original_image.size());
                assert_ne!(bounds.1.min, original_image.min);
                (bounds, scale, uv)
            };
            assert!(scale.is_some());
            let (resized, resized_scale, resized_uv) = run(
                egui::vec2(500., 400.),
                0.4,
                vec![egui::Event::PointerMoved(egui::Pos2::ZERO)],
            );
            assert_eq!(resized_scale, scale);
            assert!((resized.1.size() - bounds.1.size()).length() < 0.001);
            assert!((resized_uv.min - uv.min).length() < 1e-6);
        }
    }

    #[test]
    fn native_pixel_zoom_survives_layout_resizing_at_standard_and_retina_density() {
        let extent = [4704, 3136];
        for pixels_per_point in [1., 2.] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(pixels_per_point);
            let mut viewport = Viewport {
                one_to_one: true,
                ..Default::default()
            };
            let mut original_uv: Option<egui::Pos2> = None;
            for size in [
                egui::vec2(800., 600.),
                egui::vec2(1100., 750.),
                egui::vec2(500., 400.),
            ] {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let (_, image) = viewport.layout(ui, extent);
                        assert_eq!(
                            image.size() * ctx.pixels_per_point(),
                            egui::vec2(4704., 3136.)
                        );
                    });
                });
                let uv = viewport.visible_uv().min;
                if let Some(original) = original_uv {
                    assert!((uv - original).length() < 1e-6);
                } else {
                    original_uv = Some(uv);
                }
                assert!(!viewport.one_to_one);
            }
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
                scale: Some(0.2),
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
                assert_eq!(viewport.scale, Some(0.2));
                assert_eq!(viewport.pan, egui::vec2(47., -31.));
            }
            let origin = viewport.visible_pixels().unwrap().top_left;
            let resized_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 450.),
                )),
                ..Default::default()
            };
            let resized_preview = ctx.run(resized_input.clone(), |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| viewport.show_preview(ui, &thumb, extent));
            });
            let resized_mesh = resized_preview
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == thumb.id() => Some(mesh),
                    _ => None,
                })
                .unwrap();
            let mut resized_bounds = egui::Rect::NOTHING;
            for vertex in &resized_mesh.vertices {
                resized_bounds.extend_with(vertex.pos);
            }
            assert!((resized_bounds.size() - pipeline_bounds.size()).length() < 0.001);
            assert_eq!(viewport.visible_pixels().unwrap().top_left, origin);
            let _ = ctx.run(resized_input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert_eq!(viewport.layout(ui, extent).1, resized_bounds);
                });
            });
        }
    }
}
