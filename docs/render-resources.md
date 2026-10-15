---
summary: "The render API returns owned full-sensor floating images with separate logical views, GPU texture preparation, and export consumers."
kind: reference
---

# Use render resources

The contract is specified in [`render-resources.md`](./specs/render-resources.md).

## Render, display, and export

```rust
let engine = foveonic::render::Engine::new()?;
let source = foveonic::source::Source::open(input)?;
let settings = foveonic::render::Settings::new("Sunlight", "Standard");
let mut profile = foveonic::profile::Profile::default();
let image = engine.render(&source, &settings, &mut profile)?;
let texture = engine.view_texture(&image, Some([800, 1200]), &mut profile)?;
engine.write_tiff(&image, output, &mut profile)?;
```

`RenderedImage::description()` identifies physical storage: full-sensor, interleaved RGB32Float in tone-mapped sRGB or Adobe RGB linear primaries. `view()` separately describes the active crop, rotation, and requested output dimensions. White balance and rendering controls are baked into this TIFF-style result. Its original metadata and selected settings are retained separately.

`view_texture` creates an oriented/cropped RGBA32Float texture on GPU, preserving signed values and headroom in the working primaries. Its optional size sets presentation dimensions. The egui presentation shader supplies working-RGB conversion to sRGB SDR and display transfer encoding; monitor-profile correction is pending. RGBA32Float requires manual sampling unless the device enables float32 filtering.

GUI callers can select a surface-compatible adapter and pass its device and queue to `Engine::from_device`. Request limits sufficient for Merrill's roughly 184 MiB full-sensor storage bindings and 235 MiB processing-tile bindings when reconstruction is enabled; default WebGPU storage-binding limits can be too small. The engine and GUI must submit through the same device/queue. Optional timestamp profiling requires the device's `TIMESTAMP_QUERY` feature.

## Inspect named outputs

Image content is `Content::Image(Box<RenderedImage>)`; boxing keeps artifact variants small while the snapshot continues owning its immutable samples and configuration.

`Engine::render_artifacts(source, settings, names, profile)` accepts stable output names from `inspection::output_names(settings)`. `final` selects the final photographic result; other names select sensor/calibrated layers, RGB stages, sharpening fields, or black-estimation diagnostics. Only requested dependencies execute, and preparation stops once all requested earlier outputs are available. The GUI uses `inspection::image_outputs(settings)` to offer visual buffers with sensor-layer/RGB/scalar component meanings; the engine retains table outputs for consumers, with a separate node-data diagnostic UI deferred.

`inspection::comparison_input(settings, output)` identifies compatible operation inputs for signed layer/channel comparisons. The GUI requests output and input together through the render session, retains their oriented GPU views, and subtracts samples in the presentation shader before display mapping. Difference amplification and component changes reuse those views; scopes still measure the output. Noise/column layers share the pre-filter input; reconstruction compares with its RGB base. Cross-unit color/tone conversions and diagnostic side outputs are not enabled comparisons. Combined RGB heatmaps remain a subsequent slice.

`Settings::reconstruction = Some(noise::Controls::default())` enables the [reference-style full-resolution path](./specs/detail-and-noise.md). Its public roots are `noise.layers`, `noise.edges`, `detail.ratio`, `reconstruction.rgb`, and `detail.signal`, alongside the existing camera-color stages. Typed tile atlases stay internal. Edge-only inspection skips directional smoothing; ratio/signal inspection skips residual mixing; signal-only inspection also skips reconstruction and tone.

An artifact records its actual output name, original metadata, and settings snapshot. Image contents retain owned GPU storage. Engine consumers can read back small diagnostic tables. The black-estimation node exposes `black.levels` and `black.histogram` independently: requesting the histogram alone skips black-mean computation. Internally needed histograms remain transient when not requested as roots.

