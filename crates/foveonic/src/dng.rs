//! Exports original-derived linear DNG samples with explicit baked edits and separate camera appearance.

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
/// Rebuild from the original source and frozen output settings. WB-dependent
/// calibration, repair, highlight recovery, noise, detail, ColorDQ and selected
/// sharpening stay baked, along with active Color Editor, HDR and Local Contrast.
/// Camera response, exposure, global tone and saturation remain presentation metadata.
/// Active chromatic Color Editor edits also bake selected pre-tone color reconstruction,
/// mapping it back through the profile's inverse linear basis.
/// Classic monochrome
/// also bakes the selected color-mode mixture because it collapses the sensor
/// channels before reconstruction. Its tone and exposure remain editable.
/// With scalar-only edits, Quattro uses a canonical Standard technical color recipe. Its adaptive color
/// and spatial corrections stay baked, while the embedded selected mode is an
/// approximation. Existing reconstruction clamps cannot be undone by export.
///
/// The result is still linear working RGB, not stored DNG camera samples. Its
/// settings retain the whole requested recipe, including the later appearance.
pub fn prepare_base(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    profile: &mut Profile,
) -> Result<RenderedImage> {
    settings.validate_for_camera(&source.metadata)?;
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
    settings.validate_for_camera(&source.metadata)?;
    let white_balance = crate::render::estimated_white_balance(engine, source, settings, profile)?;
    let prepared = profile.measure("DNG presentation preparation", || {
        crate::render::dng_appearance(&source.metadata, settings, white_balance)
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
