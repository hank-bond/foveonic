//! Browses X3F paths with workspace keyboard selection and bounded embedded and rendered previews.

use crate::{preview_image::PreparedPreview, shortcuts::BrowserAction};
use eframe::egui;
use foveonic::decode_cache::EntryState;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
};

#[path = "browser_chrome.rs"]
mod chrome;
#[path = "browser_folders.rs"]
mod folders;
#[path = "browser_items.rs"]
mod items;
#[path = "browser_loader.rs"]
mod loader;
#[path = "browser_metadata.rs"]
mod metadata_loader;
#[path = "browser_query.rs"]
mod query;
#[path = "browser_review.rs"]
mod review;
use loader::{Kind, Reply, Request, Worker as Loader};
pub use query::{Field, Sort};

pub struct Selection {
    pub path: PathBuf,
    /// Captured from the navigation event, independently of loading time.
    pub preserve_viewport: bool,
}

// Leave a quiet status column beside the filename, outside photographic content.
const STATUS_RADIUS: f32 = 3.;
const STATUS_INSET: f32 = 10.;
const STATUS_GUTTER: f32 = 20.;
const THUMBNAIL_CACHE_ENTRIES: usize = 256;
const PREVIEW_CACHE_ENTRIES: usize = 8;
// Texture uploads must leave time for input and the selected image every frame.
const REPLIES_PER_FRAME: usize = 4;
// A miss may become ready after background rendering or folder prefetch.
const PREVIEW_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
#[cfg(test)]
const GRID_TILE_HEIGHT: f32 = 176.;
const LIST_ROW_HEIGHT: f32 = 40.;
// Start another column only when every grid cell can reach this width.
const GRID_REFLOW_TILE_WIDTH: f32 = 100.;
const MIN_BROWSER_WIDTH: f32 = 200.;

/// Keep a native 3:2 landscape thumbnail limited by its height at one column.
/// List mode shares this bound so switching organizer modes preserves its width.
pub fn minimum_panel_width(style: &egui::Style) -> f32 {
    MIN_BROWSER_WIDTH + egui::Frame::side_top_panel(style).inner_margin.sum().x
}
fn cache_appearance(state: EntryState) -> (egui::Color32, &'static str) {
    match state {
        EntryState::Disabled => (egui::Color32::from_gray(100), "Decode caching is disabled for this source."),
        EntryState::Unsupported => (egui::Color32::from_gray(100), "Decode caching is unavailable for this source."),
        EntryState::Unavailable => (egui::Color32::from_gray(100), "Cannot read this source to check cache eligibility."),
        EntryState::Queued => (egui::Color32::from_rgb(158, 136, 75), "Queued for caching or preparing decoded data."),
        EntryState::Ready => (egui::Color32::from_rgb(85, 137, 99), "RAW, metadata, thumbnails and original file cached and ready."),
        EntryState::Uncached => (egui::Color32::from_rgb(155, 91, 88), "Supported, but not cached. The entry may have been cleared, evicted, or unable to fit in the cache."),
    }
}

