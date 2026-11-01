//! Compares completed Merrill fresh and staged processing, with optional settled-memory sampling boundaries.

use foveonic::{
    inspection::Content,
    profile::{report, Mode, Profile},
    render::{Engine, RenderSession, Settings},
    source::Source,
};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(serde::Deserialize, serde::Serialize)]
struct Case {
    label: String,
    source: PathBuf,
    settings: Settings,
}

fn record(
    log: &mut fs::File,
    event: &str,
    mode: Mode,
    data: serde_json::Value,
) -> Result<(), Error> {
    writeln!(log, "{}", report(event, mode, data))?;
    log.flush()?;
    Ok(())
}

fn main() -> Result<(), Error> {
    let args: Vec<_> = std::env::args().collect();
    if !(7..=8).contains(&args.len()) {
        return Err("usage: profile_noise_resources CASES.json async|staged|switch off|wall|gpu REPEATS HOLD_MS OUTPUT.jsonl [IMAGE_OUTPUT]".into());
    }
    let cases: Vec<Case> = serde_json::from_slice(&fs::read(&args[1])?)?;
    let route = args[2].as_str();
    if !["async", "staged", "switch"].contains(&route) {
        return Err("unknown route".into());
    }
    let mode: Mode = args[3].parse()?;
    let repeats: usize = args[4].parse()?;
    if cases.is_empty() || repeats == 0 {
        return Err("at least one case and one repetition are required".into());
    }
    let hold = Duration::from_millis(args[5].parse()?);
    let output = args.get(7).map_or("final", String::as_str);
    let mut log = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[6])?;
    let engine = Engine::new()?;
    let sources: Vec<_> = cases
        .iter()
        .map(|case| Source::open(&case.source).map(Arc::new))
        .collect::<Result<_, _>>()?;
    record(
        &mut log,
        "ready",
        mode,
        serde_json::json!({"pid":std::process::id(),"adapter":engine.adapter_name(),"route":route,"cases":cases,"output":output,"hold_ms":hold.as_millis()}),
    )?;
    std::thread::sleep(hold);
    for iteration in 0..repeats {
        let index = if route == "switch" {
            iteration % cases.len()
        } else {
            0
        };
        let source = &sources[index];
        let settings = &cases[index].settings;
        let mut profile = Profile::new(mode);
        let before = engine.gpu_activity();
        let start = Instant::now();
        let (mut artifacts, session) = if route == "async" {
            (
                engine.render_artifacts(source, settings, &[output], &mut profile)?,
                None,
            )
        } else {
            // A fresh session measures all work, unlike an unchanged edit.
            let mut session = RenderSession::new(source.clone());
            let artifacts = engine
                .render_session(&mut session, settings, &[output], || false, &mut profile)?
                .ok_or("cancelled")?;
            (artifacts, Some(session))
        };
        let Content::Image(retained) = artifacts.remove(0).content else {
            return Err("expected image output".into());
        };
        engine.device().poll(wgpu::Maintain::Wait);
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
        let cached_bytes = session.as_ref().map_or(0, RenderSession::cached_bytes);
        record(
            &mut log,
            "completed",
            mode,
            serde_json::json!({"iteration":iteration,"case":cases[index].label,"route":route,"output":output,"completed_ms":elapsed_ms,"cache_bytes":cached_bytes,"activity":engine.gpu_activity().since(before),"profile":profile}),
        )?;
        std::thread::sleep(hold);
        drop((retained, session));
        engine.device().poll(wgpu::Maintain::Wait);
        record(
            &mut log,
            "released",
            mode,
            serde_json::json!({"iteration":iteration}),
        )?;
        std::thread::sleep(hold);
    }
    drop(sources);
    engine.device().poll(wgpu::Maintain::Wait);
    record(&mut log, "sources_released", mode, serde_json::json!({}))?;
    std::thread::sleep(hold);
    drop(engine);
    record(&mut log, "engine_released", mode, serde_json::json!({}))?;
    std::thread::sleep(hold);
    Ok(())
}
