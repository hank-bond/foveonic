//! Stores bounded, user-visible support reports independently of performance captures.

mod application;
mod report;
mod store;
mod subscriber;
mod system;

pub use application::{build_identity, default_directory, start_application, Application};
pub use report::{FailureCause, FailureReport, OperationContext, ReportedError};
pub use store::{ClearRequest, Guard, Handle, ShutdownStatus, Status};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

/// Freeze the complete processing request without changing photo-sidecar serialization.
pub fn settings_snapshot(settings: &crate::render::Settings) -> Value {
    #[derive(Serialize)]
    struct SettingsSnapshot<'a> {
        #[serde(flatten)]
        authored: &'a crate::render::Settings,
        dehaze_estimate: bool,
        processing_sections: crate::render::ProcessingSections,
        working_scale: crate::render::Scale,
        processing_quality: crate::render::ProcessingQuality,
        preview_region: Option<[u32; 4]>,
        color_range_preview: Option<usize>,
        adaptive_tone_tuning: crate::render::AdaptiveToneTuning,
    }
    serde_json::to_value(SettingsSnapshot {
        authored: settings,
        dehaze_estimate: settings.dehaze_estimate,
        processing_sections: settings.processing_sections,
        working_scale: settings.working_scale,
        processing_quality: settings.processing_quality,
        preview_region: settings.preview_region,
        color_range_preview: settings.color_range_preview,
        adaptive_tone_tuning: settings.adaptive_tone_tuning,
    })
    .unwrap_or_else(|error| {
        serde_json::json!({"status":"unavailable","reason":format!("Settings snapshot failed: {error}")})
    })
}

/// Storage and initial launch information supplied by the executable.
#[derive(Clone, Debug)]
pub struct Config {
    pub directory: PathBuf,
    pub application: String,
    pub version: String,
    pub initial_context: Value,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warning,
    Error,
}

/// One versioned event. Sequence is authoritative within its launch.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Record {
    pub version: u32,
    pub launch_id: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub thread: String,
    pub level: Level,
    pub event: String,
    pub message: String,
    pub fields: Value,
}

#[derive(Clone, Debug, Default)]
pub struct RunSnapshot {
    pub launch_id: String,
    pub started_at_ms: u64,
    pub context: Value,
    pub events: Vec<Record>,
    pub incomplete: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub runs: Vec<RunSnapshot>,
    pub incomplete: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum VisibleSelection {
    All,
    Run(String),
}

impl Snapshot {
    /// Format exactly the same retained entries presented by this snapshot.
    pub fn copy_report(&self, selection: VisibleSelection) -> String {
        let mut text = String::new();
        for notice in &self.incomplete {
            text.push_str(&format!("History incomplete: {notice}\n"));
        }
        for run in &self.runs {
            if let VisibleSelection::Run(id) = &selection {
                if id != &run.launch_id {
                    continue;
                }
            }
            text.push_str(&format!(
                "\nRun {} ({})\n",
                run.launch_id,
                utc_timestamp(run.started_at_ms)
            ));
            text.push_str(&format!("Context: {}\n", run.context));
            for notice in &run.incomplete {
                text.push_str(&format!("History incomplete: {notice}\n"));
            }
            for event in &run.events {
                text.push_str(&format!(
                    "{} #{:06} {:?} {}: {}",
                    event.timestamp_ms, event.sequence, event.level, event.event, event.message
                ));
                if !event.fields.is_null() {
                    text.push_str(&format!(" {}", event.fields));
                }
                text.push('\n');
            }
        }
        text
    }
}

fn utc_timestamp(milliseconds: u64) -> String {
    match time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(milliseconds) * 1_000_000) {
        Ok(time) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} UTC",
            time.year(),
            u8::from(time.month()),
            time.day(),
            time.hour(),
            time.minute(),
            time.second(),
            time.millisecond()
        ),
        Err(_) => format!("{milliseconds} milliseconds since Unix epoch (date out of range)"),
    }
}

fn global() -> &'static Mutex<Option<Handle>> {
    static GLOBAL: OnceLock<Mutex<Option<Handle>>> = OnceLock::new();
    GLOBAL.get_or_init(|| Mutex::new(None))
}

/// Start one application-owned store. Filesystem failures retain an in-memory view.
pub fn initialize(config: Config) -> Guard {
    let guard = store::start(config);
    store::install_panic_hook(&guard.handle());
    if let Ok(mut global) = global().lock() {
        *global = Some(guard.handle());
    }
    guard
}

pub fn event(level: Level, name: &str, message: &str, fields: Value) {
    if let Some(handle) = global().lock().ok().and_then(|guard| guard.clone()) {
        handle.event(level, name, message, fields);
    }
}

/// Record a recoverable failure without treating expected fallback as fatal.
pub fn warning_error(
    boundary: &str,
    error: &(dyn std::error::Error + 'static),
    operation: OperationContext,
) {
    let mut causes = Vec::new();
    let mut current = Some(error);
    for _ in 0..32 {
        let Some(cause) = current else {
            break;
        };
        let io = cause.downcast_ref::<std::io::Error>();
        causes.push(FailureCause {
            message: cause.to_string(),
            io_kind: io.map(|e| format!("{:?}", e.kind())),
            os_code: io.and_then(std::io::Error::raw_os_error),
        });
        current = cause.source();
    }
    event(
        Level::Warning,
        "operation.warning",
        &error.to_string(),
        serde_json::json!({
            "boundary": boundary, "causes": causes, "operation": operation,
        }),
    );
}

/// Identify cooperative background cancellation without converting its type.
pub fn is_interrupted(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(error);
    for _ in 0..32 {
        let Some(cause) = current else {
            break;
        };
        if cause.is::<crate::background::Interrupted>() {
            return true;
        }
        current = cause.source();
    }
    false
}

pub fn update_context(section: &str, fields: Value) {
    if let Some(handle) = global().lock().ok().and_then(|guard| guard.clone()) {
        handle.update_context(section, fields);
    }
}

pub fn history() -> Snapshot {
    global()
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(Handle::snapshot))
        .unwrap_or_default()
}

pub fn handle() -> Option<Handle> {
    global().lock().ok().and_then(|guard| guard.clone())
}

#[track_caller]
pub fn report_error(
    boundary: &str,
    error: &(dyn std::error::Error + 'static),
    operation: OperationContext,
) -> Arc<FailureReport> {
    let mut current = Some(error);
    for _ in 0..32 {
        let Some(cause) = current else {
            break;
        };
        if let Some(reported) = cause.downcast_ref::<ReportedError>() {
            event(
                Level::Error,
                "operation.context",
                "Additional failure context",
                serde_json::json!({
                    "failure_id": reported.report.id,
                    "boundary": boundary,
                    "operation": operation,
                }),
            );
            return reported.report.clone();
        }
        current = cause.source();
    }
    let report = Arc::new(report::capture(
        boundary,
        error,
        operation,
        std::panic::Location::caller(),
    ));
    event(
        Level::Error,
        "operation.failed",
        &report.message,
        serde_json::to_value(&*report).unwrap_or(Value::Null),
    );
    report
}

/// Preserve the original terminal error and its first boundary report together.
#[track_caller]
pub fn reported_error(
    boundary: &str,
    error: Box<dyn std::error::Error + Send + Sync>,
    operation: OperationContext,
) -> Box<dyn std::error::Error + Send + Sync> {
    let report = report_error(boundary, error.as_ref(), operation);
    Box::new(ReportedError {
        report,
        source: error,
    })
}

#[cfg(test)]
mod tests;
