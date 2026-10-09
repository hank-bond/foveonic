# Foveonic GPU

An X3F-to-DNG processor: Sigma-like tone and color, with modern treatment of sensor noise, detail and dynamic range, and maximum useful editing latitude.

## Brief

- Use three-layer sensor data for noise reduction, detail recovery, highlight recovery and monochrome conversion.
- Use X3F calibration and color-mode metadata as the foundation, retaining distinct sRGB/AdobeRGB source-profile paths.
- Bake sensor improvements into linear samples; keep color and tone presentation in DNG profiles.
- Preserve signed values and useful headroom with float32 processing intermediates.

## Initial scope

One statically configured CLI pipeline, implemented with Rust and `wgpu`/WGSL: Metal on macOS, Vulkan on Linux. Target roughly 1 GiB typical working memory and 2 GiB peak.

Named stages can write diagnostic images, masks and measurements for photographic review. These boundaries support future interactive inspection; the initial product is a converter, not an editor.
