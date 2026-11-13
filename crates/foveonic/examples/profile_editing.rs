//! Measures warm multi-photo edits with separate sessions, bounded host concurrency and node diagnostics.

#[path = "../src/options.rs"]
#[allow(dead_code)]
mod options;

use foveonic::{
    diagnostics::{self, CaptureConfig, Category, Context, Recorder},
    image::{ImageTexture, RenderedImage},
    inspection::Content,
    profile::{report, Mode, Profile},
    render::{
        ControlCapabilities, Engine, ProcessingWorkingSet, ReducedPreparation, ReducedRender,
        RenderSession, Scale, SessionKey, SessionKind, SessionUse, Settings,
    },
    source::{Result, Source},
};
use rayon::prelude::*;
use std::{
    collections::HashSet,
    ffi::OsString,
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

struct Run {
    destination: PathBuf,
    sources: Vec<PathBuf>,
    scale: Scale,
    thumb: bool,
    workers: usize,
    repeats: usize,
    mode: Mode,
    edit: String,
    cache_budget: u64,
    controls: Vec<OsString>,
    verify: bool,
    reduced: bool,
    sidecars: bool,
    gui_analysis: bool,
    sweep: bool,
    capture_dir: Option<PathBuf>,
    capture_mode: Mode,
}
impl Run {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let mut args = args.into_iter();
        let mut run = Self {
            destination: args.next().ok_or("usage: profile_editing REPORT.jsonl [--quality full|fast|draft|thumb | --scale-divisor N] [--workers N] [--repeats N] [--mode off|wall|gpu] [--capture-dir DIR] [--capture-mode wall|gpu] [--edit exposure|noise] [--cache-budget-mib N] [--verify] [--reduced] [--sidecars] [--gui-analysis] [--sweep] ORIGINAL... [-- RENDER_OPTIONS]")?.into(),
            sources: Vec::new(),
            scale: Scale::new(8)?,
            thumb: true,
            workers: 1,
            repeats: 5,
            mode: Mode::Off,
            edit: "exposure".into(),
            cache_budget: 2048 * 1024 * 1024,
            controls: Vec::new(),
            verify: false,
            reduced: false,
            sidecars: false,
            gui_analysis: false,
            sweep: false,
            capture_dir: None,
            capture_mode: Mode::Wall,
        };
        let mut capture_mode_specified = false;
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--") => {
                    run.controls.extend(args);
                    break;
                }
                Some("--verify") => run.verify = true,
                Some("--reduced") => run.reduced = true,
                Some("--sidecars") => run.sidecars = true,
                Some("--gui-analysis") => run.gui_analysis = true,
                Some("--sweep") => run.sweep = true,
                Some("--capture-dir") => {
                    run.capture_dir = Some(args.next().ok_or("missing capture directory")?.into());
                }
                Some("--quality") => {
                    let quality = args.next().and_then(|s| s.into_string().ok());
                    run.thumb = quality.as_deref() == Some("thumb");
                    run.scale = Scale::new(match quality.as_deref() {
                        Some("full") => 1,
                        Some("fast") => 2,
                        Some("draft") => 4,
                        Some("thumb") => 8,
                        _ => return Err("quality must be full, fast, draft or thumb".into()),
                    })?;
                }
                Some(
                    "--workers" | "--repeats" | "--cache-budget-mib" | "--mode" | "--edit"
                    | "--scale-divisor" | "--capture-mode",
                ) => {
                    let value = args
                        .next()
                        .and_then(|s| s.into_string().ok())
                        .ok_or("missing option value")?;
                    match arg.to_str().unwrap() {
                        "--workers" => run.workers = value.parse()?,
                        "--repeats" => run.repeats = value.parse()?,
                        "--mode" => run.mode = value.parse()?,
                        "--capture-mode" => {
                            run.capture_mode = match value.as_str() {
                                "wall" => Mode::Wall,
                                "gpu" => Mode::Gpu,
                                _ => return Err("capture mode must be wall or gpu".into()),
                            };
                            capture_mode_specified = true;
                        }
                        "--edit" => run.edit = value,
                        "--scale-divisor" => {
                            run.thumb = false;
                            run.scale = Scale::new(value.parse()?)?;
                        }
                        _ => {
                            run.cache_budget = value
                                .parse::<u64>()?
                                .checked_mul(1024 * 1024)
                                .ok_or("cache budget overflow")?
                        }
                    }
                }
                Some(s) if s.starts_with('-') => return Err(format!("unknown option {s}").into()),
                _ => run.sources.push(PathBuf::from(arg).canonicalize()?),
            }
        }
        if run.sources.is_empty()
            || run.workers == 0
            || run.workers > run.sources.len()
            || run.repeats == 0
            || run.cache_budget == 0
        {
            return Err("require sources, positive repeats/cache budget, and workers within 1..=photo count".into());
        }
        if run.sources.iter().collect::<HashSet<_>>().len() != run.sources.len() {
            return Err("use distinct originals, not repeated paths to one photograph".into());
        }
        if !["exposure", "noise"].contains(&run.edit.as_str()) {
            return Err("edit must be exposure or noise".into());
        }
        if run.reduced && run.workers != 1 {
            return Err("shared reduced working-set measurements require one worker".into());
        }
        if run.mode.gpu() && run.workers != 1 {
            return Err("run GPU diagnostics serially so timestamp resolves do not perturb concurrent schedules".into());
        }
        if capture_mode_specified && run.capture_dir.is_none() {
            return Err("--capture-mode requires --capture-dir".into());
        }
        Ok(run)
    }
}

