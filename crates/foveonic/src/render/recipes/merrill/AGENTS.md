---
summary: "Merrill recipes share sensor corrections and technical color before photographic or editable DNG output."
---

# Merrill recipes

Keep Merrill preparation and composition here. Technical construction stops when all requested intermediate outputs exist. Photographic rendering adds tone and saturation. Editable DNG stops at the linear technical base and leaves appearance to DNG metadata.

Monochrome branches after the same sensor corrections, then uses its dedicated mixing, filtering and detail operations. Numerical operations stay beside their shaders and receive functional parameters.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[dng_base.rs](./dng_base.rs)** Builds a fresh linear technical base while leaving selected appearance to DNG metadata.
- **[monochrome.rs](./monochrome.rs)** Authors sensor-layer monochrome filtering and finishing without the color reconstruction path.
- **[photographic.rs](./photographic.rs)** Completes photographic RGB with selected camera appearance, exposure, tone, and saturation.
- **[technical.rs](./technical.rs)** Builds Merrill sensor corrections and technical color with lazy inspection preparation.

<!-- INDEX:END -->
