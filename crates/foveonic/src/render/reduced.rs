//! Owns corrected reduced sensors and bounds retained multi-photo editing resources.

use super::{recipes, Engine, PreviewQuality, Settings};
use crate::{
    image::{RenderedImage, View},
    inspection::Artifact,
    interactive::{Cache, Execution, Superseded},
    profile::Profile,
    source::{Metadata, ProcessingFamily, Result, Source},
};
use std::sync::Arc;

/// The caller must recover the authoritative original outside the processing core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeedsOriginal {
    NativeOutput,
    UnsupportedFamily,
    UnsupportedGeometry,
    NativeAuxiliaryInput,
    UpstreamChanged,
    AnalysisChanged,
    EngineChanged,
}

pub enum ReducedPreparation {
    Prepared(Box<ReducedSession>),
    NeedsOriginal {
        reason: NeedsOriginal,
        source: Arc<Source>,
    },
    Superseded,
}
pub enum ReducedRender {
    Rendered(Vec<Artifact>),
    NeedsOriginal(NeedsOriginal),
    Superseded,
}

/// Owns no decoded Source. The sensor image retains camera metadata and explicit
/// reduced geometry. Only the existing downstream recipe runs during editing.
pub struct ReducedSession {
    sensor: RenderedImage,
    upstream: Vec<u8>,
    calibration: Option<std::sync::Weak<crate::noise_calibration::Calibration>>,
    cache: Cache,
    metadata_bytes: u64,
    auto_haze: Option<super::reduced_analysis::AutoHaze>,
    last_settings: Option<Box<Settings>>,
}
impl ReducedSession {
    pub fn metadata(&self) -> &Arc<Metadata> {
        &self.sensor.metadata
    }
    pub fn quality(&self) -> PreviewQuality {
        self.sensor.settings.preview_quality
    }
    /// Unique retained GPU storage, including the reduced sensor checkpoint.
    pub fn cached_bytes(&self) -> u64 {
        unique_buffer_bytes(self.buffers())
    }
    /// Logical metadata and identity payload. Allocator and map-node overhead is excluded.
    pub fn retained_cpu_bytes(&self) -> u64 {
        let mut retained = crate::recipe::RetainedBytes::default();
        let containers = self.account_keys(&mut retained);
        self.owned_cpu_bytes() + containers as u64 + retained.bytes() as u64
    }
    fn account_keys(&self, retained: &mut crate::recipe::RetainedBytes) -> usize {
        self.cache.account_keys(retained)
            + self
                .auto_haze
                .as_ref()
                .map_or(0, |haze| haze.account(retained))
    }
    fn owned_cpu_bytes(&self) -> u64 {
        self.metadata_bytes
            + self.upstream.capacity() as u64
            + std::mem::size_of::<Self>() as u64
            + self.sensor.settings.white_balance.capacity() as u64
            + self.sensor.settings.color_mode.capacity() as u64
            + self.last_settings.as_ref().map_or(0, |settings| {
                std::mem::size_of::<Settings>()
                    + settings.white_balance.capacity()
                    + settings.color_mode.capacity()
            }) as u64
    }
    fn buffers(&self) -> impl Iterator<Item = &crate::gpu::ReadGuard> {
        std::iter::once(&self.sensor.buffer).chain(self.cache.buffers())
    }
}

fn unique_buffer_bytes<'a>(buffers: impl Iterator<Item = &'a crate::gpu::ReadGuard>) -> u64 {
    let mut unique: Vec<&crate::gpu::ReadGuard> = Vec::new();
    for buffer in buffers {
        if !unique.iter().any(|prior| prior.shares_storage(buffer)) {
            unique.push(buffer);
        }
    }
    unique.into_iter().map(|buffer| buffer.size()).sum()
}

fn requirements(metadata: &Metadata, settings: &Settings, names: &[&str]) -> Option<NeedsOriginal> {
    if !matches!(
        settings.preview_quality,
        PreviewQuality::Draft | PreviewQuality::Thumb
    ) || names != ["final"]
        || settings.preview_region.is_some()
        || settings.output_size.is_some()
        || settings.is_monochrome()
    {
        return Some(NeedsOriginal::NativeOutput);
    }
    if metadata.processing_family().ok() != Some(ProcessingFamily::Merrill) {
        return Some(NeedsOriginal::UnsupportedFamily);
    }
    if !settings.small_preview_available(metadata, settings.preview_quality) {
        return Some(NeedsOriginal::UnsupportedGeometry);
    }
    let effective = settings.processing_settings(ProcessingFamily::Merrill);
    if effective.directional_banding_selected()
        || effective.adaptive_tone_active()
        || effective.dehaze_active()
        || effective.color_editor_active()
        || effective.advanced_color_active()
    {
        return Some(NeedsOriginal::NativeAuxiliaryInput);
    }
    None
}