struct Photo {
    source: PathBuf,
    engine: Arc<Engine>,
    session: Session,
    base: Settings,
    texture: Option<ImageTexture>,
    image: Option<RenderedImage>,
}
enum Session {
    Native(Box<RenderSession>),
    Reduced {
        id: u64,
        working: Arc<Mutex<ProcessingWorkingSet>>,
        budget: u64,
    },
}
impl Session {
    fn resources(&self) -> Option<foveonic::render::SessionResources> {
        match self {
            Self::Native(session) => Some(session.resources()),
            Self::Reduced { id, working, .. } => working.lock().unwrap().reduced_resources(*id),
        }
    }
    fn cached_bytes(&self) -> u64 {
        match self {
            Self::Native(s) => s.cached_bytes(),
            Self::Reduced { id, working, .. } => working
                .lock()
                .unwrap()
                .reduced_bytes(*id)
                .map_or(0, |s| s.0),
        }
    }
    fn cached_preparation_bytes(&self) -> usize {
        match self {
            Self::Native(s) => s.cached_preparation_bytes(),
            Self::Reduced { id, working, .. } => working
                .lock()
                .unwrap()
                .reduced_bytes(*id)
                .map_or(0, |s| s.1 as usize),
        }
    }
}
fn emit(file: &mut impl Write, event: &str, mode: Mode, mut data: serde_json::Value) -> Result<()> {
    // Capture is an attribution run even when the aggregate profiler stays Off.
    data["capture_mode"] = serde_json::to_value(diagnostics::active_mode())?;
    data["timing_baseline"] = (mode == Mode::Off
        && !diagnostics::enabled()
        && data.get("capture_requested_mode").is_none())
    .into();
    serde_json::to_writer(&mut *file, &report(event, mode, data))?;
    writeln!(file)?;
    file.flush()?;
    Ok(())
}

