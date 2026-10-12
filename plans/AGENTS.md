---
summary: "Small implementation plans track actionable steps, progress, discoveries, and checks."
---

# Plans

Read the selected plan and its linked specifications before work. Update checklists and findings as work proceeds. Discuss blockers with the owner and correct assumptions when evidence changes them. Keep lasting requirements in specifications.

The plans outline implementation work for review. Implementation begins with owner approval.

<!-- INDEX:START -->

<!-- Generated from document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

- **[camera-repair-default](./camera-repair-default.md)** Camera-record repair uses source-derived cutoffs with a manual override; ordered correction now fixes the subsequently reported leaf cluster.
- **[complete-reference-algorithms](./complete-reference-algorithms.md)** Autonomously complete the remaining Merrill reference algorithm stages and recipe integrations identified by the node audit, then validate the full GPU path. _Resume this autonomous implementation pass after a checkpoint or context compaction._
- **[despeckle](./despeckle.md)** GPU speckle detection and ordered correction fix the two reported red defects; separate repair/despeckle controls default on, and the owner's Dock icon is integrated.
- **[despeckle-wb-scale](./despeckle-wb-scale.md)** Align despeckle threshold WB with the actual calibration factors while retaining independent adjustment and Sensor-gain behavior.
- **[detail-and-noise](./detail-and-noise.md)** The reference-style full-resolution noise/detail shader slice is implemented and validated, with owner photographic review pending.
- **[editor-review-focus](./editor-review-focus.md)** Linked reconstruction controls, grouped inspection, folder restoration, and viewport reporting are implemented; the two identified residual speckles are corrected by the new GPU despeckle stage.
- **[first-conversion](./first-conversion.md)** Establish the shortest Merrill X3F conversion path through the shader graph to DNG and TIFF. _Read when beginning or resuming the first end-to-end conversion._
- **[gui-and-batch](./gui-and-batch.md)** Expose the working conversion engine through a minimal GUI and batch CLI using shared settings.
- **[icon-background](./icon-background.md)** A local macOS app bundle supports native automatic light/dark layered-X icons with opaque colored slabs and a restrained bevel.
- **[inspection-differences](./inspection-differences.md)** Make Inspect groups expand inline and add amplified signed layer/channel comparisons against explicit operation inputs.
- **[metadata-first-loading](./metadata-first-loading.md)** Read capture highlights independently of image decoding so the information bar updates while the selected photograph loads.
- **[ordered-repair-and-node-audit](./ordered-repair-and-node-audit.md)** Ordered camera repair fixes the leaf defect; the node audit led to the approved WB-scale fix and identifies remaining camera coverage gaps.
- **[photographic-help](./photographic-help.md)** Replace technical hover copy with photographic explanations, then review a small set of obvious A/B examples before expanding coverage.
- **[pipeline-performance](./pipeline-performance.md)** Pause algorithm expansion, stabilize a buildable baseline, measure completed cold/warm renders and GPU stages, then optimize the largest bottlenecks with output-equivalence checks. _Resume the owner-approved profiling and optimization pivot; remaining algorithm work is paused._
- **[render-resources-and-optimization](./render-resources-and-optimization.md)** Profile conversion, establish owned GPU render resources, and optimize decoding and scratch allocation without interactive caching.
- **[tiff-controls-and-batch](./tiff-controls-and-batch.md)** Add straightforward TIFF controls, optional corrective nodes, resizing, and shared-engine batch processing.
- **[ui-responsiveness](./ui-responsiveness.md)** Measure full-quality slider feedback with active-edit input reuse, image-first replies, deferred navigator presentation, and safe supersession.

<!-- INDEX:END -->
