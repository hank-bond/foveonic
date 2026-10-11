//! Inspects Merrill X3Fs and exports single images or batches through shared TIFF settings.

use foveonic::profile::Profile;
use foveonic::render::{Engine, RepairThreshold, Settings};
use foveonic::source::{Result, Source};
use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: foveonic inspect INPUT.x3f
       foveonic render INPUT.x3f OUTPUT.tiff --wb NAME --mode NAME [OPTIONS]
       foveonic batch OUTPUT_DIR INPUT_OR_DIR... --wb NAME --mode NAME [OPTIONS]

Batch directories are scanned nonrecursively for .x3f files. Outputs use each input's
stem with a .tiff extension. Existing outputs and duplicate output names are rejected.

Options (also available for batch):
  --exposure STOPS           Default 0
  --contrast VALUE          -2..2, default 0
  --shadows VALUE           -2..2, default 0
  --highlights VALUE        -2..2, default 0
  --saturation FACTOR       Nonnegative, default 1; 0 produces grayscale
  --sensor-gain FACTOR      Nonnegative uniform sensor-layer multiplier, default 1
  --color-dq-gain FACTOR     Camera ColorDQ amplitude multiplier, default 0 (off)
  --repair                  Camera-derived ordered repair (default on)
  --no-repair               Disable camera-record repair
  --despeckle               Detect and correct remaining speckles (default on)
  --no-despeckle            Disable dynamic speckle correction
  --repair-badness MIN       Enable ordered repair with a manual cutoff; 0..15
  --noise-override KEY=VALUE Override a reconstruction parameter; repeatable, enables reconstruction
                            Keys: base-blend, edge-cutoff, mean-decay, edge-scale,
                            luma-noise-scale, brightness-mix, layer-mix
  --reconstruction          Enable reference-style full-resolution noise/detail processing
  --chroma-noise CHOICE      Camera noise-table choice 0..4, default 2; enables reconstruction
  --luma-noise CHOICE        Camera noise-table choice 0..4, default 2; enables reconstruction
  --detail FACTOR           Residual detail multiplier, default 1; enables reconstruction
  --sharpen AMOUNT           Nonnegative unsharp-mask amount, default 0 (off)
  --sharpen-radius PIXELS    Gaussian sigma in source pixels, >0..16, default 1
  --size WIDTHxHEIGHT       Exact final oriented dimensions, bilinear sampling
  --profile                 Report host wall times and GPU timestamps where supported
  --help                    Show this help";

