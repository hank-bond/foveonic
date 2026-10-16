---
summary: "Small implementation plans track actionable steps, progress, discoveries, and checks."
---

# Plans

Read the selected plan and its linked specifications before work. Update checklists and findings as work proceeds. Discuss blockers with the owner and correct assumptions when evidence changes them. Keep lasting requirements in specifications.

The plans outline implementation work for review. Implementation begins with owner approval.

Move completed implementation slices to [`archived/`](./archived/AGENTS.md). Keep unresolved follow-ups explicit rather than marking them complete. Archived plans preserve historical findings and limitations, not current implementation instructions.

Keep plans self-contained. Referenced filesystem paths must be checked in; do not cite temporary or ignored artifact locations. Summarize findings, methods, and limitations in the plan instead.

<!-- INDEX:START -->

<!-- Generated from document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

- **[archived/](./archived/AGENTS.md)** Archived plans record completed implementation slices, their checks, and historical limitations.
- **[banding-slider-and-settings-defaults](./banding-slider-and-settings-defaults.md)** Add a five-stop column-banding slider and deserialize omitted image-edit fields from shared Rust defaults.
- **[camera-repair-default](./camera-repair-default.md)** Camera-record repair uses source-derived cutoffs with a manual override; ordered correction now fixes the subsequently reported leaf cluster.
- **[complete-reference-algorithms](./complete-reference-algorithms.md)** Autonomously complete the remaining Merrill reference algorithm stages and recipe integrations identified by the node audit, then validate the full GPU path. _Resume this autonomous implementation pass after a checkpoint or context compaction._
- **[despeckle-wb-scale](./despeckle-wb-scale.md)** Align despeckle threshold WB with the actual calibration factors while retaining independent adjustment and Sensor-gain behavior.
- **[despeckle](./despeckle.md)** GPU speckle detection and ordered correction fix the two reported red defects; separate repair/despeckle controls default on, and the owner's Dock icon is integrated.
- **[detail-and-noise](./detail-and-noise.md)** The reference-style full-resolution noise/detail shader slice is implemented and validated, with owner photographic review pending.
- **[dng-export](./dng-export.md)** Implement uncompressed uint16 DNG export through the existing TIFF writer, with baked technical corrections and one embedded appearance; evaluate compression later.
- **[editing-optimization-handoff](./editing-optimization-handoff.md)** Continue native editing measurements with guarded GPU ownership while preserving the deferred mismatch evidence.
- **[editing-sidecars](./editing-sidecars.md)** Persist shared photographic settings in atomic versioned JSON sidecars and restore them independently of render completion.
- **[editor-review-focus](./editor-review-focus.md)** Linked reconstruction controls, grouped inspection, folder restoration, and viewport reporting are implemented; the two identified residual speckles are corrected by the new GPU despeckle stage.
- **[first-conversion](./first-conversion.md)** Establish the shortest Merrill X3F conversion path through the shader graph to DNG and TIFF. _Read when beginning or resuming the first end-to-end conversion._
- **[gpu-output-mismatch](./gpu-output-mismatch.md)** Intermittent GPU output differences remain unresolved, with strict regression checks and retained investigation evidence.
- **[gui-and-batch](./gui-and-batch.md)** Expose the working conversion engine through a minimal GUI and batch CLI using shared settings.
- **[help-comparisons](./help-comparisons.md)** Manage release-hosted example photographs, explicit comparison rendering, and checked-in images through one shared popover template.
- **[icon-background](./icon-background.md)** A local macOS app bundle supports native automatic light/dark layered-X icons with opaque colored slabs and a restrained bevel.
- **[inspection-differences](./inspection-differences.md)** Inspect operation inputs and outputs alongside amplified signed layer/channel differences.
- **[monochrome-rendering](./monochrome-rendering.md)** GPU monochrome rendering, a triangular layer mixer, mode-specific editing, and linear monochrome DNG export are implemented and checked.
- **[ordered-repair-and-node-audit](./ordered-repair-and-node-audit.md)** Ordered camera repair fixes the leaf defect; the node audit led to the approved WB-scale fix and identifies remaining camera coverage gaps.
- **[photographic-help](./photographic-help.md)** Replace technical hover copy with photographic explanations, then review a small set of obvious A/B examples before expanding coverage.
- **[reference-banding-verification](./reference-banding-verification.md)** Record native Merrill banding presets, reached pixel consumers, and the remaining application-export checks.
- **[ui-responsiveness](./ui-responsiveness.md)** Measure full-quality slider feedback with active-edit input reuse, image-first replies, deferred navigator presentation, and safe supersession.

<!-- INDEX:END -->
