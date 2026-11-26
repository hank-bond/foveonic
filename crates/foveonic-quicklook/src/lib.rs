//! Renders X3F space-bar previews for the macOS Quick Look extension through a C interface.

use foveonic::profile::Profile;
use foveonic::render::{
    camera_settings, native_extent, EncodedImage, Engine, ProcessingQuality, Scale,
};
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
pub fn render_preview(path: &Path) -> Result<EncodedImage> {
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

/// The space-bar preview: the rendered photograph, or the camera's embedded image
/// when rendering fails.
pub fn preview(path: &Path) -> Result<EncodedImage> {
    or_embedded(path, || render_preview(path))
}

/// A render that fails or panics, as a GPU validation failure does, falls back to
/// the embedded image. Keep the render's error when that cannot stand in either.
pub fn or_embedded(
    path: &Path,
    render: impl FnOnce() -> Result<EncodedImage> + std::panic::UnwindSafe,
) -> Result<EncodedImage> {
    std::panic::catch_unwind(render)
        .unwrap_or_else(|_| Err("preview rendering panicked".into()))
        .or_else(|error| render_thumbnail(path, MIN_PREVIEW_LONG_EDGE).map_err(|_| error))
}

/// The camera's embedded image, upright and fitted within `side` pixels.
pub fn render_thumbnail(path: &Path, side: u32) -> Result<EncodedImage> {
    let preview = foveonic::preview::read_at_least(path, side)?;
    Ok(EncodedImage::from_linear(
        &preview.upright_linear(side)?,
        preview.color_space,
    ))
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
    unsafe { deliver(path, image, error, error_len, preview) }
}

/// Declared in `macos/FoveonicQuickLook.h`.
///
/// # Safety
/// As for `foveonic_ql_preview`.
#[no_mangle]
pub unsafe extern "C" fn foveonic_ql_thumbnail(
    path: *const c_char,
    side: u32,
    image: *mut FoveonicImage,
    error: *mut c_char,
    error_len: usize,
) -> bool {
    unsafe {
        deliver(path, image, error, error_len, |path| {
            render_thumbnail(path, side)
        })
    }
}

/// Hand a rendered image to Swift, which then owns its samples.
unsafe fn deliver(
    path: *const c_char,
    image: *mut FoveonicImage,
    error: *mut c_char,
    error_len: usize,
    render: impl FnOnce(&Path) -> Result<EncodedImage> + std::panic::UnwindSafe,
) -> bool {
    unsafe {
        answer(path, error, error_len, render, |encoded| {
            let [width, height] = encoded.extent;
            let profile = encoded.color_space.profile();
            let samples = Box::leak(encoded.samples.into_boxed_slice());
            image.write(FoveonicImage {
                width,
                height,
                samples: samples.as_mut_ptr(),
                samples_len: samples.len(),
                profile: profile.as_ptr(),
                profile_len: profile.len(),
            });
        })
    }
}

/// Run `work` on the C path without letting a panic cross into Swift. On success
/// `write` stores the result; on failure the message goes into `error`.
unsafe fn answer<T>(
    path: *const c_char,
    error: *mut c_char,
    error_len: usize,
    work: impl FnOnce(&Path) -> Result<T> + std::panic::UnwindSafe,
    write: impl FnOnce(T),
) -> bool {
    let result = std::panic::catch_unwind(|| {
        let path = unsafe { CStr::from_ptr(path) }.to_str()?;
        work(Path::new(path))
    })
    .unwrap_or_else(|_| Err("the Foveonic library panicked".into()));
    match result {
        Ok(value) => {
            write(value);
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

/// Text fields hold at most this many bytes plus a terminating NUL.
const CAPTURE_TEXT_BYTES: usize = 64;

/// Mirrors `FoveonicCapture` in `macos/FoveonicQuickLook.h`, which documents the fields.
#[repr(C)]
pub struct FoveonicCapture {
    pub make: [c_char; CAPTURE_TEXT_BYTES],
    pub model: [c_char; CAPTURE_TEXT_BYTES],
    pub lens: [c_char; CAPTURE_TEXT_BYTES],
    pub focal_length: f64,
    pub f_number: f64,
    pub exposure_seconds: f64,
    pub exposure_bias: f64,
    pub iso: u32,
    pub focal_length_35mm: u16,
    pub exposure_program: u16,
    pub metering: u16,
    pub taken: [c_char; CAPTURE_TEXT_BYTES],
    pub width: u32,
    pub height: u32,
}

impl FoveonicCapture {
    fn new(summary: &foveonic::source::MetadataSummary) -> Self {
        fn text(value: Option<&str>) -> [c_char; CAPTURE_TEXT_BYTES] {
            let mut out = [0; CAPTURE_TEXT_BYTES];
            let value = value.unwrap_or_default();
            // Truncate on a character boundary, keeping room for the NUL.
            let mut end = value.len().min(CAPTURE_TEXT_BYTES - 1);
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            for (slot, byte) in out.iter_mut().zip(&value.as_bytes()[..end]) {
                *slot = *byte as c_char;
            }
            out
        }
        let facts = &summary.capture;
        let [width, height] = summary.extent.unwrap_or_default();
        Self {
            make: text(Some(&facts.make)),
            model: text(facts.model.as_deref()),
            lens: text(facts.lens.as_deref()),
            focal_length: facts.focal_length.unwrap_or(f64::NAN),
            f_number: facts.f_number.unwrap_or(f64::NAN),
            exposure_seconds: facts.exposure_seconds.unwrap_or(f64::NAN),
            exposure_bias: facts.exposure_bias.unwrap_or(f64::NAN),
            iso: facts.iso.unwrap_or_default(),
            focal_length_35mm: facts.focal_length_35mm.unwrap_or_default(),
            exposure_program: facts.exposure_program.unwrap_or_default(),
            metering: facts.metering.unwrap_or_default(),
            taken: text(facts.taken.as_deref()),
            width,
            height,
        }
    }
}

/// Declared in `macos/FoveonicQuickLook.h`.
///
/// # Safety
/// `path` must be a NUL-terminated UTF-8 path, `capture` must be writable, and
/// `error` must point to `error_len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn foveonic_ql_capture(
    path: *const c_char,
    capture: *mut FoveonicCapture,
    error: *mut c_char,
    error_len: usize,
) -> bool {
    unsafe {
        answer(
            path,
            error,
            error_len,
            |path| foveonic::source::read_metadata_summary(path).map(|s| FoveonicCapture::new(&s)),
            |value| capture.write(value),
        )
    }
}

/// Declared in `macos/FoveonicQuickLook.h`.
///
/// # Safety
/// `samples` and `samples_len` must come from one successful `foveonic_ql_preview`
/// or `foveonic_ql_thumbnail` call and be freed once.
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
        // Whole lines, so a CRLF checkout matches and 4 cannot match 40.
        let declared = format!(
            "#define FOVEONIC_QL_BYTES_PER_PIXEL {}",
            foveonic::render::EncodedImage::BYTES_PER_PIXEL
        );
        assert!(header.lines().any(|line| line == declared));
        assert!(header.contains(&format!(
            "_Static_assert(sizeof(FoveonicCapture) == {},",
            std::mem::size_of::<FoveonicCapture>()
        )));
        for field in ["make", "model", "lens", "taken"] {
            assert!(
                header.contains(&format!("char {field}[{CAPTURE_TEXT_BYTES}];")),
                "{field}"
            );
        }
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
