//! Inspects supported X3Fs and exports photographs or linear DNG through shared rendering settings.

mod options;
use foveonic::profile::{self, Mode, Profile};
#[cfg(test)]
use foveonic::render::RepairThreshold;
use foveonic::render::{Engine, Settings};
use foveonic::source::{Result, Source};
use options::parse_settings;
use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: foveonic inspect INPUT.x3f
       foveonic render INPUT.x3f OUTPUT.{tiff,jpg,png,jxl} [--wb NAME] [--mode NAME] [OPTIONS]
       foveonic export-dng INPUT.x3f OUTPUT.dng [--wb NAME] [--mode NAME] [OPTIONS]
       foveonic batch OUTPUT_DIR INPUT_OR_DIR... [--wb NAME] [--mode NAME] [OPTIONS]

Batch directories are scanned nonrecursively for .x3f files. Outputs use each input's
stem with the chosen extension (TIFF by default). Existing outputs and duplicate output names are rejected.
Color requires --wb and --mode. Merrill monochrome requires --wb.
Classic monochrome requires --mode and ignores --wb. Merrill --mono-* options
select monochrome automatically. Classic uses its camera mixture and rejects overrides.
Quattro supports white balance, camera color modes, tone, saturation,
chroma and luminance noise, native Smooth/Crisp detail and sharpening, with camera-derived ISO controls.

