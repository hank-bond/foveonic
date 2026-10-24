//! Browses X3F paths with workspace keyboard selection and bounded embedded and rendered previews.

use crate::preview_image::PreparedPreview;
use eframe::egui;
use foveonic::decode_cache::EntryState;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
    thread,
};

#[path = "browser_folders.rs"]
mod folders;
#[path = "browser_review.rs"]
mod review;

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
const LIST_ROW_HEIGHT: f32 = 40.;
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mark {
    #[default]
    Unmarked,
    Pick,
    Hidden,
    Imported,
}
impl Mark {
    pub fn hidden(self) -> bool {
        matches!(self, Self::Hidden | Self::Imported)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Unmarked => "",
            Self::Pick => "Pick",
            Self::Hidden => "Hidden",
            Self::Imported => "Imported",
        }
    }
}
pub struct Browser {
    pub files: Vec<PathBuf>,
    pub root: Option<PathBuf>,
    pub import_mode: bool,
    pub marks: HashMap<PathBuf, Mark>,
    pub recorded_marks: HashMap<PathBuf, Mark>,
    pub excluded: HashSet<PathBuf>,
    pub hide_reviewed: bool,
    pub clear_selection: bool,
    shortcuts: Vec<review::Shortcut>,
    highlighted: HashSet<PathBuf>,
    anchor: Option<PathBuf>,
    last_active: Option<PathBuf>,
    review_clicked: bool,
    focus_requested: bool,
    advance_hidden: bool,
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
            root: None,
            import_mode: false,
            marks: HashMap::new(),
            recorded_marks: HashMap::new(),
            excluded: HashSet::new(),
            hide_reviewed: true,
            clear_selection: false,
            shortcuts: Vec::new(),
            highlighted: HashSet::new(),
            anchor: None,
            last_active: None,
            review_clicked: false,
            focus_requested: false,
            advance_hidden: false,
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
        let files = folders::discover(dir)?;
        self.files = files;
        self.root = Some(dir.to_owned());
        self.marks.clear();
        self.recorded_marks.clear();
        self.excluded.clear();
        self.highlighted.clear();
        self.anchor = None;
        self.last_active = None;
        self.clear_selection = false;
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
        let text = self.review_control(ui, path, caption);
        ui.painter().with_clip_rect(text).text(
            text.center(),
            egui::Align2::CENTER_CENTER,
            path.file_stem().unwrap_or_default().to_string_lossy(),
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
    pub fn mark(&self, path: &Path) -> Mark {
        self.marks
            .get(path)
            .or_else(|| self.recorded_marks.get(path))
            .copied()
            .unwrap_or_default()
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
        if self.import_mode {
            format!("{}\n{help}\n{} · Space checks highlighted photos. Delete hides without deleting originals.", path.display(), self.mark(path).label())
        } else {
            format!("{}\n{help}", path.display())
        }
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
        self.review_clicked = false;
        self.clear_selection = false;
        let focus_requested = std::mem::take(&mut self.focus_requested);
        if self.last_active.as_deref() != selected {
            if let Some(path) = selected.filter(|path| !self.excluded.contains(*path)) {
                if !self.highlighted.contains(path) {
                    self.highlighted.clear();
                    self.highlighted.insert(path.to_owned());
                    self.anchor = Some(path.to_owned());
                }
            }
            self.last_active = selected.map(Path::to_owned);
        }
        let files: Vec<_> = self
            .files
            .iter()
            .filter(|path| !self.excluded.contains(*path))
            .cloned()
            .collect();
        // Rows come and go as the list scrolls. A stable organizer-level focus
        // target keeps keyboard selection working when the focused row disappears.
        let focus = ui.interact(
            ui.available_rect_before_wrap(),
            ui.make_persistent_id("organizer focus"),
            egui::Sense::focusable_noninteractive(),
        );
        if focus_requested && ui.is_enabled() {
            focus.request_focus();
            ui.ctx().request_repaint();
        } else if focus.clicked_elsewhere() || !ui.is_enabled() {
            focus.surrender_focus();
        } else if ui.input(|input| input.pointer.primary_pressed())
            && ui.rect_contains_pointer(focus.rect)
            && !focus.has_focus()
        {
            // Keep one focus target while virtualized image rows come and go.
            focus.request_focus();
        }
        let mut chosen = None;
        let width = ui.available_width();
        let columns = icon_columns(width);
        let icons = mode == crate::preferences::Organizer::Icons;
        let height = if icons {
            GRID_TILE_HEIGHT
        } else {
            LIST_ROW_HEIGHT
        };
        let layout = folders::Layout::new(
            &files,
            self.root.as_deref(),
            if icons { columns } else { 1 },
            height,
            ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y,
            ui.spacing().item_spacing.y,
        );
        let mut active = selected.and_then(|path| files.iter().position(|p| p == path));
        let shortcuts = std::mem::take(&mut self.shortcuts);
        if !shortcuts.is_empty() {
            ui.ctx().request_repaint();
        }
        if ui.is_enabled() {
            for shortcut in shortcuts {
                match shortcut {
                    review::Shortcut::SelectAll => {
                        self.highlighted = files.iter().cloned().collect()
                    }
                    review::Shortcut::TogglePick | review::Shortcut::Hide => {
                        if self.import_mode {
                            if self.highlighted.is_empty() {
                                if let Some(path) =
                                    selected.filter(|path| files.iter().any(|file| file == *path))
                                {
                                    self.highlighted.insert(path.to_owned());
                                    self.anchor = Some(path.to_owned());
                                }
                            }
                            if matches!(shortcut, review::Shortcut::TogglePick) {
                                self.toggle_highlighted(&files);
                            } else {
                                self.hide_highlighted(&files);
                            }
                        }
                    }
                    review::Shortcut::Arrow(key, modifiers) => {
                        let next = layout.arrow(active, key);
                        if next != active {
                            active = next;
                            chosen = next.map(|index| files[index].clone());
                            if let Some(path) = &chosen {
                                self.highlight(path, &files, modifiers);
                            }
                        }
                    }
                }
            }
        }
        let mut scroll = egui::ScrollArea::vertical().id_salt("organizer items");
        let reveal_selection = std::mem::take(&mut self.reveal_selection);
        if let Some(active) = active.filter(|_| chosen.is_some() || reveal_selection) {
            let row = layout.photo_row(active);
            let top = row.top;
            let bottom = row.top + row.height;
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
        let highlighted = self.highlighted.clone();
        scroll.show_viewport(ui, |ui, viewport| {
            ui.set_height(layout.height);
            let origin = ui.max_rect().min;
            for row in layout.visible(viewport.top(), viewport.bottom()) {
                let rect = egui::Rect::from_min_size(
                    origin + egui::vec2(0., row.top),
                    egui::vec2(width, row.height),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .id_salt(match &row.content {
                            folders::Content::Folder(path) => ("folder", path),
                            folders::Content::Photos(range) => ("photos", &files[range.start]),
                        })
                        .max_rect(rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                    |ui| {
                        let folders::Content::Photos(range) = &row.content else {
                            if let folders::Content::Folder(directory) = &row.content {
                                // The first group shares the browser header's divider.
                                folders::heading(
                                    ui,
                                    rect,
                                    directory,
                                    self.root.as_deref(),
                                    row.top > 0.,
                                );
                            }
                            return;
                        };
                        if icons {
                            let tile_width = (width
                                - (columns - 1) as f32 * ui.spacing().item_spacing.x)
                                / columns as f32;
                            ui.horizontal(|ui| {
                                for path in &files[range.clone()] {
                                    if self
                                        .tile(
                                            ui,
                                            path,
                                            highlighted.contains(path),
                                            egui::vec2(tile_width, GRID_TILE_HEIGHT),
                                        )
                                        .clicked()
                                        && !self.review_clicked
                                    {
                                        self.highlight(
                                            path,
                                            &files,
                                            ui.input(|input| input.modifiers),
                                        );
                                        if self.highlighted.contains(path) {
                                            chosen = Some(path.clone());
                                        }
                                    }
                                }
                            });
                        } else {
                            let path = files[range.start].clone();
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(width, LIST_ROW_HEIGHT),
                                egui::Sense::CLICK,
                            );
                            crate::widgets::paint(ui, &response, highlighted.contains(&path));
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
                                        egui::Rect::from_min_max(
                                            egui::Pos2::ZERO,
                                            egui::pos2(1., 1.),
                                        ),
                                        egui::Color32::WHITE,
                                    );
                                }
                                let caption = egui::Rect::from_min_max(
                                    egui::pos2(rect.left() + 45., rect.top()),
                                    egui::pos2(rect.right() - STATUS_GUTTER, rect.bottom()),
                                );
                                let text = self.review_control(ui, &path, caption);
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
                            if response.on_hover_text(self.file_help(&path)).clicked()
                                && !self.review_clicked
                            {
                                self.highlight(&path, &files, ui.input(|input| input.modifiers));
                                if self.highlighted.contains(&path) {
                                    chosen = Some(path);
                                }
                            }
                        }
                    },
                );
            }
        });
        // Background history updates may conceal a row, but must not move the
        // viewed photo or trigger another render as each import completes.
        let advance_hidden = std::mem::take(&mut self.advance_hidden);
        if let Some(path) = chosen
            .as_deref()
            .or(selected)
            .filter(|path| advance_hidden && self.excluded.contains(*path))
        {
            chosen = self.next_visible(path);
            self.highlighted.clear();
            if let Some(next) = &chosen {
                self.highlighted.insert(next.clone());
                self.anchor = Some(next.clone());
            }
            self.clear_selection = chosen.is_none();
            ui.ctx().request_repaint();
        }
        if (chosen.is_some() || self.review_clicked) && !focus.has_focus() {
            focus.request_focus();
            ui.ctx().request_repaint();
        }
        if self.review_clicked {
            ui.ctx().request_repaint();
        }
        chosen.filter(|path| Some(path.as_path()) != selected)
    }
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
