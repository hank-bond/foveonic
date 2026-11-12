//! Checks recovery after the running app attempts to load an isolated truncated RAW.

use super::{App, Smoke};
use eframe::egui;
use foveonic::render::Settings;
use serde_json::{json, Value};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::mpsc,
};

#[derive(Clone, Copy)]
enum Phase {
    Prepare,
    Create,
    Failure,
    Acknowledge,
    Recovery,
    Done,
}

pub(super) struct DamagedRecovery {
    phase: Phase,
    result: Option<mpsc::Receiver<Result<u64, String>>>,
    broken: Option<PathBuf>,
    checks: Vec<Value>,
}

impl DamagedRecovery {
    pub(super) fn new() -> Self {
        Self {
            phase: Phase::Prepare,
            result: None,
            broken: None,
            checks: Vec::new(),
        }
    }

    pub(super) fn advance(
        &mut self,
        app: &mut App,
        smoke: &mut Smoke,
        source: &Path,
        scratch: &Path,
        expected: &Settings,
    ) -> Result<bool, String> {
        match self.phase {
            Phase::Prepare => {
                if app.activity.unread_errors() || app.activity.is_open() {
                    return Err("damaged-file recovery requires a closed, acknowledged log".into());
                }
                let source = source.to_owned();
                let broken = scratch.join("truncated.x3f");
                self.broken = Some(broken.clone());
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let result = (|| -> Result<u64, String> {
                        let input = std::fs::File::open(source).map_err(|e| e.to_string())?;
                        let size = input.metadata().map_err(|e| e.to_string())?.len();
                        if size <= 4096 {
                            return Err("source is too short to truncate".into());
                        }
                        std::fs::create_dir_all(scratch_for(&broken)).map_err(|e| e.to_string())?;
                        let mut output = std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&broken)
                            .map_err(|e| e.to_string())?;
                        std::io::copy(&mut input.take(4096), &mut output).map_err(|e| e.to_string())
                    })();
                    let _ = tx.send(result);
                });
                self.result = Some(rx);
                self.phase = Phase::Create;
            }
            Phase::Create => {
                let result = match self.result.as_ref().unwrap().try_recv() {
                    Ok(result) => result?,
                    Err(mpsc::TryRecvError::Empty) => return Ok(false),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err("damaged-file preparation stopped".into())
                    }
                };
                self.checks.push(json!({"check":"truncated-owned-raw-created","passed":result == 4096,"bytes":result,"path":self.broken}));
                if result != 4096 {
                    return Err("truncated RAW did not contain 4096 bytes".into());
                }
                app.open(
                    self.broken.as_ref().unwrap().clone(),
                    crate::preferences::FolderMode::Browse,
                );
                self.phase = Phase::Failure;
            }
            Phase::Failure => {
                let broken = self.broken.as_ref().unwrap();
                if app.folder_loader.loading()
                    || app.selected.as_deref() != Some(broken.as_path())
                    || !app.sidecars.ready()
                    || app.worker.busy()
                    || !app.activity.unread_errors()
                {
                    return Ok(false);
                }
                let has_image = app
                    .frame
                    .as_ref()
                    .is_some_and(|frame| frame.path == *broken);
                self.checks.push(json!({"check":"truncated-raw-reports-error","passed":!has_image,"selected":broken,"unread_errors":true,"image_accepted":has_image}));
                if has_image {
                    return Err("truncated RAW unexpectedly produced an image".into());
                }
                app.activity.open();
                let acknowledged = app.activity.is_open() && !app.activity.unread_errors();
                self.checks
                    .push(json!({"check":"damaged-raw-error-acknowledged","passed":acknowledged}));
                if !acknowledged {
                    return Err("damaged RAW error was not acknowledged".into());
                }
                smoke.direct_events.push_back(vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
                self.phase = Phase::Acknowledge;
            }
            Phase::Acknowledge => {
                if !smoke.direct_events.is_empty() || app.activity.is_open() {
                    return Ok(false);
                }
                app.open(source.to_owned(), crate::preferences::FolderMode::Browse);
                self.phase = Phase::Recovery;
            }
            Phase::Recovery => {
                if app.folder_loader.loading()
                    || app.selected.as_deref() != Some(source)
                    || !app.sidecars.ready()
                    || app.worker.busy()
                {
                    return Ok(false);
                }
                let Some(frame) = app
                    .frame
                    .as_ref()
                    .filter(|frame| frame.path.as_path() == source)
                else {
                    return Ok(false);
                };
                if !app.worker.is_settled_and_admitted(frame) {
                    return Ok(false);
                }
                let matches =
                    crate::rendering::matching::same_authored_edits(&frame.requested, expected)
                        && crate::rendering::matching::same_authored_edits(&app.settings, expected)
                        && !app.activity.unread_errors();
                self.checks.push(json!({"check":"valid-raw-recovers-latest-frame","passed":matches,"source":source,"task":frame.diagnostic_task.id,"revision":frame.diagnostic_task.revision,"unread_errors":app.activity.unread_errors()}));
                if !matches {
                    return Err(
                        "valid RAW did not restore its expected edits without a new error".into(),
                    );
                }
                self.phase = Phase::Done;
            }
            Phase::Done => return Ok(true),
        }
        Ok(false)
    }

    pub(super) fn evidence(&self) -> Value {
        json!({"actions":["truncate-owned-raw","open-truncated-raw","acknowledge-log","reopen-valid-raw"],"checks":self.checks})
    }

    pub(super) fn expects_failure(&self, path: &Path) -> bool {
        matches!(self.phase, Phase::Failure) && self.broken.as_deref() == Some(path)
    }
}

fn scratch_for(path: &Path) -> &Path {
    path.parent().expect("trial file has a parent")
}

#[cfg(test)]
#[test]
fn expected_failure_is_limited_to_the_truncated_source_and_failure_phase() {
    let mut recovery = DamagedRecovery::new();
    recovery.broken = Some(PathBuf::from("truncated.x3f"));
    assert!(!recovery.expects_failure(Path::new("truncated.x3f")));
    recovery.phase = Phase::Failure;
    assert!(recovery.expects_failure(Path::new("truncated.x3f")));
    assert!(!recovery.expects_failure(Path::new("good.x3f")));
    recovery.phase = Phase::Recovery;
    assert!(!recovery.expects_failure(Path::new("truncated.x3f")));
}
