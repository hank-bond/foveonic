//! Shares camera-choice resolution and Quality photographic and DNG exports between interactive and batch workers.

use foveonic::diagnostics as trace;
use foveonic::support::{self, Level, OperationContext};
use foveonic::{
    importing::ConflictPolicy,
    inspection::Content,
    profile::Profile,
    render::{Engine, RenderSession, Settings},
    source::Metadata,
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub use foveonic::output::Format as ExportFormat;
#[derive(Clone)]
pub struct Export {
    pub path: PathBuf,
    pub format: ExportFormat,
    pub encoding: foveonic::output::EncodingOptions,
    pub tiff_color_space: Option<foveonic::render::TiffColorSpace>,
    pub conflict: ConflictPolicy,
    pub sections: foveonic::render::ProcessingSections,
}

/// Reprocessing may replace old outputs, while different photos in one batch keep distinct names.
pub fn next_path(
    source: &std::path::Path,
    directory: &std::path::Path,
    format: ExportFormat,
    conflict: ConflictPolicy,
    reserved: &HashSet<PathBuf>,
) -> foveonic::source::Result<PathBuf> {
    let stem = source.file_stem().ok_or("source has no filename")?;
    for suffix in 0..u32::MAX {
        let mut name = stem.to_os_string();
        if suffix != 0 {
            name.push(format!("-{suffix}"));
        }
        name.push(format!(".{}", format.extension()));
        let path = directory.join(name);
        if !reserved.contains(&path) && (conflict == ConflictPolicy::Overwrite || !path.exists()) {
            return Ok(path);
        }
    }
    Err("no free output filename".into())
}

/// Publish complete single-photo and batch outputs using the same Quality recipes.
pub fn write(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
    source_path: &Path,
    export: &Export,
    profile: &mut Profile,
    progress: impl Fn(foveonic::render::RenderProgress),
) -> foveonic::source::Result<()> {
    let _export = trace::scope_with(trace::Category::Cpu, "export.write", |fields| {
        fields
            .str("destination", &export.path.to_string_lossy())
            .str("format", export.format.extension());
    });
    static NEXT_EXPORT: AtomicU64 = AtomicU64::new(1);
    let operation_id = format!("export-{}", NEXT_EXPORT.fetch_add(1, Ordering::Relaxed));
    let requested = serde_json::json!(settings);
    let mut settings = settings.for_export();
    settings.processing_sections = export.sections;
    settings.tiff_color_space = export.tiff_color_space;
    let errors = resolve_camera_settings_typed(&mut settings, session.metadata());
    let mut phase = "resolve-camera-settings";
    let mut context = OperationContext {
        operation_id: Some(operation_id.clone()),
        component: Some("export".into()),
        phase: Some(
            if errors.is_empty() {
                "publish-output"
            } else {
                "resolve-camera-settings"
            }
            .into(),
        ),
        source: Some(source_path.to_string_lossy().into_owned()),
        destination: Some(export.path.to_string_lossy().into_owned()),
        settings: Some(
            serde_json::json!({"requested":requested,"effective":settings,
            "format":export.format.extension(),"encoding":export.encoding,"conflict":export.conflict,
            "sections":export.sections,"tiff_color_space":export.tiff_color_space,
            "camera":session.metadata().properties.get("CAMMODEL"),
            "extent":[session.metadata().width,session.metadata().height]}),
        ),
        ..Default::default()
    };
    support::event(
        Level::Info,
        "operation.started",
        "Export started",
        serde_json::json!({"operation":context}),
    );
    let result = if !errors.is_empty() {
        Err(Box::new(CameraSettingsError(errors)) as Box<dyn std::error::Error + Send + Sync>)
    } else {
        phase = "prepare-output";
        foveonic::importing::write_output(&export.path, export.conflict, |temporary| {
            match export.format {
                ExportFormat::Dng => {
                    phase = "write-dng";
                    foveonic::dng::write_source(
                        engine,
                        session.source(),
                        &settings,
                        temporary,
                        profile,
                    )?;
                }
                _ => {
                    phase = "render-image";
                    let mut artifacts = engine
                        .render_session_with_progress(
                            session,
                            &settings,
                            &["final"],
                            || false,
                            profile,
                            progress,
                        )?
                        .ok_or("export render was superseded")?;
                    let artifact = artifacts.pop().ok_or("missing final export image")?;
                    let Content::Image(image) = artifact.content else {
                        return Err("final export output is not an image".into());
                    };
                    phase = "encode-image";
                    engine.write_image(
                        &image,
                        temporary,
                        export.format,
                        export.encoding,
                        profile,
                    )?;
                }
            }
            phase = "publish-output";
            Ok(())
        })
    };
    context.phase = Some(phase.into());
    trace::event(trace::Category::Io, "export.result", |fields| {
        fields.bool("success", result.is_ok());
    });
    match result {
        Ok(()) => {
            support::event(
                Level::Info,
                "artifact.published",
                "Export saved",
                serde_json::json!({"operation":context}),
            );
            Ok(())
        }
        Err(error) => Err(support::reported_error("export.write", error, context)),
    }
}

#[derive(Debug)]
struct CameraSettingsError(Vec<Box<dyn std::error::Error + Send + Sync>>);
impl std::fmt::Display for CameraSettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index != 0 {
                write!(f, "; ")?;
            }
            write!(f, "{error}")?;
        }
        Ok(())
    }
}
impl std::error::Error for CameraSettingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0
            .first()
            .map(|error| error.as_ref() as &(dyn std::error::Error + 'static))
    }
}

