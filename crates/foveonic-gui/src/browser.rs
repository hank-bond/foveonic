//! Browses X3F paths with focused keyboard selection and bounded embedded and rendered previews.

use crate::preview_image::PreparedPreview;
use eframe::egui;
use foveonic::decode_cache::EntryState;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
    thread,
};

// Leave a quiet status column beside the filename, outside photographic content.
const STATUS_RADIUS: f32 = 3.;
const STATUS_INSET: f32 = 10.;
const STATUS_GUTTER: f32 = 20.;
const THUMBNAIL_CACHE_ENTRIES: usize = 256;
const PREVIEW_CACHE_ENTRIES: usize = 2;
const REQUEST_CAPACITY: usize = 32;
// A miss may become ready after background rendering or folder prefetch.
const PREVIEW_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
// Bound queued decoded display pixels separately from the inexpensive path queue.
const REPLY_CAPACITY: usize = 2;
const GRID_TILE_HEIGHT: f32 = 146.;
// Image padding includes the filename band below each thumbnail.
const GRID_IMAGE_INSETS: egui::Vec2 = egui::vec2(12., 28.);
// Preserve the existing grid reflow thresholds independently of the panel minimum.
const GRID_REFLOW_TILE_WIDTH: f32 = 150.;
const GRID_REFLOW_GAP: f32 = 8.;
const LANDSCAPE_ASPECT: f32 = 3. / 2.;
const GRID_IMAGE_CENTER_OFFSET: egui::Vec2 = egui::vec2(0., 10.);
const CAPTION_TOP_INSET: f32 = 24.;
const CAPTION_BOTTOM_INSET: f32 = 2.;
const CAPTION_FONT_SIZE: f32 = 12.;

/// Keep a native 3:2 landscape thumbnail limited by its height at one column.
/// List mode shares this bound so switching organizer modes preserves its width.
pub fn minimum_panel_width(style: &egui::Style) -> f32 {
    let image_height = GRID_TILE_HEIGHT - GRID_IMAGE_INSETS.y;
    image_height * LANDSCAPE_ASPECT
        + GRID_IMAGE_INSETS.x
        + egui::Frame::side_top_panel(style).inner_margin.sum().x
}
fn cache_appearance(state: EntryState) -> (egui::Color32, &'static str) {
    match state {
        EntryState::Disabled => (egui::Color32::from_gray(100), "Decode caching is disabled for this source."),
        EntryState::Unsupported => (egui::Color32::from_gray(100), "Decode caching is unavailable for this source."),
        EntryState::Unavailable => (egui::Color32::from_gray(100), "Cannot read this source to check cache eligibility."),
        EntryState::Queued => (egui::Color32::from_rgb(158, 136, 75), "Queued for caching or preparing decoded data."),
        EntryState::Ready => (egui::Color32::from_rgb(85, 137, 99), "Decoded source cached and ready to open."),
        EntryState::Uncached => (egui::Color32::from_rgb(155, 91, 88), "Supported, but not cached. The entry may have been cleared, evicted, or unable to fit in the cache."),
    }
}

#[derive(Clone, Copy)]
enum RequestKind {
    EmbeddedThenCached,
    CachedOnly,
}
struct Request {
    path: PathBuf,
    version: u64,
    kind: RequestKind,
}
struct Reply {
    path: PathBuf,
    version: u64,
    image: Result<PreparedPreview, String>,
}

