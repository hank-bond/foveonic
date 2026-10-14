# Foveonic
<img src="crates/foveonic-gui/assets/logo.svg" width="200" alt="Application Icon">
An X3F processing app with both a GUI and a batch CLI, focused on processing speed, flexibility, and attractive results.

## Supported Formats
TIFF exports bake all processing into the pixel data, trading maximum compatibility for reduced editing latitude.  This is because tone and color changes are fully baked-in, with downstream editors operating "on top of" those changes.  

DNG separates out the base linear image data—only baking in some corrections and detail enhancement—and compresses all tone and color changes into a curve and matrix in the DNG metadata.  Your DNG editor will then take this metadata and apply it on top of the linear pixel data as part of its rendering pipeline.  This makes it a more flexible format for editing further, but does mean that not every editor will render it exactly as it looked in Foveon upon export.

## Rendering Techniques
All correction and enhancement metadata that exists in the X3F file is made use of.  Effort has been made to align with the Sigma's intended processing of the files wherever possible.  Additional controls are made wherever available to alter the processing approaches on a per-image basis.

The processing pipeline has been largely developed using WebGPU shaders, in order to provide a significantly improved editing experience.  This also benefits batch CLI render times.  

## Requirements
Development is currently done on a modest Apple M1 Pro with 14 GPU Cores.  The current focus is targeting MacOS.  The tooling chosen should be multi-platform compatible, but focus is not currently on delivering that.  On a machine of that caliber, you may expect full pipeline render times around 900-1600ms, with partial re-renders closer to 200-400ms using cached pipeline partials.

Expect application memory to average around 2GB, with transient operations peaking under 4GB.  Configuration to reduce memory requirements at the cost of increased processing speed will be made available at a later time.

## Build and use

For the macOS app, use macOS 26+, Rust 1.88+ with Cargo, Python 3, and Xcode 26+ with its command-line tools selected. Run these commands from the repository root.

### GUI

Build and launch with an X3F file or folder:

```sh
python3 crates/foveonic-gui/build-macos-app.py -- path/to/photo.x3f
```

This creates `target/release/Foveonic.app` without installing it. Use `--build-only` before `--` to build without launching. Omit the input to reopen the last folder. Select a photograph, adjust its settings, and use **Export TIFF** or the experimental **Export DNG**. Edits save automatically to a sidecar beside the X3F.

### CLI

Render a TIFF:

```sh
cargo run --release -p foveonic -- render path/to/photo.x3f output.tiff --wb Sunlight --mode Standard
```

The output file must not already exist. Use `cargo run --release -p foveonic -- --help` for batch processing and other options. The CLI does not load GUI editing sidecars.

Sample X3Fs are available in the [Example photographs release](https://github.com/hank-bond/foveonic/releases/tag/example-photographs). Download a photograph and open it in the GUI to try the app.
