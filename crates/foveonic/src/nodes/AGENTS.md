---
summary: "Processing nodes group sensor corrections, color and monochrome reconstruction, and finishing with their shaders and tests."
---

# Processing nodes

Keep metadata preparation and GPU operations together by photographic responsibility. Pixel arithmetic belongs in the adjacent WGSL files. Recipes choose operation order in [render/recipes](../render/recipes/AGENTS.md), while [graph operations](../graph/AGENTS.md) define typed connections and output identities.

Share operations where sample meanings and algorithms match. The monochrome group owns dedicated sensor-layer mixing and filtering. It must not reuse color reconstruction merely to desaturate its result.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[color/](./color/AGENTS.md)** Color nodes own camera modes, noise preparation, tiled reconstruction, pattern conditioning, and ColorDQ.
- **[finish/](./finish/AGENTS.md)** Finishing nodes prepare tone and resampling beside sharpening and encoding shaders.
- **[monochrome/](./monochrome/AGENTS.md)** Monochrome nodes mix sensor layers before dedicated filtering, column correction, signed detail, and neutral gain replication.
- **[sensor/](./sensor/AGENTS.md)** Sensor nodes prepare calibration and original-layer corrections before reconstruction.

### Source files

- **[mod.rs](./mod.rs)** Groups photographic operations by sensor correction, color or monochrome reconstruction, and finishing.

<!-- INDEX:END -->
