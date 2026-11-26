// C interface of the foveonic-quicklook static library for the Swift extensions and the
// Spotlight importer.
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

/// Capture details for Spotlight, read without decoding sensor data. Text is
/// NUL-terminated and empty when unknown; numbers are NaN or 0 when unknown.
typedef struct {
    char make[64];
    char model[64];
    char lens[64];
    /// Millimetres.
    double focal_length;
    double f_number;
    double exposure_seconds;
    /// EV, including any bracket offset.
    double exposure_bias;
    uint32_t iso;
    uint16_t focal_length_35mm;
    /// EXIF codes: 1 manual, 2 program, 3 aperture, 4 shutter priority.
    uint16_t exposure_program;
    /// EXIF codes: 1 average, 2 center-weighted, 5 multi-segment.
    uint16_t metering;
    /// The camera's wall-clock time as YYYY:MM:DD HH:MM:SS.
    char taken[64];
    /// Upright pixel size.
    uint32_t width;
    uint32_t height;
} FoveonicCapture;
// The Rust definition is checked against this size, so the layouts agree.
_Static_assert(sizeof(FoveonicCapture) == 312, "FoveonicCapture layout changed");

bool foveonic_ql_capture(const char *path, FoveonicCapture *capture, char *error, size_t error_len);
