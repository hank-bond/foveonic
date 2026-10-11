---
summary: "The library separates source decoding, metadata preparation, typed graph construction, GPU execution, and TIFF packaging."
---

# Processor source

Keep decoding separate from image processing. Preserve original sensor samples, camera matrix encodings, and property-list order. Read [`x3f/AGENTS.md`](./x3f/AGENTS.md) for input implementation guidance.

Image arithmetic belongs in `shaders/`. Host code prepares metadata-derived parameters, constructs typed nodes, and packages output. Keep the graph focused on the supported operations. The selected plan records photographic gaps and pending work.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[x3f/](./x3f/AGENTS.md)** The X3F reader decodes Merrill sensor samples and camera metadata from original files.

### Source files

- **[calibration.rs](./calibration.rs)** Resolves metadata-derived calibration, white balance, camera color, and image geometry.
- **[color_dq.rs](./color_dq.rs)** Builds reference-derived ColorDQ lookup tables with an explicitly selected gain.
- **[color_mode.rs](./color_mode.rs)** Resolves camera color modes and their camera-ordered tone records.
- **[gpu.rs](./gpu.rs)** Owns the GPU device, compute kernels, buffer uploads, and export readback.
- **[graph.rs](./graph.rs)** Builds typed, dependency-ordered image graphs and validates their connections.
- **[image.rs](./image.rs)** Describes owned GPU image snapshots, their source context, and logical output views.
- **[lib.rs](./lib.rs)** Provides Merrill source decoding and a shared GPU TIFF rendering engine.
- **[main.rs](./main.rs)** Inspects Merrill X3Fs and exports single images or batches through shared TIFF settings.
- **[output.rs](./output.rs)** Packages GPU-encoded RGB16 samples into an oriented, color-profiled TIFF.
- **[profile.rs](./profile.rs)** Collects lightweight host wall timings and optional GPU timestamp measurements.
- **[render.rs](./render.rs)** Builds typed processing and export recipes around owned GPU images and execution-local scratch.
- **[repair.rs](./repair.rs)** Expands selected camera bad-pixel records into an immutable per-pixel repair map.
- **[scratch.rs](./scratch.rs)** Reuses exclusively owned, fully overwritten buffers within one ordered graph execution.
- **[source.rs](./source.rs)** Owns decoded Merrill samples and typed metadata without photographic adjustments.
- **[spatial_tables.rs](./spatial_tables.rs)** Prepare Merrill spatial gain grids from camera byte tables, aperture, focus, and chief-ray angle.
- **[tone.rs](./tone.rs)** Prepares a camera-derived color-mode tone table for application in a shader.

<!-- INDEX:END -->
