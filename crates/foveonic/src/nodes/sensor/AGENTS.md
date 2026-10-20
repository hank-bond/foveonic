---
summary: "Sensor nodes prepare calibration and original-layer corrections before reconstruction."
---

# Sensor operations

Keep original samples and clipping provenance immutable. Preserve ordered repairs and stage-specific WB/noise preparation. Calibration also supplies the existing camera-color and geometry parameters. Its public image-space types remain exposed through the image API.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[banding.rs](./banding.rs)** Prepares coordinate-only row-correction weights while keeping photo measurements on the GPU.
- **[calibration.rs](./calibration.rs)** Resolves metadata-derived calibration, white balance, camera color, and image geometry.
- **[camera_repair.rs](./camera_repair.rs)** Prepares camera repair lists so the GPU can fix pixels in the camera's order.
- **[color_field.rs](./color_field.rs)** Owns original evidence, GPU brightness/color solves, and native highlight application.
- **[despeckle.rs](./despeckle.rs)** Prepares camera thresholds for GPU speckle repair without changing detection order.
- **[estimated_iso.rs](./estimated_iso.rs)** Estimates processing ISO from camera histograms and reference metadata.
- **[highlight.rs](./highlight.rs)** Keeps original clipping flags so GPU highlight recovery can use trustworthy neighboring samples.
- **[mod.rs](./mod.rs)** Prepares and corrects original camera layers before rendering-specific reconstruction.
- **[repair.rs](./repair.rs)** Selects camera-derived cutoffs and preserves ordered camera records for masked GPU correction.
- **[spatial_tables.rs](./spatial_tables.rs)** Prepare Merrill spatial gain grids from camera byte tables, aperture, focus, and chief-ray angle.

<!-- INDEX:END -->
