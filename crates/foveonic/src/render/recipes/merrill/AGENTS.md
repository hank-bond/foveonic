---
summary: "Merrill recipes share sensor corrections and technical color before photographic or editable DNG output."
---

# Merrill recipes

Keep Merrill preparation and composition here. Technical construction stops when all requested intermediate outputs exist. Color and Monochrome supply their code scale and tone context to the parent shared linear-edit recipe. Photographic rendering then adds the camera curve and saturation. Keep Merrill calibration and dedicated reconstruction here. The parent DNG recipe transfers the shared edit gain onto the technical base and leaves camera appearance to metadata.

Monochrome branches after the same sensor corrections, then uses its dedicated mixing, filtering and detail operations. Numerical operations stay beside their shaders and receive functional parameters.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[appearance.rs](./appearance.rs)** Resolves Merrill camera coordinates and selected photographic appearance for DNG.
- **[dng_base.rs](./dng_base.rs)** Composes Merrill technical DNG samples independently of shared photographic edits.
- **[monochrome.rs](./monochrome.rs)** Authors sensor-layer monochrome filtering and finishing without the color reconstruction path.
- **[photographic.rs](./photographic.rs)** Applies the camera tone curve and saturation after shared linear photographic edits.
- **[preview.rs](./preview.rs)** Prepares Merrill's half-size calibrated preview grid without changing source metadata.
- **[region.rs](./region.rs)** Maps oriented Merrill detail requests onto native sensor rectangles and processing tiles.
- **[technical.rs](./technical.rs)** Builds Merrill sensor corrections and technical color with lazy inspection preparation.

<!-- INDEX:END -->
