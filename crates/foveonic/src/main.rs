//! Inspects Merrill X3Fs and exports TIFF or linear DNG through shared rendering settings.

mod options;
use foveonic::profile::{self, Mode, Profile};
#[cfg(test)]
use foveonic::render::RepairThreshold;
use foveonic::render::{Engine, Settings};
use foveonic::source::{Result, Source};
use options::parse_settings;
use std::collections::HashSet;
#[cfg(test)]
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: foveonic inspect INPUT.x3f
       foveonic render INPUT.x3f OUTPUT.tiff --wb NAME [--mode NAME] [OPTIONS]
       foveonic export-dng INPUT.x3f OUTPUT.dng --wb NAME [--mode NAME] [OPTIONS]
       foveonic batch OUTPUT_DIR INPUT_OR_DIR... --wb NAME [--mode NAME] [OPTIONS]

Batch directories are scanned nonrecursively for .x3f files. Outputs use each input's
stem with a .tiff extension. Existing outputs and duplicate output names are rejected.
Color requires --mode. Any --mono-* option selects monochrome automatically.

Options (also available for batch):
  --exposure STOPS           Default 0
  --contrast VALUE          -2..2, default 0
  --shadows VALUE           -2..2, default 0
  --highlights VALUE        -2..2, default 0
  --rendering MODE          color (default) or monochrome
  --mono-mix R,G,B          Nonnegative sensor-layer amounts, normalized to one
  --mono-noise N            Mono histogram-noise preset 0..4 (default 2, 0 skips)
  --mono-banding N          Mono column-correction preset 0..4 (default 2)
  --saturation FACTOR       Nonnegative, default 1. Inactive in monochrome
  --color-dq-gain FACTOR     Override native ColorDQ gain (0 off). Reconstruction defaults to auto
  --repair                  Camera-derived ordered repair (default on)
  --no-repair               Disable camera-record repair
  --despeckle               Detect and correct remaining speckles (default on)
  --no-despeckle            Disable dynamic speckle correction
  --highlight-restoration   Restore original clipped-layer color (default on)
  --no-highlight-restoration Disable clipped-layer restoration and neutralization
  --highlight-threshold EV  -2..2 relative to camera threshold, or auto (default). +1 doubles it
  --row-correction          Reduce additive per-row sensor banding (default on)
  --no-row-correction       Disable per-row banding correction
  --repair-badness MIN       Enable ordered repair with a manual cutoff in 0..15
  --noise-override KEY=VALUE Override a reconstruction parameter. Repeatable, enables reconstruction
                            Keys: base-blend, edge-cutoff, mean-decay, edge-scale,
                            luma-noise-scale, brightness-mix, layer-mix
  --reconstruction          Enable reference-style full-resolution noise/detail processing
  --chroma-noise CHOICE      Camera noise-table choice 0..4, default 2. Enables reconstruction
  --luma-noise CHOICE        Camera noise-table choice 0..4, default 2. Enables reconstruction
  --banding-noise CHOICE     Column-noise preset 0..4, default 2. Enables reconstruction
  --detail FACTOR           Residual detail multiplier, default 1. Enables reconstruction
  --sharpen AMOUNT           Nonnegative native high-pass amount before tone, default 0 (off)
  --size WIDTHxHEIGHT       Exact final oriented dimensions, bilinear sampling
  --profile MODE            off (default), wall, or synchronized gpu diagnostics
  --help                    Show this help";

