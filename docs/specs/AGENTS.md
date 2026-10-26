---
summary: "Specifications record agreed application, graph, and output behavior independently of implementation progress."
---

# Specifications

Read the specification relevant to the work and its linked dependencies. Follow [`contributing.md`](../contributing.md) when changing requirements or their status. Acceptance records owner agreement. Implementation status records whether code and evidence satisfy the requirements.

These specifications capture the agreed scope. Detailed algorithm, format, and interface choices are discussed with the owner as implementation reaches them.

<!-- INDEX:START -->

<!-- Generated from document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

- **[application-workflows](./application-workflows.md)** _(accepted, incomplete, reference)._ A personal Merrill X3F app shares processing settings and one image engine between its GUI and batch CLI.
- **[browser-previews](./browser-previews.md)** _(accepted, conforming, reference)._ Small embedded thumbnails appear immediately and asynchronous cached previews follow the latest photographic render.
- **[classic-rendering](./classic-rendering.md)** _(accepted, incomplete, reference)._ Classic photographic rendering uses format-selected decoding and family-authored recipes with functional GPU operations and shared engine resources.
- **[color-field](./color-field.md)** _(accepted, incomplete, reference)._ Color-field highlight reconstruction separates brightness and color in GPU passes with immutable source reliability and optional local color-evidence selection.
- **[column-conditioning](./column-conditioning.md)** _(accepted, conforming, reference)._ GPU column conditioning corrects raw processing tiles after base filtering and before luminance/detail estimation, retaining native windows, cuts, ordered means, strict eligibility, and clipping.
- **[decode-cache](./decode-cache.md)** _(accepted, conforming, reference)._ A bounded temporary disk LRU reuses decoded sources, originals, selection metadata and previews with per-family workspace controls.
- **[despeckle](./despeckle.md)** _(accepted, conforming, reference)._ Full-image GPU despeckling detects channel extrema and corrects them in deterministic reference order, independently of camera-record repair.
- **[detail-and-noise](./detail-and-noise.md)** _(accepted, conforming, reference)._ Merrill GPU reconstruction separates noise cleanup from independently sized Fine and Coarse brightness detail.
- **[display-color-management](./display-color-management.md)** _(accepted, incomplete, reference)._ Display colors follow each image's declared encoding and the host monitor's active profile.
- **[editing-sidecars](./editing-sidecars.md)** _(accepted, conforming, reference)._ Original X3Fs and editing recipes in a visible source-folder subdirectory are authoritative.
- **[family-controls](./family-controls.md)** _(accepted, conforming, reference)._ Family-authored editing panels share widgets and use processing-owned capabilities and defaults.
- **[gui-screenshot](./gui-screenshot.md)** _(accepted, conforming, reference)._ An explicit Cargo task refreshes a lossless WebP README screenshot using the shared local example-photograph cache.
- **[gui-workspace](./gui-workspace.md)** _(accepted, conforming, reference)._ The GUI uses a persistent three-pane workspace with a reflowing organizer, current-buffer scopes, and reorderable editor sections.
- **[highlight-restoration](./highlight-restoration.md)** _(accepted, conforming, reference)._ Classic highlight restoration uses immutable original saturation provenance, ordered donor statistics, and clipped-neighborhood conditioning before despeckle.
- **[image-information-bar](./image-information-bar.md)** _(accepted, conforming, reference)._ A compact image information bar shows capture settings, filename, and visible-region dimensions and coordinates for photographic review.
- **[import-workflow](./import-workflow.md)** _(accepted, conforming, reference)._ Import mode reviews sources without source-side writes and carries selected originals, edits, and hidden-file decisions to a destination.
- **[interactive-rendering](./interactive-rendering.md)** _(accepted, incomplete, reference)._ Slider feedback uses full-quality active-edit inputs, bounded latest-request scheduling, image-first delivery, and deferred scopes and navigator presentation.
- **[merrill-fast-preview](./merrill-fast-preview.md)** _(accepted, conforming, reference)._ Merrill Fast uses a filtered quarter-pixel grid and Smart delivers native viewport detail before the whole frame.
- **[monochrome-rendering](./monochrome-rendering.md)** _(accepted, conforming, reference)._ Merrill monochrome rendering mixes calibrated sensor layers before dedicated GPU filtering and exports editable linear monochrome DNG samples.
- **[native-sharpening](./native-sharpening.md)** _(accepted, incomplete, reference)._ Optional GPU sharpening uses signed high-pass before exposure/tone, with distinct color-proportional and additive monochrome applications.
- **[output-strategies](./output-strategies.md)** _(accepted, incomplete, reference)._ DNG preserves editing latitude and embeds color-mode presentation, while TIFF bakes the selected rendering.
- **[pattern-conditioning](./pattern-conditioning.md)** _(accepted, conforming, reference)._ GPU pattern conditioning deterministically permutes selected near-neutral luminance ratios after native grouped min/max masking and before sensor-space texture restoration.
- **[photographic-finishing](./photographic-finishing.md)** _(accepted, conforming, reference)._ Shared photographic finishing offers universal tone, monochrome sensitivity and sharpening beside existing camera-derived methods.
- **[photographic-help](./photographic-help.md)** _(accepted, conforming, reference)._ Control help explains photographic effects in plain English, with reviewed A/B crops where they teach the adjustment clearly.
- **[pipeline-inspection](./pipeline-inspection.md)** _(accepted, incomplete, reference)._ Pipeline inspection requests named node outputs, executes their dependencies, and retains only selected artifacts and checkpoints.
- **[processing-graphs](./processing-graphs.md)** _(accepted, incomplete, reference)._ Internal processing DAGs use typed connections, shared metadata, node settings, and selectable algorithms for sensor and tone processing.
- **[profiling](./profiling.md)** _(accepted, conforming, reference)._ Profiling separates ordinary completed work, synchronized GPU diagnostics, and native feedback with reproducible context.
- **[quattro-fast-preview](./quattro-fast-preview.md)** _(accepted, conforming, reference)._ An optional Quattro Fast preview reduces rendering resolution while preserving Full exports and native viewport coordinates.
- **[quattro-input](./quattro-input.md)** _(accepted, conforming, reference)._ Quattro input preserves mixed-resolution sensor codes and exposes independently sized inspection planes.
- **[quattro-rendering](./quattro-rendering.md)** _(accepted, conforming, reference)._ Quattro recipes share numerical reconstruction and finishing while preserving camera-specific calibration.
- **[reduced-reconstruction](./reduced-reconstruction.md)** _(accepted, conforming, reference)._ Metadata-selected eighth-step reconstruction uses native polyphase reduced local or whole-image fixed filtering, restores full tile geometry, and preserves explicit transposed pattern scratch provenance.
- **[releases](./releases.md)** _(accepted, conforming, reference)._ Cargo packages macOS apps and release tags build Developer ID signed, unnotarized draft GitHub Releases without publishing them.
- **[render-resources](./render-resources.md)** _(accepted, conforming, reference)._ Owned GPU render results separate image processing from display and export, with explicit pixel meaning and geometry.
- **[row-banding](./row-banding.md)** _(accepted, conforming, reference)._ GPU row-banding correction fits the native geometric patch banks, rejects scene-correlated residuals, and subtracts bounded additive per-row sensor bias before reconstruction.
- **[smart-preview](./smart-preview.md)** _(accepted, conforming, reference)._ Smart preview shows Draft before Full detail, with viewport-first refinement where the recipe supports it.
- **[tiger-rendering](./tiger-rendering.md)** _(accepted, conforming, reference)._ Tiger uses its native calibrated single-frame route while sharing proven numerical operations with Quattro.
- **[tonal-ranges](./tonal-ranges.md)** _(accepted, conforming, reference)._ Experimental Merrill HDR controls use a multiscale spatial base for preview and TIFF without affecting DNG.

<!-- INDEX:END -->
