//! Keeps navigator previews small and lets the main image reach the screen first.

use crate::{rendering::worker::Frame, viewport};
use eframe::{egui, egui_wgpu, wgpu};
use foveonic::diagnostics as trace;
use foveonic::image::{ImageTexture, Space};
use std::{
    path::PathBuf,
    sync::{mpsc, Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
#[derive(Clone)]
struct SourceId(Arc<foveonic::source::Metadata>);
impl PartialEq for SourceId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for SourceId {}

// Let the main image reach the screen before preparing another display texture.
const PRESENT_FIRST: Duration = Duration::from_millis(16);
const MAX_SIDE: f32 = 512.;
const TEXEL_BYTES: u32 = size_of::<[u16; 4]>() as u32;

#[derive(Clone, Copy)]
pub struct Image {
    pub id: Option<egui::TextureId>,
    pub extent: [u32; 2],
}
#[derive(Clone, PartialEq)]
struct Key {
    revision: u64,
    source: SourceId,
    path: PathBuf,
    root: String,
    mapping: viewport::Mapping,
    extent: [u32; 2],
    space: Space,
    rotation: Option<foveonic::rotation::Rotation>,
}
impl Key {
    fn new(frame: &Frame, mapping: viewport::Mapping) -> Self {
        let rotation = frame.framing_preview.then_some(frame.requested.rotation);
        let mut extent = frame.view(mapping.input).description().extent;
        if let Some(rotation) = rotation {
            extent = rotation.oriented_extent(extent);
        }
        Self {
            rotation,
            revision: frame.revision,
            source: SourceId(frame.artifact.metadata.clone()),
            path: frame.path.clone(),
            root: frame.output.clone(),
            mapping,
            extent,
            space: frame.view(mapping.input).description().space,
        }
    }
    fn compatible(&self, new: &Self) -> bool {
        self.source == new.source
            && self.path == new.path
            && self.root == new.root
            && self.mapping == new.mapping
            && self.extent == new.extent
            && self.space == new.space
            && self.rotation == new.rotation
            && self.revision <= new.revision
    }
}
#[derive(Clone)]
struct Job {
    diagnostic_task: crate::timing::DeliveredTask,
    key: Key,
    source: Arc<ImageTexture>,
    input: Option<Arc<ImageTexture>>,
}
#[derive(Default)]
struct Pending {
    job: Option<Job>,
    active: Option<Job>,
    ready: Vec<Key>,
    revision: u64,
    stop: bool,
}
struct Tile {
    key: Key,
    texture: wgpu::Texture,
    memory: Option<crate::display_image::StagedBytes>,
}
/// One pending job and one small retained texture. No processing samples ever visit the CPU.
pub struct Navigator {
    rs: egui_wgpu::RenderState,
    pending: Arc<(Mutex<Pending>, Condvar)>,
    replies: mpsc::Receiver<Tile>,
    wanted: Option<(Job, Instant)>,
    sent: Option<Key>,
    displayed: Option<(Tile, egui::TextureId)>,
    staging: crate::display_image::Staging,
    resource_revision: u64,
}
impl Navigator {
    pub fn new(rs: egui_wgpu::RenderState, context: egui::Context) -> Self {
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let slot = pending.clone();
        let gpu = rs.clone();
        let (tx, replies) = mpsc::sync_channel(1);
        let staging = crate::display_image::Staging::default();
        let worker_staging = staging.clone();
        let builder = std::thread::Builder::new().name("navigator".into());
        let spawned = builder.spawn(move || loop {
            let job = {
                let mut state = slot.0.lock().unwrap();
                while state.job.is_none() && !state.stop {
                    state = slot.1.wait(state).unwrap();
                }
                if state.stop {
                    break;
                }
                let job = state.job.take().unwrap();
                state.active = Some(job.clone());
                state.revision = state.revision.wrapping_add(1);
                job
            };
            let _context = trace::enabled().then(|| {
                trace::enter(crate::timing::context_for(
                    job.diagnostic_task.id,
                    job.diagnostic_task.revision,
                    &job.key.path,
                    "navigator",
                ))
            });
            let _job =
                trace::scope_with(trace::Category::Presentation, "navigator.job", |fields| {
                    fields
                        .str("source", &job.key.path.to_string_lossy())
                        .str("output", &job.key.root)
                        .u64("presentation_revision", job.key.revision)
                        .str("image_purpose", job.diagnostic_task.purpose);
                });
            let started = Instant::now();
            let size = tile_extent(job.key.extent);
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Small display-ready navigator"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let memory = worker_staging.track(crate::memory::texture_bytes(&texture));
            viewport::prepare_preview(
                &gpu,
                job.source.texture(),
                job.key.space,
                &texture,
                job.key.mapping,
                job.input.as_ref().map(|image| image.texture()),
                job.key
                    .rotation
                    .map(|rotation| crate::framing::Presentation {
                        rotation,
                        native_extent: job.source.description().extent,
                    }),
            );
            // Presentation uses the same ordered queue. Its later draw observes
            // this texture after the submitted preparation without a CPU wait.
            if crate::timing::enabled() {
                crate::timing::emit(
                    "navigator",
                    || serde_json::json!({"revision":job.key.revision,"submitted_ms":started.elapsed().as_secs_f64()*1000.,"bytes":size[0]*size[1]*TEXEL_BYTES}),
                );
            }
            let ready_key = job.key.clone();
            {
                let mut state = slot.0.lock().unwrap();
                state.ready.push(ready_key.clone());
                state.revision = state.revision.wrapping_add(1);
            }
            let sent = {
                let _send = trace::scope_with(
                    trace::Category::Coordinator,
                    "navigator.reply_send",
                    |fields| {
                        fields.str("measurement", "host_wait").u64("capacity", 1);
                    },
                );
                tx.send(Tile {
                    key: job.key,
                    texture,
                    memory,
                })
            };
            {
                let mut state = slot.0.lock().unwrap();
                state.active = None;
                if sent.is_err() {
                    state.ready.retain(|key| key != &ready_key);
                }
                state.revision = state.revision.wrapping_add(1);
            }
            if sent.is_err() {
                break;
            }
            context.request_repaint();
        });
        spawned.expect("start navigator thread");
        Self {
            rs,
            pending,
            replies,
            wanted: None,
            sent: None,
            displayed: None,
            staging,
            resource_revision: 0,
        }
    }
    pub fn clear(&mut self) {
        if self.wanted.is_some() || self.sent.is_some() || self.displayed.is_some() {
            self.resource_revision = self.resource_revision.wrapping_add(1);
        }
        self.wanted = None;
        self.sent = None;
        {
            let mut state = self.pending.0.lock().unwrap();
            if state.job.take().is_some() {
                state.revision = state.revision.wrapping_add(1);
            }
        }
        self.free();
    }
    fn free(&mut self) {
        if let Some((_, id)) = self.displayed.take() {
            self.rs.renderer.write().free_texture(&id);
        }
    }
    /// Wait 16 ms after building the main UI so its image can be submitted first.
    /// While processing is busy, keep only the newest request. Queuing navigator
    /// work now would delay feedback from the main image.
    pub fn update(
        &mut self,
        frame: Option<&Frame>,
        mapping: viewport::Mapping,
        busy: bool,
        ctx: &egui::Context,
    ) {
        let Some(frame) = frame else {
            self.clear();
            return;
        };
        let key = Key::new(frame, mapping);
        while let Ok(mut tile) = self.replies.try_recv() {
            drop(tile.memory.take());
            {
                let mut state = self.pending.0.lock().unwrap();
                state.ready.retain(|key| key != &tile.key);
                state.revision = state.revision.wrapping_add(1);
            }
            self.resource_revision = self.resource_revision.wrapping_add(1);
            if self.sent.as_ref() == Some(&tile.key) {
                self.sent = None;
            }
            if tile.key.compatible(&key)
                && self
                    .displayed
                    .as_ref()
                    .is_none_or(|(old, _)| old.key.revision <= tile.key.revision)
            {
                self.free();
                let id = self.rs.renderer.write().register_native_texture(
                    &self.rs.device,
                    &tile.texture.create_view(&Default::default()),
                    wgpu::FilterMode::Linear,
                );
                self.displayed = Some((tile, id));
            }
        }
        if self
            .displayed
            .as_ref()
            .is_some_and(|(tile, _)| tile.key == key)
        {
            if self.wanted.is_some() {
                self.resource_revision = self.resource_revision.wrapping_add(1);
            }
            self.wanted = None;
            return;
        }
        if self.sent.as_ref() == Some(&key) {
            return;
        }
        if self.wanted.as_ref().is_none_or(|(job, _)| job.key != key) {
            self.resource_revision = self.resource_revision.wrapping_add(1);
            self.wanted = Some((
                Job {
                    diagnostic_task: frame.diagnostic_task,
                    key: key.clone(),
                    source: frame.view(mapping.input).clone(),
                    input: if mapping.difference_gain > 0. {
                        frame.comparison.as_ref().map(|c| c.texture.clone())
                    } else {
                        None
                    },
                },
                Instant::now() + PRESENT_FIRST,
            ));
        }
        if busy {
            return;
        }
        if let Some((_, ready)) = &self.wanted {
            if *ready > Instant::now() {
                ctx.request_repaint_after(ready.saturating_duration_since(Instant::now()));
                return;
            }
        }
        if self.sent.as_ref() != Some(&key) {
            let (job, _) = self.wanted.take().unwrap();
            self.sent = Some(job.key.clone());
            let mut state = self.pending.0.lock().unwrap();
            state.job = Some(job);
            state.revision = state.revision.wrapping_add(1);
            self.pending.1.notify_one();
        }
    }
    pub fn resource_revision(&self) -> u64 {
        self.resource_revision
            .wrapping_add(self.pending.0.lock().unwrap().revision)
            .wrapping_add(self.staging.revision())
    }
    pub fn memory_resources(&self, inventory: &mut crate::memory::Inventory) {
        let account = |job: &Job, inventory: &mut crate::memory::Inventory| {
            inventory.image_texture(&job.source);
            if let Some(input) = &job.input {
                inventory.image_texture(input);
            }
            inventory.metadata(&job.key.source.0);
        };
        if let Some((job, _)) = &self.wanted {
            account(job, inventory);
        }
        if let Some(key) = &self.sent {
            inventory.metadata(&key.source.0);
        }
        let state = self.pending.0.lock().unwrap();
        if let Some(job) = &state.job {
            account(job, inventory);
        }
        if let Some(job) = &state.active {
            account(job, inventory);
        }
        for key in &state.ready {
            inventory.metadata(&key.source.0);
        }
        // Unregistered in-flight and completed navigator textures form one display pool.
        inventory.cpu(self.staging.bytes());
        if let Some((tile, id)) = &self.displayed {
            inventory.metadata(&tile.key.source.0);
            inventory.texture(*id, crate::memory::texture_bytes(&tile.texture));
        }
    }
    pub fn image(&self, frame: &Frame, mapping: viewport::Mapping) -> Image {
        let key = Key::new(frame, mapping);
        Image {
            extent: key.extent,
            id: self
                .displayed
                .as_ref()
                .filter(|(tile, _)| tile.key.compatible(&key))
                .map(|(_, id)| *id),
        }
    }
    pub fn ready(&self, frame: &Frame, mapping: viewport::Mapping) -> bool {
        self.displayed
            .as_ref()
            .is_some_and(|(tile, _)| tile.key == Key::new(frame, mapping))
    }
}
impl Drop for Navigator {
    fn drop(&mut self) {
        self.clear();
        self.pending.0.lock().unwrap().stop = true;
        self.pending.1.notify_one();
    }
}
fn tile_extent(extent: [u32; 2]) -> [u32; 2] {
    let scale = (MAX_SIDE / extent[0].max(extent[1]) as f32).min(1.);
    extent.map(|v| (v as f32 * scale).round().max(1.) as u32)
}
#[cfg(test)]
#[path = "navigator_draw_test.rs"]
mod draw_tests;
#[cfg(test)]
#[path = "navigator_test.rs"]
pub(crate) mod gpu_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revision_pairing_allows_only_same_photo_root_mapping_and_geometry() {
        let a = Key {
            revision: 1,
            rotation: None,
            source: SourceId(Arc::new(foveonic::source::Metadata {
                processing_version: None,
                width: 3136,
                height: 4704,
                rotation: 0,
                properties: Default::default(),
                lists: Default::default(),
                matrices: Default::default(),
            })),
            path: "one.x3f".into(),
            root: "final".into(),
            mapping: viewport::Mapping::normal(-1),
            extent: [3136, 4704],
            space: Space::ToneMappedRgb(foveonic::image::ColorSpace::Srgb),
        };
        let mut b = a.clone();
        b.revision = 2;
        assert!(a.compatible(&b));
        assert!(!b.compatible(&a));
        b.mapping.channel = 0;
        assert!(!a.compatible(&b));
        b = a.clone();
        b.mapping.difference_gain = 8.;
        assert!(!a.compatible(&b));
        b = a.clone();
        b.mapping.input = true;
        assert!(!a.compatible(&b));
        b = a.clone();
        b.root = "sensor.layers".into();
        assert!(!a.compatible(&b));
        b = a.clone();
        b.path = "two.x3f".into();
        assert!(!a.compatible(&b));
        b = a.clone();
        b.extent.reverse();
        assert!(!a.compatible(&b));
    }
    #[test]
    fn tiles_are_bounded_and_preserve_both_orientations() {
        assert_eq!(tile_extent([4704, 3136]), [512, 341]);
        assert_eq!(tile_extent([3136, 4704]), [341, 512]);
        assert_eq!(tile_extent([200, 100]), [200, 100]);
    }
}
