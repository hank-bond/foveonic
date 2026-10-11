//! Runs one image job at a time, coalesces pending requests, and waits for GPU completion off the UI thread.

use eframe::egui;
use foveonic::{
    image::ImageTexture,
    inspection::{Artifact, Content},
    profile::Profile,
    render::{Engine, Settings},
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
    pub scopes: foveonic::scopes::Scopes,
    pub milliseconds: f64,
}
pub struct Reply {
    pub revision: u64,
    pub metadata: Option<Arc<Metadata>>,
    pub settings: Settings,
    pub result: Result<Option<Frame>, String>,
    pub exported: bool,
}

struct Pending {
    request: Option<Request>,
    stop: bool,
    clear: bool,
}
pub struct Worker {
    pending: Arc<(Mutex<Pending>, Condvar)>,
    pub replies: mpsc::Receiver<Reply>,
}
impl Worker {
    pub fn new(engine: Engine, context: egui::Context) -> Self {
        let pending = Arc::new((
            Mutex::new(Pending {
                request: None,
                stop: false,
                clear: false,
            }),
            Condvar::new(),
        ));
        let (tx, replies) = mpsc::sync_channel(1);
        let slot = pending.clone();
        thread::spawn(move || {
            let mut source: Option<(PathBuf, Source)> = None;
            loop {
                let request = {
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
                    state.request.take().unwrap()
                };
                let started = std::time::Instant::now();
                let mut metadata = None;
                let mut settings = request.settings.clone();
                let mut defaults_errors = Vec::new();
                let result = (|| -> foveonic::source::Result<Option<Frame>> {
                    if source.as_ref().is_none_or(|(p, _)| p != &request.path) {
                        source = None; // release the previous decoded image before loading another
                        source = Some((request.path.clone(), Source::open(&request.path)?));
                    }
                    let src = &source.as_ref().unwrap().1;
                    metadata = Some(src.metadata.clone());
                    defaults_errors = resolve_camera_settings(&mut settings, &src.metadata);
                    // A newer request can replace work that hasn't reached GPU submission yet.
                    let superseded = {
                        let state = slot.0.lock().unwrap();
                        state.request.is_some() || state.clear || state.stop
                    };
                    if superseded && request.export.is_none() {
                        return Ok(None);
                    }
                    let mut profile = Profile::default();
                    if let Some(path) = &request.export {
                        let image = engine.render(src, &settings, &mut profile)?;
                        engine.write_tiff(&image, path, &mut profile)?;
                        return Ok(None);
                    }
                    let artifact = engine
                        .render_artifacts(src, &settings, &[&request.output], &mut profile)?
                        .pop()
                        .ok_or("missing requested artifact")?;
                    let Content::Image(image) = &artifact.content else {
                        return Err(
                            "GUI inspection requires a visual image buffer; node-data diagnostics are deferred"
                                .into(),
                        );
                    };
                    let texture = Arc::new(engine.view_texture(image, None, &mut profile)?);
                    let scopes = engine.scopes(image, &mut profile)?;
                    engine.device().poll(eframe::wgpu::Maintain::Wait);
                    Ok(Some(Frame {
                        path: request.path.clone(),
                        output: request.output.clone(),
                        artifact,
                        texture,
                        scopes,
                        milliseconds: started.elapsed().as_secs_f64() * 1000.,
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
                    .send(Reply {
                        revision: request.revision,
                        metadata,
                        settings,
                        result,
                        exported: request.export.is_some(),
                    })
                    .is_err()
                {
                    break;
                }
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
        wake.notify_one();
    }
    pub fn submit(&self, request: Request) {
        let (lock, wake) = &*self.pending;
        lock.lock().unwrap().request = Some(request);
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
            let reply = worker
                .replies
                .recv_timeout(std::time::Duration::from_secs(20))
                .unwrap();
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
        let reply = worker
            .replies
            .recv_timeout(std::time::Duration::from_secs(20))
            .unwrap();
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
        let reply = worker
            .replies
            .recv_timeout(std::time::Duration::from_secs(20))
            .unwrap();
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
    fn pending_requests_replace_not_accumulate() {
        let (_, replies) = std::sync::mpsc::channel();
        let worker = super::Worker {
            pending: std::sync::Arc::new((
                std::sync::Mutex::new(super::Pending {
                    request: None,
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
