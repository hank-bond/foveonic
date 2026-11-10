//! Connects GUI tasks, native drawing and existing measurements to the shared Perfetto capture.

use foveonic::{diagnostics as trace, profile::Mode};
use serde_json::Value;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        OnceLock,
    },
    time::Instant,
};

#[cfg(test)]
#[path = "timing_test.rs"]
mod tests;

static REQUESTED: OnceLock<Option<trace::Mode>> = OnceLock::new();
static GPU_AVAILABLE: AtomicBool = AtomicBool::new(false);
static NEXT_DRAW: AtomicU64 = AtomicU64::new(1);

pub fn configure() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mode = match std::env::var("FOVEONIC_PROFILE") {
        Ok(value) => value.parse::<Mode>()?,
        Err(std::env::VarError::NotPresent) => Mode::Off,
        Err(error) => return Err(error.into()),
    };
    REQUESTED
        .set(match mode {
            Mode::Off => None,
            Mode::Wall => Some(trace::Mode::Wall),
            Mode::Gpu => Some(trace::Mode::Gpu),
        })
        .map_err(|_| "profiling was already configured")?;
    Ok(())
}
pub fn requested_mode() -> Option<trace::Mode> {
    REQUESTED.get().copied().flatten()
}
/// Shared scopes and asynchronous GPU observations record GUI captures directly.
/// The GUI does not retain the separate headless aggregate Profile vectors.
pub fn mode() -> Mode {
    Mode::Off
}
pub fn enabled() -> bool {
    trace::enabled()
}
pub fn gpu_available() -> bool {
    GPU_AVAILABLE.load(Ordering::Relaxed)
}
pub fn required_features(supported: eframe::wgpu::Features) -> eframe::wgpu::Features {
    supported & eframe::wgpu::Features::TIMESTAMP_QUERY
}
pub fn check_gpu_features(features: eframe::wgpu::Features) -> Result<(), &'static str> {
    GPU_AVAILABLE.store(
        features.contains(eframe::wgpu::Features::TIMESTAMP_QUERY),
        Ordering::Relaxed,
    );
    Ok(())
}
fn annotate(fields: &mut trace::Fields, prefix: &str, value: &Value) {
    let Value::Object(values) = value else {
        return;
    };
    for (name, value) in values {
        let name = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        match value {
            Value::Bool(value) => {
                fields.bool(&name, *value);
            }
            Value::Number(value) => {
                if let Some(value) = value.as_u64() {
                    fields.u64(&name, value);
                } else if let Some(value) = value.as_i64() {
                    fields.i64(&name, value);
                } else if let Some(value) = value.as_f64() {
                    fields.f64(&name, value);
                }
            }
            Value::String(value) => {
                fields.str(&name, value);
            }
            Value::Object(_) if name != "settings" => annotate(fields, &name, value),
            Value::Null => {}
            value => {
                fields.str(&name, &value.to_string());
            }
        }
    }
}
/// Cold metadata snapshots use the same recorder as typed execution scopes.
/// The closure is never called while recording is disabled.
pub fn emit(event: &'static str, data: impl FnOnce() -> Value) {
    let category = match event {
        "memory" => trace::Category::Memory,
        "processing" | "view" => trace::Category::Graph,
        "source" => trace::Category::Cache,
        _ => trace::Category::Ui,
    };
    trace::event(category, event, |fields| annotate(fields, "", &data()));
}
pub fn event(revision: u64, phase: &'static str) {
    trace::event(trace::Category::Ui, phase, |fields| {
        fields.u64("revision", revision);
    });
}
pub fn pointer_input(input: &eframe::egui::RawInput, pointer_tool: bool) {
    if !enabled() {
        return;
    }
    for event in &input.events {
        if let eframe::egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers,
        } = event
        {
            trace::event(trace::Category::Ui, "input.pointer_button", |fields| {
                fields
                    .f64("x", pos.x as f64)
                    .f64("y", pos.y as f64)
                    .str("button", &format!("{button:?}"))
                    .bool("pressed", *pressed)
                    .bool("alt", modifiers.alt)
                    .bool("shift", modifiers.shift)
                    .bool("command", modifiers.command)
                    .bool("focused", input.focused)
                    .bool("pointer_tool", pointer_tool);
            });
        }
    }
}
pub fn selected(revision: u64, path: &std::path::Path) {
    trace::event(trace::Category::Ui, "photo_selected", |fields| {
        fields.u64("revision", revision);
        fields.str("source", &path.to_string_lossy());
    });
}
pub fn selection_ready(revision: u64, path: &std::path::Path, began: Instant) {
    trace::interval_since(trace::Category::Ui, "selection_recipe", began, |fields| {
        fields.u64("revision", revision);
        fields.str("source", &path.to_string_lossy());
    });
}
pub fn requested(
    revision: u64,
    path: &std::path::Path,
    output: &str,
    settings: &foveonic::render::Settings,
) {
    trace::event(trace::Category::Ui, "request", |fields| {
        fields.u64("revision", revision);
        fields.str("source", &path.to_string_lossy());
        fields.str("output", output);
        fields.str(
            "settings",
            &serde_json::to_string(settings).unwrap_or_default(),
        );
    });
}
pub fn worker_started(revision: u64, queued_at: Instant, started: Instant) {
    if let (Some(queued), Some(started)) =
        (trace::timestamp_at(queued_at), trace::timestamp_at(started))
    {
        trace::interval(
            trace::Category::Coordinator,
            "render_queue",
            queued,
            started,
            |fields| {
                if trace::current_context().is_none() {
                    fields.u64("revision", revision);
                }
            },
        );
    }
}
pub fn native_frame(revision: u64, previous_cpu: Option<f32>, gap_ms: Option<f64>, busy: bool) {
    trace::event(
        trace::Category::Presentation,
        "native_frame_update",
        |fields| {
            fields.u64("presentation_revision", revision);
            fields.bool("render_pending", busy);
            if let Some(value) = previous_cpu {
                fields.f64("previous_egui_cpu_ms", f64::from(value) * 1000.);
            }
            if let Some(value) = gap_ms {
                fields.f64("frame_gap_ms", value);
            }
        },
    );
}
pub fn image_received(
    stamp: &crate::rendering::coordinator::Stamp,
    current: bool,
    queued_at: Instant,
    worker_ms: f64,
) {
    trace::interval_since(
        trace::Category::Presentation,
        "request_to_ui",
        queued_at,
        |fields| {
            let context = context(stamp);
            fields
                .u64("task", context.task)
                .u64("revision", context.revision)
                .u64("photo", context.photo)
                .str("purpose", context.purpose);
            fields.flow(
                flow_id("gui-delivery", context.task, context.revision),
                false,
            );
            fields.bool("current", current);
            fields.f64("worker_elapsed_ms", worker_ms);
        },
    );
}
pub fn scenario_step(name: &str, action_kind: &str, metadata: Value) {
    trace::event(trace::Category::Scenario, name, |fields| {
        fields.str("action_kind", action_kind);
        annotate(fields, "", &metadata);
    });
}
pub fn task(
    stamp: &crate::rendering::coordinator::Stamp,
    phase: &'static str,
    metadata: impl FnOnce() -> Value,
) {
    if !enabled() {
        return;
    }
    let _context = trace::enter(context(stamp));
    let terminating = matches!(
        phase,
        "completed"
            | "completion_removed"
            | "completion_superseded"
            | "rejected"
            | "cancelled"
            | "replaced"
    );
    trace::flow(
        trace::Category::Coordinator,
        phase,
        flow_id("gui-render", stamp.id.0, stamp.revision),
        terminating,
        |fields| {
            fields.str("source", &stamp.target.path.to_string_lossy());
            annotate(fields, "", &metadata());
        },
    );
}
fn flow_id(domain: &str, task: u64, revision: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut flow = std::collections::hash_map::DefaultHasher::new();
    (domain, task, revision).hash(&mut flow);
    flow.finish()
}
pub fn purpose(purpose: &crate::rendering::request::Purpose) -> &'static str {
    use crate::rendering::request::Purpose;
    match purpose {
        Purpose::Current => "current",
        Purpose::Camera => "camera",
        Purpose::Defaults => "defaults",
        Purpose::Detail => "detail",
        Purpose::Inspection(_, _) => "inspection",
        Purpose::Export => "export",
    }
}

