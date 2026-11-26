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
    let quality_settings = settings.for_export();
    let settings = &quality_settings;
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
    let source = profile
        .measure("DNG original source open", || Source::open(input))
        .map_err(|error| {
            crate::support::reported_error(
                "dng.source",
                error,
                crate::support::OperationContext {
                    component: Some("export".into()),
                    phase: Some("open-source".into()),
                    source: Some(input.to_string_lossy().into_owned()),
                    destination: Some(destination.to_string_lossy().into_owned()),
                    settings: Some(serde_json::json!({"requested":crate::support::settings_snapshot(settings)})),
                    ..Default::default()
                },
            )
        })?;
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
    let mut phase = "validate-destination";
    let mut temporary_path = None;
    let mut cleanup = "not-needed";
    let result = (|| -> Result<()> {
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
        phase = "validate-recipe";
        settings.validate_for_camera(&source.metadata)?;
        // The DNG profile and appearance need the color mode As Shot stands for.
        let resolved = settings.with_camera_choices(&source.metadata);
        let settings = resolved.as_ref();
        phase = "prepare-appearance";
        let white_balance =
            crate::render::estimated_white_balance(engine, source, settings, profile)?;
        let prepared = profile.measure("DNG presentation preparation", || {
            crate::render::dng_appearance(&source.metadata, settings, white_balance)
        })?;
        phase = "render-dng-base";
        let base = prepare_base(engine, source, settings, profile)?;
        let size = base.view().extent;
        let mut samples_profile = Profile::new(profile.mode);
        phase = "encode-dng-samples";
        let (packed, scale) = profile.measure("DNG samples", || {
            engine.encode_dng_base(&base, &prepared, &mut samples_profile)
        })?;
        profile.gpu.extend(samples_profile.gpu);
        drop(base);
        phase = "compose-appearance";
        let presentation =
            profile.measure("DNG appearance composition", || prepared.finish(scale))?;
        let mut name = destination.as_os_str().to_os_string();
        name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = std::path::PathBuf::from(name);
        temporary_path = Some(temporary.clone());
        phase = "open-dng-temporary";
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result: Result<()> = (|| {
            phase = "package-dng";
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
            phase = "sync-dng";
            file.sync_all()?;
            phase = "publish-dng";
            fs::rename(&temporary, destination)?;
            Ok(())
        })();
        if result.is_err() {
            match fs::remove_file(&temporary) {
                Ok(()) => cleanup = "removed",
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    cleanup = "already-absent"
                }
                Err(error) => {
                    cleanup = "failed";
                    crate::support::report_error(
                        "dng.cleanup",
                        &error,
                        crate::support::OperationContext {
                            component: Some("export".into()),
                            phase: Some("remove-temporary".into()),
                            destination: Some(temporary.to_string_lossy().into_owned()),
                            ..Default::default()
                        },
                    );
                }
            }
        }
        result
    })();
    result.map_err(|error| {
        let context = crate::support::OperationContext {
            component: Some("export".into()), phase: Some(phase.into()),
            destination: Some(destination.to_string_lossy().into_owned()),
            settings: Some(serde_json::json!({"requested":crate::support::settings_snapshot(settings),"export_recipe":crate::support::settings_snapshot(&settings.for_export()),
                "camera":source.metadata.properties.get("CAMMODEL"),"extent":[source.metadata.width,source.metadata.height],
                "temporary":temporary_path.as_ref().map(|path|path.to_string_lossy()),"cleanup":cleanup})), ..Default::default()
        };
        crate::support::reported_error("dng.write", error, context)
    })
}

#[cfg(test)]
#[path = "dng_test.rs"]
mod tests;
