//! Measures warm multi-photo edits with separate sessions, bounded host concurrency and node diagnostics.

#[path = "../src/options.rs"]
#[allow(dead_code)]
mod options;

use foveonic::{
    image::{ImageTexture, RenderedImage},
    inspection::Content,
    profile::{report, Mode, Profile},
    render::{ControlCapabilities, Engine, PreviewQuality, RenderSession, Settings},
    source::{Result, Source},
};
use rayon::prelude::*;
use std::{
    collections::HashSet,
    ffi::OsString,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

struct Run {
    destination: PathBuf,
    sources: Vec<PathBuf>,
    quality: PreviewQuality,
    workers: usize,
    repeats: usize,
    mode: Mode,
    edit: String,
    cache_budget: u64,
    controls: Vec<OsString>,
    verify: bool,
}
impl Run {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let mut args = args.into_iter();
        let mut run = Self {
            destination: args.next().ok_or("usage: profile_editing REPORT.jsonl [--quality full|fast|draft|thumb] [--workers N] [--repeats N] [--mode off|wall|gpu] [--edit exposure|noise] [--cache-budget-mib N] [--verify] ORIGINAL... [-- RENDER_OPTIONS]")?.into(),
            sources: Vec::new(),
            quality: PreviewQuality::Thumb,
            workers: 1,
            repeats: 5,
            mode: Mode::Off,
            edit: "exposure".into(),
            cache_budget: 2048 * 1024 * 1024,
            controls: Vec::new(),
            verify: false,
        };
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--") => {
                    run.controls.extend(args);
                    break;
                }
                Some("--verify") => run.verify = true,
                Some("--quality") => {
                    run.quality = match args.next().and_then(|s| s.into_string().ok()).as_deref() {
                        Some("full") => PreviewQuality::Full,
                        Some("fast") => PreviewQuality::Fast,
                        Some("draft") => PreviewQuality::Draft,
                        Some("thumb") => PreviewQuality::Thumb,
                        _ => return Err("quality must be full, fast, draft or thumb".into()),
                    };
                }
                Some("--workers" | "--repeats" | "--cache-budget-mib" | "--mode" | "--edit") => {
                    let value = args
                        .next()
                        .and_then(|s| s.into_string().ok())
                        .ok_or("missing option value")?;
                    match arg.to_str().unwrap() {
                        "--workers" => run.workers = value.parse()?,
                        "--repeats" => run.repeats = value.parse()?,
                        "--mode" => run.mode = value.parse()?,
                        "--edit" => run.edit = value,
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
        if run.mode.gpu() && run.workers != 1 {
            return Err("run GPU diagnostics serially so timestamp resolves do not perturb concurrent schedules".into());
        }
        Ok(run)
    }
}

struct Photo {
    source: PathBuf,
    original: Arc<Source>,
    engine: Engine,
    session: RenderSession,
    base: Settings,
    texture: Option<ImageTexture>,
    image: Option<RenderedImage>,
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
    let mut artifacts = photo
        .engine
        .render_session(
            &mut photo.session,
            settings,
            &["final"],
            || false,
            &mut profile,
        )?
        .ok_or("render superseded")?;
    let artifact = artifacts.pop().ok_or("missing final image")?;
    let effective = artifact.settings.preview_quality;
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
        "source": photo.source, "execution":"session", "started_ms":started_ms,
        "finished_ms":finished_ms, "completed_ms":completed_ms, "profile_started_ms":profile_started_ms,
        "quality":settings.preview_quality, "effective_quality":effective, "extent":extent,
        "settings":settings, "cache_before_bytes":cache_before,
        "cache_after_bytes":photo.session.cached_bytes(),
        "preparation_bytes":photo.session.cached_preparation_bytes(),
        "activity":photo.engine.gpu_activity().since(before), "profile":profile,
    }))
}
fn edit(base: &Settings, kind: &str, index: usize) -> Settings {
    let mut settings = base.clone();
    if kind == "exposure" {
        settings.exposure_auto = false;
        settings.exposure += if index.is_multiple_of(2) { 0.25 } else { -0.25 };
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
    let engine = Engine::new()?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(run.workers)
        .build()?;
    let mut photos = Vec::new();
    emit(
        &mut file,
        "editing_setup",
        run.mode,
        serde_json::json!({
            "sources":run.sources,"quality":run.quality,"workers":run.workers,"adapter":engine.adapter_name(),
            "edit":run.edit,"repeats":run.repeats,"cache_budget_bytes":run.cache_budget,
            "schedule":"whole-render", "queue":"shared-ordered", "version":env!("CARGO_PKG_VERSION"),
            "render_options":run.controls,"verify":run.verify,
        }),
    )?;
    for path in &run.sources {
        let began = Instant::now();
        let source = Arc::new(Source::open(path)?);
        let load_ms = began.elapsed().as_secs_f64() * 1000.;
        let capabilities = ControlCapabilities::from_metadata(&source.metadata)?;
        let controls = [
            OsString::from("--wb"),
            capabilities.default_white_balance(&source.metadata)?.into(),
            OsString::from("--mode"),
            capabilities.default_color_mode(&source.metadata)?.into(),
        ]
        .into_iter()
        .chain(run.controls.iter().cloned());
        let (mut base, paths, control_mode) =
            options::parse_settings_for_camera(controls, &source.metadata)?;
        if !paths.is_empty() || control_mode != Mode::Off {
            return Err("render options must contain only photographic controls. Select profiling with the runner's --mode".into());
        }
        if run.edit == "noise" {
            base.reconstruction
                .get_or_insert_with(Default::default)
                .chroma = 0;
        }
        base.preview_quality = run.quality;
        photos.push(Photo {
            source: path.clone(),
            engine: engine.executor(),
            session: RenderSession::new(source.clone()),
            original: source,
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
        if bytes > run.cache_budget {
            return Err(format!("retained session buffers {bytes} exceed cache budget {}. Budget excludes originals, textures, scratch and driver memory",run.cache_budget).into());
        }
    }
    emit(
        &mut file,
        "editing_ready",
        run.mode,
        serde_json::json!({"setup_ms":setup.elapsed().as_secs_f64()*1000., "cache_bytes":photos.iter().map(|p|p.session.cached_bytes()).sum::<u64>()}),
    )?;
    for round in 0..run.repeats {
        let began = Instant::now();
        let results: Result<Vec<_>> = pool.install(|| {
            photos
                .par_iter_mut()
                .enumerate()
                .map(|(index, photo)| {
                    let settings = edit(&photo.base, &run.edit, round);
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
        emit(
            &mut file,
            "editing_group",
            run.mode,
            serde_json::json!({"round":round,"quality":run.quality,"workers":run.workers,"edit":run.edit,"photo_count":photos.len(),"completed_ms":group_ms,"first_ms":first_ms,"cache_bytes":cache_bytes}),
        )?;
        if cache_bytes > run.cache_budget {
            return Err("edited session buffers exceed cache budget".into());
        }
    }
    if run.verify {
        // Readback is outside all edit intervals and does not enter activity profiles.
        for photo in &photos {
            let image = photo.image.as_ref().ok_or("missing linear image")?;
            let bytes = photo.engine.readback_image(image)?;
            let verifier = engine.executor();
            let settings = edit(&photo.base, &run.edit, run.repeats - 1);
            let fresh = verifier.render(&photo.original, &settings, &mut Profile::default())?;
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
                serde_json::json!({"source":photo.source,"quality":run.quality,"matches_fresh":true,"crc32":format!("{:08x}",crc32fast::hash(&bytes))}),
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
        assert!(parse(&["--cache-budget-mib", "18446744073709551615"], false).is_err());
    }
    #[test]
    fn edits_alternate_and_force_manual_exposure() {
        let mut base = Settings::new("Sunlight", "Standard");
        base.exposure_auto = true;
        base.exposure = 1.;
        assert_eq!(edit(&base, "exposure", 0).exposure, 1.25);
        assert_eq!(edit(&base, "exposure", 1).exposure, 0.75);
        assert!(!edit(&base, "exposure", 0).exposure_auto);
        assert_eq!(edit(&base, "noise", 0).reconstruction.unwrap().chroma, 1);
        assert_eq!(edit(&base, "noise", 1).reconstruction.unwrap().chroma, 2);
        assert!(base.reconstruction.is_none());
    }
}
