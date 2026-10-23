---
summary: "Quattro preparation owns camera resources and source-derived controls beside its multiresolution recipe."
---

# Quattro preparation

Keep camera version and calibration selection here. Operations receive numerical parameters and immutable tables. Raw extraction, preprocessing, color reconstruction and photographic response have separate inspection boundaries. A decoded camera does not imply a completed rendering recipe.

The parent recipe wires stages and stops once the requested outputs exist. Calibration preparation resolves metadata into operation parameters. The inspection catalogue owns output names and channel meanings. Stage1, Stage2, fusion and Stage3 preparation live in their stage modules. Preserve source histories explicitly when the reference implementation reuses buffers. Reuse a signal type across stages when its units and channel meaning remain the same.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[appearance.rs](./appearance.rs)** Prepares supported color-mode responses from exposure and tone controls.
- **[calibration.rs](./calibration.rs)** Resolves Quattro extraction, phase gain and selected rendering color calibration.
- **[color_shading.rs](./color_shading.rs)** Decodes and combines ordinary metadata shading grids for explicit channels and white balance.
- **[controls.rs](./controls.rs)** Resolves saved Quattro adjustments, preset choices and supported control domains.
- **[defects.rs](./defects.rs)** Expands camera defect records and prepares ordinary DP chroma repair controls.
- **[dng.rs](./dng.rs)** Prepares camera-like Quattro DNG coordinates and a deferred sampled appearance.
- **[highlights.rs](./highlights.rs)** Prepares ordinary enabled highlight restoration and odd-phase blue mismatch controls.
- **[hlp.rs](./hlp.rs)** Prepares the selected ordinary DP sparse blue LUT and ordered ordinary/HP shading gains.
- **[inspection.rs](./inspection.rs)** Describes Quattro inspection outputs and their sensor component meanings.
- **[luma_repair.rs](./luma_repair.rs)** Prepares bounded luma defect catalogues and ordinary high-plane repair controls.
- **[mcc.rs](./mcc.rs)** Prepares color-mode chroma sectors from the selected ordinary DP metadata table.
- **[monochrome.rs](./monochrome.rs)** Prepares monochrome channel filtering, reconstruction and finishing controls.
- **[optical.rs](./optical.rs)** Prepares the DP1 optical color correction independently of graph construction.
- **[region.rs](./region.rs)** Plans aligned regional reconstruction after full-frame Quattro calibration and color preparation.
- **[repair.rs](./repair.rs)** Prepares the ordinary DP sparse-repair noise model and correction switch.
- **[resources.rs](./resources.rs)** Loads bundled or explicitly overridden blue calibration maps and prepares aperture/focus blends.
- **[sd.rs](./sd.rs)** Prepares interchangeable-lens Quattro sparse phase-detection correction tables.
- **[stage1.rs](./stage1.rs)** Prepares ordinary full-size Quattro two-pass noise controls and exposure-guided filtering.
- **[stage2.rs](./stage2.rs)** Prepares and composes ordinary DP color shading, raw response and chroma sectors.
- **[stage2b.rs](./stage2b.rs)** Prepares Full and reduced preview reconstruction, luminance filtering and residual detail.
- **[stage3.rs](./stage3.rs)** Prepares local contrast and adaptive sharpening for Full and reduced preview grids.

<!-- INDEX:END -->
