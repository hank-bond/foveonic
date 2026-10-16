//! Captures an isolated native GUI session and atomically refreshes its lossless WebP README asset.

use crate::examples;
use anyhow::{ensure, Context, Result};
use image::codecs::webp::WebPEncoder;
use std::{
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

const ASSETS: &str = "crates/foveonic-gui/assets";

pub fn source(root: &Path, sources: &Path) -> Result<std::path::PathBuf> {
    let recipe: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(ASSETS).join("gui-screenshot.json"))?)?;
    let name = recipe["source"]
        .as_str()
        .context("Screenshot recipe has no source filename")?;
    ensure!(
        examples::filename(name),
        "Unsafe screenshot source filename: {name}"
    );
    let source = sources.join(name);
    ensure!(
        source.is_file(),
        "Missing screenshot source {}. Run cargo xtask examples sync",
        source.display()
    );
    Ok(fs::canonicalize(source)?)
}

fn command(binary: &Path, source: &Path, directory: &Path) -> Command {
    let mut command = Command::new(binary);
    command.arg(source);
    // Diagnostic modes and old smoke flags must not change a documentation capture.
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"FOVEONIC_") {
            command.env_remove(key);
        }
    }
    command
        .env("FOVEONIC_GUI_SMOKE_DIR", directory)
        .env("FOVEONIC_GUI_SMOKE_SCREENSHOT", "1");
    command
}

pub fn capture(root: &Path, binary: &Path, source: &Path) -> Result<()> {
    let directory = tempfile::Builder::new()
        .prefix("foveonic-screenshot-")
        .tempdir()?;
    let mut child = command(binary, source, directory.path())
        .spawn()
        .context("Cannot start the screenshot app. A native graphical/GPU session is required")?;
    let began = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "Screenshot app failed: {status}");
            break;
        }
        if began.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Screenshot timed out. Check the native graphical/GPU session. Previous screenshot is unchanged");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let png = directory.path().join("readme.png");
    ensure!(
        png.is_file(),
        "App exited without a completed README frame. Previous screenshot is unchanged"
    );
    let target = root.join(ASSETS).join("gui-screenshot.webp");
    encode(&png, &target)?;
    println!("Updated {}", target.display());
    Ok(())
}

fn encode(png: &Path, target: &Path) -> Result<()> {
    let image = image::open(png)?.into_rgba8();
    let mut staged =
        tempfile::NamedTempFile::new_in(target.parent().context("Screenshot path has no parent")?)?;
    WebPEncoder::new_lossless(staged.as_file_mut()).encode(
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )?;
    let decoded =
        image::load_from_memory_with_format(&fs::read(staged.path())?, image::ImageFormat::WebP)?
            .into_rgba8();
    ensure!(
        image == decoded,
        "WebP round trip did not preserve screenshot pixels"
    );
    staged.persist(target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webp_is_lossless_and_failed_input_keeps_previous_asset() {
        let directory = tempfile::tempdir().unwrap();
        let png = directory.path().join("capture.png");
        let target = directory.path().join("screenshot.webp");
        let pixels = image::RgbaImage::from_fn(64, 32, |x, y| {
            image::Rgba([(x * 4) as u8, (y * 8) as u8, ((x + y) * 2) as u8, 255])
        });
        pixels.save(&png).unwrap();
        encode(&png, &target).unwrap();
        assert_eq!(image::open(&target).unwrap().into_rgba8(), pixels);
        let previous = fs::read(&target).unwrap();
        fs::write(&png, b"invalid capture").unwrap();
        assert!(encode(&png, &target).is_err());
        assert_eq!(fs::read(&target).unwrap(), previous);
    }

    #[test]
    fn capture_is_explicitly_isolated_from_inherited_diagnostics() {
        let command = command(
            Path::new("app"),
            Path::new("source.x3f"),
            Path::new("isolated"),
        );
        let env: std::collections::HashMap<_, _> = command.get_envs().collect();
        assert_eq!(
            env[std::ffi::OsStr::new("FOVEONIC_GUI_SMOKE_DIR")],
            Some(std::ffi::OsStr::new("isolated"))
        );
        assert_eq!(
            env[std::ffi::OsStr::new("FOVEONIC_GUI_SMOKE_SCREENSHOT")],
            Some(std::ffi::OsStr::new("1"))
        );
        for (key, _) in
            std::env::vars_os().filter(|(key, _)| key.as_encoded_bytes().starts_with(b"FOVEONIC_"))
        {
            if key != "FOVEONIC_GUI_SMOKE_DIR" && key != "FOVEONIC_GUI_SMOKE_SCREENSHOT" {
                assert_eq!(env[key.as_os_str()], None);
            }
        }
    }
}
