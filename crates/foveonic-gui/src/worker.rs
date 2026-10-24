//! Keeps rendering off the UI thread and replaces outdated requests instead of queuing edits.

use crate::{
    export::{self, resolve_camera_settings, Export, ExportFormat},
    preferences::PreviewMode,
};
use eframe::egui;
#[cfg(test)]
use foveonic::source::Source;
use foveonic::{
    image::{ImageTexture, RenderedImage},
    inspection::{Artifact, Content},
    profile::Profile,
    render::{Engine, PreviewQuality, RenderSession, Settings},
    source::Metadata,
};
use std::{
    path::PathBuf,
    sync::{mpsc, Arc, Condvar, Mutex},
    thread,
};

pub struct Request {
    pub revision: u64,
    pub path: PathBuf,
    pub settings: Settings,
    pub preview_quality: PreviewMode,
    pub output: String,
    pub comparison: bool,
    pub export: Option<Export>,
}
fn effective_preview_quality(
    request: &Request,
    settings: &Settings,
    metadata: &Metadata,
) -> PreviewQuality {
    if request.export.is_none()
        && request.output == "final"
        && !request.comparison
        && settings.fast_preview_available(metadata)
    {
        match request.preview_quality {
            PreviewMode::Full => PreviewQuality::Full,
            PreviewMode::Fast => PreviewQuality::Fast,
            PreviewMode::Smart if request.settings.preview_region.is_some() => PreviewQuality::Full,
            PreviewMode::Smart => PreviewQuality::Draft,
        }
    } else {
        PreviewQuality::Full
    }
}

