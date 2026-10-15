//! Starts DNG exports from the original photo so display edits never become baked samples.

pub(crate) mod appearance;
mod writer;

use crate::{
    image::RenderedImage,
    profile::Profile,
    render::Engine,
    render::Settings,
    source::{Result, Source},
};

/// Keep technical corrections in the samples and the selected look in metadata.
///
/// Start from the original source so an earlier display render cannot bake in tone
/// or color edits. WB-dependent calibration, repair, highlight recovery, noise and
/// detail, ColorDQ, and sharpening stay baked. Mode, exposure, tone, and saturation
/// stay separate so the exported samples retain editing room.
///
/// The result is still linear working RGB, not stored DNG camera samples. Its
/// settings retain the whole requested recipe, including the later appearance.
pub fn prepare_base(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    profile: &mut Profile,
) -> Result<RenderedImage> {
    settings.validate()?;
    // Reject an unknown look before spending time on the technical render.
    profile.measure("prepare DNG presentation mode", || {
        crate::nodes::color::color_mode::prepare(&source.metadata, &settings.color_mode)
    })?;
    engine.render_dng_base(source, settings, profile)
}

/// Keep the original X3F as the source of an export, rather than a displayed image.
/// A failed export leaves any existing destination intact. The caller chooses
/// whether to load a sidecar and supplies the resulting settings.
pub fn write_file(
    engine: &Engine,
    input: &std::path::Path,
    settings: &Settings,
    destination: &std::path::Path,
    profile: &mut Profile,
) -> Result<()> {
    let source = profile.measure("DNG original source open", || Source::open(input))?;
    write_source(engine, &source, settings, destination, profile)
}

/// Use the decoded original so export cannot inherit a cached display look.
pub fn write_source(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    destination: &std::path::Path,
    profile: &mut Profile,
) -> Result<()> {
    use std::{
        fs::{self, OpenOptions},
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if destination
        .extension()
        .is_none_or(|x| !x.eq_ignore_ascii_case("dng"))
    {
        return Err("DNG destination must end in .dng".into());
    }
    settings.validate()?;
    let prepared = profile.measure("DNG presentation preparation", || {
        appearance::Prepared::new(&source.metadata, settings)
    })?;
    let base = prepare_base(engine, source, settings, profile)?;
    let size = base.view().extent;
    let mut samples_profile = Profile::new(profile.mode);
    let (packed, scale) = profile.measure("DNG samples", || {
        engine.encode_dng_base(&base, &prepared, &mut samples_profile)
    })?;
    profile.gpu.extend(samples_profile.gpu);
    drop(base);
    let presentation = profile.measure("DNG appearance composition", || prepared.finish(scale))?;
    let mut name = destination.as_os_str().to_os_string();
    name.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = std::path::PathBuf::from(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        profile.measure("DNG uncompressed packaging", || {
            writer::write(
                &mut file,
                &packed,
                size,
                &prepared,
                &presentation,
                &source.metadata,
                (settings, scale),
            )
        })?;
        file.sync_all()?;
        fs::rename(&temporary, destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
#[path = "dng_test.rs"]
mod tests;
