//! Compares completed Smart transitions and regional-first feedback in matched sessions.

#[path = "../src/options.rs"]
#[allow(dead_code)]
mod options;

use foveonic::{
    image::{ImageTexture, RenderedImage},
    inspection::Content,
    profile::{GpuActivity, Mode, Profile},
    render::{Engine, PreviewQuality, RenderSession, Settings},
    source::{Result, Source},
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

fn epoch_millis() -> Result<u128> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())
}

#[derive(Serialize)]
struct Completed {
    milliseconds: f64,
    extent: [u32; 2],
    activity: GpuActivity,
    profile: Profile,
}

fn camera_settings(source: &Source) -> Result<Settings> {
    let mut settings = Settings::new("Sunlight", "Standard");
    foveonic::render::ControlCapabilities::from_metadata(&source.metadata)?
        .apply_capture_defaults(&mut settings, &source.metadata)?;
    Ok(settings)
}
fn completed(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
    mode: Mode,
) -> Result<(Completed, ImageTexture)> {
    let (measurement, texture, _) = completed_image(engine, session, settings, mode)?;
    Ok((measurement, texture))
}
fn route(
    engine: &Engine,
    source: Arc<Source>,
    initial_settings: &Settings,
    progressive: bool,
    mode: Mode,
) -> Result<serde_json::Value> {
    let mut settings = initial_settings.clone();
    let mut session = RenderSession::new(source);
    settings.preview_quality = PreviewQuality::Fast;
    let (overview, _overview_texture) = completed(engine, &mut session, &settings, Mode::Off)?;
    let [width, height] = overview.extent.map(|v| v * 2);
    // Match the central 200% viewport in the native landscape smoke, including overscan.
    let tile = [992.min(width), 1002.min(height)];
    settings.preview_quality = PreviewQuality::Full;
    if progressive {
        let x = (width - tile[0]) / 2;
        let y = (height - tile[1]) / 2;
        settings.preview_region = Some([x, y, x + tile[0], y + tile[1]]);
        let (detail, _detail_texture) = completed(engine, &mut session, &settings, mode)?;
        settings.preview_region = None;
        let (full, _full_texture) = completed(engine, &mut session, &settings, mode)?;
        Ok(
            serde_json::json!({"route":"detail-first", "overview":overview, "first_detail":detail, "full_continuation":full, "total_full_ms":detail.milliseconds + full.milliseconds}),
        )
    } else {
        let (full, _full_texture) = completed(engine, &mut session, &settings, mode)?;
        Ok(serde_json::json!({"route":"direct-full", "overview":overview, "full":full}))
    }
}
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let source_path = PathBuf::from(
        args.next()
            .ok_or("usage: profile_smart_preview ORIGINAL.x3f REPORT.json [--region-first [fresh|exposure|noise] | --direct-only | --warm-edits] [--fractions LIST] [--repeats N] [-- RENDER_OPTIONS]")?,
    );
    let destination = PathBuf::from(args.next().ok_or("missing report destination")?);
    if destination.exists() {
        return Err("report destination must be new".into());
    }
    let rest: Vec<_> = args.collect();
    let split = rest
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or(rest.len());
    let (runner, controls) = rest.split_at(split);
    let controls = if split < rest.len() {
        &controls[1..]
    } else {
        controls
    };
    let mut region_mode = false;
    let mut direct_only = false;
    let mut warm_edits = false;
    let mut scenario = "fresh";
    let mut repeats = 5usize;
    let mut fractions = vec![0.25, 0.5, 0.75];
    let mut fractions_set = false;
    let mut runner = runner.iter().peekable();
    while let Some(arg) = runner.next() {
        match arg.to_str() {
            Some("--direct-only") => direct_only = true,
            Some("--warm-edits") => warm_edits = true,
            Some("--region-first") => {
                region_mode = true;
                if let Some(value) = runner.peek().and_then(|v| v.to_str()) {
                    if ["fresh", "exposure", "noise"].contains(&value) {
                        scenario = value;
                        runner.next();
                    }
                }
            }
            Some("--repeats") => {
                repeats = runner
                    .next()
                    .and_then(|v| v.to_str())
                    .ok_or("missing repeats")?
                    .parse()?;
                if repeats == 0 {
                    return Err("repeats must be positive".into());
                }
            }
            Some("--fractions") => {
                fractions_set = true;
                fractions = runner
                    .next()
                    .and_then(|v| v.to_str())
                    .ok_or("missing fractions")?
                    .split(',')
                    .map(str::parse)
                    .collect::<std::result::Result<Vec<f64>, _>>()?;
                if fractions.is_empty()
                    || fractions
                        .iter()
                        .any(|v| !v.is_finite() || *v <= 0. || *v > 1.)
                {
                    return Err("fractions must be within (0, 1]".into());
                }
            }
            _ => return Err("invalid Smart profiler arguments".into()),
        }
    }
    if fractions_set && !region_mode {
        return Err("fractions require region-first".into());
    }
    if direct_only && region_mode {
        return Err("direct-only cannot be combined with region-first".into());
    }
    if warm_edits && (direct_only || region_mode || fractions_set) {
        return Err("warm-edits cannot be combined with other routes or fractions".into());
    }
    let source = Arc::new(Source::open(&source_path)?);
    let settings = if controls.is_empty() {
        camera_settings(&source)?
    } else {
        let initial = camera_settings(&source)?;
        let noise = foveonic::render::ControlCapabilities::from_metadata(&source.metadata)?
            .default_noise_controls_for_metadata(&source.metadata)?;
        let (settings, paths, mode) =
            options::parse_settings_with_noise_defaults(controls.iter().cloned(), noise, initial)?;
        if !paths.is_empty() || mode != Mode::Off {
            return Err("Smart controls must not include paths or a profile mode".into());
        }
        settings
    };
    if !direct_only && settings.output_size.is_some() {
        return Err("regional comparisons require native output size".into());
    }
    if direct_only && !settings.fast_preview_available(&source.metadata) {
        return Err("this camera recipe does not support a Fast overview".into());
    }
    if !direct_only && !settings.regional_preview_available(&source.metadata) {
        return Err("this camera recipe does not support regional Smart preview".into());
    }
    let engine = Engine::new()?;
    if warm_edits {
        return warm_edit_benchmark(&engine, source, source_path, destination, settings, repeats);
    }
    if region_mode {
        return region_benchmark(
            &engine,
            source,
            source_path,
            destination,
            scenario,
            &settings,
            repeats,
            &fractions,
        );
    }
    let mut runs = Vec::new();
    // Discard one warmup pair. Alternate route order to reduce ordering bias.
    for round in 0..=repeats {
        let routes: &[bool] = if direct_only {
            &[false]
        } else if round % 2 == 0 {
            &[false, true]
        } else {
            &[true, false]
        };
        for &progressive in routes {
            let start_epoch_ms = epoch_millis()?;
            let result = route(&engine, source.clone(), &settings, progressive, Mode::Off)?;
            let end_epoch_ms = epoch_millis()?;
            eprintln!("round {round}: {}", result);
            runs.push(serde_json::json!({"warmup":round == 0, "round":round, "start_epoch_ms":start_epoch_ms, "end_epoch_ms":end_epoch_ms, "result":result}));
        }
    }
    let mut diagnostics = vec![route(&engine, source.clone(), &settings, false, Mode::Gpu)?];
    if !direct_only {
        diagnostics.push(route(&engine, source, &settings, true, Mode::Gpu)?);
    }
    let report = serde_json::json!({"source":source_path, "adapter":engine.adapter_name(), "settings":settings, "timing":if direct_only { "completed render and view preparation, source decode excluded, one warmup trial" } else { "completed render and view preparation, source decode excluded, alternating matched-session trials, one warmup pair" }, "timestamp_scope":"whole route, including prior overview and any verification", "runs":runs, "gpu_diagnostics":diagnostics});
    std::fs::write(destination, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}

