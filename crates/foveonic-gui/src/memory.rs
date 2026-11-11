//! Counts shared photographic samples and distinct frontend display pools for the worker budget.

use crate::rendering::worker;
use eframe::egui;
use foveonic::{image::ImageTexture, inspection::Content, render::MemoryReservation};
use std::sync::{Arc, Weak};

#[derive(Clone, Default)]
pub struct Inventory {
    reservation: MemoryReservation,
    image_textures: Vec<Arc<ImageTexture>>,
    textures: std::collections::HashMap<egui::TextureId, u64>,
}
impl Inventory {
    pub fn cpu(&mut self, bytes: u64) {
        self.reservation.exclusive_bytes = self.reservation.exclusive_bytes.saturating_add(bytes);
    }
    pub fn texture(&mut self, id: egui::TextureId, bytes: u64) {
        let old = self.textures.insert(id, bytes).unwrap_or(0);
        self.reservation.exclusive_bytes = self
            .reservation
            .exclusive_bytes
            .saturating_sub(old)
            .saturating_add(bytes);
    }
    pub fn image_texture(&mut self, image: &Arc<ImageTexture>) {
        if !self
            .image_textures
            .iter()
            .any(|prior| Arc::ptr_eq(prior, image))
        {
            self.reservation.exclusive_bytes = self
                .reservation
                .exclusive_bytes
                .saturating_add(texture_bytes(image.texture()));
            self.image_textures.push(image.clone());
        }
    }
    pub fn metadata(&mut self, metadata: &Arc<foveonic::source::Metadata>) {
        self.reservation.add_metadata(metadata);
    }
    pub fn image(&mut self, image: &foveonic::image::RenderedImage) {
        self.reservation.add_image(image);
    }
    pub fn frame(&mut self, frame: &worker::Frame) {
        self.metadata(&frame.artifact.metadata);
        if let Content::Image(image) = &frame.artifact.content {
            self.reservation.add_image(image);
        }
        self.image_texture(&frame.texture);
        if let Some(comparison) = &frame.comparison {
            self.image_texture(&comparison.texture);
        }
    }
    #[cfg(test)]
    pub fn finish(self) -> MemoryReservation {
        self.reservation
    }
    pub fn reservation(&self) -> &MemoryReservation {
        &self.reservation
    }
    pub fn contains_image_texture(&self, texture: &Arc<ImageTexture>) -> bool {
        self.image_textures
            .iter()
            .any(|prior| Arc::ptr_eq(prior, texture))
    }
}

/// Track a queued image until the frontend either inventories or drops it.
pub struct PendingImage {
    shared: MemoryReservation,
    texture: Weak<ImageTexture>,
    comparison: Option<Weak<ImageTexture>>,
}
impl PendingImage {
    pub fn new(frame: &worker::Frame) -> Self {
        let mut shared = MemoryReservation::default();
        shared.add_metadata(&frame.artifact.metadata);
        if let Content::Image(image) = &frame.artifact.content {
            shared.add_image(image);
        }
        Self {
            shared,
            texture: Arc::downgrade(&frame.texture),
            comparison: frame
                .comparison
                .as_ref()
                .map(|comparison| Arc::downgrade(&comparison.texture)),
        }
    }
    pub fn reserve(&self, frontend: &Inventory, combined: &mut Inventory) -> bool {
        let Some(texture) = self.texture.upgrade() else {
            return false;
        };
        if frontend.contains_image_texture(&texture) {
            return false;
        }
        combined.reservation.extend_shared(&self.shared);
        combined.image_texture(&texture);
        if let Some(texture) = self.comparison.as_ref().and_then(Weak::upgrade) {
            combined.image_texture(&texture);
        }
        true
    }
}

pub fn texture_bytes(texture: &eframe::wgpu::Texture) -> u64 {
    let (block_width, block_height) = texture.format().block_dimensions();
    let block_bytes = u64::from(texture.format().block_copy_size(None).unwrap_or(0));
    (0..texture.mip_level_count())
        .map(|level| {
            let width = (texture.width() >> level).max(1).div_ceil(block_width);
            let height = (texture.height() >> level).max(1).div_ceil(block_height);
            let depth = if texture.dimension() == eframe::wgpu::TextureDimension::D3 {
                (texture.depth_or_array_layers() >> level).max(1)
            } else {
                texture.depth_or_array_layers()
            };
            u64::from(width)
                * u64::from(height)
                * u64::from(depth)
                * block_bytes
                * u64::from(texture.sample_count())
        })
        .sum()
}

pub fn frame_key(frame: &worker::Frame, key: &mut Vec<u64>) {
    key.extend([
        frame.revision,
        Arc::as_ptr(&frame.texture) as usize as u64,
        Arc::as_ptr(&frame.artifact.metadata) as usize as u64,
        frame.comparison.as_ref().map_or(0, |comparison| {
            Arc::as_ptr(&comparison.texture) as usize as u64
        }),
    ]);
}

impl crate::App {
    pub(crate) fn update_memory(&mut self) {
        let mut key = vec![
            self.preferences.memory_cache_bytes,
            self.memory_reply_epoch,
            self.browser.resource_revision(),
            self.navigator.resource_revision(),
            self.previews.resource_revision(),
        ];
        for frame in [self.frame.as_ref(), self.detail.as_ref()] {
            key.push(u64::from(frame.is_some()));
            if let Some(frame) = frame {
                frame_key(frame, &mut key);
            }
        }
        if let Some(metadata) = &self.metadata {
            key.push(Arc::as_ptr(metadata) as usize as u64);
        }
        self.before.memory_key(&mut key);
        self.multi_edit.memory_key(&mut key);
        if let Some(images) = &self.scope_images {
            images.memory_key(&mut key);
        }
        key.push(
            self.scope_snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.data.counts.capacity() as u64),
        );
        if self.memory_key == key {
            return;
        }
        self.memory_key = key;
        let mut inventory = Inventory::default();
        for frame in self.frame.iter().chain(self.detail.iter()) {
            inventory.frame(frame);
        }
        if let Some(metadata) = &self.metadata {
            inventory.metadata(metadata);
        }
        self.before.memory_resources(&mut inventory);
        self.multi_edit.memory_resources(&mut inventory);
        self.browser.memory_resources(&mut inventory);
        self.navigator.memory_resources(&mut inventory);
        self.previews.memory_resources(&mut inventory);
        if let Some(images) = &self.scope_images {
            images.memory_resources(&mut inventory);
        }
        if let Some(snapshot) = &self.scope_snapshot {
            inventory.cpu((snapshot.data.counts.capacity() * size_of::<u32>()) as u64);
        }
        self.worker
            .set_memory_budget(self.preferences.memory_cache_bytes, inventory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_browser_and_navigator_texture_ids_are_reserved_once() {
        let mut inventory = Inventory::default();
        inventory.texture(egui::TextureId::User(7), 1024);
        inventory.texture(egui::TextureId::User(7), 1024);
        inventory.texture(egui::TextureId::User(8), 2048);
        inventory.cpu(512);
        assert_eq!(inventory.finish().exclusive_bytes, 3584);
    }
}
