//! Defers tiny GPU navigator presentation behind main-image feedback and registers revision-paired egui images.

use crate::{viewport, worker::Frame};
use eframe::{egui, egui_wgpu, wgpu};
use foveonic::image::{ImageTexture, Space};
#[derive(Clone)]
struct SourceId(Arc<foveonic::source::Metadata>);
impl PartialEq for SourceId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for SourceId {}
use std::{
    path::PathBuf,
    sync::{mpsc, Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

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
}
impl Key {
    fn new(frame: &Frame, mapping: viewport::Mapping) -> Self {
        Self {
            revision: frame.revision,
            source: SourceId(frame.artifact.metadata.clone()),
            path: frame.path.clone(),
            root: frame.output.clone(),
            mapping,
            extent: frame.texture.description().extent,
            space: frame.texture.description().space,
        }
    }
    fn compatible(&self, new: &Self) -> bool {
        self.source == new.source
            && self.path == new.path
            && self.root == new.root
            && self.mapping == new.mapping
            && self.extent == new.extent
            && self.space == new.space
            && self.revision <= new.revision
    }
}
struct Job {
    key: Key,
    source: Arc<ImageTexture>,
    input: Option<Arc<ImageTexture>>,
}
#[derive(Default)]
struct Pending {
    job: Option<Job>,
    stop: bool,
}
struct Tile {
    key: Key,
    texture: wgpu::Texture,
}
/// One pending job and one small retained texture. No processing samples ever visit the CPU.
pub struct Navigator {
    rs: egui_wgpu::RenderState,
    pending: Arc<(Mutex<Pending>, Condvar)>,
    replies: mpsc::Receiver<Tile>,
    wanted: Option<(Job, Instant)>,
    sent: Option<Key>,
    displayed: Option<(Tile, egui::TextureId)>,
}
impl Navigator {
    pub fn new(rs: egui_wgpu::RenderState, context: egui::Context) -> Self {
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let slot = pending.clone();
        let gpu = rs.clone();
        let (tx, replies) = mpsc::sync_channel(1);
        std::thread::spawn(move || loop {
            let job = {
                let mut state = slot.0.lock().unwrap();
                while state.job.is_none() && !state.stop {
                    state = slot.1.wait(state).unwrap();
                }
                if state.stop {
                    break;
                }
                state.job.take().unwrap()
            };
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
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            viewport::prepare_navigator(
                &gpu,
                job.source.texture(),
                job.key.space,
                &texture,
                job.key.mapping,
                job.input.as_ref().map(|image| image.texture()),
            );
            gpu.device.poll(wgpu::Maintain::Wait);
            if std::env::var_os("FOVEONIC_UI_TIMING").is_some() {
                eprintln!(
                    "NAVIGATOR revision={} completed_ms={:.3} bytes={}",
                    job.key.revision,
                    started.elapsed().as_secs_f64() * 1000.,
                    size[0] * size[1] * 4
                );
            }
            if tx
                .send(Tile {
                    key: job.key,
                    texture,
                })
                .is_err()
            {
                break;
            }
            context.request_repaint();
        });
        Self {
            rs,
            pending,
            replies,
            wanted: None,
            sent: None,
            displayed: None,
        }
    }
    pub fn clear(&mut self) {
        self.wanted = None;
        self.sent = None;
        self.pending.0.lock().unwrap().job = None;
        self.free();
    }
    fn free(&mut self) {
        if let Some((_, id)) = self.displayed.take() {
            self.rs.renderer.write().free_texture(&id);
        }
    }
    /// Called after building the main UI; a 16 ms deferral lets that main image submit first.
    /// During processing only remember the newest navigator request, without entering the GPU queue.
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
        while let Ok(tile) = self.replies.try_recv() {
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
            self.wanted = None;
            return;
        }
        if self.sent.as_ref() == Some(&key) {
            return;
        }
        if self.wanted.as_ref().is_none_or(|(job, _)| job.key != key) {
            self.wanted = Some((
                Job {
                    key: key.clone(),
                    source: frame.texture.clone(),
                    input: if mapping.difference_gain > 0. {
                        frame.comparison.as_ref().map(|c| c.texture.clone())
                    } else {
                        None
                    },
                },
                Instant::now() + Duration::from_millis(16),
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
            self.pending.1.notify_one();
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
    let scale = (512. / extent[0].max(extent[1]) as f32).min(1.);
    extent.map(|v| (v as f32 * scale).round().max(1.) as u32)
}
#[cfg(test)]
#[path = "navigator_draw_test.rs"]
mod draw_tests;
#[cfg(test)]
#[path = "navigator_test.rs"]
mod gpu_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revision_pairing_allows_only_same_photo_root_mapping_and_geometry() {
        let a = Key {
            revision: 1,
            source: SourceId(Arc::new(foveonic::source::Metadata {
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
