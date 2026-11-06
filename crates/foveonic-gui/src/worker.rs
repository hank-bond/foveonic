//! Keeps rendering off the UI thread and replaces outdated requests instead of queuing edits.

#[path = "worker_presentation.rs"]
pub(crate) mod presentation;
#[path = "worker_reduced.rs"]
mod reduced;

#[cfg(test)]
use crate::export::ExportFormat;
use crate::{
    export::{self, resolve_camera_settings, Export},
    preferences::PreviewMode,
};
use eframe::egui;
#[cfg(test)]
use foveonic::source::Source;
use foveonic::{
    image::{ImageTexture, RenderedImage},
    inspection::{Artifact, Content},
    profile::Profile,
    render::{Engine, RenderSession, Scale, Settings},
    source::Metadata,
};
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{mpsc, Arc, Condvar, Mutex},
    thread,
};

#[cfg(test)]
#[derive(Default)]
struct Checks {
    source_loads: std::sync::atomic::AtomicU64,
    recipes: Mutex<Vec<(u64, u64)>>,
}
#[cfg(test)]
thread_local! { static CHECKS: std::cell::RefCell<Option<Arc<Checks>>> = const { std::cell::RefCell::new(None) }; }

#[derive(Clone)]
pub struct Request {
    pub framing_preview: bool,
    pub calibration: Option<Arc<foveonic::noise_calibration::Calibration>>,
    pub rendition: crate::editor::Rendition,
    pub revision: u64,
    pub path: PathBuf,
    pub settings: Settings,
    pub preview_quality: PreviewMode,
    /// Workspace choice, independent of Smart refinement passes.
    pub preview_policy: PreviewMode,
    pub output: String,
    pub comparison: bool,
    pub export: Option<Export>,
}
fn effective_working_scale(request: &Request) -> Scale {
    if request.export.is_some()
        || request.output != "final"
        || request.comparison
        || request.settings.preview_region.is_some()
    {
        Scale::NATIVE
    } else {
        request.preview_quality.working_scale()
    }
}
pub(crate) fn same_pixel_request(a: &Request, b: &Request) -> bool {
    presentation::same_context(a, b)
        && presentation::same_pixels(&a.settings, &b.settings)
        && (presentation::presentation_change(a, b)
            || (a.preview_quality == b.preview_quality
                && a.settings.preview_region == b.settings.preview_region))
}
pub(crate) fn same_delivery_request(a: &Request, b: &Request) -> bool {
    same_pixel_request(a, b)
        && !presentation::presentation_change(a, b)
        && a.settings == b.settings
        && a.preview_quality == b.preview_quality
}

fn resolve_preview(request: &Request, settings: &mut Settings, metadata: &Metadata) {
    settings.working_scale = effective_working_scale(request);
    if request.preview_quality != PreviewMode::Smart
        || request.export.is_some()
        || request.comparison
        || request.output != "final"
        || !settings.regional_preview_available(metadata)
    {
        settings.preview_region = None;
    }
}

