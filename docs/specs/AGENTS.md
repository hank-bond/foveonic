---
summary: "Specifications record agreed application, graph, and output behavior independently of implementation progress."
---

# Specifications

Read the specification relevant to the work and its linked dependencies. Follow [`contributing.md`](../contributing.md) when changing requirements or their status. Acceptance records owner agreement. Implementation status records whether code and evidence satisfy the requirements.

These specifications capture the agreed scope. Detailed algorithm, format, and interface choices are discussed with the owner as implementation reaches them.

<!-- INDEX:START -->

<!-- Generated from document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

- **[application-workflows](./application-workflows.md)** _(accepted, incomplete, reference)._ A personal Merrill X3F app shares processing settings and one image engine between its GUI and batch CLI.
- **[column-conditioning](./column-conditioning.md)** _(accepted, conforming, reference)._ GPU column conditioning corrects raw processing tiles after base filtering and before luminance/detail estimation, retaining native windows, cuts, ordered means, strict eligibility, and clipping.
- **[despeckle](./despeckle.md)** _(accepted, conforming, reference)._ Full-image GPU despeckling detects channel extrema and corrects them in deterministic reference order, independently of camera-record repair.
- **[detail-and-noise](./detail-and-noise.md)** _(accepted, conforming, reference)._ Reference-style GPU noise filtering and detail reconstruction provide an inspectable photographic baseline.
- **[gui-workspace](./gui-workspace.md)** _(accepted, conforming, reference)._ The GUI uses a persistent three-pane workspace with a reflowing organizer, current-buffer scopes, and reorderable editor sections.
- **[highlight-restoration](./highlight-restoration.md)** _(accepted, conforming, reference)._ Reference highlight restoration uses immutable original saturation provenance, ordered donor statistics, and clipped-neighborhood conditioning before despeckle.
- **[image-information-bar](./image-information-bar.md)** _(accepted, conforming, reference)._ A compact image information bar shows capture settings, filename, and visible-region dimensions and coordinates for photographic review.
- **[interactive-rendering](./interactive-rendering.md)** _(accepted, incomplete, reference)._ Slider feedback uses full-quality active-edit inputs, bounded latest-request scheduling, image-first delivery, and deferred scopes and navigator presentation.
- **[native-sharpening](./native-sharpening.md)** _(accepted, conforming, reference)._ Optional native sharpening applies the separate signed mono high-pass through quarter-code packing and bounded brightness proportions before exposure/tone; Gaussian radius and blur stages are retired.
- **[output-strategies](./output-strategies.md)** _(accepted, incomplete, reference)._ Compressed DNG preserves editing latitude and embeds color-mode presentation, while TIFF bakes the selected rendering.
- **[pattern-conditioning](./pattern-conditioning.md)** _(accepted, conforming, reference)._ GPU pattern conditioning deterministically permutes selected near-neutral luminance ratios after native grouped min/max masking and before residual-detail mixing.
- **[photographic-help](./photographic-help.md)** _(accepted, incomplete, reference)._ Control help explains photographic effects in plain English, with reviewed A/B crops where they teach the adjustment clearly.
- **[pipeline-inspection](./pipeline-inspection.md)** _(accepted, incomplete, reference)._ Pipeline inspection requests named node outputs, executes their dependencies, and retains only selected artifacts and checkpoints.
- **[processing-graphs](./processing-graphs.md)** _(accepted, incomplete, reference)._ Internal processing DAGs use typed connections, shared metadata, node settings, and selectable algorithms for sensor and tone processing.
- **[profiling](./profiling.md)** _(accepted, conforming, reference)._ Profiling separates ordinary completed work, synchronized GPU diagnostics, and native feedback with reproducible context.
- **[reduced-reconstruction](./reduced-reconstruction.md)** _(accepted, conforming, reference)._ Metadata-selected eighth-step reconstruction uses native polyphase reduced local or whole-image fixed filtering, restores full tile geometry, and preserves explicit transposed pattern scratch provenance.
- **[render-resources](./render-resources.md)** _(accepted, conforming, reference)._ Owned GPU render results separate image processing from display and export, with explicit pixel meaning and geometry.
- **[row-banding](./row-banding.md)** _(accepted, conforming, reference)._ GPU row-banding correction fits the native geometric patch banks, rejects scene-correlated residuals, and subtracts bounded additive per-row sensor bias before reconstruction.

<!-- INDEX:END -->
