---
summary: "The library separates source decoding, metadata preparation, typed graph construction, GPU execution, and TIFF packaging."
---

# Processor source

Keep decoding separate from image processing. Preserve original sensor samples, camera matrix encodings, and property-list order. Read [`x3f/AGENTS.md`](./x3f/AGENTS.md) for input implementation guidance.

Image arithmetic belongs in WGSL beside its owning [node group](./nodes/AGENTS.md). Shared presentation sampling lives in `gpu/shaders/`. Host code prepares metadata-derived parameters, constructs typed nodes, and packages output. [Recipe builders](./render/recipes/AGENTS.md) choose processing order independently of [execution](./render/execute.rs). Keep the graph focused on supported operations. The selected plan records photographic gaps and pending work.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[dng/](./dng/AGENTS.md)** DNG export owns camera-coordinate storage, ordered presentation composition, and uncompressed TIFF-based packaging.
- **[gpu/](./gpu/AGENTS.md)** GPU internals register node shaders and protect storage ownership and scratch reuse.
- **[graph/](./graph/AGENTS.md)** Graph operations define supported node parameters, typed ports, cache keys, and named output dependencies.
- **[nodes/](./nodes/AGENTS.md)** Processing nodes group sensor corrections, color and monochrome reconstruction, and finishing with their shaders and tests.
- **[render/](./render/AGENTS.md)** Rendering separates shared settings, authored recipes, graph execution, and owned inspection artifacts.
- **[x3f/](./x3f/AGENTS.md)** The X3F reader decodes sensor samples and camera metadata from original files.

### Source files

- **[background.rs](./background.rs)** Gives foreground work priority over interruptible prefetch and pausable background jobs.
- **[color_space.rs](./color_space.rs)** Defines RGB primaries, camera color-space selection, output colorants, and transfer functions.
- **[decode_cache.rs](./decode_cache.rs)** Reuses decoded sources, original bytes, selection metadata and previews in a bounded temporary disk LRU.
- **[decode_cache_lru.rs](./decode_cache_lru.rs)** Applies the shared disk budget while favoring nearby source bundles and display previews.
- **[decode_cache_original.rs](./decode_cache_original.rs)** Retains original X3F bytes for imports under the decoded cache's identity and budget.
- **[decode_cache_preview.rs](./decode_cache_preview.rs)** Stores bounded display previews under the decoded cache's identity, recipe, and shared LRU budget.
- **[decode_cache_selection.rs](./decode_cache_selection.rs)** Keeps complete local selection metadata and thumbnails beside decoded sources.
- **[decode_cache_window.rs](./decode_cache_window.rs)** Orders a bounded selection neighborhood for prefetch and cache retention.
- **[decode_cache_worker.rs](./decode_cache_worker.rs)** Schedules bounded cache writes, selection prefetch, and readiness scans behind foreground work.
- **[decode_pool.rs](./decode_pool.rs)** Shares a bounded FIFO sensor-layer pool across ordered speculative photo loads.
- **[dng.rs](./dng.rs)** Starts DNG exports from the original photo so display edits never become baked samples.
- **[gpu.rs](./gpu.rs)** Owns the GPU device, compute kernels, buffer uploads, and export readback.
- **[graph.rs](./graph.rs)** Builds typed dependency-ordered graphs and validates connections between operation ports.
- **[image.rs](./image.rs)** Keeps GPU images with their photo settings and defines crop and orientation views.
- **[importing.rs](./importing.rs)** Copies originals and stores current import and hidden decisions in one destination file.
- **[inspection.rs](./inspection.rs)** Describes named inspection artifacts and preserves the configuration behind each result.
- **[interactive.rs](./interactive.rs)** Reuses unchanged inputs for the current edit while keeping earlier images safe.
- **[lib.rs](./lib.rs)** Shares X3F decoding and GPU photo processing between TIFF and DNG exports.
- **[main.rs](./main.rs)** Inspects supported X3Fs and exports TIFF or linear DNG through shared rendering settings.
- **[options.rs](./options.rs)** Shares photographic CLI option parsing between conversion and the completed-processing profiler.
- **[output.rs](./output.rs)** Packages GPU-encoded RGB16 samples into an oriented, color-profiled TIFF.
- **[preview.rs](./preview.rs)** Decodes embedded bitmap and JPEG previews into color-managed sRGB display pixels.
- **[preview_color.rs](./preview_color.rs)** Normalizes embedded display pixels from camera metadata and JPEG profiles to sRGB.
- **[profile.rs](./profile.rs)** Collects host/GPU timings and engine-owned transfer, submission, and allocation counts.
- **[recipe.rs](./recipe.rs)** Encodes exact effective operation parameters for graph versioning without hashing photographic samples.
- **[render.rs](./render.rs)** Shares GPU engine entry points for photographic rendering, inspection, display, and export.
- **[scopes.rs](./scopes.rs)** Builds histograms and smooth waveform traces from GPU image regions.
- **[sidecar.rs](./sidecar.rs)** Keeps bounded editing recipes in a visible source-folder subdirectory with atomic replacement.
- **[source.rs](./source.rs)** Owns original sensor storage layouts and typed camera metadata.

<!-- INDEX:END -->
