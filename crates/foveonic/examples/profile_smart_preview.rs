//! Compares completed Quattro detail-first and direct Full latency from matched Fast sessions.

use foveonic::{
    image::ImageTexture,
    inspection::Content,
    profile::{GpuActivity, Mode, Profile},
    render::{Engine, PreviewQuality, RenderSession, Settings},
    source::{Result, Source},
};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc, time::Instant};

#[derive(Serialize)]
struct Completed {
    milliseconds: f64,
    extent: [u32; 2],
    activity: GpuActivity,
    profile: Profile,
}
fn completed(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
    mode: Mode,
) -> Result<(Completed, ImageTexture)> {
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
    ))
}
fn route(
    engine: &Engine,
    source: Arc<Source>,
    progressive: bool,
    mode: Mode,
) -> Result<serde_json::Value> {
    let mut session = RenderSession::new(source);
    let mut settings = Settings::new("Sunlight", "Standard");
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
            .ok_or("usage: profile_smart_preview ORIGINAL.x3f REPORT.json")?,
    );
    let destination = PathBuf::from(args.next().ok_or("missing report destination")?);
    if destination.exists() {
        return Err("report destination must be new".into());
    }
    let source = Arc::new(Source::open(&source_path)?);
    let engine = Engine::new()?;
    let mut runs = Vec::new();
    // Discard one warmup pair. Alternate route order to reduce ordering bias.
    for round in 0..6 {
        for progressive in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let result = route(&engine, source.clone(), progressive, Mode::Off)?;
            eprintln!("round {round}: {}", result);
            runs.push(serde_json::json!({"warmup":round == 0, "round":round, "result":result}));
        }
    }
    let diagnostics = [
        route(&engine, source.clone(), false, Mode::Gpu)?,
        route(&engine, source, true, Mode::Gpu)?,
    ];
    let report = serde_json::json!({"source":source_path, "adapter":engine.adapter_name(), "settings":Settings::new("Sunlight", "Standard"), "timing":"completed render and view preparation, source decode excluded, five warm matched-session trials", "runs":runs, "gpu_diagnostics":diagnostics});
    std::fs::write(destination, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
