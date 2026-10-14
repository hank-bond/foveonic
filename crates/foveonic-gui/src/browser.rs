//! Browses X3F paths with focused keyboard selection and bounded lazy embedded-JPEG thumbnails.

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
        let info = std::fs::metadata(path)?;
        let dir = if info.is_dir() {
            path
        } else if info.is_file() {
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "not a file or folder",
            ));
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
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::CLICK);
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
        // Rows come and go as the list scrolls. A stable organizer-level focus
        // target keeps keyboard selection working when the focused row disappears.
        let focus = ui.interact(
            ui.available_rect_before_wrap(),
            ui.make_persistent_id("organizer focus"),
            egui::Sense::focusable_noninteractive(),
        );
        if focus.clicked_elsewhere() || !ui.is_enabled() {
            focus.surrender_focus();
        } else if ui.input(|input| input.pointer.primary_pressed())
            && ui.rect_contains_pointer(focus.rect)
            && !focus.has_focus()
        {
            // Waiting until click release would miss the first arrow key afterward.
            // Give egui time to install the key filter by focusing on press.
            focus.request_focus();
        }
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                focus.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                    ..Default::default()
                },
            );
        });
        let mut chosen = None;
        let width = ui.available_width();
        let columns = icon_columns(width);
        let icons = mode == crate::preferences::Organizer::Icons;
        let rows = if icons {
            self.files.len().div_ceil(columns)
        } else {
            self.files.len()
        };
        let height = if icons { 146. } else { 40. };
        let mut active = selected.and_then(|path| self.files.iter().position(|p| p == path));
        if ui.is_enabled() && focus.has_focus() {
            for key in [
                egui::Key::ArrowLeft,
                egui::Key::ArrowRight,
                egui::Key::ArrowUp,
                egui::Key::ArrowDown,
            ] {
                if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key)) {
                    let next = arrow_selection(
                        active,
                        self.files.len(),
                        if icons { columns } else { 1 },
                        key,
                    );
                    if next != active {
                        active = next;
                        chosen = next.map(|index| self.files[index].clone());
                    }
                }
            }
        }
        let mut scroll = egui::ScrollArea::vertical().id_salt("organizer items");
        if chosen.is_some() {
            let row = active.unwrap() / if icons { columns } else { 1 };
            let stride = height + ui.spacing().item_spacing.y;
            let top = row as f32 * stride;
            let bottom = top + height;
            let id = ui.make_persistent_id(egui::Id::new("organizer items"));
            let offset = egui::scroll_area::State::load(ui.ctx(), id)
                .unwrap_or_default()
                .offset
                .y;
            let viewport = ui.available_height();
            let offset = if top < offset {
                top
            } else if bottom > offset + viewport {
                (bottom - viewport).max(0.)
            } else {
                offset
            };
            scroll = scroll.vertical_scroll_offset(offset);
        }
        let highlighted = chosen.as_deref().or(selected).map(Path::to_owned);
        scroll.show_rows(ui, height, rows, |ui, range| {
            for row in range {
                if icons {
                    let tile_width = (width - (columns - 1) as f32 * ui.spacing().item_spacing.x)
                        / columns as f32;
                    ui.horizontal(|ui| {
                        for index in row * columns..((row + 1) * columns).min(self.files.len()) {
                            let path = self.files[index].clone();
                            if self
                                .tile(
                                    ui,
                                    &path,
                                    highlighted.as_deref() == Some(path.as_path()),
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
                        ui.allocate_exact_size(egui::vec2(width, 40.), egui::Sense::CLICK);
                    ui.painter().rect_filled(
                        rect,
                        3.,
                        if highlighted.as_deref() == Some(path.as_path()) {
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
        if chosen.is_some() && !focus.has_focus() {
            focus.request_focus();
            // Install the arrow filter on the next pass, before subsequent keyboard input.
            ui.ctx().request_repaint();
        }
        chosen
    }
}
fn arrow_selection(
    current: Option<usize>,
    count: usize,
    columns: usize,
    key: egui::Key,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let Some(index) = current else {
        return Some(0);
    };
    Some(match key {
        egui::Key::ArrowLeft => index.saturating_sub(1),
        egui::Key::ArrowRight => (index + 1).min(count - 1),
        egui::Key::ArrowUp if index >= columns => index - columns,
        egui::Key::ArrowDown if index / columns < (count - 1) / columns => {
            (index + columns).min(count - 1)
        }
        _ => index,
    })
}
fn icon_columns(width: f32) -> usize {
    ((width + 8.) / 158.).floor().max(1.) as usize
}
#[cfg(test)]
#[path = "browser_test.rs"]
mod interaction_tests;

#[cfg(test)]
mod tests {
    #[test]
    fn unavailable_folder_does_not_open_its_parent_or_replace_the_organizer() {
        let mut browser = super::Browser::new(eframe::egui::Context::default());
        browser.files = vec![std::path::PathBuf::from("retained.x3f")];
        let missing =
            std::env::temp_dir().join(format!("foveonic-missing-folder-{}", std::process::id()));
        assert!(browser.open(&missing).is_err());
        assert_eq!(browser.files, [std::path::PathBuf::from("retained.x3f")]);
    }
    #[test]
    fn pane_width_reflows_icon_columns() {
        for (width, count) in [(150., 1), (280., 1), (330., 2), (500., 3)] {
            assert_eq!(super::icon_columns(width), count);
        }
    }
}
