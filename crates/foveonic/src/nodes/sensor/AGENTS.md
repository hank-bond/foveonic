---
summary: "Sensor nodes prepare calibration and original-layer corrections before reconstruction."
---

# Sensor operations

Keep original samples and clipping provenance immutable. Preserve ordered repairs and stage-specific WB/noise preparation. Calibration also supplies the existing camera-color and geometry parameters. Shared RGB space definitions live in the color-space module and remain exposed through the image API.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[banding.rs](./banding.rs)** Prepares coordinate-only row-correction weights while keeping photo measurements on the GPU.
- **[blue_despeckle.rs](./blue_despeckle.rs)** Repairs immutable blue planes with paired dark-speckle, general-neighborhood, or neighbor-range equations.
- **[blue_mismatch.rs](./blue_mismatch.rs)** Repairs odd-phase blue runs after validating the source counter initialization precondition.
- **[blue_pattern.rs](./blue_pattern.rs)** Removes directional blue phase patterns through explicit coarse masks and ordered pair corrections.
- **[blue_refresh.rs](./blue_refresh.rs)** Refreshes RGB blue from four spatial blue phases while preserving red and green.
- **[calibration.rs](./calibration.rs)** Resolves metadata-derived calibration, white balance, camera color, and image geometry.
- **[camera_repair.rs](./camera_repair.rs)** Prepares camera repair lists so the GPU can fix pixels in the camera's order.
- **[chroma_clusters.rs](./chroma_clusters.rs)** Repairs ordered RGB clusters by matching nearby valid three-by-three templates.
- **[chroma_repair.rs](./chroma_repair.rs)** Repairs ordered RGB defect records with directional and masked-neighborhood passes.
- **[color_field.rs](./color_field.rs)** Owns original evidence, GPU brightness/color solves, and native highlight application.
- **[column_gain.rs](./column_gain.rs)** Applies six-channel per-column affine gains while preserving packed sensor cells.
- **[despeckle.rs](./despeckle.rs)** Prepares camera thresholds for GPU speckle repair without changing detection order.
- **[estimated_iso.rs](./estimated_iso.rs)** Estimates processing ISO from camera histograms and reference metadata.
- **[fixed_point.rs](./fixed_point.rs)** Executes signed fixed-point layer correction and nonlinear matrix transforms with explicit numerical contracts.
- **[fppd.rs](./fppd.rs)** Corrects sparse RGB gains and paired mono defects with explicit numerical grids.
- **[highlight.rs](./highlight.rs)** Keeps original clipping flags so GPU highlight recovery can use trustworthy neighboring samples.
- **[highlight_restoration.rs](./highlight_restoration.rs)** Reconstructs saturated RGB through directional interpolation and inverse-camera neutralization.
- **[hlp_calibration.rs](./hlp_calibration.rs)** Applies sparse blue correction and ordered shading while retaining pre-color scratch samples.
- **[luma_cluster.rs](./luma_cluster.rs)** Repairs ordered mono clusters with parallel candidate scoring and stable source-order ties.
- **[mod.rs](./mod.rs)** Prepares and corrects original camera layers before rendering-specific reconstruction.
- **[ordered_blue_repair.rs](./ordered_blue_repair.rs)** Groups ordered mono defect records into independent GPU batches without changing their dependencies.
- **[phase_extraction.rs](./phase_extraction.rs)** Separates six-sample cells into RGB and spatial blue codes with optional affine calibration.
- **[phase_gain.rs](./phase_gain.rs)** Resizes interleaved phase gain maps and corrects blue samples below a supplied threshold.
- **[red_detail.rs](./red_detail.rs)** Adds signed red-channel detail using a separable five-row binomial support.
- **[repair.rs](./repair.rs)** Selects camera-derived cutoffs and preserves ordered camera records for masked GPU correction.
- **[rgb_despeckle.rs](./rgb_despeckle.rs)** Removes isolated RGB excursions using immutable masked five-by-five neighborhoods.
- **[signed_repair.rs](./signed_repair.rs)** Repairs signed RGB code values with ordered packed records or a vertically ordered despeckle sweep.
- **[sparse_repair.rs](./sparse_repair.rs)** Repairs sparse RGB and four-phase blue defects through ordered ghost and pattern passes.
- **[spatial_tables.rs](./spatial_tables.rs)** Prepare Merrill spatial gain grids from camera byte tables, aperture, focus, and chief-ray angle.

<!-- INDEX:END -->
