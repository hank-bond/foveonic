//! Shares camera-choice resolution and Full TIFF/DNG exports between interactive and batch workers.

use foveonic::{
    importing::ConflictPolicy,
    inspection::Content,
    profile::Profile,
    render::{Engine, PreviewQuality, RenderSession, Settings},
    source::Metadata,
};
use std::{collections::HashSet, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExportFormat {
    Tiff,
    #[default]
    Dng,
}
impl ExportFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tiff => "TIFF",
            Self::Dng => "DNG",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Tiff => "tiff",
            Self::Dng => "dng",
        }
    }
}
#[derive(Clone)]
pub struct Export {
    pub path: PathBuf,
    pub format: ExportFormat,
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

/// Publish complete single-photo and batch outputs using the same Full recipes.
pub fn write(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
    export: &Export,
    profile: &mut Profile,
    progress: impl Fn(foveonic::render::RenderProgress),
) -> foveonic::source::Result<()> {
    let mut settings = settings.clone();
    settings.processing_sections = export.sections;
    settings.tiff_color_space = export.tiff_color_space;
    settings.preview_quality = PreviewQuality::Full;
    settings.preview_region = None;
    let errors = resolve_camera_settings(&mut settings, session.metadata());
    if !errors.is_empty() {
        return Err(errors.join("; ").into());
    }
    foveonic::importing::write_output(&export.path, export.conflict, |temporary| {
        match export.format {
            ExportFormat::Dng => {
                foveonic::dng::write_source(
                    engine,
                    session.source(),
                    &settings,
                    temporary,
                    profile,
                )?;
            }
            ExportFormat::Tiff => {
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
                engine.write_tiff(&image, temporary, profile)?;
            }
        }
        Ok(())
    })
}

/// Camera defaults fill missing choices only. Otherwise loading metadata
/// could overwrite a launch override or an edit made while the photo was opening.
pub fn resolve_camera_settings(settings: &mut Settings, metadata: &Metadata) -> Vec<String> {
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
                Err(error) if required => errors.push(error.to_string()),
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