fn batch_jobs(inputs: &[PathBuf], destination: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
    if destination.exists() && !destination.is_dir() {
        return Err("batch destination is not a directory".into());
    }
    let mut files = Vec::new();
    for input in inputs {
        if input.is_dir() {
            let mut entries = Vec::new();
            for entry in std::fs::read_dir(input)? {
                let entry = entry?;
                if entry.file_type()?.is_file()
                    && entry
                        .path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("x3f"))
                {
                    entries.push(entry.path());
                }
            }
            entries.sort();
            files.extend(entries);
        } else if input.is_file() {
            files.push(input.clone());
        } else {
            return Err(format!("input does not exist: {}", input.display()).into());
        }
    }
    if files.is_empty() {
        return Err("batch has no input files".into());
    }
    let mut outputs = HashSet::new();
    let mut jobs = Vec::new();
    for input in files {
        let mut name = input
            .file_stem()
            .ok_or("input has no filename")?
            .to_os_string();
        name.push(".tiff");
        let output = destination.join(name);
        // Case-fold output names conservatively for the owner's case-insensitive macOS filesystem.
        let key = output.file_name().unwrap().to_string_lossy().to_lowercase();
        if !outputs.insert(key) {
            return Err(format!("duplicate batch output: {}", output.display()).into());
        }
        if output.exists() {
            return Err(format!("output already exists: {}", output.display()).into());
        }
        jobs.push((input, output));
    }
    Ok(jobs)
}

