//! Browses X3F paths and lazily decodes embedded JPEGs through a bounded thumbnail worker.

use eframe::egui;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

struct DecodedThumbnail {
    image: egui::ColorImage,
    native_extent: [u32; 2],
}
type Thumb = Result<DecodedThumbnail, String>;

#[derive(Clone)]
pub struct Thumbnail {
    pub texture: egui::TextureHandle,
    /// Oriented full-size presentation dimensions, independent of the thumbnail's resolution.
    pub native_extent: [u32; 2],
}
pub struct Browser {
    pub files: Vec<PathBuf>,
    sender: mpsc::SyncSender<PathBuf>,
    receiver: mpsc::Receiver<(PathBuf, Thumb)>,
    pending: HashSet<PathBuf>,
    cache: HashMap<PathBuf, Option<Thumbnail>>,
    order: VecDeque<PathBuf>,
}
impl Browser {
    pub fn new(context: egui::Context) -> Self {
        let (sender, requests) = mpsc::sync_channel::<PathBuf>(32);
        let (ready, receiver) = mpsc::sync_channel(32);
        thread::spawn(move || {
            while let Ok(path) = requests.recv() {
                let thumb = (|| -> foveonic::source::Result<DecodedThumbnail> {
                    let preview = foveonic::preview::read(&path)?;
                    let image = image::load_from_memory_with_format(
                        &preview.jpeg,
                        image::ImageFormat::Jpeg,
                    )?;
                    let image = match preview.rotation {
                        90 => image.rotate90(),
                        180 => image.rotate180(),
                        270 => image.rotate270(),
                        _ => image,
                    };
                    let rgba = image.thumbnail(256, 256).to_rgba8();
                    let mut native_extent = preview.native_extent;
                    if matches!(preview.rotation, 90 | 270) {
                        native_extent.swap(0, 1);
                    }
                    Ok(DecodedThumbnail {
                        image: egui::ColorImage::from_rgba_unmultiplied(
                            [rgba.width() as usize, rgba.height() as usize],
                            &rgba,
                        ),
                        native_extent,
                    })
                })()
                .map_err(|e| e.to_string());
                if ready.send((path, thumb)).is_err() {
                    break;
                }
                context.request_repaint();
            }
        });
        Self {
            files: Vec::new(),
            sender,
            receiver,
            pending: HashSet::new(),
            cache: HashMap::new(),
            order: VecDeque::new(),
        }
    }
    pub fn open(&mut self, path: &Path) -> std::io::Result<()> {
        let dir = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(Path::new("."))
        };
        let mut files = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && p.extension().is_some_and(|s| s.eq_ignore_ascii_case("x3f")))
            .collect::<Vec<_>>();
        files.sort();
        self.files = files;
        self.cache.clear();
        self.order.clear();
        Ok(())
    }
    pub fn receive(&mut self, ctx: &egui::Context) {
        while let Ok((path, image)) = self.receiver.try_recv() {
            self.pending.remove(&path);
            if !self.files.contains(&path) {
                continue;
            }
            let image = image.ok().map(|i| Thumbnail {
                texture: ctx.load_texture(
                    path.to_string_lossy(),
                    i.image,
                    egui::TextureOptions::LINEAR,
                ),
                native_extent: i.native_extent,
            });
            self.cache.insert(path.clone(), image);
            self.order.push_back(path);
            while self.order.len() > 256 {
                if let Some(old) = self.order.pop_front() {
                    self.cache.remove(&old);
                }
            }
        }
    }
    pub fn thumbnail(&mut self, path: &Path) -> Option<Thumbnail> {
        if let Some(texture) = self.cache.get(path) {
            return texture.clone();
        }
        if !self.pending.contains(path) && self.sender.try_send(path.to_owned()).is_ok() {
            self.pending.insert(path.to_owned());
        }
        None
    }
    pub fn tile(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        selected: bool,
        size: egui::Vec2,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        ui.painter().rect_filled(
            rect,
            4.,
            if selected {
                egui::Color32::from_rgb(40, 65, 95)
            } else {
                egui::Color32::from_gray(24)
            },
        );
        if let Some(image) = ui
            .is_rect_visible(rect)
            .then(|| self.thumbnail(path))
            .flatten()
        {
            let intrinsic = image.texture.size_vec2();
            let max = rect.size() - egui::vec2(12., 28.);
            let s = (max.x / intrinsic.x).min(max.y / intrinsic.y);
            let image_rect =
                egui::Rect::from_center_size(rect.center() - egui::vec2(0., 10.), intrinsic * s);
            ui.painter().image(
                image.texture.id(),
                image_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        ui.painter().with_clip_rect(rect.shrink(4.)).text(
            rect.center_bottom() - egui::vec2(0., 13.),
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::proportional(12.),
            egui::Color32::LIGHT_GRAY,
        );
        response.on_hover_text(path.display().to_string())
    }
    pub fn organizer(
        &mut self,
        ui: &mut egui::Ui,
        selected: Option<&Path>,
        mode: crate::preferences::Organizer,
    ) -> Option<PathBuf> {
        let mut chosen = None;
        let width = ui.available_width();
        let columns = icon_columns(width);
        let icons = mode == crate::preferences::Organizer::Icons;
        let rows = if icons {
            self.files.len().div_ceil(columns)
        } else {
            self.files.len()
        };
        let height = if icons { 154. } else { 44. };
        egui::ScrollArea::vertical()
            .id_salt("organizer items")
            .show_rows(ui, height, rows, |ui, range| {
                for row in range {
                    if icons {
                        let tile_width = (width
                            - (columns - 1) as f32 * ui.spacing().item_spacing.x)
                            / columns as f32;
                        ui.horizontal(|ui| {
                            for index in row * columns..((row + 1) * columns).min(self.files.len())
                            {
                                let path = self.files[index].clone();
                                if self
                                    .tile(
                                        ui,
                                        &path,
                                        selected == Some(path.as_path()),
                                        egui::vec2(tile_width, 146.),
                                    )
                                    .clicked()
                                {
                                    chosen = Some(path);
                                }
                            }
                        });
                    } else {
                        let path = self.files[row].clone();
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(width, 40.), egui::Sense::click());
                        ui.painter().rect_filled(
                            rect,
                            3.,
                            if selected == Some(path.as_path()) {
                                egui::Color32::from_rgb(35, 57, 83)
                            } else {
                                egui::Color32::from_gray(20)
                            },
                        );
                        if ui.is_rect_visible(rect) {
                            if let Some(thumb) = self.thumbnail(&path) {
                                let size = thumb.texture.size_vec2();
                                let factor = (32. / size.x).min(32. / size.y);
                                ui.painter().image(
                                    thumb.texture.id(),
                                    egui::Rect::from_center_size(
                                        egui::pos2(rect.left() + 21., rect.center().y),
                                        size * factor,
                                    ),
                                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                                    egui::Color32::WHITE,
                                );
                            }
                            let text = egui::Rect::from_min_max(
                                egui::pos2(rect.left() + 45., rect.top()),
                                rect.max,
                            );
                            ui.painter().with_clip_rect(text).text(
                                egui::pos2(text.left(), text.center().y),
                                egui::Align2::LEFT_CENTER,
                                path.file_name().unwrap_or_default().to_string_lossy(),
                                egui::FontId::proportional(12.),
                                egui::Color32::LIGHT_GRAY,
                            );
                        }
                        if response.on_hover_text(path.display().to_string()).clicked() {
                            chosen = Some(path);
                        }
                    }
                }
            });
        chosen
    }
}
fn icon_columns(width: f32) -> usize {
    ((width + 8.) / 158.).floor().max(1.) as usize
}
#[cfg(test)]
mod tests {
    #[test]
    fn pane_width_reflows_icon_columns() {
        for (width, count) in [(150., 1), (280., 1), (330., 2), (500., 3)] {
            assert_eq!(super::icon_columns(width), count);
        }
    }
}
