//! Uploads bounded linear-light photographic previews without clipping them to sRGB.

use eframe::{egui, egui_wgpu, wgpu};
use std::sync::Arc;

#[cfg(test)]
#[path = "display_image_test.rs"]
mod tests;

pub(crate) struct Pixels {
    pub size: [usize; 2],
    pub rgba: Vec<[u16; 4]>,
}
impl Pixels {
    pub fn from_linear(image: &image::Rgb32FImage, space: foveonic::image::ColorSpace) -> Self {
        let matrix = foveonic::color_space::TiffColorSpace::Srgb.transform(space);
        Self {
            size: [image.width() as usize, image.height() as usize],
            rgba: image
                .as_raw()
                .as_chunks::<3>()
                .0
                .iter()
                .map(|pixel| {
                    std::array::from_fn(|channel| {
                        let value = if channel == 3 {
                            1.
                        } else {
                            (0..3).map(|c| matrix[channel][c] * pixel[c]).sum()
                        };
                        half::f16::from_f32(value).to_bits()
                    })
                })
                .collect(),
        }
    }

    pub fn upload(self, rs: &egui_wgpu::RenderState) -> Texture {
        let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Linear wide-gamut photo preview"),
            size: wgpu::Extent3d {
                width: self.size[0] as u32,
                height: self.size[1] as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        rs.queue.write_texture(
            texture.as_image_copy(),
            bytemuck::cast_slice(&self.rgba),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.size[0] as u32 * size_of::<[u16; 4]>() as u32),
                rows_per_image: None,
            },
            texture.size(),
        );
        let id = rs.renderer.write().register_native_texture(
            &rs.device,
            &texture.create_view(&Default::default()),
            wgpu::FilterMode::Linear,
        );
        Texture(Arc::new(Storage::Native {
            id,
            size: self.size,
            rs: rs.clone(),
            _texture: texture,
        }))
    }

    /// Headless browser tests exercise reply ordering without a graphics device.
    #[cfg(test)]
    pub fn headless(self, context: &egui::Context) -> Texture {
        let mut image = egui::ColorImage::new(self.size, egui::Color32::BLACK);
        for (out, pixel) in image.pixels.iter_mut().zip(self.rgba) {
            let rgb: [u8; 3] = std::array::from_fn(|c| {
                (foveonic::preview::encode_sample(
                    foveonic::color_space::TiffColorSpace::Srgb,
                    half::f16::from_bits(pixel[c]).to_f32(),
                ) / 257) as u8
            });
            *out = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        }
        Texture(Arc::new(Storage::Headless(context.load_texture(
            "headless photo",
            image,
            egui::TextureOptions::LINEAR,
        ))))
    }
}

enum Storage {
    Native {
        id: egui::TextureId,
        size: [usize; 2],
        rs: egui_wgpu::RenderState,
        _texture: wgpu::Texture,
    },
    #[cfg(test)]
    Headless(egui::TextureHandle),
}
impl Drop for Storage {
    fn drop(&mut self) {
        match self {
            Self::Native { id, rs, .. } => rs.renderer.write().free_texture(id),
            #[cfg(test)]
            Self::Headless(_) => {}
        }
    }
}

#[derive(Clone)]
pub(crate) struct Texture(Arc<Storage>);
impl Texture {
    pub fn id(&self) -> egui::TextureId {
        match &*self.0 {
            Storage::Native { id, .. } => *id,
            #[cfg(test)]
            Storage::Headless(texture) => texture.id(),
        }
    }
    pub fn size(&self) -> [usize; 2] {
        match &*self.0 {
            Storage::Native { size, .. } => *size,
            #[cfg(test)]
            Storage::Headless(texture) => texture.size(),
        }
    }
}
impl From<&Texture> for egui::load::SizedTexture {
    fn from(texture: &Texture) -> Self {
        Self::new(texture.id(), texture.size().map(|v| v as f32))
    }
}

#[cfg(test)]
impl From<egui::TextureHandle> for Texture {
    fn from(texture: egui::TextureHandle) -> Self {
        Self(Arc::new(Storage::Headless(texture)))
    }
}