Multiple requested roots have independent retention. Imported snapshots validate their layout and space and retain their owned storage. Scalar32Float storage and scalar-field texture preparation support future image-sized masks; highlight-mask generation is pending. Scalar fields occupy four bytes per pixel; RGB occupies twelve. Source/calibrated layer visualization is grayscale with an initial 0–4095 range; scalar coverage maps directly to display gray in 0–1. `Space::LuminanceRatio` displays 0–2 with neutral 1 at middle gray. `Space::SignedDetail` preserves camera-code samples and displays −1000…1000 with zero at middle gray. Layer/channel selection controls presentation. Image views use the logical active crop and orientation.

## Analyze the current buffer

`Engine::scopes(image, region, profile)` analyzes the retained floating RGB/scalar buffer. `None` selects the whole image. `Some([left, top, right, bottom])` selects a region in cropped, rotated output pixels, with right and bottom excluded. Bounds must fit `image.view().extent`, including resized outputs.

A grid of at most 512×512 samples keeps the region's aspect ratio. Histograms have 256 bins. Waveforms have 256×256 cells, with each sample's weight shared bilinearly between nearby cells. `Scopes::density()` returns weights in `DENSITY_SCALE` units per sample. Divide by that constant to get sample units. Splitting preserves total weight. Histogram and outside counts remain integer sample counts.

Only about 1 MiB of counts and weights is read back. The image stays on the GPU. Run analysis on the background worker because readback waits for the GPU.

RGB scopes preserve working primaries; RGB luminance uses matching sRGB or Adobe RGB weights. Pre-tone `Space::LinearRgb` stores camera-code values and uses a 0–4095 inspection range. Tone-mapped RGB uses linear 0–1. Sensor/calibrated scopes normalize camera codes by 4095, and scalar scopes use 0–1. Luminance-ratio scopes use 0–2, and signed detail scopes use −1000…1000 camera codes. Analysis uses the buffer's values directly. `Scopes::space` and `samples` identify the signal and sample grid. Diagnostic artifacts contain their own tables.

## Ownership

Images and textures are immutable owned snapshots. Their storage stays reserved for their lifetime, including during subsequent renders. Export resources must belong to the exporting engine.

The render result shares original metadata through `Arc`. `Source` owns decoded CPU samples; retain it for reprocessing settings. The GUI retains one decoded source and its selected artifact, while coalescing pending requests and waiting for GPU completion on the worker. Interactive sessions retain one active edit's unchanged continuation boundary, with processing-owned keys and invalidation.

Scratch pooling is execution-local and limited to transient floating-image buffers. Graph values, cache entries and retained images share read guards. A write guard requires exclusive ownership; scratch rejects allocations with retained readers. Dispatch bindings declare `Read` or `Write` access and hold their guards through synchronous command submission. On the shared ordered queue, queued overwrites follow prior reads without an extra completion fence. Raw processing buffer handles remain private to the GPU/storage layer.

`Engine::readback_image(image)` provides explicit diagnostic storage readback with engine-owner validation. It replaces direct public buffer access and contributes to engine activity counters; benchmark callers keep it outside measured render/export intervals. It is not a display or photographic-processing path.

Ownership assertions identify the allocation, write generation, size, label and last submitted writer. Tests additionally compare explicit binding modes against the shader compiler's entry-point write analysis. There is no production event-tracing or intermediate-capture path.

Calibration and repairs operate in original sensor coordinates. Texture resizing sets presentation dimensions after processing.

## Profiling

Use `--profile` with render or batch. Reports include host wall timings and GPU compute-pass timestamps. Plane-worker timings measure overlapping work. GPU timestamps exclude buffer allocation, uploads, implicit clears, readback, and packaging. Readback wall time includes any GPU work still pending at that point.

Resolving timestamps waits at graph boundaries, so profiled runs can change CPU/GPU overlap. Use ordinary unprofiled runs for end-to-end throughput and peak-memory comparisons. Devices without timestamp support report host measurements only.
