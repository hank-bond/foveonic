---
summary: "Color nodes own camera modes, noise preparation, tiled reconstruction, pattern conditioning, and ColorDQ."
---

# Color operations

Keep tiled sample meanings and side-output dependencies explicit. Noise controls remain available through `foveonic::noise`. A monochrome reconstruction path can share sensor corrections without inheriting color matrices or residual-gain algorithms.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[color_dq.rs](./color_dq.rs)** Builds reference-derived ColorDQ lookup tables with an explicitly selected gain.
- **[color_mode.rs](./color_mode.rs)** Resolves camera color modes and their camera-ordered tone records.
- **[mod.rs](./mod.rs)** Owns color noise preparation, reconstruction, and camera color conditioning.
- **[noise.rs](./noise.rs)** Resolves camera noise presets and linked overrides for shared GPU reconstruction.
- **[pattern.rs](./pattern.rs)** Prepares repeatable pattern-correction geometry without moving photo work to the CPU.
- **[reconstruction.rs](./reconstruction.rs)** Executes reference-style processing tiles, noise filtering, residual detail, and image projections on GPU.

<!-- INDEX:END -->
