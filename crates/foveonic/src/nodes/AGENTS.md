---
summary: "Processing nodes group sensor corrections, color reconstruction, and finishing with their shaders and tests."
---

# Processing nodes

Keep metadata preparation and GPU operations together by photographic responsibility. Pixel arithmetic belongs in the adjacent WGSL files. Recipes choose operation order in [render/recipes](../render/recipes/AGENTS.md), while [graph operations](../graph/AGENTS.md) define typed connections and output identities.

Add monochrome-specific operations alongside these groups when their implementation begins. Share operations where their sample meanings and algorithms match.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[color/](./color/AGENTS.md)** Color nodes own camera modes, noise preparation, tiled reconstruction, pattern conditioning, and ColorDQ.
- **[finish/](./finish/AGENTS.md)** Finishing nodes prepare tone and resampling beside sharpening and encoding shaders.
- **[sensor/](./sensor/AGENTS.md)** Sensor nodes prepare calibration and original-layer corrections before reconstruction.

### Source files

- **[mod.rs](./mod.rs)** Groups photographic operations by sensor correction, color reconstruction, and finishing.

<!-- INDEX:END -->