fn warm_edit_benchmark(
    engine: &Engine,
    source: Arc<Source>,
    source_path: PathBuf,
    destination: PathBuf,
    mut settings: Settings,
    repeats: usize,
) -> Result<()> {
    let initial = settings.clone();
    let mut session = RenderSession::new(source);
    settings.preview_quality = PreviewQuality::Fast;
    let overview = region_completed(engine, &mut session, &settings)?;
    let extent = overview.0.extent.map(|v| v * 2);
    let size = [992.min(extent[0]), 1002.min(extent[1])];
    let origin = [(extent[0] - size[0]) / 2, (extent[1] - size[1]) / 2];
    let region = Some([
        origin[0],
        origin[1],
        origin[0] + size[0],
        origin[1] + size[1],
    ]);
    settings.preview_quality = PreviewQuality::Full;
    settings.preview_region = region;
    let mut detail = region_completed(engine, &mut session, &settings)?;
    settings.preview_region = None;
    let mut full = region_completed(engine, &mut session, &settings)?;
    let mut runs = Vec::new();
    for round in 0..repeats {
        settings.exposure = initial.exposure + (round + 1) as f32 * 0.1;
        let started = Instant::now();
        settings.preview_region = region;
        detail = region_completed(engine, &mut session, &settings)?;
        settings.preview_region = None;
        full = region_completed(engine, &mut session, &settings)?;
        let total_ms = started.elapsed().as_secs_f64() * 1000.;
        eprintln!(
            "warm edit {round}: slice {:.1} ms, Full {:.1} ms, total {total_ms:.1} ms",
            detail.0.milliseconds, full.0.milliseconds
        );
        runs.push(serde_json::json!({"round":round, "exposure":settings.exposure, "slice":detail.0, "full":full.0, "total_ms":total_ms, "cached_bytes":session.cached_bytes()}));
    }
    settings.exposure += 0.1;
    settings.preview_region = region;
    let diagnostic_slice = completed(engine, &mut session, &settings, Mode::Wall)?;
    settings.preview_region = None;
    let diagnostic_full = completed(engine, &mut session, &settings, Mode::Wall)?;
    // Retain both displayed artifacts through the complete handoff.
    drop((overview, detail, full));
    let report = serde_json::json!({"source":source_path, "adapter":engine.adapter_name(), "settings":initial, "region":region, "timing":"completed warm slice and Full continuation with view preparation, source decode and initial grid priming excluded, previous displayed artifacts retained, no scopes or physical presentation", "runs":runs, "wall_diagnostics":{"slice":diagnostic_slice.0,"full":diagnostic_full.0}});
    std::fs::write(destination, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}

// The image and texture stay alive through continuation, matching foreground ownership.
fn region_completed(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
) -> Result<(Completed, ImageTexture, RenderedImage)> {
    completed_image(engine, session, settings, Mode::Off)
}

fn completed_image(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
    mode: Mode,
) -> Result<(Completed, ImageTexture, RenderedImage)> {
    let started = Instant::now();
    let before = engine.gpu_activity();
    let mut profile = Profile::new(mode);
    let mut artifacts = engine
        .render_session(session, settings, &["final"], || false, &mut profile)?
        .ok_or("render superseded")?;
    let Content::Image(image) = artifacts.pop().ok_or("missing image")?.content else {
        return Err("image expected".into());
    };
    let texture = engine.view_texture(&image, None, &mut profile)?;
    engine.device().poll(wgpu::Maintain::Wait);
    Ok((
        Completed {
            milliseconds: started.elapsed().as_secs_f64() * 1000.,
            extent: texture.description().extent,
            activity: engine.gpu_activity().since(before),
            profile,
        },
        texture,
        *image,
    ))
}

#[allow(clippy::too_many_arguments)]
fn region_route(
    engine: &Engine,
    source: Arc<Source>,
    fraction: f64,
    extent: [u32; 2],
    region_first: bool,
    verify: bool,
    full_baseline: &[u8],
    scenario: &str,
    initial_settings: &Settings,
) -> Result<serde_json::Value> {
    let mut settings = initial_settings.clone();
    let size = extent.map(|v| ((f64::from(v) * fraction).round() as u32).max(1));
    let origin = [(extent[0] - size[0]) / 2, (extent[1] - size[1]) / 2];
    let rect = [
        origin[0],
        origin[1],
        origin[0] + size[0],
        origin[1] + size[1],
    ];
    let mut session = RenderSession::new(source);
    let _previous = if scenario == "fresh" {
        None
    } else {
        Some(region_completed(engine, &mut session, &settings)?)
    };
    apply_edit(&mut settings, scenario);
    settings.preview_quality = if region_first {
        PreviewQuality::Full
    } else {
        PreviewQuality::Fast
    };
    settings.preview_region = region_first.then_some(rect);
    let (first, _first_texture, first_image) = region_completed(engine, &mut session, &settings)?;
    settings.preview_quality = PreviewQuality::Full;
    settings.preview_region = Some(rect);
    let detail = if region_first {
        None
    } else {
        Some(region_completed(engine, &mut session, &settings)?)
    };
    settings.preview_region = None;
    let (full, _full_texture, full_image) = region_completed(engine, &mut session, &settings)?;
    if verify {
        if engine.readback_image(&full_image)? != full_baseline {
            return Err("Full continuation differs from independent Full".into());
        }
        let detail_image = detail.as_ref().map_or(&first_image, |(_, _, image)| image);
        verify_region(engine, detail_image, &full_image, rect)?;
    }
    let detail_ms = detail
        .as_ref()
        .map_or(0., |(completed, _, _)| completed.milliseconds);
    let total_full_ms = first.milliseconds + detail_ms + full.milliseconds;
    let first_full_detail_ms = first.milliseconds + detail_ms;
    Ok(
        serde_json::json!({"route": if region_first {"region-first"} else {"fast-first"}, "viewport_fraction_per_axis":fraction, "rect":rect, "first_full_detail_ms":first_full_detail_ms, "first":first, "detail":detail.as_ref().map(|(completed, _, _)| completed), "full_continuation":full, "total_full_ms":total_full_ms, "exact_checked":verify}),
    )
}

fn verify_region(
    engine: &Engine,
    region: &RenderedImage,
    full: &RenderedImage,
    rect: [u32; 4],
) -> Result<()> {
    let cv = region.view();
    let fv = full.uncropped_view();
    let [cl, ct, cr, cb] = cv.crop;
    let [l, t, r, b] = fv.crop;
    let [x0, y0, x1, y1] = rect;
    // The same oriented-to-physical mapping used by the crop and regional recipes.
    let [fl, ft, fr, fb] = match fv.rotation {
        0 => [l + x0, t + y0, l + x1, t + y1],
        90 => [l + y0, b - x1, l + y1, b - x0],
        180 => [r - x1, b - y1, r - x0, b - y0],
        270 => [r - y1, t + x0, r - y0, t + x1],
        _ => return Err("unexpected rotation".into()),
    };
    if cv.rotation != fv.rotation
        || cv.extent != [x1 - x0, y1 - y0]
        || [cr - cl, cb - ct] != [fr - fl, fb - ft]
    {
        return Err(format!("region geometry mismatch: {cv:?}, {fv:?}, rect {rect:?}").into());
    }
    let c = engine.readback_image(region)?;
    let f = engine.readback_image(full)?;
    let cw = region.description().extent[0] as usize;
    let fw = full.description().extent[0] as usize;
    for y in 0..(cb - ct) as usize {
        let ci = ((ct as usize + y) * cw + cl as usize) * 12;
        let fi = ((ft as usize + y) * fw + fl as usize) * 12;
        let n = (cr - cl) as usize * 12;
        if c[ci..ci + n] != f[fi..fi + n] {
            return Err(format!("region differs from Full at physical row {y}").into());
        }
    }
    Ok(())
}

fn apply_edit(settings: &mut Settings, scenario: &str) {
    match scenario {
        "exposure" => settings.exposure += 0.5,
        "noise" => {
            settings.reconstruction = if settings.reconstruction.is_some() {
                None
            } else {
                Some(Default::default())
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn region_benchmark(
    engine: &Engine,
    source: Arc<Source>,
    source_path: PathBuf,
    destination: PathBuf,
    scenario: &str,
    initial_settings: &Settings,
    repeats: usize,
    fractions: &[f64],
) -> Result<()> {
    let mut settings = initial_settings.clone();
    apply_edit(&mut settings, scenario);
    settings.preview_quality = PreviewQuality::Full;
    let mut session = RenderSession::new(source.clone());
    let (_, _, whole) = region_completed(engine, &mut session, &settings)?;
    let extent = whole.uncropped_view().extent;
    let full_baseline = engine.readback_image(&whole)?;
    drop((whole, session));
    let mut runs = Vec::new();
    for &fraction in fractions {
        for round in 0..=repeats {
            for region_first in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let start_epoch_ms = epoch_millis()?;
                let result = region_route(
                    engine,
                    source.clone(),
                    fraction,
                    extent,
                    region_first,
                    round == 0,
                    &full_baseline,
                    scenario,
                    initial_settings,
                )?;
                let end_epoch_ms = epoch_millis()?;
                eprintln!("viewport fraction {fraction}, round {round}: {result}");
                runs.push(serde_json::json!({"warmup":round == 0, "round":round, "start_epoch_ms":start_epoch_ms, "end_epoch_ms":end_epoch_ms, "result":result}));
            }
        }
    }
    let report = serde_json::json!({"source":source_path, "adapter":engine.adapter_name(), "extent":extent, "scenario":scenario, "initial_settings":initial_settings, "requested_settings":settings, "output":"final", "timing":"completed render and view preparation, source decode and previous image excluded, alternating pairs per viewport, one warmup pair; off mode", "timestamp_scope":"whole route, including prior image and any verification", "runs":runs});
    std::fs::write(destination, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
