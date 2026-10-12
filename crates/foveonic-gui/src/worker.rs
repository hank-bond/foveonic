//! Runs one image job at a time, coalesces pending requests, and waits for GPU completion off the UI thread.

use eframe::egui;
use foveonic::{
    image::ImageTexture,
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

pub struct Request {
    pub revision: u64,
    pub path: PathBuf,
    pub settings: Settings,
    pub output: String,
    pub export: Option<PathBuf>,
}
pub struct Frame {
    pub path: PathBuf,
    pub output: String,
    pub artifact: Artifact,
    pub texture: Arc<ImageTexture>,
    pub scopes: Option<foveonic::scopes::Scopes>,
    pub milliseconds: f64,
    pub revision: u64,
    pub queued_at: std::time::Instant,
}
pub struct Reply {
    pub revision: u64,
    pub metadata: Option<Arc<Metadata>>,
    pub settings: Settings,
    pub result: Result<Option<Frame>, String>,
    pub exported: bool,
}

pub enum Update {
    Image(Box<Reply>),
    Scopes {
        revision: u64,
        result: Result<foveonic::scopes::Scopes, String>,
    },
}

struct Pending {
    request: Option<Request>,
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
                    while !state.stop && !state.clear && state.request.is_none() {
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
                    (
                        state.request.take().unwrap(),
                        state.queued_at.take().unwrap(),
                    )
                };
                let started = std::time::Instant::now();
                let mut metadata = None;
                let mut settings = request.settings.clone();
                let mut defaults_errors = Vec::new();
                let mut scope_image = None;
                let result = (|| -> foveonic::source::Result<Option<Frame>> {
                    if source.as_ref().is_none_or(|(p, _)| p != &request.path) {
                        source = None; // release the previous decoded image before loading another
                        source = Some((request.path.clone(), RenderSession::new(Source::open(&request.path)?)));
                    }
                    let session = &mut source.as_mut().unwrap().1;
                    metadata = Some(session.metadata().clone());
                    defaults_errors = resolve_camera_settings(&mut settings, session.metadata());
                    let edit=session.changed_parameters(&settings);
                    let cancel=||{
                        if request.export.is_some(){return false;}
                        let state=slot.0.lock().unwrap();
                        state.clear||state.stop||state.request.as_ref().is_some_and(|pending|{
                            let mut next=pending.settings.clone();
                            resolve_camera_settings(&mut next,metadata.as_ref().unwrap());
                            pending.path!=request.path||pending.output!=request.output||(!edit.is_empty()&&settings.changed_parameters(&next).iter().any(|parameter|!edit.contains(parameter)))
                        })
                    };
                    let mut profile = Profile::default();
                    let root=if request.export.is_some(){"final"}else{&request.output};
                    let Some(mut artifacts)=engine.render_session(session,&settings,&[root],cancel,&mut profile)? else{return Ok(None);};
                    let artifact = artifacts.pop().ok_or("missing requested artifact")?;
                    let Content::Image(image) = &artifact.content else {
                        return Err(
                            "GUI inspection requires a visual image buffer; node-data diagnostics are deferred"
                                .into(),
                        );
                    };
                    if let Some(path)=&request.export{engine.write_tiff(image,path,&mut profile)?;return Ok(None);}
                    if cancel(){return Ok(None);}
                    let texture = Arc::new(engine.view_texture(image, None, &mut profile)?);
                    engine.device().poll(eframe::wgpu::Maintain::Wait);
                    scope_image=Some((**image).clone());
                    if std::env::var_os("FOVEONIC_UI_TIMING").is_some(){eprintln!("GPU-CACHE revision={} bytes={}",request.revision,session.cached_bytes());}
                    Ok(Some(Frame {
                        path: request.path.clone(),
                        output: request.output.clone(),
                        artifact,
                        texture,
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
                if tx
                    .send(Update::Image(Box::new(Reply {
                        revision: request.revision,
                        metadata,
                        settings,
                        result,
                        exported: request.export.is_some(),
                    })))
                    .is_err()
                {
                    break;
                }
                context.request_repaint();
                if let Some(image) = scope_image {
                    // Image publication is independent; latest image work outranks scope completion.
                    if scope_superseded(&slot) {
                        continue;
                    }
                    let result = engine
                        .scopes(&image, &mut Profile::default())
                        .map_err(|error| error.to_string());
                    if scope_superseded(&slot) {
                        continue;
                    }
                    if tx
                        .send(Update::Scopes {
                            revision: request.revision,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                    context.request_repaint();
                }
            }
        });
        Self { pending, replies }
    }
    pub fn clear(&self) {
        let (lock, wake) = &*self.pending;
        let mut state = lock.lock().unwrap();
        state.clear = true;
        state.request = None;
        state.queued_at = None;
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

fn scope_superseded(slot: &Arc<(Mutex<Pending>, Condvar)>) -> bool {
    let state = slot.0.lock().unwrap();
    state.stop || state.clear || state.request.is_some()
}

/// Resolve only unspecified fields: explicit launch and current-image edits win.
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
                        }
                    } else if let super::Update::Scopes { revision, result } = update {
                        if revision == last {
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
                        eprintln!(
                        "DRAG control={control} published_during_drag={} duration_ms={:.3} settled_ms={:.3}",
                        progress.len(),
                        start.elapsed().as_secs_f64() * 1000.,
                        stopped.elapsed().as_secs_f64() * 1000.
                    );
                    }
                    super::Update::Scopes { revision, result } if revision == last => {
                        assert!(result.is_ok());
                        scopes = true;
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn pending_requests_replace_not_accumulate() {
        let (_, replies) = std::sync::mpsc::channel();
        let worker = super::Worker {
            pending: std::sync::Arc::new((
                std::sync::Mutex::new(super::Pending {
                    request: None,
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
