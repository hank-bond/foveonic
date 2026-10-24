//! Discovers descendant X3Fs and lays out folder groups for virtualized browsing.

use eframe::egui;
use std::{
    io,
    ops::Range,
    path::{Path, PathBuf},
};

/// Keep files in their own folder before visiting its sorted subdirectories.
/// Hidden metadata entries and symbolic links do not belong to the photo tree.
pub(super) fn discover(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_owned()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory).map_err(|error| {
            io::Error::new(error.kind(), format!("{}: {error}", directory.display()))
        })?;
        let mut children = Vec::new();
        let mut photographs = Vec::new();
        for entry in entries {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let kind = entry.file_type()?;
            let path = entry.path();
            if kind.is_dir() {
                children.push(path);
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("x3f"))
            {
                photographs.push(path);
            }
        }
        photographs.sort();
        children.sort();
        files.extend(photographs);
        pending.extend(children.into_iter().rev());
    }
    Ok(files)
}

pub(super) enum Content {
    Folder(PathBuf),
    Photos(Range<usize>),
}
pub(super) struct Row {
    pub content: Content,
    pub top: f32,
    pub height: f32,
}
pub(super) struct Layout {
    pub rows: Vec<Row>,
    photo_rows: Vec<usize>,
    pub height: f32,
}
impl Layout {
    pub fn new(
        files: &[PathBuf],
        root: Option<&Path>,
        columns: usize,
        photo_height: f32,
        folder_height: f32,
        gap: f32,
    ) -> Self {
        let mut result = Self {
            rows: Vec::new(),
            photo_rows: Vec::with_capacity(files.len()),
            height: 0.,
        };
        let mut start = 0;
        while start < files.len() {
            let directory = files[start].parent().unwrap_or(Path::new(""));
            let end = start
                + files[start..]
                    .iter()
                    .take_while(|file| file.parent() == Some(directory))
                    .count();
            if root.is_some_and(|root| directory != root) {
                result.push(Content::Folder(directory.to_owned()), folder_height, gap);
            }
            for first in (start..end).step_by(columns) {
                let last = (first + columns).min(end);
                result
                    .photo_rows
                    .extend(std::iter::repeat_n(result.rows.len(), last - first));
                result.push(Content::Photos(first..last), photo_height, gap);
            }
            start = end;
        }
        result.height = (result.height - gap).max(0.);
        result
    }
    fn push(&mut self, content: Content, height: f32, gap: f32) {
        self.rows.push(Row {
            content,
            top: self.height,
            height,
        });
        self.height += height + gap;
    }
    pub fn photo_row(&self, index: usize) -> &Row {
        &self.rows[self.photo_rows[index]]
    }
    pub fn visible(&self, top: f32, bottom: f32) -> &[Row] {
        let first = self.rows.partition_point(|row| row.top + row.height < top);
        let last = self.rows.partition_point(|row| row.top <= bottom);
        &self.rows[first..last]
    }
    pub fn arrow(&self, current: Option<usize>, key: egui::Key) -> Option<usize> {
        let count = self.photo_rows.len();
        if count == 0 {
            return None;
        }
        let Some(index) = current else {
            return Some(0);
        };
        let row = self.photo_rows[index];
        let Content::Photos(range) = &self.rows[row].content else {
            unreachable!()
        };
        let column = index - range.start;
        let neighboring = match key {
            egui::Key::ArrowLeft => return Some(index.saturating_sub(1)),
            egui::Key::ArrowRight => return Some((index + 1).min(count - 1)),
            egui::Key::ArrowUp => {
                self.rows[..row]
                    .iter()
                    .rev()
                    .find_map(|row| match &row.content {
                        Content::Photos(range) => Some(range),
                        _ => None,
                    })
            }
            egui::Key::ArrowDown => {
                self.rows[row + 1..]
                    .iter()
                    .find_map(|row| match &row.content {
                        Content::Photos(range) => Some(range),
                        _ => None,
                    })
            }
            _ => None,
        };
        Some(neighboring.map_or(index, |range| range.start + column.min(range.len() - 1)))
    }
}

/// Draw one divider above the folder name, then leave its images directly below.
pub(super) fn heading(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    directory: &Path,
    root: Option<&Path>,
    divider: bool,
) {
    if divider {
        ui.painter().line_segment(
            [rect.left_top(), rect.right_top()],
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
    }
    let label = root
        .and_then(|root| directory.strip_prefix(root).ok())
        .unwrap_or(directory);
    let text = egui::Rect::from_min_max(
        rect.min + egui::vec2(0., ui.spacing().item_spacing.y),
        rect.max,
    );
    ui.put(text, egui::Label::new(label.to_string_lossy()).truncate())
        .on_hover_text(directory.display().to_string());
}

#[cfg(test)]
#[path = "browser_folders_test.rs"]
mod tests;
