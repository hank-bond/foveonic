//! Keeps the app's rendered images in one collection keyed by photo, look and output.

use super::{matching::same_delivery_request, request::RenderRequest, view_plan::Rendered};
use crate::editor::Rendition;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

/// What an image shows: the photo, its look and its output.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct ImageKey {
    pub path: PathBuf,
    pub look: Rendition,
    pub output: String,
    pub comparison: bool,
    /// The crop and rotation plane shows the uncropped editing extent.
    pub framing: bool,
}
impl ImageKey {
    /// The photo's Current final image: kept while the photo is displayed or in
    /// history, and drawn while another look or output renders.
    pub fn current(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            look: Rendition::Current,
            output: "final".into(),
            comparison: false,
            framing: false,
        }
    }
}

/// A delivered image the collection can hold, ordered by its request id.
pub(crate) trait Stored: Rendered {
    fn key(&self) -> ImageKey;
    fn patch(&self) -> bool;
}

/// One key's whole image and patch.
pub(crate) struct Images<T> {
    pub whole: Option<T>,
    pub patch: Option<T>,
}
impl<T> Default for Images<T> {
    fn default() -> Self {
        Self {
            whole: None,
            patch: None,
        }
    }
}

pub(crate) struct RenderedImages<T> {
    entries: HashMap<ImageKey, Images<T>>,
    /// Requests whose render failed. Rendering one again would fail the same way,
    /// so it counts as done until the request changes.
    failed: Vec<Arc<RenderRequest>>,
}
impl<T> Default for RenderedImages<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            failed: Vec::new(),
        }
    }
}
impl<T: Stored> RenderedImages<T> {
    pub fn get(&self, key: &ImageKey) -> Option<&Images<T>> {
        self.entries.get(key)
    }

    /// The key's whole image or, until it has one, the photo's Current image.
    pub fn whole(&self, key: &ImageKey) -> Option<&T> {
        let whole = |key: &ImageKey| self.entries.get(key)?.whole.as_ref();
        whole(key).or_else(|| whole(&ImageKey::current(&key.path)))
    }

    /// Store a delivered image unless the stored image of the same kind is newer.
    pub fn accept(&mut self, image: T) -> bool {
        let entry = self.entries.entry(image.key()).or_default();
        let slot = if image.patch() {
            &mut entry.patch
        } else {
            &mut entry.whole
        };
        if slot
            .as_ref()
            .is_some_and(|old| old.request_id() > image.request_id())
        {
            return false;
        }
        *slot = Some(image);
        true
    }

    pub fn fail(&mut self, request: Arc<RenderRequest>) {
        if !self.failed(&request) {
            self.failed.push(request);
        }
    }

    /// Whether this request, with any id, already failed.
    pub fn failed(&self, request: &RenderRequest) -> bool {
        self.failed
            .iter()
            .any(|failed| same_delivery_request(failed, request))
    }

    /// Drop every image and failure whose key `keep` rejects.
    pub fn retain(&mut self, keep: impl Fn(&ImageKey) -> bool) {
        self.entries.retain(|key, _| keep(key));
        self.failed.retain(|request| keep(&request_key(request)));
    }

    pub fn images(&self) -> impl Iterator<Item = &T> {
        self.entries
            .values()
            .flat_map(|images| images.whole.iter().chain(images.patch.iter()))
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.failed.clear();
    }
}

fn request_key(request: &RenderRequest) -> ImageKey {
    ImageKey {
        path: request.path.clone(),
        look: request.rendition,
        output: request.output.clone(),
        comparison: request.comparison,
        framing: request.framing_preview,
    }
}

impl Stored for super::worker::RenderResult {
    /// Filed under the request it answers.
    fn key(&self) -> ImageKey {
        request_key(&self.request)
    }
    fn patch(&self) -> bool {
        self.request.detail
    }
}

#[cfg(test)]
#[path = "rendered_test.rs"]
mod tests;
