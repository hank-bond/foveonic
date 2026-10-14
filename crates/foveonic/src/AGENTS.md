---
summary: "The library separates source decoding, metadata preparation, typed graph construction, GPU execution, and TIFF packaging."
---

# Processor source

Keep decoding separate from image processing. Preserve original sensor samples, camera matrix encodings, and property-list order. Read [`x3f/AGENTS.md`](./x3f/AGENTS.md) for input implementation guidance.

Image arithmetic belongs in `shaders/`. Host code prepares metadata-derived parameters, constructs typed nodes, and packages output. Keep the graph focused on the supported operations. The selected plan records photographic gaps and pending work.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[dng/](./dng/AGENTS.md)** DNG export owns camera-coordinate storage, ordered presentation composition, and uncompressed TIFF-based packaging.
- **[x3f/](./x3f/AGENTS.md)** The X3F reader decodes Merrill sensor samples and camera metadata from original files.

### Source files

- **[banding.rs](./banding.rs)** Prepares coordinate-only row-correction weights while keeping photo measurements on the GPU.
- **[calibration.rs](./calibration.rs)** Resolves metadata-derived calibration, white balance, camera color, and image geometry.
- **[camera_repair.rs](./camera_repair.rs)** Prepares camera repair lists so the GPU can fix pixels in the camera's order.
- **[color_dq.rs](./color_dq.rs)** Builds reference-derived ColorDQ lookup tables with an explicitly selected gain.
- **[color_mode.rs](./color_mode.rs)** Resolves camera color modes and their camera-ordered tone records.
- **[despeckle.rs](./despeckle.rs)** Prepares camera thresholds for GPU speckle repair without changing detection order.
- **[dng.rs](./dng.rs)** Starts DNG exports from the original photo so display edits never become baked samples.
- **[estimated_iso.rs](./estimated_iso.rs)** Estimates processing ISO from camera histograms and reference metadata.
- **[gpu.rs](./gpu.rs)** Owns the GPU device, compute kernels, buffer uploads, and export readback.
- **[gpu_storage.rs](./gpu_storage.rs)** Prevents a later GPU write from changing an image that still has readers.
- **[graph.rs](./graph.rs)** Builds typed, dependency-ordered image graphs and validates their connections.
- **[highlight.rs](./highlight.rs)** Keeps original clipping flags so GPU highlight recovery can use trustworthy neighboring samples.
- **[image.rs](./image.rs)** Keeps GPU images with their photo settings and defines crop and orientation views.
- **[inspection.rs](./inspection.rs)** Describes named inspection artifacts and preserves the configuration behind each result.
- **[interactive.rs](./interactive.rs)** Reuses unchanged inputs for the current edit while keeping earlier images safe.
- **[lib.rs](./lib.rs)** Shares Merrill decoding and GPU photo processing between TIFF and DNG exports.
- **[main.rs](./main.rs)** Inspects Merrill X3Fs and exports single images or batches through shared TIFF settings.
- **[noise.rs](./noise.rs)** Resolves camera noise presets and linked overrides for shared GPU reconstruction.
- **[options.rs](./options.rs)** Shares photographic CLI option parsing between conversion and the completed-processing profiler.
- **[output.rs](./output.rs)** Packages GPU-encoded RGB16 samples into an oriented, color-profiled TIFF.
- **[pattern.rs](./pattern.rs)** Prepares repeatable pattern-correction geometry without moving photo work to the CPU.
- **[preview.rs](./preview.rs)** Reads embedded JPEG previews and presentation dimensions from X3F sections.
- **[profile.rs](./profile.rs)** Collects host/GPU timings and engine-owned transfer, submission, and allocation counts.
- **[recipe.rs](./recipe.rs)** Encodes exact effective operation parameters for graph versioning without hashing photographic samples.
- **[reconstruction.rs](./reconstruction.rs)** Executes reference-style processing tiles, noise filtering, residual detail, and image projections on GPU.
- **[render.rs](./render.rs)** Builds typed processing and export recipes around owned GPU images and execution-local scratch.
- **[repair.rs](./repair.rs)** Selects camera-derived cutoffs and preserves ordered camera records for masked GPU correction.
- **[resample.rs](./resample.rs)** Prepares camera resampling weights and leaves image calculations on the GPU.
- **[scopes.rs](./scopes.rs)** Builds histograms and smooth waveform traces from GPU image regions.
- **[scratch.rs](./scratch.rs)** Recycles guarded storage only after the last shared reader releases it on the ordered GPU queue.
- **[sidecar.rs](./sidecar.rs)** Reads and atomically replaces bounded versioned JSON editing recipes beside immutable X3F inputs.
- **[source.rs](./source.rs)** Owns original Merrill sensor samples and typed camera metadata.
- **[spatial_tables.rs](./spatial_tables.rs)** Prepare Merrill spatial gain grids from camera byte tables, aperture, focus, and chief-ray angle.
- **[tone.rs](./tone.rs)** Prepares a camera-derived color-mode tone table for application in a shader.

<!-- INDEX:END -->