#[derive(Clone)]
pub struct Thumbnail {
    pub texture: egui::TextureHandle,
    /// Oriented full-size presentation dimensions, independent of the thumbnail's resolution.
    pub native_extent: [u32; 2],
}
pub struct Browser {
    pub files: Vec<PathBuf>,
    pub decode_states: Arc<HashMap<PathBuf, EntryState>>,
    pub cache_enabled: bool,
    sender: mpsc::SyncSender<Request>,
    receiver: mpsc::Receiver<Reply>,
    pending: HashSet<PathBuf>,
    cache: HashMap<PathBuf, Option<Thumbnail>>,
    previews: HashMap<PathBuf, Thumbnail>,
    preview_order: VecDeque<PathBuf>,
    versions: HashMap<PathBuf, u64>,
    sequence: u64,
    preview_attempts: HashMap<PathBuf, std::time::Instant>,
    rendered_revisions: HashMap<PathBuf, u64>,
    order: VecDeque<PathBuf>,
    reveal_selection: bool,
}
impl Browser {
    #[cfg(test)]
    pub fn new(context: egui::Context) -> Self {
        Self::with_cache(context, None)
    }
    pub fn with_cache(
        context: egui::Context,
        cache: Option<Arc<foveonic::decode_cache::Cache>>,
    ) -> Self {
        let (sender, requests) = mpsc::sync_channel::<Request>(REQUEST_CAPACITY);
        let (ready, receiver) = mpsc::sync_channel(REPLY_CAPACITY);
        thread::spawn(move || {
            while let Ok(Request {
                path,
                version,
                kind,
            }) = requests.recv()
            {
                if matches!(kind, RequestKind::EmbeddedThenCached) {
                    let thumb = (|| -> foveonic::source::Result<PreparedPreview> {
                        let preview = foveonic::preview::read(&path)?;
                        PreparedPreview::new(preview, false)
                    })()
                    .map_err(|e| e.to_string());
                    if ready
                        .send(Reply {
                            path: path.clone(),
                            version,
                            image: thumb,
                        })
                        .is_err()
                    {
                        break;
                    }
                    context.request_repaint();
                }
                if let Some(preview) = cache.as_ref().and_then(|c| c.preview(&path)) {
                    let prepared = PreparedPreview::new(preview, true).map_err(|e| e.to_string());
                    if ready
                        .send(Reply {
                            path,
                            version,
                            image: prepared,
                        })
                        .is_err()
                    {
                        break;
                    }
                    context.request_repaint();
                } else if matches!(kind, RequestKind::CachedOnly) {
                    if ready
                        .send(Reply {
                            path,
                            version,
                            image: Err("No cached preview".into()),
                        })
                        .is_err()
                    {
                        break;
                    }
                    context.request_repaint();
                }
            }
        });
        Self {
            files: Vec::new(),
            decode_states: Arc::new(HashMap::new()),
            cache_enabled: false,
            sender,
            receiver,
            pending: HashSet::new(),
            cache: HashMap::new(),
            previews: HashMap::new(),
            preview_order: VecDeque::new(),
            versions: HashMap::new(),
            sequence: 0,
            preview_attempts: HashMap::new(),
            rendered_revisions: HashMap::new(),
            order: VecDeque::new(),
            reveal_selection: false,
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
        self.decode_states = Arc::new(HashMap::new());
        self.cache.clear();
        self.previews.clear();
        self.preview_order.clear();
        self.versions.clear();
        self.pending.clear();
        self.preview_attempts.clear();
        self.rendered_revisions.clear();
        self.order.clear();
        Ok(())
    }
    pub fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(Reply {
            path,
            version,
            image,
        }) = self.receiver.try_recv()
        {
            if self.versions.get(&path) != Some(&version) || !self.files.contains(&path) {
                continue;
            }
            self.pending.remove(&path);
            if let Ok(image) = image {
                self.insert(ctx, path, image);
            } else {
                self.cache.entry(path).or_insert(None);
            }
        }
    }
    fn insert(&mut self, ctx: &egui::Context, path: PathBuf, image: PreparedPreview) {
        let native_extent = image.thumbnail.native_extent;
        let thumbnail = Thumbnail {
            texture: ctx.load_texture(
                path.to_string_lossy(),
                image.thumbnail.image,
                egui::TextureOptions::LINEAR,
            ),
            native_extent,
        };
        self.cache.insert(path.clone(), Some(thumbnail));
        self.order.retain(|p| p != &path);
        self.order.push_back(path.clone());
        while self.order.len() > THUMBNAIL_CACHE_ENTRIES {
            if let Some(old) = self.order.pop_front() {
                self.cache.remove(&old);
            }
        }
        if let Some(preview) = image.preview {
            self.previews.insert(
                path.clone(),
                Thumbnail {
                    texture: ctx.load_texture(
                        format!("{} preview", path.display()),
                        preview,
                        egui::TextureOptions::LINEAR,
                    ),
                    native_extent,
                },
            );
            self.preview_order.retain(|p| p != &path);
            self.preview_order.push_back(path);
            while self.preview_order.len() > PREVIEW_CACHE_ENTRIES {
                if let Some(old) = self.preview_order.pop_front() {
                    self.previews.remove(&old);
                }
            }
        }
    }
    pub(crate) fn rendered(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        revision: u64,
        image: PreparedPreview,
    ) {
        if self.files.contains(&path)
            && self
                .rendered_revisions
                .get(&path)
                .is_none_or(|old| *old <= revision)
        {
            self.rendered_revisions.insert(path.clone(), revision);
            self.sequence += 1;
            self.versions.insert(path.clone(), self.sequence);
            self.pending.remove(&path);
            self.insert(ctx, path, image);
        }
    }
    pub fn rendered_revision(&self, path: &Path) -> Option<u64> {
        self.rendered_revisions.get(path).copied()
    }
    pub fn preview(&mut self, path: &Path) -> Option<Thumbnail> {
        if let Some(preview) = self.previews.get(path) {
            return Some(preview.clone());
        }
        if self.cache.contains_key(path)
            && self
                .preview_attempts
                .get(path)
                .is_none_or(|t| t.elapsed() >= PREVIEW_RETRY_INTERVAL)
        {
            self.preview_attempts
                .insert(path.to_owned(), std::time::Instant::now());
            self.request(path, RequestKind::CachedOnly);
        }
        self.thumbnail(path)
    }
    fn request(&mut self, path: &Path, kind: RequestKind) {
        if self.pending.contains(path) {
            return;
        }
        self.sequence += 1;
        if self
            .sender
            .try_send(Request {
                path: path.to_owned(),
                version: self.sequence,
                kind,
            })
            .is_ok()
        {
            self.versions.insert(path.to_owned(), self.sequence);
            self.pending.insert(path.to_owned());
        }
    }
    pub fn reveal_selection(&mut self) {
        self.reveal_selection = true;
    }
    pub fn thumbnails_ready(&self) -> bool {
        self.pending.is_empty() && self.cache.values().all(Option::is_some)
    }
    pub fn thumbnail(&mut self, path: &Path) -> Option<Thumbnail> {
        if let Some(texture) = self.cache.get(path) {
            return texture.clone();
        }
        self.request(path, RequestKind::EmbeddedThenCached);
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
        crate::widgets::paint(ui, &response, selected);
        if let Some(image) = ui
            .is_rect_visible(rect)
            .then(|| self.thumbnail(path))
            .flatten()
        {
            let intrinsic =
                egui::vec2(image.native_extent[0] as f32, image.native_extent[1] as f32);
            let max = rect.size() - GRID_IMAGE_INSETS;
            let s = (max.x / intrinsic.x).min(max.y / intrinsic.y);
            let image_rect = egui::Rect::from_center_size(
                rect.center() - GRID_IMAGE_CENTER_OFFSET,
                intrinsic * s,
            );
            ui.painter().image(
                image.texture.id(),
                image_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
        }
        let name = path.file_stem().unwrap_or_default().to_string_lossy();
        let caption = egui::Rect::from_min_max(
            egui::pos2(
                rect.left() + GRID_IMAGE_INSETS.x / 2.,
                rect.bottom() - CAPTION_TOP_INSET,
            ),
            egui::pos2(
                rect.right() - STATUS_GUTTER,
                rect.bottom() - CAPTION_BOTTOM_INSET,
            ),
        );
        ui.painter().with_clip_rect(caption).text(
            caption.center(),
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::proportional(CAPTION_FONT_SIZE),
            egui::Color32::LIGHT_GRAY,
        );
        self.cache_badge(
            ui,
            path,
            egui::pos2(rect.right() - STATUS_INSET, caption.center().y),
        );
        response.on_hover_text(self.file_help(path))
    }
    fn cache_state(&self, path: &Path) -> EntryState {
        if !self.cache_enabled {
            EntryState::Disabled
        } else {
            self.decode_states
                .get(path)
                .copied()
                .unwrap_or(EntryState::Queued)
        }
    }
    fn file_help(&self, path: &Path) -> String {
        let (_, help) = cache_appearance(self.cache_state(path));
        format!("{}\n{help}", path.display())
    }
    fn cache_badge(&self, ui: &egui::Ui, path: &Path, center: egui::Pos2) {
        let (color, _) = cache_appearance(self.cache_state(path));
        ui.painter().circle_filled(center, STATUS_RADIUS, color);
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
        let height = if icons { GRID_TILE_HEIGHT } else { 40. };
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
        let reveal_selection = std::mem::take(&mut self.reveal_selection);
        if let Some(active) = active.filter(|_| chosen.is_some() || reveal_selection) {
            let row = active / if icons { columns } else { 1 };
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
                                    egui::vec2(tile_width, GRID_TILE_HEIGHT),
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
                    crate::widgets::paint(
                        ui,
                        &response,
                        highlighted.as_deref() == Some(path.as_path()),
                    );
                    if ui.is_rect_visible(rect) {
                        if let Some(thumb) = self.thumbnail(&path) {
                            let size = egui::vec2(
                                thumb.native_extent[0] as f32,
                                thumb.native_extent[1] as f32,
                            );
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
                            egui::pos2(rect.right() - STATUS_GUTTER, rect.bottom()),
                        );
                        ui.painter().with_clip_rect(text).text(
                            egui::pos2(text.left(), text.center().y),
                            egui::Align2::LEFT_CENTER,
                            path.file_stem().unwrap_or_default().to_string_lossy(),
                            egui::FontId::proportional(CAPTION_FONT_SIZE),
                            egui::Color32::LIGHT_GRAY,
                        );
                    }
                    self.cache_badge(
                        ui,
                        &path,
                        rect.right_center() + egui::vec2(-STATUS_INSET, 0.),
                    );
                    if response.on_hover_text(self.file_help(&path)).clicked() {
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
    ((width + GRID_REFLOW_GAP) / (GRID_REFLOW_TILE_WIDTH + GRID_REFLOW_GAP))
        .floor()
        .max(1.) as usize
}
#[cfg(test)]
#[path = "browser_test.rs"]
mod interaction_tests;

#[cfg(test)]
mod tests {
    fn prepared(color: [u8; 3], extent: [u32; 2]) -> super::PreparedPreview {
        super::PreparedPreview::new(
            foveonic::preview::Preview {
                rgb: color.repeat((extent[0] * extent[1]) as usize),
                extent,
                rotation: 0,
                native_extent: [6000, 3000],
            },
            true,
        )
        .unwrap()
    }
    #[test]
    fn rendered_thumbnails_replace_embedded_images_and_reject_late_replies() {
        let context = eframe::egui::Context::default();
        let mut browser = super::Browser::new(context.clone());
        let path = std::path::PathBuf::from("photo.x3f");
        browser.files.push(path.clone());
        browser.sequence = 1;
        browser.versions.insert(path.clone(), 1);
        browser.insert(&context, path.clone(), prepared([255, 0, 0], [16, 8]));
        let original = browser.thumbnail(&path).unwrap().texture.id();
        browser.rendered(
            &context,
            path.clone(),
            2,
            prepared([0, 255, 0], [1024, 512]),
        );
        let rendered = browser.thumbnail(&path).unwrap().texture.id();
        assert_ne!(original, rendered);
        assert_eq!(browser.preview(&path).unwrap().texture.size(), [1024, 512]);
        let (tx, rx) = std::sync::mpsc::channel();
        browser.receiver = rx;
        tx.send(super::Reply {
            path: path.clone(),
            version: 1,
            image: Ok(prepared([0, 0, 255], [16, 8])),
        })
        .unwrap();
        browser.receive(&context);
        assert_eq!(browser.thumbnail(&path).unwrap().texture.id(), rendered);
        assert_eq!(browser.preview(&path).unwrap().texture.size(), [1024, 512]);
    }
    #[test]
    fn larger_previews_are_bounded_independently_of_thumbnails() {
        let context = eframe::egui::Context::default();
        let mut browser = super::Browser::new(context.clone());
        for name in ["one.x3f", "two.x3f", "three.x3f"] {
            browser.insert(&context, name.into(), prepared([128; 3], [16, 8]));
        }
        assert_eq!(browser.previews.len(), 2);
        assert_eq!(browser.cache.len(), 3);
        assert!(!browser
            .previews
            .contains_key(std::path::Path::new("one.x3f")));
        assert!(browser.thumbnail(std::path::Path::new("one.x3f")).is_some());
    }
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