pub struct Frame {
    pub origin: foveonic::source::SourceOrigin,
    pub framing_preview: bool,
    native_extent: [u32; 2],
    pub calibration_signature: Option<String>,
    pub rendition: crate::editor::Rendition,
    pub requested: Settings,
    pub path: PathBuf,
    pub output: String,
    pub artifact: Artifact,
    pub texture: Arc<ImageTexture>,
    pub comparison: Option<Comparison>,
    pub milliseconds: f64,
    pub revision: u64,
    pub queued_at: std::time::Instant,
}
impl Frame {
    /// Keep zoom coordinates native while Fast stores fewer pixels.
    pub fn display_extent(&self, input: bool) -> [u32; 2] {
        let [l, t, r, b] = self.display_window(input);
        [r - l, b - t]
    }
    pub fn camera_extent(&self) -> [u32; 2] {
        self.native_extent
    }
    pub fn uncropped_extent(&self) -> [u32; 2] {
        self.requested.rotation.extent(self.native_extent)
    }
    pub fn editing_extent(&self) -> [u32; 2] {
        self.requested.rotation.oriented_extent(self.native_extent)
    }
    /// Native oriented bounds represented by the main texture, including crop offsets.
    pub fn display_window(&self, input: bool) -> [u32; 4] {
        if self.framing_preview {
            let extent = self.editing_extent();
            return [0, 0, extent[0], extent[1]];
        }
        if self.output == "final"
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
        match self.artifact.settings.working_scale.divisor() {
            1 => "Full".into(),
            2 => "Fast".into(),
            4 => "Draft".into(),
            8 => "Thumb".into(),
            divisor => format!("1/{divisor}"),
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
    pub stamp: crate::render_coordinator::Stamp,
    pub path: PathBuf,
    pub rendition: crate::editor::Rendition,
    pub revision: u64,
    pub metadata: Option<Arc<Metadata>>,
    pub settings: Settings,
    pub result: Result<Option<Frame>, String>,
    pub numeric_settings: Option<Settings>,
    pub reset_settings: Option<Settings>,
    pub exported: Option<crate::export::Export>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeKey {
    pub revision: u64,
    pub region: Option<foveonic::scopes::Region>,
}
pub struct ScopeRequest {
    pub key: ScopeKey,
    pub image: RenderedImage,
}
pub struct ScopeSnapshot {
    pub key: ScopeKey,
    pub data: foveonic::scopes::Scopes,
}

pub enum Update {
    Image(Box<Reply>),
    ColorPick {
        id: crate::render_coordinator::TaskId,
        reply: ColorPickReply,
    },
    Scopes {
        id: crate::render_coordinator::TaskId,
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
struct Pending {
    memory: Option<MemoryBudget>,
    plan: Arc<crate::render_coordinator::WorkPlan>,
    export: Option<Arc<crate::render_coordinator::Task>>,
    activity: Option<foveonic::decode_cache::Activity>,
    stop: bool,
    clear: bool,
    clear_all: bool,
}
struct MemoryBudget {
    limit: u64,
    frontend: crate::memory::Inventory,
}
impl Default for MemoryBudget {
    fn default() -> Self {
        Self {
            limit: crate::preferences::DEFAULT_MEMORY_CACHE_BYTES,
            frontend: Default::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryStatus {
    pub resources: foveonic::render::MemoryResources,
    pub over_budget_bytes: u64,
}
pub struct Worker {
    #[cfg(test)]
    checks: Arc<Checks>,
    pending: Arc<(Mutex<Pending>, Condvar)>,
    pub replies: mpsc::Receiver<Update>,
    pub progress: crate::progress::Tracker,
    pub cache: Arc<foveonic::decode_cache::Cache>,
    context: egui::Context,
    memory: Arc<Mutex<MemoryStatus>>,
    coordinator: RefCell<crate::render_coordinator::RenderCoordinator>,
}
impl Worker {
    pub fn new(engine: Engine, context: egui::Context) -> Self {
        let pending = Arc::new((
            Mutex::new(Pending {
                memory: None,
                plan: Arc::default(),
                export: None,
                activity: None,
                stop: false,
                clear: false,
                clear_all: false,
            }),
            Condvar::new(),
        ));
        let progress = crate::progress::Tracker::default();
        let progress_updates = progress.clone();
        let cache = Arc::new(foveonic::decode_cache::Cache::new());
        let decoded_cache = cache.clone();
        let repaint = context.clone();
        let (tx, replies) = mpsc::sync_channel(2);
        let slot = pending.clone();
        let memory = Arc::new(Mutex::new(MemoryStatus::default()));
        let memory_updates = memory.clone();
        #[cfg(test)]
        let checks = Arc::new(Checks::default());
        #[cfg(test)]
        let observations = checks.clone();
        thread::spawn(move || {
            #[cfg(test)]
            CHECKS.with(|slot| *slot.borrow_mut() = Some(observations.clone()));
            let mut engine = engine;
            let mut source: Option<(PathBuf, RenderSession)> = None;
            let mut reduced = reduced::Photos::default();
            let mut presented = presentation::Presentation::default();
            let mut budget = MemoryBudget::default();
            let mut pending_images = Vec::<crate::memory::PendingImage>::new();
            reduced.configure_memory(budget.limit);
            let mut cursor = crate::render_coordinator::Cursor::default();
            let mut previous: Option<crate::render_coordinator::Stamp> = None;
            'worker: loop {
                let memory_update = slot.0.lock().unwrap().memory.take();
                if let Some(next) = memory_update {
                    budget = next;
                    trim_memory(
                        &engine,
                        &mut source,
                        &mut reduced,
                        &presented,
                        &budget,
                        &mut pending_images,
                        &memory_updates,
                        &context,
                        &slot,
                    );
                }
                let task = {
                    let (lock, wake) = &*slot;
                    let mut state = lock.lock().unwrap();
                    loop {
                        if state.stop {
                            break 'worker;
                        }
                        if state.clear {
                            state.clear = false;
                            let all = std::mem::take(&mut state.clear_all);
                            drop(state);
                            if let Some((_, previous)) = source.take() {
                                engine.release_native_preparation(previous.metadata());
                            }
                            presented.clear();
                            previous = None;
                            cursor = Default::default();
                            if all {
                                reduced.clear(&engine);
                            }
                            trim_memory(
                                &engine,
                                &mut source,
                                &mut reduced,
                                &presented,
                                &budget,
                                &mut pending_images,
                                &memory_updates,
                                &context,
                                &slot,
                            );
                            continue 'worker;
                        }
                        if let Some(pick_task) = cursor.pick(&state.plan) {
                            let pick = &pick_task.request;
                            let _activity = decoded_cache.foreground();
                            drop(state);
                            let cancel = || {
                                let state = slot.0.lock().unwrap();
                                state.stop
                                    || state.clear
                                    || state
                                        .plan
                                        .pick
                                        .as_ref()
                                        .is_none_or(|current| current.id != pick_task.id)
                            };
                            let sample = (|| {
                                if cancel() {
                                    return Err("color selection canceled".into());
                                }
                                reduced.park_active(&engine, 0);
                                load_native_source(
                                    &decoded_cache,
                                    &engine,
                                    &mut source,
                                    &pick.path,
                                    0,
                                    |_| {},
                                )?;
                                if pick_task.origin.as_ref().is_some_and(|origin| {
                                    origin != source.as_ref().unwrap().1.source().origin()
                                }) {
                                    return Err("photo changed while selecting a color".into());
                                }
                                pick_color(&engine, source.as_mut(), pick, cancel)
                            })()
                            .map_err(
                                |error: Box<dyn std::error::Error + Send + Sync>| error.to_string(),
                            );
                            trim_memory(
                                &engine,
                                &mut source,
                                &mut reduced,
                                &presented,
                                &budget,
                                &mut pending_images,
                                &memory_updates,
                                &context,
                                &slot,
                            );
                            if tx
                                .send(Update::ColorPick {
                                    id: pick_task.id,
                                    reply: ColorPickReply {
                                        tool: pick.tool,
                                        token: pick.token,
                                        path: pick.path.clone(),
                                        sample,
                                    },
                                })
                                .is_err()
                            {
                                break 'worker;
                            }
                            context.request_repaint();
                            continue 'worker;
                        }
                        let next = state
                            .export
                            .take()
                            .or_else(|| {
                                previous
                                    .as_ref()
                                    .and_then(|prior| cursor.followup(&state.plan, prior))
                            })
                            .or_else(|| cursor.next(&state.plan));
                        if let Some(task) = next {
                            break task;
                        }
                        if let Some(scopes) = cursor.scope(&state.plan) {
                            let _activity = decoded_cache.foreground();
                            drop(state);
                            if !analyze_scopes(&engine, &slot, &tx, &context, scopes) {
                                break 'worker;
                            }
                            trim_memory(
                                &engine,
                                &mut source,
                                &mut reduced,
                                &presented,
                                &budget,
                                &mut pending_images,
                                &memory_updates,
                                &context,
                                &slot,
                            );
                            continue 'worker;
                        }
                        if state.memory.is_some() {
                            continue 'worker;
                        }
                        state.activity = None;
                        state = wake.wait(state).unwrap();
                    }
                };
                let _activity = decoded_cache.foreground();
                let request = &task.request;
                let queued_at = task.queued_at;
                if let Some(seed) = &task.seed {
                    presented.seed(seed);
                }
                let prepare_defaults = matches!(
                    task.stamp.target.purpose,
                    crate::render_coordinator::Purpose::Current
                ) && request.preview_quality == PreviewMode::Thumb;
                previous = Some(task.stamp.clone());
                progress_updates.begin(request.revision, queued_at);
                let reporter =
                    crate::progress::Reporter::new(progress_updates.clone(), request.revision);
                let started = std::time::Instant::now();
                crate::timing::worker_started(request.revision, queued_at, started);
                engine.set_noise_calibration(request.calibration.clone());
                let mut metadata = None;
                let mut settings = request.settings.clone();
                let mut defaults_errors = Vec::new();
                let result = (|| -> foveonic::source::Result<Option<Frame>> {
                    let composition_quality = presented.quality_for_view_change(request, &decoded_cache);
                    let mut composed = presented.prepare(request, &decoded_cache)?;
                    let reused_display = composed.as_ref().and_then(|composed| composed.display.clone());
                    let reusing_pixels = composed.is_some();
                    let mut reduced_loaded = if !reusing_pixels && reduced::Photos::requested(request)
                        && composition_quality.is_none_or(|scale| scale.is_reduced()) {
                        Some(reduced.load(&request.path, request.revision, &decoded_cache, &engine, &mut source, &|stage| reporter.loaded(stage))?)
                    } else { None };
                    if !reusing_pixels && reduced_loaded.is_none() {
                        reduced.park_active(&engine, request.revision);
                        load_native_source(&decoded_cache, &engine, &mut source, &request.path, request.revision, |stage| reporter.loaded(stage))?;
                    }
                    // Cached inputs have already completed both loading sections.
                    reporter.loaded(foveonic::source::LoadStage::Decoded);
                    metadata = Some(if let Some(composed) = &composed { composed.metadata.clone() }
                        else if let Some(loaded) = &reduced_loaded { loaded.metadata.clone() }
                        else { source.as_ref().unwrap().1.metadata().clone() });
                    let origin = if let Some(composed) = &composed { composed.origin.clone() }
                        else if let Some(loaded) = &reduced_loaded { loaded.origin().clone() }
                        else { source.as_ref().unwrap().1.source().origin().clone() };
                    defaults_errors = resolve_camera_settings(&mut settings, metadata.as_ref().unwrap());
                    if let Some(composed) = &composed {
                        settings.working_scale = composed.quality;
                        settings.preview_region = None;
                    } else {
                        resolve_preview(request, &mut settings, metadata.as_ref().unwrap());
                        if let Some(quality) = composition_quality { settings.working_scale = quality; settings.preview_region = None; }
                    }
                    if let Some(export) = request.export.as_ref() {
                        let mut profile=Profile::new(crate::timing::mode());
                        let before=crate::timing::enabled().then(||engine.gpu_activity());
                        export::write(&engine, &mut source.as_mut().unwrap().1, &settings, export, &mut profile, |stage| reporter.rendered(stage))?;
                        if let Some(before)=before {crate::timing::emit("export",serde_json::json!({"revision":request.revision,"source":request.path,"destination":export.path,"format":export.format.extension(),"settings":settings,"profile":profile,"activity":engine.gpu_activity().since(before)}));}
                        reporter.rendered(foveonic::render::RenderProgress::Stage { completed: 1, total: 1 });
                        return Ok(None);
                    }
                    settings.dehaze_estimate = request.output == "final"
                        && request.rendition == crate::editor::Rendition::Current
                        && settings.dehaze_auto();
                    let native_history = source.as_ref().filter(|(path, _)| path == &request.path);
                    let edit = if reusing_pixels { Vec::new() } else if let Some(loaded) = &reduced_loaded {
                        reduced.changed_parameters(loaded, &settings)
                            .or_else(|| native_history.map(|(_, session)| session.changed_parameters(&settings)))
                            .unwrap_or_default()
                    } else { source.as_ref().unwrap().1.changed_parameters(&settings) };
                    let cancel=||{
                        if request.export.is_some(){return false;}
                        let state=slot.0.lock().unwrap();
                        if state.clear || state.stop { return true; }
                        let Some(desired) = state.plan.task(&task.stamp.target) else { return true; };
                        if task.origin.is_some() && desired.origin.is_some() && task.origin != desired.origin { return true; }
                        if desired.stamp.id == task.stamp.id { return false; }
                        let pending = &desired.request;
                        let mut next=pending.settings.clone();
                        resolve_camera_settings(&mut next,metadata.as_ref().unwrap());
                        resolve_preview(pending, &mut next, metadata.as_ref().unwrap());
                        if presentation::same_context(request, pending) && presentation::presentation_change(request, pending)
                            && presentation::same_pixels(&settings, &next) { return false; }
                        !presentation::same_context(request, pending) || settings.working_scale!=next.working_scale || settings.preview_region!=next.preview_region
                            || (!edit.is_empty() && settings.changed_parameters(&next).iter().any(|parameter| !edit.contains(parameter)))
                    };
                    let mut profile = Profile::new(crate::timing::mode());
                    let before = crate::timing::enabled().then(||engine.gpu_activity());
                    let cache_before = before.map(|_|if reusing_pixels { 0 } else if let Some(loaded) = &reduced_loaded { reduced.cached_bytes(loaded) }
                        else { source.as_ref().unwrap().1.cached_bytes() });
                    let mut used_reduced = false;
                    let root=if request.export.is_some(){"final"}else{&request.output};
                    let input = if request.comparison && request.export.is_none() {
                        Some(foveonic::inspection::comparison_input(&settings, root, metadata.as_deref())
                            .ok_or("selected output has no operation-input comparison")?)
                    } else {
                        None
                    };
                    let mut roots = vec![root];
                    if let Some(input) = input { roots.push(input); }
                    #[cfg(test)]
                    let recipe_before = engine.gpu_activity();
                    let rendered = {
                        // Timing a host span must not add a GPU wait and change the
                        // responsiveness we are trying to measure.
                        let _span = crate::timing::Span::new(request.revision, "processing");
                        if let Some(composed) = composed.take() {
                            reporter.rendered(foveonic::render::RenderProgress::Prepared);
                            reporter.rendered(foveonic::render::RenderProgress::Stage { completed: 1, total: 1 });
                            Some(composed.artifacts)
                        } else if let Some(loaded) = reduced_loaded.take() {
                            used_reduced = true;
                            reduced.render(&engine, &decoded_cache, &mut source, loaded, &settings, &cancel, &mut profile,
                                &|stage| reporter.loaded(stage), &|stage| reporter.rendered(stage))?
                        } else {
                            engine.render_session_with_progress(&mut source.as_mut().unwrap().1, &settings, &roots, cancel, &mut profile, |stage| reporter.rendered(stage))?
                        }
                    };
                    #[cfg(test)]
                    observations.recipes.lock().unwrap().push((request.revision, engine.gpu_activity().since(recipe_before).dispatches));
                    if let Some(before) = before {
                        crate::timing::emit("processing", serde_json::json!({"revision":request.revision,"source":request.path,"output":root,"outputs":roots,"settings":settings,"execution":if reusing_pixels {"presentation"} else if used_reduced {"reduced-session"} else {"session"},"status":if rendered.is_some(){"complete"}else{"superseded"},"cache_before_bytes":cache_before,"cache_after_bytes":if reusing_pixels {0} else if used_reduced {reduced.cached_bytes_for_path(&request.path)} else {source.as_ref().unwrap().1.cached_bytes()},"profile":profile,"activity":engine.gpu_activity().since(before)}));
                    }
                    let mut profile = Profile::new(crate::timing::mode());
                    let Some(mut artifacts) = rendered else { return Ok(None); };
                    if cancel(){return Ok(None);}
                    let retain_whole = presented.retain(request, &settings, &artifacts, &origin);
                    let view_before = crate::timing::enabled().then(||engine.gpu_activity());
                    let comparison = if let Some(input) = input {
                        let baseline = artifacts.pop().ok_or("missing comparison input")?;
                        let Content::Image(image) = baseline.content else {
                            return Err("comparison input must be an image".into());
                        };
                        let _span = crate::timing::Span::new(request.revision, "comparison_view_prepare_host");
                        Some(Comparison {
                            input,
                            texture: if let Some(display) = &reused_display { display.comparison.as_ref().unwrap().texture.clone() }
                                else { Arc::new(engine.view_texture(&image, None, &mut profile)?) },
                        })
                    } else { None };
                    let artifact = artifacts.pop().ok_or("missing requested artifact")?;
                    let Content::Image(image) = &artifact.content else {
                        return Err(
                            "GUI inspection requires a visual image buffer; node-data diagnostics are deferred"
                                .into(),
                        );
                    };
                    if cancel(){return Ok(None);}
                    let native_extent=if root=="final" {foveonic::render::native_extent(&artifact.metadata)?} else {image.uncropped_view().extent};
                    let texture = {
                        let _span = crate::timing::Span::new(request.revision, "view_prepare_host");
                        if let Some(display) = &reused_display { display.texture.clone() } else {
                            let uncropped=request.framing_preview.then(||image.without_composition());
                            Arc::new(engine.view_texture(uncropped.as_ref().unwrap_or(image.as_ref()), None, &mut profile)?)
                        }
                    };
                    if comparison.as_ref().is_some_and(|c| c.texture.description().extent != texture.description().extent) {
                        return Err("comparison views have different geometry".into());
                    }
                    if reused_display.is_none() {
                        let _span = crate::timing::Span::new(request.revision, "view_completion_wait");
                        engine.device().poll(eframe::wgpu::Maintain::Wait);
                    }
                    if let Some(before)=view_before{crate::timing::emit("view",serde_json::json!({"revision":request.revision,"source":request.path,"output":root,"profile":profile,"activity":engine.gpu_activity().since(before)}));}
                    if retain_whole { presented.display(texture.clone(), comparison.clone(), request.framing_preview); }
                    Ok(Some(Frame {
                        origin: origin.clone(),
                        framing_preview: request.framing_preview,
                        native_extent,
                        calibration_signature: request.calibration.as_ref().map(|p| p.signature().to_owned()),
                        rendition: request.rendition,
                        requested: request.settings.clone(),
                        path: request.path.clone(),
                        output: request.output.clone(),
                        artifact,
                        texture,
                        comparison,
                        milliseconds: started.elapsed().as_secs_f64() * 1000.,
                        revision:request.revision,
                        queued_at,
                    }))
                })()
                .map_err(|e| {
                    if defaults_errors.is_empty() {
                        e.to_string()
                    } else {
                        format!(
                            "{e}; {}. Choose a preset in Processing.",
                            defaults_errors.join("; ")
                        )
                    }
                });
                // Reply settings populate photo edits. Preview policy stays in UI preferences.
                settings.working_scale = Scale::NATIVE;
                settings.preview_region = None;
                settings.dehaze_estimate = false;
                let resolved_numeric = if prepare_defaults {
                    metadata.as_deref().and_then(|metadata| {
                        crate::multi_edit_runtime::numeric_settings(&settings, metadata).ok()
                    })
                } else {
                    None
                };
                let (mut numeric_settings, mut reset_settings) = resolved_numeric
                    .map_or((None, None), |(numeric, reset)| {
                        (Some(numeric), Some(reset))
                    });
                if let Some(numeric) = &mut numeric_settings {
                    if numeric.dehaze.shadow.is_none() {
                        numeric.dehaze.shadow = result
                            .as_ref()
                            .ok()
                            .and_then(Option::as_ref)
                            .and_then(|frame| frame.artifact.dehaze_estimate);
                    }
                }
                if let Some(defaults) = &mut reset_settings {
                    defaults.dehaze.shadow = result
                        .as_ref()
                        .ok()
                        .and_then(Option::as_ref)
                        .and_then(|frame| frame.artifact.dehaze_estimate);
                }
                let publication = crate::timing::Span::new(request.revision, "reply_publication");
                if let Ok(Some(frame)) = &result {
                    pending_images.push(crate::memory::PendingImage::new(frame));
                }
                trim_memory(
                    &engine,
                    &mut source,
                    &mut reduced,
                    &presented,
                    &budget,
                    &mut pending_images,
                    &memory_updates,
                    &context,
                    &slot,
                );
                crate::timing::event(request.revision, "reply_ready");
                if tx
                    .send(Update::Image(Box::new(Reply {
                        stamp: task.stamp.clone(),
                        path: request.path.clone(),
                        rendition: request.rendition,
                        revision: request.revision,
                        metadata,
                        settings,
                        numeric_settings,
                        reset_settings,
                        result,
                        exported: request.export.clone(),
                    })))
                    .is_err()
                {
                    break;
                }
                drop(publication);
                crate::timing::event(request.revision, "reply_published");
                context.request_repaint();
            }
        });
        Self {
            #[cfg(test)]
            checks,
            pending,
            replies,
            progress,
            cache,
            context: repaint,
            memory,
            coordinator: RefCell::default(),
        }
    }
    fn publish(&self) {
        let plan = self.coordinator.borrow().plan();
        let (lock, wake) = &*self.pending;
        let mut pending = lock.lock().unwrap();
        pending.activity = (!plan.tasks.is_empty()
            || plan.scopes.is_some()
            || plan.pick.is_some()
            || pending.export.is_some())
        .then(|| self.cache.foreground());
        pending.plan = plan;
        wake.notify_one();
        drop(pending);
        self.context.request_repaint();
    }
    pub fn activate(&self, path: &std::path::Path) {
        self.coordinator.borrow_mut().activate(path);
        self.publish();
    }
    pub fn pick_color(
        &self,
        request: ColorPickRequest,
        image_revision: u64,
        origin: Option<foveonic::source::SourceOrigin>,
    ) {
        self.coordinator
            .borrow_mut()
            .pick(request, image_revision, origin);
        self.publish();
    }
    pub fn set_memory_budget(&self, limit: u64, frontend: crate::memory::Inventory) {
        let (lock, wake) = &*self.pending;
        lock.lock().unwrap().memory = Some(MemoryBudget { limit, frontend });
        wake.notify_one();
    }
    pub fn memory_status(&self) -> MemoryStatus {
        *self.memory.lock().unwrap()
    }
    pub fn retire_before(&self, path: &std::path::Path) {
        self.coordinator.borrow_mut().retire_before(path);
        self.publish();
    }
    pub fn clear(&self) {
        self.clear_with_cache(false);
    }
    pub fn clear_all(&self) {
        self.clear_with_cache(true);
    }
    fn clear_with_cache(&self, all: bool) {
        self.progress.clear();
        self.coordinator.borrow_mut().clear();
        self.publish();
        let (lock, wake) = &*self.pending;
        let mut state = lock.lock().unwrap();
        state.clear = true;
        state.clear_all |= all;
        wake.notify_one();
    }
    pub fn submit_scopes(&self, request: ScopeRequest) {
        self.coordinator.borrow_mut().scopes(request);
        self.publish();
    }
    pub fn submit(&self, request: Request) {
        self.submit_at(request, std::time::Instant::now());
    }
    pub fn submit_at(&self, request: Request, queued_at: std::time::Instant) {
        let origin = self.cache.source_origin(&request.path);
        if request.export.is_some() {
            self.progress.begin(request.revision, queued_at);
            let task =
                self.coordinator
                    .borrow_mut()
                    .frozen_export(Arc::new(request), origin, queued_at);
            self.pending.0.lock().unwrap().export = Some(task);
        } else {
            let mut coordinator = self.coordinator.borrow_mut();
            let target = crate::render_coordinator::Target::of(&request);
            let previous = coordinator.desired(&target).map(|task| task.stamp.clone());
            let revision = request.revision;
            coordinator.desire(Arc::new(request), origin, queued_at);
            if coordinator.desired(&target).map(|task| &task.stamp) != previous.as_ref() {
                self.progress.begin(revision, queued_at);
            }
        }
        self.publish();
    }
    pub fn set_overviews(&self, requests: Vec<Request>, retained: &[PathBuf]) {
        let paths = requests
            .iter()
            .map(|request| request.path.clone())
            .collect::<Vec<_>>();
        let queued_at = std::time::Instant::now();
        {
            let mut coordinator = self.coordinator.borrow_mut();
            coordinator.retain_overviews(&paths, retained);
            for request in requests {
                let origin = self.cache.source_origin(&request.path);
                coordinator.desire(Arc::new(request), origin, queued_at);
            }
        }
        self.publish();
    }
    pub fn retain_overviews(&self, paths: &[PathBuf], retained: &[PathBuf]) {
        self.coordinator
            .borrow_mut()
            .retain_overviews(paths, retained);
        self.publish();
    }
    pub fn busy(&self) -> bool {
        self.coordinator.borrow().busy()
    }
    pub fn finish_image(
        &self,
        stamp: &crate::render_coordinator::Stamp,
    ) -> crate::render_coordinator::Completion {
        let result = self.coordinator.borrow_mut().finish(stamp);
        self.publish();
        result
    }
    pub fn admit_image(&self, stamp: &crate::render_coordinator::Stamp, frame: &Frame) -> bool {
        let admitted = {
            let mut coordinator = self.coordinator.borrow_mut();
            if coordinator
                .desired(&stamp.target)
                .and_then(|task| task.origin.as_ref())
                .is_some_and(|origin| origin != &frame.origin)
            {
                return false;
            }
            coordinator.admit(stamp)
        };
        self.publish();
        admitted
    }
    pub fn retain_pixels(&self, stamp: &crate::render_coordinator::Stamp, frame: &Frame) {
        self.coordinator.borrow_mut().note_pixels(stamp, frame);
        self.publish();
    }
    pub fn finish_scope(&self, id: crate::render_coordinator::TaskId) -> bool {
        let current = self.coordinator.borrow_mut().finish_scope(id);
        self.publish();
        current
    }
    pub fn finish_pick(&self, id: crate::render_coordinator::TaskId) -> bool {
        let current = self.coordinator.borrow_mut().finish_pick(id);
        self.publish();
        current
    }
    pub fn resource_revision(&self) -> u64 {
        self.coordinator.borrow().resource_revision()
    }
    pub fn delivered(&self, revision: u64) {
        self.coordinator.borrow_mut().delivered(revision);
        self.publish();
    }
    pub fn memory_resources(&self, inventory: &mut crate::memory::Inventory) {
        self.coordinator.borrow().memory_resources(inventory);
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.pending;
        lock.lock().unwrap().stop = true;
        wake.notify_one();
    }
}

#[allow(clippy::too_many_arguments)]
fn trim_memory(
    engine: &Engine,
    source: &mut Option<(PathBuf, RenderSession)>,
    reduced: &mut reduced::Photos,
    presented: &presentation::Presentation,
    budget: &MemoryBudget,
    pending_images: &mut Vec<crate::memory::PendingImage>,
    updates: &Arc<Mutex<MemoryStatus>>,
    context: &egui::Context,
    slot: &Arc<(Mutex<Pending>, Condvar)>,
) {
    let mut inventory = budget.frontend.clone();
    if let Some(scopes) = &slot.0.lock().unwrap().plan.scopes {
        inventory.image(&scopes.request.image);
    }
    presented.memory_resources(&mut inventory);
    pending_images.retain(|image| image.reserve(&budget.frontend, &mut inventory));
    let trim = reduced.trim_memory(
        engine,
        source.as_mut().map(|(_, session)| session),
        budget.limit,
        inventory.reservation(),
    );
    let status = MemoryStatus {
        resources: trim.resources,
        over_budget_bytes: trim.over_budget_bytes,
    };
    let changed = {
        let mut previous = updates.lock().unwrap();
        let changed = *previous != status;
        *previous = status;
        changed
    };
    if changed {
        if crate::timing::enabled() {
            crate::timing::emit(
                "memory",
                serde_json::json!({"limit_bytes": budget.limit, "resources": trim.resources, "over_budget_bytes": trim.over_budget_bytes, "evicted_photos": trim.evicted_photos}),
            );
        }
        context.request_repaint();
    }
}

fn load_native_source(
    cache: &foveonic::decode_cache::Cache,
    engine: &Engine,
    source: &mut Option<(PathBuf, RenderSession)>,
    path: &PathBuf,
    revision: u64,
    progress: impl Fn(foveonic::source::LoadStage),
) -> foveonic::source::Result<()> {
    if source.as_ref().is_some_and(|(loaded, session)| {
        loaded == path
            && cache
                .source_origin(path)
                .is_none_or(|known| known == *session.source().origin())
    }) {
        return Ok(());
    }
    if let Some((_, previous)) = source.take() {
        engine.release_native_preparation(previous.metadata());
    }
    let _span = crate::timing::Span::new(revision, "source_open");
    let loaded = cache.open(path, progress)?;
    record_source_load(revision, path, &loaded);
    *source = Some((path.clone(), RenderSession::new(loaded)));
    Ok(())
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
            serde_json::json!({"revision":revision,"source":path,"timings":loaded.load_timings(),"read_cache_state":"uncontrolled","input":if loaded.load_timings().iter().any(|t|t.stage=="decoded cache read"){"decoded-cache"}else{"original"},"extent":[loaded.metadata.width,loaded.metadata.height]}),
        );
    }
}

fn pick_color(
    engine: &Engine,
    source: Option<&mut (PathBuf, RenderSession)>,
    request: &ColorPickRequest,
    cancel: impl Fn() -> bool,
) -> foveonic::source::Result<foveonic::render::ColorRangeSample> {
    if cancel() {
        return Err("color selection canceled".into());
    }
    let Some((path, session)) = source else {
        return Err("photo is no longer loaded".into());
    };
    if *path != request.path {
        return Err("photo changed before color selection".into());
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
        .render_session(
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
    tx: &mpsc::SyncSender<Update>,
    context: &egui::Context,
    task: Arc<crate::render_coordinator::ScopeTask>,
) -> bool {
    let request = &task.request;
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
    let span = crate::timing::Span::new(request.key.revision, "scopes");
    let result = engine
        .scopes(&request.image, request.key.region, &mut profile)
        .map_err(|error| error.to_string());
    if let Some(before) = before {
        crate::timing::emit(
            "scopes",
            serde_json::json!({"revision":request.key.revision,"region":request.key.region,"profile":profile,"activity":engine.gpu_activity().since(before),"status":if result.is_ok(){"complete"}else{"error"}}),
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
    context.request_repaint();
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn coalesced_cached_requests_do_not_restart_the_elapsed_timer() {
        // Drive the real UI mailbox without a processing executor.
        let (_sender, replies) = std::sync::mpsc::sync_channel(2);
        let worker = super::Worker {
            checks: Default::default(),
            pending: std::sync::Arc::new((
                std::sync::Mutex::new(super::Pending::default()),
                std::sync::Condvar::new(),
            )),
            replies,
            progress: Default::default(),
            cache: std::sync::Arc::new(foveonic::decode_cache::Cache::new()),
            context: Default::default(),
            memory: Default::default(),
            coordinator: Default::default(),
        };
        let request = |revision, exposure| {
            let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
            settings.exposure = exposure;
            super::Request {
                framing_preview: false,
                calibration: None,
                rendition: Default::default(),
                revision,
                path: "cached.x3f".into(),
                settings,
                preview_quality: crate::preferences::PreviewMode::Thumb,
                preview_policy: crate::preferences::PreviewMode::Thumb,
                output: "final".into(),
                comparison: false,
                export: None,
            }
        };
        worker.submit(request(1, 0.));
        let stamp = worker.pending.0.lock().unwrap().plan.tasks[0].stamp.clone();
        worker.finish_image(&stamp);
        worker.progress.settle(worker.busy(), None);
        let completed = worker.progress.snapshot().unwrap();
        assert!(!completed.running());

        worker.submit(request(2, 0.));
        assert!(!worker.busy());
        let cached = worker.progress.snapshot().unwrap();
        assert!(!cached.running());
        assert_eq!(cached.revision, completed.revision);
        assert_eq!(cached.milliseconds(), completed.milliseconds());

        worker.submit(request(3, 0.2));
        assert!(worker.busy());
        let changed = worker.progress.snapshot().unwrap();
        assert!(changed.running());
        assert_eq!(changed.revision, 3);
    }

    fn receive(worker: &super::Worker) -> super::Reply {
        loop {
            if let super::Update::Image(reply) = worker
                .replies
                .recv_timeout(std::time::Duration::from_secs(20))
                .unwrap()
            {
                worker.finish_image(&reply.stamp);
                if let Ok(Some(frame)) = &reply.result {
                    worker.retain_pixels(&reply.stamp, frame);
                }
                return *reply;
            }
        }
    }
    #[test]
    #[ignore = "requires a GPU and QUATTRO_X3F ordinary full-resolution sample"]
    fn quattro_dng_export_uses_original_source_from_any_inspection_view() {
        use foveonic::{
            render::{ControlCapabilities, Rendering, Settings},
            source::Source,
        };
        let path = std::path::PathBuf::from(std::env::var_os("QUATTRO_X3F").unwrap());
        let source = Source::open(&path).unwrap();
        let capabilities = ControlCapabilities::from_metadata(&source.metadata).unwrap();
        assert_eq!(
            capabilities.family,
            foveonic::source::ProcessingFamily::Quattro
        );
        let expected_model = source.metadata.properties["CAMMODEL"].clone();
        let mut selections = Vec::new();
        for rendering in [Rendering::Color, Rendering::Monochrome] {
            let mut settings = Settings::new("", "");
            settings.rendering = rendering;
            capabilities
                .apply_capture_defaults(&mut settings, &source.metadata)
                .unwrap();
            // Exercise a non-default presentation without baking it into a GUI frame.
            if rendering == Rendering::Color {
                settings.color_mode = "Vivid".into();
            }
            settings.exposure = 0.4;
            settings.tone.shadows = 0.2;
            selections.push(settings);
        }
        drop(source);
        let directory = std::env::temp_dir().join(format!(
            "foveonic-quattro-gui-dng-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let worker = super::Worker::new(
            foveonic::render::Engine::new().unwrap(),
            eframe::egui::Context::default(),
        );
        for (index, settings) in selections.into_iter().enumerate() {
            let destination = directory.join(format!("{index}.dng"));
            worker.submit(super::Request {
                framing_preview: false,
                calibration: None,
                rendition: Default::default(),
                preview_quality: crate::preferences::PreviewMode::Full,
                preview_policy: crate::preferences::PreviewMode::Full,
                revision: index as u64 + 1,
                path: path.clone(),
                settings: settings.clone(),
                output: "sensor.blue".into(),
                comparison: true,
                export: Some(super::Export {
                    path: destination.clone(),
                    format: super::ExportFormat::Dng,
                    tiff_color_space: settings.tiff_color_space,
                    encoding: Default::default(),
                    sections: Default::default(),
                    conflict: foveonic::importing::ConflictPolicy::Rename,
                }),
            });
            let reply = loop {
                if let super::Update::Image(reply) = worker
                    .replies
                    .recv_timeout(std::time::Duration::from_secs(60))
                    .unwrap()
                {
                    break *reply;
                }
            };
            assert_eq!(reply.revision, index as u64 + 1);
            assert_eq!(
                reply.exported.map(|e| e.format),
                Some(super::ExportFormat::Dng)
            );
            assert!(reply.result.expect("Quattro DNG export").is_none());
            assert_eq!(reply.settings.rendering, settings.rendering);
            assert_eq!(reply.settings.exposure, settings.exposure);
            assert_eq!(reply.settings.tone, settings.tone);
            assert_eq!(reply.settings.monochrome, settings.monochrome);
            if !settings.is_monochrome() {
                assert_eq!(reply.settings.color_mode, settings.color_mode);
            }
            assert_eq!(
                reply.metadata.unwrap().properties["CAMMODEL"],
                expected_model
            );
            let bytes = std::fs::read(&destination).unwrap();
            assert!(bytes.len() > 1024);
            assert_eq!(&bytes[..4], b"II\x2a\0");
            std::fs::remove_file(destination).unwrap();
        }
        drop(worker);
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    #[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
    fn worker_keeps_latest_image_request_and_rejects_table_inspection() {
        let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f");
        let worker = super::Worker::new(
            foveonic::render::Engine::new().unwrap(),
            eframe::egui::Context::default(),
        );
        for revision in 1..=20 {
            worker.submit(super::Request {
                framing_preview: false,
                calibration: None,
                rendition: Default::default(),
                preview_quality: crate::preferences::PreviewMode::Full,
                preview_policy: crate::preferences::PreviewMode::Full,
                revision,
                path: path.clone(),
                settings: foveonic::render::Settings::new("", ""),
                output: "sensor.layers".into(),
                comparison: false,
                export: None,
            });
        }
        loop {
            let reply = receive(&worker);
            if reply.revision != 20 {
                continue;
            }
            let frame = reply.result.unwrap().unwrap();
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
            break;
        }
        worker.submit(super::Request {
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            preview_quality: crate::preferences::PreviewMode::Full,
            preview_policy: crate::preferences::PreviewMode::Full,
            revision: 21,
            path: path.clone(),
            settings: foveonic::render::Settings::new("", ""),
            output: "final".into(),
            comparison: false,
            export: None,
        });
        let reply = receive(&worker);
        assert_eq!(reply.revision, 21);
        let frame = reply.result.unwrap().unwrap();
        assert_eq!(frame.artifact.name, "tone.rgb"); // final aliases the last enabled recipe output
        assert!(matches!(
            frame.texture.description().space,
            foveonic::image::Space::ToneMappedRgb(_)
        ));
        assert_eq!(frame.artifact.settings.white_balance, "Sunlight");
        assert_eq!(frame.artifact.settings.color_mode, "Standard");
        worker.submit(super::Request {
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            preview_quality: crate::preferences::PreviewMode::Full,
            preview_policy: crate::preferences::PreviewMode::Full,
            revision: 22,
            path,
            settings: foveonic::render::Settings::new("", ""),
            output: "black.histogram".into(),
            comparison: false,
            export: None,
        });
        let reply = receive(&worker);
        assert_eq!(reply.revision, 22);
        assert!(reply
            .result
            .err()
            .unwrap()
            .contains("GUI inspection requires a visual image buffer"));
        worker.clear();
    }

    #[test]
    #[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
    fn comparison_frames_pair_operation_inputs_with_the_same_configuration() {
        use foveonic::{image::Space, render::Settings};
        let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f");
        let worker = super::Worker::new(
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
            ("reconstruction.rgb", "color-dq.rgb"),
            ("sharpen.rgb", "reconstruction.rgb"),
            ("saturation.rgb", "tone.rgb"),
        ]
        .into_iter()
        .enumerate()
        {
            let revision = index as u64 + 1;
            worker.submit(super::Request {
                framing_preview: false,
                calibration: None,
                rendition: Default::default(),
                preview_quality: crate::preferences::PreviewMode::Full,
                preview_policy: crate::preferences::PreviewMode::Full,
                revision,
                path: path.clone(),
                settings: settings.clone(),
                output: output.into(),
                comparison: true,
                export: None,
            });
            let reply = receive(&worker);
            assert_eq!(reply.revision, revision);
            let frame = reply.result.unwrap().unwrap();
            assert_eq!(frame.output, output);
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
        worker.submit(super::Request {
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            preview_quality: crate::preferences::PreviewMode::Full,
            preview_policy: crate::preferences::PreviewMode::Full,
            revision: 9,
            path: path.clone(),
            settings: settings.clone(),
            output: "repair.layers".into(),
            comparison: true,
            export: None,
        });
        let frame = receive(&worker).result.unwrap().unwrap();
        assert_eq!(
            frame.comparison.as_ref().unwrap().input,
            "calibration.layers"
        );
        assert_eq!(frame.texture.description().space, Space::CalibratedLayers);
        drop(frame);
        worker.submit(super::Request {
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            preview_quality: crate::preferences::PreviewMode::Full,
            preview_policy: crate::preferences::PreviewMode::Full,
            revision: 10,
            path,
            settings,
            output: "final".into(),
            comparison: true,
            export: None,
        });
        assert!(receive(&worker)
            .result
            .err()
            .unwrap()
            .contains("no operation-input comparison"));
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
        let worker = super::Worker::new(
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
            worker.submit(super::Request {
                framing_preview: false,
                calibration: None,
                rendition: Default::default(),
                preview_quality: crate::preferences::PreviewMode::Full,
                preview_policy: crate::preferences::PreviewMode::Full,
                revision,
                path: path.clone(),
                settings: settings.clone(),
                output: "final".into(),
                comparison: false,
                export: None,
            });
            let reply = receive(&worker);
            assert_eq!(reply.revision, revision);
            let frame = reply.result.unwrap().unwrap();
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
        if std::env::var_os("FOVEONIC_PROFILE").is_some() {
            crate::timing::configure().expect("run the profiled worker test with --exact");
        }
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
            let worker = super::Worker::new(
                foveonic::render::Engine::new().unwrap(),
                eframe::egui::Context::default(),
            );
            let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
            settings.reconstruction = Some(Default::default());
            settings.sharpen_amount = Some(0.75);
            let submit = |revision, settings: foveonic::render::Settings| {
                worker.submit(super::Request {
                    framing_preview: false,
                    calibration: None,
                    rendition: Default::default(),
                    preview_quality: crate::preferences::PreviewMode::Full,
                    preview_policy: crate::preferences::PreviewMode::Full,
                    revision,
                    path: path.clone(),
                    settings,
                    output: "final".into(),
                    comparison: false,
                    export: None,
                })
            };
            submit(1, settings.clone());
            assert!(receive(&worker).result.unwrap().is_some());
            // Prime this control's input boundary. The shared memory budget may
            // have evicted its optional initial frontier while retaining Exposure.
            let warmup = std::time::Instant::now();
            let initial = settings.clone();
            edit(&mut settings, control, 2);
            if settings == initial {
                edit(&mut settings, control, 3);
            }
            submit(2, settings.clone());
            assert!(receive(&worker).result.unwrap().is_some());
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
                        worker.finish_image(&reply.stamp);
                        if let Ok(Some(frame)) = &reply.result {
                            worker.retain_pixels(&reply.stamp, frame);
                        }
                        if let Some(frame) = reply.result.unwrap() {
                            let mut expected =
                                foveonic::render::Settings::new("Sunlight", "Standard");
                            expected.reconstruction = Some(Default::default());
                            expected.sharpen_amount = Some(0.75);
                            edit(&mut expected, control, reply.revision);
                            assert_eq!(frame.artifact.settings, expected);
                            progress.push(reply.revision);
                            settled |= reply.revision == last;
                            if reply.revision == last {
                                request_scopes(&worker, &frame, None);
                            }
                        }
                    } else if let super::Update::Scopes { id, key, result } = update {
                        worker.finish_scope(id);
                        if key.revision == last {
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
                    super::Update::Image(reply) if reply.revision == last => {
                        worker.finish_image(&reply.stamp);
                        if let Ok(Some(frame)) = &reply.result {
                            worker.retain_pixels(&reply.stamp, frame);
                        }
                        assert!(!settled, "unexpected extra final image");
                        let frame = reply.result.unwrap().unwrap();
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
                    super::Update::Scopes { id, key, result } if key.revision == last => {
                        worker.finish_scope(id);
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
        frame: &super::Frame,
        region: Option<foveonic::scopes::Region>,
    ) {
        let foveonic::inspection::Content::Image(image) = &frame.artifact.content else {
            panic!("expected image");
        };
        worker.submit_scopes(super::ScopeRequest {
            key: super::ScopeKey {
                revision: frame.revision,
                region,
            },
            image: (**image).clone(),
        });
    }

    #[test]
    #[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
    fn viewport_scope_jobs_reuse_the_displayed_image_and_settle_on_latest_region() {
        let engine = foveonic::render::Engine::new().unwrap();
        let worker = super::Worker::new(engine, eframe::egui::Context::default());
        let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f");
        worker.submit(super::Request {
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            preview_quality: crate::preferences::PreviewMode::Full,
            preview_policy: crate::preferences::PreviewMode::Full,
            revision: 1,
            path,
            settings: foveonic::render::Settings::new("Sunlight", "Standard"),
            output: "final".into(),
            comparison: false,
            export: None,
        });
        let frame = receive(&worker).result.unwrap().unwrap();
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
                    assert_eq!(key.revision, frame.revision);
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
    fn photo_selection_clears_native_source_and_keeps_reduced_lru_until_explicit_clear() {
        use std::sync::atomic::Ordering;
        let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let paths = [
            root.join("blue-parking-sign.x3f"),
            root.join("merrill-hazy-coast.x3f"),
        ];
        let worker = super::Worker::new(
            foveonic::render::Engine::new().unwrap(),
            eframe::egui::Context::default(),
        );
        let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        let request = |revision: u64, index: usize| super::Request {
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            revision,
            path: paths[index].clone(),
            settings: settings.clone(),
            preview_quality: crate::preferences::PreviewMode::Thumb,
            preview_policy: crate::preferences::PreviewMode::Thumb,
            output: "final".into(),
            comparison: false,
            export: None,
        };
        for (round, index) in [0, 1, 0].into_iter().enumerate() {
            worker.clear();
            worker.submit(request(round as u64 + 1, index));
            let reply = receive(&worker);
            assert_eq!(reply.revision, round as u64 + 1);
            let frame = reply.result.unwrap().unwrap();
            assert_eq!(frame.path, paths[index]);
            assert_eq!(
                frame.artifact.settings.working_scale,
                foveonic::render::Scale::new(8).unwrap()
            );
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
        assert!(receive(&worker).result.unwrap().is_some());
        assert_eq!(worker.checks.source_loads.load(Ordering::Relaxed), 3);
    }

    #[test]
    #[ignore = "requires GPU and CACHED_SELECTION_FILES with existing local decoded entries"]
    fn profile_cached_import_selection_and_warm_edits() {
        if std::env::var_os("FOVEONIC_PROFILE").is_some() {
            crate::timing::configure().expect("run the profiled worker test with --exact");
        }
        let paths: Vec<_> =
            std::env::split_paths(&std::env::var_os("CACHED_SELECTION_FILES").unwrap()).collect();
        assert!(!paths.is_empty());
        let worker = super::Worker::new(
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
                    super::Request {
                        framing_preview: false,
                        calibration: None,
                        rendition: Default::default(),
                        revision,
                        path: path.clone(),
                        settings,
                        preview_quality: crate::preferences::PreviewMode::Thumb,
                        preview_policy: crate::preferences::PreviewMode::Thumb,
                        output: "final".into(),
                        comparison: false,
                        export: None,
                    }
                })
                .collect();
            let began = std::time::Instant::now();
            worker.set_overviews(requests, &[]);
            for _ in &paths {
                let reply = receive(&worker);
                let frame = reply.result.unwrap().unwrap();
                assert_eq!(frame.artifact.settings.working_scale.divisor(), 8);
                eprintln!("CACHED_SELECTION revision={revision} photo={} worker_ms={:.3} group_elapsed_ms={:.3}", reply.path.display(), frame.milliseconds, began.elapsed().as_secs_f64() * 1000.);
                frames.insert(reply.path, frame);
            }
            let status = worker.cache.status();
            assert_eq!(status.misses, 0, "selection must use local decoded entries");
            assert_eq!(status.hits, paths.len() as u64);
            eprintln!("CACHED_SELECTION revision={revision} photos={} total_ms={:.3} cache_hits={} cache_misses={}", paths.len(), began.elapsed().as_secs_f64() * 1000., status.hits, status.misses);
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
        let worker = super::Worker::new(
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
                    super::Request {
                        framing_preview: false,
                        calibration: None,
                        rendition: Default::default(),
                        revision,
                        path: path.clone(),
                        settings,
                        preview_quality: crate::preferences::PreviewMode::Thumb,
                        preview_policy: crate::preferences::PreviewMode::Thumb,
                        output: "final".into(),
                        comparison: false,
                        export: None,
                    }
                })
                .collect()
        };
        worker.set_overviews(requests(1), &[]);
        for _ in 0..paths.len() {
            assert!(receive(&worker).result.unwrap().is_some());
        }
        for revision in 2..=12 {
            worker.set_overviews(requests(revision), &[]);
        }
        let mut delivered = std::collections::BTreeSet::new();
        while delivered.len() < paths.len() {
            let reply = receive(&worker);
            if reply.revision != 12 {
                continue;
            }
            let index = paths.iter().position(|path| *path == reply.path).unwrap();
            let numeric = reply
                .numeric_settings
                .as_ref()
                .expect("worker resolves each target's numeric defaults");
            assert!(numeric.sharpen_amount.is_some());
            let frame = reply
                .result
                .unwrap()
                .expect("latest batch must finish every target");
            if index == 0 {
                assert_eq!(numeric.dehaze.shadow, frame.artifact.dehaze_estimate);
                assert!(numeric.dehaze.shadow.is_some());
            }
            assert_eq!(frame.artifact.settings.working_scale.divisor(), 8);
            assert!((frame.requested.exposure - (12. * 0.03 + index as f32 * 0.2)).abs() < 1e-6);
            assert!(delivered.insert(reply.path));
        }
    }

    #[test]
    fn immutable_plan_coalesces_each_photo_without_dropping_other_targets() {
        let (_, replies) = std::sync::mpsc::channel();
        let worker = super::Worker {
            checks: Default::default(),
            pending: std::sync::Arc::new((
                std::sync::Mutex::new(super::Pending::default()),
                std::sync::Condvar::new(),
            )),
            replies,
            progress: Default::default(),
            cache: Default::default(),
            context: eframe::egui::Context::default(),
            memory: Default::default(),
            coordinator: Default::default(),
        };
        let requests = |generation: u64| {
            (0_u64..8)
                .map(|index| {
                    let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
                    settings.exposure = generation as f32 * 0.1;
                    super::Request {
                        framing_preview: false,
                        calibration: None,
                        rendition: Default::default(),
                        preview_quality: crate::preferences::PreviewMode::Thumb,
                        preview_policy: crate::preferences::PreviewMode::Thumb,
                        revision: generation * 8 + index,
                        path: format!("photo-{index}.x3f").into(),
                        settings,
                        output: "final".into(),
                        comparison: false,
                        export: None,
                    }
                })
                .collect()
        };
        worker.set_overviews(requests(1), &[]);
        worker.set_overviews(requests(2), &[]);
        {
            let state = worker.pending.0.lock().unwrap();
            assert_eq!(state.plan.tasks.len(), 8);
            assert!(state
                .plan
                .tasks
                .iter()
                .all(|task| (task.request.settings.exposure - 0.2).abs() < 1e-6));
        }
        worker.retain_overviews(&["photo-0.x3f".into()], &[]);
        {
            let state = worker.pending.0.lock().unwrap();
            assert_eq!(state.plan.tasks.len(), 1);
            assert_eq!(
                state.plan.tasks[0].request.path,
                std::path::Path::new("photo-0.x3f")
            );
        }
        worker.clear();
        let state = worker.pending.0.lock().unwrap();
        assert!(state.plan.tasks.is_empty());
        assert!(state.clear);
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    fn metadata() -> Metadata {
        use foveonic::source::{Matrix, MatrixEncoding};
        Metadata {
            width: 5888,
            height: 3776,
            rotation: 0,
            processing_version: Some([3, 1, 2, 0]),
            properties: [("CAMMODEL".into(), "SIGMA sd Quattro".into())].into(),
            matrices: [("CAMERAID", 40.), ("CaptureISO", 100.)]
                .into_iter()
                .map(|(k, v)| {
                    (
                        k.into(),
                        Matrix {
                            encoding: MatrixEncoding::Float32,
                            representation: "test",
                            dimensions: vec![1],
                            values: vec![v],
                        },
                    )
                })
                .collect(),
            lists: Default::default(),
        }
    }
    #[test]
    fn merrill_smart_uses_fast_overview_and_regional_full_without_changing_edits() {
        use foveonic::source::{Matrix, MatrixEncoding};
        let mut metadata = metadata();
        metadata.width = 4928;
        metadata.height = 3264;
        metadata.processing_version = None;
        metadata
            .properties
            .insert("CAMMODEL".into(), "SIGMA DP2 Merrill".into());
        metadata.matrices.insert(
            "ActiveImageArea".into(),
            Matrix {
                encoding: MatrixEncoding::Uint32,
                representation: "test",
                dimensions: vec![4],
                values: vec![112., 64., 4815., 3199.],
            },
        );
        let mut request = Request {
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            revision: 1,
            path: "merrill.x3f".into(),
            settings: Settings::new("Sunlight", "Standard"),
            preview_quality: PreviewMode::Smart,
            preview_policy: PreviewMode::Smart,
            output: "final".into(),
            comparison: false,
            export: None,
        };
        request.settings.dehaze.amount = 35.;
        let mut resolved = request.settings.clone();
        resolve_preview(&request, &mut resolved, &metadata);
        assert_eq!(resolved.working_scale, Scale::new(2).unwrap());
        assert_eq!(resolved.preview_region, None);
        assert!(resolved.row_correction && resolved.despeckle);
        for (mode, quality) in [
            (PreviewMode::Draft, Scale::new(4).unwrap()),
            (PreviewMode::Thumb, Scale::new(8).unwrap()),
        ] {
            request.preview_quality = mode;
            resolve_preview(&request, &mut resolved, &metadata);
            assert_eq!(resolved.working_scale, quality);
        }
        request.preview_quality = PreviewMode::Fast;
        resolve_preview(&request, &mut resolved, &metadata);
        assert_eq!(resolved.working_scale, Scale::new(2).unwrap());
        assert!(crate::smart::same_edits(&resolved, &request.settings));
        request.settings.working_scale = Scale::new(4).unwrap();
        request.preview_quality = PreviewMode::Full;
        resolve_preview(&request, &mut resolved, &metadata);
        assert_eq!(resolved.working_scale, Scale::NATIVE);
        request.preview_quality = PreviewMode::Smart;
        request.settings.preview_region = Some([100, 100, 400, 300]);
        resolved = request.settings.clone();
        resolve_preview(&request, &mut resolved, &metadata);
        assert_eq!(resolved.working_scale, Scale::NATIVE);
        assert_eq!(
            resolved.preview_region, request.settings.preview_region,
            "Merrill must preserve the requested native detail region"
        );
        for format in [ExportFormat::Tiff, ExportFormat::Dng] {
            request.export = Some(Export {
                path: "export.output".into(),
                format,
                tiff_color_space: None,
                encoding: Default::default(),
                sections: Default::default(),
                conflict: foveonic::importing::ConflictPolicy::Rename,
            });
            resolve_preview(&request, &mut resolved, &metadata);
            assert_eq!(resolved.working_scale, Scale::NATIVE);
            assert_eq!(resolved.preview_region, None);
        }
    }
    #[test]
    fn numeric_modes_preserve_controls_and_native_purposes_are_explicit() {
        let mut r = Request {
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            revision: 1,
            path: "test.x3f".into(),
            settings: Settings::new("Sunlight", "Standard"),
            preview_quality: PreviewMode::Fast,
            preview_policy: PreviewMode::Fast,
            output: "final".into(),
            comparison: false,
            export: None,
        };
        r.settings.rendering = foveonic::render::Rendering::Monochrome;
        r.settings.dehaze.amount = 35.;
        r.settings.photographic.tone_method = foveonic::render::PhotographicMethod::Universal;
        r.settings.photographic.base_response = foveonic::render::BaseResponse::Strong;
        for mode in [
            PreviewMode::Full,
            PreviewMode::Fast,
            PreviewMode::Smart,
            PreviewMode::Draft,
            PreviewMode::Thumb,
        ] {
            r.preview_quality = mode;
            assert_eq!(effective_working_scale(&r), mode.working_scale());
        }
        r.settings.output_size = Some([640, 480]);
        assert_eq!(effective_working_scale(&r), Scale::new(8).unwrap());
        r.output = "sensor.rgb".into();
        assert_eq!(effective_working_scale(&r), Scale::NATIVE);
        r.output = "final".into();
        r.comparison = true;
        assert_eq!(effective_working_scale(&r), Scale::NATIVE);
        r.comparison = false;
        r.settings.preview_region = Some([100, 100, 400, 300]);
        assert_eq!(effective_working_scale(&r), Scale::NATIVE);
        r.settings.preview_region = None;
        for format in [ExportFormat::Tiff, ExportFormat::Dng] {
            r.export = Some(Export {
                path: "test.output".into(),
                format,
                tiff_color_space: None,
                encoding: Default::default(),
                sections: Default::default(),
                conflict: foveonic::importing::ConflictPolicy::Rename,
            });
            assert_eq!(effective_working_scale(&r), Scale::NATIVE);
        }
    }

    #[test]
    #[ignore = "requires GPU and QUATTRO_X3F"]
    fn fast_preference_exports_full_files_and_returns_to_fast_preview() {
        let path = PathBuf::from(std::env::var_os("QUATTRO_X3F").unwrap());
        preview_exports_full_files_and_returns_to_fast(path);
    }

    #[test]
    #[ignore = "requires GPU and MERRILL_X3F"]
    fn merrill_smart_exports_full_files_and_returns_to_fast_preview() {
        let path = PathBuf::from(std::env::var_os("MERRILL_X3F").unwrap());
        preview_exports_full_files_and_returns_to_fast(path);
    }

    fn preview_exports_full_files_and_returns_to_fast(path: PathBuf) {
        let source = Source::open(&path).unwrap();
        let engine = Engine::new().unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        settings.exposure = 0.5;
        settings.tiff_color_space = Some(foveonic::render::TiffColorSpace::Srgb);
        let expected = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        let native = expected.view().extent;
        engine
            .write_tiff(
                &expected,
                &directory.join("expected.tiff"),
                &mut Profile::default(),
            )
            .unwrap();
        drop(expected);
        foveonic::dng::write_source(
            &engine,
            &source,
            &settings,
            &directory.join("expected.dng"),
            &mut Profile::default(),
        )
        .unwrap();
        drop(source);
        let worker = Worker::new(engine, egui::Context::default());
        let receive = || loop {
            if let Update::Image(reply) = worker
                .replies
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap()
            {
                break *reply;
            }
        };
        for (index, (mode, format)) in [
            (PreviewMode::Fast, ExportFormat::Tiff),
            (PreviewMode::Fast, ExportFormat::Dng),
            (PreviewMode::Smart, ExportFormat::Tiff),
            (PreviewMode::Smart, ExportFormat::Dng),
        ]
        .into_iter()
        .enumerate()
        {
            let extension = format.extension();
            let destination = directory.join(format!("export-{index}.{extension}"));
            let mut export_settings = settings.clone();
            export_settings.preview_region = Some([100, 100, 400, 300]);
            export_settings.tiff_color_space = Some(foveonic::render::TiffColorSpace::ProPhotoRgb);
            worker.submit(Request {
                framing_preview: false,
                calibration: None,
                rendition: Default::default(),
                revision: index as u64 + 1,
                path: path.clone(),
                settings: export_settings,
                preview_quality: mode,
                preview_policy: mode,
                output: "sensor.rgb".into(),
                comparison: false,
                export: Some(Export {
                    path: destination.clone(),
                    format,
                    tiff_color_space: settings.tiff_color_space,
                    encoding: Default::default(),
                    sections: Default::default(),
                    conflict: foveonic::importing::ConflictPolicy::Rename,
                }),
            });
            let exported = receive();
            assert!(exported.result.unwrap().is_none());
            assert_eq!(exported.exported.map(|e| e.format), Some(format));
            assert_eq!(
                exported.settings.tiff_color_space,
                Some(foveonic::render::TiffColorSpace::ProPhotoRgb)
            );
            assert!(
                std::fs::read(directory.join(format!("expected.{extension}"))).unwrap()
                    == std::fs::read(destination).unwrap(),
                "{extension} export differs from independent Full output"
            );
        }
        worker.submit(Request {
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            revision: 5,
            path,
            settings,
            preview_quality: crate::preferences::PreviewMode::Fast,
            preview_policy: crate::preferences::PreviewMode::Fast,
            output: "final".into(),
            comparison: false,
            export: None,
        });
        let reply = receive();
        assert_eq!(reply.settings.working_scale, Scale::NATIVE);
        let frame = reply.result.unwrap().unwrap();
        assert_eq!(
            frame.artifact.settings.working_scale,
            Scale::new(2).unwrap()
        );
        assert_eq!(frame.texture.description().extent, native.map(|v| v / 2));
        assert_eq!(frame.display_extent(false), native);
        assert!(frame.artifact.settings.reconstruction.is_some());
        assert!(reply.settings.reconstruction.is_some());
        drop(worker);
    }
}

#[cfg(test)]
mod color_pick_tests {
    use super::*;
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
        let worker = Worker::new(Engine::new().unwrap(), egui::Context::default());
        worker.submit(Request {
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            revision: 1,
            path: path.clone(),
            settings: settings.clone(),
            preview_quality: PreviewMode::Full,
            preview_policy: PreviewMode::Full,
            output: "final".into(),
            comparison: false,
            export: None,
        });
        loop {
            if let Update::Image(reply) = worker
                .replies
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap()
            {
                assert!(reply.result.unwrap().is_some());
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
                1,
                None,
            );
            let reply = loop {
                if let Update::ColorPick { id, reply } = worker
                    .replies
                    .recv_timeout(std::time::Duration::from_secs(30))
                    .unwrap()
                {
                    assert!(worker.finish_pick(id));
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
