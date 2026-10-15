---
summary: "Photographic and DNG-base builders compose shared technical nodes before one executor runs the graph."
---

# Authored recipes

Shared technical construction stops as soon as all requested intermediate outputs exist. Resolve camera modes only when the color stage needs them. The photographic builder adds pixel exposure, tone, and saturation. DNG uses uncompensated working color and stops at the linear technical base, leaving selected appearance to [DNG metadata](../../dng/AGENTS.md).

Rendering mode and export format are separate choices. Monochrome algorithms and monochrome DNG representation remain future work.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[dng_base.rs](./dng_base.rs)** Builds a fresh linear technical base while leaving selected appearance to DNG metadata.
- **[mod.rs](./mod.rs)** Selects authored photographic and DNG-base graphs before shared execution.
- **[photographic.rs](./photographic.rs)** Completes photographic RGB with selected camera appearance, exposure, tone, and saturation.
- **[shared.rs](./shared.rs)** Builds the shared original-source technical graph with lazy inspection preparation.

<!-- INDEX:END -->
