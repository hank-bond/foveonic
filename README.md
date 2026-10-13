# Foveonic GPU

A personal Merrill X3F processing app with a GUI and a batch CLI, focused on attractive photographs and further editing latitude.

A shared shader-based image engine builds internal processing graphs from selected settings and algorithms. Output recipes target compressed DNGs with editable samples and embedded looks, or TIFFs with white balance, color, and tone baked in.

TIFF rendering and an opt-in metadata-selected noise/detail path are implemented. Original-saturation highlight restoration is also implemented; compressed DNG remains planned.

## Run the GUI

On macOS 26 with Xcode 26 or later, build and launch the local app bundle:

```sh
python3 crates/foveonic-gui/build-macos-app.py -- {X3F_OR_FOLDER}
```

This creates `target/release/Foveonic.app` and lets macOS select the light/dark icon according to system icon appearance settings. Nothing is installed or published. Omit the input to reopen the last folder; `--debug` selects the debug profile and `--build-only` skips launching. Other platforms use `cargo run --release -p foveonic-gui -- {X3F_OR_FOLDER}`.

The egui desktop app uses the owner's [layered-X application icon](./crates/foveonic-gui/assets/AGENTS.md) and keeps an embedded-JPEG organizer, GPU image viewport, and tools visible in three resizable panes. Switch the organizer between icons and compact list; drag tool headers to reorder them or click to collapse. Tools include a draggable navigator with a zoom readout and 100%/200%/400% buttons, current-buffer histogram/parade/waveform scopes, processing controls, and photographic metadata. Double-click the main image or navigator image to toggle Fit and the snap zoom (default 200%); main-image zoom keeps the clicked point under the cursor, while navigator zoom centres the clicked image location. ⌘-click a zoom button (Ctrl-click outside macOS) to set the remembered snap target, marked by a colored underline; ordinary clicks zoom immediately. Sliders have editable values and double-click reset. Focused value boxes support Up/Down nudging (0.1 for floats, 1 for integers unless overridden). One Sensor gain scales the calibrated layers before color conversion. Hover over editing controls, inspection choices, or scopes for [plain-English explanations](./docs/specs/photographic-help.md) of their effects and tradeoffs.

Selection renders automatically using the X3F's recorded WB and color mode; `--wb`/`--mode` override them. Inspection offers visual image buffers, including grayscale sensor layers; non-image node diagnostics are deferred. Export always runs the final TIFF recipe. UI layout, scope/organizer choices, and the last opened folder persist; an explicit launch path takes precedence. Editing changes remain session-only; [per-X3F sidecars](./docs/specs/editing-sidecars.md) are planned. A bar below the image shows capture settings, filename, and visible image-pixel bounds. Capture settings load independently, so they appear while the selected image is still loading. Click Inspect group rows to expand RGB channels or sensor layers. **Δ Difference** compares compatible layer/channel views with their named operation input; Amplify makes small changes visible without changing edits or export. Presentation uses sRGB SDR; monitor-profile correction is pending. Scopes show values in the buffer's signal space.

## Run the CLI

Inspect metadata or render an RGB16 TIFF through the GPU graph. TIFF color space follows the file's sRGB or Adobe RGB camera route.

```sh
cargo run --release -p foveonic -- inspect {INPUT_X3F}
cargo run --release -p foveonic -- render {INPUT_X3F} {OUTPUT_TIFF} --wb Sunlight --mode Standard
cargo run --release -p foveonic -- batch {OUTPUT_DIR} {INPUT_DIR} --wb Sunlight --mode Standard
cargo run --release -p foveonic -- --help
```

Controls include exposure, contrast, shadows, highlights, saturation, and a global sensor gain (`--sensor-gain`, neutral at 1×). Bad-pixel repair and Despeckle are on by default with separate GUI checkboxes; CLI `--no-repair` and `--no-despeckle` disable them. Repair applies complete camera lists and packed clusters, using an automatic camera cutoff in the GUI; CLI `--repair-badness MIN` retains a manual diagnostic override. Highlight restoration also defaults on, independently of repair/despeckle and the tone Highlights slider; `--no-highlight-restoration` disables it. Its [restored layers and original clipping density](./docs/specs/highlight-restoration.md) are inspectable. [Row banding correction](./docs/specs/row-banding.md) follows despeckle and also defaults on; `--no-row-correction` disables it. ColorDQ conditioning (automatic with reconstruction; `--color-dq-gain` overrides it, 0 off), [native high-pass sharpening](./docs/specs/native-sharpening.md), and exact-size bilinear resizing are opt-in; `--help` lists their settings. Batch accepts files and nonrecursive directories, reusing one engine and processing one image at a time. Output files must be new, with distinct names within a batch. `--profile` reports host-stage and supported GPU timestamp measurements.

Enable **Noise and detail** in Editing or use `--reconstruction` for the [noise/detail path](./docs/specs/detail-and-noise.md), including [native reduced local/global filtering](./docs/specs/reduced-reconstruction.md). `--chroma-noise` and `--luma-noise` select camera-table choices 0–4 (default 2); `--detail` sets residual strength (default 1×). These options also enable reconstruction. Inspection exposes filtered layers, edge weights, a luminance ratio, reconstructed RGB, and a signed detail signal. Advanced noise and detail controls link to their basic presets; unlink a value to override it. CLI accepts repeated `--noise-override KEY=VALUE`. Noise preset 0 still filters; Detail 0 shows the filtered base.

The [render-resource API](./docs/render-resources.md) separates GPU processing, texture preparation, and export. Inspection executes only requested output dependencies and retains selected artifacts. Interactive sessions reuse the active edit's unchanged inputs and deliver images before scopes; the navigator is a deferred GPU-prepared small texture. Combined RGB heatmaps, arbitrary branch comparisons, and native menus remain deferred. See the [responsiveness plan](./plans/ui-responsiveness.md) for measurements and remaining limits.

## Project documents

Start at [`AGENTS.md`](./AGENTS.md) for repository instructions, specifications, and progress plans.

Maintain the documentation and Rust source indexes with:

```sh
cargo run -p docs-index
cargo run -p docs-index -- --check
cargo test -p docs-index
```