/// The processing identity survives later presentation-only revision changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeliveredTask {
    pub id: u64,
    pub revision: u64,
    pub purpose: &'static str,
}
impl From<&crate::rendering::coordinator::Stamp> for DeliveredTask {
    fn from(stamp: &crate::rendering::coordinator::Stamp) -> Self {
        Self {
            id: stamp.id.0,
            revision: stamp.revision,
            purpose: purpose(&stamp.target.purpose),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageIdentity {
    pub context: trace::Context,
    pub presentation_revision: u64,
    pub artifact: &'static str,
    pub scale: u32,
}
impl DeliveredTask {
    pub fn image(
        self,
        presentation_revision: u64,
        path: &std::path::Path,
        artifact: &'static str,
        scale: u32,
    ) -> ImageIdentity {
        ImageIdentity {
            context: context_for(self.id, self.revision, path, self.purpose),
            presentation_revision,
            artifact,
            scale,
        }
    }
}
impl ImageIdentity {
    fn annotate(self, fields: &mut trace::Fields<'_>, draw: u64) {
        fields
            .u64("task", self.context.task)
            .u64("revision", self.context.revision)
            .u64("photo", self.context.photo)
            .str("purpose", self.context.purpose)
            .u64("presentation_revision", self.presentation_revision)
            .str("artifact", self.artifact)
            .u64("scale", u64::from(self.scale))
            .u64("draw", draw);
    }
}
pub fn context(stamp: &crate::rendering::coordinator::Stamp) -> trace::Context {
    context_for(
        stamp.id.0,
        stamp.revision,
        &stamp.target.path,
        purpose(&stamp.target.purpose),
    )
}
pub fn context_for(
    task: u64,
    revision: u64,
    path: &std::path::Path,
    purpose: &'static str,
) -> trace::Context {
    trace::Context {
        task,
        revision,
        photo: trace::photo_id(path),
        purpose,
        node: None,
    }
}
pub struct Span(Option<trace::Scope>);
impl Span {
    pub fn new(revision: u64, phase: &'static str) -> Self {
        let category = if phase.starts_with("draw_") {
            trace::Category::Presentation
        } else if phase == "reply_publication" {
            trace::Category::Coordinator
        } else if phase == "source_open" {
            trace::Category::Io
        } else {
            trace::Category::Ui
        };
        Self(enabled().then(|| {
            trace::scope_with(category, phase, |fields| {
                if phase == "ui_update" {
                    fields.u64("ui_revision", revision);
                } else if trace::current_context().is_none() {
                    fields.u64("revision", revision);
                }
                if matches!(phase, "view_completion_wait" | "reply_publication") {
                    fields.str("measurement", "host_wait");
                } else {
                    fields.str("measurement", "host_elapsed");
                }
            })
        }))
    }
}

pub fn activity(fields: &mut trace::Fields<'_>, activity: foveonic::profile::GpuActivity) {
    fields
        .u64("upload_bytes", activity.upload_bytes)
        .u64("uniform_bytes", activity.uniform_bytes)
        .u64("storage_bytes", activity.storage_bytes)
        .u64("allocations", activity.allocations)
        .u64("dispatches", activity.dispatches)
        .u64("submissions", activity.submissions)
        .u64("device_copy_bytes", activity.device_copy_bytes)
        .u64("readback_bytes", activity.readback_bytes)
        .u64("readbacks", activity.readbacks)
        .u64("timestamp_resolves", activity.timestamp_resolves);
}
impl Drop for Span {
    fn drop(&mut self) {
        self.0.take();
    }
}
struct DrawState {
    image: ImageIdentity,
    id: u64,
    prepared: OnceLock<Instant>,
}
pub struct DrawProbe(Option<DrawState>);
impl DrawProbe {
    pub fn new(image: Option<ImageIdentity>) -> Self {
        Self(image.filter(|_| enabled()).map(|image| DrawState {
            image,
            id: NEXT_DRAW.fetch_add(1, Ordering::Relaxed),
            prepared: OnceLock::new(),
        }))
    }
    pub fn prepare(&self) -> Span {
        self.span("draw_prepare")
    }
    pub fn prepared(&self) {
        if let Some(state) = &self.0 {
            let _ = state.prepared.set(Instant::now());
        }
    }
    pub fn encode(&self) -> Span {
        if let Some(state) = &self.0 {
            trace::event(trace::Category::Presentation, "image_frame_use", |fields| {
                state.image.annotate(fields, state.id);
                fields.flow(
                    flow_id(
                        "gui-delivery",
                        state.image.context.task,
                        state.image.context.revision,
                    ),
                    true,
                );
                fields.str("milestone", "native_command_encoding");
                if let Some(prepared) = state.prepared.get() {
                    fields.f64(
                        "prepared_to_encode_ms",
                        prepared.elapsed().as_secs_f64() * 1000.,
                    );
                }
            });
        }
        self.span("draw_encode")
    }
    fn span(&self, phase: &'static str) -> Span {
        match &self.0 {
            Some(state) => Span(enabled().then(|| {
                trace::scope_with(trace::Category::Presentation, phase, |fields| {
                    state.image.annotate(fields, state.id);
                    fields.str("measurement", "host_elapsed");
                })
            })),
            None => Span(None),
        }
    }
}
