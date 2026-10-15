//! Keeps rendering off the UI thread and replaces outdated requests instead of queuing edits.

use eframe::egui;
use foveonic::{
    image::{ImageTexture, RenderedImage},
    inspection::{Artifact, Content},
    profile::Profile,
    render::{Engine, RenderSession, Settings},
    source::{Metadata, Source},
};
use std::{
    path::PathBuf,
    sync::{mpsc, Arc, Condvar, Mutex},
    thread,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Tiff,
    Dng,
}
impl ExportFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tiff => "TIFF",
            Self::Dng => "DNG",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Tiff => "tiff",
            Self::Dng => "dng",
        }
    }
}
pub struct Export {
    pub path: PathBuf,
    pub format: ExportFormat,
}

pub struct Request {
    pub revision: u64,
    pub path: PathBuf,
    pub settings: Settings,
    pub output: String,
    pub comparison: bool,
    pub export: Option<Export>,
}
pub struct Frame {
    pub path: PathBuf,
    pub output: String,
    pub artifact: Artifact,
    pub texture: Arc<ImageTexture>,
    pub comparison: Option<Comparison>,
    pub scopes: Option<ScopeSnapshot>,
    pub milliseconds: f64,
    pub revision: u64,
    pub queued_at: std::time::Instant,
}
impl Frame {
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
    request: Option<Request>,
    scopes: Option<ScopeRequest>,
    queued_at: Option<std::time::Instant>,
    stop: bool,
    clear: bool,
}
pub struct Worker {
    pending: Arc<(Mutex<Pending>, Condvar)>,
    pub replies: mpsc::Receiver<Update>,
}
impl Worker {
    pub fn new(engine: Engine, context: egui::Context) -> Self {
        let pending = Arc::new((
            Mutex::new(Pending {
                request: None,
                scopes: None,
                queued_at: None,
                stop: false,
                clear: false,
            }),
            Condvar::new(),
        ));
        let (tx, replies) = mpsc::sync_channel(2);
        let slot = pending.clone();
        thread::spawn(move || {
            let mut source: Option<(PathBuf, RenderSession)> = None;
            loop {
                let (request, queued_at) = {
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
                        drop(state);
                        if !analyze_scopes(&engine, &slot, &tx, &context, scopes) {
                            break;
                        }
                        continue;
                    }
                    (
                        state.request.take().unwrap(),
                        state.queued_at.take().unwrap(),
                    )
                };
                let started = std::time::Instant::now();
                crate::timing::worker_started(request.revision, queued_at, started);
                let mut metadata = None;
                let mut settings = request.settings.clone();
                let mut defaults_errors = Vec::new();
                let result = (|| -> foveonic::source::Result<Option<Frame>> {
                    if source.as_ref().is_none_or(|(p, _)| p != &request.path) {
                        source = None; // Avoid keeping two decoded photos in memory during a switch.
                        let _span = crate::timing::Span::new(request.revision, "source_open");
                        let loaded = Source::open(&request.path)?;
                        if crate::timing::enabled() {
                            crate::timing::emit("source_load", serde_json::json!({"revision":request.revision,"source":request.path,"timings":loaded.load_timings(),"read_cache_state":"uncontrolled","extent":[loaded.metadata.width,loaded.metadata.height]}));
                        }
                        source = Some((request.path.clone(), RenderSession::new(loaded)));
                    }
                    let session = &mut source.as_mut().unwrap().1;
                    metadata = Some(session.metadata().clone());
                    defaults_errors = resolve_camera_settings(&mut settings, session.metadata());
                    if let Some(export) = request.export.as_ref().filter(|e| e.format == ExportFormat::Dng) {
                        let mut profile=Profile::new(crate::timing::mode());
                        let before=crate::timing::enabled().then(||engine.gpu_activity());
                        foveonic::dng::write_source(&engine,session.source(),&settings,&export.path,&mut profile)?;
                        if let Some(before)=before {crate::timing::emit("export",serde_json::json!({"revision":request.revision,"source":request.path,"destination":export.path,"format":"dng","settings":settings,"profile":profile,"activity":engine.gpu_activity().since(before)}));}
                        return Ok(None);
                    }
                    let edit=session.changed_parameters(&settings);
                    let cancel=||{
                        if request.export.is_some(){return false;}
                        let state=slot.0.lock().unwrap();
                        state.clear||state.stop||state.request.as_ref().is_some_and(|pending|{
                            let mut next=pending.settings.clone();
                            resolve_camera_settings(&mut next,metadata.as_ref().unwrap());
                            pending.path!=request.path||pending.output!=request.output||pending.comparison!=request.comparison||(!edit.is_empty()&&settings.changed_parameters(&next).iter().any(|parameter|!edit.contains(parameter)))
                        })
                    };
                    let mut profile = Profile::new(crate::timing::mode());
                    let before = crate::timing::enabled().then(||engine.gpu_activity());
                    let cache_before = before.map(|_|session.cached_bytes());
                    let root=if request.export.is_some(){"final"}else{&request.output};
                    let input = if request.comparison && request.export.is_none() {
                        Some(foveonic::inspection::comparison_input(&settings, root)
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
                        engine.render_session(session, &settings, &roots, cancel, &mut profile)?
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
                    if let Some(export)=&request.export{
                        let path=&export.path;
                        let before=crate::timing::enabled().then(||engine.gpu_activity());
                        engine.write_tiff(image,path,&mut profile)?;
                        if let Some(before)=before{crate::timing::emit("export",serde_json::json!({"revision":request.revision,"source":request.path,"destination":path,"settings":settings,"profile":profile,"activity":engine.gpu_activity().since(before)}));}
                        return Ok(None);
                    }
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
                        scopes:None,
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
        Self { pending, replies }
    }
    pub fn clear(&self) {
        let (lock, wake) = &*self.pending;
        let mut state = lock.lock().unwrap();
        state.clear = true;
        state.request = None;
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
        let (lock, wake) = &*self.pending;
        let mut state = lock.lock().unwrap();
        state.request = Some(request);
        state.queued_at = Some(std::time::Instant::now());
        wake.notify_one();
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

/// Camera defaults fill missing choices only. Otherwise loading metadata
/// could overwrite a launch override or an edit made while the photo was opening.
fn resolve_camera_settings(settings: &mut Settings, metadata: &Metadata) -> Vec<String> {
    let mut errors = Vec::new();
    for (value, recorded) in [
        (
            &mut settings.white_balance,
            metadata.recorded_white_balance(),
        ),
        (&mut settings.color_mode, metadata.recorded_color_mode()),
    ] {
        if value.is_empty() {
            match recorded {
                Ok(name) => *value = name,
                Err(error) => errors.push(error.to_string()),
            }
        }
    }
    errors
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
        settings.sensor_gain = 1.5;
        settings.reconstruction = Some(Default::default());
        settings.sharpen_amount = 0.75;
        settings.saturation = 1.2;
        for (index, (output, input)) in [
            ("gain.layers", "calibration.layers"),
            ("repair.layers", "gain.layers"),
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
            assert!(frame.scopes.is_none()); // image-first publication remains intact
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
        settings.sensor_gain = 1.;
        worker.submit(super::Request {
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
    fn defaults_resolve_independently_without_replacing_explicit_choices() {
        let metadata = foveonic::source::Metadata {
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
        let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f");
        let worker = super::Worker::new(
            foveonic::render::Engine::new().unwrap(),
            eframe::egui::Context::default(),
        );
        let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
        settings.sensor_gain = 1.3;
        settings.reconstruction = Some(Default::default());
        settings.sharpen_amount = 0.75;
        for revision in 0..10 {
            let control = if revision < 5 {
                settings.exposure = revision as f32 * 0.1;
                "exposure"
            } else if revision < 8 {
                settings.saturation = 1. + (revision - 5) as f32 * 0.1;
                "saturation"
            } else {
                settings.sharpen_amount = 0.8 + (revision - 8) as f32 * 0.1;
                "sharpen"
            };
            let start = std::time::Instant::now();
            worker.submit(super::Request {
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
            settings.sharpen_amount = 0.75;
            let submit = |revision, settings: foveonic::render::Settings| {
                worker.submit(super::Request {
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
                            expected.sharpen_amount = 0.75;
                            edit(&mut expected, control, reply.revision);
                            assert_eq!(frame.artifact.settings, expected);
                            assert!(frame.scopes.is_none());
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
                    request: None,
                    scopes: None,
                    queued_at: None,
                    stop: false,
                    clear: false,
                }),
                std::sync::Condvar::new(),
            )),
            replies,
        };
        for revision in 1..100 {
            worker.submit(super::Request {
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
        worker.clear();
        let state = worker.pending.0.lock().unwrap();
        assert!(state.request.is_none());
        assert!(state.clear);
    }
}