#[derive(Clone)]
pub struct Thumbnail {
    pub texture: crate::display_image::Texture,
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
    metadata_loader: metadata_loader::Loader,
    metadata_generation: u64,
    records: HashMap<PathBuf, query::Entry>,
    metadata_loaded: HashSet<PathBuf>,
    rating_changes: HashMap<PathBuf, foveonic::review::Rating>,
    rating_errors: std::collections::BTreeMap<PathBuf, String>,
    pub sort: Sort,
    applied_sort: Sort,
    filter: query::Filter,
    query_dirty: bool,
    render_state: Option<eframe::egui_wgpu::RenderState>,
    pub files: Vec<PathBuf>,
    pub root: Option<PathBuf>,
    pub import_mode: bool,
    pub marks: HashMap<PathBuf, Mark>,
    pub recorded_marks: HashMap<PathBuf, Mark>,
    pub excluded: HashSet<PathBuf>,
    pub hide_reviewed: bool,
    pub show_ratings: bool,
    pub clear_selection: bool,
    actions: Vec<(BrowserAction, egui::Modifiers)>,
    highlighted: HashSet<PathBuf>,
    anchor: Option<PathBuf>,
    last_active: Option<PathBuf>,
    review_clicked: bool,
    focus_requested: bool,
    advance_hidden: bool,
    pub decode_states: Arc<HashMap<PathBuf, EntryState>>,
    pub cache_enabled: bool,
    prefetch_dirty: bool,
    thumbnail_worker: Loader,
    preview_worker: Loader,
    thumbnail_pass: bool,
    visible: Vec<PathBuf>,
    receiver: mpsc::Receiver<Reply>,
    pending: HashSet<PathBuf>,
    pending_previews: HashSet<PathBuf>,
    preview_demand: HashSet<PathBuf>,
    // Complete one larger-preview lookup per thumbnail. The bounded viewport
    // cache must not make every visible row reload its larger image each second.
    preview_checked: HashSet<PathBuf>,
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
        Self::start(context, None, None)
    }
    pub fn with_cache(
        context: egui::Context,
        cache: Option<Arc<foveonic::decode_cache::Cache>>,
        render_state: eframe::egui_wgpu::RenderState,
    ) -> Self {
        Self::start(context, cache, Some(render_state))
    }
    fn start(
        context: egui::Context,
        cache: Option<Arc<foveonic::decode_cache::Cache>>,
        render_state: Option<eframe::egui_wgpu::RenderState>,
    ) -> Self {
        let (ready, receiver) = mpsc::sync_channel(loader::REPLY_CAPACITY);
        let metadata_loader = metadata_loader::Loader::new(context.clone(), cache.clone());
        let thumbnail_worker = Loader::start(
            Kind::Thumbnail,
            cache.clone(),
            context.clone(),
            ready.clone(),
        );
        let preview_worker = Loader::start(Kind::Preview, cache, context, ready);
        Self {
            metadata_loader,
            metadata_generation: 0,
            records: HashMap::new(),
            metadata_loaded: HashSet::new(),
            rating_changes: HashMap::new(),
            rating_errors: Default::default(),
            sort: Sort::default(),
            applied_sort: Sort::default(),
            filter: query::Filter::default(),
            query_dirty: false,
            render_state,
            files: Vec::new(),
            root: None,
            import_mode: false,
            marks: HashMap::new(),
            recorded_marks: HashMap::new(),
            excluded: HashSet::new(),
            hide_reviewed: true,
            show_ratings: false,
            clear_selection: false,
            actions: Vec::new(),
            highlighted: HashSet::new(),
            anchor: None,
            last_active: None,
            review_clicked: false,
            focus_requested: false,
            advance_hidden: false,
            decode_states: Arc::new(HashMap::new()),
            cache_enabled: false,
            prefetch_dirty: true,
            thumbnail_worker,
            preview_worker,
            thumbnail_pass: true,
            visible: Vec::new(),
            receiver,
            pending: HashSet::new(),
            pending_previews: HashSet::new(),
            preview_demand: HashSet::new(),
            preview_checked: HashSet::new(),
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
        self.thumbnail_worker.replace(Vec::new(), &[]);
        self.preview_worker.replace(Vec::new(), &[]);
        self.thumbnail_pass = true;
        self.visible.clear();
        self.files = files;
        self.metadata_generation = self.metadata_loader.replace(&self.files);
        self.records.clear();
        self.metadata_loaded.clear();
        query::sort(&mut self.files, &self.records, self.sort);
        self.rating_changes.clear();
        self.rating_errors.clear();
        self.query_dirty = true;
        self.prefetch_dirty = true;
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
        self.pending_previews.clear();
        self.preview_demand.clear();
        self.preview_checked.clear();
        self.preview_attempts.clear();
        self.rendered_revisions.clear();
        self.order.clear();
        Ok(())
    }
    pub fn set_actions(&mut self, actions: Vec<(BrowserAction, egui::Modifiers)>) {
        self.actions = actions;
    }
    pub fn invalidate_previews(&mut self) {
        for path in &self.files {
            self.thumbnail_worker.cancel(path);
            self.preview_worker.cancel(path);
        }
        self.sequence += 1;
        self.versions.clear();
        self.pending.clear();
        self.pending_previews.clear();
        self.preview_demand.clear();
        self.preview_checked.clear();
        self.preview_attempts.clear();
        self.cache.clear();
        self.previews.clear();
        self.preview_order.clear();
        self.rendered_revisions.clear();
        self.order.clear();
        self.thumbnail_pass = true;
    }
    pub fn invalidate_prefetch(&mut self) {
        self.prefetch_dirty = true;
    }
    /// Publish order changes once. Selection changes also invalidate this snapshot.
    /// A photo concealed by a history update may remain active in the viewport.
    pub fn take_prefetch_order(&mut self, selected: Option<&Path>) -> Option<Vec<PathBuf>> {
        std::mem::take(&mut self.prefetch_dirty).then(|| {
            self.files
                .iter()
                .filter(|path| !self.excluded.contains(*path) || selected == Some(path.as_path()))
                .cloned()
                .collect()
        })
    }
    pub fn receive(&mut self, ctx: &egui::Context) {
        self.receive_metadata(ctx);
        for _ in 0..REPLIES_PER_FRAME {
            let Ok(Reply {
                path,
                version,
                kind,
                image,
            }) = self.receiver.try_recv()
            else {
                return;
            };
            if self.versions.get(&path) != Some(&version) || !self.files.contains(&path) {
                continue;
            }
            match kind {
                Kind::Thumbnail => {
                    self.pending.remove(&path);
                }
                Kind::Preview => {
                    self.pending_previews.remove(&path);
                    self.preview_checked.insert(path.clone());
                }
            }
            if let Ok(image) = image {
                self.insert(ctx, path, image);
            } else if kind == Kind::Thumbnail {
                // A missing larger preview says nothing about a small thumbnail
                // that may have been evicted while this reply was in flight.
                self.cache.entry(path).or_insert(None);
            }
        }
        ctx.request_repaint();
    }
    fn upload(
        &self,
        _ctx: &egui::Context,
        pixels: crate::display_image::Pixels,
    ) -> crate::display_image::Texture {
        if let Some(rs) = &self.render_state {
            return pixels.upload(rs);
        }
        #[cfg(test)]
        {
            pixels.headless(_ctx)
        }
        #[cfg(not(test))]
        {
            unreachable!("browser requires the shared display device")
        }
    }
    fn insert(&mut self, ctx: &egui::Context, path: PathBuf, image: PreparedPreview) {
        let native_extent = image.thumbnail.native_extent;
        let entry = self.records.entry(path.clone()).or_default();
        if entry.extent != Some(native_extent) {
            entry.extent = Some(native_extent);
            self.query_dirty = true;
        }
        let thumbnail = Thumbnail {
            texture: self.upload(ctx, image.thumbnail.image),
            native_extent,
        };
        self.cache.insert(path.clone(), Some(thumbnail));
        self.order.retain(|p| p != &path);
        self.order.push_back(path.clone());
        while self.order.len() > THUMBNAIL_CACHE_ENTRIES {
            let index = self
                .order
                .iter()
                .position(|p| !self.visible.contains(p))
                .unwrap_or(0);
            if let Some(old) = self.order.remove(index) {
                self.cache.remove(&old);
            }
        }
        if let Some(preview) = image.preview {
            self.previews.insert(
                path.clone(),
                Thumbnail {
                    texture: self.upload(ctx, preview),
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
            self.pending_previews.remove(&path);
            self.preview_checked.insert(path.clone());
            self.thumbnail_worker.cancel(&path);
            self.preview_worker.cancel(&path);
            self.insert(ctx, path, image);
        }
    }
    pub fn rendered_revision(&self, path: &Path) -> Option<u64> {
        self.rendered_revisions.get(path).copied()
    }
    pub fn preview(&mut self, path: &Path) -> Option<Thumbnail> {
        self.preview_demand.insert(path.to_owned());
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
            self.request(path, Kind::Preview);
        }
        self.thumbnail(path)
    }
    fn request(&mut self, path: &Path, kind: Kind) {
        let pending = match kind {
            Kind::Thumbnail => &mut self.pending,
            Kind::Preview => &mut self.pending_previews,
        };
        if !pending.insert(path.to_owned()) {
            return;
        }
        let version = if kind == Kind::Thumbnail {
            self.pending_previews.remove(path);
            self.preview_checked.remove(path);
            self.preview_worker.cancel(path);
            self.sequence += 1;
            self.versions.insert(path.to_owned(), self.sequence);
            self.sequence
        } else if let Some(version) = self.versions.get(path) {
            *version
        } else {
            pending.remove(path);
            return;
        };
        let request = Request {
            path: path.to_owned(),
            version,
        };
        match kind {
            Kind::Thumbnail => self.thumbnail_worker.request(request),
            Kind::Preview => self.preview_worker.request(request),
        }
    }
    /// Retire preview work only after every browser and viewport consumer has drawn.
    pub fn finish_previews(&mut self) {
        let wanted: Vec<_> = self.preview_demand.drain().collect();
        self.preview_worker.retain(&wanted);
        self.pending_previews.retain(|p| wanted.contains(p));
    }
    fn schedule_thumbnails(&mut self, visible: Vec<PathBuf>) {
        self.visible = visible;
        self.metadata_loader.visible(&self.visible);
        if std::mem::take(&mut self.thumbnail_pass) {
            let requests = self
                .files
                .iter()
                .filter(|p| !self.cache.contains_key(*p))
                .map(|path| {
                    let version = if self.pending.contains(path) {
                        self.versions[path]
                    } else {
                        self.sequence += 1;
                        self.versions.insert(path.clone(), self.sequence);
                        self.pending.insert(path.clone());
                        self.sequence
                    };
                    Request {
                        path: path.clone(),
                        version,
                    }
                })
                .collect();
            self.thumbnail_worker.replace(requests, &self.visible);
        } else {
            self.thumbnail_worker.visible(&self.visible);
        }
        for path in self.visible.clone() {
            if self.cache.get(&path).is_some_and(Option::is_some)
                && !self.preview_checked.contains(&path)
            {
                self.preview(&path);
            }
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
        self.request(path, Kind::Thumbnail);
        None
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
    ) -> Option<Selection> {
        self.apply_query();
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
        let panel_width = ui.available_width();
        let icons = mode == crate::preferences::Organizer::Icons;
        let list = mode == crate::preferences::Organizer::List;
        let columns = if icons { icon_columns(panel_width) } else { 1 };
        let width = if list {
            panel_width.max(items::list_width(self.show_ratings))
        } else {
            panel_width
        };
        let header_height = if list { items::LIST_HEADER } else { 0. };
        let tile_width = width / columns as f32;
        let tile_height = items::dense_height(tile_width);
        let layout = folders::Layout::new(
            &files,
            self.root.as_deref(),
            columns,
            |range| {
                if icons {
                    tile_height
                } else if list {
                    LIST_ROW_HEIGHT
                } else {
                    self.large_height(&files[range.start], width)
                }
            },
            ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y,
            if icons {
                0.
            } else {
                ui.spacing().item_spacing.y
            },
        );
        let mut active = selected.and_then(|path| files.iter().position(|p| p == path));
        let actions = std::mem::take(&mut self.actions);
        if !actions.is_empty() {
            ui.ctx().request_repaint();
        }
        if ui.is_enabled() {
            for (action, modifiers) in actions {
                match action {
                    BrowserAction::SelectAll => self.highlighted = files.iter().cloned().collect(),
                    BrowserAction::TogglePick | BrowserAction::Hide => {
                        if self.import_mode {
                            if self.highlighted.is_empty() {
                                if let Some(path) =
                                    selected.filter(|path| files.iter().any(|file| file == *path))
                                {
                                    self.highlighted.insert(path.to_owned());
                                    self.anchor = Some(path.to_owned());
                                }
                            }
                            if matches!(action, BrowserAction::TogglePick) {
                                self.toggle_highlighted(&files);
                            } else {
                                self.hide_highlighted(&files);
                            }
                        }
                    }
                    BrowserAction::Arrow(key) => {
                        let next = layout.arrow(active, key);
                        if next != active {
                            active = next;
                            chosen = next.map(|index| Selection {
                                path: files[index].clone(),
                                preserve_viewport: modifiers.alt,
                            });
                            if let Some(selection) = &chosen {
                                self.highlight(&selection.path, &files, modifiers);
                            }
                        }
                    }
                    BrowserAction::Rate(stars) => {
                        let targets = self.processing_selection(selected);
                        self.rate(
                            &targets,
                            foveonic::review::Rating::try_from(stars).expect("registered rating"),
                        );
                    }
                }
            }
        }
        let scroll = if list {
            egui::ScrollArea::both()
        } else {
            egui::ScrollArea::vertical()
        }
        .id_salt("organizer items")
        .animated(false);
        let reveal_selection = std::mem::take(&mut self.reveal_selection);
        let reveal = active.filter(|_| chosen.is_some() || reveal_selection);
        let highlighted = self.highlighted.clone();
        scroll.show_viewport(ui, |ui, viewport| {
            ui.set_height(layout.height + header_height);
            ui.set_width(width);
            let origin = ui.max_rect().min;
            if list {
                self.list_header(
                    ui,
                    egui::Rect::from_min_size(origin, egui::vec2(width, header_height)),
                );
            }
            let origin = origin + egui::vec2(0., header_height);
            if let Some(index) = reveal {
                let row = layout.photo_row(index);
                let top = if index == 0 { -header_height } else { row.top };
                ui.scroll_to_rect(
                    egui::Rect::from_min_max(
                        origin + egui::vec2(viewport.left(), top),
                        origin + egui::vec2(viewport.right(), row.top + row.height),
                    ),
                    None,
                );
            }
            if files.is_empty() {
                ui.weak("No matching images");
            }
            let visible = layout
                .visible(
                    viewport.top() - header_height,
                    viewport.bottom() - header_height,
                )
                .iter()
                .filter_map(|row| {
                    if let folders::Content::Photos(range) = &row.content {
                        Some(range.clone())
                    } else {
                        None
                    }
                })
                .flat_map(|range| files[range].iter().cloned())
                .collect();
            self.schedule_thumbnails(visible);
            for row in layout.visible(
                viewport.top() - header_height,
                viewport.bottom() - header_height,
            ) {
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
                        if list {
                            let path = &files[range.start];
                            if self
                                .list_item(
                                    ui,
                                    path,
                                    highlighted.contains(path),
                                    selected == Some(path.as_path()),
                                    egui::vec2(width, row.height),
                                )
                                .clicked()
                                && !self.review_clicked
                            {
                                self.highlight(path, &files, ui.input(|input| input.modifiers));
                                if self.highlighted.contains(path) {
                                    chosen = Some(Selection {
                                        path: path.clone(),
                                        preserve_viewport: false,
                                    });
                                }
                            }
                        } else {
                            ui.horizontal(|ui| {
                                if icons {
                                    ui.spacing_mut().item_spacing.x = 0.;
                                }
                                for path in &files[range.clone()] {
                                    if self
                                        .tile(
                                            ui,
                                            path,
                                            highlighted.contains(path),
                                            selected == Some(path.as_path()),
                                            egui::vec2(tile_width, row.height),
                                            icons,
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
                                            chosen = Some(Selection {
                                                path: path.clone(),
                                                preserve_viewport: false,
                                            });
                                        }
                                    }
                                }
                            });
                        }
                    },
                );
            }
        });
        // Background history updates may conceal a row, but must not move the
        // viewed photo or trigger another render as each import completes.
        let advance_hidden = std::mem::take(&mut self.advance_hidden);
        if let Some(path) = chosen
            .as_ref()
            .map(|selection| selection.path.as_path())
            .or(selected)
            .filter(|path| advance_hidden && self.excluded.contains(*path))
        {
            chosen = self.next_visible(path).map(|path| Selection {
                path,
                preserve_viewport: false,
            });
            self.highlighted.clear();
            if let Some(next) = &chosen {
                self.highlighted.insert(next.path.clone());
                self.anchor = Some(next.path.clone());
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
        chosen.filter(|selection| Some(selection.path.as_path()) != selected)
    }
}
fn icon_columns(width: f32) -> usize {
    (width / GRID_REFLOW_TILE_WIDTH).floor().max(1.) as usize
}
#[cfg(test)]
#[path = "browser_test.rs"]
mod interaction_tests;

#[cfg(test)]
mod tests {
    fn prepared(color: [u8; 3], extent: [u32; 2]) -> super::PreparedPreview {
        super::PreparedPreview::new(
            foveonic::preview::Preview {
                color_space: foveonic::color_space::TiffColorSpace::Srgb,
                rgb: color
                    .map(|v| u16::from(v) * 257)
                    .repeat((extent[0] * extent[1]) as usize),
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
            kind: super::Kind::Thumbnail,
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
        for index in 0..9 {
            browser.insert(
                &context,
                format!("{index}.x3f").into(),
                prepared([128; 3], [16, 8]),
            );
        }
        assert_eq!(browser.previews.len(), super::PREVIEW_CACHE_ENTRIES);
        assert_eq!(browser.cache.len(), 9);
        assert!(!browser.previews.contains_key(std::path::Path::new("0.x3f")));
        assert!(browser.thumbnail(std::path::Path::new("0.x3f")).is_some());
    }
    #[test]
    fn sidebar_and_comparison_share_preview_retention_until_both_finish_drawing() {
        let context = eframe::egui::Context::default();
        let mut browser = super::Browser::new(context.clone());
        let sidebar = std::path::PathBuf::from("sidebar.x3f");
        let comparison = std::path::PathBuf::from("comparison.x3f");
        browser.files = vec![sidebar.clone(), comparison.clone()];
        for path in &browser.files.clone() {
            let mut image = prepared([128; 3], [16, 8]);
            image.preview = None;
            browser.insert(&context, path.clone(), image);
            browser.pending_previews.insert(path.clone());
        }

        browser.preview(&comparison);
        browser.schedule_thumbnails(vec![sidebar.clone()]);
        browser.finish_previews();
        assert!(browser.pending_previews.contains(&comparison));
        assert!(browser.pending_previews.contains(&sidebar));

        browser.schedule_thumbnails(vec![sidebar.clone()]);
        browser.finish_previews();
        assert!(!browser.pending_previews.contains(&comparison));
        assert!(browser.pending_previews.contains(&sidebar));
    }
    #[test]
    fn sidebar_preview_lookup_finishes_once_while_the_selected_view_can_retry() {
        let context = eframe::egui::Context::default();
        let mut browser = super::Browser::new(context.clone());
        let path = std::path::PathBuf::from("photo.x3f");
        browser.files.push(path.clone());
        browser.versions.insert(path.clone(), 1);
        let mut small = prepared([128; 3], [16, 8]);
        small.preview = None;
        browser.insert(&context, path.clone(), small);
        browser.pending_previews.insert(path.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        browser.receiver = rx;
        tx.send(super::Reply {
            path: path.clone(),
            version: 1,
            kind: super::Kind::Preview,
            image: Err("No cached preview".into()),
        })
        .unwrap();
        browser.receive(&context);
        browser.schedule_thumbnails(vec![path.clone()]);
        assert!(browser.pending_previews.is_empty());
        assert!(browser.preview(&path).is_some());
        assert!(browser.pending_previews.contains(&path));
    }
    #[test]
    fn a_late_preview_miss_does_not_suppress_reloading_an_evicted_thumbnail() {
        let context = eframe::egui::Context::default();
        let mut browser = super::Browser::new(context.clone());
        let path = std::path::PathBuf::from("evicted.x3f");
        browser.files.push(path.clone());
        browser.versions.insert(path.clone(), 1);
        browser.pending_previews.insert(path.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        browser.receiver = rx;
        tx.send(super::Reply {
            path: path.clone(),
            version: 1,
            kind: super::Kind::Preview,
            image: Err("No cached preview".into()),
        })
        .unwrap();
        browser.receive(&context);
        assert!(!browser.cache.contains_key(&path));
        assert!(browser.thumbnail(&path).is_none());
        assert!(browser.pending.contains(&path));
    }
    #[test]
    fn folder_pass_prepares_offscreen_thumbnails_once_and_keeps_visible_textures() {
        use std::time::{Duration, Instant};
        let folder = tempfile::tempdir().unwrap();
        let mut data = vec![0; 40];
        data[..4].copy_from_slice(b"FOVb");
        data[4..8].copy_from_slice(&0x20000_u32.to_le_bytes());
        data.extend(b"SECi");
        for word in [0x20000_u32, 2, 3, 2, 1, 6] {
            data.extend(word.to_le_bytes());
        }
        data.extend([128_u8; 6]);
        let directory = data.len() as u32;
        data.extend(b"SECd");
        for word in [0x20000_u32, 1, 40, 34] {
            data.extend(word.to_le_bytes());
        }
        data.extend(b"IMAG");
        data.extend(directory.to_le_bytes());
        let count = super::THUMBNAIL_CACHE_ENTRIES + 20;
        for index in 0..count {
            std::fs::write(folder.path().join(format!("photo-{index:04}.x3f")), &data).unwrap();
        }
        let context = eframe::egui::Context::default();
        let mut browser = super::Browser::new(context.clone());
        browser.open(folder.path()).unwrap();
        let draw = |browser: &mut super::Browser| {
            context.run(
                eframe::egui::RawInput {
                    screen_rect: Some(eframe::egui::Rect::from_min_size(
                        eframe::egui::Pos2::ZERO,
                        eframe::egui::vec2(350., 400.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    eframe::egui::CentralPanel::default().show(ctx, |ui| {
                        browser.organizer(ui, None, crate::preferences::Organizer::Icons);
                    });
                },
            )
        };
        let _ = draw(&mut browser);
        assert!(!browser.visible.is_empty());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !browser.pending.is_empty() && Instant::now() < deadline {
            browser.receive(&context);
            let _ = draw(&mut browser);
            std::thread::yield_now();
        }
        assert!(browser.pending.is_empty(), "folder warmup did not finish");
        assert_eq!(browser.versions.len(), count);
        assert_eq!(browser.order.len(), super::THUMBNAIL_CACHE_ENTRIES);
        assert!(browser
            .visible
            .iter()
            .all(|p| browser.cache.get(p).is_some_and(Option::is_some)));
        let revision = browser.sequence;
        for _ in 0..3 {
            let _ = draw(&mut browser);
        }
        assert!(browser.pending.is_empty());
        assert_eq!(
            browser.sequence, revision,
            "eviction restarted the greedy pass"
        );
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
        for (width, count) in [
            (100., 1),
            (199., 1),
            (200., 2),
            (299., 2),
            (300., 3),
            (499., 4),
            (500., 5),
        ] {
            assert_eq!(super::icon_columns(width), count);
        }
    }
}
