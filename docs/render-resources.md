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

`view_texture` creates an oriented/cropped RGBA32Float texture on GPU, without clipping or CPU readback. Its optional size changes presentation only. The texture retains working primaries and is not monitor encoded. The future presentation shader must supply color conversion and display transfer encoding. RGBA32Float requires manual sampling unless the device enables float32 filtering.

GUI callers can select a surface-compatible adapter and pass its device and queue to `Engine::from_device`. Request limits sufficient for Merrill's roughly 184 MiB full-sensor storage bindings; default WebGPU storage-binding limits can be too small. The engine and GUI must submit through the same device/queue. Optional timestamp profiling requires the device's `TIMESTAMP_QUERY` feature.

## Ownership

Images and textures are owned snapshots. Consumers must only read their storage. Retaining one while another render runs does not make it eligible for scratch reuse. Resources used by an export must come from that engine.

The render result shares original metadata through `Arc`, without retaining decoded CPU samples. Retain `Source` separately if settings will be reprocessed. Interactive intermediate caching and invalidation are not implemented.

Scratch pooling is execution-local. Buffers become reusable only after their final graph read has been submitted, and queued overwrites follow those reads. Imported images and retained roots are excluded. Histogram/atomic buffers and encoded outputs are not pooled as floating image scratch.

Current calibration and repairs still operate in original sensor coordinates. Texture resizing is presentation preparation, not a reduced-resolution processing pipeline.

## Profiling

Use `--profile` with render or batch. Reports distinguish host wall timings from GPU compute-pass timestamps; plane-worker timings overlap and must not be summed. GPU timestamps exclude buffer allocation, uploads, implicit clears, readback, and packaging. Readback wall time includes any GPU work still pending at that point.

Resolving timestamps waits at graph boundaries, so profiled runs can change CPU/GPU overlap. Use ordinary unprofiled runs for end-to-end throughput and peak-memory comparisons. Devices without timestamp support report host measurements only.
