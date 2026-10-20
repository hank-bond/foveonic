//! Renders saved-sidecar highlight comparisons and measures completed fresh recipes without changing edits.
use foveonic::{
    profile::{Mode, Profile},
    render::{ColorFieldApproach, Engine, HighlightMode, Settings},
    sidecar,
    source::{Result, Source},
};
use std::{io::Write, path::PathBuf, time::Instant};

fn variants(base: &Settings) -> Vec<(&'static str, Settings)> {
    let mut stock = base.clone();
    stock.highlight_mode = HighlightMode::Classic;
    let mut auto = stock.clone();
    auto.highlight_threshold_ev = None;
    let mut field = base.clone();
    field.highlight_mode = HighlightMode::ColorField;
    field.color_field.approach = ColorFieldApproach::Faithful;
    let mut local = field.clone();
    local.color_field.approach = ColorFieldApproach::LocalGradient;
    vec![
        ("stock-saved", stock),
        ("stock-auto", auto),
        ("color-field", field),
        ("local-gradient", local),
    ]
}
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let corpus=PathBuf::from(args.next().ok_or("compare_color_field CORPUS NEW_REPORT_DIR [--warm N] [--scenes stem,...] [--variants name,...] [--gpu] [--no-export] [--previews PYTHON]")?);
    let output = PathBuf::from(args.next().ok_or("missing output directory")?);
    if output.exists() {
        return Err("comparison directory must be new".into());
    }
    let mut warm = 2usize;
    let mut scenes = None;
    let mut selected = None;
    let mut diagnostic = false;
    let mut no_export = false;
    let mut preview_python = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--warm" => warm = args.next().ok_or("missing warm count")?.parse()?,
            "--scenes" => {
                scenes = Some(
                    args.next()
                        .ok_or("missing scenes")?
                        .split(',')
                        .map(String::from)
                        .collect::<Vec<_>>(),
                )
            }
            "--variants" => {
                selected = Some(
                    args.next()
                        .ok_or("missing variants")?
                        .split(',')
                        .map(String::from)
                        .collect::<Vec<_>>(),
                )
            }
            "--gpu" => diagnostic = true,
            "--no-export" => no_export = true,
            "--previews" => {
                preview_python = Some(PathBuf::from(
                    args.next().ok_or("missing preview Python interpreter")?,
                ))
            }
            _ => return Err(format!("unknown argument {arg}").into()),
        }
    }
    if let Some(selected) = &selected {
        let supported = variants(&Settings::new("Sunlight", "Standard"));
        for name in selected {
            if !supported.iter().any(|(value, _)| value == name) {
                return Err(format!("unsupported comparison variant {name}").into());
            }
        }
    }
    let start = Instant::now();
    let engine = Engine::new()?;
    let init_ms = start.elapsed().as_secs_f64() * 1000.;
    std::fs::create_dir_all(&output)?;
    let mut log = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("measurements.jsonl"))?;
    writeln!(
        log,
        "{}",
        serde_json::json!({"event":"session","adapter":engine.adapter_name(),"engine_init_ms":init_ms,"warm_repetitions":warm,"no_export":no_export,"previews":preview_python.is_some(),"work_domain":"full sensor raster before active-area crop","note":"Fresh full pipeline per iteration. Completed waits included. First recipe runs share one initialized engine. GPU diagnostics are separate synchronized samples."})
    )?;
    let mut inputs = std::fs::read_dir(&corpus)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    inputs.sort();
    for input in inputs {
        if !input
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("x3f"))
        {
            continue;
        }
        let stem = input.file_stem().unwrap().to_string_lossy().to_string();
        if scenes.as_ref().is_some_and(|s| !s.contains(&stem)) {
            continue;
        }
        let source = Source::open(&input)?;
        let saved = sidecar::load(&input)?;
        let sidecar_used = saved.is_some();
        let base = saved.unwrap_or_else(|| {
            let mut s = Settings::new("Sunlight", "Standard");
            s.exposure = -1.5;
            s
        });
        let directory = output.join(&stem);
        std::fs::create_dir(&directory)?;
        std::fs::write(
            directory.join("base-settings.json"),
            serde_json::to_vec_pretty(&base)?,
        )?;
        for (name, settings) in variants(&base) {
            if selected
                .as_ref()
                .is_some_and(|s| !s.iter().any(|v| v == name))
            {
                continue;
            }
            eprintln!("{stem} {name}");
            let mut failed = false;
            for iteration in 0..warm + 1 + usize::from(diagnostic) {
                let gpu = diagnostic && iteration == warm + 1;
                let mut profile = Profile::new(if gpu { Mode::Gpu } else { Mode::Wall });
                let before = engine.gpu_activity();
                let start = Instant::now();
                let image = match engine.render(&source, &settings, &mut profile) {
                    Ok(v) => v,
                    Err(error) => {
                        let record = serde_json::json!({"event":"error","scene":stem,"variant":name,"iteration":iteration,"settings":settings,"error":error.to_string()});
                        writeln!(log, "{record}")?;
                        log.flush()?;
                        eprintln!("FAILED {record}");
                        failed = true;
                        break;
                    }
                };
                let submitted_ms = start.elapsed().as_secs_f64() * 1000.;
                engine.device().poll(wgpu::Maintain::Wait);
                let completed_ms = start.elapsed().as_secs_f64() * 1000.;
                let record = serde_json::json!({"event":"render","scene":stem,"variant":name,"sidecar_used":sidecar_used,"iteration":iteration,"first":iteration==0,"mode":if gpu{"gpu"}else{"wall"},"submitted_ms":submitted_ms,"completed_ms":completed_ms,"settings":settings,"profile":profile,"activity":engine.gpu_activity().since(before)});
                writeln!(log, "{record}")?;
                log.flush()?;
                if iteration == 0 && !no_export {
                    let start = Instant::now();
                    let mut export = Profile::new(Mode::Wall);
                    let preview_only = preview_python.is_some();
                    let destination = directory.join(format!(
                        "{}{name}.tiff",
                        if preview_only { ".staging-" } else { "" }
                    ));
                    let dimensions = engine.write_tiff(&image, &destination, &mut export)?;
                    let completed_ms = start.elapsed().as_secs_f64() * 1000.;
                    let previews = if preview_only {
                        let result = std::process::Command::new(preview_python.as_ref().unwrap())
                            .arg(
                                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                                    .join("../../scripts/color_field_report.py"),
                            )
                            .arg("--stage")
                            .arg(&destination)
                            .arg("--scene")
                            .arg(&stem)
                            .arg("--variant")
                            .arg(name)
                            .output()?;
                        if !result.status.success() {
                            return Err(format!(
                                "comparison preview failed: {}",
                                String::from_utf8_lossy(&result.stderr)
                            )
                            .into());
                        }
                        let assets = serde_json::from_slice::<serde_json::Value>(&result.stdout)?;
                        if assets["dimensions"] != serde_json::to_value(dimensions)? {
                            return Err("preview dimensions differ from native export".into());
                        }
                        assets
                    } else {
                        serde_json::Value::Null
                    };
                    writeln!(
                        log,
                        "{}",
                        serde_json::json!({"event":"export","scene":stem,"variant":name,"dimensions":dimensions,"completed_ms":completed_ms,"profile":export,"native_retained":!preview_only,"preview_assets":previews})
                    )?;
                    log.flush()?;
                }
            }
            if !failed {
                eprintln!("finished {stem} {name}");
            }
        }
    }
    Ok(())
}
