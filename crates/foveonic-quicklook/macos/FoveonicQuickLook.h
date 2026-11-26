// C interface of the foveonic-quicklook static library for the Swift extension.
// The Rust definitions in src/lib.rs must match these declarations.
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/// Little-endian RGB16 with one padding component per pixel, rows top to bottom.
#define FOVEONIC_QL_BYTES_PER_PIXEL 8

typedef struct {
    uint32_t width;
    uint32_t height;
    /// Release with foveonic_ql_samples_free.
    uint8_t *samples;
    size_t samples_len;
    /// ICC profile of the samples' color space. Static, never freed.
    const uint8_t *profile;
    size_t profile_len;
} FoveonicImage;

/// Renders the camera's default recipe at preview size, or returns the camera's embedded
/// image when rendering fails. If neither works, writes a NUL-terminated message into
/// error and returns false.
bool foveonic_ql_preview(const char *path, FoveonicImage *image, char *error, size_t error_len);

/// Decodes the camera's embedded image, upright and fitted within side pixels.
/// Reports failure as foveonic_ql_preview does.
bool foveonic_ql_thumbnail(const char *path, uint32_t side, FoveonicImage *image, char *error,
                           size_t error_len);

void foveonic_ql_samples_free(uint8_t *samples, size_t samples_len);
