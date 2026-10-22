//! Measures completed fresh processing with normal photographic controls, separate GPU diagnostics and optional exact goldens.

#[path = "../src/options.rs"]
mod options;
use foveonic::{
    profile::{self, Mode, Profile},
    render::Engine,
    source::Source,
};
use std::{ffi::OsString, hash::Hasher, path::PathBuf, time::Instant};

fn milliseconds(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.
}
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut args = std::env::args_os().skip(1);
    let input=PathBuf::from(args.next().ok_or("usage: profile_pipeline INPUT.x3f NEW_EXPORT_DIR [--warm N] [--goldens DIR] --wb NAME --mode NAME [normal render options]")?);
    let output = PathBuf::from(args.next().ok_or("missing export directory")?);
    // Mixing old and new runs would make the evidence ambiguous. Require
    // a new output directory rather than silently appending.
    if output.exists() {
        return Err("export directory must be new".into());
    }
    let mut warm = 2usize;
    let mut goldens = None;
    let mut preview_quality = foveonic::render::PreviewQuality::Full;
    let mut controls = Vec::<OsString>::new();
    while let Some(arg) = args.next() {
        if arg == "--preview-quality" {
            preview_quality = match args.next().and_then(|v| v.into_string().ok()).as_deref() {
                Some("full") => foveonic::render::PreviewQuality::Full,
                Some("fast") => foveonic::render::PreviewQuality::Fast,
                _ => return Err("preview quality requires full or fast".into()),
            };
        } else if arg == "--warm" {
            warm = args
                .next()
                .ok_or("missing warm count")?
                .into_string()
                .map_err(|_| "warm count must be UTF-8")?
                .parse()?;
        } else if arg == "--goldens" {
            goldens = Some(PathBuf::from(
                args.next().ok_or("missing golden directory")?,
            ));
        } else {
            controls.push(arg);
        }
    }
    let source = Source::open(&input)?;
    let (mut settings, paths, mode) =
        options::parse_settings_for_camera(controls.into_iter(), &source.metadata)?;
    settings.preview_quality = preview_quality;
    if !paths.is_empty() {
        return Err("unexpected extra input paths".into());
    }
    let engine = Engine::new()?;
    mode.check_gpu_features(engine.device().features())?;
    std::fs::create_dir(&output)?;
    let stem = input
        .file_stem()
        .ok_or("missing input stem")?
        .to_string_lossy();
    println!(
        "{}",
        profile::report(
            "session",
            mode,
            serde_json::json!({"frontend":"completed_processing","preview_quality":format!("{preview_quality:?}"),"source":input,"adapter":engine.adapter_name(),"settings":settings,"gpu_supported":engine.device().features().contains(wgpu::Features::TIMESTAMP_QUERY)})
        )
    );
    println!(
        "{}",
        profile::report(
            "source_load",
            mode,
            serde_json::json!({"source":input,"timings":source.load_timings(),"read_cache_state":"uncontrolled"})
        )
    );
    // Reusing an edit session would measure slider feedback, not a fresh render.
    // Keep synchronized GPU diagnostics separate because their waits change wall time.
    for iteration in 0..warm + 1 + usize::from(mode.gpu()) {
        let diagnostic = mode.gpu() && iteration == warm + 1;
        let sample_mode = if diagnostic {
            Mode::Gpu
        } else if mode.enabled() {
            Mode::Wall
        } else {
            Mode::Off
        };
        let mut profile = Profile::new(sample_mode);
        let before = engine.gpu_activity();
        let start = Instant::now();
        let image = engine.render(&source, &settings, &mut profile)?;
        let render_call = milliseconds(start);
        engine.device().poll(wgpu::Maintain::Wait);
        let completed = milliseconds(start);
        let activity = engine.gpu_activity().since(before);
        println!(
            "{}",
            profile::report(
                "processing",
                sample_mode,
                serde_json::json!({"source":input,"settings":settings,"output":"final","execution":"fresh","iteration":iteration,"cold":iteration==0,"render_call_ms":render_call,"completed_ms":completed,"profile":profile,"activity":activity})
            )
        );
        let mut profile = Profile::new(sample_mode);
        let before = engine.gpu_activity();
        let start = Instant::now();
        let destination = output.join(format!("{stem}-{iteration}.tiff"));
        engine.write_tiff(&image, &destination, &mut profile)?;
        engine.device().poll(wgpu::Maintain::Wait);
        println!(
            "{}",
            profile::report(
                "export",
                sample_mode,
                serde_json::json!({"source":input,"settings":settings,"iteration":iteration,"destination":destination,"completed_ms":milliseconds(start),"profile":profile,"activity":engine.gpu_activity().since(before)})
            )
        );
        // Reading every sample for comparison adds work and transfers. Keep that
        // validation outside the intervals used for performance claims.
        if iteration == 0 {
            let bytes = engine.readback_image(&image)?;
            let mut hasher = std::hash::DefaultHasher::new();
            hasher.write(&bytes);
            if let Some(dir) = &goldens {
                std::fs::create_dir_all(dir)?;
                let file = dir.join(format!("{stem}.f32"));
                if file.exists() {
                    if bytes != std::fs::read(file)? {
                        return Err(format!("non-equivalent output: {stem}").into());
                    }
                } else {
                    std::fs::write(file, &bytes)?;
                }
            }
            println!(
                "{}",
                profile::report(
                    "validation",
                    sample_mode,
                    serde_json::json!({"source":input,"digest":format!("{:016x}",hasher.finish()),"outside_timing":true})
                )
            );
        }
    }
    Ok(())
}
