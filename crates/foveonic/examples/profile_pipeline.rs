//! Measures completed fresh processing with normal photographic controls, separate GPU diagnostics and optional exact goldens.

#[path = "../src/options.rs"]
#[allow(dead_code)]
mod options;
#[path = "profile_pipeline/sample_difference.rs"]
mod sample_difference;
#[path = "profile_pipeline/verify_batch.rs"]
mod verify_batch;
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
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|arg| arg == "--verify-batch") {
        if arguments.len() != 2 {
            return Err("usage: profile_pipeline --verify-batch MANIFEST.json".into());
        }
        return verify_batch::run(std::path::Path::new(&arguments[1]));
    }
    if arguments
        .first()
        .is_some_and(|arg| arg == "--compare-samples")
    {
        if arguments.len() != 5 || arguments[3] != "--endianness" {
            return Err(
                "usage: profile_pipeline --compare-samples LEFT RIGHT --endianness little|big"
                    .into(),
            );
        }
        return sample_difference::run(
            std::path::Path::new(&arguments[1]),
            std::path::Path::new(&arguments[2]),
            arguments[4].to_str().ok_or("endianness must be UTF-8")?,
        );
    }
    let mut args = arguments.into_iter();
    let input=PathBuf::from(args.next().ok_or("usage: profile_pipeline INPUT.x3f NEW_EXPORT_DIR [--warm N] [--goldens DIR] [--render-only] --wb NAME --mode NAME [normal render options]")?);
    let output = PathBuf::from(args.next().ok_or("missing export directory")?);
    // Mixing old and new runs would make the evidence ambiguous. Require
    // a new output directory rather than silently appending.
    if output.exists() {
        return Err("export directory must be new".into());
    }
    let mut warm = 2usize;
    let mut render_only = false;
    let mut goldens = None;
    let mut capture = None;
    let mut settings_snapshot = None;
    let mut working_scale = foveonic::render::Scale::NATIVE;
    let mut scale_selected = false;
    let mut thumb = false;
    let mut controls = Vec::<OsString>::new();
    while let Some(arg) = args.next() {
        if arg == "--render-only" {
            render_only = true;
        } else if arg == "--resolution" {
            scale_selected = true;
            let quality = args.next().and_then(|v| v.into_string().ok());
            thumb = quality.as_deref() == Some("thumb");
            working_scale = match quality.as_deref() {
                Some("full") => foveonic::render::Scale::NATIVE,
                Some("fast") => foveonic::render::Scale::new(2).unwrap(),
                Some("draft") => foveonic::render::Scale::new(4).unwrap(),
                Some("thumb") => foveonic::render::Scale::new(8).unwrap(),
                _ => return Err("resolution requires full, fast, draft or thumb".into()),
            };
        } else if arg == "--working-scale" {
            scale_selected = true;
            thumb = false;
            let divisor = args
                .next()
                .ok_or("missing working scale divisor")?
                .into_string()
                .map_err(|_| "working scale must be UTF-8")?
                .parse()?;
            working_scale = foveonic::render::Scale::new(divisor)?;
        } else if arg == "--warm" {
            warm = args
                .next()
                .ok_or("missing warm count")?
                .into_string()
                .map_err(|_| "warm count must be UTF-8")?
                .parse()?;
        } else if arg == "--settings-snapshot" {
            settings_snapshot = Some(PathBuf::from(
                args.next().ok_or("missing settings snapshot")?,
            ));
        } else if arg == "--capture-float" {
            let name = args
                .next()
                .ok_or("missing capture output name")?
                .into_string()
                .map_err(|_| "capture name must be UTF-8")?;
            let path = PathBuf::from(args.next().ok_or("missing capture file")?);
            capture = Some((name, path));
        } else if arg == "--goldens" {
            goldens = Some(PathBuf::from(
                args.next().ok_or("missing golden directory")?,
            ));
        } else {
            controls.push(arg);
        }
    }
    if capture.as_ref().is_some_and(|(_, path)| path.exists()) {
        return Err("capture destination must be new".into());
    }
    let source = Source::open(&input)?;
    let (settings, mode) = if let Some(path) = settings_snapshot {
        if scale_selected || !controls.is_empty() {
            return Err("a settings snapshot cannot be combined with render options".into());
        }
        let snapshot: profile::RenderSettingsSnapshot =
            serde_json::from_slice(&std::fs::read(path)?)?;
        let settings = snapshot.into_settings();
        working_scale = settings.working_scale;
        (settings, Mode::Off)
    } else {
        let (mut settings, paths, mode) =
            options::parse_settings_for_camera(&controls, &source.metadata)?;
        if thumb {
            working_scale = foveonic::render::ControlCapabilities::from_metadata(&source.metadata)?
                .thumb_scale();
        }
        settings.working_scale = working_scale;
        if !paths.is_empty() {
            return Err("unexpected extra input paths".into());
        }
        (settings, mode)
    };
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
            serde_json::json!({"frontend":"completed_processing","render_only":render_only,"scale_divisor":working_scale.divisor(),"family":source.metadata.processing_family().ok(),"source":input,"adapter":engine.adapter_name(),"settings":settings,"processing_quality":settings.processing_quality,"gpu_supported":engine.device().features().contains(wgpu::Features::TIMESTAMP_QUERY)})
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
        let image = if render_only {
            // Use the same named-output request path as interactive sessions.
            // The convenience render API deliberately accepts whole photographs.
            let artifact = engine
                .render_artifacts(&source, &settings, &["final"], &mut profile)?
                .pop()
                .ok_or("missing final image")?;
            let foveonic::inspection::Content::Image(image) = artifact.content else {
                return Err("final output is not an image".into());
            };
            *image
        } else {
            engine.render(&source, &settings, &mut profile)?
        };
        let render_call = milliseconds(start);
        engine
            .device()
            .poll(wgpu::PollType::Wait)
            .expect("GPU polling failed");
        let completed = milliseconds(start);
        let activity = engine.gpu_activity().since(before);
        println!(
            "{}",
            profile::report(
                "processing",
                sample_mode,
                serde_json::json!({"source":input,"settings":settings,"processing_quality":settings.processing_quality,"effective_scale_divisor":image.settings().working_scale.divisor(),"output":"final","execution":"fresh","iteration":iteration,"cold":iteration==0,"render_call_ms":render_call,"completed_ms":completed,"profile":profile,"activity":activity})
            )
        );
        // Regional previews are useful numerical outputs but cannot be exported
        // as authored whole photographs. Keep their measurements render-only.
        if !render_only {
            let mut profile = Profile::new(sample_mode);
            let before = engine.gpu_activity();
            let start = Instant::now();
            let destination = output.join(format!("{stem}-{iteration}.tiff"));
            engine.write_tiff(&image, &destination, &mut profile)?;
            engine
                .device()
                .poll(wgpu::PollType::Wait)
                .expect("GPU polling failed");
            println!(
                "{}",
                profile::report(
                    "export",
                    sample_mode,
                    serde_json::json!({"source":input,"settings":settings,"processing_quality":settings.processing_quality,"iteration":iteration,"destination":destination,"completed_ms":milliseconds(start),"profile":profile,"activity":engine.gpu_activity().since(before)})
                )
            );
        }
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
                    serde_json::json!({"source":input,"digest":format!("{:016x}",hasher.finish()),
                        "extent":image.description().extent,"layout":format!("{:?}",image.description().layout),
                        "space":format!("{:?}",image.description().space),"linear_white":image.description().linear_white,
                        "view_baked":image.description().view_baked,
                        "endianness":if cfg!(target_endian="little") {"little"} else {"big"},
                        "view_extent":image.view().extent,"view_crop":image.view().crop,
                        "view_rotation":image.view().rotation,"outside_timing":true})
                )
            );
        }
    }
    if let Some((name, path)) = capture {
        use foveonic::inspection::Content;
        use std::io::Write;
        let mut artifacts =
            engine.render_artifacts(&source, &settings, &[&name], &mut Profile::default())?;
        let Content::Image(image) = artifacts.remove(0).content else {
            return Err("capture output is not an image".into());
        };
        let description = image.description();
        if !matches!(
            description.layout,
            foveonic::image::Layout::Rgb32Float | foveonic::image::Layout::Scalar32Float
        ) {
            return Err("capture output must use float RGB or scalar storage".into());
        }
        let view = image.view();
        let pixels = engine.readback_image(&image)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(&pixels)?;
        println!(
            "{}",
            profile::report(
                "capture",
                Mode::Off,
                serde_json::json!({
                    "source": input, "settings": settings, "processing_quality": settings.processing_quality, "effective_scale_divisor": image.settings().working_scale.divisor(), "output": name, "file": path, "extent": description.extent,
                    "dtype": "f32", "endianness": if cfg!(target_endian = "little") {"little"} else {"big"},
                    "layout": format!("{:?}", description.layout), "space": format!("{:?}", description.space),
                    "linear_white": description.linear_white, "view_crop": view.crop,
                    "rotation": view.rotation, "view_extent": view.extent,
                    "outside_timing": true
                })
            )
        );
    }
    Ok(())
}
