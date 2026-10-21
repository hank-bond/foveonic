---
summary: "Quattro preparation owns camera resources and source-derived controls beside its multiresolution recipe."
---

# Quattro preparation

Keep camera version and calibration selection here. Operations receive numerical parameters and immutable tables. Raw extraction, offset correction and phase gain are separate inspection boundaries. They do not establish complete preprocessing or photographic rendering.

The parent recipe wires stages and stops once the requested outputs exist. Calibration preparation resolves metadata into operation parameters. The inspection catalogue owns output names and channel meanings. Add later reconstruction preparation here when its numerical stages are implemented. Reuse a signal type across stages when its units and channel meaning remain the same.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[calibration.rs](./calibration.rs)** Resolves Quattro camera calibration into numerical extraction and phase-gain parameters.
- **[inspection.rs](./inspection.rs)** Describes Quattro inspection outputs and their sensor component meanings.
- **[resources.rs](./resources.rs)** Loads explicitly supplied blue calibration maps and prepares aperture/focus blends.
- **[stage1.rs](./stage1.rs)** Prepares the ordinary full-size two-pass noise controls from DP2 Quattro metadata.

<!-- INDEX:END -->
