# Foveonic GPU

A personal Merrill X3F processing app with a GUI and a batch CLI, focused on attractive photographs and further editing latitude.

A shared shader-based image engine builds internal processing graphs from selected settings and algorithms. Output recipes target compressed DNGs with editable samples and embedded looks, or TIFFs with white balance, color, and tone baked in.

TIFF rendering and an opt-in full-resolution noise/detail baseline are implemented. Compressed DNG and highlight reconstruction follow incrementally.

## Run the GUI

```sh
cargo run --release -p foveonic-gui -- {X3F_OR_FOLDER}
```

The egui desktop app keeps an embedded-JPEG organizer, GPU image viewport, and tools visible in three resizable panes. Switch the organizer between icons and compact list; drag tool headers to reorder them or click to collapse. Tools include a draggable navigator (double-click to fit), current-buffer histogram/parade/waveform scopes, processing controls, and photographic metadata. Sliders have editable values and double-click reset. Focused value boxes support Up/Down nudging (0.1 for floats, 1 for integers unless overridden). One Sensor gain scales the calibrated layers before color conversion.

Selection renders automatically using the X3F's recorded WB and color mode; `--wb`/`--mode` override them. Inspection offers visual image buffers, including grayscale sensor layers; non-image node diagnostics are deferred. Export always runs the final TIFF recipe. UI layout, scope/organizer choices, and the last opened folder persist; an explicit launch path takes precedence. Editing changes remain session-only. A bar below the image shows capture settings, filename, and visible image-pixel bounds. Inspect groups RGB channels and sensor layers under their processing buffers. Presentation uses sRGB SDR; monitor-profile correction is pending. Scopes show values in the buffer's signal space.

## Run the CLI

Inspect metadata or render an RGB16 TIFF through the GPU graph. TIFF color space follows the file's sRGB or Adobe RGB camera route.

```sh
cargo run --release -p foveonic -- inspect {INPUT_X3F}
cargo run --release -p foveonic -- render {INPUT_X3F} {OUTPUT_TIFF} --wb Sunlight --mode Standard
cargo run --release -p foveonic -- batch {OUTPUT_DIR} {INPUT_DIR} --wb Sunlight --mode Standard
cargo run --release -p foveonic -- --help
```

Controls include exposure, contrast, shadows, highlights, saturation, and a global sensor gain (`--sensor-gain`, neutral at 1×). Camera-record repair and dynamic despeckling are on by default with separate GUI controls; CLI `--no-repair` and `--no-despeckle` disable them. Repair uses the camera cutoff unless `--repair-badness MIN` selects a manual one. ColorDQ conditioning (`--color-dq-gain`, default off), Gaussian unsharp masking, and exact-size bilinear resizing are opt-in; `--help` lists their settings. Batch accepts files and nonrecursive directories, reusing one engine and processing one image at a time. Output files must be new, with distinct names within a batch. `--profile` reports host-stage and supported GPU timestamp measurements.

Enable **Reference reconstruction** in Editing or use `--reconstruction` for the [full-resolution noise/detail path](./docs/specs/detail-and-noise.md). `--chroma-noise` and `--luma-noise` select camera-table choices 0–4 (default 2); `--detail` sets residual strength (default 1×). These options also enable reconstruction. Inspection exposes filtered layers, edge weights, a luminance ratio, reconstructed RGB, and a signed detail signal. Advanced reconstruction controls link to their basic presets; unlink a value to override it. CLI accepts repeated `--noise-override KEY=VALUE`. Noise preset 0 still filters; Detail 0 shows the filtered base.

The [render-resource API](./docs/render-resources.md) separates GPU processing, texture preparation, and export. Inspection executes only requested output dependencies and retains selected artifacts. Intermediate checkpoint caching, comparisons, and native menus remain deferred.

## Project documents

Start at [`AGENTS.md`](./AGENTS.md) for repository instructions, specifications, and progress plans.

Maintain the documentation and Rust source indexes with:

```sh
cargo run -p docs-index
cargo run -p docs-index -- --check
cargo test -p docs-index
```
