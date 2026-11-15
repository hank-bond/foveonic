//! Uploads bounded linear-light photographic previews without clipping them to sRGB.

use eframe::{egui, egui_wgpu, wgpu};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

#[cfg(test)]
#[path = "display_image_test.rs"]
mod tests;

pub(crate) struct Pixels {
    pub size: [usize; 2],
    pub rgba: Vec<[u16; 4]>,
}
impl Pixels {
    pub(crate) fn retained_bytes(&self) -> u64 {
        (self.rgba.capacity() * size_of::<[u16; 4]>()) as u64
    }
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
        Texture::from_native(rs, texture)
    }

    /// Headless browser tests exercise reply ordering without a graphics device.
    #[cfg(test)]
    pub fn headless(self, context: &egui::Context) -> Texture {
        let mut image = egui::ColorImage::filled(self.size, egui::Color32::BLACK);
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

/// Counts pixel payloads owned by bounded workers and their reply queues.
#[derive(Clone, Default)]
pub(crate) struct Staging(Arc<StagingState>);
#[derive(Default)]
struct StagingState {
    bytes: AtomicU64,
    revision: AtomicU64,
}
pub(crate) struct StagedBytes {
    state: Arc<StagingState>,
    bytes: u64,
}
impl Staging {
    pub(crate) fn track(&self, bytes: u64) -> Option<StagedBytes> {
        if bytes == 0 {
            return None;
        }
        self.0.bytes.fetch_add(bytes, Ordering::AcqRel);
        self.0.revision.fetch_add(1, Ordering::AcqRel);
        Some(StagedBytes {
            state: self.0.clone(),
            bytes,
        })
    }
    pub(crate) fn bytes(&self) -> u64 {
        self.0.bytes.load(Ordering::Acquire)
    }
    pub(crate) fn revision(&self) -> u64 {
        self.0.revision.load(Ordering::Acquire)
    }
}
impl Drop for StagedBytes {
    fn drop(&mut self) {
        self.state.bytes.fetch_sub(self.bytes, Ordering::AcqRel);
        self.state.revision.fetch_add(1, Ordering::AcqRel);
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
    pub(crate) fn from_native(rs: &egui_wgpu::RenderState, texture: wgpu::Texture) -> Self {
        let size = [texture.width() as usize, texture.height() as usize];
        let id = rs.renderer.write().register_native_texture(
            &rs.device,
            &texture.create_view(&Default::default()),
            wgpu::FilterMode::Linear,
        );
        Self(Arc::new(Storage::Native {
            id,
            size,
            rs: rs.clone(),
            _texture: texture,
        }))
    }
    pub(crate) fn quarter_turn(&self, clockwise: bool) -> Option<Self> {
        let (rs, source) = match &*self.0 {
            Storage::Native { rs, _texture, .. } => (rs, _texture),
            #[cfg(test)]
            Storage::Headless(_) => return None,
        };
        let target = rs.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Rotated browser preview"),
            size: wgpu::Extent3d {
                width: source.height(),
                height: source.width(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        crate::viewport::rotate_linear_preview(
            rs,
            source,
            &target,
            crate::framing::Presentation {
                rotation: foveonic::rotation::Rotation {
                    turns: if clockwise { 1 } else { 3 },
                    degrees: 0.,
                },
                native_extent: [source.width(), source.height()],
            },
        );
        Some(Self::from_native(rs, target))
    }
    pub(crate) fn memory_resources(&self, inventory: &mut crate::memory::Inventory) {
        let bytes = match &*self.0 {
            Storage::Native { _texture, .. } => crate::memory::texture_bytes(_texture),
            #[cfg(test)]
            Storage::Headless(texture) => {
                texture
                    .size()
                    .into_iter()
                    .map(|v| v as u64)
                    .product::<u64>()
                    * 4
            }
        };
        inventory.texture(self.id(), bytes);
    }
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

#[cfg(test)]
mod memory_tests {
    use super::*;
    #[test]
    fn staging_counts_live_payload_leases_and_releases_dropped_replies() {
        let staging = Staging::default();
        let observer = staging.clone();
        let first = staging.track(1024);
        let second = staging.track(2048);
        assert_eq!(observer.bytes(), 3072);
        let revision = observer.revision();
        drop(first);
        assert_eq!(observer.bytes(), 2048);
        assert!(observer.revision() > revision);
        drop(second);
        assert_eq!(observer.bytes(), 0);
        assert!(staging.track(0).is_none());
    }
    #[test]
    fn shared_preview_texture_is_inventoried_once() {
        let ctx = egui::Context::default();
        let texture = Pixels {
            size: [20, 10],
            rgba: vec![[0; 4]; 200],
        }
        .headless(&ctx);
        let alias = texture.clone();
        let mut inventory = crate::memory::Inventory::default();
        texture.memory_resources(&mut inventory);
        alias.memory_resources(&mut inventory);
        assert_eq!(inventory.finish().exclusive_bytes, 20 * 10 * 4);
    }
}
