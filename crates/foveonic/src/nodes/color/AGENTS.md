---
summary: "Color nodes own camera modes, noise preparation, tiled reconstruction, pattern conditioning, and ColorDQ."
---

# Color operations

Keep tiled sample meanings and side-output dependencies explicit. Noise controls remain available through `foveonic::noise`. A monochrome reconstruction path can share sensor corrections without inheriting color matrices or residual-gain algorithms.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[chroma.rs](./chroma.rs)** Applies signed-code chroma smoothing with caller-selected support and tapered correction tables.
- **[code_gain.rs](./code_gain.rs)** Applies a bounded Q12 gain to signed linear RGB codes independently of filtering.
- **[color_dq.rs](./color_dq.rs)** Builds reference-derived ColorDQ lookup tables with an explicitly selected gain.
- **[color_mode.rs](./color_mode.rs)** Resolves camera color modes and their camera-ordered tone records.
- **[highlight_stream.rs](./highlight_stream.rs)** Executes delayed five-row highlight neutralization with explicit threshold controls.
- **[log_reconstruction.rs](./log_reconstruction.rs)** Reconstructs signed RGB from guided log residuals with caller-selected filtering controls.
- **[mod.rs](./mod.rs)** Owns color noise preparation, reconstruction, and camera color conditioning.
- **[noise.rs](./noise.rs)** Resolves camera noise presets and linked overrides for shared GPU reconstruction.
- **[pattern.rs](./pattern.rs)** Prepares repeatable pattern-correction geometry without moving photo work to the CPU.
- **[range_reconstruction.rs](./range_reconstruction.rs)** Reconstructs linear RGB with range filtering, guided means, and deterministic residual diffusion.
- **[reconstruction.rs](./reconstruction.rs)** Executes reference-style processing tiles, noise filtering, residual detail, and image projections on GPU.
- **[weighted_white_balance.rs](./weighted_white_balance.rs)** Estimates a camera-layer neutral from reduced pixels and chroma-weighted histograms. Equations follow the recovered reference implementation. Image planes stay on the GPU.
- **[weighted_white_balance_stats.rs](./weighted_white_balance_stats.rs)** Converts compact histograms into tone controls and constrains estimated illuminants. Thresholds, rounding, and polygon coordinates follow the reference implementation.

<!-- INDEX:END -->
