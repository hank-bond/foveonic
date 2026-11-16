//! Routes application instrumentation and bounded dependency warnings into the support store.

use super::{self as support, Level};
use serde_json::{json, Map, Value};
use std::{collections::HashMap, sync::Mutex};
use tracing::{
    field::{Field, Visit},
    Event, Subscriber,
};
use tracing_log::NormalizeEvent;
use tracing_subscriber::{layer::Context, prelude::*, registry::LookupSpan, Layer};

pub(super) fn install() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(DependencyEvents::default()),
    )?;
    tracing_log::LogTracer::init_with_filter(tracing_log::log::LevelFilter::Warn)?;
    Ok(())
}

#[derive(Default)]
struct DependencyEvents {
    counts: Mutex<HashMap<String, u32>>,
    #[cfg(test)]
    sink: Option<super::Handle>,
}

impl DependencyEvents {
    fn submit(&self, level: Level, name: &str, message: &str, fields: Value) {
        #[cfg(test)]
        if let Some(sink) = &self.sink {
            sink.event(level, name, message, fields);
            return;
        }
        support::event(level, name, message, fields);
    }
    fn occurrence(
        &self,
        level: tracing::Level,
        target: &str,
        file: Option<&str>,
        line: Option<u32>,
    ) -> u32 {
        if application_target(target) || level == tracing::Level::ERROR {
            return 0;
        }
        let key = format!("{target}:{file:?}:{line:?}:{level}");
        let mut counts = self.counts.lock().unwrap_or_else(|p| p.into_inner());
        if counts.len() >= 128 && !counts.contains_key(&key) {
            if counts.insert("callsite_limit".into(), 1).is_none() {
                return 21;
            }
            return 22;
        }
        let count = counts.entry(key).or_default();
        *count = count.saturating_add(1);
        *count
    }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for DependencyEvents {
    fn enabled(&self, metadata: &tracing::Metadata<'_>, _: Context<'_, S>) -> bool {
        supported_target(metadata.target())
            && (*metadata.level() <= tracing::Level::WARN
                || (application_target(metadata.target())
                    && *metadata.level() <= tracing::Level::INFO))
    }

    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        context: Context<'_, S>,
    ) {
        let mut fields = Fields::default();
        attributes.record(&mut fields);
        if let Some(span) = context.span(id) {
            span.extensions_mut().insert(fields);
        }
    }

    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        let normalized = event.normalized_metadata();
        let metadata = normalized.as_ref().unwrap_or_else(|| event.metadata());
        let suppressed = self.occurrence(
            *metadata.level(),
            metadata.target(),
            metadata.file(),
            metadata.line(),
        );
        if suppressed > 21 {
            return;
        }
        if suppressed == 21 {
            self.submit(
                Level::Warning,
                "dependency.repeated",
                "Further dependency messages suppressed for this launch",
                json!({"target":metadata.target(),"history_incomplete":true,"reason":"dependency message limit reached"}),
            );
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        if let Some(scope) = context.event_scope(event) {
            let spans: Vec<_> = scope
                .from_root()
                .filter_map(|span| {
                    span.extensions()
                        .get::<Fields>()
                        .map(|fields| Value::Object(fields.0.clone()))
                })
                .collect();
            if !spans.is_empty() {
                fields.0.insert("operation_context".into(), json!(spans));
            }
        }
        let event_name = fields
            .0
            .remove("event")
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "dependency.message".into());
        let message = fields
            .0
            .remove("message")
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| metadata.name().into());
        fields.0.insert("target".into(), json!(metadata.target()));
        fields.0.insert("capture_site".into(),json!({"file":metadata.file(),"line":metadata.line(),"kind":"dependency event, typed cause unavailable"}));
        if *metadata.level() == tracing::Level::ERROR {
            let trace = std::backtrace::Backtrace::force_capture();
            fields
                .0
                .insert("backtrace".into(), json!(format!("{trace:#}")));
            fields.0.insert(
                "backtrace_status".into(),
                json!(format!("{:?}", trace.status())),
            );
        }
        self.submit(
            if *metadata.level() == tracing::Level::ERROR {
                Level::Error
            } else if *metadata.level() == tracing::Level::WARN {
                Level::Warning
            } else {
                Level::Info
            },
            &event_name,
            &message,
            Value::Object(fields.0),
        );
    }
}

fn supported_target(target: &str) -> bool {
    [
        "foveonic",
        "foveonic_gui",
        "wgpu",
        "wgpu_core",
        "wgpu_hal",
        "naga",
        "winit",
        "eframe",
        "egui_wgpu",
        "smithay_client_toolkit",
        "calloop",
    ]
    .iter()
    .any(|prefix| {
        target == *prefix
            || target
                .strip_prefix(prefix)
                .is_some_and(|s| s.starts_with("::"))
    })
}

