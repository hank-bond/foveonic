---
summary: "Color nodes own camera modes, noise preparation, tiled reconstruction, pattern conditioning, and ColorDQ."
---

# Color operations

Keep tiled sample meanings and side-output dependencies explicit. Noise controls remain available through `foveonic::noise`. A monochrome reconstruction path can share sensor corrections without inheriting color matrices or residual-gain algorithms.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[blue_guided_reconstruct.rs](./blue_guided_reconstruct.rs)** Reconstructs code RGB from color histories and blue at matching or doubled output resolution.
- **[byte_mask_double.rs](./byte_mask_double.rs)** Doubles byte-valued masks with the selected cubic phases and exact integer accumulation.
- **[cell_gain.rs](./cell_gain.rs)** Applies a spatial gain to each two-by-two cell with Float32 horizontal recurrence.
- **[channel_noise.rs](./channel_noise.rs)** Composes blue-speckle repair, channel-ratio correction and neighborhood filtering without changing its source.
- **[chroma.rs](./chroma.rs)** Applies signed-code chroma smoothing with caller-selected support and tapered correction tables.
- **[chromatic_ratio.rs](./chromatic_ratio.rs)** Suppresses channel noise by blending RGB ratios toward calibrated quadratic color curves.
- **[code_gain.rs](./code_gain.rs)** Applies a bounded Q12 gain to signed linear RGB codes independently of filtering.
- **[color_dq.rs](./color_dq.rs)** Builds reference-derived ColorDQ lookup tables with an explicitly selected gain.
- **[color_mode.rs](./color_mode.rs)** Resolves camera color modes and their camera-ordered tone records.
- **[highlight_stream.rs](./highlight_stream.rs)** Executes delayed five-row highlight neutralization with explicit threshold controls.
- **[histogram_bilateral.rs](./histogram_bilateral.rs)** Filters a scalar code plane with quantized range weights and an adaptive residual blend.
- **[log_reconstruction.rs](./log_reconstruction.rs)** Reconstructs signed RGB from guided log residuals with caller-selected filtering controls.
- **[mod.rs](./mod.rs)** Owns color noise preparation, reconstruction, and camera color conditioning.
- **[neighborhood_decorrelate.rs](./neighborhood_decorrelate.rs)** Decorrelates RGB noise through ordered neighborhood updates with supplied variance and weight tables.
- **[noise.rs](./noise.rs)** Resolves camera noise presets and linked overrides for shared GPU reconstruction.
- **[packed_lab.rs](./packed_lab.rs)** Converts unsigned RGB code triples to packed Lab codes and back with supplied matrices.
- **[pattern.rs](./pattern.rs)** Prepares repeatable pattern-correction geometry without moving photo work to the CPU.
- **[purple_fringe.rs](./purple_fringe.rs)** Corrects spatial green fringes through three immutable, brightness-preserving RGB passes.
- **[pyramid_resample.rs](./pyramid_resample.rs)** Resamples RGB by one half or two using the reference triangular pyramid kernels.
- **[range_reconstruction.rs](./range_reconstruction.rs)** Reconstructs linear RGB with range filtering, guided means, and deterministic residual diffusion.
- **[raw_color_blend.rs](./raw_color_blend.rs)** Applies highlight neutralization, adaptive camera-color response and chroma outlier protection.
- **[reconstruction.rs](./reconstruction.rs)** Executes reference-style processing tiles, noise filtering, residual detail, and image projections on GPU.
- **[residual_finish.rs](./residual_finish.rs)** Applies residual gains and derives signed detail from reconstructed unsigned code RGB.
- **[rgb_prefilter.rs](./rgb_prefilter.rs)** Composes white balance, UV classification and fixed-radius range filtering in camera RGB codes.
- **[sector_color.rs](./sector_color.rs)** Applies prepared chroma-sector matrices without selecting a camera or color mode.
- **[spatial_color_gain.rs](./spatial_color_gain.rs)** Multiplies red and blue by prepared spatial gains while preserving green.
- **[two_scale_noise.rs](./two_scale_noise.rs)** Combines full/half-resolution RGB filtering with brightness protection or edge-guided residual blending.
- **[uv_repair.rs](./uv_repair.rs)** Converts RGB/YUV and classifies radius-one chroma neighborhoods against a supplied noise model.
- **[weighted_white_balance.rs](./weighted_white_balance.rs)** Estimates a camera-layer neutral from reduced pixels and chroma-weighted histograms. Equations follow the recovered reference implementation. Image planes stay on the GPU.
- **[weighted_white_balance_stats.rs](./weighted_white_balance_stats.rs)** Converts compact histograms into tone controls and constrains estimated illuminants. Thresholds, rounding, and polygon coordinates follow the reference implementation.

<!-- INDEX:END -->
