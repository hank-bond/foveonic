//! Keeps rendering off the UI thread and replaces outdated requests instead of queuing edits.

mod diagnostics;
mod preparation;
pub(crate) use super::source_loader::LoadHold;
pub(crate) use diagnostics::DeliveryHold;
mod host;
pub(crate) mod presentation;
mod processing;
pub(crate) use preparation::Config as PreparationConfig;

use super::{
    matching::{presentation_changed, same_pixel_settings, same_request_context},
    request::RenderRequest,
};
use crate::export::resolve_camera_settings;
use crate::rendering::request::RequestId;
use eframe::egui;
use foveonic::{
    image::{ImageTexture, RenderedImage},
    inspection::{Artifact, Content},
    profile::Profile,
    render::{
        Engine, Interruption, NativePhoto, ProcessingQuality, Scale, Scheduler, Settings,
        WorkingSet,
    },
    source::Metadata,
};
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread,
};

/// How long an idle worker waits before rechecking for work without a notification.
const IDLE_RECHECK: std::time::Duration = std::time::Duration::from_millis(100);

#[cfg(test)]
#[derive(Default)]
struct Checks {
    source_loads: std::sync::atomic::AtomicU64,
    deferred: (Mutex<Vec<super::coordinator::Stamp>>, Condvar),
    recipes: Mutex<Vec<(u64, u64)>>,
    speculative_hold: Mutex<
        Option<(
            crossbeam_channel::Sender<()>,
            crossbeam_channel::Receiver<()>,
        )>,
    >,
    yielded: (Mutex<Vec<super::coordinator::Stamp>>, Condvar),
    /// The most jobs admitted together.
    widest: std::sync::atomic::AtomicU64,
}
#[cfg(test)]
thread_local! { static CHECKS: std::cell::RefCell<Option<Arc<Checks>>> = const { std::cell::RefCell::new(None) }; }

/// How a render task ended.
pub enum RenderOutcome {
    Rendered(Box<RenderResult>),
    /// Superseded or stopped before it finished. It still settles its task.
    Cancelled,
    Failed(String),
}
impl RenderOutcome {
    pub fn rendered(&self) -> Option<&RenderResult> {
        match self {
            Self::Rendered(image) => Some(image),
            _ => None,
        }
    }
    pub fn failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
    /// The rendered image, panicking with the failure otherwise.
    #[cfg(test)]
    pub fn unwrap_rendered(self) -> RenderResult {
        match self {
            Self::Rendered(image) => *image,
            Self::Cancelled => panic!("render was cancelled"),
            Self::Failed(error) => panic!("render failed: {error}"),
        }
    }
    /// The failure, panicking if the render did not fail.
    #[cfg(test)]
    pub fn unwrap_failed(self) -> String {
        match self {
            Self::Failed(error) => error,
            _ => panic!("render did not fail"),
        }
    }
}

