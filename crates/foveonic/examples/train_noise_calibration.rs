//! Fits a user cap calibration from a JSON array of X3F paths using the app's Rust/GPU trainer.

use foveonic::{noise_calibration::Capture, render::Engine};
fn main() -> foveonic::source::Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        return Err("usage: train_noise_calibration CAP_PATHS.json OUTPUT_PROFILE.json".into());
    }
    let paths: Vec<std::path::PathBuf> = serde_json::from_reader(std::fs::File::open(&args[1])?)?;
    let captures = paths
        .iter()
        .map(Capture::read)
        .collect::<Result<Vec<_>, _>>()?;
    let engine = Engine::new()?;
    let profile = engine.train_noise_calibration(
        &captures,
        |p| {
            println!(
                "{} ISO{} {}us {} / {}",
                p.condition.camera_model,
                p.condition.iso,
                p.condition.shutter_micros,
                p.completed,
                p.total
            )
        },
        || false,
    )?;
    profile.save(&args[2])?;
    println!(
        "Saved {} captures and {} camera-specific defect maps",
        profile.captures().len(),
        profile.defects().len()
    );
    Ok(())
}