fn resolve_preview(request: &Request, settings: &mut Settings, metadata: &Metadata) {
    settings.preview_quality = effective_preview_quality(request, settings, metadata);
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
    /// Keep zoom coordinates native while Fast and Draft store fewer pixels.
    pub fn display_extent(&self, input: bool) -> [u32; 2] {
        let extent = self.view(input).description().extent;
        if self.artifact.settings.preview_quality.is_reduced()
            && self.artifact.settings.output_size.is_none()
        {
            extent.map(|v| v * 2)
        } else {
            extent
        }
    }
    pub fn quality_label(&self) -> &'static str {
        match self.artifact.settings.preview_quality {
            PreviewQuality::Full => "Full",
            PreviewQuality::Fast => "Fast",
            PreviewQuality::Draft => "Draft",
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
pub struct Comparison {
    pub input: &'static str,
    pub texture: Arc<ImageTexture>,
}
pub struct Reply {
    pub revision: u64,
    pub metadata: Option<Arc<Metadata>>,
    pub settings: Settings,
    pub result: Result<Option<Frame>, String>,
    pub exported: Option<ExportFormat>,
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
    Scopes {
        key: ScopeKey,
        result: Result<foveonic::scopes::Scopes, String>,
    },
}

struct Pending {
    activity: Option<foveonic::decode_cache::Activity>,
    request: Option<Request>,
    scopes: Option<ScopeRequest>,
    queued_at: Option<std::time::Instant>,
    stop: bool,
    clear: bool,
}
pub struct Worker {
    pending: Arc<(Mutex<Pending>, Condvar)>,
    pub replies: mpsc::Receiver<Update>,
    pub progress: crate::progress::Tracker,
    pub cache: Arc<foveonic::decode_cache::Cache>,
    context: egui::Context,
}
impl Worker {
    pub fn new(engine: Engine, context: egui::Context) -> Self {
        let pending = Arc::new((
            Mutex::new(Pending {
                activity: None,
                request: None,
                scopes: None,
                queued_at: None,
                stop: false,
                clear: false,
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
        thread::spawn(move || {
            let mut source: Option<(PathBuf, RenderSession)> = None;
            loop {
                let (request, queued_at, _activity) = {
                    let (lock, wake) = &*slot;
                    let mut state = lock.lock().unwrap();
                    while !state.stop
                        && !state.clear
                        && state.request.is_none()
                        && state.scopes.is_none()
                    {
                        state = wake.wait(state).unwrap();
                    }
                    if state.stop {
                        break;
                    }
                    if state.clear {
                        state.clear = false;
                        source = None;
                        continue;
                    }
                    if state.request.is_none() {
                        let scopes = state.scopes.take().unwrap();
                        let _activity = decoded_cache.foreground();
                        drop(state);
                        if !analyze_scopes(&engine, &slot, &tx, &context, scopes) {
                            break;
                        }
                        continue;
                    }
                    (
                        state.request.take().unwrap(),
                        state.queued_at.take().unwrap(),
                        state.activity.take(),
                    )
                };
                let reporter =
                    crate::progress::Reporter::new(progress_updates.clone(), request.revision);
                let started = std::time::Instant::now();
                crate::timing::worker_started(request.revision, queued_at, started);
                let mut metadata = None;
                let mut settings = request.settings.clone();
                let mut defaults_errors = Vec::new();
                let result = (|| -> foveonic::source::Result<Option<Frame>> {
                    if source.as_ref().is_none_or(|(p, _)| p != &request.path) {
                        source = None; // Avoid keeping two decoded photos in memory during a switch.
                        let _span = crate::timing::Span::new(request.revision, "source_open");
                        let loaded = decoded_cache.open(&request.path, |stage| reporter.loaded(stage))?;
                        if crate::timing::enabled() {
                            crate::timing::emit("source_load", serde_json::json!({"revision":request.revision,"source":request.path,"timings":loaded.load_timings(),"read_cache_state":"uncontrolled","input":if loaded.load_timings().iter().any(|t|t.stage=="decoded cache read"){"decoded-cache"}else{"original"},"extent":[loaded.metadata.width,loaded.metadata.height]}));
                        }
                        source = Some((request.path.clone(), RenderSession::new(loaded)));
                    }
                    // Cached sources have already completed both loading sections.
                    reporter.loaded(foveonic::source::LoadStage::Decoded);
                    let session = &mut source.as_mut().unwrap().1;
                    metadata = Some(session.metadata().clone());
                    defaults_errors = resolve_camera_settings(&mut settings, session.metadata());
                    resolve_preview(&request, &mut settings, session.metadata());
                    if let Some(export) = request.export.as_ref() {
                        let mut profile=Profile::new(crate::timing::mode());
                        let before=crate::timing::enabled().then(||engine.gpu_activity());
                        export::write(&engine, session, &settings, export, &mut profile, |stage| reporter.rendered(stage))?;
                        if let Some(before)=before {crate::timing::emit("export",serde_json::json!({"revision":request.revision,"source":request.path,"destination":export.path,"format":export.format.extension(),"settings":settings,"profile":profile,"activity":engine.gpu_activity().since(before)}));}
                        reporter.rendered(foveonic::render::RenderProgress::Stage { completed: 1, total: 1 });
                        return Ok(None);
                    }
                    let edit=session.changed_parameters(&settings);
                    let cancel=||{
                        if request.export.is_some(){return false;}
                        let state=slot.0.lock().unwrap();
                        state.clear||state.stop||state.request.as_ref().is_some_and(|pending|{
                            let mut next=pending.settings.clone();
                            resolve_camera_settings(&mut next,metadata.as_ref().unwrap());
                            resolve_preview(pending, &mut next, metadata.as_ref().unwrap());
                            pending.path!=request.path||pending.output!=request.output||pending.comparison!=request.comparison||settings.preview_quality!=next.preview_quality||settings.preview_region!=next.preview_region||(!edit.is_empty()&&settings.changed_parameters(&next).iter().any(|parameter|!edit.contains(parameter)))
                        })
                    };
                    let mut profile = Profile::new(crate::timing::mode());
                    let before = crate::timing::enabled().then(||engine.gpu_activity());
                    let cache_before = before.map(|_|session.cached_bytes());
                    let root=if request.export.is_some(){"final"}else{&request.output};
                    let input = if request.comparison && request.export.is_none() {
                        Some(foveonic::inspection::comparison_input(&settings, root, metadata.as_deref())
                            .ok_or("selected output has no operation-input comparison")?)
                    } else {
                        None
                    };
                    let mut roots = vec![root];
                    if let Some(input) = input { roots.push(input); }
                    let rendered = {
                        // Timing a host span must not add a GPU wait and change the
                        // responsiveness we are trying to measure.
                        let _span = crate::timing::Span::new(request.revision, "processing");
                        engine.render_session_with_progress(session, &settings, &roots, cancel, &mut profile, |stage| reporter.rendered(stage))?
                    };
                    if let Some(before) = before {
                        crate::timing::emit("processing", serde_json::json!({"revision":request.revision,"source":request.path,"output":root,"outputs":roots,"settings":settings,"execution":"session","status":if rendered.is_some(){"complete"}else{"superseded"},"cache_before_bytes":cache_before,"cache_after_bytes":session.cached_bytes(),"profile":profile,"activity":engine.gpu_activity().since(before)}));
                    }
                    let mut profile = Profile::new(crate::timing::mode());
                    let Some(mut artifacts) = rendered else { return Ok(None); };
                    if cancel(){return Ok(None);}
                    let view_before = crate::timing::enabled().then(||engine.gpu_activity());
                    let comparison = if let Some(input) = input {
                        let baseline = artifacts.pop().ok_or("missing comparison input")?;
                        let Content::Image(image) = baseline.content else {
                            return Err("comparison input must be an image".into());
                        };
                        let _span = crate::timing::Span::new(request.revision, "comparison_view_prepare_host");
                        Some(Comparison {
                            input,
                            texture: Arc::new(engine.view_texture(&image, None, &mut profile)?),
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
                    let texture = {
                        let _span = crate::timing::Span::new(request.revision, "view_prepare_host");
                        Arc::new(engine.view_texture(image, None, &mut profile)?)
                    };
                    if comparison.as_ref().is_some_and(|c| c.texture.description().extent != texture.description().extent) {
                        return Err("comparison views have different geometry".into());
                    }
                    {
                        let _span = crate::timing::Span::new(request.revision, "view_completion_wait");
                        engine.device().poll(eframe::wgpu::Maintain::Wait);
                    }
                    if let Some(before)=view_before{crate::timing::emit("view",serde_json::json!({"revision":request.revision,"source":request.path,"output":root,"profile":profile,"activity":engine.gpu_activity().since(before)}));}
                    Ok(Some(Frame {
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
                settings.preview_quality = PreviewQuality::Full;
                settings.preview_region = None;
                let publication = crate::timing::Span::new(request.revision, "reply_publication");
                crate::timing::event(request.revision, "reply_ready");
                if tx
                    .send(Update::Image(Box::new(Reply {
                        revision: request.revision,
                        metadata,
                        settings,
                        result,
                        exported: request.export.as_ref().map(|e| e.format),
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
            pending,
            replies,
            progress,
            cache,
            context: repaint,
        }
    }
    pub fn clear(&self) {
        self.progress.clear();
        let (lock, wake) = &*self.pending;
        let mut state = lock.lock().unwrap();
        state.clear = true;
        state.request = None;
        state.activity = None;
        state.scopes = None;
        state.queued_at = None;
        wake.notify_one();
    }
    pub fn submit_scopes(&self, request: ScopeRequest) {
        let (lock, wake) = &*self.pending;
        lock.lock().unwrap().scopes = Some(request);
        wake.notify_one();
    }
    pub fn submit(&self, request: Request) {
        self.submit_at(request, std::time::Instant::now());
    }
    /// A newly opened photo includes recipe loading before it reaches this queue.
    pub fn submit_at(&self, request: Request, queued_at: std::time::Instant) {
        self.progress.begin(request.revision, queued_at);
        let (lock, wake) = &*self.pending;
        let mut state = lock.lock().unwrap();
        state.activity = Some(self.cache.foreground());
        state.request = Some(request);
        state.queued_at = Some(queued_at);
        wake.notify_one();
        drop(state);
        self.context.request_repaint();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.pending;
        lock.lock().unwrap().stop = true;
        wake.notify_one();
    }
}

fn analyze_scopes(
    engine: &Engine,
    slot: &Arc<(Mutex<Pending>, Condvar)>,
    tx: &mpsc::SyncSender<Update>,
    context: &egui::Context,
    request: ScopeRequest,
) -> bool {
    // Scope-only jobs reuse the immutable displayed buffer and yield to image work.
    {
        let mut state = slot.0.lock().unwrap();
        if state.stop || state.clear || state.scopes.is_some() {
            return true;
        }
        if state.request.is_some() {
            state.scopes = Some(request);
            return true;
        }
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
    {
        let state = slot.0.lock().unwrap();
        if state.stop || state.clear || state.scopes.is_some() {
            return true;
        }
    }
    if tx
        .send(Update::Scopes {
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
    fn receive(worker: &super::Worker) -> super::Reply {
        loop {
            if let super::Update::Image(reply) = worker
                .replies
                .recv_timeout(std::time::Duration::from_secs(20))
                .unwrap()
            {
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
                preview_quality: crate::preferences::PreviewMode::Full,
                revision: index as u64 + 1,
                path: path.clone(),
                settings: settings.clone(),
                output: "sensor.blue".into(),
                comparison: true,
                export: Some(super::Export {
                    path: destination.clone(),
                    format: super::ExportFormat::Dng,
                    tiff_color_space: settings.tiff_color_space,
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
            assert_eq!(reply.exported, Some(super::ExportFormat::Dng));
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
                preview_quality: crate::preferences::PreviewMode::Full,
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
            preview_quality: crate::preferences::PreviewMode::Full,
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
            preview_quality: crate::preferences::PreviewMode::Full,
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
                preview_quality: crate::preferences::PreviewMode::Full,
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
            preview_quality: crate::preferences::PreviewMode::Full,
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
            preview_quality: crate::preferences::PreviewMode::Full,
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
                preview_quality: crate::preferences::PreviewMode::Full,
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
        fn edit(settings: &mut foveonic::render::Settings, control: &str, revision: u64) {
            use foveonic::noise::Parameter;
            match control {
                "detail" => {
                    settings.reconstruction.as_mut().unwrap().detail = revision as f32 / 20.
                }
                "exposure" => settings.exposure = revision as f32 / 40.,
                "chroma" => {
                    settings.reconstruction.as_mut().unwrap().chroma = (revision % 5) as u32
                }
                "luma" => {
                    settings.reconstruction.as_mut().unwrap().luminance = (revision % 5) as u32
                }
                key => {
                    let p = Parameter::ALL.into_iter().find(|p| p.key() == key).unwrap();
                    let bounds = p.bounds();
                    let value = *bounds.start()
                        + (*bounds.end() - *bounds.start()) * ((revision % 40) as f32 / 40.);
                    settings
                        .reconstruction
                        .as_mut()
                        .unwrap()
                        .overrides
                        .set(p, Some(value));
                }
            }
        }
        for control in [
            "detail",
            "exposure",
            "chroma",
            "luma",
            "base-blend",
            "edge-cutoff",
            "mean-decay",
            "edge-scale",
            "luma-noise-scale",
            "brightness-mix",
            "layer-mix",
        ] {
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
                    preview_quality: crate::preferences::PreviewMode::Full,
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
            let start = std::time::Instant::now();
            let mut progress = Vec::new();
            let mut settled = false;
            let mut scopes = false;
            for revision in 2..=last {
                edit(&mut settings, control, revision);
                submit(revision, settings.clone());
                std::thread::sleep(std::time::Duration::from_millis(8));
                while let Ok(update) = worker.replies.try_recv() {
                    if let super::Update::Image(reply) = update {
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
                    } else if let super::Update::Scopes { key, result } = update {
                        if key.revision == last {
                            assert!(result.is_ok());
                            scopes = true;
                        }
                    }
                }
            }
            assert!(
                !progress.is_empty(),
                "continuous dragging starved image publication"
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
                    super::Update::Scopes { key, result } if key.revision == last => {
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
            preview_quality: crate::preferences::PreviewMode::Full,
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
                super::Update::Scopes { key, result } => {
                    assert_eq!(key.revision, frame.revision);
                    let data = result.unwrap();
                    if key.region == latest {
                        assert_eq!(data.samples, [64, 32]);
                        assert_eq!(data.histogram(0).iter().sum::<u32>(), 64 * 32);
                        break;
                    }
                }
                super::Update::Image(_) => panic!("scope-only jobs must not render another image"),
            }
        }
        request_scopes(&worker, &frame, None);
        let super::Update::Scopes { key, result } = worker
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
    fn pending_requests_replace_not_accumulate() {
        let (_, replies) = std::sync::mpsc::channel();
        let worker = super::Worker {
            pending: std::sync::Arc::new((
                std::sync::Mutex::new(super::Pending {
                    activity: None,
                    request: None,
                    queued_at: None,
                    scopes: None,
                    stop: false,
                    clear: false,
                }),
                std::sync::Condvar::new(),
            )),
            replies,
            progress: Default::default(),
            cache: Default::default(),
            context: eframe::egui::Context::default(),
        };
        for revision in 1..100 {
            worker.submit(super::Request {
                preview_quality: crate::preferences::PreviewMode::Full,
                revision,
                path: "test.x3f".into(),
                settings: foveonic::render::Settings::new("Sunlight", "Standard"),
                output: "final".into(),
                comparison: false,
                export: None,
            });
        }
        assert_eq!(
            worker
                .pending
                .0
                .lock()
                .unwrap()
                .request
                .as_ref()
                .unwrap()
                .revision,
            99
        );
        let opening_began = std::time::Instant::now() - std::time::Duration::from_millis(250);
        worker.submit_at(
            super::Request {
                preview_quality: crate::preferences::PreviewMode::Full,
                revision: 100,
                path: "test.x3f".into(),
                settings: foveonic::render::Settings::new("Sunlight", "Standard"),
                output: "final".into(),
                comparison: false,
                export: None,
            },
            opening_began,
        );
        assert_eq!(
            worker.pending.0.lock().unwrap().queued_at,
            Some(opening_began)
        );
        assert!(worker.progress.snapshot().unwrap().milliseconds() >= 250.);
        worker.clear();
        let state = worker.pending.0.lock().unwrap();
        assert!(state.request.is_none());
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
            revision: 1,
            path: "merrill.x3f".into(),
            settings: Settings::new("Sunlight", "Standard"),
            preview_quality: PreviewMode::Smart,
            output: "final".into(),
            comparison: false,
            export: None,
        };
        let mut resolved = request.settings.clone();
        resolve_preview(&request, &mut resolved, &metadata);
        assert_eq!(resolved.preview_quality, PreviewQuality::Draft);
        assert_eq!(resolved.preview_region, None);
        assert!(resolved.row_correction && resolved.despeckle);
        request.preview_quality = PreviewMode::Fast;
        resolve_preview(&request, &mut resolved, &metadata);
        assert_eq!(resolved.preview_quality, PreviewQuality::Fast);
        assert!(crate::smart::same_edits(&resolved, &request.settings));
        request.preview_quality = PreviewMode::Full;
        resolve_preview(&request, &mut resolved, &metadata);
        assert_eq!(resolved.preview_quality, PreviewQuality::Full);
        request.preview_quality = PreviewMode::Smart;
        request.settings.preview_region = Some([100, 100, 400, 300]);
        resolved = request.settings.clone();
        resolve_preview(&request, &mut resolved, &metadata);
        assert_eq!(resolved.preview_quality, PreviewQuality::Full);
        assert_eq!(
            resolved.preview_region, request.settings.preview_region,
            "Merrill must preserve the requested native detail region"
        );
        for format in [ExportFormat::Tiff, ExportFormat::Dng] {
            request.export = Some(Export {
                path: "export.output".into(),
                format,
                tiff_color_space: None,
            });
            resolve_preview(&request, &mut resolved, &metadata);
            assert_eq!(resolved.preview_quality, PreviewQuality::Full);
            assert_eq!(resolved.preview_region, None);
        }
    }
    #[test]
    fn fast_policy_keeps_inspection_monochrome_comparisons_and_exports_full() {
        let mut m = metadata();
        let mut r = Request {
            revision: 1,
            path: "test.x3f".into(),
            settings: Settings::new("Sunlight", "Standard"),
            preview_quality: crate::preferences::PreviewMode::Fast,
            output: "final".into(),
            comparison: false,
            export: None,
        };
        assert_eq!(
            effective_preview_quality(&r, &r.settings, &m),
            PreviewQuality::Fast
        );
        r.preview_quality = PreviewMode::Smart;
        assert_eq!(
            effective_preview_quality(&r, &r.settings, &m),
            PreviewQuality::Draft
        );
        r.settings.preview_region = Some([100, 100, 400, 300]);
        assert_eq!(
            effective_preview_quality(&r, &r.settings, &m),
            PreviewQuality::Full
        );
        r.settings.preview_region = None;
        r.preview_quality = PreviewMode::Fast;
        r.output = "sensor.rgb".into();
        assert_eq!(
            effective_preview_quality(&r, &r.settings, &m),
            PreviewQuality::Full
        );
        r.output = "final".into();
        r.comparison = true;
        assert_eq!(
            effective_preview_quality(&r, &r.settings, &m),
            PreviewQuality::Full
        );
        r.comparison = false;
        for format in [ExportFormat::Tiff, ExportFormat::Dng] {
            r.export = Some(Export {
                path: "test.output".into(),
                format,
                tiff_color_space: None,
            });
            assert_eq!(
                effective_preview_quality(&r, &r.settings, &m),
                PreviewQuality::Full
            );
        }
        r.export = None;
        r.settings.rendering = foveonic::render::Rendering::Monochrome;
        assert_eq!(
            effective_preview_quality(&r, &r.settings, &m),
            PreviewQuality::Full
        );
        r.settings.rendering = foveonic::render::Rendering::Color;
        m.processing_version = Some([4, 0, 1, 0]);
        assert_eq!(
            effective_preview_quality(&r, &r.settings, &m),
            PreviewQuality::Full
        );
        m.width = 6656;
        m.height = 4480;
        m.matrices.get_mut("CAMERAID").unwrap().values[0] = 41.;
        for quality in [PreviewMode::Fast, PreviewMode::Smart] {
            r.preview_quality = quality;
            assert_eq!(
                effective_preview_quality(&r, &r.settings, &m),
                if quality == PreviewMode::Smart {
                    PreviewQuality::Draft
                } else {
                    PreviewQuality::Fast
                }
            );
        }
        r.settings.preview_region = Some([100, 100, 400, 300]);
        assert_eq!(
            effective_preview_quality(&r, &r.settings, &m),
            PreviewQuality::Full
        );
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
                revision: index as u64 + 1,
                path: path.clone(),
                settings: export_settings,
                preview_quality: mode,
                output: "sensor.rgb".into(),
                comparison: false,
                export: Some(Export {
                    path: destination.clone(),
                    format,
                    tiff_color_space: settings.tiff_color_space,
                }),
            });
            let exported = receive();
            assert!(exported.result.unwrap().is_none());
            assert_eq!(exported.exported, Some(format));
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
            revision: 5,
            path,
            settings,
            preview_quality: crate::preferences::PreviewMode::Fast,
            output: "final".into(),
            comparison: false,
            export: None,
        });
        let reply = receive();
        assert_eq!(reply.settings.preview_quality, PreviewQuality::Full);
        let frame = reply.result.unwrap().unwrap();
        assert_eq!(
            frame.artifact.settings.preview_quality,
            PreviewQuality::Fast
        );
        assert_eq!(frame.texture.description().extent, native.map(|v| v / 2));
        assert_eq!(frame.display_extent(false), native);
        assert!(frame.artifact.settings.reconstruction.is_some());
        assert!(reply.settings.reconstruction.is_some());
        drop(worker);
    }
}
