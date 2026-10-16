# Foveonic

<img src="crates/foveonic-gui/assets/logo.svg" width="200" alt="Application icon">

A Merrill X3F processing app with a GUI and batch CLI, focused on speed, editing flexibility, and photographic results.

## Output formats

TIFF bakes the selected processing into pixels. DNG retains a linear technical base and stores the selected appearance in metadata. Technical corrections and detail processing are baked into the DNG samples. Receiving editors may render the embedded appearance differently.

## Rendering

Choose **Color** or **Monochrome** in the editor. Monochrome mixes the original sensor layers with a triangular R/G/B selector before dedicated filtering steps.

Monochrome DNG bakes the mix into linear neutral samples while keeping exposure and tone in metadata.

Photographic processing runs in GPU shaders. Camera metadata supplies calibration, correction parameters, and color looks.

## Platform and performance

Development targets macOS on an Apple M1 Pro with 14 GPU cores. Cross-platform runtime support is not yet verified.

Earlier color measurements were roughly 900–1600 ms for full processing and 200–400 ms for cached edits. Three full-sensor monochrome originals measured 600–900 ms for warm completed processing without the interactive cache. Source loading and export are separate. 

Normal editing targets about 2 GB of memory, with transient peaks below 4 GB. These are targets, not enforced limits.

## Build and use

For the macOS app, use macOS 26+, Rust 1.88+ with Cargo, Python 3, and Xcode 26+ with its command-line tools selected. Run these commands from the repository root.

### GUI

Build and launch with an X3F file or folder:

```sh
python3 crates/foveonic-gui/build-macos-app.py -- path/to/photo.x3f
```

This creates `target/release/Foveonic.app` without installing it. Use `--build-only` before `--` to build without launching. Omit the input to reopen the last folder. Select a photograph, adjust its settings, and use **Export TIFF** or the experimental **Export DNG**. Edits save automatically to a sidecar beside the X3F.

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

The output file must not already exist. Use `cargo run --release -p foveonic -- --help` for batch processing and other options. The CLI does not load GUI editing sidecars.

Sample X3Fs are available in the [Example photographs release](https://github.com/hank-bond/foveonic/releases/tag/example-photographs). Download a photograph and open it in the GUI to try the app.

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