Options (also available for batch):
  --tone-method METHOD      reference (default) or universal tone
  --base-response RESPONSE  camera (default), natural or strong (Merrill Universal tone)
  --adaptive-black-limit AMOUNT   Override both profile caps, 0..1
  --adaptive-shadow-limit AMOUNT  Override both profile caps, 0..1
  --adaptive-highlight-limit AMOUNT  Override both profile caps, 0..1
  --adaptive-white-limit AMOUNT  Override both profile caps, 0..1
  --blacks VALUE            Shared HDR -1..1, default 0
  --hdr-shadows VALUE       Shared HDR -1..1, default 0
  --hdr-highlights VALUE    Shared HDR -1..1, default 0
  --whites VALUE            Shared HDR -1..1, default 0
  --local-contrast VALUE    -1..1, default 0. Shared multiscale contrast
  --dehaze VALUE            -100..100, default 0. Remove or add haze
  --haze-color VALUE        auto or R,G,B in normalized linear sRGB
                            HDR, Local Contrast and Universal sharpening bake into DNG
  --color-sensitivity VALUES Eight amounts -1..1: Red,Orange,Yellow,Green,Cyan,Blue,Purple,Magenta\n                            Color/mono sensitivity is baked into DNG
  --sharpen-method METHOD   reference (default) or universal
  --universal-sharpen VALUE 0..4, default 0
  --sharpen-radius PIXELS   Universal Gaussian sigma 0.3..4, default 0.8
  --sharpen-threshold VALUE Universal threshold 0..0.2, default 0.01
  --halo-suppression VALUE  Universal halo suppression 0..1, default 0.5
                            Universal sharpening runs before camera tone
  --exposure STOPS           Default 0
  --auto-exposure            Use inverse capture exposure compensation per photo
  --auto-exposure-compensation MODE  bracket, dial or both (default both)
  --contrast VALUE          -2..2, default 0
  --shadows VALUE           -2..2, default 0
  --highlights VALUE        -2..2, default 0
  --rendering MODE          color (default) or monochrome
  --mono-mix R,G,B          Nonnegative sensor-layer amounts, normalized to one
  --mono-noise N            Mono histogram-noise preset 0..4 (default 2, 0 skips)
  --mono-banding N          Mono column-correction preset 0..4 (default 2)
  --saturation FACTOR       Reference amount, nonnegative, default 1
  --saturation-method METHOD reference (default) or universal, independent of tone
  --universal-saturation N   Processing amount, -1..2 (GUI +1 maps to +2), default 0
  --saturation-contrast N    Universal colorfulness contrast, -1..1, default 0
  --saturation-shadows N     Universal shadow amount, -1..2, default 0
  --saturation-midtones N    Universal midtone amount, -1..2, default 0
  --saturation-highlights N  Universal highlight amount, -1..2, default 0
  --prevent-color-clipping  Protect Universal/Color Range saturation (default)
  --no-prevent-color-clipping Unlock saturation while retaining the shoulder
  --color-dq-gain FACTOR     Override native ColorDQ gain (0 off). Reconstruction defaults to auto
  --repair                  Camera-derived ordered repair (default on)
  --no-repair               Disable camera-record repair
  --despeckle               Detect and correct remaining speckles (default on)
  --no-despeckle            Disable dynamic speckle correction
  --highlight-restoration   Restore original clipped-layer color (default on)
  --no-highlight-restoration Disable clipped-layer restoration and neutralization
  --highlight-threshold EV  Merrill classic method: -2..2 relative to camera threshold, or auto
  --highlight-mode MODE     classic (default), color-field-local, or color-field-global
  --color-field-selectivity N 0..1 selection strength (default 0: original behavior)
                            Higher rejects more inconsistent evidence
  --row-correction          Reduce additive per-row sensor banding (default on)
  --no-row-correction       Disable per-row banding correction
  --repair-badness MIN       Enable ordered repair with a manual cutoff in 0..15
  --reconstruction          Enable full-resolution noise/detail processing
  --fine-detail FACTOR      Fine texture restoration 0..2, default 1
  --coarse-detail FACTOR    Coarse texture restoration 0..2, default 1
  --fine-size FACTOR        Fine boundary size 0.5..2, default 1
  --coarse-size FACTOR      Coarse boundary size 0.5..2, default 1
  --chroma-noise CHOICE      Chroma cleanup 0..4, default 2. Enables reconstruction
  --merrill-banding-method NAME  reference (default) or directional
  --merrill-noise-method NAME  reference (default), rows, or rows-columns. Enables reconstruction
  --luma-noise CHOICE        Luma cleanup 0..4, default 2. Enables reconstruction
  --banding-noise CHOICE     Column-noise preset 0..4, default 2. Enables reconstruction
  --mono-smoothness VALUE    Quattro monochrome Smooth/Crisp 0..1, default from file or 0.5
  --smooth-crisp FACTOR     Quattro Smooth/Crisp 0..1 (default from file or 0.5)
  --sharpen AMOUNT           auto, off, or manual amount (Quattro -2..2, default from file)
  --highlight-amount VALUE  Quattro highlight recovery 0..1, default from file or 1
  --format FORMAT           Batch format: tiff (default), jpeg, png or jxl
  --jpeg-quality N          1..100, default 95
  --png-depth N             8 or 16, default 16
  --jxl-lossless            Preserve exact RGB16 samples (default)
  --jxl-lossy               Use perceptual lossy JPEG XL compression
  --jxl-quality N           Lossy quality 1..100, default 90
  --jxl-effort N            Compression effort 1..11, default 7. 11 is extremely slow
  --tiff-compression NAME   none (default), lzw or zip. All preserve RGB16
  --tiff-color-space SPACE   camera (default), srgb, adobe-rgb, or prophoto-rgb
  --size WIDTHxHEIGHT       Exact final oriented photographic dimensions, reference photographic resampling
  --profile MODE            off (default), wall, or synchronized gpu diagnostics
  --help                    Show this help";

