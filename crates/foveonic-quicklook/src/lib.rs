//! Renders X3F space-bar previews for the macOS Quick Look extension through a C interface.

use foveonic::profile::Profile;
use foveonic::render::{camera_settings, native_extent, Engine, ProcessingQuality, Scale};
use foveonic::source::{Result, Source};
use std::ffi::{c_char, CStr};
use std::path::Path;

/// Previews keep at least this many pixels on their long edge.
/// It is enough for a full-screen preview without rendering larger files at native size.
const MIN_PREVIEW_LONG_EDGE: u32 = 2048;

/// Largest power-of-two divisor that keeps the long edge at or above the preview minimum.
fn preview_divisor(native: [u32; 2]) -> u32 {
    let long = native[0].max(native[1]);
    let mut divisor = 1;
    while long / (divisor * 2) >= MIN_PREVIEW_LONG_EDGE {
        divisor *= 2;
    }
    divisor
}

/// Render the camera's default recipe at preview size with Speed processing.
pub fn render_preview(path: &Path) -> Result<foveonic::render::EncodedImage> {
    let engine = Engine::new()?;
    let source = Source::open(path)?;
    let metadata = &source.metadata;
    let mut settings = camera_settings(metadata, None)?;
    settings.processing_quality = ProcessingQuality::Speed;
    settings.working_scale = Scale::new(preview_divisor(native_extent(metadata)?))?;
    let mut profile = Profile::default();
    let image = engine.render(&source, &settings, &mut profile)?;
    // Release decoded sensor samples before reading pixels back.
    drop(source);
    engine.encode_image(&image, &mut profile)
}

/// Mirrors `FoveonicImage` in `macos/FoveonicQuickLook.h`, which documents the fields.
#[repr(C)]
pub struct FoveonicImage {
    pub width: u32,
    pub height: u32,
    pub samples: *mut u8,
    pub samples_len: usize,
    pub profile: *const u8,
    pub profile_len: usize,
}

/// Declared in `macos/FoveonicQuickLook.h`.
///
/// # Safety
/// `path` must be a NUL-terminated UTF-8 path, `image` must be writable, and
/// `error` must point to `error_len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn foveonic_ql_preview(
    path: *const c_char,
    image: *mut FoveonicImage,
    error: *mut c_char,
    error_len: usize,
) -> bool {
    let result = std::panic::catch_unwind(|| {
        let path = unsafe { CStr::from_ptr(path) }.to_str()?;
        render_preview(Path::new(path))
    })
    .unwrap_or_else(|_| Err("preview rendering panicked".into()));
    match result {
        Ok(encoded) => {
            let [width, height] = encoded.extent;
            let profile = encoded.color_space.profile();
            let samples = Box::leak(encoded.samples.into_boxed_slice());
            unsafe {
                image.write(FoveonicImage {
                    width,
                    height,
                    samples: samples.as_mut_ptr(),
                    samples_len: samples.len(),
                    profile: profile.as_ptr(),
                    profile_len: profile.len(),
                });
            }
            true
        }
        Err(message) => {
            if error_len > 0 {
                let text = message.to_string();
                let len = text.len().min(error_len - 1);
                unsafe {
                    std::ptr::copy_nonoverlapping(text.as_ptr(), error.cast(), len);
                    *error.add(len) = 0;
                }
            }
            false
        }
    }
}

/// Declared in `macos/FoveonicQuickLook.h`.
///
/// # Safety
/// `samples` and `samples_len` must come from one successful `foveonic_ql_preview`
/// call and be freed once.
#[no_mangle]
pub unsafe extern "C" fn foveonic_ql_samples_free(samples: *mut u8, samples_len: usize) {
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(samples, samples_len)) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_declares_the_engine_pixel_size() {
        let header = include_str!("../macos/FoveonicQuickLook.h");
        let declared = format!(
            "#define FOVEONIC_QL_BYTES_PER_PIXEL {}\n",
            foveonic::render::EncodedImage::BYTES_PER_PIXEL
        );
        assert!(header.contains(&declared));
    }

    #[test]
    fn previews_halve_only_while_the_long_edge_stays_at_the_minimum() {
        // Merrill High, Medium and Low, then a Classic DP1 and two larger sizes.
        for (native, divisor) in [
            ([4704, 3136], 2),
            ([3264, 2176], 1),
            ([2336, 1568], 1),
            ([2640, 1760], 1),
            ([1760, 2640], 1),
            ([8000, 6000], 2),
            ([9000, 6000], 4),
            ([4096, 2048], 2),
        ] {
            assert_eq!(preview_divisor(native), divisor, "{native:?}");
        }
    }
}