fn application_target(target: &str) -> bool {
    ["foveonic", "foveonic_gui"].iter().any(|prefix| {
        target == *prefix
            || target
                .strip_prefix(prefix)
                .is_some_and(|suffix| suffix.starts_with("::"))
    })
}

#[derive(Default)]
struct Fields(Map<String, Value>);
impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().into(), json!(format!("{value:?}")));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().into(), json!(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().into(), json!(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().into(), json!(value));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().into(), json!(value));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name().into(), json!(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridged_callsites_keep_their_location_and_independent_warning_budgets() {
        use tracing_log::log::{Log, Record};
        let directory = tempfile::tempdir().unwrap();
        let mut guard = super::super::store::start(super::super::Config {
            directory: directory.path().to_owned(),
            application: "subscriber test".into(),
            version: "1".into(),
            initial_context: json!({}),
        });
        let sink = guard.handle();
        let layer = DependencyEvents {
            sink: Some(sink.clone()),
            ..Default::default()
        };
        let dispatcher = tracing::Dispatch::new(tracing_subscriber::registry().with(layer));
        tracing::dispatcher::with_default(&dispatcher, || {
            let logger = tracing_log::LogTracer::new();
            for _ in 0..40 {
                logger.log(
                    &Record::builder()
                        .args(format_args!("repeated warning"))
                        .level(tracing_log::log::Level::Warn)
                        .target("wgpu_core::device")
                        .file(Some("first.rs"))
                        .line(Some(10))
                        .build(),
                );
            }
            logger.log(
                &Record::builder()
                    .args(format_args!("other warning"))
                    .level(tracing_log::log::Level::Warn)
                    .target("wgpu_core::device")
                    .file(Some("second.rs"))
                    .line(Some(20))
                    .build(),
            );
            logger.log(
                &Record::builder()
                    .args(format_args!("important error"))
                    .level(tracing_log::log::Level::Error)
                    .target("wgpu_core::device")
                    .file(Some("first.rs"))
                    .line(Some(10))
                    .build(),
            );
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let snapshot = loop {
            let snapshot = sink.snapshot();
            if snapshot
                .runs
                .iter()
                .flat_map(|run| &run.events)
                .any(|event| event.message == "important error")
            {
                break snapshot;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "bridged records were not delivered"
            );
            std::thread::yield_now();
        };
        let records: Vec<_> = snapshot.runs.iter().flat_map(|run| &run.events).collect();
        assert_eq!(
            records
                .iter()
                .filter(|record| record.message == "repeated warning")
                .count(),
            20
        );
        let other = records
            .iter()
            .find(|record| record.message == "other warning")
            .unwrap();
        assert_eq!(other.fields["target"], "wgpu_core::device");
        assert_eq!(other.fields["capture_site"]["file"], "second.rs");
        assert_eq!(other.fields["capture_site"]["line"], 20);
        let error = records
            .iter()
            .find(|record| record.message == "important error")
            .unwrap();
        assert_eq!(error.level, Level::Error);
        assert!(!error.fields["backtrace"].as_str().unwrap().is_empty());
        assert_eq!(
            guard.shutdown(std::time::Duration::from_secs(3)),
            super::super::ShutdownStatus::Drained
        );
    }
    #[test]
    fn dependency_filter_uses_complete_target_components() {
        assert!(supported_target("wgpu_core::device"));
        assert!(supported_target("foveonic::render"));
        assert!(supported_target("foveonic_gui::rendering::source_loader"));
        assert!(!supported_target("wgpu_unrelated"));
    }
    #[test]
    fn dependency_warning_flood_does_not_hide_errors_or_other_callsites() {
        let layer = DependencyEvents::default();
        for _ in 0..40 {
            layer.occurrence(
                tracing::Level::WARN,
                "wgpu_core::device",
                Some("device.rs"),
                Some(1),
            );
        }
        assert_eq!(
            layer.occurrence(
                tracing::Level::ERROR,
                "wgpu_core::device",
                Some("device.rs"),
                Some(1)
            ),
            0
        );
        assert_eq!(
            layer.occurrence(
                tracing::Level::WARN,
                "wgpu_core::device",
                Some("device.rs"),
                Some(2)
            ),
            1
        );
        assert_eq!(
            layer.occurrence(
                tracing::Level::INFO,
                "foveonic_gui::rendering",
                Some("mod.rs"),
                Some(1)
            ),
            0
        );
        for line in 3..1000 {
            layer.occurrence(
                tracing::Level::WARN,
                "wgpu_core::device",
                Some("device.rs"),
                Some(line),
            );
        }
        assert!(layer.counts.lock().unwrap().len() <= 129);
        assert_eq!(
            layer.occurrence(
                tracing::Level::ERROR,
                "wgpu_core::new",
                Some("other.rs"),
                Some(2)
            ),
            0
        );
    }
}
