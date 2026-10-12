//! Measures completed cold/warm all-enabled processing, transfers, export, and optional bit-exact goldens.

use foveonic::{
    image::RenderedImage,
    profile::Profile,
    render::{Engine, Settings},
    source::Source,
};
use std::{hash::Hasher, path::PathBuf, time::Instant};

// Diagnostic readback is deliberately outside both the measured render and engine activity.
fn pixels(
    engine: &Engine,
    image: &RenderedImage,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let staging = engine.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("Equivalence readback"),
        size: image.buffer().size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = engine.device().create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(image.buffer(), 0, &staging, 0, staging.size());
    engine.queue().submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    engine.device().poll(wgpu::Maintain::Wait);
    rx.recv()??;
    let bytes = staging.slice(..).get_mapped_range().to_vec();
    staging.unmap();
    Ok(bytes)
}
fn milliseconds(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.
}
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        return Err(
            "usage: profile_pipeline INPUT.x3f NEW_EXPORT_DIR [GOLDEN_DIR] [warm-count, default 2]"
                .into(),
        );
    }
    let input = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&output)?;
    let stem = input
        .file_stem()
        .ok_or("missing input stem")?
        .to_string_lossy();
    let start = Instant::now();
    let source = Source::open(&input)?;
    let decode = milliseconds(start);
    let start = Instant::now();
    let engine = Engine::new()?;
    engine.device().poll(wgpu::Maintain::Wait);
    let setup = milliseconds(start);
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    settings.sensor_gain = 1.3;
    settings.exposure = 0.5;
    settings.tone.contrast = 0.25;
    settings.tone.shadows = 0.1;
    settings.tone.highlights = -0.2;
    settings.saturation = 1.2;
    settings.sharpen_amount = 0.75;
    // Also exercise encoding/resizing without repeatedly writing full-frame TIFFs.
    settings.output_size = Some(if source.metadata.rotation % 180 == 90 {
        [800, 1200]
    } else {
        [1200, 800]
    });
    println!(
        "{}",
        serde_json::json!({"event":"setup","input":input,"adapter":engine.adapter_name(),"decode_ms":decode,"engine_ms":setup,"settings":format!("{settings:?}")})
    );
    let warm: usize = args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(2);
    for iteration in 0..warm + 2 {
        let timestamped = iteration == warm + 1;
        let mut profile = Profile::new(timestamped);
        let before = engine.gpu_activity();
        let start = Instant::now();
        let image = engine.render(&source, &settings, &mut profile)?;
        let submission = milliseconds(start);
        engine.device().poll(wgpu::Maintain::Wait);
        let complete = milliseconds(start);
        let render_activity = engine.gpu_activity().since(before);
        let export_path = output.join(format!("{stem}-{iteration}.tiff"));
        let before = engine.gpu_activity();
        let start = Instant::now();
        engine.write_tiff(&image, &export_path, &mut profile)?;
        engine.device().poll(wgpu::Maintain::Wait);
        let export = milliseconds(start);
        let export_activity = engine.gpu_activity().since(before);
        let mut digest = None;
        if iteration == 0 {
            let bytes = pixels(&engine, &image)?;
            let mut hasher = std::hash::DefaultHasher::new();
            hasher.write(&bytes);
            digest = Some(format!("{:016x}", hasher.finish()));
            if let Some(dir) = args.get(3).filter(|d| d.as_str() != "-") {
                let dir = PathBuf::from(dir);
                std::fs::create_dir_all(&dir)?;
                let golden = dir.join(format!("{stem}.f32"));
                if golden.exists() {
                    let expected = std::fs::read(golden)?;
                    if bytes != expected {
                        return Err(format!("non-equivalent output: {stem}").into());
                    }
                } else {
                    std::fs::write(golden, bytes)?;
                }
            }
        }
        println!(
            "{}",
            serde_json::json!({"event":"render","iteration":iteration,"timestamped":timestamped,"submission_ms":submission,"completed_ms":complete,"export_ms":export,"render_activity":render_activity,"export_activity":export_activity,"digest":digest,"profile":profile})
        );
    }
    Ok(())
}
