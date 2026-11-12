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
- **[behavioral-testing](./behavioral-testing.md)** Extend real-app interaction checks and diagnostic feedback across camera families without binding behavior to the current render architecture.
- **[classic-coverage](./classic-coverage.md)** Complete ordinary color rendering and editable DNG export across all pre-Merrill Sigma Classic cameras.
- **[color-field-boundaries](./color-field-boundaries.md)** Prepare the reviewed Color Field boundary recovery with separate evidence and application masks for a scoped commit.
- **[color-field-controls](./color-field-controls.md)** Investigate whether color agreement and boundary controls produce visible changes independently of GUI and cache behavior.
- **[color-field-production](./color-field-production.md)** Remove exploratory color-field paths, resolve the strict Output blocker, and prepare reviewed integration onto current main.
- **[color-field-restraint](./color-field-restraint.md)** Expose one per-image highlight color restraint adjustment and remove rejected shader experiments while preserving optimized Local/Global defaults.
- **[color-field](./color-field.md)** Implement GPU color-field highlight reconstruction, validate against the pinned reference, and compare quality/performance experiments using saved edits.
- **[complete-reference-algorithms](./complete-reference-algorithms.md)** Track remaining reference-algorithm follow-ups and the completed default photographic output resizer.
- **[coordination-overhead-profile](./coordination-overhead-profile.md)** Measure data movement, preparation, scheduling, synchronization and delivery overhead before selecting current-architecture optimizations.
- **[dehaze](./dehaze.md)** Adapt recovered haze estimation and patch picking into a saved GPU Dehaze correction.
- **[display-color-management](./display-color-management.md)** Integrate ICC-derived display limits, managed macOS presentation and gamut-preserving previews.
- **[dng-export](./dng-export.md)** Implement uncompressed uint16 DNG export through the existing TIFF writer, with baked technical corrections and one embedded appearance; evaluate compression later.
- **[early-sensor-reduction](./early-sensor-reduction.md)** Move the sensor reduction boundary earlier with working-resolution inputs, preserved native algorithms and existing recipe ownership.
- **[editing-optimization-handoff](./editing-optimization-handoff.md)** Continue native editing measurements with guarded GPU ownership while preserving the deferred mismatch evidence.
- **[editing-sidecars](./editing-sidecars.md)** Persist shared photographic settings in atomic versioned JSON sidecars and restore them independently of render completion.
- **[first-conversion](./first-conversion.md)** Establish the shortest Merrill X3F conversion path through the shader graph to DNG and TIFF. _Read when beginning or resuming the first end-to-end conversion._
- **[full-option-scaled-pipeline](./full-option-scaled-pipeline.md)** Replace recipe-wide scale exceptions with numeric working grids, graph-owned dependency cuts and complete downstream option support.
- **[gpu-output-mismatch](./gpu-output-mismatch.md)** Intermittent GPU output differences remain unresolved, with strict regression checks and retained investigation evidence.
- **[gui-and-batch](./gui-and-batch.md)** Expose the working conversion engine through a minimal GUI and batch CLI using shared settings.
- **[merrill-banding-width](./merrill-banding-width.md)** Compare scale-separated native row and column correction against the accepted fine-band baseline.
- **[multi-edit-latency](./multi-edit-latency.md)** Reduce multi-edit application latency through work avoidance, reusable preparation, scale-aware execution and coordinated delivery.
- **[multi-edit](./multi-edit.md)** Implement independent viewport editing selection, Pointer-only selection routing and shared mixed-value controls before finalizing memory policy.
- **[perceptual-editing](./perceptual-editing.md)** Evaluate shared colorfulness and an Exposure, spatial HDR and anchored Brightness workflow.
- **[performance-diagnostics](./performance-diagnostics.md)** Integrate the owner-approved Perfetto recorder and application scenarios, then validate causal coverage, capture completeness and overhead before renderer experiments.
- **[photographic-help](./photographic-help.md)** Replace technical hover copy with photographic explanations, then review a small set of obvious A/B examples before expanding coverage.
- **[reference-banding-verification](./reference-banding-verification.md)** Record native Merrill banding presets, reached pixel consumers, and the remaining application-export checks.
- **[render-coordination-consistency](./render-coordination-consistency.md)** Inventory coordination, scheduling and cache inconsistencies before choosing small current-architecture cleanup slices.
- **[render-runtime](./render-runtime.md)** Clarify render ownership and responsibilities through one owner-reviewed concept at a time while preserving serial engine execution.
- **[sequoia-compatibility](./sequoia-compatibility.md)** Ship a macOS 15-targeted Apple Silicon build for Sequoia user testing while retaining explicit runtime coverage limits.
- **[shared-photographic-finishing](./shared-photographic-finishing.md)** Integrate shared photographic tone, monochrome sensitivity, local contrast and sharpening with camera recipes and bounded interactive reuse.
- **[tiff-color-and-auto-sharpen](./tiff-color-and-auto-sharpen.md)** Add selectable TIFF destination color spaces and camera-derived Auto sharpening without changing existing defaults.
- **[tonal-ranges](./tonal-ranges.md)** Implement and evaluate coordinated Merrill HDR controls after researching spatial tone mapping.
- **[ui-responsiveness](./ui-responsiveness.md)** Measure full-quality slider feedback with active-edit input reuse, image-first replies, deferred navigator presentation, and safe supersession.
- **[viewport-workflows](./viewport-workflows.md)** Review small ownership and quality-policy changes supporting Smart comparisons, persistent zoom and a Full-detail loupe.
- **[wide-rendering](./wide-rendering.md)** Preserve deferred wide-rendering hypotheses and evidence for a separate engine investigation after application ownership is clearer.

<!-- INDEX:END -->