/// Only controls consumed before sensor reduction belong here. Chroma, luminance
/// and texture edits remain downstream and must keep using the reduced sensor.
fn upstream(settings: &Settings) -> Result<Vec<u8>> {
    let effective = settings.processing_settings(ProcessingFamily::Merrill);
    Ok(serde_json::to_vec(&(
        settings.preview_quality,
        &effective.white_balance,
        effective.repair,
        effective.highlight_restoration,
        effective.highlight_threshold_ev.map(f32::to_bits),
        effective.highlight_mode,
        effective.highlight_amount.map(f32::to_bits),
        effective.color_field,
        effective.despeckle,
        effective.merrill_speckles,
        effective.merrill_noise_method,
        effective.multiscale_noise_selected(),
        effective.row_correction,
        effective.reference_rows_selected(),
        effective.sensor_noise_controls(),
    ))?)
}

// Metadata owns numerical matrices rather than the decoded photographic samples.
// Count their allocations and string payload once per shared metadata owner.
fn metadata_bytes(metadata: &Metadata) -> u64 {
    let properties: usize = metadata
        .properties
        .iter()
        .map(|(k, v)| k.capacity() + v.capacity())
        .sum();
    let matrices: usize = metadata
        .matrices
        .iter()
        .map(|(k, v)| {
            k.capacity()
                + std::mem::size_of_val(v)
                + v.values.capacity() * std::mem::size_of::<f64>()
                + v.dimensions.capacity() * std::mem::size_of::<usize>()
        })
        .sum();
    let lists: usize = metadata
        .lists
        .iter()
        .map(|(k, list)| {
            k.capacity()
                + list
                    .names()
                    .map(|name| {
                        name.len()
                            + list.get(name).map_or(0, String::capacity)
                            + std::mem::size_of::<(String, String)>()
                    })
                    .sum::<usize>()
        })
        .sum();
    (std::mem::size_of_val(metadata) + properties + matrices + lists) as u64
}

impl Engine {
    /// Check known native-only requirements before discarding a warm native session.
    /// Sealing and rendering still validate source, engine and upstream identities.
    pub fn reduced_session_requirement(
        metadata: &Metadata,
        settings: &Settings,
    ) -> Option<NeedsOriginal> {
        requirements(metadata, settings, &["final"])
    }

