//! Owns bounded photo settings and images with independent editing selection and render demand.

use crate::{multi_edit_controls, sidecars};
use foveonic::{render::Settings, source::Metadata};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Default)]
struct Photo {
    settings: Option<Settings>,
    metadata: Option<Arc<Metadata>>,
    numeric_settings: Option<Settings>,
    reset_settings: Option<Settings>,
    defaults_basis: Option<Settings>,
    error: Option<String>,
}

#[derive(Default)]
pub struct Runtime {
    editing_paths: Vec<PathBuf>,
    render_paths: Vec<PathBuf>,
    // Several settings and frame snapshots make each photo large. Keep map
    // insertion and balancing from moving those records through the stack.
    photos: BTreeMap<PathBuf, Box<Photo>>,
    recent: Vec<PathBuf>,
    retention_changed: bool,
    viewed: Option<Viewed>,
}
struct Viewed {
    path: PathBuf,
    /// A thumbnail-only inspection must not silently become ordinary render demand.
    transient: bool,
}
impl Runtime {
    fn targets(&self) -> impl Iterator<Item = &Photo> {
        self.editing_paths
            .iter()
            .map(|path| self.photos[path].as_ref())
    }
    pub fn selected(&mut self, path: &Path) {
        if self.photos.contains_key(path) {
            self.recent.retain(|old| old != path);
            self.recent.push(path.to_owned());
        }
    }
    pub fn retained_paths(&self) -> Vec<PathBuf> {
        self.photos.keys().cloned().collect()
    }
    /// Inspect one displayed photograph without changing the editing selection.
    pub fn view_detail(&mut self, path: Option<&Path>, sidecars: &mut sidecars::Session) {
        if self.viewed.as_ref().map(|view| view.path.as_path()) == path {
            return;
        }
        if let Some(previous) = self.viewed.take() {
            if previous.transient && !self.editing_paths.contains(&previous.path) {
                self.photos.remove(&previous.path);
                self.recent.retain(|path| path != &previous.path);
            }
        }
        if let Some(path) = path {
            let transient = !self.photos.contains_key(path);
            let photo = self.photos.entry(path.to_owned()).or_default();
            if photo.settings.is_none() {
                sidecars.resolve_settings(&[path.to_owned()]);
            }
            self.viewed = Some(Viewed {
                path: path.to_owned(),
                transient,
            });
            self.selected(path);
        }
        self.retention_changed = true;
    }
    pub fn sync(
        &mut self,
        paths: &[PathBuf],
        limit: usize,
        sidecars: &mut sidecars::Session,
    ) -> bool {
        if let Some(view) = &mut self.viewed {
            if paths.contains(&view.path) {
                view.transient = false;
            }
        }
        let limit = limit.max(paths.len())
            + usize::from(
                self.viewed
                    .as_ref()
                    .is_some_and(|view| !paths.contains(&view.path)),
            );
        if self.editing_paths == paths && self.photos.len() <= limit {
            return false;
        }
        self.retention_changed = true;
        self.editing_paths = paths.to_vec();
        for path in paths {
            self.photos.entry(path.clone()).or_default();
            if !self.recent.contains(path) {
                self.recent.push(path.clone());
            }
        }
        while self.photos.len() > limit {
            let Some(index) = self.recent.iter().position(|path| {
                !paths.contains(path) && self.viewed.as_ref().is_none_or(|view| &view.path != path)
            }) else {
                break;
            };
            let path = self.recent.remove(index);
            self.photos.remove(&path);
            self.render_paths.retain(|render| render != &path);
        }
        let missing = paths
            .iter()
            .filter(|path| self.photos[*path].settings.is_none())
            .cloned()
            .collect::<Vec<_>>();
        sidecars.resolve_settings(&missing);
        true
    }
    pub fn resolve(
        &mut self,
        replies: Vec<sidecars::Resolved>,
        overrides: &[String; 2],
    ) -> Vec<String> {
        let mut errors = Vec::new();
        for reply in replies {
            let Some(photo) = self.photos.get_mut(&reply.path) else {
                continue;
            };
            if photo.settings.is_some() {
                continue;
            }
            match reply.settings {
                Ok(settings) => {
                    photo.settings = Some(sidecars::selection_settings(settings, overrides));
                }
                Err(error) => {
                    photo.error = Some(error.clone());
                    errors.push(format!("{}: {error}", reply.path.display()));
                }
            }
        }
        errors
    }
    pub fn observe_active(
        &mut self,
        path: &Path,
        settings: &Settings,
        metadata: Option<&Arc<Metadata>>,
    ) {
        if let Some(photo) = self.photos.get_mut(path) {
            if photo.settings.as_ref() != Some(settings) {
                photo.settings = Some(settings.clone());
            }
            // Changed settings may render where the last attempt failed.
            photo.error = None;
            if let Some(metadata) = metadata {
                photo.metadata = Some(metadata.clone());
            }
        }
    }
    pub fn observe_requested(&mut self, path: &Path, settings: &Settings) {
        if let Some(photo) = self.photos.get_mut(path) {
            if photo.settings.as_ref() != Some(settings) {
                photo.settings = Some(settings.clone());
            }
        }
    }
    pub fn multiple(&self) -> bool {
        self.editing_paths.len() > 1
    }
    pub fn ready(&self) -> bool {
        self.targets()
            .all(|photo| photo.settings.is_some() && photo.error.is_none())
    }
    pub fn editable_ready(&self) -> bool {
        self.ready() && self.targets().all(|photo| photo.numeric_settings.is_some())
    }
    pub fn snapshots(&self) -> Vec<Settings> {
        if !self.multiple() || !self.ready() {
            return Vec::new();
        }
        self.editing_paths
            .iter()
            .map(|path| self.photos[path].settings.as_ref().unwrap().clone())
            .collect()
    }
    pub fn numeric_snapshots(&self) -> Vec<Settings> {
        if !self.multiple() || self.targets().any(|photo| photo.numeric_settings.is_none()) {
            return Vec::new();
        }
        self.editing_paths
            .iter()
            .map(|path| {
                let photo = &self.photos[path];
                let mut numeric = photo.settings.as_ref().unwrap().clone();
                let defaults = photo
                    .reset_settings
                    .as_ref()
                    .unwrap_or_else(|| photo.numeric_settings.as_ref().unwrap());
                numeric.reconstruction = numeric.reconstruction.or(defaults.reconstruction);
                numeric.sharpen_amount = numeric.sharpen_amount.or(defaults.sharpen_amount);
                numeric.color_dq_gain = numeric.color_dq_gain.or(defaults.color_dq_gain);
                numeric.highlight_threshold_ev = numeric
                    .highlight_threshold_ev
                    .or(defaults.highlight_threshold_ev);
                numeric.highlight_amount = numeric.highlight_amount.or(defaults.highlight_amount);
                numeric.monochrome_smoothness = numeric
                    .monochrome_smoothness
                    .or(defaults.monochrome_smoothness);
                numeric.dehaze.shadow = numeric.dehaze.shadow.or(defaults.dehaze.shadow);
                if photo.settings.as_ref().unwrap().sharpen_amount.is_none()
                    && !default_ready(photo, "/sharpen_amount")
                {
                    numeric.sharpen_amount = None;
                }
                if photo.settings.as_ref().unwrap().color_dq_gain.is_none()
                    && !default_ready(photo, "/color_dq_gain")
                {
                    numeric.color_dq_gain = None;
                }
                if photo.settings.as_ref().unwrap().dehaze.shadow.is_none()
                    && !default_ready(photo, "/dehaze/shadow")
                {
                    numeric.dehaze.shadow = None;
                }
                if numeric.exposure_auto {
                    if let Some(metadata) = &photo.metadata {
                        let capture =
                            foveonic::source::CaptureExposureCompensation::from_properties(
                                &metadata.properties,
                            );
                        numeric.exposure = numeric
                            .exposure_auto_compensation
                            .inverse(capture)
                            .unwrap_or(0.);
                    }
                }
                numeric
            })
            .collect()
    }
    pub fn settings(&self, path: &Path) -> Option<&Settings> {
        self.photos.get(path)?.settings.as_ref()
    }
    pub fn optional_readiness(&self) -> [(&'static str, bool); 3] {
        ["/sharpen_amount", "/color_dq_gain", "/dehaze/shadow"].map(|pointer| {
            (
                pointer,
                self.targets().all(|photo| default_ready(photo, pointer)),
            )
        })
    }
    pub fn metadata(&self, path: &Path) -> Option<Arc<Metadata>> {
        self.photos.get(path)?.metadata.clone()
    }
    pub fn metadata_snapshots(&self) -> Vec<Arc<Metadata>> {
        self.editing_paths
            .iter()
            .filter_map(|path| self.metadata(path))
            .collect()
    }
    pub fn control_ranges(&self) -> Vec<(String, [f64; 2])> {
        let mut ranges = BTreeMap::<String, [f64; 2]>::new();
        for photo in self.targets() {
            let (Some(settings), Some(metadata)) = (&photo.settings, &photo.metadata) else {
                continue;
            };
            let pointers = [
                "/exposure".to_owned(),
                "/sharpen_amount".to_owned(),
                "/merrill_speckles/threshold".to_owned(),
            ]
            .into_iter()
            .chain(["rows", "columns"].into_iter().flat_map(|axis| {
                (0..2).map(move |index| format!("/merrill_banding/{axis}/bands/{index}/size"))
            }));
            let mut edits = pointers
                .map(|pointer| multi_edit_controls::Edit::Number {
                    pointer,
                    change: multi_edit_controls::NumberChange::Relative(0.),
                    range: [0., 0.],
                    mapping: Default::default(),
                })
                .collect::<Vec<_>>();
            per_photo_ranges(
                &mut edits,
                settings,
                photo.numeric_settings.as_ref(),
                metadata,
            );
            for edit in edits {
                let multi_edit_controls::Edit::Number { pointer, range, .. } = edit else {
                    unreachable!()
                };
                ranges
                    .entry(pointer)
                    .and_modify(|union| {
                        union[0] = union[0].min(range[0]);
                        union[1] = union[1].max(range[1]);
                    })
                    .or_insert(range);
            }
        }
        ranges.into_iter().collect()
    }
    /// Save all edits and report whether any changed rendering.
    pub fn apply_edits(
        &mut self,
        edits: &[multi_edit_controls::Edit],
        sidecars: &mut sidecars::Session,
    ) -> bool {
        if !self.multiple() || !self.editable_ready() || edits.is_empty() {
            return false;
        }
        let applicable_edits = edits
            .iter()
            .filter(|edit| match edit {
                multi_edit_controls::Edit::Optional {
                    pointer,
                    enabled: true,
                } => self.targets().all(|photo| default_ready(photo, pointer)),
                multi_edit_controls::Edit::Number { pointer, .. }
                    if matches!(pointer.as_str(), "/sharpen_amount" | "/color_dq_gain") =>
                {
                    self.targets().all(|photo| {
                        let settings = photo.settings.as_ref().unwrap();
                        let manual = if pointer == "/sharpen_amount" {
                            settings.sharpen_amount.is_some()
                        } else {
                            settings.color_dq_gain.is_some()
                        };
                        manual || default_ready(photo, pointer)
                    })
                }
                _ => true,
            })
            .cloned()
            .collect::<Vec<_>>();
        let edits = &applicable_edits;
        if edits.is_empty() {
            return false;
        }
        let mut changed = false;
        for path in &self.editing_paths {
            let photo = self.photos.get_mut(path).unwrap();
            let settings = photo.settings.as_mut().unwrap();
            let before = settings.clone();
            if let Some(numeric) = photo
                .reset_settings
                .as_ref()
                .or(photo.numeric_settings.as_ref())
            {
                materialize_defaults(settings, numeric, edits);
                if settings.exposure_auto && edits.iter().any(|edit| matches!(edit, multi_edit_controls::Edit::Number { pointer, .. } if pointer == "/exposure")) {
                    settings.exposure = photo.metadata.as_deref().map_or(numeric.exposure, |metadata| {
                        let capture = foveonic::source::CaptureExposureCompensation::from_properties(&metadata.properties);
                        settings.exposure_auto_compensation.inverse(capture).unwrap_or(0.)
                    });
                    settings.exposure_auto = false;
                }
            }
            let mut target_edits = resolve_optionals(
                settings,
                photo
                    .reset_settings
                    .as_ref()
                    .or(photo.numeric_settings.as_ref()),
                edits,
            );
            apply_reset_defaults(&mut target_edits, photo.reset_settings.as_ref());
            if let Some(metadata) = &photo.metadata {
                per_photo_ranges(
                    &mut target_edits,
                    settings,
                    photo.numeric_settings.as_ref(),
                    metadata,
                );
            }
            multi_edit_controls::apply(settings, &target_edits);
            if let Some(metadata) = &photo.metadata {
                crate::controls::normalize_with_capture(settings, Some(metadata), None);
            }
            if *settings != before {
                sidecars.apply(path.clone(), settings.clone());
                changed |= !crate::rendering::matching::same_render_settings(&before, settings);
            }
        }
        changed
    }
    /// Editing admits photos into the bounded owner. Display alone may keep
    /// those photos rendering, but does not admit thumbnail-only photographs.
    pub fn sync_render_paths(&mut self, displayed: &[PathBuf]) -> bool {
        let mut paths = self.editing_paths.clone();
        for path in displayed {
            if self.photos.contains_key(path)
                && !self
                    .viewed
                    .as_ref()
                    .is_some_and(|view| view.transient && &view.path == path)
                && !paths.contains(path)
            {
                paths.push(path.clone());
            }
        }
        let changed = std::mem::take(&mut self.retention_changed) || paths != self.render_paths;
        self.render_paths = paths;
        changed
    }
    pub fn render_paths(&self) -> &[PathBuf] {
        &self.render_paths
    }
    /// The photo's settings could not be resolved from its sidecar.
    pub fn failed(&self, path: &Path) -> bool {
        self.photos
            .get(path)
            .is_some_and(|photo| photo.error.is_some())
    }
    pub fn rendering(&self, path: &Path) -> bool {
        self.render_paths.iter().any(|candidate| candidate == path)
    }
    pub fn complete(
        &mut self,
        path: &Path,
        metadata: Option<Arc<Metadata>>,
        numeric_settings: Option<Settings>,
        reset_settings: Option<Settings>,
        basis: &Settings,
    ) {
        if let Some(photo) = self.photos.get_mut(path) {
            if let Some(metadata) = metadata {
                photo.metadata = Some(metadata);
            }
            if let Some(numeric_settings) = numeric_settings {
                photo.numeric_settings = Some(numeric_settings);
            }
            if let Some(reset_settings) = reset_settings {
                photo.reset_settings = Some(reset_settings);
                photo.defaults_basis = Some(basis.clone());
            }
        }
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

fn default_ready(photo: &Photo, pointer: &str) -> bool {
    photo.settings.as_ref().is_some_and(|settings| {
        default_ready_values(
            settings,
            photo.defaults_basis.as_ref(),
            photo.metadata.as_deref(),
            pointer,
        )
    })
}
fn default_ready_values(
    settings: &Settings,
    basis: Option<&Settings>,
    metadata: Option<&Metadata>,
    pointer: &str,
) -> bool {
    let Some(basis) = basis else {
        return false;
    };
    let Some(metadata) = metadata else {
        return true;
    };
    let mut current = settings.clone();
    crate::export::resolve_camera_settings(&mut current, metadata);
    match pointer {
        "/sharpen_amount" => current.automatic_sharpening_matches(basis, metadata),
        "/color_dq_gain" => current.automatic_color_dq_matches(basis),
        "/dehaze/shadow" => {
            basis.dehaze.shadow.is_none()
                && crate::editor::dehaze::estimate_settings(&current)
                    == crate::editor::dehaze::estimate_settings(basis)
        }
        _ => true,
    }
}

fn materialize_defaults(
    settings: &mut Settings,
    numeric: &Settings,
    edits: &[multi_edit_controls::Edit],
) {
    for edit in edits {
        let pointer = edit.pointer();
        if pointer.starts_with("/reconstruction/") && settings.reconstruction.is_none() {
            settings.reconstruction = numeric.reconstruction;
        }
        if pointer.starts_with("/dehaze/shadow/") && settings.dehaze.shadow.is_none() {
            settings.dehaze.shadow = numeric.dehaze.shadow;
        }
        if !matches!(edit, multi_edit_controls::Edit::Number { .. }) {
            continue;
        }
        match pointer {
            "/sharpen_amount" if settings.sharpen_amount.is_none() => {
                settings.sharpen_amount = numeric.sharpen_amount
            }
            "/color_dq_gain" if settings.color_dq_gain.is_none() => {
                settings.color_dq_gain = numeric.color_dq_gain
            }
            "/highlight_threshold_ev" if settings.highlight_threshold_ev.is_none() => {
                settings.highlight_threshold_ev = numeric.highlight_threshold_ev
            }
            "/highlight_amount" if settings.highlight_amount.is_none() => {
                settings.highlight_amount = numeric.highlight_amount
            }
            "/monochrome_smoothness" if settings.monochrome_smoothness.is_none() => {
                settings.monochrome_smoothness = numeric.monochrome_smoothness
            }
            _ => {}
        }
    }
}

fn apply_reset_defaults(edits: &mut [multi_edit_controls::Edit], defaults: Option<&Settings>) {
    let Some(defaults) = defaults.and_then(|settings| serde_json::to_value(settings).ok()) else {
        return;
    };
    for edit in edits {
        let multi_edit_controls::Edit::Number {
            pointer,
            change: multi_edit_controls::NumberChange::Reset(value),
            ..
        } = edit
        else {
            continue;
        };
        if pointer.starts_with("/reconstruction/")
            || matches!(
                pointer.as_str(),
                "/monochrome_smoothness" | "/highlight_amount"
            )
        {
            if let Some(default) = defaults
                .pointer(pointer)
                .and_then(serde_json::Value::as_f64)
            {
                *value = default;
            }
        }
    }
}

fn resolve_optionals(
    settings: &Settings,
    numeric: Option<&Settings>,
    edits: &[multi_edit_controls::Edit],
) -> Vec<multi_edit_controls::Edit> {
    let authored = serde_json::to_value(settings).unwrap_or_default();
    let numeric = numeric.and_then(|settings| serde_json::to_value(settings).ok());
    edits
        .iter()
        .filter_map(|edit| {
            if let multi_edit_controls::Edit::Optional {
                pointer,
                enabled: true,
            } = edit
            {
                if !authored
                    .pointer(pointer)
                    .is_some_and(serde_json::Value::is_null)
                {
                    return None;
                }
                let value = numeric.as_ref()?.pointer(pointer)?.clone();
                if value.is_null() {
                    return None;
                }
                Some(multi_edit_controls::Edit::Assign {
                    pointer: pointer.clone(),
                    value,
                })
            } else {
                Some(edit.clone())
            }
        })
        .collect()
}

fn per_photo_ranges(
    edits: &mut [multi_edit_controls::Edit],
    settings: &Settings,
    numeric: Option<&Settings>,
    metadata: &Metadata,
) {
    let Ok(capabilities) = foveonic::render::ControlCapabilities::from_metadata(metadata) else {
        return;
    };
    for edit in edits {
        let multi_edit_controls::Edit::Number { pointer, range, .. } = edit else {
            continue;
        };
        if pointer == "/exposure" {
            let mut bounds = capabilities.exposure_range();
            let capture = foveonic::source::CaptureExposureCompensation::from_properties(
                &metadata.properties,
            );
            if let Some(calculated) = settings.exposure_auto_compensation.inverse(capture) {
                bounds[0] = bounds[0].min(calculated);
                bounds[1] = bounds[1].max(calculated);
            }
            *range = bounds.map(f64::from);
        } else if pointer == "/sharpen_amount" {
            let mut bounds = capabilities.reference_sharpening_range();
            if let Some(amount) = numeric.and_then(|settings| settings.sharpen_amount) {
                bounds[1] = bounds[1].max(amount);
            }
            *range = bounds.map(f64::from);
        } else if pointer == "/merrill_speckles/threshold" {
            let bounds = settings.merrill_speckles.threshold_range();
            *range = [f64::from(*bounds.start()), f64::from(*bounds.end())];
        } else if let Some(path) = pointer.strip_prefix("/merrill_banding/") {
            let parts = path.split('/').collect::<Vec<_>>();
            if let [axis, "bands", index, "size"] = parts.as_slice() {
                let controls = match *axis {
                    "rows" => &settings.merrill_banding.rows,
                    "columns" => &settings.merrill_banding.columns,
                    _ => continue,
                };
                if let Ok(index) = index.parse::<usize>() {
                    if index < controls.bands.len() {
                        let bounds = controls.size_range(index);
                        *range = [f64::from(*bounds.start()), f64::from(*bounds.end())];
                    }
                }
            }
        }
    }
}

impl crate::App {
    pub(crate) fn sync_multi_edit(&mut self) {
        let _scope = foveonic::diagnostics::scope(
            foveonic::diagnostics::Category::Coordinator,
            "ui.multi_sync",
        );
        if self.multi_edit.sync(
            self.edit_selection.paths(),
            self.preferences.multi_edit_limit,
            &mut self.sidecars,
        ) {
            if let Some(path) = self
                .selected
                .as_deref()
                .filter(|path| self.edit_selection.contains(path))
            {
                self.multi_edit.selected(path);
            }
        }
        for path in self.multi_edit.retained_paths() {
            if self.selected.as_ref() != Some(&path) {
                if let Some(settings) = self.sidecars.requested_settings(&path) {
                    self.multi_edit.observe_requested(&path, settings);
                }
            }
        }
        if self.sidecars.ready() {
            if let Some(path) = &self.selected {
                self.multi_edit
                    .observe_active(path, &self.settings, self.metadata.as_ref());
            }
        }
        for error in self
            .multi_edit
            .resolve(self.sidecars.take_resolved(), &self.camera_overrides)
        {
            self.fail(format!("Cannot load editing target: {error}"));
        }
    }

    pub(crate) fn sync_render_demand(&mut self) {
        let displayed = self.shown_photos();
        if self.multi_edit.sync_render_paths(&displayed) {
            self.sync_render_progress(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use multi_edit_controls::{Edit, Mapping, NumberChange};
    fn session() -> sidecars::Session {
        let mut session = sidecars::Session::new(
            eframe::egui::Context::default(),
            false,
            Arc::new(foveonic::decode_cache::Cache::new()),
        );
        session.set_memory_only(true);
        session
    }
    fn number(pointer: &str, delta: f64, range: [f64; 2]) -> Edit {
        Edit::Number {
            pointer: pointer.into(),
            change: NumberChange::Relative(delta),
            range,
            mapping: Mapping::Identity,
        }
    }
    #[test]
    fn thumbnail_inspection_is_bounded_detail_demand_without_ordinary_or_edit_admission() {
        let mut runtime = runtime();
        let original = runtime.editing_paths.clone();
        let mut sidecars = session();
        let inspected = PathBuf::from("thumbnail.x3f");
        runtime.view_detail(Some(&inspected), &mut sidecars);
        runtime.sync(&original, 2, &mut sidecars);
        let mut displayed = original.clone();
        displayed.push(inspected.clone());
        runtime.sync_render_paths(&displayed);
        assert_eq!(runtime.editing_paths, original);
        assert_eq!(runtime.render_paths(), original);
        assert_eq!(runtime.retained_paths().len(), 3);
        runtime.view_detail(None, &mut sidecars);
        runtime.sync_render_paths(&displayed);
        assert!(!runtime.retained_paths().contains(&inspected));
    }

    #[test]
    fn inspection_retains_existing_history_and_can_become_an_editing_target() {
        let mut runtime = runtime();
        let mut sidecars = session();
        let a = PathBuf::from("a.x3f");
        let b = PathBuf::from("b.x3f");
        runtime.view_detail(Some(&b), &mut sidecars);
        runtime.sync(std::slice::from_ref(&a), 1, &mut sidecars);
        assert!(runtime.retained_paths().contains(&b));
        runtime.view_detail(None, &mut sidecars);
        assert!(runtime.retained_paths().contains(&b));
        runtime.sync(std::slice::from_ref(&a), 1, &mut sidecars);
        assert!(!runtime.retained_paths().contains(&b));
        runtime.view_detail(Some(&b), &mut sidecars);
        runtime.sync(std::slice::from_ref(&b), 1, &mut sidecars);
        runtime.view_detail(None, &mut sidecars);
        assert!(runtime.retained_paths().contains(&b));
        assert_eq!(runtime.editing_paths, vec![b]);
    }
    fn runtime() -> Runtime {
        let paths = vec![PathBuf::from("a.x3f"), PathBuf::from("b.x3f")];
        let mut runtime = Runtime {
            editing_paths: paths.clone(),
            render_paths: paths.clone(),
            recent: paths.clone(),
            ..Default::default()
        };
        for (index, path) in paths.into_iter().enumerate() {
            let mut settings =
                Settings::new(if index == 0 { "Sunlight" } else { "Shade" }, "Standard");
            settings.exposure = if index == 0 { 7.8 } else { 1.2 };
            let mut numeric = settings.clone();
            numeric.sharpen_amount = Some(if index == 0 { 0.2 } else { 0.7 });
            numeric.reconstruction = Some(foveonic::noise::Controls {
                noise_enabled: false,
                detail_enabled: false,
                chroma: if index == 0 { 1 } else { 3 },
                ..Default::default()
            });
            runtime.photos.insert(
                path,
                Box::new(Photo {
                    defaults_basis: Some(settings.clone()),
                    settings: Some(settings),
                    numeric_settings: Some(numeric),
                    ..Default::default()
                }),
            );
        }
        runtime
    }

    #[test]
    fn range_names_are_retained_without_changing_render_demand() {
        let mut runtime = runtime();
        let paths = runtime.editing_paths.clone();
        for path in &paths {
            runtime
                .photos
                .get_mut(path)
                .unwrap()
                .settings
                .as_mut()
                .unwrap()
                .photographic
                .color_editor
                .advanced
                .push(Default::default());
        }
        let mut renamed = runtime.settings(&paths[0]).unwrap().clone();
        renamed.photographic.color_editor.advanced.ranges[0].name = "Sky".try_into().unwrap();
        runtime.observe_active(&paths[0], &renamed, None);
        assert_eq!(runtime.settings(&paths[0]), Some(&renamed));

        renamed.photographic.color_editor.advanced.ranges[0].name = "Clouds".try_into().unwrap();
        runtime.observe_requested(&paths[0], &renamed);
        assert_eq!(runtime.settings(&paths[0]), Some(&renamed));

        renamed.photographic.color_editor.advanced.ranges[0].name = "Blue".try_into().unwrap();
        runtime.observe_active(&paths[0], &renamed, None);
        renamed.color_range_preview = Some(0);
        runtime.observe_requested(&paths[0], &renamed);
    }

    #[test]
    fn multi_photo_range_names_are_saved_without_rendering() {
        let mut runtime = runtime();
        let paths = runtime.editing_paths.clone();
        for path in &paths {
            runtime
                .photos
                .get_mut(path)
                .unwrap()
                .settings
                .as_mut()
                .unwrap()
                .photographic
                .color_editor
                .advanced
                .push(Default::default());
        }
        let mut sidecars = session();
        let rename = Edit::Assign {
            pointer: "/photographic/color_editor/advanced/0/name".into(),
            value: serde_json::json!("Petals"),
        };
        assert!(!runtime.apply_edits(&[rename], &mut sidecars));
        for path in &paths {
            let settings = runtime.settings(path).unwrap();
            assert_eq!(
                settings.photographic.color_editor.advanced.ranges[0]
                    .name
                    .as_str(),
                "Petals"
            );
            assert_eq!(sidecars.requested_settings(path), Some(settings));
        }
        assert!(runtime.apply_edits(&[number("/exposure", 0.1, [-8., 8.])], &mut sidecars));
    }

    #[test]
    fn authoritative_active_recipe_recovers_a_failed_background_resolution() {
        let mut runtime = runtime();
        let path = runtime.editing_paths[0].clone();
        runtime.photos.get_mut(&path).unwrap().settings = None;
        runtime.resolve(
            vec![sidecars::Resolved {
                path: path.clone(),
                settings: Err("old recipe failure".into()),
            }],
            &[String::new(), String::new()],
        );
        assert!(runtime.failed(&path));
        let restored = Settings::new("", "");
        runtime.observe_active(&path, &restored, None);
        assert!(runtime.photos[&path].error.is_none());
        assert!(!runtime.failed(&path));
        assert_eq!(runtime.settings(&path), Some(&restored));
    }

    #[test]
    fn subset_switches_retain_recent_photos_and_evict_only_inactive_lru() {
        let mut runtime = runtime();
        let mut sidecars = session();
        let a = PathBuf::from("a.x3f");
        let b = PathBuf::from("b.x3f");
        let c = PathBuf::from("c.x3f");
        let original = runtime.settings(&a).unwrap().clone();
        runtime.sync(std::slice::from_ref(&b), 2, &mut sidecars);
        runtime.selected(&b);
        runtime.sync(std::slice::from_ref(&a), 2, &mut sidecars);
        runtime.selected(&a);
        assert_eq!(runtime.settings(&a), Some(&original));
        assert!(runtime.settings(&b).is_some());
        assert_eq!(runtime.photos.len(), 2);

        runtime.sync(std::slice::from_ref(&c), 2, &mut sidecars);
        assert!(
            runtime.settings(&a).is_some(),
            "A was selected most recently"
        );
        assert!(!runtime.photos.contains_key(&b));
        assert!(
            runtime.photos.contains_key(&c),
            "current targets are protected"
        );
        assert_eq!(runtime.photos.len(), 2);
    }

    #[test]
    fn inactive_retained_photos_do_not_block_or_receive_edits() {
        let mut runtime = runtime();
        let mut sidecars = session();
        let inactive = PathBuf::from("inactive.x3f");
        runtime.photos.insert(
            inactive.clone(),
            Box::new(Photo {
                error: Some("old load failed".into()),
                ..Default::default()
            }),
        );
        assert!(runtime.editable_ready());
        assert_eq!(runtime.snapshots().len(), 2);
        assert_eq!(runtime.numeric_snapshots().len(), 2);
        assert!(runtime.apply_edits(&[number("/exposure", 0.1, [-8., 8.])], &mut sidecars));
        assert!(runtime.photos[&inactive].settings.is_none());
    }

    #[test]
    fn display_does_not_admit_sources_or_promote_retention_and_eviction_removes_demand() {
        let mut runtime = runtime();
        let mut sidecars = session();
        let paths = runtime.editing_paths.clone();
        let unknown = PathBuf::from("unknown.x3f");
        let mut displayed = paths.clone();
        displayed.push(unknown.clone());
        runtime.sync_render_paths(&displayed);
        let recency = runtime.recent.clone();
        assert!(!runtime.sync_render_paths(&displayed));
        assert_eq!(runtime.recent, recency);
        assert!(!runtime.rendering(&unknown));
        assert_eq!(runtime.photos.len(), 2);
        runtime.sync(std::slice::from_ref(&unknown), 2, &mut sidecars);
        assert!(
            !runtime.rendering(&paths[0]),
            "evicted photos leave demand immediately"
        );
        runtime.sync_render_paths(&displayed);
        assert_eq!(runtime.render_paths().len(), 2);
        assert!(!runtime.photos.contains_key(&paths[0]));
        assert!(!runtime.sync_render_paths(&displayed));
    }

    #[test]
    fn relative_edits_clamp_independently_and_preserve_unrelated_authored_values() {
        let mut runtime = runtime();
        let mut session = session();
        assert!(runtime.apply_edits(&[number("/exposure", 0.5, [-8., 8.])], &mut session));
        assert_eq!(runtime.settings(Path::new("a.x3f")).unwrap().exposure, 8.);
        assert!((runtime.settings(Path::new("b.x3f")).unwrap().exposure - 1.7).abs() < 1e-6);
        for (path, balance) in [("a.x3f", "Sunlight"), ("b.x3f", "Shade")] {
            let settings = runtime.settings(Path::new(path)).unwrap();
            assert_eq!(settings.white_balance, balance);
            assert!(settings.sharpen_amount.is_none());
            assert!(settings.reconstruction.is_none());
            assert_eq!(session.requested_settings(Path::new(path)), Some(settings));
        }
    }
    #[test]
    fn optional_numbers_materialize_each_photos_effective_value_only_when_edited() {
        let mut runtime = runtime();
        let mut session = session();
        assert!(runtime.apply_edits(&[number("/sharpen_amount", 0.1, [0., 3.])], &mut session));
        assert!(
            (runtime
                .settings(Path::new("a.x3f"))
                .unwrap()
                .sharpen_amount
                .unwrap()
                - 0.3)
                .abs()
                < 1e-6
        );
        assert!(
            (runtime
                .settings(Path::new("b.x3f"))
                .unwrap()
                .sharpen_amount
                .unwrap()
                - 0.8)
                .abs()
                < 1e-6
        );
        assert!(runtime.apply_edits(
            &[number("/reconstruction/chroma", 1., [0., 4.])],
            &mut session
        ));
        assert_eq!(
            runtime
                .settings(Path::new("a.x3f"))
                .unwrap()
                .reconstruction
                .unwrap()
                .chroma,
            2
        );
        assert_eq!(
            runtime
                .settings(Path::new("b.x3f"))
                .unwrap()
                .reconstruction
                .unwrap()
                .chroma,
            4
        );
        let numeric = runtime.numeric_snapshots();
        assert_eq!(numeric[0].reconstruction.unwrap().chroma, 2);
        assert_eq!(numeric[1].reconstruction.unwrap().chroma, 4);
    }
    #[test]
    fn reconstruction_toggle_leaf_uses_each_photos_own_defaults() {
        let mut runtime = runtime();
        let mut session = session();
        assert!(runtime.apply_edits(
            &[Edit::Assign {
                pointer: "/reconstruction/noise_enabled".into(),
                value: serde_json::Value::Bool(true)
            }],
            &mut session
        ));
        for (path, chroma) in [("a.x3f", 1), ("b.x3f", 3)] {
            let controls = runtime
                .settings(Path::new(path))
                .unwrap()
                .reconstruction
                .unwrap();
            assert!(controls.noise_enabled);
            assert!(!controls.detail_enabled);
            assert_eq!(controls.chroma, chroma);
        }
    }

    #[test]
    fn optional_controls_use_each_photos_defaults_and_automatic_shadow_estimate() {
        let mut runtime = runtime();
        for (path, color) in [("a.x3f", [0.1, 0.2, 0.3]), ("b.x3f", [0.6, 0.5, 0.4])] {
            let photo = runtime.photos.get_mut(Path::new(path)).unwrap();
            photo.settings.as_mut().unwrap().dehaze.shadow = None;
            photo.numeric_settings.as_mut().unwrap().dehaze.shadow = Some(color);
        }
        let edits = [
            Edit::Optional {
                pointer: "/sharpen_amount".into(),
                enabled: true,
            },
            Edit::Optional {
                pointer: "/dehaze/shadow".into(),
                enabled: true,
            },
        ];
        assert!(runtime.apply_edits(&edits, &mut session()));
        for (path, amount, color) in [
            ("a.x3f", 0.2, [0.1, 0.2, 0.3]),
            ("b.x3f", 0.7, [0.6, 0.5, 0.4]),
        ] {
            let settings = runtime.settings(Path::new(path)).unwrap();
            assert_eq!(settings.sharpen_amount, Some(amount));
            assert_eq!(settings.dehaze.shadow, Some(color));
        }
        assert!(runtime.apply_edits(
            &[Edit::Optional {
                pointer: "/sharpen_amount".into(),
                enabled: false
            }],
            &mut session()
        ));
        assert!(runtime.photos.values().all(|photo| photo
            .settings
            .as_ref()
            .unwrap()
            .sharpen_amount
            .is_none()));
    }

    #[test]
    fn mixed_camera_edits_clamp_to_each_targets_supported_range() {
        let mut runtime = runtime();
        for (path, model, exposure) in [
            ("a.x3f", "SIGMA sd Quattro", 1.9),
            ("b.x3f", "SIGMA DP2 Merrill", 3.9),
        ] {
            let photo = runtime.photos.get_mut(Path::new(path)).unwrap();
            photo.metadata = Some(Arc::new(crate::capture::processing_metadata(
                [("CAMMODEL".into(), model.into())].into(),
            )));
            photo.settings.as_mut().unwrap().exposure = exposure;
        }
        assert!(runtime.apply_edits(&[number("/exposure", 0.5, [-2., 2.])], &mut session()));
        assert_eq!(runtime.settings(Path::new("a.x3f")).unwrap().exposure, 2.);
        assert_eq!(runtime.settings(Path::new("b.x3f")).unwrap().exposure, 4.);
    }

    #[test]
    fn pending_rendering_defaults_block_only_dependent_manual_actions() {
        let mut runtime = runtime();
        for photo in runtime.photos.values_mut() {
            photo.metadata = Some(Arc::new(crate::capture::processing_metadata(
                [("CAMMODEL".into(), "SIGMA sd Quattro".into())].into(),
            )));
            photo.settings.as_mut().unwrap().exposure = 0.;
            photo.settings.as_mut().unwrap().rendering = foveonic::render::Rendering::Monochrome;
        }
        assert!(!runtime.optional_readiness()[0].1);
        let off = Edit::Optional {
            pointer: "/sharpen_amount".into(),
            enabled: true,
        };
        assert!(!runtime.apply_edits(std::slice::from_ref(&off), &mut session()));
        assert!(runtime.photos.values().all(|photo| photo
            .settings
            .as_ref()
            .unwrap()
            .sharpen_amount
            .is_none()));
        assert!(runtime.apply_edits(&[number("/exposure", 0.25, [-2., 2.])], &mut session()));
        for photo in runtime.photos.values_mut() {
            photo.defaults_basis = photo.settings.clone();
            photo.reset_settings = Some(Settings {
                sharpen_amount: Some(1.3),
                ..Default::default()
            });
        }
        assert!(runtime.optional_readiness()[0].1);
        assert!(runtime.apply_edits(&[off], &mut session()));
        assert!(runtime.photos.values().all(|photo| photo
            .settings
            .as_ref()
            .unwrap()
            .sharpen_amount
            == Some(1.3)));
    }

    #[test]
    fn automatic_manual_round_trip_uses_automatic_default_not_the_previous_manual_value() {
        let mut runtime = runtime();
        for photo in runtime.photos.values_mut() {
            photo.numeric_settings.as_mut().unwrap().sharpen_amount = Some(0.9);
            photo.reset_settings = Some(Settings {
                sharpen_amount: Some(0.25),
                ..Default::default()
            });
        }
        assert!(runtime.apply_edits(
            &[Edit::Optional {
                pointer: "/sharpen_amount".into(),
                enabled: true
            }],
            &mut session()
        ));
        assert!(runtime.photos.values().all(|photo| photo
            .settings
            .as_ref()
            .unwrap()
            .sharpen_amount
            == Some(0.25)));
    }

    #[test]
    fn wb_dependent_colordq_does_not_block_constant_color_sharpening_or_exposure() {
        let mut runtime = runtime();
        for photo in runtime.photos.values_mut() {
            photo.metadata = Some(Arc::new(crate::capture::processing_metadata(
                [("CAMMODEL".into(), "SIGMA DP2 Merrill".into())].into(),
            )));
            photo.settings.as_mut().unwrap().white_balance = "Overcast".into();
            photo.reset_settings = Some(Settings {
                color_dq_gain: Some(0.4),
                ..Default::default()
            });
        }
        assert!(runtime.optional_readiness()[0].1);
        assert!(!runtime.optional_readiness()[1].1);
        assert!(!runtime.apply_edits(
            &[Edit::Optional {
                pointer: "/color_dq_gain".into(),
                enabled: true
            }],
            &mut session()
        ));
        assert!(runtime.apply_edits(&[number("/exposure", -0.25, [-4., 4.])], &mut session()));
    }

    #[test]
    fn captured_reset_values_remain_individual() {
        let mut runtime = runtime();
        for (path, chroma) in [("a.x3f", 1), ("b.x3f", 3)] {
            let photo = runtime.photos.get_mut(Path::new(path)).unwrap();
            photo.settings.as_mut().unwrap().reconstruction = Some(foveonic::noise::Controls {
                chroma: 4,
                ..Default::default()
            });
            photo.reset_settings = Some(Settings {
                reconstruction: Some(foveonic::noise::Controls {
                    chroma,
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
        let mut edit = number("/reconstruction/chroma", 0., [0., 4.]);
        if let Edit::Number { change, .. } = &mut edit {
            *change = NumberChange::Reset(2.);
        }
        assert!(runtime.apply_edits(&[edit], &mut session()));
        assert_eq!(
            runtime
                .settings(Path::new("a.x3f"))
                .unwrap()
                .reconstruction
                .unwrap()
                .chroma,
            1
        );
        assert_eq!(
            runtime
                .settings(Path::new("b.x3f"))
                .unwrap()
                .reconstruction
                .unwrap()
                .chroma,
            3
        );
    }

    #[test]
    fn automatic_exposure_shows_and_edits_each_captures_computed_value() {
        let mut runtime = runtime();
        for (path, compensation) in [("a.x3f", "-1"), ("b.x3f", "-2")] {
            let photo = runtime.photos.get_mut(Path::new(path)).unwrap();
            photo.metadata = Some(Arc::new(crate::capture::processing_metadata(
                [
                    ("CAMMODEL".into(), "SIGMA DP2 Merrill".into()),
                    ("EXPCOMP".into(), compensation.into()),
                ]
                .into(),
            )));
            photo.settings.as_mut().unwrap().exposure_auto = true;
        }
        let snapshots = runtime.numeric_snapshots();
        assert_eq!(snapshots[0].exposure, 1.);
        assert_eq!(snapshots[1].exposure, 2.);
        assert!(runtime.apply_edits(&[number("/exposure", 0.5, [-4., 4.])], &mut session()));
        assert_eq!(runtime.settings(Path::new("a.x3f")).unwrap().exposure, 1.5);
        assert_eq!(runtime.settings(Path::new("b.x3f")).unwrap().exposure, 2.5);
        assert!(runtime.photos.values().all(|photo| !photo
            .settings
            .as_ref()
            .unwrap()
            .exposure_auto));
    }
}
