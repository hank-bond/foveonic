# Foveonic GPU

A personal Merrill X3F processing app with a GUI and a batch CLI, focused on attractive photographs and further editing latitude.

A shared shader-based image engine builds internal processing graphs from selected settings and algorithms. Output recipes target compressed DNGs with editable samples and embedded looks, or TIFFs with white balance, color, and tone baked in.

Development starts with the shortest useful end-to-end conversion path. Detail recovery, denoising, and highlight reconstruction follow incrementally.

## Run the CLI

Inspect metadata or render an RGB16 TIFF through the GPU graph. TIFF color space follows the file's sRGB or Adobe RGB camera route.

```sh
cargo run --release -p foveonic -- inspect {INPUT_X3F}
cargo run --release -p foveonic -- render {INPUT_X3F} {OUTPUT_TIFF} --wb Sunlight --mode Standard
cargo run --release -p foveonic -- batch {OUTPUT_DIR} {INPUT_DIR} --wb Sunlight --mode Standard
cargo run --release -p foveonic -- --help
```

Controls include exposure, contrast, shadows, highlights, saturation, and RGB gains. ColorDQ conditioning (`--color-dq-gain`, default off), isolated bad-pixel repair, Gaussian unsharp masking, and exact-size bilinear resizing are opt-in; `--help` lists their settings. Batch accepts files and nonrecursive directories, reusing one engine and processing one image at a time. Output files must be new, with distinct names within a batch. `--profile` reports host-stage and supported GPU timestamp measurements.

The [render-resource API](./docs/render-resources.md) separates GPU processing, texture preparation, and export. Interactive result caching is deferred.

## Project documents

Start at [`AGENTS.md`](./AGENTS.md) for repository instructions, specifications, and progress plans.

Maintain the documentation and Rust source indexes with:

```sh
cargo run -p docs-index
cargo run -p docs-index -- --check
cargo test -p docs-index
```