    /// Consume the native source after executing its existing corrected sensor
    /// prefix. Success releases native CPU samples and temporary native graph keys.
    /// Unsupported requests return the same Arc for authoritative fallback. Other
    /// owners, such as a pending disk-cache write, may temporarily share the source.
    pub fn prepare_reduced_session(
        &self,
        source: impl Into<Arc<Source>>,
        settings: &Settings,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<ReducedPreparation> {
        let source = source.into();
        settings.validate_values()?;
        profile.mode.check_gpu_features(self.device().features())?;
        if let Some(reason) = requirements(&source.metadata, settings, &["final"]) {
            return Ok(ReducedPreparation::NeedsOriginal { reason, source });
        }
        let identity = upstream(settings)?;
        let effective = settings.processing_settings(ProcessingFamily::Merrill);
        let mut execution = Execution {
            cancel: Some(&cancel),
            ..Execution::default()
        };
        let prepared =
            (|| -> Result<(RenderedImage, Option<super::reduced_analysis::AutoHaze>)> {
                execution.check()?;
                let built = recipes::merrill::prepare_reduced_sensor(
                    self,
                    &source,
                    &effective,
                    profile,
                    &mut execution,
                )?;
                let mut values = self.execute_controlled(
                    super::Input::Source(&source),
                    &built.graph,
                    &[built.final_id],
                    profile,
                    &mut execution,
                )?;
                let value = values.pop().ok_or("missing reduced sensor")?;
                let description = value.description.ok_or("reduced sensor must be an image")?;
                let sensor = RenderedImage {
                    buffer: value.buffer,
                    description,
                    view: View {
                        crop: built.params.crop,
                        rotation: built.params.source[2],
                        extent: [built.params.output[0], built.params.output[1]],
                    },
                    metadata: source.metadata.clone(),
                    settings: settings.clone(),
                    owner: self.owner.clone(),
                    dehaze_basis: None,
                };
                let auto_haze = if settings.dehaze_estimate && settings.dehaze_auto() {
                    Some(super::reduced_analysis::AutoHaze::prepare(
                        self,
                        &source,
                        &sensor,
                        settings,
                        profile,
                        &mut execution,
                    )?)
                } else {
                    None
                };
                Ok((sensor, auto_haze))
            })();
        self.release_native_preparation(&source.metadata);
        match prepared {
            Ok((sensor, auto_haze)) => Ok(ReducedPreparation::Prepared(Box::new(ReducedSession {
                metadata_bytes: metadata_bytes(&sensor.metadata),
                sensor,
                upstream: identity,
                calibration: self.noise_calibration.as_ref().map(Arc::downgrade),
                cache: Cache::default(),
                auto_haze,
                last_settings: None,
            }))),
            Err(error) if error.downcast_ref::<Superseded>().is_some() => {
                Ok(ReducedPreparation::Superseded)
            }
            Err(error) => Err(error),
        }
    }

    pub fn render_reduced_session(
        &self,
        session: &mut ReducedSession,
        settings: &Settings,
        names: &[&str],
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<ReducedRender> {
        self.render_reduced_session_observed(session, settings, names, cancel, profile, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_reduced_session_with_progress(
        &self,
        session: &mut ReducedSession,
        settings: &Settings,
        names: &[&str],
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: impl Fn(super::RenderProgress),
    ) -> Result<ReducedRender> {
        self.render_reduced_session_observed(
            session,
            settings,
            names,
            cancel,
            profile,
            Some(&progress),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_reduced_session_observed(
        &self,
        session: &mut ReducedSession,
        settings: &Settings,
        names: &[&str],
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: Option<&dyn Fn(super::RenderProgress)>,
    ) -> Result<ReducedRender> {
        settings.validate_values()?;
        if !Arc::ptr_eq(&session.sensor.owner, &self.owner) {
            return Ok(ReducedRender::NeedsOriginal(NeedsOriginal::EngineChanged));
        }
        if let Some(reason) = requirements(session.metadata(), settings, names) {
            return Ok(ReducedRender::NeedsOriginal(reason));
        }
        let same_calibration = match (&session.calibration, &self.noise_calibration) {
            (None, None) => true,
            (Some(a), Some(b)) => a.upgrade().is_some_and(|a| Arc::ptr_eq(&a, b)),
            _ => false,
        };
        if !same_calibration || upstream(settings)? != session.upstream {
            return Ok(ReducedRender::NeedsOriginal(NeedsOriginal::UpstreamChanged));
        }
        let auto_haze_requested = settings.dehaze_estimate && settings.dehaze_auto();
        if auto_haze_requested {
            let Some(haze) = &session.auto_haze else {
                return Ok(ReducedRender::NeedsOriginal(NeedsOriginal::AnalysisChanged));
            };
            let mut execution = Execution {
                cancel: Some(&cancel),
                ..Execution::default()
            };
            match haze.matches(self, &session.sensor, settings, profile, &mut execution) {
                Ok(true) => {}
                Ok(false) => {
                    return Ok(ReducedRender::NeedsOriginal(NeedsOriginal::AnalysisChanged))
                }
                Err(error) if error.downcast_ref::<Superseded>().is_some() => {
                    return Ok(ReducedRender::Superseded)
                }
                Err(error) => return Err(error),
            }
        }
        let mut request = std::borrow::Cow::Borrowed(settings);
        if auto_haze_requested {
            request.to_mut().dehaze_estimate = false;
        }
        session.last_settings = Some(Box::new(settings.clone()));
        session.cache.bind(&self.owner);
        session.cache.select(settings.preview_quality, None);
        let mut execution = Execution {
            cache: Some(&mut session.cache),
            cancel: Some(&cancel),
            progress,
            ..Execution::default()
        };
        let result = self.render_recipe(
            recipes::RecipeInput::Reduced(&session.sensor),
            &request,
            recipes::Recipe::Photographic(names),
            profile,
            &mut execution,
        );
        session.cache.finish();
        match result {
            Ok(mut artifacts) => {
                if auto_haze_requested {
                    for artifact in &mut artifacts {
                        artifact.settings = settings.clone();
                        artifact.dehaze_estimate = session.auto_haze.as_ref().unwrap().color;
                        if let crate::inspection::Content::Image(image) = &mut artifact.content {
                            image.settings = settings.clone();
                        }
                    }
                }
                Ok(ReducedRender::Rendered(artifacts))
            }
            Err(error) if error.downcast_ref::<Superseded>().is_some() => {
                Ok(ReducedRender::Superseded)
            }
            Err(error) => Err(error),
        }
    }
}

struct Entry {
    id: u64,
    session: Box<ReducedSession>,
}
/// An LRU for reduced-photo inputs and downstream checkpoints. The caller owns
/// photo identifiers and reloads evicted native inputs outside the core.
/// Mutation always enforces the budget, including a render that grows its cache.
pub struct ReducedWorkingSet {
    limit: u64,
    entries: Vec<Entry>,
}
impl ReducedWorkingSet {
    pub fn new(limit: u64) -> Self {
        Self {
            limit,
            entries: Vec::new(),
        }
    }
    pub fn contains(&self, id: u64) -> bool {
        self.entries.iter().any(|entry| entry.id == id)
    }
    pub fn metadata(&self, id: u64) -> Option<&Arc<Metadata>> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.session.metadata())
    }
    pub fn last_settings(&self, id: u64) -> Option<&Settings> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .and_then(|entry| entry.session.last_settings.as_deref())
    }
    pub fn session_bytes(&self, id: u64) -> Option<(u64, u64)> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| {
                (
                    entry.session.cached_bytes(),
                    entry.session.retained_cpu_bytes(),
                )
            })
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn retained_gpu_bytes(&self) -> u64 {
        unique_buffer_bytes(
            self.entries
                .iter()
                .flat_map(|entry| entry.session.buffers()),
        )
    }
    pub fn retained_cpu_bytes(&self) -> u64 {
        self.account_cpu(&mut crate::recipe::RetainedBytes::default())
    }
    fn account_cpu(&self, retained: &mut crate::recipe::RetainedBytes) -> u64 {
        let mut metadata = Vec::new();
        let containers: u64 = self
            .entries
            .iter()
            .map(|entry| {
                let session = &entry.session;
                let mut bytes = session.owned_cpu_bytes() + session.account_keys(retained) as u64;
                if metadata
                    .iter()
                    .any(|old| Arc::ptr_eq(old, session.metadata()))
                {
                    bytes -= session.metadata_bytes;
                } else {
                    metadata.push(session.metadata().clone());
                }
                bytes
            })
            .sum();
        containers + retained.bytes() as u64
    }
    /// Include engine-owned preparation, deduplicating payloads shared by keys.
    /// Caller-owned delivered images, display textures and execution scratch are separate.
    pub fn retained_bytes(&self, engine: &Engine) -> u64 {
        let mut retained = crate::recipe::RetainedBytes::default();
        engine.account_preparation(&mut retained);
        self.retained_gpu_bytes() + self.account_cpu(&mut retained)
    }
    pub fn insert(
        &mut self,
        id: u64,
        session: Box<ReducedSession>,
        engine: &Engine,
    ) -> Result<Vec<u64>> {
        engine.check_owner(&session.sensor)?;
        self.entries.retain(|entry| entry.id != id);
        self.entries.push(Entry { id, session });
        Ok(self.enforce(engine))
    }
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        before != self.entries.len()
    }
    pub fn render(
        &mut self,
        engine: &Engine,
        id: u64,
        settings: &Settings,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<Option<ReducedRender>> {
        self.render_observed(engine, id, settings, cancel, profile, None)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_with_progress(
        &mut self,
        engine: &Engine,
        id: u64,
        settings: &Settings,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: impl Fn(super::RenderProgress),
    ) -> Result<Option<ReducedRender>> {
        self.render_observed(engine, id, settings, cancel, profile, Some(&progress))
    }
    #[allow(clippy::too_many_arguments)]
    fn render_observed(
        &mut self,
        engine: &Engine,
        id: u64,
        settings: &Settings,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: Option<&dyn Fn(super::RenderProgress)>,
    ) -> Result<Option<ReducedRender>> {
        let Some(index) = self.entries.iter().position(|entry| entry.id == id) else {
            return Ok(None);
        };
        let mut entry = self.entries.remove(index);
        let result = engine.render_reduced_session_observed(
            &mut entry.session,
            settings,
            &["final"],
            cancel,
            profile,
            progress,
        );
        self.entries.push(entry);
        self.enforce(engine);
        result.map(Some)
    }
    fn enforce(&mut self, engine: &Engine) -> Vec<u64> {
        let mut evicted = Vec::new();
        while !self.entries.is_empty() && self.retained_bytes(engine) > self.limit {
            evicted.push(self.entries.remove(0).id);
        }
        if self.retained_bytes(engine) > self.limit {
            engine.clear_preparation();
        }
        evicted
    }
}

#[cfg(test)]
#[path = "reduced_test.rs"]
mod tests;