fn parse_settings(args: impl Iterator<Item = OsString>) -> Result<(Settings, Vec<PathBuf>, bool)> {
    let mut settings = Settings::new("", "");
    let mut paths = Vec::new();
    let mut profiling = false;
    let mut args = args;
    while let Some(arg) = args.next() {
        if arg == "--reconstruction" {
            settings.reconstruction.get_or_insert_with(Default::default);
            continue;
        }
        if arg == "--repair" {
            settings.repair = Some(RepairThreshold::Camera);
            continue;
        }
        if arg == "--no-repair" {
            settings.repair = None;
            continue;
        }
        if arg == "--despeckle" || arg == "--no-despeckle" {
            settings.despeckle = arg == "--despeckle";
            continue;
        }
        if arg == "--profile" {
            profiling = true;
            continue;
        }
        if !arg.to_str().is_some_and(|s| s.starts_with("--")) {
            paths.push(arg.into());
            continue;
        }
        let flag = arg.to_str().ok_or("option must be UTF-8")?;
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?
            .into_string()
            .map_err(|_| "setting must be UTF-8")?;
        match flag {
            "--wb" => settings.white_balance = value,
            "--mode" => settings.color_mode = value,
            "--exposure" => settings.exposure = value.parse()?,
            "--contrast" => settings.tone.contrast = value.parse()?,
            "--shadows" => settings.tone.shadows = value.parse()?,
            "--highlights" => settings.tone.highlights = value.parse()?,
            "--saturation" => settings.saturation = value.parse()?,
            "--sensor-gain" => settings.sensor_gain = value.parse()?,
            "--color-dq-gain" => settings.color_dq_gain = value.parse()?,
            "--repair-badness" => settings.repair = Some(RepairThreshold::Manual(value.parse()?)),
            "--noise-override" => {
                let (key, value) = value
                    .split_once('=')
                    .ok_or("noise override requires KEY=VALUE")?;
                let parameter = foveonic::noise::Parameter::ALL
                    .into_iter()
                    .find(|p| p.key() == key)
                    .ok_or("unknown noise override parameter")?;
                settings
                    .reconstruction
                    .get_or_insert_with(Default::default)
                    .overrides
                    .set(parameter, Some(value.parse()?));
            }
            "--chroma-noise" => {
                settings
                    .reconstruction
                    .get_or_insert_with(Default::default)
                    .chroma = value.parse()?
            }
            "--luma-noise" => {
                settings
                    .reconstruction
                    .get_or_insert_with(Default::default)
                    .luminance = value.parse()?
            }
            "--detail" => {
                settings
                    .reconstruction
                    .get_or_insert_with(Default::default)
                    .detail = value.parse()?
            }
            "--sharpen" => settings.sharpen_amount = value.parse()?,
            "--sharpen-radius" => settings.sharpen_radius = value.parse()?,
            "--size" => {
                let (width, height) = value.split_once('x').ok_or("size requires WIDTHxHEIGHT")?;
                settings.output_size = Some([width.parse()?, height.parse()?]);
            }
            _ => return Err(format!("unknown option {flag}\n{USAGE}").into()),
        }
    }
    settings.validate()?;
    Ok((settings, paths, profiling))
}

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
        Some("render") => {
            let destination = PathBuf::from(args.next().ok_or(USAGE)?);
            let (settings, extra, profiling) = parse_settings(args)?;
            if !extra.is_empty() {
                return Err(USAGE.into());
            }
            if destination.exists() {
                return Err(format!("output already exists: {}", destination.display()).into());
            }
            let source = Source::open(path)?;
            let mut profile = Profile::new(profiling);
            let engine = profile.measure("GPU init", Engine::new)?;
            let image = engine.render(&source, &settings, &mut profile)?;
            let size = engine.write_tiff(&image, &destination, &mut profile)?;
            if profiling {
                eprintln!("Profile: {}", serde_json::to_string(&profile)?);
            }
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
            std::fs::create_dir_all(&destination)?;
            for (index, (input, output)) in jobs.iter().enumerate() {
                // One source at a time; batch retains neither decoded images nor graph buffers.
                let source =
                    Source::open(input).map_err(|error| format!("{}: {error}", input.display()))?;
                let mut profile = Profile::new(profiling);
                let image = engine
                    .render(&source, &settings, &mut profile)
                    .map_err(|error| format!("{}: {error}", input.display()))?;
                let size = engine
                    .write_tiff(&image, output, &mut profile)
                    .map_err(|error| format!("{}: {error}", input.display()))?;
                if profiling {
                    eprintln!("Profile: {}", serde_json::to_string(&profile)?);
                }
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
    fn controls_are_shared_and_defaults_enable_independent_corrections() {
        let parse = |args: &str| parse_settings(args.split_whitespace().map(OsString::from));
        let (neutral, paths, profiling) = parse("--wb Sunlight --mode Standard").unwrap();
        assert!(!profiling);
        assert!(parse("--profile --wb Sunlight --mode Standard").unwrap().2);
        assert!(paths.is_empty());
        assert_eq!(neutral.exposure, 0.);
        assert_eq!(neutral.sensor_gain, 1.);
        assert_eq!(neutral.saturation, 1.);
        assert_eq!(neutral.color_dq_gain, 0.);
        assert_eq!(neutral.sharpen_amount, 0.);
        assert_eq!(neutral.repair, Some(RepairThreshold::Camera));
        assert!(neutral.despeckle);
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
        assert!(neutral.output_size.is_none());
        let (settings, paths, _) = parse("a.x3f --wb Auto --mode Vivid --contrast 0.5 --shadows -1 --highlights 1 --sensor-gain 1.3 --saturation 1.2 --color-dq-gain 1.5 --repair-badness 8 --sharpen 0.5 --sharpen-radius 1.5 --size 600x400 b.x3f").unwrap();
        assert_eq!(paths, [PathBuf::from("a.x3f"), PathBuf::from("b.x3f")]);
        assert_eq!(settings.tone.contrast, 0.5);
        assert_eq!(settings.tone.shadows, -1.);
        assert_eq!(settings.tone.highlights, 1.);
        assert_eq!(settings.sensor_gain, 1.3);
        assert_eq!(settings.output_size, Some([600, 400]));
        assert_eq!(settings.color_dq_gain, 1.5);
        assert_eq!(
            parse("--wb Sunlight --mode Standard --color-dq-gain 0")
                .unwrap()
                .0
                .color_dq_gain,
            0.
        );
        for invalid in [
            "--contrast NaN",
            "--shadows 3",
            "--channel-gains 1,1,1", // retired CLI option
            "--sensor-gain -1",
            "--sensor-gain NaN",
            "--sensor-gain inf",
            "--saturation -1",
            "--color-dq-gain -1",
            "--color-dq-gain NaN",
            "--color-dq-gain inf",
            "--repair-badness 16",
            "--noise-override mean-decay=NaN",
            "--noise-override edge-cutoff=1",
            "--noise-override unknown=1",
            "--noise-override mean-decay:1",
            "--sharpen-radius 0",
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
