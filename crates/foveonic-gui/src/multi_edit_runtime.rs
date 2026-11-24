//! Owns bounded photo settings and images with independent editing selection and render demand.

use crate::{multi_edit_controls, sidecars};
use foveonic::{
    render::{EditScope, Settings},
    source::{Metadata, ProcessingFamily},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Default)]
struct Photo {
    settings: Option<Settings>,
    /// The camera family from the photo's summary, set when its settings
    /// load. A file whose summary cannot be read has none, and no image.
    family: Option<ProcessingFamily>,
    metadata: Option<Arc<Metadata>>,
    numeric_settings: Option<Settings>,
    reset_settings: Option<Settings>,
    defaults_basis: Option<Settings>,
}

/// One change to each target photo's settings.
pub enum Change {
    /// Field edits from the panel and tools.
    Fields(Vec<multi_edit_controls::Edit>),
    /// A quarter turn, which turns the crop with the photo.
    Turn { clockwise: bool },
    /// A preset, reset or default applied over one scope.
    Scope {
        scope: EditScope,
        source: Box<Settings>,
    },
    /// A whole pasted recipe.
    Replace(Box<Settings>),
}

#[derive(Default)]
pub struct Runtime {
    editing_paths: Vec<PathBuf>,
    /// Photos the viewport draws. Their settings are loaded so they can render.
    viewport: Vec<PathBuf>,
    // Several settings and frame snapshots make each photo large. Keep map
    // insertion and balancing from moving those records through the stack.
    photos: BTreeMap<PathBuf, Box<Photo>>,
    recent: Vec<PathBuf>,
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
    /// The loupe inspects a viewport photo: it counts as recently used.
    pub fn view_detail(&mut self, path: Option<&Path>) {
        if let Some(path) = path {
            self.selected(path);
        }
    }
    /// Load settings for the editing selection and every viewport photo, and keep
    /// recent photos as history while the total stays within `limit`. Loaded
    /// photos alone may exceed it; history is then empty.
    pub fn sync(
        &mut self,
        paths: &[PathBuf],
        viewport: &[PathBuf],
        limit: usize,
        sidecars: &mut sidecars::Session,
    ) -> bool {
        let loaded = loaded_paths(paths, viewport);
        let limit = limit.max(loaded.len());
        if self.editing_paths == paths && self.viewport == viewport && self.photos.len() <= limit {
            return false;
        }
        self.editing_paths = paths.to_vec();
        self.viewport = viewport.to_vec();
        for path in &loaded {
            self.photos.entry(path.clone()).or_default();
            if !self.recent.contains(path) {
                self.recent.push(path.clone());
            }
        }
        while self.photos.len() > limit {
            let Some(index) = self.recent.iter().position(|path| !loaded.contains(path)) else {
                break;
            };
            let path = self.recent.remove(index);
            self.photos.remove(&path);
        }
        let missing = loaded
            .iter()
            .filter(|path| self.photos[*path].settings.is_none())
            .cloned()
            .collect::<Vec<_>>();
        sidecars.resolve_settings(&missing);
        true
    }
    /// Load resolved settings once each photo's summary is read, with the
    /// family it gives, fitted to that family. A fit that changes a recipe is
    /// saved. `summary` gives a read summary's family, which an unreadable
    /// file does not have.
    pub fn resolve(
        &mut self,
        summary: impl Fn(&Path) -> Option<Option<ProcessingFamily>>,
        sidecars: &mut sidecars::Session,
    ) {
        // Replies for photos no longer held are taken too, and dropped.
        let replies = sidecars
            .take_resolved(|path| !self.photos.contains_key(path) || summary(path).is_some());
        for reply in replies {
            let Some(photo) = self.photos.get_mut(&reply.path) else {
                continue;
            };
            if photo.settings.is_some() {
                continue;
            }
            photo.family = summary(&reply.path).flatten();
            let mut settings = reply.settings;
            if crate::controls::normalize(&mut settings, photo.family) {
                sidecars.apply(reply.path, settings.clone());
            }
            photo.settings = Some(settings);
        }
    }
    pub fn multiple(&self) -> bool {
        self.editing_paths.len() > 1
    }
    pub fn ready(&self) -> bool {
        self.targets().all(|photo| photo.settings.is_some())
    }
    pub fn editable_ready(&self) -> bool {
        self.ready() && self.targets().all(|photo| photo.numeric_settings.is_some())
    }
    pub fn snapshots(&self) -> Vec<Settings> {
        if !self.ready() {
            return Vec::new();
        }
        self.editing_paths
            .iter()
            .map(|path| self.photos[path].settings.as_ref().unwrap().clone())
            .collect()
    }
    pub fn numeric_snapshots(&self) -> Vec<Settings> {
        if !self.editable_ready() {
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
    /// Start collecting the editing panel's edits, with what its controls need
    /// to know about every editing photo.
    pub fn begin_panel(&self, ctx: &eframe::egui::Context) {
        multi_edit_controls::begin_with_numeric(ctx, self.snapshots(), self.numeric_snapshots());
        multi_edit_controls::set_metadata(ctx, self.metadata_snapshots());
        for (pointer, ready) in self.optional_readiness() {
            multi_edit_controls::set_optional_ready(ctx, pointer, ready);
        }
        for (pointer, range) in self.control_ranges() {
            multi_edit_controls::set_range(ctx, &pointer, range);
        }
    }
    pub fn optional_readiness(&self) -> [(&'static str, bool); 3] {
        ["/sharpen_amount", "/color_dq_gain", "/dehaze/shadow"].map(|pointer| {
            (
                pointer,
                self.targets().all(|photo| default_ready(photo, pointer)),
            )
        })
    }
    /// The photo's camera family, once its summary is read.
    pub fn family(&self, path: &Path) -> Option<ProcessingFamily> {
        self.photos.get(path)?.family
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
    /// Apply `change` to each photo in `paths`, save what changed and report
    /// whether rendering changed. Every photo must have loaded settings.
    pub fn edit(
        &mut self,
        paths: &[PathBuf],
        change: &Change,
        sidecars: &mut sidecars::Session,
    ) -> bool {
        if paths.is_empty() || paths.iter().any(|path| self.settings(path).is_none()) {
            return false;
        }
        let fields = if let Change::Fields(edits) = change {
            let fields = self.applicable(paths, edits);
            if fields.is_empty() {
                return false;
            }
            fields
        } else {
            Vec::new()
        };
        let mut render_changed = false;
        for path in paths {
            let photo = self.photos.get_mut(path).unwrap();
            let before = photo.settings.clone().unwrap();
            match change {
                Change::Fields(_) => apply_fields(photo, &fields),
                Change::Turn { clockwise } => {
                    let settings = photo.settings.as_mut().unwrap();
                    settings.rotation.turn(*clockwise, &mut settings.crop);
                }
                // Scoped actions need a render's metadata, which a photo
                // without a family never has.
                Change::Scope { scope, source } => {
                    if let (Some(family), Some(settings)) = (photo.family, &mut photo.settings) {
                        scope.apply(source, settings, family);
                    }
                }
                Change::Replace(recipe) => photo.settings = Some((**recipe).clone()),
            }
            let settings = photo.settings.as_mut().unwrap();
            crate::controls::normalize(settings, photo.family);
            if *settings != before {
                sidecars.apply(path.clone(), settings.clone());
                render_changed |=
                    !crate::rendering::matching::same_render_settings(&before, settings);
            }
        }
        render_changed
    }
    /// Give `paths` a whole recipe: loaded photos through `edit`, the rest
    /// recorded on the writer, which they resolve to when they load. Reports
    /// whether rendering changed.
    pub fn replace(
        &mut self,
        paths: &[PathBuf],
        recipe: &Settings,
        sidecars: &mut sidecars::Session,
    ) -> bool {
        let (loaded, unloaded): (Vec<_>, Vec<_>) = paths
            .iter()
            .cloned()
            .partition(|path| self.settings(path).is_some());
        for path in unloaded {
            sidecars.apply(path, recipe.clone());
        }
        self.edit(
            &loaded,
            &Change::Replace(Box::new(recipe.clone())),
            sidecars,
        )
    }
    /// The field edits every target can take: automatic values wait for each
    /// target's render-produced default.
    fn applicable(
        &self,
        paths: &[PathBuf],
        edits: &[multi_edit_controls::Edit],
    ) -> Vec<multi_edit_controls::Edit> {
        let targets = paths.iter().map(|path| self.photos[path].as_ref());
        edits
            .iter()
            .filter(|edit| match edit {
                multi_edit_controls::Edit::Optional {
                    pointer,
                    enabled: true,
                } => targets.clone().all(|photo| default_ready(photo, pointer)),
                multi_edit_controls::Edit::Number { pointer, .. }
                    if matches!(pointer.as_str(), "/sharpen_amount" | "/color_dq_gain") =>
                {
                    targets.clone().all(|photo| {
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
            .collect()
    }
    /// The photo's settings are loaded or loading, so it can render.
    pub fn loaded(&self, path: &Path) -> bool {
        self.photos.contains_key(path)
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
    /// Reload every photo's settings, as when Import review's in-memory edits end.
    pub fn forget_settings(&mut self, sidecars: &mut sidecars::Session) {
        for photo in self.photos.values_mut() {
            photo.settings = None;
        }
        sidecars.resolve_settings(&loaded_paths(&self.editing_paths, &self.viewport));
    }
}

/// The photos whose settings stay loaded: the editing selection, then the
/// viewport photos outside it.
fn loaded_paths(editing: &[PathBuf], viewport: &[PathBuf]) -> Vec<PathBuf> {
    let mut loaded = editing.to_vec();
    loaded.extend(
        viewport
            .iter()
            .filter(|path| !editing.contains(path))
            .cloned(),
    );
    loaded
}

/// Apply field edits with this photo's own defaults and ranges.
fn apply_fields(photo: &mut Photo, edits: &[multi_edit_controls::Edit]) {
    let settings = photo.settings.as_mut().unwrap();
    let defaults = photo
        .reset_settings
        .as_ref()
        .or(photo.numeric_settings.as_ref());
    if let Some(numeric) = defaults {
        materialize_defaults(settings, numeric, edits);
        // Leaving Auto, by turning it off or editing the value, starts from
        // this photo's own calculated exposure.
        let leaves_auto = edits.iter().any(|edit| {
            matches!(edit, multi_edit_controls::Edit::Number { pointer, .. } if pointer == "/exposure")
                || *edit == multi_edit_controls::Edit::set(multi_edit_controls::field::EXPOSURE_AUTO, &false)
        });
        if settings.exposure_auto && leaves_auto {
            settings.exposure = photo
                .metadata
                .as_deref()
                .map_or(numeric.exposure, |metadata| {
                    let capture = foveonic::source::CaptureExposureCompensation::from_properties(
                        &metadata.properties,
                    );
                    settings
                        .exposure_auto_compensation
                        .inverse(capture)
                        .unwrap_or(0.)
                });
            settings.exposure_auto = false;
        }
    }
    let mut target_edits = resolve_optionals(settings, defaults, edits);
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
        let viewport = self.viewport_photos();
        if self.multi_edit.sync(
            self.edit_selection.paths(),
            &viewport,
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
        let browser = &self.browser;
        self.multi_edit.resolve(
            |path| browser.metadata_summary(path).map(|summary| summary.family),
            &mut self.sidecars,
        );
    }
}

#[cfg(test)]
impl Runtime {
    /// Editing photos as after their first render: each with its metadata and
    /// the `numeric` values the render produced.
    pub(crate) fn rendered(
        photos: impl IntoIterator<Item = (PathBuf, Settings, Settings, Option<Arc<Metadata>>)>,
    ) -> Self {
        let mut runtime = Self::default();
        for (path, settings, numeric, metadata) in photos {
            runtime.editing_paths.push(path.clone());
            runtime.recent.push(path.clone());
            runtime.photos.insert(
                path,
                Box::new(Photo {
                    defaults_basis: Some(settings.clone()),
                    settings: Some(settings),
                    family: metadata
                        .as_deref()
                        .and_then(|metadata| metadata.processing_family().ok()),
                    metadata,
                    numeric_settings: Some(numeric),
                    reset_settings: None,
                }),
            );
        }
        runtime
    }
    /// This photo as before its first render: settings without rendered values.
    pub(crate) fn unrendered(&mut self, path: &Path) {
        self.photos.get_mut(path).unwrap().numeric_settings = None;
    }
}

#[cfg(test)]
#[path = "multi_edit_panel_test.rs"]
mod panel_tests;

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
    /// Edit the editing selection as the panel does.
    fn apply(runtime: &mut Runtime, edits: &[Edit], sidecars: &mut sidecars::Session) -> bool {
        let paths = runtime.editing_paths.clone();
        runtime.edit(&paths, &Change::Fields(edits.to_vec()), sidecars)
    }
    fn runtime() -> Runtime {
        let paths = vec![PathBuf::from("a.x3f"), PathBuf::from("b.x3f")];
        let mut runtime = Runtime {
            editing_paths: paths.clone(),
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
                    family: Some(ProcessingFamily::Merrill),
                    numeric_settings: Some(numeric),
                    ..Default::default()
                }),
            );
        }
        runtime
    }

    #[test]
    fn settings_load_fitted_to_their_family_once_the_summary_is_read() {
        let mut runtime = runtime();
        let mut sidecars = session();
        let a = PathBuf::from("a.x3f");
        runtime.photos.insert(a.clone(), Box::default());
        let merrill_only = Settings {
            merrill_noise_method: foveonic::render::MerrillNoiseMethod::MultiscaleRows,
            ..Default::default()
        };
        sidecars.apply(a.clone(), merrill_only);
        sidecars.resolve_settings(std::slice::from_ref(&a));
        runtime.resolve(|_| None, &mut sidecars);
        assert_eq!(runtime.settings(&a), None, "settings wait for the summary");

        runtime.resolve(|_| Some(Some(ProcessingFamily::Classic)), &mut sidecars);
        let fitted = runtime.settings(&a).unwrap();
        assert_eq!(
            fitted.merrill_noise_method,
            foveonic::render::MerrillNoiseMethod::Reference,
            "a Classic photo loses Merrill-only choices"
        );
        assert_eq!(
            sidecars.requested_settings(&a),
            Some(fitted),
            "the fit is saved"
        );

        // A file whose summary cannot be read still loads, so its render can
        // report why it fails.
        let unreadable = PathBuf::from("unreadable.x3f");
        runtime.photos.insert(unreadable.clone(), Box::default());
        sidecars.apply(unreadable.clone(), Settings::default());
        sidecars.resolve_settings(std::slice::from_ref(&unreadable));
        runtime.resolve(|_| Some(None), &mut sidecars);
        assert!(runtime.settings(&unreadable).is_some());
        assert_eq!(runtime.family(&unreadable), None);
    }

    #[test]
    fn turns_scopes_and_pasted_recipes_save_each_target() {
        let mut runtime = runtime();
        let mut sidecars = session();
        let paths = runtime.editing_paths.clone();
        assert!(runtime.edit(
            &paths[..1],
            &Change::Turn { clockwise: true },
            &mut sidecars
        ));
        assert_eq!(runtime.settings(&paths[0]).unwrap().rotation.turns, 1);
        assert_eq!(runtime.settings(&paths[1]).unwrap().rotation.turns, 0);

        let source = Settings {
            exposure: 0.5,
            ..Default::default()
        };
        let tone = Change::Scope {
            scope: EditScope::Tool(foveonic::render::EditSection::Tone),
            source: Box::new(source.clone()),
        };
        assert!(runtime.edit(&paths, &tone, &mut sidecars));
        for path in &paths {
            let settings = runtime.settings(path).unwrap();
            assert_eq!(settings.exposure, 0.5);
            assert_eq!(sidecars.requested_settings(path), Some(settings));
        }
        assert_eq!(
            runtime.settings(&paths[0]).unwrap().rotation.turns,
            1,
            "a scope keeps the photo's framing"
        );

        let pasted = Settings {
            saturation: 0.25,
            ..Default::default()
        };
        let replace = Change::Replace(Box::new(pasted.clone()));
        assert!(runtime.edit(&paths[1..], &replace, &mut sidecars));
        assert_eq!(runtime.settings(&paths[1]), Some(&pasted));
        assert!(
            !runtime.edit(&paths[1..], &replace, &mut sidecars),
            "the same recipe changes nothing"
        );
    }

    #[test]
    fn edits_need_every_target_loaded() {
        let mut runtime = runtime();
        let mut sidecars = session();
        let unloaded = PathBuf::from("c.x3f");
        runtime.photos.insert(unloaded.clone(), Box::default());
        let before = runtime.settings(Path::new("a.x3f")).cloned();
        let paths = [PathBuf::from("a.x3f"), unloaded];
        assert!(!runtime.edit(&paths, &Change::Turn { clockwise: true }, &mut sidecars));
        assert_eq!(runtime.settings(&paths[0]).cloned(), before);
        assert_eq!(sidecars.requested_settings(&paths[0]), None);
    }

    #[test]
    fn a_whole_recipe_reaches_loaded_photos_now_and_unloaded_photos_when_they_load() {
        let mut runtime = runtime();
        let mut sidecars = session();
        let loaded = PathBuf::from("a.x3f");
        let unloaded = PathBuf::from("c.x3f");
        runtime.photos.insert(unloaded.clone(), Box::default());
        let pasted = Settings {
            saturation: 0.25,
            ..Default::default()
        };
        assert!(runtime.replace(&[loaded.clone(), unloaded.clone()], &pasted, &mut sidecars));
        assert_eq!(runtime.settings(&loaded), Some(&pasted));
        assert_eq!(runtime.settings(&unloaded), None);
        assert_eq!(sidecars.requested_settings(&unloaded), Some(&pasted));
        sidecars.resolve_settings(std::slice::from_ref(&unloaded));
        runtime.resolve(|_| Some(Some(ProcessingFamily::Merrill)), &mut sidecars);
        assert_eq!(runtime.settings(&unloaded), Some(&pasted));
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
        assert!(!apply(&mut runtime, &[rename], &mut sidecars));
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
        assert!(apply(
            &mut runtime,
            &[number("/exposure", 0.1, [-8., 8.])],
            &mut sidecars
        ));
    }

    #[test]
    fn forgotten_review_edits_reload_instead_of_lingering() {
        let mut runtime = runtime();
        let mut sidecars = session();
        let a = PathBuf::from("a.x3f");
        assert!(apply(
            &mut runtime,
            &[number("/exposure", 0.1, [-8., 8.])],
            &mut sidecars
        ));
        let edited = runtime.settings(&a).unwrap().clone();
        sidecars.set_memory_only(false);
        runtime.forget_settings(&mut sidecars);
        assert!(!runtime.ready(), "settings reload before editing resumes");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !runtime.ready() {
            sidecars.receive();
            runtime.resolve(|_| Some(Some(ProcessingFamily::Merrill)), &mut sidecars);
            assert!(
                std::time::Instant::now() < deadline,
                "settings did not reload"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_ne!(runtime.settings(&a), Some(&edited));
        assert_eq!(
            runtime.settings(&a),
            Some(&Settings::default()),
            "a session without recipes reloads the camera defaults"
        );
    }

    #[test]
    fn every_viewport_photo_is_loaded_and_only_photos_off_screen_are_evicted() {
        let mut runtime = runtime();
        let mut sidecars = session();
        let [a, b, c, d] = ["a.x3f", "b.x3f", "c.x3f", "d.x3f"].map(PathBuf::from);
        // Editing A while the viewport also shows C and D, with history for one more.
        runtime.sync(
            std::slice::from_ref(&a),
            &[a.clone(), c.clone(), d.clone()],
            1,
            &mut sidecars,
        );
        for path in [&a, &c, &d] {
            assert!(
                runtime.photos.contains_key(path),
                "{path:?} is on screen, so its settings load"
            );
        }
        assert!(
            !runtime.photos.contains_key(&b),
            "B is off screen and beyond the history limit"
        );
        assert!(runtime.loaded(&c) && runtime.loaded(&d));
    }

    #[test]
    fn subset_switches_retain_recent_photos_and_evict_only_inactive_lru() {
        let mut runtime = runtime();
        let mut sidecars = session();
        let a = PathBuf::from("a.x3f");
        let b = PathBuf::from("b.x3f");
        let c = PathBuf::from("c.x3f");
        let original = runtime.settings(&a).unwrap().clone();
        runtime.sync(std::slice::from_ref(&b), &[], 2, &mut sidecars);
        runtime.selected(&b);
        runtime.sync(std::slice::from_ref(&a), &[], 2, &mut sidecars);
        runtime.selected(&a);
        assert_eq!(runtime.settings(&a), Some(&original));
        assert!(runtime.settings(&b).is_some());
        assert_eq!(runtime.photos.len(), 2);

        runtime.sync(std::slice::from_ref(&c), &[], 2, &mut sidecars);
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
        runtime.photos.insert(inactive.clone(), Box::default());
        assert!(runtime.editable_ready());
        assert_eq!(runtime.snapshots().len(), 2);
        assert_eq!(runtime.numeric_snapshots().len(), 2);
        assert!(apply(
            &mut runtime,
            &[number("/exposure", 0.1, [-8., 8.])],
            &mut sidecars
        ));
        assert!(runtime.photos[&inactive].settings.is_none());
    }

    #[test]
    fn relative_edits_clamp_independently_and_preserve_unrelated_authored_values() {
        let mut runtime = runtime();
        let mut session = session();
        assert!(apply(
            &mut runtime,
            &[number("/exposure", 0.5, [-8., 8.])],
            &mut session
        ));
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
        assert!(apply(
            &mut runtime,
            &[number("/sharpen_amount", 0.1, [0., 3.])],
            &mut session
        ));
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
        assert!(apply(
            &mut runtime,
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
        assert!(apply(
            &mut runtime,
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
        assert!(apply(&mut runtime, &edits, &mut session()));
        for (path, amount, color) in [
            ("a.x3f", 0.2, [0.1, 0.2, 0.3]),
            ("b.x3f", 0.7, [0.6, 0.5, 0.4]),
        ] {
            let settings = runtime.settings(Path::new(path)).unwrap();
            assert_eq!(settings.sharpen_amount, Some(amount));
            assert_eq!(settings.dehaze.shadow, Some(color));
        }
        assert!(apply(
            &mut runtime,
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
            photo.metadata = Some(Arc::new(crate::metadata::processing_metadata(
                [("CAMMODEL".into(), model.into())].into(),
            )));
            photo.settings.as_mut().unwrap().exposure = exposure;
        }
        assert!(apply(
            &mut runtime,
            &[number("/exposure", 0.5, [-2., 2.])],
            &mut session()
        ));
        assert_eq!(runtime.settings(Path::new("a.x3f")).unwrap().exposure, 2.);
        assert_eq!(runtime.settings(Path::new("b.x3f")).unwrap().exposure, 4.);
    }

    #[test]
    fn pending_rendering_defaults_block_only_dependent_manual_actions() {
        let mut runtime = runtime();
        for photo in runtime.photos.values_mut() {
            photo.metadata = Some(Arc::new(crate::metadata::processing_metadata(
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
        assert!(!apply(
            &mut runtime,
            std::slice::from_ref(&off),
            &mut session()
        ));
        assert!(runtime.photos.values().all(|photo| photo
            .settings
            .as_ref()
            .unwrap()
            .sharpen_amount
            .is_none()));
        assert!(apply(
            &mut runtime,
            &[number("/exposure", 0.25, [-2., 2.])],
            &mut session()
        ));
        for photo in runtime.photos.values_mut() {
            photo.defaults_basis = photo.settings.clone();
            photo.reset_settings = Some(Settings {
                sharpen_amount: Some(1.3),
                ..Default::default()
            });
        }
        assert!(runtime.optional_readiness()[0].1);
        assert!(apply(&mut runtime, &[off], &mut session()));
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
        assert!(apply(
            &mut runtime,
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
    fn wb_dependent_automatic_defaults_wait_for_a_render_while_exposure_edits_apply() {
        let mut runtime = runtime();
        for photo in runtime.photos.values_mut() {
            photo.metadata = Some(Arc::new(crate::metadata::processing_metadata(
                [("CAMMODEL".into(), "SIGMA DP2 Merrill".into())].into(),
            )));
            photo.settings.as_mut().unwrap().white_balance = "Overcast".into();
            photo.reset_settings = Some(Settings {
                color_dq_gain: Some(0.4),
                ..Default::default()
            });
        }
        // Merrill color Auto sharpening and ColorDQ both follow the noise
        // tables, which depend on white balance.
        assert!(!runtime.optional_readiness()[0].1);
        assert!(!runtime.optional_readiness()[1].1);
        assert!(!apply(
            &mut runtime,
            &[Edit::Optional {
                pointer: "/color_dq_gain".into(),
                enabled: true
            }],
            &mut session()
        ));
        assert!(apply(
            &mut runtime,
            &[number("/exposure", -0.25, [-4., 4.])],
            &mut session()
        ));
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
        assert!(apply(&mut runtime, &[edit], &mut session()));
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
            photo.metadata = Some(Arc::new(crate::metadata::processing_metadata(
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
        assert!(apply(
            &mut runtime,
            &[number("/exposure", 0.5, [-4., 4.])],
            &mut session()
        ));
        assert_eq!(runtime.settings(Path::new("a.x3f")).unwrap().exposure, 1.5);
        assert_eq!(runtime.settings(Path::new("b.x3f")).unwrap().exposure, 2.5);
        assert!(runtime.photos.values().all(|photo| !photo
            .settings
            .as_ref()
            .unwrap()
            .exposure_auto));
    }
}
