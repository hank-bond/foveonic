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

- **[banding.rs](./banding.rs)** Prepares geometric least-squares row-banding fits and deterministic patch/row traversal for GPU-only photographic estimation and correction.
- **[calibration.rs](./calibration.rs)** Resolves metadata-derived calibration, white balance, camera color, and image geometry.
- **[camera_repair.rs](./camera_repair.rs)** Prepares complete native camera repair metadata and executes raw-signal bad-red selection, dependency-safe pixel phases, and ordered masked cluster matching on GPU.
- **[color_dq.rs](./color_dq.rs)** Builds reference-derived ColorDQ lookup tables with an explicitly selected gain.
- **[color_mode.rs](./color_mode.rs)** Resolves camera color modes and their camera-ordered tone records.
- **[despeckle.rs](./despeckle.rs)** Prepares reference speckle thresholds and runs deterministic detection and ordered chroma-anchored correction on GPU.
- **[estimated_iso.rs](./estimated_iso.rs)** Estimates processing ISO from camera histograms and reference metadata.
- **[gpu.rs](./gpu.rs)** Owns the GPU device, compute kernels, buffer uploads, and export readback.
- **[gpu_storage.rs](./gpu_storage.rs)** Guards GPU buffer contents with shared read ownership, exclusive writes and allocation generations.
- **[graph.rs](./graph.rs)** Builds typed, dependency-ordered image graphs and validates their connections.
- **[highlight.rs](./highlight.rs)** Prepares immutable camera saturation provenance and performs reference highlight restoration and neighborhood conditioning on GPU.
- **[image.rs](./image.rs)** Describes owned GPU image snapshots, their source context, and logical output views.
- **[inspection.rs](./inspection.rs)** Describes named inspection artifacts and preserves the configuration behind each result.
- **[interactive.rs](./interactive.rs)** Retains one active edit's immutable input boundary, exact graph recipes, and safe render supersession.
- **[lib.rs](./lib.rs)** Provides Merrill source decoding and a shared GPU TIFF rendering engine.
- **[main.rs](./main.rs)** Inspects Merrill X3Fs and exports single images or batches through shared TIFF settings.
- **[noise.rs](./noise.rs)** Prepares shared camera noise recipes, correction couplings, and metadata-selected full/reduced reconstruction controls.
- **[options.rs](./options.rs)** Shares photographic CLI option parsing between conversion and the completed-processing profiler.
- **[output.rs](./output.rs)** Packages GPU-encoded RGB16 samples into an oriented, color-profiled TIFF.
- **[pattern.rs](./pattern.rs)** Metadata/geometry-only setup for native deterministic luminance-ratio pattern conditioning.
- **[preview.rs](./preview.rs)** Reads embedded JPEG previews and presentation dimensions from X3F sections.
- **[profile.rs](./profile.rs)** Collects host/GPU timings and engine-owned transfer, submission, and allocation counts.
- **[recipe.rs](./recipe.rs)** Encodes exact effective operation parameters for graph versioning without hashing photographic samples.
- **[reconstruction.rs](./reconstruction.rs)** Executes reference-style processing tiles, noise filtering, residual detail, and image projections on GPU.
- **[render.rs](./render.rs)** Builds typed processing and export recipes around owned GPU images and execution-local scratch.
- **[repair.rs](./repair.rs)** Selects camera-derived cutoffs and preserves ordered camera records for masked GPU correction.
- **[resample.rs](./resample.rs)** Native triangular polyphase metadata preparation and immutable GPU RGB resampling.
- **[scopes.rs](./scopes.rs)** Samples the current floating GPU buffer into compact histogram and waveform counts.
- **[scratch.rs](./scratch.rs)** Recycles guarded storage only after the last shared reader releases it on the ordered GPU queue.
- **[sidecar.rs](./sidecar.rs)** Reads and atomically replaces bounded versioned JSON editing recipes beside immutable X3F inputs.
- **[source.rs](./source.rs)** Owns original Merrill sensor samples and typed camera metadata.
- **[spatial_tables.rs](./spatial_tables.rs)** Prepare Merrill spatial gain grids from camera byte tables, aperture, focus, and chief-ray angle.
- **[tone.rs](./tone.rs)** Prepares a camera-derived color-mode tone table for application in a shader.

<!-- INDEX:END -->