/// A delivered render ready to draw. What it shows (photo, look, output,
/// framing) and its request id come from the request it answers.
pub struct RenderResult {
    pub request: Arc<RenderRequest>,
    pub diagnostic_task: crate::timing::DeliveredTask,
    pub origin: foveonic::source::SourceOrigin,
    native_extent: [u32; 2],
    pub artifact: Artifact,
    pub texture: Arc<ImageTexture>,
    pub comparison: Option<Comparison>,
    pub milliseconds: f64,
    pub queued_at: std::time::Instant,
}
impl RenderResult {
    /// The processing quality the render actually used.
    pub fn processing_quality(&self) -> ProcessingQuality {
        self.artifact.settings.processing_quality
    }
    pub fn calibration_signature(&self) -> Option<&str> {
        self.request
            .calibration
            .as_ref()
            .map(|profile| profile.signature())
    }
    /// Whole edited photographs can outlive their viewport and seed browser previews.
    pub fn is_photographic(&self) -> bool {
        self.request.rendition == crate::editor::Rendition::Current
            && self.request.output == "final"
            && !self.request.framing_preview
            && self.artifact.settings.color_range_preview.is_none()
            && self.artifact.settings.preview_region.is_none()
            && !self.request.detail
    }
    pub fn is_current_detail(&self) -> bool {
        self.request.rendition == crate::editor::Rendition::Current
            && self.request.output == "final"
            && !self.request.framing_preview
            && self.comparison.is_none()
            && self.artifact.settings.color_range_preview.is_none()
            && self.request.detail
    }
    pub fn detail_region(&self) -> [u32; 4] {
        self.artifact
            .settings
            .preview_region
            .unwrap_or_else(|| self.display_window(false))
    }
    pub fn diagnostic_identity(&self, input: bool) -> Option<crate::timing::ImageIdentity> {
        crate::timing::enabled().then(|| {
            self.diagnostic_task.image(
                self.request.id.0,
                &self.request.path,
                if input {
                    self.comparison
                        .as_ref()
                        .map_or(self.artifact.name, |comparison| comparison.input)
                } else {
                    self.artifact.name
                },
                self.artifact.settings.working_scale.divisor(),
            )
        })
    }
    /// Keep zoom coordinates native while Fast stores fewer pixels.
    pub fn display_extent(&self, input: bool) -> [u32; 2] {
        let [l, t, r, b] = self.display_window(input);
        [r - l, b - t]
    }
    pub fn camera_extent(&self) -> [u32; 2] {
        self.native_extent
    }
    /// Native sampling extent before any requested final output resize.
    pub fn native_display_extent(&self) -> [u32; 2] {
        crate::rendering::quality::shown_extent(
            self.native_extent,
            &self.request.settings,
            self.request.framing_preview,
        )
    }
    pub fn uncropped_extent(&self) -> [u32; 2] {
        self.request.settings.rotation.extent(self.native_extent)
    }
    pub fn editing_extent(&self) -> [u32; 2] {
        self.request
            .settings
            .rotation
            .oriented_extent(self.native_extent)
    }
    /// Native oriented bounds represented by the main texture, including crop offsets.
    pub fn display_window(&self, input: bool) -> [u32; 4] {
        if self.request.framing_preview {
            let extent = self.editing_extent();
            return [0, 0, extent[0], extent[1]];
        }
        if self.request.output == "final"
            && !input
            && self.artifact.settings.output_size.is_none()
            && self.artifact.settings.preview_region.is_none()
        {
            let extent = self.uncropped_extent();
            return self.artifact.settings.crop.pixels(extent);
        }
        let extent = self.view(input).description().extent;
        if self.artifact.settings.working_scale.is_reduced()
            && self.artifact.settings.output_size.is_none()
        {
            let divisor = self.artifact.settings.working_scale.divisor();
            [0, 0, extent[0] * divisor, extent[1] * divisor]
        } else {
            [0, 0, extent[0], extent[1]]
        }
    }
    pub fn quality_label(&self) -> String {
        match self.processing_quality() {
            ProcessingQuality::Quality => "Quality".into(),
            ProcessingQuality::Speed => "Speed".into(),
        }
    }
    /// Keep the output visible until its matching input is ready.
    pub fn view(&self, input: bool) -> &Arc<ImageTexture> {
        if input {
            if let Some(comparison) = &self.comparison {
                return &comparison.texture;
            }
        }
        &self.texture
    }
}
#[derive(Clone)]
pub struct Comparison {
    pub input: &'static str,
    pub texture: Arc<ImageTexture>,
}
pub struct Reply {
    pub handling: Option<super::coordinator::ResultUpdate>,
    pub stamp: crate::rendering::coordinator::Stamp,
    /// The request the result answers, which a failure is recorded against.
    pub request: Arc<RenderRequest>,
    pub metadata: Option<Arc<Metadata>>,
    pub settings: Settings,
    pub result: RenderOutcome,
    pub numeric_settings: Option<Settings>,
    pub reset_settings: Option<Settings>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeKey {
    /// The request id of the image the scopes describe.
    pub request_id: RequestId,
    pub region: Option<foveonic::scopes::Region>,
}
pub struct ScopeRequest {
    pub key: ScopeKey,
    pub image: RenderedImage,
    pub photo: u64,
    pub parent_render_task: u64,
    pub parent_render_revision: u64,
}
pub struct ScopeSnapshot {
    pub key: ScopeKey,
    pub data: foveonic::scopes::Scopes,
}

pub enum Update {
    Image(Box<Reply>),
    ColorPick {
        id: crate::rendering::coordinator::TaskId,
        reply: ColorPickReply,
    },
    Scopes {
        id: crate::rendering::coordinator::TaskId,
        key: ScopeKey,
        result: Result<foveonic::scopes::Scopes, String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorPickTool {
    Basic,
    Advanced,
    Dehaze,
    WhiteBalance,
}

pub struct ColorPickRequest {
    pub tool: ColorPickTool,
    pub token: u64,
    pub path: PathBuf,
    pub settings: Settings,
    pub uv: [f32; 2],
}
pub struct ColorPickReply {
    pub tool: ColorPickTool,
    pub token: u64,
    pub path: PathBuf,
    pub sample: Result<foveonic::render::ColorRangeSample, String>,
}

#[derive(Default)]
pub(super) struct Pending {
    pub(super) source: Option<super::source_loader::Loaded>,
    // Paths queued, loading or awaiting consumption of their source handoff.
    pub(super) source_requests: std::collections::HashSet<PathBuf>,
    pub(super) memory: Option<MemoryTarget>,
    /// Tests cap how many jobs run together. None leaves width to memory.
    #[cfg(test)]
    pub(super) width_cap: Option<usize>,
    pub(super) preparation: Option<Arc<PreparationConfig>>,
    pub(super) plan: Arc<crate::rendering::coordinator::WorkPlan>,
    pub(super) activity: Option<foveonic::priority::Demand>,
    pub(super) auxiliary_activity: Option<foveonic::priority::Demand>,
    pub(super) thumbnail_activity: Option<foveonic::priority::Demand>,
    pub(super) stop: bool,
    pub(super) clear: bool,
    pub(super) clear_all: bool,
    pub(super) retained: Arc<[Arc<presentation::Snapshot>]>,
}
pub(super) struct MemoryTarget {
    pub(super) bytes: u64,
    pub(super) frontend: crate::memory::Inventory,
}
impl Default for MemoryTarget {
    fn default() -> Self {
        Self {
            bytes: crate::preferences::DEFAULT_MEMORY_CACHE_BYTES,
            frontend: Default::default(),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryStatus {
    pub resources: foveonic::render::MemoryResources,
    pub over_target_bytes: u64,
    pub reduced_sources: Vec<(PathBuf, foveonic::source::SourceOrigin)>,
}

pub struct Worker {
    #[cfg(test)]
    checks: Arc<Checks>,
    #[cfg(test)]
    test_pending: Arc<(Mutex<Pending>, Condvar)>,
    pub replies: crossbeam_channel::Receiver<Update>,
    pub progress: crate::progress::Tracker,
    pub cache: Arc<foveonic::decode_cache::Cache>,
    memory: Arc<Mutex<MemoryStatus>>,
    runtime: super::runtime::Handle,
    observed_memory: RefCell<MemoryStatus>,
    diagnostics: diagnostics::Handle,
    source_control: super::source_loader::Control,
}
/// Resolve camera controls and sampling once, preserving a reused photographic plane's grid.
fn resolve_settings(
    request: &RenderRequest,
    metadata: &Metadata,
    existing_scale: Option<Scale>,
) -> foveonic::source::Result<(Settings, Vec<String>)> {
    let mut settings = request.settings.clone();
    // The request's quality is the authority; `quality::for_view` decides it.
    settings.processing_quality = request.processing_quality;
    let errors = resolve_camera_settings(&mut settings, metadata);
    if let Some(scale) = existing_scale {
        settings.working_scale = scale;
        settings.preview_region = None;
    } else {
        request.apply_preview(&mut settings, metadata)?;
    }
    settings.dehaze_estimate = request.output == "final"
        && request.rendition == crate::editor::Rendition::Current
        && settings.dehaze_auto();
    Ok((settings, errors))
}

/// A running job continues when the newer request can still use its result:
/// only the view changed, or only parameters it already recomputes.
fn interruption(
    state: &Pending,
    task: &crate::rendering::coordinator::Task,
    settings: &Settings,
    metadata: &Metadata,
    edit: &[&str],
) -> Interruption {
    if state.clear || state.stop {
        return Interruption::Cancel;
    }
    let Some(desired) = state.plan.task(&task.stamp.target) else {
        return Interruption::Cancel;
    };
    if task.origin.is_some() && desired.origin.is_some() && task.origin != desired.origin {
        return Interruption::Cancel;
    }
    if desired.stamp.id == task.stamp.id {
        return Interruption::Continue;
    }
    let pending = &desired.request;
    let Ok((next, _)) = resolve_settings(pending, metadata, None) else {
        return Interruption::Cancel;
    };
    if same_request_context(&task.request, pending)
        && presentation_changed(&task.request, pending)
        && same_pixel_settings(settings, &next)
    {
        return Interruption::Continue;
    }
    if !same_request_context(&task.request, pending)
        || settings.working_scale != next.working_scale
        || settings.preview_region != next.preview_region
        || (!edit.is_empty()
            && settings
                .changed_parameters(&next)
                .iter()
                .any(|parameter| !edit.contains(parameter)))
    {
        Interruption::Cancel
    } else {
        Interruption::Continue
    }
}

impl Worker {
    pub fn new(engine: Engine, context: egui::Context) -> Self {
        let pending = Arc::new((
            Mutex::new(Pending {
                source: None,
                source_requests: Default::default(),
                memory: None,
                #[cfg(test)]
                width_cap: None,
                preparation: None,
                plan: Arc::default(),
                activity: None,
                auxiliary_activity: None,
                thumbnail_activity: None,
                stop: false,
                clear: false,
                clear_all: false,
                retained: Default::default(),
            }),
            Condvar::new(),
        ));
        let progress = crate::progress::Tracker::default();
        let progress_updates = progress.clone();
        let cache = Arc::new(foveonic::decode_cache::Cache::new());
        let decoded_cache = cache.clone();
        let (sources, loader, source_control) =
            super::source_loader::start(cache.clone(), progress.clone(), pending.clone());
        let repaint = context.clone();
        let (tx, results) = crossbeam_channel::bounded(2);
        let (diagnostics, diagnostic_work) = diagnostics::channel(pending.clone());
        let slot = pending.clone();
        let memory = Arc::new(Mutex::new(MemoryStatus::default()));
        let memory_updates = memory.clone();
        let image_cache = foveonic::render::ImageCache::default();
        let worker_cache = image_cache.clone();
        #[cfg(test)]
        let checks = Arc::new(Checks::default());
        #[cfg(test)]
        let observations = checks.clone();
        let builder = thread::Builder::new().name("render-worker".into());
        let spawned = builder.spawn(move || {
            #[cfg(test)]
            CHECKS.with(|slot| *slot.borrow_mut() = Some(observations.clone()));
            let mut scheduler =
                Scheduler::new(engine, MemoryTarget::default().bytes, worker_cache.clone());
            let mut host = host::WorkerHost::new(
                sources,
                slot.clone(),
                tx,
                decoded_cache,
                progress_updates,
                memory_updates,
                context.clone(),
                diagnostic_work,
                worker_cache,
                #[cfg(test)]
                observations,
            );
            loop {
                let (stop, clear, memory) = {
                    let mut state = slot.0.lock().unwrap();
                    #[cfg(test)]
                    scheduler.set_width_cap(state.width_cap);
                    let clear = std::mem::take(&mut state.clear)
                        .then(|| std::mem::take(&mut state.clear_all));
                    (state.stop, clear, state.memory.take())
                };
                if stop {
                    break;
                }
                if let Some(all) = clear {
                    scheduler.drop_jobs();
                    host.reset();
                    if all {
                        let (engine, working) = scheduler.engine_and_working();
                        host.photos
                            .clear(working, engine)
                            .expect("jobs retire before clear");
                        host.reset_preparation();
                    }
                    host.trim(&mut scheduler);
                    continue;
                }
                if let Some(target) = memory {
                    scheduler.set_target(target.bytes);
                    host.target = target;
                    host.trim(&mut scheduler);
                }
                host.sync(&mut scheduler);
                let worked = scheduler.tick(&mut host);
                #[cfg(test)]
                host.observations.widest.fetch_max(
                    scheduler.widest() as u64,
                    std::sync::atomic::Ordering::Relaxed,
                );
                if host.disconnected {
                    break;
                }
                if worked {
                    host.trim(&mut scheduler);
                    continue;
                }
                if host.diagnostic_work.pending_capture() {
                    host.diagnostic_work.capture_one(scheduler.engine());
                    context.request_repaint();
                    continue;
                }
                let (lock, wake) = &*slot;
                let mut state = lock.lock().unwrap();
                // Nothing can run now, so release viewport demand that other
                // threads would otherwise wait behind.
                state.activity = None;
                if state.stop
                    || state.clear
                    || state.memory.is_some()
                    || host.plan_changed(&state)
                    || host.diagnostic_work.pending_capture()
                {
                    continue;
                }
                drop(wake.wait_timeout(state, IDLE_RECHECK).unwrap());
            }
            scheduler.drop_jobs();
        });
        spawned.expect("start render-worker thread");
        let (runtime, replies) = super::runtime::Handle::start(
            pending.clone(),
            results,
            cache.clone(),
            progress.clone(),
            repaint.clone(),
            loader,
        );
        Self {
            #[cfg(test)]
            checks,
            #[cfg(test)]
            test_pending: pending,
            replies,
            progress,
            cache,
            memory,
            runtime,
            observed_memory: Default::default(),
            diagnostics,
            source_control,
        }
    }

    /// Diagnostic readback of an immutable frame after the measured interaction.
    pub fn capture_image(
        &self,
        image: RenderedImage,
        destination: PathBuf,
    ) -> crossbeam_channel::Receiver<Result<(), String>> {
        self.diagnostics.capture(image, destination)
    }
    pub fn hold_delivery(&self, path: PathBuf, revision: u64) -> DeliveryHold {
        self.diagnostics.hold_delivery(path, revision)
    }
    pub fn hold_source(&self, path: PathBuf) -> LoadHold {
        self.source_control.hold(path)
    }
    pub fn configure_preparation(&self, config: PreparationConfig) {
        self.runtime.preparation(config);
    }
    pub fn pick_color(
        &self,
        request: ColorPickRequest,
        image_request_id: RequestId,
        origin: Option<foveonic::source::SourceOrigin>,
    ) {
        self.runtime.pick(request, image_request_id, origin);
    }
    pub fn set_memory_target(&self, target: u64, frontend: crate::memory::Inventory) {
        self.runtime.memory(target, frontend);
    }
    pub fn memory_status(&self) -> MemoryStatus {
        if let Ok(status) = self.memory.try_lock() {
            *self.observed_memory.borrow_mut() = status.clone();
        }
        self.observed_memory.borrow().clone()
    }
    pub fn clear_all(&self) {
        self.runtime.clear(true);
    }
    pub fn submit_scopes(&self, request: ScopeRequest) {
        self.runtime.scopes(request);
    }
    pub fn plan(
        &self,
        photos: super::coordinator::Photos,
        planned: Vec<super::view_plan::Planned>,
        queued_at: std::time::Instant,
    ) -> bool {
        self.runtime.plan(photos, planned, queued_at)
    }
    pub fn planned_patch(&self, path: &std::path::Path) -> Option<[u32; 4]> {
        self.runtime.planned_patch(path)
    }
    #[cfg(test)]
    pub fn clear(&self) {
        self.runtime.clear(false);
    }
    pub fn busy(&self) -> bool {
        self.runtime.busy()
    }
    #[cfg(test)]
    pub fn is_settled_and_admitted(&self, frame: &RenderResult) -> bool {
        self.runtime.is_settled_and_admitted(frame)
    }
    pub fn whole_delivered(&self, request_id: RequestId) {
        self.runtime.whole_delivered(request_id);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn memory_snapshot(
    phase: &'static str,
    resources: foveonic::render::MemoryResources,
    target: u64,
    working: &WorkingSet,
) {
    foveonic::diagnostics::event(
        foveonic::diagnostics::Category::Memory,
        "memory.snapshot",
        |fields| {
            fields
                .str("phase", phase)
                .u64("cpu_bytes", resources.cpu_bytes)
                .u64("gpu_bytes", resources.gpu_bytes)
                .u64("reserved_bytes", resources.reserved_bytes)
                .u64("retained_bytes", resources.total_bytes)
                .u64("target_bytes", target)
                .u64(
                    "over_target_bytes",
                    resources.total_bytes.saturating_sub(target),
                );
            fields.u64("native_sessions", working.native_count() as u64);
        },
    );
}

fn memory_inventory(
    presented: &presentation::Presentation,
    target: &MemoryTarget,
    pending_images: &mut Vec<crate::memory::PendingImage>,
    slot: &Arc<(Mutex<Pending>, Condvar)>,
) -> crate::memory::Inventory {
    let mut inventory = target.frontend.clone();
    let (retained, scopes, loaded) = {
        let pending = slot.0.lock().unwrap();
        (
            pending.retained.clone(),
            pending.plan.scopes.clone(),
            pending
                .source
                .as_ref()
                .and_then(|loaded| loaded.result.as_ref().ok())
                .cloned(),
        )
    };
    if let Some(source) = loaded {
        inventory.source(&source);
    }
    for snapshot in retained.iter() {
        snapshot.memory_resources(&mut inventory);
    }
    if let Some(scopes) = scopes {
        inventory.image(&scopes.request.image);
    }
    presented.memory_resources(&mut inventory);
    pending_images.retain(|image| image.reserve(&target.frontend, &mut inventory));
    inventory
}

fn record_source_load(revision: u64, path: &std::path::Path, loaded: &foveonic::source::Source) {
    #[cfg(test)]
    CHECKS.with(|slot| {
        if let Some(checks) = slot.borrow().as_ref() {
            checks
                .source_loads
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    });
    if crate::timing::enabled() {
        crate::timing::emit(
            "source_load",
            || serde_json::json!({"revision":revision,"source":path,"timings":loaded.load_timings(),"read_cache_state":"uncontrolled","input":if loaded.load_timings().iter().any(|t|t.stage=="decoded cache read"){"decoded-cache"}else{"original"},"extent":[loaded.metadata.width,loaded.metadata.height]}),
        );
    }
}

fn pick_color(
    engine: &Engine,
    session: &mut NativePhoto,
    request: &ColorPickRequest,
    cancel: impl Fn() -> bool,
) -> foveonic::source::Result<foveonic::render::ColorRangeSample> {
    if cancel() {
        return Err("color selection canceled".into());
    }
    let mut settings = request.settings.clone();
    settings.preview_region = None;
    settings.output_size = None;
    // Sampling native coordinates gives Full, Fast and Smart the same subject selection.
    settings.working_scale = Scale::NATIVE;
    let errors = resolve_camera_settings(&mut settings, session.metadata());
    if !errors.is_empty() {
        return Err(errors.join("; ").into());
    }
    let artifacts = engine
        .render_native_photo(
            session,
            &settings,
            &[match request.tool {
                ColorPickTool::Dehaze => "dehaze.input",
                ColorPickTool::WhiteBalance => "linear.base",
                _ => "color-editor.guide",
            }],
            &cancel,
            &mut Profile::default(),
        )?
        .ok_or("color selection canceled")?;
    if cancel() {
        return Err("color selection canceled".into());
    }
    let Some(Artifact {
        content: Content::Image(guide),
        ..
    }) = artifacts.into_iter().next()
    else {
        return Err("color selection requires a guide image".into());
    };
    if request.tool == ColorPickTool::WhiteBalance {
        let balance = engine.pick_white_balance(&guide, request.uv)?;
        let xyz = balance.neutral.ok_or("missing sampled neutral")?;
        Ok(foveonic::render::ColorRangeSample {
            weights: [0.; 8],
            color: [xyz[0], xyz[1], xyz[2], 1.],
        })
    } else if request.tool == ColorPickTool::Dehaze {
        let rgb = engine.pick_haze_color(&guide, request.uv)?;
        Ok(foveonic::render::ColorRangeSample {
            weights: [0.; 8],
            color: [rgb[0], rgb[1], rgb[2], 1.],
        })
    } else {
        engine.pick_color_range(&guide, request.uv)
    }
}

fn analyze_scopes(
    engine: &Engine,
    slot: &Arc<(Mutex<Pending>, Condvar)>,
    tx: &crossbeam_channel::Sender<Update>,
    task: Arc<crate::rendering::coordinator::ScopeTask>,
) -> bool {
    let request = &task.request;
    let _diagnostic_context = crate::timing::enabled().then(|| {
        foveonic::diagnostics::enter(foveonic::diagnostics::Context {
            task: task.id.0,
            revision: request.key.request_id.0,
            photo: request.photo,
            purpose: "scopes",
            node: None,
        })
    });
    let _scope = foveonic::diagnostics::scope_with(
        foveonic::diagnostics::Category::Cpu,
        "scope_task",
        |fields| {
            fields
                .u64("parent_render_task", request.parent_render_task)
                .u64("parent_render_revision", request.parent_render_revision);
        },
    );
    let current = || {
        let state = slot.0.lock().unwrap();
        !state.stop
            && !state.clear
            && state
                .plan
                .scopes
                .as_ref()
                .is_some_and(|desired| desired.id == task.id)
    };
    if !current() {
        return true;
    }
    let before = crate::timing::enabled().then(|| engine.gpu_activity());
    let mut profile = Profile::new(crate::timing::mode());
    let span = crate::timing::Span::new(request.key.request_id.0, "scopes");
    let result = engine
        .scopes(&request.image, request.key.region, &mut profile)
        .map_err(|error| {
            foveonic::support::report_error("render.scopes",error.as_ref(),foveonic::support::OperationContext {
                component:Some("scopes".into()),revision:Some(request.key.request_id.to_string()),
                settings:Some(serde_json::json!({"region":request.key.region,"parent_render_revision":request.parent_render_revision})),
                ..Default::default()
            });
            error.to_string()
        });
    if let Some(before) = before {
        crate::timing::emit(
            "scopes",
            || serde_json::json!({"revision":request.key.request_id,"region":request.key.region,"activity":engine.gpu_activity().since(before),"status":if result.is_ok(){"complete"}else{"error"}}),
        );
    }
    drop(span);
    if !current() {
        return true;
    }
    if tx
        .send(Update::Scopes {
            id: task.id,
            key: request.key,
            result,
        })
        .is_err()
    {
        return false;
    }
    true
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::rendering::request::RequestId;
    use foveonic::render::{ProcessingQuality, Scale};
    /// A worker driven like the app: every image a test wants is published
    /// together as one plan, so tests and the app share the planning path.
    /// Unlike the app, every wanted image is rendered (`ImageState::Render`), so
    /// held whole images and patch-first ordering are not exercised here, and
    /// `submit` keeps earlier photos wanted instead of retiring them. Publish
    /// only through these methods: the worker's own `plan` bypasses `wanted`.
    pub(crate) struct TestWorker {
        worker: super::Worker,
        /// `(request, speculative)` for each wanted image, in priority order.
        wanted: std::cell::RefCell<Vec<(super::RenderRequest, bool)>>,
        history: std::cell::RefCell<Vec<std::path::PathBuf>>,
    }
    impl std::ops::Deref for TestWorker {
        type Target = super::Worker;
        fn deref(&self) -> &Self::Target {
            &self.worker
        }
    }
    impl TestWorker {
        pub fn new(engine: foveonic::render::Engine, context: eframe::egui::Context) -> Self {
            Self {
                worker: super::Worker::new(engine, context),
                wanted: Default::default(),
                history: Default::default(),
            }
        }
        /// Cap how many jobs run together. Set it before submitting work.
        pub fn set_width_cap(&self, cap: usize) {
            self.worker.test_pending.0.lock().unwrap().width_cap = Some(cap);
        }
        /// Want this image too, replacing the image wanted for the same target.
        pub fn submit(&self, request: super::RenderRequest) {
            self.plan_images(vec![(request, false)]);
        }
        /// Want these images, each `(request, speculative)`, replacing those
        /// wanted for the same targets.
        pub fn plan_images(&self, images: Vec<(super::RenderRequest, bool)>) {
            let mut wanted = self.wanted.borrow_mut();
            for (request, speculative) in images {
                let target = crate::rendering::coordinator::Target::of(&request);
                wanted.retain(|(old, _)| crate::rendering::coordinator::Target::of(old) != target);
                wanted.push((request, speculative));
            }
            drop(wanted);
            self.publish();
        }
        /// Want only these images, as the app does when it shows them, keeping
        /// `retained` photos' finished images as history.
        pub fn show(&self, requests: Vec<super::RenderRequest>, retained: &[std::path::PathBuf]) {
            *self.wanted.borrow_mut() = requests
                .into_iter()
                .map(|request| (request, false))
                .collect();
            *self.history.borrow_mut() = retained.to_vec();
            self.publish();
        }
        /// Forget every wanted image and history photo, and clear the worker.
        pub fn clear(&self) {
            self.wanted.borrow_mut().clear();
            self.history.borrow_mut().clear();
            self.worker.clear();
        }
        fn publish(&self) {
            let wanted = self.wanted.borrow();
            let mut viewport = Vec::new();
            for (request, _) in wanted.iter() {
                if !viewport.contains(&request.path) {
                    viewport.push(request.path.clone());
                }
            }
            let planned = wanted
                .iter()
                .map(
                    |(request, speculative)| crate::rendering::view_plan::Planned {
                        request: request.clone(),
                        speculative: *speculative,
                        state: crate::rendering::view_plan::ImageState::Render,
                        tracked: (!speculative).then_some(0),
                    },
                )
                .collect();
            let photos = crate::rendering::coordinator::Photos {
                planned: viewport,
                history: self.history.borrow().clone(),
                ..Default::default()
            };
            self.worker.plan(photos, planned, std::time::Instant::now());
        }
    }
    /// What the coordinator decided about a reply before sending it on.
    pub(super) fn handling(reply: &super::Reply) -> super::super::coordinator::ResultUpdate {
        reply.handling.expect("coordinator handled the result")
    }
    pub(super) fn receive(worker: &super::Worker) -> super::Reply {
        loop {
            if let super::Update::Image(reply) = worker
                .replies
                .recv_timeout(std::time::Duration::from_secs(20))
                .unwrap()
            {
                handling(&reply);
                return *reply;
            }
        }
    }
    #[test]
    #[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
    fn worker_coalesces_identical_requests_and_rejects_table_inspection() {
        let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f");
        let worker = TestWorker::new(
            foveonic::render::Engine::new().unwrap(),
            eframe::egui::Context::default(),
        );
        for revision in 1..=20 {
            worker.submit(super::RenderRequest {
                detail: false,
                framing_preview: false,
                calibration: None,
                rendition: Default::default(),
                preview_scale: Scale::NATIVE,
                processing_quality: ProcessingQuality::Quality,
                id: RequestId(revision),
                path: path.clone(),
                settings: foveonic::render::Settings::new("", ""),
                output: "sensor.layers".into(),
                comparison: false,
            });
        }
        {
            let reply = receive(&worker);
            let frame = reply.result.unwrap_rendered();
            assert!(worker.is_settled_and_admitted(&frame));
            assert_eq!(frame.artifact.name, "sensor.layers");
            assert!(std::sync::Arc::ptr_eq(frame.view(true), &frame.texture));
            assert_eq!(
                frame.texture.description().space,
                foveonic::image::Space::SensorLayers
            );
            assert_eq!(reply.settings.white_balance, "Sunlight");
            assert_eq!(reply.settings.color_mode, "Standard");
            assert_eq!(frame.artifact.settings.white_balance, "Sunlight");
            assert!(reply.metadata.unwrap().properties.contains_key("CAMMODEL"));
        }
        worker.submit(super::RenderRequest {
            detail: false,
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            preview_scale: Scale::NATIVE,
            processing_quality: ProcessingQuality::Quality,
            id: RequestId(21),
            path: path.clone(),
            settings: foveonic::render::Settings::new("", ""),
            output: "final".into(),
            comparison: false,
        });
        let reply = receive(&worker);
        assert_eq!(reply.request.id, RequestId(21));
        let frame = reply.result.unwrap_rendered();
        assert_eq!(frame.artifact.name, "tone.rgb"); // final aliases the last enabled recipe output
        assert!(matches!(
            frame.texture.description().space,
            foveonic::image::Space::ToneMappedRgb(_)
        ));
        assert_eq!(frame.artifact.settings.white_balance, "Sunlight");
        assert_eq!(frame.artifact.settings.color_mode, "Standard");
        worker.submit(super::RenderRequest {
            detail: false,
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            preview_scale: Scale::NATIVE,
            processing_quality: ProcessingQuality::Quality,
            id: RequestId(22),
            path,
            settings: foveonic::render::Settings::new("", ""),
            output: "black.histogram".into(),
            comparison: false,
        });
        let reply = receive(&worker);
        assert_eq!(reply.request.id, RequestId(22));
        assert_eq!(reply.settings.white_balance, "Sunlight");
        assert_eq!(reply.settings.color_mode, "Standard");
        assert_eq!(reply.settings.working_scale, Scale::NATIVE);
        assert!(reply.settings.preview_region.is_none());
        assert!(!reply.settings.dehaze_estimate);
        assert!(reply
            .metadata
            .as_ref()
            .unwrap()
            .properties
            .contains_key("CAMMODEL"));
        assert!(reply
            .result
            .unwrap_failed()
            .contains("GUI inspection requires a visual image buffer"));
        worker.clear();
    }

    #[test]
    #[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
    fn comparison_frames_pair_operation_inputs_with_the_same_configuration() {
        use foveonic::{image::Space, render::Settings};
        let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f");
        let worker = TestWorker::new(
            foveonic::render::Engine::new().unwrap(),
            eframe::egui::Context::default(),
        );
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        settings.sharpen_amount = Some(0.75);
        settings.saturation = 1.2;
        for (index, (output, input)) in [
            ("repair.layers", "calibration.layers"),
            ("noise.layers", "banding.layers"),
            ("column.layers", "banding.layers"),
            ("color-dq.rgb", "color.rgb"),
            ("sharpen.rgb", "reconstruction.rgb"),
            ("saturation.rgb", "tone.rgb"),
        ]
        .into_iter()
        .enumerate()
        {
            let revision = index as u64 + 1;
            worker.show(
                vec![super::RenderRequest {
                    detail: false,
                    framing_preview: false,
                    calibration: None,
                    rendition: Default::default(),
                    preview_scale: Scale::NATIVE,
                    processing_quality: ProcessingQuality::Quality,
                    id: RequestId(revision),
                    path: path.clone(),
                    settings: settings.clone(),
                    output: output.into(),
                    comparison: true,
                }],
                &[],
            );
            let reply = receive(&worker);
            assert_eq!(reply.request.id, RequestId(revision));
            let frame = reply.result.unwrap_rendered();
            assert_eq!(frame.request.output, output);
            assert_eq!(frame.artifact.settings, settings);
            let comparison = frame.comparison.as_ref().unwrap();
            assert_eq!(comparison.input, input);
            for _ in 0..4 {
                assert!(std::sync::Arc::ptr_eq(frame.view(false), &frame.texture));
                assert!(std::sync::Arc::ptr_eq(
                    frame.view(true),
                    &comparison.texture
                ));
            }
            assert_eq!(
                comparison.texture.description().extent,
                frame.texture.description().extent
            );
            assert_eq!(
                comparison.texture.description().space,
                frame.texture.description().space
            );
        }
        worker.show(
            vec![super::RenderRequest {
                detail: false,
                framing_preview: false,
                calibration: None,
                rendition: Default::default(),
                preview_scale: Scale::NATIVE,
                processing_quality: ProcessingQuality::Quality,
                id: RequestId(9),
                path: path.clone(),
                settings: settings.clone(),
                output: "repair.layers".into(),
                comparison: true,
            }],
            &[],
        );
        let frame = receive(&worker).result.unwrap_rendered();
        assert_eq!(
            frame.comparison.as_ref().unwrap().input,
            "calibration.layers"
        );
        assert_eq!(frame.texture.description().space, Space::CalibratedLayers);
        drop(frame);
        for (revision, output) in [(10, "reconstruction.rgb"), (11, "final")] {
            worker.show(
                vec![super::RenderRequest {
                    detail: false,
                    framing_preview: false,
                    calibration: None,
                    rendition: Default::default(),
                    preview_scale: Scale::NATIVE,
                    processing_quality: ProcessingQuality::Quality,
                    id: RequestId(revision),
                    path: path.clone(),
                    settings: settings.clone(),
                    output: output.into(),
                    comparison: true,
                }],
                &[],
            );
            assert!(receive(&worker)
                .result
                .unwrap_failed()
                .contains("no operation-input comparison"));
        }
    }

    #[test]
    fn quattro_defaults_wait_for_recorded_white_balance_and_preserve_edits() {
        use foveonic::source::{Metadata, PropertyList};
        let mut metadata = Metadata {
            processing_version: Some([3, 1, 2, 0]),
            width: 5888,
            height: 3672,
            rotation: 0,
            properties: [
                ("CAMMODEL".into(), "SIGMA dp2 Quattro".into()),
                ("WB_DESC".into(), "Auto".into()),
                ("CM_DESC".into(), "Standard".into()),
            ]
            .into(),
            matrices: Default::default(),
            lists: [(
                "ColorModeCompensations".into(),
                PropertyList::new(vec![("Standard".into(), "standard".into())]).unwrap(),
            )]
            .into(),
        };
        let capabilities = foveonic::render::ControlCapabilities::from_metadata(&metadata).unwrap();
        let mut settings = foveonic::render::Settings::new("", "");
        capabilities.normalize(&mut settings);
        assert!(settings.white_balance.is_empty());
        assert_eq!(
            super::resolve_camera_settings(&mut settings, &metadata).len(),
            1
        );
        assert!(settings.white_balance.is_empty());
        assert_eq!(settings.color_mode, "Standard");
        metadata.lists.insert(
            "WhiteBalanceGains".into(),
            PropertyList::new(vec![("Auto".into(), "auto_gain".into())]).unwrap(),
        );
        assert!(super::resolve_camera_settings(&mut settings, &metadata).is_empty());
        assert_eq!(settings.white_balance, "Auto");
        settings.white_balance = "Shade".into();
        settings.color_mode = "Vivid".into();
        capabilities.normalize(&mut settings);
        assert!(super::resolve_camera_settings(&mut settings, &metadata).is_empty());
        assert_eq!(settings.white_balance, "Shade");
        assert_eq!(settings.color_mode, "Vivid");
    }

    #[test]
    fn defaults_resolve_independently_without_replacing_explicit_choices() {
        let metadata = foveonic::source::Metadata {
            processing_version: None,
            width: 1,
            height: 1,
            rotation: 0,
            properties: [
                ("WB_DESC".into(), "Unknown".into()),
                ("CM_DESC".into(), "Vivid".into()),
            ]
            .into(),
            matrices: Default::default(),
            lists: [(
                "ColorModeCompensations".into(),
                foveonic::source::PropertyList::new(vec![("Vivid".into(), "v".into())]).unwrap(),
            )]
            .into(),
        };
        let mut settings = foveonic::render::Settings::new("", "");
        assert_eq!(
            super::resolve_camera_settings(&mut settings, &metadata).len(),
            1
        );
        assert!(settings.white_balance.is_empty());
        assert_eq!(settings.color_mode, "Vivid");
        settings.white_balance = "Auto".into();
        settings.color_mode = "Standard".into();
        assert!(super::resolve_camera_settings(&mut settings, &metadata).is_empty());
        assert_eq!(settings.white_balance, "Auto");
        assert_eq!(settings.color_mode, "Standard");
    }

    #[test]
    #[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR; prints completed slider feedback timings"]
    fn slider_feedback_benchmark() {
        let path = std::env::var_os("FOVEONIC_SLIDER_X3F")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
                    .join("blue-parking-sign.x3f")
            });
        let worker = TestWorker::new(
            foveonic::render::Engine::new().unwrap(),
            eframe::egui::Context::default(),
        );
        let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        settings.sharpen_amount = Some(0.75);
        for revision in 0..10 {
            let control = if revision < 5 {
                settings.exposure = revision as f32 * 0.1;
                "exposure"
            } else if revision < 8 {
                settings.saturation = 1. + (revision - 5) as f32 * 0.1;
                "saturation"
            } else {
                settings.sharpen_amount = Some(0.8 + (revision - 8) as f32 * 0.1);
                "sharpen"
            };
            let start = std::time::Instant::now();
            worker.submit(super::RenderRequest {
                detail: false,
                framing_preview: false,
                calibration: None,
                rendition: Default::default(),
                preview_scale: Scale::NATIVE,
                processing_quality: ProcessingQuality::Quality,
                id: RequestId(revision),
                path: path.clone(),
                settings: settings.clone(),
                output: "final".into(),
                comparison: false,
            });
            let reply = receive(&worker);
            assert_eq!(reply.request.id, RequestId(revision));
            let frame = reply.result.unwrap_rendered();
            eprintln!(
                "SLIDER revision={revision} control={control} completed_ms={:.3} worker_ms={:.3}",
                start.elapsed().as_secs_f64() * 1000.,
                frame.milliseconds
            );
        }
    }

    #[test]
    #[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
    fn continuous_drag_makes_progress_then_settles_with_matching_scopes() {
        // Opt-in diagnostics run this test alone, keeping GPU waits out of normal checks.
        let _capture = if std::env::var_os("FOVEONIC_PROFILE").is_some() {
            crate::timing::configure().expect("run the profiled worker test with --exact");
            crate::diagnostics::test_capture()
        } else {
            None
        };
        let selected_control = std::env::var("FOVEONIC_SLIDER_CONTROL").ok();
        fn edit(settings: &mut foveonic::render::Settings, control: &str, revision: u64) {
            match control {
                "detail" => {
                    settings.reconstruction.as_mut().unwrap().texture.fine = revision as f32 / 20.
                }
                "exposure" => settings.exposure = revision as f32 / 40.,
                "chroma" => {
                    settings.reconstruction.as_mut().unwrap().chroma = (revision % 5) as u32
                }
                "luma" => {
                    settings.reconstruction.as_mut().unwrap().luminance = (revision % 5) as u32
                }
                "coarse" => {
                    settings.reconstruction.as_mut().unwrap().texture.coarse =
                        (revision % 40) as f32 / 20.
                }
                "fine-size" => {
                    settings.reconstruction.as_mut().unwrap().texture.fine_size =
                        0.5 + (revision % 40) as f32 / 40.
                }
                "coarse-size" => {
                    settings
                        .reconstruction
                        .as_mut()
                        .unwrap()
                        .texture
                        .coarse_size = 0.5 + (revision % 40) as f32 / 40.
                }
                _ => unreachable!(),
            }
        }
        for control in [
            "detail",
            "exposure",
            "chroma",
            "luma",
            "coarse",
            "fine-size",
            "coarse-size",
        ] {
            if selected_control
                .as_deref()
                .is_some_and(|selected| selected != control)
            {
                continue;
            }
            let last = if matches!(control, "detail" | "exposure") {
                41
            } else {
                161
            };
            let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
                .join("blue-parking-sign.x3f");
            let worker = TestWorker::new(
                foveonic::render::Engine::new().unwrap(),
                eframe::egui::Context::default(),
            );
            let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
            settings.reconstruction = Some(Default::default());
            settings.sharpen_amount = Some(0.75);
            let submit = |revision, settings: foveonic::render::Settings| {
                worker.submit(super::RenderRequest {
                    detail: false,
                    framing_preview: false,
                    calibration: None,
                    rendition: Default::default(),
                    preview_scale: Scale::NATIVE,
                    processing_quality: ProcessingQuality::Quality,
                    id: RequestId(revision),
                    path: path.clone(),
                    settings,
                    output: "final".into(),
                    comparison: false,
                })
            };
            submit(1, settings.clone());
            assert!(receive(&worker).result.rendered().is_some());
            // Prime this control's input boundary. The shared memory target may
            // have evicted its optional initial frontier while retaining Exposure.
            let warmup = std::time::Instant::now();
            let initial = settings.clone();
            edit(&mut settings, control, 2);
            if settings == initial {
                edit(&mut settings, control, 3);
            }
            submit(2, settings.clone());
            assert!(receive(&worker).result.rendered().is_some());
            eprintln!(
                "DRAG control={control} boundary_warmup_ms={:.3}",
                warmup.elapsed().as_secs_f64() * 1000.
            );
            let start = std::time::Instant::now();
            let mut progress = Vec::new();
            let mut settled = false;
            let mut scopes = false;
            for revision in 3..=last {
                edit(&mut settings, control, revision);
                submit(revision, settings.clone());
                std::thread::sleep(std::time::Duration::from_millis(8));
                while let Ok(update) = worker.replies.try_recv() {
                    if let super::Update::Image(reply) = update {
                        handling(&reply);
                        if let super::RenderOutcome::Rendered(frame) = reply.result {
                            let mut expected =
                                foveonic::render::Settings::new("Sunlight", "Standard");
                            expected.reconstruction = Some(Default::default());
                            expected.sharpen_amount = Some(0.75);
                            edit(&mut expected, control, reply.request.id.0);
                            assert_eq!(frame.artifact.settings, expected);
                            progress.push(reply.request.id);
                            settled |= reply.request.id == RequestId(last);
                            if reply.request.id == RequestId(last) {
                                request_scopes(&worker, &frame, None);
                            }
                        }
                    } else if let super::Update::Scopes { key, result, .. } = update {
                        if key.request_id == RequestId(last) {
                            assert!(result.is_ok());
                            scopes = true;
                        }
                    }
                }
            }
            assert!(
                !progress.is_empty(),
                "continuous dragging {control} starved image publication"
            );
            assert!(progress
                .windows(2)
                .all(|revisions| revisions[0] < revisions[1]));
            let stopped = std::time::Instant::now();
            while !settled || !scopes {
                match worker
                    .replies
                    .recv_timeout(std::time::Duration::from_secs(20))
                    .unwrap()
                {
                    super::Update::Image(reply) if reply.request.id == RequestId(last) => {
                        handling(&reply);
                        assert!(!settled, "unexpected extra final image");
                        let frame = reply.result.unwrap_rendered();
                        assert_eq!(frame.artifact.settings.exposure, settings.exposure);
                        assert_eq!(
                            frame.artifact.settings.reconstruction,
                            settings.reconstruction
                        );
                        settled = true;
                        request_scopes(&worker, &frame, None);
                        eprintln!(
                            "DRAG control={control} published_during_drag={} duration_ms={:.3} settled_ms={:.3}",
                            progress.len(),
                            start.elapsed().as_secs_f64() * 1000.,
                            stopped.elapsed().as_secs_f64() * 1000.
                        );
                    }
                    super::Update::Scopes { key, result, .. }
                        if key.request_id == RequestId(last) =>
                    {
                        assert!(result.is_ok());
                        scopes = true;
                    }
                    _ => {}
                }
            }
        }
    }

    fn request_scopes(
        worker: &super::Worker,
        frame: &super::RenderResult,
        region: Option<foveonic::scopes::Region>,
    ) {
        let foveonic::inspection::Content::Image(image) = &frame.artifact.content else {
            panic!("expected image");
        };
        worker.submit_scopes(super::ScopeRequest {
            key: super::ScopeKey {
                request_id: frame.request.id,
                region,
            },
            image: (**image).clone(),
            photo: foveonic::diagnostics::photo_id(&frame.request.path),
            parent_render_task: frame.diagnostic_task.id,
            parent_render_revision: frame.diagnostic_task.revision,
        });
    }

    #[test]
    #[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
    fn viewport_scope_jobs_reuse_the_displayed_image_and_settle_on_latest_region() {
        let engine = foveonic::render::Engine::new().unwrap();
        let worker = TestWorker::new(engine, eframe::egui::Context::default());
        let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f");
        worker.submit(super::RenderRequest {
            detail: false,
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            preview_scale: Scale::NATIVE,
            processing_quality: ProcessingQuality::Quality,
            id: RequestId(1),
            path,
            settings: foveonic::render::Settings::new("Sunlight", "Standard"),
            output: "final".into(),
            comparison: false,
        });
        let frame = receive(&worker).result.unwrap_rendered();
        let foreground = worker
            .cache
            .priority()
            .demand(foveonic::priority::Priority::Viewport);
        request_scopes(&worker, &frame, Some([0, 0, 64, 32]));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while worker.test_pending.0.lock().unwrap().plan.scopes.is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "scope demand was not published"
            );
            std::thread::yield_now();
        }
        assert!(
            worker
                .replies
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err(),
            "scope work ran ahead of foreground demand"
        );
        drop(foreground);
        let super::Update::Scopes { key, result, .. } = worker
            .replies
            .recv_timeout(std::time::Duration::from_secs(20))
            .unwrap()
        else {
            panic!("expected the scope held behind foreground work");
        };
        assert_eq!(key.region, Some([0, 0, 64, 32]));
        assert_eq!(result.unwrap().samples, [64, 32]);
        for left in 0..100 {
            request_scopes(&worker, &frame, Some([left, 0, left + 64, 32]));
        }
        let latest = Some([99, 0, 163, 32]);
        loop {
            match worker
                .replies
                .recv_timeout(std::time::Duration::from_secs(20))
                .unwrap()
            {
                super::Update::Scopes { key, result, .. } => {
                    assert_eq!(key.request_id, frame.request.id);
                    let data = result.unwrap();
                    if key.region == latest {
                        assert_eq!(data.samples, [64, 32]);
                        assert_eq!(data.histogram(0).iter().sum::<u32>(), 64 * 32);
                        break;
                    }
                }
                super::Update::Image(_) | super::Update::ColorPick { .. } => {
                    panic!("scope-only jobs must not publish other jobs")
                }
            }
        }
        request_scopes(&worker, &frame, None);
        let super::Update::Scopes { key, result, .. } = worker
            .replies
            .recv_timeout(std::time::Duration::from_secs(20))
            .unwrap()
        else {
            panic!("expected scopes");
        };
        assert_eq!(key.region, None);
        let samples = result.unwrap().samples;
        assert_eq!(samples[0].max(samples[1]), 512);
    }

    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn mixed_native_and_reduced_edits_preserve_warm_native_work() {
        use foveonic::{inspection::Content, render::Settings};
        use std::sync::atomic::Ordering;
        let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let paths = [
            root.join("blue-parking-sign.x3f"),
            root.join("merrill-hazy-coast.x3f"),
        ];
        let engine = foveonic::render::Engine::new().unwrap();
        let verifier = engine.executor();
        let worker = TestWorker::new(engine, eframe::egui::Context::default());
        worker.set_memory_target(4 * 1024 * 1024 * 1024, Default::default());
        let mut full = Settings::new("Sunlight", "Standard");
        let mut thumb = full.clone();
        thumb.reconstruction = Some(Default::default());
        let render = |revision, index: usize, quality: Scale, settings: &Settings| {
            worker.submit(super::RenderRequest {
                detail: false,
                path: paths[index].clone(),
                id: RequestId(revision),
                settings: settings.clone(),
                framing_preview: false,
                calibration: None,
                rendition: Default::default(),
                preview_scale: quality,
                processing_quality: ProcessingQuality::Quality,
                output: "final".into(),
                comparison: false,
            });
            let reply = receive(&worker);
            assert_eq!(reply.request.id, RequestId(revision));
            let frame = reply.result.unwrap_rendered();
            assert_eq!(frame.request.path, paths[index]);
            assert_eq!(
                frame.artifact.settings.working_scale.divisor(),
                quality.divisor()
            );
            frame
        };
        render(1, 0, Scale::NATIVE, &full);
        full.exposure = 0.125;
        render(2, 0, Scale::NATIVE, &full);
        let warm_dispatches = worker.checks.recipes.lock().unwrap().last().unwrap().1;
        assert!(warm_dispatches > 0);
        render(3, 1, Scale::new(8).unwrap(), &thumb); // Cold reduced acquisition.
        assert_eq!(worker.checks.source_loads.load(Ordering::Relaxed), 2);
        for round in 0..3 {
            full.exposure += 0.125;
            render(4 + round * 2, 0, Scale::NATIVE, &full);
            assert_eq!(
                worker.checks.source_loads.load(Ordering::Relaxed),
                2,
                "reduced work must not discard another photograph's native source"
            );
            assert_eq!(
                worker.checks.recipes.lock().unwrap().last().unwrap().1,
                warm_dispatches,
                "mixed edits must retain the native photograph's warm graph checkpoints"
            );
            thumb.exposure += 0.125;
            render(5 + round * 2, 1, Scale::new(8).unwrap(), &thumb); // Cached reduced edits.
            assert_eq!(worker.checks.source_loads.load(Ordering::Relaxed), 2);
        }
        // A finer reduced grid recovers its own original, retaining the other photo.
        let reduced_frame = render(10, 1, Scale::new(4).unwrap(), &thumb);
        assert_eq!(worker.checks.source_loads.load(Ordering::Relaxed), 3);
        full.exposure += 0.125;
        let native_frame = render(11, 0, Scale::NATIVE, &full);
        assert_eq!(worker.checks.source_loads.load(Ordering::Relaxed), 3);
        assert_eq!(
            worker.checks.recipes.lock().unwrap().last().unwrap().1,
            warm_dispatches
        );
        let captures = tempfile::tempdir().unwrap();
        for frame in [native_frame, reduced_frame] {
            let fresh = verifier
                .render_artifacts(
                    &foveonic::source::Source::open(&frame.request.path).unwrap(),
                    &frame.artifact.settings,
                    &["final"],
                    &mut Default::default(),
                )
                .unwrap()
                .pop()
                .unwrap();
            let Content::Image(expected) = fresh.content else {
                panic!("fresh photograph")
            };
            let Content::Image(actual) = &frame.artifact.content else {
                panic!("retained photograph")
            };
            let captured = captures.path().join(format!("{}.f32", frame.request.id));
            worker
                .capture_image(actual.as_ref().clone(), captured.clone())
                .recv_timeout(std::time::Duration::from_secs(20))
                .unwrap()
                .unwrap();
            assert_eq!(
                std::fs::read(captured).unwrap(),
                verifier.readback_image(&expected).unwrap()
            );
        }
    }

    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn photo_selection_retains_native_and_reduced_history_until_explicit_clear() {
        use std::sync::atomic::Ordering;
        let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let paths = [
            root.join("blue-parking-sign.x3f"),
            root.join("merrill-hazy-coast.x3f"),
        ];
        for quality in [Scale::new(8).unwrap(), Scale::NATIVE] {
            let worker = TestWorker::new(
                foveonic::render::Engine::new().unwrap(),
                eframe::egui::Context::default(),
            );
            worker.set_memory_target(4 * 1024 * 1024 * 1024, Default::default());
            let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
            settings.reconstruction = Some(Default::default());
            let request = |revision: u64, index: usize| super::RenderRequest {
                detail: false,
                framing_preview: false,
                calibration: None,
                rendition: Default::default(),
                id: RequestId(revision),
                path: paths[index].clone(),
                settings: settings.clone(),
                preview_scale: quality,
                processing_quality: ProcessingQuality::Quality,
                output: "final".into(),
                comparison: false,
            };
            for (round, index) in [0, 1, 0].into_iter().enumerate() {
                worker.clear();
                worker.submit(request(round as u64 + 1, index));
                let reply = receive(&worker);
                assert_eq!(reply.request.id, RequestId(round as u64 + 1));
                let frame = reply.result.unwrap_rendered();
                assert_eq!(frame.request.path, paths[index]);
                assert_eq!(frame.artifact.settings.working_scale, quality);
                assert_eq!(
                    worker.checks.source_loads.load(Ordering::Relaxed),
                    [1, 2, 2][round]
                );
            }
            let history = worker.checks.recipes.lock().unwrap().clone();
            assert!(
                history[2].1 < history[0].1,
                "returning to A must skip native preparation"
            );
            worker.clear_all();
            worker.clear(); // A later source reset must not weaken an already pending full clear.
            worker.submit(request(4, 0));
            assert!(receive(&worker).result.rendered().is_some());
            assert_eq!(worker.checks.source_loads.load(Ordering::Relaxed), 3);
        }
    }

    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn shared_source_consumption_resumes_current_before_and_pick() {
        use crate::editor::Rendition;
        use std::sync::atomic::Ordering;
        let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f");
        let worker = TestWorker::new(
            foveonic::render::Engine::new().unwrap(),
            eframe::egui::Context::default(),
        );
        let settings = foveonic::render::Settings::new("Sunlight", "Standard");
        let request = |revision, rendition, quality| super::RenderRequest {
            detail: false,
            path: path.clone(),
            id: RequestId(revision),
            rendition,
            settings: settings.clone(),
            framing_preview: false,
            calibration: None,
            preview_scale: quality,
            processing_quality: ProcessingQuality::Quality,
            output: "final".into(),
            comparison: false,
        };
        worker.submit(request(1, Rendition::Current, Scale::new(8).unwrap()));
        assert!(receive(&worker).result.rendered().is_some());
        worker.checks.deferred.0.lock().unwrap().clear();
        let hold = worker.hold_source(path.clone());
        worker.submit(request(2, Rendition::Current, Scale::NATIVE));
        hold.reached
            .recv_timeout(std::time::Duration::from_secs(20))
            .unwrap();
        worker.submit(request(3, Rendition::Camera, Scale::NATIVE));
        let (lock, wake) = &worker.checks.deferred;
        let (deferred, timeout) = wake
            .wait_timeout_while(
                lock.lock().unwrap(),
                std::time::Duration::from_secs(20),
                |tasks| tasks.len() < 2,
            )
            .unwrap();
        assert!(
            !timeout.timed_out(),
            "both renditions must defer before releasing the load: {deferred:?}"
        );
        drop(deferred);
        worker.pick_color(
            super::ColorPickRequest {
                tool: super::ColorPickTool::Basic,
                token: 1,
                path: path.clone(),
                settings,
                uv: [0.5, 0.5],
            },
            RequestId(3),
            None,
        );
        drop(hold);
        let mut rendered = Vec::new();
        let mut picked = false;
        while rendered.len() < 2 || !picked {
            match worker
                .replies
                .recv_timeout(std::time::Duration::from_secs(20))
                .unwrap()
            {
                super::Update::Image(reply) => {
                    let frame = reply.result.unwrap_rendered();
                    assert_eq!(
                        frame.artifact.settings.working_scale,
                        foveonic::render::Scale::NATIVE
                    );
                    assert!(!rendered.contains(&reply.request.rendition));
                    rendered.push(reply.request.rendition);
                }
                super::Update::ColorPick { reply, .. } => {
                    assert!(reply.sample.is_ok());
                    picked = true;
                }
                _ => {}
            }
        }
        assert!(rendered.contains(&Rendition::Current));
        assert!(rendered.contains(&Rendition::Camera));
        assert!(!worker.busy());
        assert_eq!(
            worker.checks.source_loads.load(Ordering::Relaxed),
            2,
            "initial Thumb and the single shared native acquisition must suffice"
        );
    }

    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn speculative_whole_yields_to_new_foreground_then_completes() {
        use crate::editor::Rendition;
        use std::time::{Duration, Instant};
        let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let paths = [
            root.join("blue-parking-sign.x3f"),
            root.join("merrill-hazy-coast.x3f"),
        ];
        let worker = TestWorker::new(foveonic::render::Engine::new().unwrap(), Default::default());
        let (reached, waiting) = crossbeam_channel::bounded(1);
        let (release, resume) = crossbeam_channel::bounded(1);
        *worker.checks.speculative_hold.lock().unwrap() = Some((reached, resume));
        let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
        settings.preview_region = Some([100, 100, 512, 512]);
        let first = super::RenderRequest {
            detail: true,
            framing_preview: false,
            calibration: None,
            rendition: Rendition::Current,
            id: RequestId(1),
            path: paths[0].clone(),
            settings,
            preview_scale: Scale::NATIVE,
            processing_quality: ProcessingQuality::Quality,
            output: "final".into(),
            comparison: false,
        };
        let mut whole = first.clone();
        whole.id = RequestId(2);
        whole.detail = false;
        whole.settings.preview_region = None;
        worker.plan_images(vec![(first.clone(), false), (whole.clone(), true)]);
        let regional = receive(&worker);
        assert_eq!(regional.request.id, RequestId(1));
        assert!(regional
            .result
            .unwrap_rendered()
            .artifact
            .settings
            .preview_region
            .is_some());
        waiting.recv_timeout(Duration::from_secs(30)).unwrap();
        let mut foreground = whole.clone();
        foreground.path = paths[1].clone();
        foreground.id = RequestId(3);
        foreground.preview_scale = Scale::new(8).unwrap();
        worker.submit(foreground);
        let deadline = Instant::now() + Duration::from_secs(30);
        while !worker
            .test_pending
            .0
            .lock()
            .unwrap()
            .plan
            .tasks
            .iter()
            .any(|task| task.stamp.request_id == RequestId(3))
        {
            assert!(Instant::now() < deadline, "foreground was not published");
            std::thread::yield_now();
        }
        release.send(()).unwrap();
        let ready = receive(&worker);
        assert_eq!(
            ready.request.id,
            RequestId(3),
            "foreground must arrive before the continuation"
        );
        assert!(ready.result.rendered().is_some());
        let completed = receive(&worker);
        assert_eq!(
            completed.request.id,
            RequestId(2),
            "yield must preserve the continuation"
        );
        let image = completed.result.unwrap_rendered();
        assert!(image.artifact.settings.preview_region.is_none());
        assert_eq!(worker.checks.yielded.0.lock().unwrap().len(), 1);
    }

    #[test]
    #[ignore = "requires GPU and CACHED_SELECTION_FILES with existing local decoded entries"]
    fn profile_cached_import_selection_and_warm_edits() {
        let _capture = if std::env::var_os("FOVEONIC_PROFILE").is_some() {
            crate::timing::configure().expect("run the profiled worker test with --exact");
            crate::diagnostics::test_capture()
        } else {
            None
        };
        let paths: Vec<_> =
            std::env::split_paths(&std::env::var_os("CACHED_SELECTION_FILES").unwrap()).collect();
        assert!(!paths.is_empty());
        let worker = TestWorker::new(
            foveonic::render::Engine::new().unwrap(),
            eframe::egui::Context::default(),
        );
        // Reuse existing entries without admitting prefetch or pruning the owner's cache.
        worker.cache.configure(foveonic::decode_cache::Settings {
            max_bytes: u64::MAX,
            classic: true,
            ..Default::default()
        });
        let mut frames = std::collections::HashMap::new();
        for revision in 1..=2 {
            let requests = paths
                .iter()
                .map(|path| {
                    let mut settings = foveonic::render::Settings::new("", "");
                    if revision == 2 {
                        settings.exposure = 0.1;
                    }
                    super::RenderRequest {
                        detail: false,
                        framing_preview: false,
                        calibration: None,
                        rendition: Default::default(),
                        id: RequestId(revision),
                        path: path.clone(),
                        settings,
                        preview_scale: Scale::new(8).unwrap(),
                        processing_quality: ProcessingQuality::Quality,
                        output: "final".into(),
                        comparison: false,
                    }
                })
                .collect();
            let began = std::time::Instant::now();
            worker.show(requests, &[]);
            for _ in &paths {
                let reply = receive(&worker);
                let frame = reply.result.unwrap_rendered();
                assert_eq!(frame.artifact.settings.working_scale.divisor(), 8);
                eprintln!(
                    "CACHED_SELECTION revision={revision} photo={} worker_ms={:.3} group_elapsed_ms={:.3}",
                    reply.request.path.display(),
                    frame.milliseconds,
                    began.elapsed().as_secs_f64() * 1000.
                );
                frames.insert(reply.request.path.clone(), frame);
            }
            let status = worker.cache.status();
            assert_eq!(status.misses, 0, "selection must use local decoded entries");
            assert_eq!(status.hits, paths.len() as u64);
            eprintln!(
                "CACHED_SELECTION revision={revision} photos={} total_ms={:.3} cache_hits={} cache_misses={}",
                paths.len(),
                began.elapsed().as_secs_f64() * 1000.,
                status.hits,
                status.misses
            );
        }
    }

    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn latest_multi_edit_batch_delivers_every_target_at_thumb() {
        let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let paths = [
            root.join("blue-parking-sign.x3f"),
            root.join("merrill-hazy-coast.x3f"),
        ];
        let worker = TestWorker::new(
            foveonic::render::Engine::new().unwrap(),
            eframe::egui::Context::default(),
        );
        let requests = |revision: u64| {
            paths
                .iter()
                .enumerate()
                .map(|(index, path)| {
                    let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
                    settings.exposure = revision as f32 * 0.03 + index as f32 * 0.2;
                    if index == 0 {
                        settings.dehaze.shadow = None;
                        settings.dehaze.amount = 0.;
                    }
                    super::RenderRequest {
                        detail: false,
                        framing_preview: false,
                        calibration: None,
                        rendition: Default::default(),
                        id: RequestId(revision),
                        path: path.clone(),
                        settings,
                        preview_scale: Scale::new(8).unwrap(),
                        processing_quality: ProcessingQuality::Quality,
                        output: "final".into(),
                        comparison: false,
                    }
                })
                .collect()
        };
        worker.show(requests(1), &[]);
        for _ in 0..paths.len() {
            assert!(receive(&worker).result.rendered().is_some());
        }
        for revision in 2..=12 {
            worker.show(requests(revision), &[]);
        }
        let mut delivered = std::collections::BTreeSet::new();
        while delivered.len() < paths.len() {
            let reply = receive(&worker);
            if reply.request.id != RequestId(12) {
                continue;
            }
            let index = paths
                .iter()
                .position(|path| *path == reply.request.path)
                .unwrap();
            let numeric = reply
                .numeric_settings
                .as_ref()
                .expect("worker resolves each target's numeric defaults");
            assert!(numeric.sharpen_amount.is_some());
            let frame = reply.result.unwrap_rendered();
            assert!(worker.is_settled_and_admitted(&frame));
            if index == 0 {
                assert_eq!(numeric.dehaze.shadow, frame.artifact.dehaze_estimate);
                assert!(numeric.dehaze.shadow.is_some());
            }
            assert_eq!(frame.artifact.settings.working_scale.divisor(), 8);
            assert!(
                (frame.request.settings.exposure - (12. * 0.03 + index as f32 * 0.2)).abs() < 1e-6
            );
            assert!(delivered.insert(reply.request.path.clone()));
        }
    }

    /// Two photographs from each camera family.
    const FAMILY_FIXTURES: [&str; 8] = [
        "sd9",
        "sd14",
        "blue-parking-sign",
        "merrill-hazy-coast",
        "dp0-quattro",
        "sd-quattro",
        "sd-quattro-h",
        "dp2-quattro",
    ];

    fn captured_pixels(worker: &TestWorker, frame: &super::RenderResult) -> Vec<u8> {
        let foveonic::inspection::Content::Image(image) = &frame.artifact.content else {
            panic!("image frame")
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pixels.f32");
        worker
            .capture_image(image.as_ref().clone(), path.clone())
            .recv_timeout(std::time::Duration::from_secs(60))
            .unwrap()
            .unwrap();
        std::fs::read(path).unwrap()
    }

    /// Render warm reduced edits and native regions for every family at one width
    /// cap and processing quality.
    fn family_renders(
        width: usize,
        quality: ProcessingQuality,
    ) -> (std::collections::BTreeMap<(String, bool), Vec<u8>>, u64) {
        use std::sync::atomic::Ordering;
        let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let worker = TestWorker::new(foveonic::render::Engine::new().unwrap(), Default::default());
        worker.set_memory_target(8 * 1024 * 1024 * 1024, Default::default());
        worker.set_width_cap(width);
        let request = |name: &str, revision: u64, detail: bool| {
            let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
            settings.exposure = revision as f32 * 0.1;
            if detail {
                settings.preview_region = Some([256, 256, 768, 768]);
            }
            super::RenderRequest {
                detail,
                framing_preview: false,
                calibration: None,
                rendition: Default::default(),
                id: RequestId(revision),
                path: root.join(format!("{name}.x3f")),
                settings,
                preview_scale: if detail {
                    Scale::NATIVE
                } else {
                    Scale::new(8).unwrap()
                },
                processing_quality: quality,
                output: "final".into(),
                comparison: false,
            }
        };
        let mut pixels = std::collections::BTreeMap::new();
        for detail in [false, true] {
            for revision in [1, 2] {
                let requests: Vec<_> = FAMILY_FIXTURES
                    .iter()
                    .map(|name| request(name, revision + u64::from(detail) * 10, detail))
                    .collect();
                worker.show(requests.clone(), &[]);
                let mut frames = Vec::new();
                while frames.len() < requests.len() {
                    let reply = receive(&worker);
                    if requests
                        .iter()
                        .any(|request| request.id == reply.request.id)
                    {
                        frames.push(reply.result.unwrap_rendered());
                    }
                }
                // Captures run while the worker is idle, after every reply is received.
                if revision == 2 {
                    for frame in frames {
                        let name = frame
                            .request
                            .path
                            .file_stem()
                            .unwrap()
                            .to_string_lossy()
                            .into_owned();
                        pixels.insert((name, detail), captured_pixels(&worker, &frame));
                    }
                }
            }
        }
        (pixels, worker.checks.widest.load(Ordering::Relaxed))
    }

    /// Median milliseconds to deliver an exposure edit to every photograph in the
    /// group, and the widest admitted set, at one width cap. A warm edit reuses the
    /// group's earlier work. A fresh one renders after that work was cleared, as
    /// when the group is first opened.
    fn group_latency(width: usize, detail: bool, fresh: bool) -> (f64, u64) {
        use std::sync::atomic::Ordering;
        const TRIALS: u64 = 7;
        let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let worker = TestWorker::new(foveonic::render::Engine::new().unwrap(), Default::default());
        worker.set_memory_target(8 * 1024 * 1024 * 1024, Default::default());
        worker.set_width_cap(width);
        let requests = |revision: u64| -> Vec<super::RenderRequest> {
            FAMILY_FIXTURES
                .iter()
                .map(|name| {
                    let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
                    settings.exposure = revision as f32 * 0.05;
                    if detail {
                        settings.preview_region = Some([256, 256, 768, 768]);
                    }
                    super::RenderRequest {
                        detail,
                        framing_preview: false,
                        calibration: None,
                        rendition: Default::default(),
                        id: RequestId(revision),
                        path: root.join(format!("{name}.x3f")),
                        settings,
                        preview_scale: if detail {
                            Scale::NATIVE
                        } else {
                            Scale::new(8).unwrap()
                        },
                        processing_quality: ProcessingQuality::Speed,
                        output: "final".into(),
                        comparison: false,
                    }
                })
                .collect()
        };
        let deliver = |revision: u64| {
            if fresh {
                worker.clear();
            }
            let started = std::time::Instant::now();
            worker.show(requests(revision), &[]);
            let mut delivered = 0;
            while delivered < FAMILY_FIXTURES.len() {
                let reply = receive(&worker);
                if reply.request.id == RequestId(revision) {
                    reply.result.unwrap_rendered();
                    delivered += 1;
                }
            }
            started.elapsed().as_secs_f64() * 1000.
        };
        deliver(1);
        deliver(2);
        let mut trials: Vec<f64> = (0..TRIALS).map(|trial| deliver(3 + trial)).collect();
        trials.sort_by(f64::total_cmp);
        (
            trials[trials.len() / 2],
            worker.checks.widest.load(Ordering::Relaxed),
        )
    }

    /// Prints one line per work kind, view and width cap. `FOVEONIC_GROUP_WIDTHS`
    /// limits the caps, so a caller can sample one cap's peak memory per process.
    #[test]
    #[ignore = "timing experiment: requires GPU, an uncontended host and FOVEONIC_FIXTURE_DIR with every camera family"]
    fn group_latency_by_width() {
        let widths: Vec<usize> = std::env::var("FOVEONIC_GROUP_WIDTHS")
            .map(|widths| widths.split(',').map(|w| w.parse().unwrap()).collect())
            .unwrap_or_else(|_| vec![1, 2, 4, 8]);
        for fresh in [false, true] {
            for detail in [false, true] {
                for &width in &widths {
                    let (median, widest) = group_latency(width, detail, fresh);
                    println!(
                        "{}",
                        serde_json::json!({"work": if fresh {"fresh"} else {"warm"},
                            "view": if detail {"native_region"} else {"reduced"},
                            "width_cap": width, "widest": widest, "median_ms": median})
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Classic, Merrill, Quattro and Quattro H originals"]
    fn every_family_renders_identically_at_every_width() {
        for quality in [ProcessingQuality::Quality, ProcessingQuality::Speed] {
            let (serial, widest) = family_renders(1, quality);
            assert_eq!(widest, 1);
            for width in [2, 4, 8] {
                let (wide, widest) = family_renders(width, quality);
                assert!(widest > 1, "width {width} never ran jobs together");
                assert!(widest <= width as u64);
                assert_eq!(wide.len(), serial.len());
                for (key, pixels) in &serial {
                    assert!(
                        wide[key] == *pixels,
                        "{key:?} differs at width {width} in {quality:?}"
                    );
                }
            }
        }
    }

    fn merrill_request(
        revision: u64,
        settings: foveonic::render::Settings,
    ) -> super::RenderRequest {
        let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        super::RenderRequest {
            detail: false,
            framing_preview: false,
            calibration: None,
            rendition: crate::editor::Rendition::Current,
            id: RequestId(revision),
            path: root.join("blue-parking-sign.x3f"),
            settings,
            preview_scale: Scale::new(8).unwrap(),
            processing_quality: ProcessingQuality::Quality,
            output: "final".into(),
            comparison: false,
        }
    }

    fn last_recipe_dispatches(worker: &TestWorker) -> u64 {
        worker.checks.recipes.lock().unwrap().last().unwrap().1
    }

    /// Fit the target to what is retained now, so any further work is under pressure.
    fn tighten_target(worker: &TestWorker) {
        let retained = worker.memory_status().resources.total_bytes;
        worker.set_memory_target(retained, Default::default());
    }

    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn speculative_whole_image_waits_for_free_memory_and_keeps_the_edited_region_warm() {
        use std::sync::atomic::Ordering;
        let worker = TestWorker::new(foveonic::render::Engine::new().unwrap(), Default::default());
        worker.set_memory_target(4 * 1024 * 1024 * 1024, Default::default());
        let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
        settings.preview_region = Some([100, 100, 612, 612]);
        let refine = |revision: u64, settings: &foveonic::render::Settings| {
            let mut first = merrill_request(revision, settings.clone());
            first.detail = true;
            first.preview_scale = Scale::NATIVE;
            // Without background completion only the patch is in use.
            worker.plan_images(vec![(first, false)]);
            let reply = receive(&worker);
            assert_eq!(reply.request.id, RequestId(revision));
            reply.result.unwrap_rendered();
        };
        refine(1, &settings);
        settings.exposure = 0.25;
        refine(3, &settings);
        let warm = last_recipe_dispatches(&worker);
        let loads = worker.checks.source_loads.load(Ordering::Relaxed);

        // The whole image needs more than the free memory, so it must not run.
        tighten_target(&worker);
        settings.exposure = 0.5;
        let mut first = merrill_request(5, settings.clone());
        first.detail = true;
        first.preview_scale = Scale::NATIVE;
        let mut whole = first.clone();
        whole.id = RequestId(4);
        whole.detail = false;
        whole.settings.preview_region = None;
        worker.plan_images(vec![(first, false), (whole, true)]);
        loop {
            let reply = receive(&worker);
            assert_ne!(
                reply.request.id.0, 4,
                "speculative work ran without free memory"
            );
            if reply.request.id == RequestId(5) {
                reply.result.unwrap_rendered();
                break;
            }
        }
        assert_eq!(worker.checks.source_loads.load(Ordering::Relaxed), loads);
        assert_eq!(
            last_recipe_dispatches(&worker),
            warm,
            "speculative work must not cost the edited region its restart inputs"
        );
    }

    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn before_render_keeps_the_current_edit_warm() {
        use std::sync::atomic::Ordering;
        let worker = TestWorker::new(foveonic::render::Engine::new().unwrap(), Default::default());
        worker.set_memory_target(4 * 1024 * 1024 * 1024, Default::default());
        let camera = foveonic::render::Settings::new("Sunlight", "Standard");
        let mut current = camera.clone();
        current.reconstruction = Some(Default::default());
        let render = |request: super::RenderRequest| {
            let revision = request.id;
            worker.submit(request);
            let reply = receive(&worker);
            assert_eq!(reply.request.id, revision);
            reply.result.unwrap_rendered();
        };
        render(merrill_request(1, current.clone()));
        current.exposure = 0.25;
        render(merrill_request(2, current.clone()));
        let warm = last_recipe_dispatches(&worker);
        let loads = worker.checks.source_loads.load(Ordering::Relaxed);

        tighten_target(&worker);
        let mut before = merrill_request(3, camera);
        before.rendition = crate::editor::Rendition::Camera;
        render(before);

        current.exposure = 0.5;
        render(merrill_request(4, current));
        assert_eq!(worker.checks.source_loads.load(Ordering::Relaxed), loads);
        assert_eq!(
            last_recipe_dispatches(&worker),
            warm,
            "a Before render must not cost the current edit its restart inputs"
        );
    }
}

#[cfg(test)]
mod color_pick_tests {
    use super::tests::TestWorker;
    use super::*;
    use crate::rendering::request::RequestId;
    #[test]
    #[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
    fn original_color_pick_reuses_loaded_photo_and_keeps_creative_hsl_out_of_the_guide() {
        let path = PathBuf::from(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap());
        let source = foveonic::source::Source::open(&path).unwrap();
        let caps = foveonic::render::ControlCapabilities::from_metadata(&source.metadata).unwrap();
        let mut settings = caps
            .editing_defaults(&source.metadata, foveonic::render::Rendering::Color)
            .unwrap();
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
        let worker = TestWorker::new(Engine::new().unwrap(), egui::Context::default());
        worker.submit(RenderRequest {
            detail: false,
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            id: RequestId(1),
            path: path.clone(),
            settings: settings.clone(),
            preview_scale: Scale::NATIVE,
            processing_quality: ProcessingQuality::Quality,
            output: "final".into(),
            comparison: false,
        });
        loop {
            if let Update::Image(reply) = worker
                .replies
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap()
            {
                assert!(reply.result.rendered().is_some());
                break;
            }
        }
        let mut expected = None;
        for token in 1..=5 {
            if token == 2 {
                settings.photographic.color_editor.master.hue = 0.8;
                settings.photographic.color_editor.master.lightness = 0.7;
            }
            if token == 3 {
                settings.working_scale = Scale::new(2).unwrap();
            }
            if token == 4 {
                settings.white_balance.clear();
                settings.color_mode.clear();
            }
            if token == 5 {
                settings.rendering = foveonic::render::Rendering::Monochrome;
            }
            let began = std::time::Instant::now();
            worker.pick_color(
                ColorPickRequest {
                    tool: ColorPickTool::Basic,
                    token,
                    path: path.clone(),
                    settings: settings.clone(),
                    uv: [0.5, 0.5],
                },
                RequestId(1),
                None,
            );
            let reply = loop {
                if let Update::ColorPick { reply, .. } = worker
                    .replies
                    .recv_timeout(std::time::Duration::from_secs(30))
                    .unwrap()
                {
                    break reply;
                }
            };
            assert_eq!(reply.token, token);
            assert_eq!(reply.path, path);
            eprintln!(
                "color pick {token}: {:.1} ms",
                began.elapsed().as_secs_f64() * 1000.
            );
            let weights = reply
                .sample
                .unwrap_or_else(|error| panic!("pick {token}: {error}"))
                .weights;
            assert!(
                (weights.iter().sum::<f32>() - 1.).abs() < 1e-5,
                "{weights:?}"
            );
            if token == 2 {
                assert_eq!(expected.unwrap(), weights);
            } else if token == 1 {
                expected = Some(weights);
            }
        }
    }
}
