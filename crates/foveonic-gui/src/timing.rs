//! Measures GUI events when requested without changing when the app repaints.

use foveonic::profile::{self, Mode};
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        OnceLock,
    },
    time::Instant,
};

#[cfg(test)]
#[path = "timing_test.rs"]
mod tests;

static MODE: OnceLock<Mode> = OnceLock::new();
static ORIGIN: OnceLock<Instant> = OnceLock::new();
static NEXT_DRAW: AtomicU64 = AtomicU64::new(1);

/// Configure before native device creation. Historical flags are not live aliases.
pub fn configure() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mode = match std::env::var("FOVEONIC_PROFILE") {
        Ok(value) => value.parse()?,
        Err(std::env::VarError::NotPresent) => Mode::Off,
        Err(error) => return Err(error.into()),
    };
    MODE.set(mode)
        .map_err(|_| "profiling was already configured")?;
    Ok(())
}
pub fn mode() -> Mode {
    *MODE.get_or_init(|| Mode::Off)
}
pub fn enabled() -> bool {
    mode().enabled()
}

fn selected_features(mode: Mode, supported: eframe::wgpu::Features) -> eframe::wgpu::Features {
    if mode.gpu() {
        supported & eframe::wgpu::Features::TIMESTAMP_QUERY
    } else {
        eframe::wgpu::Features::empty()
    }
}
pub fn required_features(supported: eframe::wgpu::Features) -> eframe::wgpu::Features {
    selected_features(mode(), supported)
}
fn check_features(mode: Mode, features: eframe::wgpu::Features) -> Result<(), &'static str> {
    mode.check_gpu_features(features)
}
pub fn check_gpu_features(features: eframe::wgpu::Features) -> Result<(), &'static str> {
    check_features(mode(), features)
}

/// Keep each report on one marked JSON line so capture tools can separate
/// measurements from unrelated stderr messages.
pub fn emit(event: &'static str, data: impl serde::Serialize) {
    if enabled() {
        eprintln!("{}", profile::report(event, mode(), data));
    }
}
fn clock() -> Instant {
    *ORIGIN.get_or_init(Instant::now)
}
fn milliseconds(start: Instant, finish: Instant) -> f64 {
    finish.saturating_duration_since(start).as_secs_f64() * 1000.
}
pub fn event(revision: u64, phase: &'static str) {
    if enabled() {
        emit(
            "lifecycle",
            json!({"revision":revision,"phase":phase,"at_ms":milliseconds(clock(), Instant::now())}),
        );
    }
}
pub fn selected(revision: u64, path: &std::path::Path) {
    if enabled() {
        emit(
            "selection",
            json!({"revision":revision,"source":path,"phase":"started","at_ms":milliseconds(clock(),Instant::now())}),
        );
    }
}
pub fn selection_ready(revision: u64, path: &std::path::Path, began: Instant) {
    if enabled() {
        emit(
            "selection",
            json!({"revision":revision,"source":path,"phase":"recipe_ready","elapsed_ms":began.elapsed().as_secs_f64()*1000.,"at_ms":milliseconds(clock(),Instant::now())}),
        );
    }
}
pub fn requested(
    revision: u64,
    path: &std::path::Path,
    output: &str,
    settings: &foveonic::render::Settings,
) {
    if enabled() {
        emit(
            "request",
            json!({"revision":revision,"source":path,"output":output,"settings":settings,"at_ms":milliseconds(clock(), Instant::now())}),
        );
    }
}
pub fn worker_started(revision: u64, queued_at: Instant, started: Instant) {
    if enabled() {
        emit(
            "lifecycle",
            json!({"revision":revision,"phase":"worker_started","at_ms":milliseconds(clock(),started),"queue_ms":milliseconds(queued_at,started)}),
        );
    }
}
/// Previous native CPU excludes surface acquire, submission and presentation waits.
pub fn native_frame(revision: u64, previous_cpu: Option<f32>, gap_ms: Option<f64>, busy: bool) {
    if enabled() {
        emit(
            "frame",
            json!({"revision":revision,"at_ms":milliseconds(clock(),Instant::now()),"previous_cpu_ms":previous_cpu.map(|v|v*1000.),"gap_ms":gap_ms,"busy":busy}),
        );
    }
}
pub fn image_received(revision: u64, current: bool, queued_at: Instant, worker_ms: f64) {
    if enabled() {
        emit(
            "image",
            json!({"revision":revision,"current":current,"queue_to_ui_ms":queued_at.elapsed().as_secs_f64()*1000.,"worker_ms":worker_ms}),
        );
    }
}

struct Record {
    revision: u64,
    draw: u64,
    phase: &'static str,
    origin: Instant,
    start: Instant,
    finish: Instant,
}
impl Record {
    fn data(&self) -> serde_json::Value {
        json!({"revision":self.revision,"draw":self.draw,"phase":self.phase,
            "start_ms":milliseconds(self.origin,self.start),"end_ms":milliseconds(self.origin,self.finish),
            "elapsed_ms":milliseconds(self.start,self.finish)})
    }
}
/// Leave ordinary runs free of clock sampling. When enabled, measure existing
/// host boundaries without adding GPU waits.
pub struct Span(Option<Record>);
impl Span {
    pub fn new(revision: u64, phase: &'static str) -> Self {
        Self::for_draw(revision, 0, phase)
    }
    fn for_draw(revision: u64, draw: u64, phase: &'static str) -> Self {
        Self(if enabled() {
            let origin = clock();
            let start = Instant::now();
            Some(Record {
                revision,
                draw,
                phase,
                origin,
                start,
                finish: start,
            })
        } else {
            None
        })
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some(record) = &mut self.0 {
            record.finish = Instant::now();
            emit("span", record.data());
        }
    }
}
struct DrawState {
    revision: u64,
    id: u64,
    prepared: OnceLock<Instant>,
}
/// Encoding a callback does not mean its image has reached the screen.
/// The gap between callbacks also includes acquiring the surface and other
/// egui work, so it cannot be treated as GPU wait time alone.
pub struct DrawProbe(Option<DrawState>);
impl DrawProbe {
    pub fn new(revision: u64) -> Self {
        Self(enabled().then(|| DrawState {
            revision,
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
            if let Some(prepared) = state.prepared.get() {
                let now = Instant::now();
                emit(
                    "draw",
                    json!({"revision":state.revision,"draw":state.id,"at_ms":milliseconds(clock(),now),"prepared_to_encode_ms":milliseconds(*prepared,now)}),
                );
            }
        }
        self.span("draw_encode")
    }
    fn span(&self, phase: &'static str) -> Span {
        match &self.0 {
            Some(s) => Span::for_draw(s.revision, s.id, phase),
            None => Span(None),
        }
    }
}
