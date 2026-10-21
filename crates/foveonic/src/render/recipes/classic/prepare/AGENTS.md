---
summary: "Classic preparation separates calibration, color transformation, reconstruction, and appearance."
---

# Preparation responsibilities

The parent module owns shared contracts and metadata helpers. These modules prepare parameters only when the recipe reaches their stage. Preserve lazy inspection and keep pixel arithmetic in processing nodes.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[appearance.rs](./appearance.rs)** Prepares Classic root detail, photographic response, and editable DNG appearance.
- **[calibration.rs](./calibration.rs)** Prepares Classic sensor correction, geometry, and white-balance-derived camera inputs.
- **[color.rs](./color.rs)** Prepares Classic selected camera matrices and fixed-point nonlinear color conversion.
- **[reconstruction.rs](./reconstruction.rs)** Prepares Classic chroma smoothing and gain-preserving logarithmic reconstruction controls.

<!-- INDEX:END -->