fn batch_jobs(
    inputs: &[PathBuf],
    destination: &Path,
    format: foveonic::output::Format,
) -> Result<Vec<(PathBuf, PathBuf)>> {
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
        name.push(format!(".{}", format.extension()));
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

/// Encoder choices are job preferences, separate from the shared photographic settings parser.
fn parse_export_options(
    args: impl Iterator<Item = OsString>,
) -> Result<(
    Option<foveonic::output::Format>,
    foveonic::output::EncodingOptions,
    Vec<OsString>,
)> {
    use foveonic::output::{BitDepth, EncodingOptions, Format, TiffCompression};
    let mut format = None;
    let mut options = EncodingOptions::default();
    let mut rest = Vec::new();
    let mut args = args;
    while let Some(arg) = args.next() {
        let flag = arg.to_str().unwrap_or("");
        if flag == "--jxl-lossless" {
            options.jxl_lossless = true;
            continue;
        }
        if flag == "--jxl-lossy" {
            options.jxl_lossless = false;
            continue;
        }
        if !matches!(
            flag,
            "--format"
                | "--jpeg-quality"
                | "--png-depth"
                | "--jxl-quality"
                | "--jxl-effort"
                | "--tiff-compression"
        ) {
            rest.push(arg);
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        let value = value.to_str().ok_or("export option must be text")?;
        match flag {
            "--format" => {
                format = Some(match value {
                    "tif" | "tiff" => Format::Tiff,
                    "jpg" | "jpeg" => Format::Jpeg,
                    "png" => Format::Png,
                    "jxl" => Format::Jxl,
                    _ => return Err("format must be tiff, jpeg, png or jxl".into()),
                });
            }
            "--jpeg-quality" => options.jpeg_quality = value.parse()?,
            "--png-depth" => {
                options.png_depth = match value {
                    "8" => BitDepth::Eight,
                    "16" => BitDepth::Sixteen,
                    _ => return Err("PNG depth must be 8 or 16".into()),
                }
            }
            "--jxl-quality" => options.jxl_quality = value.parse()?,
            "--jxl-effort" => options.jxl_effort = value.parse()?,
            "--tiff-compression" => {
                options.tiff_compression = match value {
                    "none" => TiffCompression::None,
                    "lzw" => TiffCompression::Lzw,
                    "zip" | "deflate" => TiffCompression::Zip,
                    _ => return Err("TIFF compression must be none, lzw or zip".into()),
                }
            }
            _ => unreachable!(),
        }
    }
    Ok((format, options, rest))
}

struct Output<'a> {
    path: &'a Path,
    format: foveonic::output::Format,
    encoding: foveonic::output::EncodingOptions,
}

fn convert(
    engine: &Engine,
    source: &Source,
    input: &Path,
    settings: &Settings,
    output: Output<'_>,
    mode: Mode,
) -> Result<[u32; 2]> {
    let Output {
        path: destination,
        format,
        encoding,
    } = output;
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
    let size = engine.write_image(&image, destination, format, encoding, &mut profile)?;
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

fn configured_engine() -> Result<Engine> {
    let engine = Engine::new()?;
    if let Some(directory) = std::env::var_os("FOVEONIC_CALIBRATION_RESOURCES") {
        engine.with_calibration_resources(directory)
    } else {
        Ok(engine)
    }
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
            let (requested_format, encoding, control_args) = parse_export_options(args)?;
            let format = foveonic::output::Format::from_extension(&destination)?;
            if requested_format.is_some_and(|requested| requested != format) {
                return Err("format does not match output extension".into());
            }
            if command == "render" {
                encoding.validate(format)?;
            } else if format != foveonic::output::Format::Dng {
                return Err("export-dng requires a .dng destination".into());
            }
            let control_args = control_args.into_iter();
            let (_settings, extra, profiling) = parse_settings(control_args.clone())?;
            if !extra.is_empty() {
                return Err(USAGE.into());
            }
            if destination.exists() {
                return Err(format!("output already exists: {}", destination.display()).into());
            }
            let source = Source::open(&path)?;
            let (settings, _, _) =
                options::parse_settings_for_camera(control_args, &source.metadata)?;
            let mut setup = Profile::new(profiling);
            let engine = setup.measure("GPU init", configured_engine)?;
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
                Output {
                    path: &destination,
                    format,
                    encoding,
                },
                profiling,
            )?;
            eprintln!(
                "Rendered {}x{} {} using {}: {}",
                size[0],
                size[1],
                format.label(),
                engine.adapter_name(),
                destination.display()
            );
        }
        Some("batch") => {
            let destination = PathBuf::from(path);
            let (format, encoding, control_args) = parse_export_options(args)?;
            let format = format.unwrap_or(foveonic::output::Format::Tiff);
            encoding.validate(format)?;
            let control_args = control_args.into_iter();
            let (_settings, inputs, profiling) = parse_settings(control_args.clone())?;
            if inputs.is_empty() {
                return Err(USAGE.into());
            }
            let jobs = batch_jobs(&inputs, &destination, format)?;
            let engine = configured_engine()?;
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
                let (settings, _, _) =
                    options::parse_settings_for_camera(control_args.clone(), &source.metadata)?;
                let size = convert(
                    &engine,
                    &source,
                    input,
                    &settings,
                    Output {
                        path: output,
                        format,
                        encoding,
                    },
                    profiling,
                )
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
        assert_eq!(neutral.sharpen_amount, None);
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
        assert_eq!(
            parse("--wb Sunlight --mode Standard --reconstruction")
                .unwrap()
                .0
                .reconstruction,
            Some(Default::default())
        );
        assert_eq!(
            parse(
                "--wb Sunlight --mode Standard --chroma-noise 1 --luma-noise 4 --smooth-crisp 0.75"
            )
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
        for (option, amount) in [("auto", None), ("off", Some(0.)), ("0.75", Some(0.75))] {
            let settings = parse(&format!("--wb Sunlight --mode Standard --sharpen {option}"))
                .unwrap()
                .0;
            assert_eq!(settings.sharpen_amount, amount);
        }
        for (option, destination) in [
            ("camera", None),
            ("srgb", Some(foveonic::render::TiffColorSpace::Srgb)),
            (
                "adobe-rgb",
                Some(foveonic::render::TiffColorSpace::AdobeRgb),
            ),
            (
                "prophoto-rgb",
                Some(foveonic::render::TiffColorSpace::ProPhotoRgb),
            ),
        ] {
            let settings = parse(&format!(
                "--wb Sunlight --mode Standard --tiff-color-space {option}"
            ))
            .unwrap()
            .0;
            assert_eq!(settings.tiff_color_space, destination);
        }
        for invalid in [
            "--sharpen invalid",
            "--tiff-color-space invalid",
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
            "--sharpen NaN",
            "--size 0x400",
            "--chroma-noise 5",
            "--luma-noise 5",
            "--smooth-crisp -1",
            "--smooth-crisp NaN",
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
        let jobs = batch_jobs(
            std::slice::from_ref(&input),
            &out,
            foveonic::output::Format::Tiff,
        )
        .unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].1, out.join("a.tiff"));
        assert!(!out.join("a.tiff").exists());
        assert!(batch_jobs(
            &[input.clone(), input.join("a.x3f")],
            &out,
            foveonic::output::Format::Tiff
        )
        .is_err());
        std::fs::write(input.join("A.other"), b"").unwrap();
        assert!(batch_jobs(
            &[input.clone(), input.join("A.other")],
            &out,
            foveonic::output::Format::Tiff
        )
        .is_err());
        std::fs::write(out.join("a.tiff"), b"keep").unwrap();
        assert!(batch_jobs(&[input], &out, foveonic::output::Format::Tiff).is_err());
        assert_eq!(std::fs::read(out.join("a.tiff")).unwrap(), b"keep");
    }
}

