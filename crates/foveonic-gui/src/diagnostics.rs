//! Controls bounded manual and scripted captures through the shared background recorder.

use eframe::egui;
use foveonic::diagnostics::{self as trace, CaptureConfig, Mode, Recorder, SavedCapture};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::mpsc};

type Reply<T> = mpsc::Receiver<Result<T, Box<dyn std::error::Error + Send + Sync>>>;

pub enum Action {
    Start(Mode),
    Mark(String),
    StopSave(PathBuf),
}

pub struct Controller {
    recorder: Recorder,
    starting: Option<Reply<()>>,
    saving: Option<Reply<SavedCapture>>,
    manifest: Value,
    saved: Option<SavedCapture>,
    saved_directory: Option<PathBuf>,
    error: Option<String>,
    gpu: bool,
    marker: String,
    default_directory: Option<PathBuf>,
}
impl Default for Controller {
    fn default() -> Self {
        Self {
            recorder: Recorder::new(),
            starting: None,
            saving: None,
            manifest: Value::Null,
            saved: None,
            saved_directory: None,
            error: None,
            gpu: false,
            marker: "Slow interaction".into(),
            default_directory: std::env::var_os("FOVEONIC_DIAGNOSTICS_DIR").map(PathBuf::from),
        }
    }
}
impl Controller {
    pub fn start(&mut self, mode: Mode, manifest: Value) -> Result<(), String> {
        if self.starting() || self.saving() || self.active() {
            return Err("A diagnostic capture is already active or being saved".into());
        }
        let config = CaptureConfig {
            mode,
            ..CaptureConfig::default()
        };
        self.starting = Some(
            self.recorder
                .start(config)
                .map_err(|error| error.to_string())?,
        );
        self.manifest = json!({"application":"foveonic-gui", "version":env!("CARGO_PKG_VERSION"),
            "capture_mode":format!("{mode:?}"), "configuration":manifest,"capture_actions":[]});
        self.action("start", json!({"mode":format!("{mode:?}")}));
        self.saved = None;
        self.saved_directory = None;
        self.error = None;
        Ok(())
    }
    pub fn mark(&mut self, label: &str, metadata: Value) {
        self.action("mark", json!({"label":label,"metadata":metadata}));
        crate::timing::scenario_step(label, "user-marker", metadata);
    }
    pub fn set_start_state(&mut self, state: Value) {
        self.manifest["starting_state"] = state;
    }
    pub fn set_end_state(&mut self, state: Value) {
        self.manifest["ending_state"] = state;
    }
    fn action(&mut self, name: &str, details: Value) {
        if let Some(actions) = self.manifest["capture_actions"].as_array_mut() {
            if actions.len() < 256 {
                actions.push(json!({"action":name,"details":details}));
            } else {
                let omitted = self.manifest["omitted_capture_actions"]
                    .as_u64()
                    .unwrap_or(0);
                self.manifest["omitted_capture_actions"] = json!(omitted + 1);
            }
        }
    }
    pub fn stop_to(&mut self, directory: PathBuf) -> Result<(), String> {
        if !self.active() || self.starting() || self.saving() {
            return Err("No completed diagnostic capture is ready to save".into());
        }
        self.action("stop_save_requested", json!({"directory":directory}));
        self.saving = Some(
            self.recorder
                .stop_save(directory.clone(), self.manifest.clone())
                .map_err(|error| error.to_string())?,
        );
        self.saved_directory = Some(directory);
        Ok(())
    }
    pub fn save_on_exit_enabled(&self) -> bool {
        self.saving() || self.starting() || self.active()
    }
    /// Shutdown may wait for the recorder after interaction has ended. Recordings
    /// use temporary storage when no save directory was supplied.
    pub fn save_on_exit(&mut self) -> Result<(), String> {
        if self.saving.is_none() {
            let parent = self
                .default_directory
                .clone()
                .unwrap_or_else(|| std::env::temp_dir().join("foveonic-captures"));
            self.wait_for_start()?;
            if !self.active() {
                return Ok(());
            }
            self.action("application_exit", json!({}));
            self.stop_to(unique_directory(parent))?;
        }
        self.wait_for_save()
    }
    fn wait_for_start(&mut self) -> Result<(), String> {
        if let Some(reply) = self.starting.take() {
            reply
                .recv()
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
    fn wait_for_save(&mut self) -> Result<(), String> {
        if let Some(reply) = self.saving.take() {
            self.saved = Some(
                reply
                    .recv()
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?,
            );
        }
        Ok(())
    }
    /// Preserve an opt-in smoke capture during fatal unwinding. SDK control waits
    /// happen only after the measured interaction has already failed.
    pub fn save_smoke_failure(&mut self, directory: PathBuf) {
        let result = (|| -> Result<(), String> {
            if self.saving.is_none() {
                self.wait_for_start()?;
                if !self.active() {
                    return Ok(());
                }
                trace::event(trace::Category::Scenario, "smoke.failed", |fields| {
                    fields.str("reason", "panic");
                });
                if let Some(manifest) = self.manifest.as_object_mut() {
                    manifest.insert("failure".into(), json!({"reason":"smoke panic"}));
                }
                self.action("smoke_failed", json!({"reason":"panic"}));
                self.stop_to(directory)?;
            }
            self.wait_for_save()
        })();
        if let Err(error) = result {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "Cannot save failed smoke capture: {error}"
            );
        }
    }
    pub fn active(&self) -> bool {
        trace::enabled()
    }
    pub fn starting(&self) -> bool {
        self.starting.is_some()
    }
    pub fn saving(&self) -> bool {
        self.saving.is_some()
    }
    pub fn last_saved(&self) -> Option<&SavedCapture> {
        self.saved.as_ref()
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(reply) = &self.starting {
            match reply.try_recv() {
                Ok(result) => {
                    self.starting = None;
                    if let Err(error) = result {
                        self.error = Some(error.to_string());
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.starting = None;
                    self.error = Some("Diagnostic recorder disconnected while starting".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(reply) = &self.saving {
            match reply.try_recv() {
                Ok(result) => {
                    self.saving = None;
                    match result {
                        Ok(saved) => self.saved = Some(saved),
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.saving = None;
                    self.error = Some("Diagnostic recorder disconnected while saving".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.starting() || self.saving() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
    pub fn controls(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let active = self.active();
        let changing = self.starting() || self.saving();
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!active && !changing, |ui| {
                ui.add_enabled(
                    crate::timing::gpu_available(),
                    egui::Checkbox::new(&mut self.gpu, "GPU timestamps"),
                )
                .on_hover_text(
                    "Record supported device timestamps without adding completion waits.",
                )
                .on_disabled_hover_text(
                    "GPU timestamps are unavailable on this device. Host timing remains available.",
                );
                if ui.button("Start").clicked() {
                    action = Some(Action::Start(
                        if self.gpu && crate::timing::gpu_available() {
                            Mode::Gpu
                        } else {
                            Mode::Wall
                        },
                    ));
                }
            });
            if ui
                .add_enabled(active && !changing, egui::Button::new("Stop and save…"))
                .clicked()
            {
                if let Some(directory) = self.default_directory.clone().or_else(|| {
                    rfd::FileDialog::new()
                        .set_title("Save diagnostic capture folder")
                        .pick_folder()
                }) {
                    action = Some(Action::StopSave(unique_directory(directory)));
                }
            }
        });
        ui.horizontal(|ui| {
            ui.add_enabled(
                active && !changing,
                egui::TextEdit::singleline(&mut self.marker).desired_width(190.),
            );
            if ui
                .add_enabled(active && !changing, egui::Button::new("Mark"))
                .clicked()
            {
                action = Some(Action::Mark(self.marker.clone()));
            }
        });
        if self.starting() {
            ui.weak("Starting capture…");
        } else if self.saving() {
            ui.weak("Saving capture in the background…");
        } else if active {
            ui.weak("Recording a bounded local capture.");
        } else if self.last_saved().is_some() {
            if let Some(directory) = &self.saved_directory {
                ui.weak(format!("Saved to {}", directory.display()));
            }
        }
        if let Some(error) = self.error() {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.weak("Open trace.pftrace in Perfetto to inspect the timeline.");
        action
    }
    pub fn fail(&mut self, error: String) {
        self.error = Some(error);
    }
}

fn unique_directory(parent: PathBuf) -> PathBuf {
    static NEXT_SAVE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    parent.join(format!(
        "capture-{}-{:09}-{}-{}-{}",
        now.as_secs(),
        now.subsec_nanos(),
        std::process::id(),
        trace::capture_id().unwrap_or(0),
        NEXT_SAVE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}

#[cfg(test)]
#[test]
fn manual_saves_choose_distinct_children_without_creating_files() {
    let parent = PathBuf::from("capture-parent");
    let a = unique_directory(parent.clone());
    let b = unique_directory(parent.clone());
    assert_eq!(a.parent(), Some(parent.as_path()));
    assert_eq!(b.parent(), Some(parent.as_path()));
    assert_ne!(a, b);
}

#[cfg(test)]
#[test]
fn smoke_failure_waits_for_the_existing_save_without_starting_another() {
    let mut controller = Controller::default();
    let (send, receive) = mpsc::channel();
    let directory = PathBuf::from("existing-smoke-save");
    send.send(Ok(SavedCapture {
        trace: directory.join("trace.pftrace"),
        manifest: directory.join("manifest.json"),
        summary: directory.join("summary.json"),
        directory: directory.clone(),
        trace_bytes: 1,
    }))
    .unwrap();
    controller.saving = Some(receive);
    controller.save_smoke_failure(PathBuf::from("unused-save-destination"));
    assert!(!controller.saving());
    assert_eq!(controller.last_saved().unwrap().directory, directory);
}

#[cfg(test)]
#[test]
fn exit_finishes_an_existing_save_without_replacing_its_destination() {
    let mut controller = Controller {
        default_directory: Some(PathBuf::from("unused-exit-parent")),
        ..Default::default()
    };
    let (send, receive) = mpsc::channel();
    let directory = PathBuf::from("existing-manual-save");
    send.send(Ok(SavedCapture {
        trace: directory.join("trace.pftrace"),
        manifest: directory.join("manifest.json"),
        summary: directory.join("summary.json"),
        directory: directory.clone(),
        trace_bytes: 1,
    }))
    .unwrap();
    controller.saving = Some(receive);
    assert!(controller.save_on_exit_enabled());
    controller.save_on_exit().unwrap();
    assert_eq!(controller.last_saved().unwrap().directory, directory);
    assert!(!controller.saving());
}

#[cfg(test)]
#[test]
fn ordinary_exit_does_not_start_a_capture_or_create_output() {
    let mut controller = Controller {
        default_directory: None,
        ..Default::default()
    };
    assert!(!controller.save_on_exit_enabled());
    controller.save_on_exit().unwrap();
    assert!(controller.last_saved().is_none());
    assert!(controller.saved_directory.is_none());
}

#[cfg(test)]
#[test]
#[ignore = "owns global SDK capture state; run alone with --exact"]
fn manual_exit_preserves_a_recording_without_a_configured_directory() {
    let mut controller = Controller {
        default_directory: None,
        ..Default::default()
    };
    controller
        .start(Mode::Wall, json!({"entry":"manual"}))
        .unwrap();
    controller.wait_for_start().unwrap();
    assert!(controller.save_on_exit_enabled());
    controller.save_on_exit().unwrap();
    let saved = controller.last_saved().unwrap();
    assert_eq!(
        saved.directory.parent().unwrap(),
        std::env::temp_dir().join("foveonic-captures")
    );
    assert!(saved.trace_bytes > 0);
    assert!(saved.trace.is_file());
    std::fs::remove_dir_all(&saved.directory).unwrap();
}

/// Run alone with `--ignored --exact diagnostics::review_exit_saves_active_and_starting_captures`.
#[cfg(test)]
#[test]
#[ignore = "owns global SDK capture state; run alone with --exact"]
fn review_exit_saves_active_and_starting_captures() {
    for starting in [false, true] {
        let temporary = tempfile::tempdir().unwrap();
        let mut controller = Controller {
            default_directory: Some(temporary.path().to_owned()),
            ..Default::default()
        };
        controller
            .start(Mode::Wall, json!({"entry":"exit-save-proof"}))
            .unwrap();
        if !starting {
            controller.wait_for_start().unwrap();
            trace::event(trace::Category::Scenario, "exit-save.proof", |_| {});
        }
        controller.set_end_state(json!({"proof":"saved-on-close"}));
        assert!(controller.save_on_exit_enabled());
        controller.save_on_exit().unwrap();
        let saved = controller.last_saved().unwrap();
        assert!(saved.trace_bytes > 0);
        assert!(saved.trace.is_file());
        assert!(saved.summary.is_file());
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(&saved.manifest).unwrap()).unwrap();
        assert_eq!(
            manifest["application"]["ending_state"]["proof"],
            "saved-on-close"
        );
        assert!(manifest["application"]["capture_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| { action["action"] == "application_exit" }));
        let directory = saved.directory.clone();
        controller.save_on_exit().unwrap();
        assert_eq!(controller.last_saved().unwrap().directory, directory);
        assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 1);
    }
}

/// Run alone with `--ignored --exact diagnostics::smoke_panic_saves_active_sdk_capture`.
#[cfg(test)]
#[test]
#[ignore = "owns global SDK capture state; run alone with --exact"]
fn smoke_panic_saves_active_sdk_capture() {
    struct SaveOnPanic {
        controller: Controller,
        directory: PathBuf,
    }
    impl Drop for SaveOnPanic {
        fn drop(&mut self) {
            if std::thread::panicking() {
                self.controller.save_smoke_failure(self.directory.clone());
            }
        }
    }
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().join("capture");
    let mut controller = Controller::default();
    controller
        .start(Mode::Wall, json!({"entry":"smoke-unwind-proof"}))
        .unwrap();
    controller.starting.take().unwrap().recv().unwrap().unwrap();
    controller.set_start_state(json!({"proof":"preserved"}));
    let failure_directory = directory.clone();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _guard = SaveOnPanic {
            controller,
            directory: failure_directory,
        };
        panic!("original smoke panic");
    }))
    .unwrap_err();
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"original smoke panic"));
    assert!(!trace::enabled());
    let bytes = std::fs::read(directory.join("trace.pftrace")).unwrap();
    assert!(bytes
        .windows(b"smoke.failed".len())
        .any(|name| name == b"smoke.failed"));
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["application"]["failure"]["reason"], "smoke panic");
    assert_eq!(
        manifest["application"]["starting_state"]["proof"],
        "preserved"
    );
    assert!(directory.join("summary.json").is_file());

    // A failed save receipt must also preserve the panic that triggered unwinding.
    let mut controller = Controller::default();
    let (send, receive) = mpsc::channel();
    send.send(Err(std::io::Error::other("expected save error").into()))
        .unwrap();
    controller.saving = Some(receive);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _guard = SaveOnPanic {
            controller,
            directory,
        };
        panic!("original panic despite save error");
    }))
    .unwrap_err();
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"original panic despite save error")
    );
}

/// Isolated opt-in worker tests wait only outside the measured interaction.
#[cfg(test)]
pub struct TestCapture {
    controller: Controller,
    directory: PathBuf,
}
#[cfg(test)]
pub fn test_capture() -> Option<TestCapture> {
    let mode = crate::timing::requested_mode()?;
    let mut controller = Controller::default();
    controller
        .start(
            mode,
            json!({"entry":"isolated-worker-test","process_id":std::process::id()}),
        )
        .unwrap();
    controller.starting.take().unwrap().recv().unwrap().unwrap();
    let directory = std::env::var_os("FOVEONIC_DIAGNOSTICS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("foveonic-worker-capture-{}", std::process::id()))
        });
    Some(TestCapture {
        controller,
        directory,
    })
}
#[cfg(test)]
impl Drop for TestCapture {
    fn drop(&mut self) {
        if let Err(error) = self.controller.stop_to(self.directory.clone()) {
            eprintln!("Cannot save worker capture: {error}");
            return;
        }
        match self.controller.saving.take().unwrap().recv() {
            Ok(Ok(saved)) => eprintln!("Saved Perfetto capture: {}", saved.trace.display()),
            Ok(Err(error)) => eprintln!("Cannot save worker capture: {error}"),
            Err(error) => eprintln!("Worker capture recorder disconnected: {error}"),
        }
    }
}
