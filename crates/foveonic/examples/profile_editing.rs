//! Measures warm multi-photo edits with separate sessions, bounded host concurrency and node diagnostics.

#[path = "../src/options.rs"]
#[allow(dead_code)]
mod options;

use foveonic::{
    image::{ImageTexture, RenderedImage},
    inspection::Content,
    profile::{report, Mode, Profile},
    render::{
        ControlCapabilities, Engine, ReducedPreparation, ReducedRender, ReducedWorkingSet,
        RenderSession, Scale, Settings,
    },
    source::{Result, Source},
};
use rayon::prelude::*;
use std::{
    collections::HashSet,
    ffi::OsString,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

struct Run {
    destination: PathBuf,
    sources: Vec<PathBuf>,
    scale: Scale,
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
}
impl Run {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let mut args = args.into_iter();
        let mut run = Self {
            destination: args.next().ok_or("usage: profile_editing REPORT.jsonl [--quality full|fast|draft|thumb | --scale-divisor N] [--workers N] [--repeats N] [--mode off|wall|gpu] [--edit exposure|noise] [--cache-budget-mib N] [--verify] [--reduced] [--sidecars] [--gui-analysis] [--sweep] ORIGINAL... [-- RENDER_OPTIONS]")?.into(),
            sources: Vec::new(),
            scale: Scale::new(8)?,
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
        };
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
                Some("--quality") => {
                    run.scale = Scale::new(
                        match args.next().and_then(|s| s.into_string().ok()).as_deref() {
                            Some("full") => 1,
                            Some("fast") => 2,
                            Some("draft") => 4,
                            Some("thumb") => 8,
                            _ => return Err("quality must be full, fast, draft or thumb".into()),
                        },
                    )?;
                }
                Some(
                    "--workers" | "--repeats" | "--cache-budget-mib" | "--mode" | "--edit"
                    | "--scale-divisor",
                ) => {
                    let value = args
                        .next()
                        .and_then(|s| s.into_string().ok())
                        .ok_or("missing option value")?;
                    match arg.to_str().unwrap() {
                        "--workers" => run.workers = value.parse()?,
                        "--repeats" => run.repeats = value.parse()?,
                        "--mode" => run.mode = value.parse()?,
                        "--edit" => run.edit = value,
                        "--scale-divisor" => run.scale = Scale::new(value.parse()?)?,
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
        working: Arc<Mutex<ReducedWorkingSet>>,
    },
}
impl Session {
    fn resources(&self) -> Option<foveonic::render::SessionResources> {
        match self {
            Self::Native(session) => Some(session.resources()),
            Self::Reduced { id, working } => working.lock().unwrap().session_resources(*id),
        }
    }
    fn cached_bytes(&self) -> u64 {
        match self {
            Self::Native(s) => s.cached_bytes(),
            Self::Reduced { id, working } => working
                .lock()
                .unwrap()
                .session_bytes(*id)
                .map_or(0, |s| s.0),
        }
    }
    fn cached_preparation_bytes(&self) -> usize {
        match self {
            Self::Native(s) => s.cached_preparation_bytes(),
            Self::Reduced { id, working } => working
                .lock()
                .unwrap()
                .session_bytes(*id)
                .map_or(0, |s| s.1 as usize),
        }
    }
}
fn emit(file: &mut impl Write, event: &str, mode: Mode, data: serde_json::Value) -> Result<()> {
    serde_json::to_writer(&mut *file, &report(event, mode, data))?;
    writeln!(file)?;
    file.flush()?;
    Ok(())
}
fn render(
    photo: &mut Photo,
    settings: &Settings,
    mode: Mode,
    group: Instant,
) -> Result<serde_json::Value> {
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
        Session::Reduced { id, working } => {
            let mut working = working.lock().map_err(|_| "reduced working set poisoned")?;
            let outcome = working.render(&photo.engine, *id, settings, || false, &mut profile)?;
            let outcome = match outcome {
                None | Some(ReducedRender::NeedsOriginal(_)) => {
                    reduced_reloaded = true;
                    let original = Source::open(&photo.source)?;
                    if working.contains(*id) {
                        working
                            .refresh(
                                &photo.engine,
                                *id,
                                &original,
                                settings,
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
                        working.insert(*id, session, &photo.engine)?;
                    }
                    working
                        .render(&photo.engine, *id, settings, || false, &mut profile)?
                        .ok_or("session does not fit working-set budget")?
                }
                Some(outcome) => outcome,
            };
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
    let setup = Instant::now();
    let engine = Arc::new(Engine::new()?);
    let reduced_working = Arc::new(Mutex::new(ReducedWorkingSet::new(run.cache_budget)));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(run.workers)
        .build()?;
    let mut photos = Vec::new();
    emit(
        &mut file,
        "editing_setup",
        run.mode,
        serde_json::json!({
            "sources":run.sources,"scale_divisor":run.scale.divisor(),"workers":run.workers,"adapter":engine.adapter_name(),
            "edit":run.edit,"repeats":run.repeats,"cache_budget_bytes":run.cache_budget,
            "schedule":"whole-render", "queue":"shared-ordered", "version":env!("CARGO_PKG_VERSION"),
            "render_options":run.controls,"verify":run.verify,"reduced":run.reduced,
            "sidecars":run.sidecars,"gui_analysis":run.gui_analysis,"sweep":run.sweep,
        }),
    )?;
    for path in &run.sources {
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
        base.working_scale = run.scale;
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
                    reduced_working
                        .lock()
                        .unwrap()
                        .insert(id, session, &photo_engine)?;
                    Session::Reduced {
                        id,
                        working: reduced_working.clone(),
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
        let began = Instant::now();
        let results: Result<Vec<_>> = pool.install(|| {
            photos
                .par_iter_mut()
                .enumerate()
                .map(|(index, photo)| {
                    let settings = edit(&photo.base, &run.edit, round, run.sweep);
                    let mut result = render(photo, &settings, run.mode, began)?;
                    result["photo_index"] = index.into();
                    result["round"] = round.into();
                    result["cold"] = false.into();
                    result["workers"] = run.workers.into();
                    result["edit"] = run.edit.clone().into();
                    Ok(result)
                })
                .collect()
        });
        let group_ms = began.elapsed().as_secs_f64() * 1000.;
        let results = results?;
        let first_ms = results
            .iter()
            .filter_map(|r| r["finished_ms"].as_f64())
            .fold(f64::INFINITY, f64::min);
        for result in results {
            emit(&mut file, "editing_photo", run.mode, result)?;
        }
        let cache_bytes: u64 = photos.iter().map(|p| p.session.cached_bytes()).sum();
        let (working_set_bytes, resident_sessions, active_bytes, historical_bytes) = if run.reduced
        {
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
            serde_json::json!({"round":round,"scale_divisor":run.scale.divisor(),"workers":run.workers,"edit":run.edit,"photo_count":photos.len(),"completed_ms":group_ms,"first_ms":first_ms,"cache_bytes":cache_bytes,"working_set_bytes":working_set_bytes,"resident_sessions":resident_sessions,"active_bytes":active_bytes,"historical_bytes":historical_bytes}),
        )?;
        if !run.reduced && cache_bytes > run.cache_budget {
            return Err("edited session buffers exceed cache budget".into());
        }
    }
    if run.verify {
        // Readback is outside all edit intervals and does not enter activity profiles.
        for photo in &photos {
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
                serde_json::json!({"source":photo.source,"scale_divisor":run.scale.divisor(),"matches_fresh":true,"crc32":format!("{:08x}",crc32fast::hash(&bytes))}),
            )?;
        }
    }
    Ok(())
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
        assert!(parse(&["--scale-divisor", "16"], false).is_ok());
        assert!(parse(&["--scale-divisor", "3"], false).is_err());
        assert!(parse(&["--cache-budget-mib", "18446744073709551615"], false).is_err());
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
