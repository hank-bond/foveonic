---
summary: "Monochrome nodes mix sensor layers before dedicated filtering, column correction, signed detail, and neutral gain replication."
---

# Monochrome nodes

Follow the [monochrome requirements](../../../../../docs/specs/monochrome-rendering.md) and [implementation plan](../../../../../plans/archived/monochrome-rendering.md). Keep reference attribution when adapting arithmetic. Triangle amounts are actual barycentric proportions, not the reference's sequential normalization.

Tables contain metadata-derived coefficients, never photographic samples. Keep mixed brightness typed separately from masks, original corrected layers immutable, and histogram neighbors distinct from the corrected mono plane. Preserve the zero-noise route without histogram filtering. Independent Fine/Coarse texture adjusts the clean scalar image. Shared [color sensitivity](../finish/sensitivity.rs) preserves the authored sensor mixture. Neutral controls must bypass both added paths.

The histogram uses one GPU lane per interior column and interleaved count/sum bins in bin-major GPU scratch. Reuse that scratch across the fixed algorithmic strips and preserve each column's ordered initialization, updates, and weighted sums. Large private histograms previously produced nonrepeatable Metal output. Check full-sensor repeated rendering, fresh/session equality, and full-float/TIFF goldens when changing storage or traversal, not just the small arithmetic fixture. See the [performance plan](../../../../../plans/archived/monochrome-performance.md).

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[apply.rs](./apply.rs)** Dispatches monochrome nodes with separate immutable inputs and explicit guarded output storage.
- **[mod.rs](./mod.rs)** Mixes original layers and runs dedicated monochrome filtering, detail, and gain on GPU.
- **[parameters.rs](./parameters.rs)** Resolves monochrome layer weights, ISO presets, limits, and range-weight tables from metadata.
- **[texture.rs](./texture.rs)** Restores Fine and Coarse monochrome texture independently of noise cleanup.

<!-- INDEX:END -->