/// Camera defaults fill missing choices only. Otherwise loading metadata
/// could overwrite a launch override or an edit made while the photo was opening.
pub fn resolve_camera_settings(settings: &mut Settings, metadata: &Metadata) -> Vec<String> {
    resolve_camera_settings_typed(settings, metadata)
        .into_iter()
        .map(|error| error.to_string())
        .collect()
}

fn resolve_camera_settings_typed(
    settings: &mut Settings,
    metadata: &Metadata,
) -> Vec<Box<dyn std::error::Error + Send + Sync>> {
    let mut errors = Vec::new();
    let capabilities = foveonic::render::ControlCapabilities::from_metadata(metadata).ok();
    let color_required =
        capabilities.map_or(!settings.is_monochrome(), |c| c.color_mode_active(settings));
    let wb_required = capabilities.is_none_or(|c| c.white_balance_active(settings));
    for (value, recorded, required) in [
        (
            &mut settings.white_balance,
            capabilities.map_or_else(
                || metadata.recorded_white_balance(),
                |c| c.default_white_balance(metadata),
            ),
            wb_required,
        ),
        (
            &mut settings.color_mode,
            capabilities.map_or_else(
                || metadata.recorded_color_mode(),
                |c| c.default_color_mode(metadata),
            ),
            color_required,
        ),
    ] {
        if value.is_empty() {
            match recorded {
                Ok(name) => *value = name,
                Err(error) if required => errors.push(error),
                Err(_) => (),
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_processing_uses_free_output_names() {
        let folder = tempfile::tempdir().unwrap();
        let source = std::path::Path::new("card/photo.x3f");
        for name in ["photo.dng", "photo-1.dng"] {
            let output = next_path(
                source,
                folder.path(),
                ExportFormat::Dng,
                ConflictPolicy::Rename,
                &HashSet::new(),
            )
            .unwrap();
            assert_eq!(output, folder.path().join(name));
            std::fs::write(output, b"existing output").unwrap();
        }
        assert_eq!(
            next_path(
                source,
                folder.path(),
                ExportFormat::Tiff,
                ConflictPolicy::Rename,
                &HashSet::new()
            )
            .unwrap(),
            folder.path().join("photo.tiff")
        );
        assert_eq!(
            std::fs::read(folder.path().join("photo.dng")).unwrap(),
            b"existing output"
        );
    }

    #[test]
    fn overwrite_reuses_existing_names_but_never_collides_with_another_photo_in_the_batch() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("photo.dng");
        std::fs::write(&path, b"previous render").unwrap();
        let source = std::path::Path::new("folder/photo.x3f");
        assert_eq!(
            next_path(
                source,
                folder.path(),
                ExportFormat::Dng,
                ConflictPolicy::Overwrite,
                &HashSet::new()
            )
            .unwrap(),
            path
        );
        let reserved = [path.clone()].into();
        assert_eq!(
            next_path(
                source,
                folder.path(),
                ExportFormat::Dng,
                ConflictPolicy::Overwrite,
                &reserved
            )
            .unwrap(),
            folder.path().join("photo-1.dng")
        );
    }
}