#[cfg(test)]
#[test]
fn color_sensitivity_cli_preserves_the_selected_rendering() {
    let args = [
        "--wb",
        "Sunlight",
        "--mode",
        "Standard",
        "--color-sensitivity",
        "0.2,0.1,0,-0.3,0,0,0.05,0.1",
    ]
    .into_iter()
    .map(OsString::from);
    let (settings, _, _) = parse_settings(args).unwrap();
    assert_eq!(settings.rendering, foveonic::render::Rendering::Color);
    assert_eq!(
        settings.photographic.color_editor,
        foveonic::render::ColorEditorControls::from_lightness([
            0.2,
            (0.2 + 0.) * 0.5,
            0.,
            -0.3,
            0.,
            0.,
            (0. + 0.1) * 0.5,
            0.1
        ])
    );
}

#[cfg(test)]
#[test]
fn encoding_flags_leave_photographic_options_and_inputs_intact() {
    let args = [
        "--format",
        "jxl",
        "--wb",
        "Sunlight",
        "photo.x3f",
        "--jxl-lossy",
        "--jxl-quality",
        "92",
        "--jxl-effort",
        "9",
        "--exposure",
        "-0.5",
    ];
    let (format, encoding, rest) =
        parse_export_options(args.into_iter().map(OsString::from)).unwrap();
    assert_eq!(format, Some(foveonic::output::Format::Jxl));
    assert_eq!(encoding.jxl_quality, 92);
    assert_eq!(encoding.jxl_effort, 9);
    assert!(!encoding.jxl_lossless);
    assert_eq!(
        rest,
        ["--wb", "Sunlight", "photo.x3f", "--exposure", "-0.5"].map(OsString::from)
    );
    encoding.validate(format.unwrap()).unwrap();
    for args in [
        vec!["--format", "avif"],
        vec!["--png-depth", "10"],
        vec!["--jpeg-quality"],
        vec!["--tiff-compression", "jpeg"],
    ] {
        assert!(parse_export_options(args.into_iter().map(OsString::from)).is_err());
    }
}