fn task_context(
    path: &Path,
    revision: usize,
    index: usize,
    count: usize,
    phase: &'static str,
) -> Option<Context> {
    diagnostics::enabled().then(|| Context {
        task: (revision * count + index + 1) as u64,
        revision: revision as u64,
        photo: diagnostics::photo_id(path),
        purpose: phase,
        node: None,
    })
}
fn task_flow(path: &Path, name: &str, terminal: bool, phase: &'static str, scale: Scale) {
    if let Some(context) = diagnostics::current_context() {
        diagnostics::flow(Category::Scenario, name, context.task, terminal, |fields| {
            fields
                .str("source", &path.to_string_lossy())
                .str("phase", phase)
                .str("coverage", "direct-command")
                .u64("scale_divisor", scale.divisor() as u64);
        });
    }
}
fn group_event(run: &Run, round: usize, phase: &str) {
    diagnostics::event(Category::Scenario, "editing.group", |fields| {
        fields
            .str("phase", phase)
            .str("coverage", "direct-command")
            .u64("group", round as u64)
            .u64("photo_count", run.sources.len() as u64)
            .u64("workers", run.workers as u64)
            .u64("scale_divisor", run.scale.divisor() as u64)
            .str("schedule", "whole-render")
            .bool("reduced", run.reduced);
    });
}
fn render(
    photo: &mut Photo,
    settings: &Settings,
    mode: Mode,
    group: Instant,
) -> Result<serde_json::Value> {
    let _scope = diagnostics::scope_with(Category::Cpu, "editing.photo", |fields| {
        fields
            .str("source", &photo.source.to_string_lossy())
            .u64("scale_divisor", settings.working_scale.divisor() as u64);
    });
    let began = Instant::now();
    let started_ms = group.elapsed().as_secs_f64() * 1000.;
    let before = photo.engine.gpu_activity();
    let cache_before = photo.session.cached_bytes();
    let profile_started_ms = group.elapsed().as_secs_f64() * 1000.;
    let mut profile = Profile::new(mode);
    let mut reduced_reloaded = false;
    let mut artifacts = match &mut photo.session {
        Session::Native(session) => photo
            .engine
            .render_session(session, settings, &["final"], || false, &mut profile)?
            .ok_or("render superseded")?,
        Session::Reduced {
            id,
            working,
            budget,
        } => {
            let mut working = working.lock().map_err(|_| "reduced working set poisoned")?;
            let outcome = working.render_reduced(
                &photo.engine,
                *id,
                settings,
                SessionUse::Foreground,
                || false,
                &mut profile,
            )?;
            let outcome = match outcome {
                None | Some(ReducedRender::NeedsOriginal(_)) => {
                    reduced_reloaded = true;
                    let original = Source::open(&photo.source)?;
                    if working.contains(SessionKey {
                        photo: *id,
                        kind: SessionKind::Reduced,
                    }) {
                        working
                            .refresh_reduced(
                                &photo.engine,
                                *id,
                                &original,
                                settings,
                                SessionUse::Foreground,
                                || false,
                                &mut profile,
                            )?
                            .ok_or("refresh superseded")?;
                    } else {
                        let ReducedPreparation::Prepared(session) = photo
                            .engine
                            .prepare_reduced_session(original, settings, || false, &mut profile)?
                        else {
                            return Err("preparation superseded".into());
                        };
                        working.insert_reduced(
                            *id,
                            session,
                            &photo.engine,
                            SessionUse::Foreground,
                        )?;
                    }
                    working
                        .render_reduced(
                            &photo.engine,
                            *id,
                            settings,
                            SessionUse::Foreground,
                            || false,
                            &mut profile,
                        )?
                        .ok_or("session does not fit working-set budget")?
                }
                Some(outcome) => outcome,
            };
            let mut reservation = foveonic::render::MemoryReservation::default();
            if let Some(image) = &photo.image {
                reservation.add_image(image);
            }
            if let Some(texture) = &photo.texture {
                reservation.exclusive_bytes += texture.description().byte_len();
            }
            if let ReducedRender::Rendered(artifacts) = &outcome {
                for artifact in artifacts {
                    if let Content::Image(image) = &artifact.content {
                        reservation.add_image(image);
                    }
                }
            }
            photo
                .engine
                .trim_memory(&mut working, *budget, &reservation);
            match outcome {
                ReducedRender::Rendered(artifacts) => artifacts,
                ReducedRender::NeedsOriginal(reason) => {
                    return Err(format!("reduced edit requires original: {reason:?}").into())
                }
                ReducedRender::Superseded => return Err("render superseded".into()),
            }
        }
    };
    let artifact = artifacts.pop().ok_or("missing final image")?;
    let effective = artifact.settings.working_scale;
    let Content::Image(image) = artifact.content else {
        return Err("image expected".into());
    };
    let texture = photo.engine.view_texture(&image, None, &mut profile)?;
    // This is the harness's completed-image boundary, also present in Off mode.
    // Concurrent device-wide waits can include another executor's queued work.
    profile.measure("complete display texture", || {
        photo.engine.device().poll(wgpu::Maintain::Wait)
    });
    let completed_ms = began.elapsed().as_secs_f64() * 1000.;
    let finished_ms = group.elapsed().as_secs_f64() * 1000.;
    let extent = texture.description().extent;
    photo.texture = Some(texture);
    photo.image = Some(*image);
    diagnostics::event(Category::Presentation, "editing.image.ready", |fields| {
        fields
            .str("source", &photo.source.to_string_lossy())
            .u64("scale_divisor", effective.divisor() as u64)
            .u64("width", extent[0] as u64)
            .u64("height", extent[1] as u64)
            .str("output", "final")
            .str("coverage", "headless-completed-texture");
    });
    Ok(serde_json::json!({
        "source": photo.source, "execution":"session", "reduced_reloaded":reduced_reloaded, "started_ms":started_ms,
        "finished_ms":finished_ms, "completed_ms":completed_ms, "profile_started_ms":profile_started_ms,
        "requested_scale_divisor":settings.working_scale.divisor(), "effective_scale_divisor":effective.divisor(), "extent":extent,
        "settings":settings, "cache_before_bytes":cache_before,
        "cache_after_bytes":photo.session.cached_bytes(),
        "preparation_bytes":photo.session.cached_preparation_bytes(),
        "engine_preparation_bytes":photo.engine.cached_preparation_bytes(),
        "activity":photo.engine.gpu_activity().since(before), "profile":profile,
    }))
}
fn edit(base: &Settings, kind: &str, index: usize, sweep: bool) -> Settings {
    let mut settings = base.clone();
    if kind == "exposure" {
        settings.exposure_auto = false;
        settings.exposure += if sweep {
            // Every round changes the response rather than revisiting two cached tables.
            0.01 * (index + 1) as f32
        } else if index.is_multiple_of(2) {
            0.25
        } else {
            -0.25
        };
    } else {
        settings
            .reconstruction
            .get_or_insert_with(Default::default)
            .chroma = if index.is_multiple_of(2) { 1 } else { 2 };
    }
    settings
}
fn main() -> Result<()> {
    let run = Run::parse(std::env::args_os().skip(1))?;
    // Never truncate existing evidence. Reports are flushed after each completed group.
    let mut file = BufWriter::new(File::create_new(&run.destination)?);
    let recorder = if run.capture_dir.is_some() {
        let recorder = Recorder::new();
        recorder
            .start(CaptureConfig {
                mode: run.capture_mode,
                ..Default::default()
            })?
            .recv()??;
        Some(recorder)
    } else {
        None
    };
    let mut capture_photos = Vec::new();
    let mut capture_adapter = None;
    let result = (|| -> Result<()> {
        let setup = Instant::now();
        let engine = Arc::new(Engine::new()?);
        if recorder.is_some() {
            capture_adapter = Some(engine.adapter_name().to_owned());
        }
        let reduced_working = Arc::new(Mutex::new(ProcessingWorkingSet::new()));
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(run.workers)
            .thread_name(|index| format!("editing-worker-{index}"))
            .build()?;
        let mut photos = Vec::new();
        emit(
            &mut file,
            "editing_setup",
            run.mode,
            serde_json::json!({
                "sources":run.sources,"nominal_scale_divisor":run.scale.divisor(),"thumb_policy":run.thumb,"workers":run.workers,"adapter":engine.adapter_name(),
                "edit":run.edit,"repeats":run.repeats,"cache_budget_bytes":run.cache_budget,
                "schedule":"whole-render", "queue":"shared-ordered", "version":env!("CARGO_PKG_VERSION"),
                "render_options":run.controls,"verify":run.verify,"reduced":run.reduced,
                "sidecars":run.sidecars,"gui_analysis":run.gui_analysis,"sweep":run.sweep,
            }),
        )?;
        for (index, path) in run.sources.iter().enumerate() {
            let _context =
                task_context(path, 0, index, run.sources.len(), "warmup").map(diagnostics::enter);
            task_flow(path, "editing.task.enqueued", false, "warmup", run.scale);
            task_flow(path, "editing.task.started", false, "warmup", run.scale);
            let began = Instant::now();
            let source = Source::open(path)?;
            let load_ms = began.elapsed().as_secs_f64() * 1000.;
            let capabilities = ControlCapabilities::from_metadata(&source.metadata)?;
            let mut initial = if run.sidecars {
                foveonic::sidecar::load(path)?.ok_or_else(|| {
                    format!("--sidecars requires a saved recipe for {}", path.display())
                })?
            } else {
                foveonic::render::camera_settings(&source.metadata, None)?
            };
            if initial.white_balance.is_empty() {
                initial.white_balance = capabilities.default_white_balance(&source.metadata)?;
            }
            if initial.color_mode.is_empty() {
                initial.color_mode = capabilities.default_color_mode(&source.metadata)?;
            }
            let (mut base, paths, control_mode) = options::parse_settings_with_noise_defaults(
                run.controls.iter().cloned(),
                capabilities.default_noise_controls_for_metadata(&source.metadata)?,
                initial,
            )?;
            base.dehaze_estimate = run.gui_analysis && base.dehaze_auto();
            if !paths.is_empty() || control_mode != Mode::Off {
                return Err("render options must contain only photographic controls. Select profiling with the runner's --mode".into());
            }
            if run.edit == "noise" {
                base.reconstruction
                    .get_or_insert_with(Default::default)
                    .chroma = 0;
            }
            base.working_scale = if run.thumb {
                capabilities.thumb_scale()
            } else {
                run.scale
            };
            if recorder.is_some() {
                let identity = match source.origin() {
                    foveonic::source::SourceOrigin::File(identity) => Some(identity),
                    _ => None,
                };
                capture_photos.push(
                    serde_json::json!({"source":path,"photo":diagnostics::photo_id(path),
                "identity":identity,"settings":base}),
                );
            }
            let photo_engine = if run.reduced {
                engine.clone()
            } else {
                Arc::new(engine.executor())
            };
            let session = if run.reduced {
                match photo_engine.prepare_reduced_session(
                    source,
                    &base,
                    || false,
                    &mut Profile::default(),
                )? {
                    ReducedPreparation::Prepared(session) => {
                        let id = photos.len() as u64;
                        let mut working = reduced_working.lock().unwrap();
                        working.insert_reduced(
                            id,
                            session,
                            &photo_engine,
                            SessionUse::Foreground,
                        )?;
                        photo_engine.trim_memory(
                            &mut working,
                            run.cache_budget,
                            &Default::default(),
                        );
                        drop(working);
                        Session::Reduced {
                            id,
                            working: reduced_working.clone(),
                            budget: run.cache_budget,
                        }
                    }
                    ReducedPreparation::Superseded => return Err("preparation superseded".into()),
                }
            } else {
                Session::Native(Box::new(RenderSession::new(Arc::new(source))))
            };
            photos.push(Photo {
                source: path.clone(),
                engine: photo_engine,
                session,
                base,
                texture: None,
                image: None,
            });
            let photo = photos.last_mut().unwrap();
            let settings = photo.base.clone();
            let mut warmup = render(photo, &settings, run.mode, Instant::now())?;
            task_flow(path, "editing.task.completed", true, "warmup", run.scale);
            warmup["load_ms"] = load_ms.into();
            warmup["cold"] = true.into();
            warmup["workers"] = 1.into();
            warmup["edit"] = run.edit.clone().into();
            emit(&mut file, "editing_warmup", run.mode, warmup)?;
            let bytes: u64 = photos.iter().map(|p| p.session.cached_bytes()).sum();
            if !run.reduced && bytes > run.cache_budget {
                return Err(format!("retained session buffers {bytes} exceed cache budget {}. Budget excludes originals, textures, scratch and driver memory",run.cache_budget).into());
            }
        }
        emit(
            &mut file,
            "editing_ready",
            run.mode,
            serde_json::json!({"setup_ms":setup.elapsed().as_secs_f64()*1000., "cache_bytes":photos.iter().map(|p|p.session.cached_bytes()).sum::<u64>(),"session_cpu_bytes":photos.iter().map(|p|p.session.cached_preparation_bytes()).sum::<usize>(),"engine_preparation_bytes":if run.reduced {engine.cached_preparation_bytes()} else {photos.iter().map(|p|p.engine.cached_preparation_bytes()).sum::<usize>()},"working_set_bytes":if run.reduced {reduced_working.lock().unwrap().retained_bytes(&engine)} else {0},"session_resources":photos.iter().map(|p|serde_json::json!({"source":p.source,"resources":p.session.resources()})).collect::<Vec<_>>()}),
        )?;
        for round in 0..run.repeats {
            group_event(&run, round, "begin");
            let photo_count = photos.len();
            if diagnostics::enabled() {
                for (index, photo) in photos.iter().enumerate() {
                    diagnostics::with_context(
                        task_context(&photo.source, round + 1, index, photo_count, "edit"),
                        || {
                            task_flow(
                                &photo.source,
                                "editing.task.enqueued",
                                false,
                                "edit",
                                photo.base.working_scale,
                            );
                        },
                    );
                }
            }
            let began = Instant::now();
            let results: Result<Vec<_>> = pool.install(|| {
                photos
                    .par_iter_mut()
                    .enumerate()
                    .map(|(index, photo)| {
                        diagnostics::with_context(
                            task_context(&photo.source, round + 1, index, photo_count, "edit"),
                            || {
                                task_flow(
                                    &photo.source,
                                    "editing.task.started",
                                    false,
                                    "edit",
                                    photo.base.working_scale,
                                );
                                let settings = edit(&photo.base, &run.edit, round, run.sweep);
                                let rendered = render(photo, &settings, run.mode, began);
                                task_flow(
                                    &photo.source,
                                    if rendered.is_ok() {
                                        "editing.task.completed"
                                    } else {
                                        "editing.task.failed"
                                    },
                                    true,
                                    "edit",
                                    photo.base.working_scale,
                                );
                                let mut result = rendered?;
                                result["photo_index"] = index.into();
                                result["round"] = round.into();
                                result["cold"] = false.into();
                                result["workers"] = run.workers.into();
                                result["edit"] = run.edit.clone().into();
                                Ok(result)
                            },
                        )
                    })
                    .collect()
            });
            let group_ms = began.elapsed().as_secs_f64() * 1000.;
            group_event(
                &run,
                round,
                if results.is_ok() {
                    "complete"
                } else {
                    "failed"
                },
            );
            let results = results?;
            let first_ms = results
                .iter()
                .filter_map(|r| r["finished_ms"].as_f64())
                .fold(f64::INFINITY, f64::min);
            for result in results {
                emit(&mut file, "editing_photo", run.mode, result)?;
            }
            let cache_bytes: u64 = photos.iter().map(|p| p.session.cached_bytes()).sum();
            let (working_set_bytes, resident_sessions, active_bytes, historical_bytes) =
                if run.reduced {
                    let working = reduced_working.lock().unwrap();
                    (
                        working.retained_bytes(&engine),
                        working.len(),
                        working.active_bytes(),
                        working.historical_bytes(),
                    )
                } else {
                    (0, photos.len(), 0, 0)
                };
            emit(
                &mut file,
                "editing_group",
                run.mode,
                serde_json::json!({"round":round,"nominal_scale_divisor":run.scale.divisor(),"thumb_policy":run.thumb,"workers":run.workers,"edit":run.edit,"photo_count":photos.len(),"completed_ms":group_ms,"first_ms":first_ms,"cache_bytes":cache_bytes,"working_set_bytes":working_set_bytes,"resident_sessions":resident_sessions,"active_bytes":active_bytes,"historical_bytes":historical_bytes}),
            )?;
            if !run.reduced && cache_bytes > run.cache_budget {
                return Err("edited session buffers exceed cache budget".into());
            }
        }
        diagnostics::event(Category::Scenario, "editing.measured.complete", |fields| {
            fields
                .u64("groups", run.repeats as u64)
                .u64("photo_count", photos.len() as u64);
        });
        if run.verify {
            // Readback is outside all edit intervals and does not enter activity profiles.
            for (index, photo) in photos.iter().enumerate() {
                let _context = task_context(
                    &photo.source,
                    run.repeats + 1,
                    index,
                    photos.len(),
                    "verification",
                )
                .map(diagnostics::enter);
                task_flow(
                    &photo.source,
                    "editing.task.enqueued",
                    false,
                    "verification",
                    photo.base.working_scale,
                );
                task_flow(
                    &photo.source,
                    "editing.task.started",
                    false,
                    "verification",
                    photo.base.working_scale,
                );
                let image = photo.image.as_ref().ok_or("missing linear image")?;
                let bytes = photo.engine.readback_image(image)?;
                let verifier = engine.executor();
                let settings = edit(&photo.base, &run.edit, run.repeats - 1, run.sweep);
                let original = Source::open(&photo.source)?;
                let fresh = verifier.render(&original, &settings, &mut Profile::default())?;
                let expected = verifier.readback_image(&fresh)?;
                if image.description() != fresh.description()
                    || image.view() != fresh.view()
                    || bytes != expected
                {
                    return Err(format!(
                        "cached/scheduled output differs from fresh processing: {}",
                        photo.source.display()
                    )
                    .into());
                }
                emit(
                    &mut file,
                    "editing_verification",
                    run.mode,
                    serde_json::json!({"source":photo.source,"scale_divisor":settings.working_scale.divisor(),"thumb_policy":run.thumb,"matches_fresh":true,"crc32":format!("{:08x}",crc32fast::hash(&bytes))}),
                )?;
                task_flow(
                    &photo.source,
                    "editing.task.completed",
                    true,
                    "verification",
                    photo.base.working_scale,
                );
            }
        }
        Ok(())
    })();
    if let Some(recorder) = recorder {
        diagnostics::event(Category::Scenario, "editing.run.complete", |fields| {
            fields.bool("success", result.is_ok());
            if let Err(error) = &result {
                fields.str("error", &error.to_string());
            }
        });
        let manifest = serde_json::json!({"frontend":"profile_editing","coverage":"direct-command",
            "version":env!("CARGO_PKG_VERSION"),"adapter":capture_adapter,
            "hardware":{"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,
                "logical_cpu_workers":std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)},
            "aggregate_mode":run.mode,"capture_mode":run.capture_mode,
            "nominal_scale_divisor":run.scale.divisor(),"thumb_policy":run.thumb,"workers":run.workers,"repeats":run.repeats,
            "edit":run.edit,"reduced":run.reduced,"sidecars":run.sidecars,
            "gui_analysis":run.gui_analysis,"sweep":run.sweep,"verify":run.verify,
            "cache_budget_bytes":run.cache_budget,"cache_state":"fresh setup then retained edits; OS cache uncontrolled",
            "capture_phases":["warmup","edit","verification"],
            "schedule":"whole-render","queue":"shared-ordered","photos":capture_photos,
            "report":run.destination,"success":result.is_ok()});
        let saved = (|| -> Result<_> {
            recorder
                .stop_save(run.capture_dir.as_ref().unwrap().clone(), manifest)?
                .recv()?
        })();
        match saved {
            Ok(saved) => emit(
                &mut file,
                "editing_capture",
                run.mode,
                serde_json::json!({
                "directory":saved.directory,"trace":saved.trace,"trace_bytes":saved.trace_bytes,
                "capture_requested_mode":run.capture_mode,"success":result.is_ok()}),
            )?,
            Err(error) => {
                if result.is_ok() {
                    return Err(error);
                }
                emit(
                    &mut file,
                    "editing_capture",
                    run.mode,
                    serde_json::json!({"save_error":error.to_string(),"capture_requested_mode":run.capture_mode,"success":false}),
                )?;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_duplicate_photos_invalid_bounds_and_concurrent_gpu_diagnostics() {
        let source = std::env::current_exe().unwrap().into_os_string();
        let parse = |flags: &[&str], duplicate: bool| {
            let mut args: Vec<OsString> = ["unused.jsonl"]
                .into_iter()
                .chain(flags.iter().copied())
                .map(Into::into)
                .collect();
            args.push(source.clone());
            if duplicate {
                args.push(source.clone());
            }
            Run::parse(args)
        };
        assert!(parse(&[], false).is_ok());
        assert!(parse(&[], true).is_err());
        assert!(parse(&["--workers", "0"], false).is_err());
        assert!(parse(&["--workers", "2"], false).is_err());
        assert!(parse(&["--repeats", "0"], false).is_err());
        assert!(parse(&["--mode", "gpu", "--workers", "2"], true).is_err());
        assert!(parse(&["--quality", "tiny"], false).is_err());
        assert!(parse(&["--quality", "thumb"], false).unwrap().thumb);
        let numeric = parse(&["--quality", "thumb", "--scale-divisor", "8"], false).unwrap();
        assert!(!numeric.thumb);
        assert_eq!(numeric.scale.divisor(), 8);
        assert!(
            parse(&["--scale-divisor", "8", "--quality", "thumb"], false)
                .unwrap()
                .thumb
        );
        assert!(parse(&["--scale-divisor", "16"], false).is_ok());
        assert!(parse(&["--scale-divisor", "3"], false).is_err());
        assert!(parse(&["--cache-budget-mib", "18446744073709551615"], false).is_err());
    }
    #[test]
    fn capture_flags_do_not_select_synchronous_gpu_profiling_or_change_worker_admission() {
        let first = std::env::current_exe().unwrap();
        let second = tempfile::NamedTempFile::new().unwrap();
        let parse = |flags: &[&str]| {
            let mut args: Vec<OsString> = ["unused.jsonl"]
                .into_iter()
                .chain(flags.iter().copied())
                .map(Into::into)
                .collect();
            args.extend([
                first.clone().into_os_string(),
                second.path().as_os_str().to_owned(),
            ]);
            Run::parse(args)
        };
        let plain = parse(&[]).unwrap();
        assert!(plain.capture_dir.is_none());
        let wall = parse(&["--capture-dir", "capture"]).unwrap();
        assert_eq!(wall.capture_mode, Mode::Wall);
        assert_eq!(wall.mode, Mode::Off);
        let gpu = parse(&[
            "--capture-dir",
            "capture",
            "--capture-mode",
            "gpu",
            "--workers",
            "2",
        ])
        .unwrap();
        assert_eq!(gpu.capture_mode, Mode::Gpu);
        assert_eq!(gpu.mode, Mode::Off);
        assert_eq!(gpu.workers, 2);
        assert!(parse(&["--capture-mode", "gpu"]).is_err());
        assert!(parse(&["--capture-dir", "capture", "--capture-mode", "off"]).is_err());
        assert!(parse(&[
            "--capture-dir",
            "capture",
            "--capture-mode",
            "gpu",
            "--mode",
            "gpu",
            "--workers",
            "2"
        ])
        .is_err());
        assert!(parse(&["--capture-dir", "capture", "--reduced", "--workers", "2"]).is_err());
    }
    #[test]
    fn edits_alternate_and_force_manual_exposure() {
        let mut base = Settings::new("Sunlight", "Standard");
        base.exposure_auto = true;
        base.exposure = 1.;
        assert_eq!(edit(&base, "exposure", 0, false).exposure, 1.25);
        assert_eq!(edit(&base, "exposure", 1, false).exposure, 0.75);
        assert!(!edit(&base, "exposure", 0, false).exposure_auto);
        assert_eq!(
            edit(&base, "noise", 0, false)
                .reconstruction
                .unwrap()
                .chroma,
            1
        );
        assert_eq!(
            edit(&base, "noise", 1, false)
                .reconstruction
                .unwrap()
                .chroma,
            2
        );
        assert!(base.reconstruction.is_none());
        let values: HashSet<_> = (0..100)
            .map(|i| edit(&base, "exposure", i, true).exposure.to_bits())
            .collect();
        assert_eq!(values.len(), 100);
    }
}
