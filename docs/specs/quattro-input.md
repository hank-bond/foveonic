---
summary: "Quattro input preserves mixed-resolution sensor codes and exposes independently sized inspection planes."
kind: reference
status: accepted
implementation: conforming
---

# Quattro input and plane inspection

## Requirements

The input reader must decode the acquired unsigned format35, format37 and format39 originals independently of photographic rendering. Preserve original code values, four blue phases, special-column replacement and metadata representation. Support X3F4.1/4.2 and CAMF5. Preserve the header processing version separately from camera properties. Files without PROP must use consistent preview Exif and maker-note properties for both capture-only and full loading.

Source storage must distinguish equal-resolution interleaved RGB from packed six-sample cells. The latter contain R, G, B00, B01, B10 and B11. Keep samples immutable and expose their storage layout and physical cell extent. A shared encoding must not imply a shared rendering or calibration route.

A functional GPU extraction operation must publish low-resolution RGB and full-resolution blue with their own image descriptions. Ordinary numerical calibration applies offset and scale, retains negative values, and caps only the high-resolution blue. The separate [Tiger contract](./tiger-rendering.md) adds per-channel digital gains and a calibrated upper cap on both planes. Camera-version selection belongs to recipe preparation. Unsupported calibration variants must fail explicitly while raw plane inspection remains available.

The graph must preserve the distinction between original sensor codes and offset/scale-corrected codes. Blue has twice the cell width and height. Requested planes, retained siblings, view crop and rotation must respect those physical dimensions. Inspection must not require unrelated later metadata, and numerical nodes must remain camera unaware.

The recipe must separate graph composition from metadata and resource preparation. Its inspection catalogue owns names and channel meanings. Image layout and coordinate-view semantics belong to the image contract and must be shared by execution and presentation consumers. Keep these contracts specific to demonstrated operations. A broader rendering-capability framework is not required for completing Quattro support.

## Blue calibration maps

The default engine must use the bundled exact map data for camera identifiers 80 through 83. These are 24 files, with six `C{camera-id}_{1..6}` maps per camera. Keep their [source provenance and file hashes](../../crates/foveonic/assets/quattro/README.md) with the bundled data. Rendering and blue-map inspection must work without an installed reference application, environment configuration or a working-directory convention.

An explicitly supplied calibration directory may override the bundled resources through the existing resource API. Each engine owns an immutable resource snapshot for its lifetime. A requested mapped camera needs a complete six-map set. Missing maps in a supplied override must report an error rather than falling back silently. A camera with no map in the recovered catalogue is a separate supported absence.

Map selection derives from camera identifier, aperture and focus metadata. Host preparation may blend the small camera tables. Image resizing and correction belong on the GPU. Preserve the reference's parity geometry, zero-support triangular sampling, threshold equality, signed values and odd trailing rows or columns.

## Conformance

Compare all acquired decoded sensor components and CAMF bytes with independent native captures. Validate raw and corrected GPU extraction, map blends and complete blue-map correction independently. Exercise selective graph requests, sibling retention, physical dimensions and display scaling. Preserve existing Classic and Merrill behavior.

Photographic reconstruction and TIFF output have a separate [initial color-rendering specification](./quattro-rendering.md). Editable DNG follows the [output contract](./output-strategies.md). Sensor-plane inspection must remain available independently of photographic support. Cameras without a completed rendering recipe must retain an honest unavailable Final state.