fn convert(
    engine: &Engine,
    source: &Source,
    input: &Path,
    settings: &Settings,
    destination: &Path,
    mode: Mode,
) -> Result<[u32; 2]> {
    mode.check_gpu_features(engine.device().features())?;
    if mode.enabled() {
        eprintln!(
            "{}",
            profile::report(
                "source_load",
                mode,
                serde_json::json!({"source":input,"timings":source.load_timings(),"read_cache_state":"uncontrolled"})
            )
        );
    }
    let mut profile = Profile::new(mode);
    let before = engine.gpu_activity();
    let start = std::time::Instant::now();
    let image = engine.render(source, settings, &mut profile)?;
    if mode.enabled() {
        eprintln!(
            "{}",
            profile::report(
                "processing",
                mode,
                serde_json::json!({"source":input,"settings":settings,"output":"final","execution":"fresh","render_call_ms":start.elapsed().as_secs_f64()*1000.,"profile":profile,"activity":engine.gpu_activity().since(before)})
            )
        );
    }
    let mut profile = Profile::new(mode);
    let before = engine.gpu_activity();
    let start = std::time::Instant::now();
    let size = engine.write_tiff(&image, destination, &mut profile)?;
    if mode.enabled() {
        eprintln!(
            "{}",
            profile::report(
                "export",
                mode,
                serde_json::json!({"source":input,"destination":destination,"settings":settings,"completed_ms":start.elapsed().as_secs_f64()*1000.,"profile":profile,"activity":engine.gpu_activity().since(before)})
            )
        );
    }
    Ok(size)
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.iter().any(|a| a == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    let mut args = args.into_iter();
    let command = args.next().ok_or(USAGE)?;
    let path = args.next().ok_or(USAGE)?;
    match command.to_str() {
        Some("inspect") => {
            if args.next().is_some() {
                return Err(USAGE.into());
            }
            let source = Source::open(path)?;
            let mut output = std::io::stdout().lock();
            serde_json::to_writer_pretty(&mut output, source.metadata.as_ref())?;
            writeln!(output)?;
        }
        Some("render") | Some("export-dng") => {
            let destination = PathBuf::from(args.next().ok_or(USAGE)?);
            let (settings, extra, profiling) = parse_settings(args)?;
            if !extra.is_empty() {
                return Err(USAGE.into());
            }
            if destination.exists() {
                return Err(format!("output already exists: {}", destination.display()).into());
            }
            let source = Source::open(&path)?;
            let mut setup = Profile::new(profiling);
            let engine = setup.measure("GPU init", Engine::new)?;
            if profiling.enabled() {
                eprintln!(
                    "{}",
                    profile::report(
                        "session",
                        profiling,
                        serde_json::json!({"frontend":"cli","adapter":engine.adapter_name(),"profile":setup,"gpu_supported":engine.device().features().contains(wgpu::Features::TIMESTAMP_QUERY)})
                    )
                );
            }
            if command == "export-dng" {
                let mut profile = Profile::new(profiling);
                foveonic::dng::write_source(
                    &engine,
                    &source,
                    &settings,
                    &destination,
                    &mut profile,
                )?;
                if profiling.enabled() {
                    eprintln!(
                        "{}",
                        profile::report(
                            "export",
                            profiling,
                            serde_json::json!({"source":path,"destination":destination,"settings":settings,"profile":profile})
                        )
                    );
                }
                eprintln!(
                    "Exported linear DNG using {}: {}",
                    engine.adapter_name(),
                    destination.display()
                );
                return Ok(());
            }
            let size = convert(
                &engine,
                &source,
                Path::new(&path),
                &settings,
                &destination,
                profiling,
            )?;
            eprintln!(
                "Rendered {}x{} RGB16 TIFF using {}: {}",
                size[0],
                size[1],
                engine.adapter_name(),
                destination.display()
            );
        }
        Some("batch") => {
            let destination = PathBuf::from(path);
            let (settings, inputs, profiling) = parse_settings(args)?;
            if inputs.is_empty() {
                return Err(USAGE.into());
            }
            let jobs = batch_jobs(&inputs, &destination)?;
            let engine = Engine::new()?;
            if profiling.enabled() {
                eprintln!(
                    "{}",
                    profile::report(
                        "session",
                        profiling,
                        serde_json::json!({"frontend":"cli","adapter":engine.adapter_name(),"gpu_supported":engine.device().features().contains(wgpu::Features::TIMESTAMP_QUERY)})
                    )
                );
            }
            std::fs::create_dir_all(&destination)?;
            for (index, (input, output)) in jobs.iter().enumerate() {
                // Drop each photo before opening the next so batch memory use does not
                // grow with the number of files.
                let source =
                    Source::open(input).map_err(|error| format!("{}: {error}", input.display()))?;
                let size = convert(&engine, &source, input, &settings, output, profiling)
                    .map_err(|error| format!("{}: {error}", input.display()))?;
                eprintln!(
                    "[{}/{}] {}x{}: {}",
                    index + 1,
                    jobs.len(),
                    size[0],
                    size[1],
                    output.display()
                );
            }
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monochrome_cli_resolves_actual_layer_proportions_without_a_color_mode() {
        let parse = |args: &str| parse_settings(args.split_whitespace().map(Into::into));
        let s = parse("--wb Sunlight --mono-mix 0,0,1 --mono-noise 0 --mono-banding 4")
            .unwrap()
            .0;
        assert!(s.is_monochrome());
        assert_eq!(s.monochrome.mix, [0., 0., 1.]);
        assert_eq!(s.monochrome.noise, 0);
        assert_eq!(s.monochrome.banding, 4);
        assert_eq!(
            parse("--wb Sunlight --mono-mix 2,1,1")
                .unwrap()
                .0
                .monochrome
                .mix,
            [0.5, 0.25, 0.25]
        );
        for bad in [
            "--mono-mix 0,0,0",
            "--mono-mix 1,1",
            "--mono-mix -1,1,1",
            "--mono-mix NaN,1,1",
            "--mono-noise 5",
            "--mono-banding 5",
            "--rendering invalid",
        ] {
            assert!(parse(&format!("--wb Sunlight {bad}")).is_err(), "{bad}");
        }
        assert!(parse("--wb Sunlight --rendering color").is_err());
    }

    #[test]
    fn controls_are_shared_and_defaults_enable_independent_corrections() {
        let parse = |args: &str| parse_settings(args.split_whitespace().map(OsString::from));
        let (neutral, paths, profiling) = parse("--wb Sunlight --mode Standard").unwrap();
        assert_eq!(profiling, Mode::Off);
        assert_eq!(
            parse("--profile wall --wb Sunlight --mode Standard")
                .unwrap()
                .2,
            Mode::Wall
        );
        assert_eq!(
            parse("--profile gpu --wb Sunlight --mode Standard")
                .unwrap()
                .2,
            Mode::Gpu
        );
        assert!(parse("--profile invalid --wb Sunlight --mode Standard").is_err());
        assert!(paths.is_empty());
        assert_eq!(neutral.exposure, 0.);
        assert_eq!(neutral.saturation, 1.);
        assert_eq!(neutral.color_dq_gain, None);
        assert_eq!(neutral.highlight_threshold_ev, None);
        assert_eq!(neutral.sharpen_amount, 0.);
        assert_eq!(neutral.repair, Some(RepairThreshold::Camera));
        assert!(neutral.despeckle && neutral.highlight_restoration && neutral.row_correction);
        let rows_off = parse("--wb Sunlight --mode Standard --no-row-correction")
            .unwrap()
            .0;
        assert!(
            !rows_off.row_correction
                && rows_off.despeckle
                && rows_off.highlight_restoration
                && rows_off.repair.is_some()
        );
        assert!(
            parse("--wb Sunlight --mode Standard --no-row-correction --row-correction")
                .unwrap()
                .0
                .row_correction
        );
        let off = parse("--wb Sunlight --mode Standard --no-highlight-restoration")
            .unwrap()
            .0;
        assert!(!off.highlight_restoration && off.repair.is_some() && off.despeckle);
        assert!(
            parse(
                "--wb Sunlight --mode Standard --no-highlight-restoration --highlight-restoration"
            )
            .unwrap()
            .0
            .highlight_restoration
        );
        for ev in [-2., -1., 0., 1., 2.] {
            let manual = parse(&format!(
                "--wb Sunlight --mode Standard --highlight-threshold {ev}"
            ))
            .unwrap()
            .0;
            assert_eq!(manual.highlight_threshold_ev, Some(ev));
            assert!(manual.highlight_restoration);
            assert_eq!(manual.tone, neutral.tone);
        }
        let off = parse(
            "--wb Sunlight --mode Standard --highlight-threshold 1 --no-highlight-restoration",
        )
        .unwrap()
        .0;
        assert_eq!(off.highlight_threshold_ev, Some(1.));
        assert!(!off.highlight_restoration);
        assert_eq!(
            parse(
                "--wb Sunlight --mode Standard --highlight-threshold 1 --highlight-threshold auto"
            )
            .unwrap()
            .0,
            neutral
        );
        assert!(parse("--wb Sunlight --mode Standard --highlight-threshold").is_err());
        let disabled = parse("--wb Sunlight --mode Standard --no-repair --no-despeckle")
            .unwrap()
            .0;
        assert!(disabled.repair.is_none() && !disabled.despeckle);
        let restored =
            parse("--wb Sunlight --mode Standard --no-repair --repair --no-despeckle --despeckle")
                .unwrap()
                .0;
        assert_eq!(restored.repair, Some(RepairThreshold::Camera));
        assert!(restored.despeckle);
        assert!(neutral.reconstruction.is_none());
        assert_eq!(
            parse("--wb Sunlight --mode Standard --repair")
                .unwrap()
                .0
                .repair,
            Some(RepairThreshold::Camera)
        );
        assert_eq!(
            parse("--wb Sunlight --mode Standard --repair-badness 2")
                .unwrap()
                .0
                .repair,
            Some(RepairThreshold::Manual(2))
        );
        let overridden = parse("--wb Sunlight --mode Standard --noise-override mean-decay=0.7 --noise-override brightness-mix=0.5").unwrap().0.reconstruction.unwrap();
        assert_eq!(
            overridden
                .overrides
                .get(foveonic::noise::Parameter::MeanDecay),
            Some(0.7)
        );
        assert_eq!(
            overridden
                .overrides
                .get(foveonic::noise::Parameter::BrightnessMix),
            Some(0.5)
        );
        assert_eq!(
            parse("--wb Sunlight --mode Standard --reconstruction")
                .unwrap()
                .0
                .reconstruction,
            Some(Default::default())
        );
        assert_eq!(
            parse("--wb Sunlight --mode Standard --chroma-noise 1 --luma-noise 4 --detail 0.75")
                .unwrap()
                .0
                .reconstruction,
            Some(foveonic::noise::Controls {
                chroma: 1,
                luminance: 4,
                detail: 0.75,
                ..Default::default()
            })
        );
        for banding in 0..=4 {
            let settings = parse(&format!(
                "--wb Sunlight --mode Standard --banding-noise {banding}"
            ))
            .unwrap()
            .0;
            assert_eq!(settings.reconstruction.unwrap().banding, banding);
            assert!(settings.row_correction);
        }
        assert!(neutral.output_size.is_none());
        let (settings, paths, _) = parse("a.x3f --wb Auto --mode Vivid --contrast 0.5 --shadows -1 --highlights 1 --saturation 1.2 --color-dq-gain 1.5 --repair-badness 8 --sharpen 0.5 --size 600x400 b.x3f").unwrap();
        assert_eq!(paths, [PathBuf::from("a.x3f"), PathBuf::from("b.x3f")]);
        assert_eq!(settings.tone.contrast, 0.5);
        assert_eq!(settings.tone.shadows, -1.);
        assert_eq!(settings.tone.highlights, 1.);
        assert_eq!(settings.output_size, Some([600, 400]));
        assert_eq!(settings.color_dq_gain, Some(1.5));
        assert_eq!(
            parse("--wb Sunlight --mode Standard --color-dq-gain 0")
                .unwrap()
                .0
                .color_dq_gain,
            Some(0.)
        );
        for invalid in [
            "--highlight-threshold NaN",
            "--highlight-threshold inf",
            "--highlight-threshold -inf",
            "--highlight-threshold -2.01",
            "--highlight-threshold 2.01",
            "--highlight-threshold unknown",
            "--contrast NaN",
            "--shadows 3",
            "--channel-gains 1,1,1", // retired CLI option
            "--sensor-gain 1",
            "--saturation -1",
            "--color-dq-gain -1",
            "--color-dq-gain NaN",
            "--color-dq-gain inf",
            "--repair-badness 16",
            "--banding-noise 5",
            "--banding-noise -1",
            "--banding-noise 1.5",
            "--noise-override mean-decay=NaN",
            "--noise-override edge-cutoff=1",
            "--noise-override unknown=1",
            "--noise-override mean-decay:1",
            "--sharpen-radius 1.5", // retired Gaussian control
            "--sharpen -1",
            "--sharpen NaN",
            "--size 0x400",
            "--chroma-noise 5",
            "--luma-noise 5",
            "--detail -1",
            "--detail NaN",
        ] {
            assert!(
                parse(&format!("--wb Sunlight --mode Standard {invalid}")).is_err(),
                "{invalid}"
            );
        }
        assert!(parse("--wb Sunlight").is_err());
    }

    #[test]
    fn batch_scans_one_level_and_preflights_name_collisions_and_existing_outputs() {
        let root = std::env::temp_dir().join(format!(
            "foveonic-batch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let input = root.join("input");
        let out = root.join("out");
        std::fs::create_dir(&input).unwrap();
        std::fs::create_dir(&out).unwrap();
        std::fs::write(input.join("b.X3F"), b"").unwrap();
        std::fs::write(input.join("a.x3f"), b"").unwrap();
        std::fs::write(input.join("ignore.jpg"), b"").unwrap();
        std::fs::create_dir(input.join("nested")).unwrap();
        std::fs::write(input.join("nested/c.x3f"), b"").unwrap();
        let jobs = batch_jobs(std::slice::from_ref(&input), &out).unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].1, out.join("a.tiff"));
        assert!(!out.join("a.tiff").exists());
        assert!(batch_jobs(&[input.clone(), input.join("a.x3f")], &out).is_err());
        std::fs::write(input.join("A.other"), b"").unwrap();
        assert!(batch_jobs(&[input.clone(), input.join("A.other")], &out).is_err());
        std::fs::write(out.join("a.tiff"), b"keep").unwrap();
        assert!(batch_jobs(&[input], &out).is_err());
        assert_eq!(std::fs::read(out.join("a.tiff")).unwrap(), b"keep");
    }
}
