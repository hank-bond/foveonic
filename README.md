# Foveonic
<img src="crates/foveonic-gui/assets/logo.svg" width="200" alt="Application Icon">
An X3F processing app with both a GUI and a batch CLI, focused on processing speed, flexibility, and attractive results.  This is not a hand-made project.

----

![Foveonic editing an X3F photograph](crates/foveonic-gui/assets/gui-screenshot.webp)

## Supported Formats
TIFF exports bake all processing into the pixel data, trading maximum compatibility for reduced editing latitude.  This is because tone and color changes are fully baked-in, with downstream editors operating "on top of" those changes.

DNG separates out the base linear image data—only baking in some corrections and detail enhancement—and compresses all tone and color changes into a curve and matrix in the DNG metadata.  Your DNG editor will then take this metadata and apply it on top of the linear pixel data as part of its rendering pipeline.  This makes it a more flexible format for editing further, but does mean that not every editor will render it exactly as it looked in Foveon upon export.

The Classic experiment also renders low-ISO SD15, DP2 and DP1s format-30 originals to TIFF and editable color DNG. Use `--reconstruction --sharpen 1 --wb Sunlight --mode Standard` for the validated reference baseline. Classic retains linear ROMM working output and uses shared display and TIFF color conversion. Earlier encodings, high-ISO reconstruction, Classic monochrome and custom noise controls remain unsupported. Classic DNG embeds an approximate photographic appearance. See the [experiment requirements](docs/specs/classic-rendering.md).

## Rendering Techniques
All correction and enhancement metadata that exists in the X3F file is made use of.  Effort has been made to align with the Sigma's intended processing of the files wherever possible.  Additional controls are made wherever available to alter the processing approaches on a per-image basis.

The processing pipeline has been largely developed using WebGPU shaders, in order to provide a significantly improved editing experience.  This also benefits batch CLI render times.

Choose **Color** or **Monochrome** in the editor. Monochrome mixes the original sensor layers with a triangular R/G/B selector before dedicated filtering steps.  This is different from using a monochrome mode in your editor becuase it operates on the sensor layers *before* they are converted into RGB space.  The primary benefit is that you can use just Layer 3 (B) which has a much better signal-to-noise ratio than the others.

Monochrome DNG bakes the mix into linear neutral samples while keeping exposure and tone in metadata.

Highlights offers **Classic** (default), **Color Field: Local**, and **Color Field: Global**. Classic retains its threshold control. Local uses 64 local brightness sweeps, while Global solves globally for finer gradients at higher cost. Method comes first, followed by a shared Highlights slider and enable checkbox. Classic retains A. Both Color Field methods default to 0, preserving the original reconstruction, without a redundant A button. Increasing selectivity from 0 toward 1 rejects more inconsistent evidence without increasing the original neutral weights. See the [comparison workflow](docs/color-field-evaluation.md).

## Requirements
Development is currently done on a modest Apple M1 Pro with 14 GPU Cores.  The current focus is targeting MacOS.  The tooling chosen should be multi-platform compatible, but focus is not currently on delivering that.  On a machine of that caliber, you may expect full pipeline render times around 900-1600ms, with partial re-renders closer to 200-400ms using cached pipeline partials.

Expect application memory to average around 2GB, with transient operations peaking under 4GB.  Configuration to reduce memory requirements at the cost of increased processing speed will be made available at a later time.

## Build and use

For the macOS app, use macOS 26+, Rust 1.88+ with Cargo, and Xcode 26+ with its command-line tools selected. Run these commands from the repository root.

### GUI

Build and launch with an X3F file or folder:

```sh
cargo xtask run -- path/to/photo.x3f
```

This creates `target/release/Foveonic.app` without installing it. Use `cargo xtask bundle` to build without launching, or add `--debug` before `--` for a debug build. Custom Cargo target directories are respected. Omit the input to reopen the last folder. Select a photograph, adjust its settings, and use **Export TIFF** or the experimental **Export DNG**. Edits save automatically to a sidecar beside the X3F.

### CLI

Render a color TIFF:

```sh
cargo run --release -p foveonic -- render path/to/photo.x3f output.tiff --wb Sunlight --mode Standard
```

Render blue-layer monochrome, or export its linear DNG:

```sh
cargo run --release -p foveonic -- render path/to/photo.x3f mono.tiff --wb Sunlight --mono-mix 0,0,1
cargo run --release -p foveonic -- export-dng path/to/photo.x3f mono.dng --wb Sunlight --mono-mix 0,0,1
```

`--rendering monochrome` selects the equal-layer mix. `--mono-noise` and `--mono-banding` select presets 0–4. Estimated ISO above 6400 is currently unsupported for monochrome column correction.

TIFF exports accept `--tiff-color-space srgb`, `adobe-rgb`, or `prophoto-rgb`. Omit it to preserve the camera's stored space, falling back to sRGB when none is recorded. Use `--sharpen auto` for camera-derived sharpening or a numeric amount for manual sharpening. Sharpening defaults to Off.

Add `--size 600x400` to resize a TIFF export to exact dimensions using reference photographic resampling. This also works for TIFF batches. Omit it for native dimensions.

The output file must not already exist. Use `cargo run --release -p foveonic -- --help` for batch processing and other options. The CLI does not load GUI editing sidecars.

Sample X3Fs are available in the [Example photographs release](https://github.com/hank-bond/foveonic/releases/tag/example-photographs). Download a photograph and open it in the GUI to try the app.

## Releases

Release-tag pushes build an unsigned Apple Silicon app for macOS 26+ and attach it to a draft GitHub Release. See the [changelog](CHANGELOG.md) and [release checklist](docs/releases.md). Publication remains manual.

## Checks

```sh
cargo test --offline --workspace
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run --offline -p docs-index
cargo run --offline -p docs-index -- --check
```

Run GPU and original-file tests serially with `FOVEONIC_FIXTURE_DIR` pointing to the local X3F fixtures:

```sh
FOVEONIC_FIXTURE_DIR=path/to/x3f cargo test --offline --release --workspace -- --ignored --test-threads=1
```

Classic original-file checks additionally use `CLASSIC_X3F`, `CLASSIC_DECODED_REFERENCE`, `CLASSIC_CORRECTION_REFERENCE` and `CLASSIC_COLOR_REFERENCE`. Set `CLASSIC_REFERENCE_DIR` for the composed reference comparison and `FOVEONIC_LOG_CAPTURE` for the complete FIR boundary. These are local research inputs. Bounded numerical fixtures are checked in.
