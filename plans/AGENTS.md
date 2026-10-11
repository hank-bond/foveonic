---
summary: "Small implementation plans track actionable steps, progress, discoveries, and checks."
---

# Plans

Read the selected plan and its linked specifications before work. Update checklists and findings as work proceeds. Discuss blockers with the owner and correct assumptions when evidence changes them. Keep lasting requirements in specifications.

The plans outline implementation work for review. Implementation begins with owner approval.

<!-- INDEX:START -->

<!-- Generated from document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

- **[camera-repair-default](./camera-repair-default.md)** Camera-record repair uses source-derived cutoffs with a manual override; ordered correction now fixes the subsequently reported leaf cluster.
- **[despeckle](./despeckle.md)** GPU speckle detection and ordered correction fix the two reported red defects; separate repair/despeckle controls default on.
- **[detail-and-noise](./detail-and-noise.md)** The reference-style full-resolution noise/detail shader slice is implemented and validated, with owner photographic review pending.
- **[editor-review-focus](./editor-review-focus.md)** Linked reconstruction controls, grouped inspection and viewport reporting are implemented; the two identified residual speckles are corrected by the new GPU despeckle stage.
- **[first-conversion](./first-conversion.md)** Establish the shortest Merrill X3F conversion path through the shader graph to DNG and TIFF. _Read when beginning or resuming the first end-to-end conversion._
- **[gui-and-batch](./gui-and-batch.md)** Expose the working conversion engine through a minimal GUI and batch CLI using shared settings.
- **[ordered-repair-and-node-audit](./ordered-repair-and-node-audit.md)** Ordered camera repair fixes the leaf defect; the node audit identifies remaining camera coverage and WB-scale gaps.
- **[render-resources-and-optimization](./render-resources-and-optimization.md)** Profile conversion, establish owned GPU render resources, and optimize decoding and scratch allocation without interactive caching.
- **[tiff-controls-and-batch](./tiff-controls-and-batch.md)** Add straightforward TIFF controls, optional corrective nodes, resizing, and shared-engine batch processing.

<!-- INDEX:END -->
