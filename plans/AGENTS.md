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
- **[cache-workflow-measurements](./cache-workflow-measurements.md)** Measure memory ownership and latency across editing workflows before recommending cache budgets or changing retention policy.
- **[classic-coverage](./classic-coverage.md)** Complete ordinary color rendering and editable DNG export across all pre-Merrill Sigma Classic cameras.
- **[coherence-cleanup](./coherence-cleanup.md)** Finish removing duplicated owners and near-duplicate paths in 0.10.0, following the one-owner principle that 0.9.0 started applying.
- **[color-field-boundaries](./color-field-boundaries.md)** Prepare the reviewed Color Field boundary recovery with separate evidence and application masks for a scoped commit.
- **[color-field-controls](./color-field-controls.md)** Investigate whether color agreement and boundary controls produce visible changes independently of GUI and cache behavior.
- **[color-field-production](./color-field-production.md)** Remove exploratory color-field paths, resolve the strict Output blocker, and prepare reviewed integration onto current main.
- **[color-field-restraint](./color-field-restraint.md)** Expose one per-image highlight color restraint adjustment and remove rejected shader experiments while preserving optimized Local/Global defaults.
- **[color-field](./color-field.md)** Implement GPU color-field highlight reconstruction, validate against the pinned reference, and compare quality/performance experiments using saved edits.
- **[comparable-runs](./comparable-runs.md)** Generate a render run from any revision and compare two runs for pixel changes and timing, and time every behavioral step, so release checks need nothing stored on one machine.
- **[complete-reference-algorithms](./complete-reference-algorithms.md)** Track remaining reference-algorithm follow-ups and the completed default photographic output resizer.
- **[coordination-overhead-profile](./coordination-overhead-profile.md)** Measure data movement, preparation, scheduling, synchronization and delivery overhead before selecting current-architecture optimizations.
- **[dehaze](./dehaze.md)** Adapt recovered haze estimation and patch picking into a saved GPU Dehaze correction.
- **[display-color-management](./display-color-management.md)** Integrate ICC-derived display limits, managed macOS presentation and gamut-preserving previews.
- **[dng-export](./dng-export.md)** Implement uncompressed uint16 DNG export through the existing TIFF writer, with baked technical corrections and one embedded appearance; evaluate compression later.
- **[early-sensor-reduction](./early-sensor-reduction.md)** Move the sensor reduction boundary earlier with working-resolution inputs, preserved native algorithms and existing recipe ownership.
- **[editing-optimization-handoff](./editing-optimization-handoff.md)** Continue native editing measurements with guarded GPU ownership while preserving the deferred mismatch evidence.
- **[editing-sidecars](./editing-sidecars.md)** Persist shared photographic settings in atomic versioned JSON sidecars and restore them independently of render completion.
- **[exif-export](./exif-export.md)** Audit and preserve X3F capture metadata across all supported export formats.
- **[first-conversion](./first-conversion.md)** Establish the shortest Merrill X3F conversion path through the shader graph to DNG and TIFF. _Read when beginning or resuming the first end-to-end conversion._
- **[full-option-scaled-pipeline](./full-option-scaled-pipeline.md)** Replace recipe-wide scale exceptions with numeric working grids, graph-owned dependency cuts and complete downstream option support.
- **[gpu-output-mismatch](./gpu-output-mismatch.md)** Intermittent GPU output differences remain unresolved, with strict regression checks and retained investigation evidence.
- **[gui-and-batch](./gui-and-batch.md)** Expose the working conversion engine through a minimal GUI and batch CLI using shared settings.
- **[gui-release-regressions](./gui-release-regressions.md)** Resolve GUI and GPU correctness failures and require packaged desktop checks before releasing 0.8.2.
- **[loupe-motion-diagnostics](./loupe-motion-diagnostics.md)** Attribute loupe-motion stuttering through sustained input and native presentation timing before changing scheduling.
- **[memory-sweep](./memory-sweep.md)** Reduce peak and retained application memory through measured, behavior-preserving changes.
- **[merrill-banding-width](./merrill-banding-width.md)** Compare scale-separated native row and column correction against the accepted fine-band baseline.
- **[multi-edit-latency](./multi-edit-latency.md)** Reduce multi-edit application latency through work avoidance, reusable preparation, scale-aware execution and coordinated delivery.
- **[multi-edit](./multi-edit.md)** Implement independent viewport editing selection, Pointer-only selection routing and shared mixed-value controls before finalizing memory policy.
- **[perceptual-editing](./perceptual-editing.md)** Evaluate shared colorfulness and an Exposure, spatial HDR and anchored Brightness workflow.
- **[performance-diagnostics](./performance-diagnostics.md)** Integrate the owner-approved Perfetto recorder and application scenarios, then validate causal coverage, capture completeness and overhead before renderer experiments.
- **[photographic-help](./photographic-help.md)** Replace technical hover copy with photographic explanations, then review a small set of obvious A/B examples before expanding coverage.
- **[quality-speed](./quality-speed.md)** Replace five preview modes with consistent Quality and Speed processing, dense intermediates and policy-aware reuse.
- **[quick-look-previews](./quick-look-previews.md)** Ship Quick Look X3F previews in 0.9.2 with one source of a photograph's defaults for the app, the command line and the extensions.
- **[reference-banding-verification](./reference-banding-verification.md)** Record native Merrill banding presets, reached pixel consumers, and the remaining application-export checks.
- **[regional-bounds](./regional-bounds.md)** Inventory regional storage, active samples and delivered coverage before a separate geometry consolidation slice.
- **[regional-preview-completion](./regional-preview-completion.md)** Complete native visible-region rendering for adaptive tone, monochrome and authored rotation, with whole-image comparisons and measured app latency.
- **[release-0.8.4](./release-0.8.4.md)** Prepare and validate 0.8.4 with reviewed capture-metadata and TIFF interoperability fixes.
- **[release-0.9.0](./release-0.9.0.md)** 0.9.0 is held until the render scheduler replaces the serial and Merrill-only wide paths. Its viewing features and release lanes remain in scope.
- **[release-0.9.1](./release-0.9.1.md)** Remove the last-selected photograph from editing and display, and release 0.9.1.
- **[release-test-efficiency](./release-test-efficiency.md)** Reduce healthy release validation toward thirty minutes with bounded correctness workers, reusable verification and explicit isolation.
- **[render-coordination-consistency](./render-coordination-consistency.md)** Inventory coordination, scheduling and cache inconsistencies before choosing small current-architecture cleanup slices.
- **[render-runtime](./render-runtime.md)** Clarify render ownership and responsibilities through one owner-reviewed concept at a time. Execution now belongs to the render scheduler plan.
- **[render-scheduler](./render-scheduler.md)** Replace the separate serial and Merrill-only wide paths with one core Scheduler, working set and priority order before releasing 0.9.0.
- **[sectional-rendering](./sectional-rendering.md)** Reduce native sectional rendering work through explicit input support and measured Whole comparisons.
- **[sequoia-compatibility](./sequoia-compatibility.md)** Ship a macOS 15-targeted Apple Silicon build for Sequoia user testing while retaining explicit runtime coverage limits.
- **[shared-photographic-finishing](./shared-photographic-finishing.md)** Integrate shared photographic tone, monochrome sensitivity, local contrast and sharpening with camera recipes and bounded interactive reuse.
- **[slider-session-reuse](./slider-session-reuse.md)** Pilot native slider session reuse while retaining declared case isolation and independent photographic comparisons.
- **[support-logging](./support-logging.md)** Astra and eight Terra audits map missing user-support evidence to shared error reporting and staged logging work.
- **[tiff-color-and-auto-sharpen](./tiff-color-and-auto-sharpen.md)** Add selectable TIFF destination color spaces and camera-derived Auto sharpening without changing existing defaults.
- **[tonal-ranges](./tonal-ranges.md)** Implement and evaluate coordinated Merrill HDR controls after researching spatial tone mapping.
- **[ui-responsiveness](./ui-responsiveness.md)** Measure full-quality slider feedback with active-edit input reuse, image-first replies, deferred navigator presentation, and safe supersession.
- **[view-planning](./view-planning.md)** Move each viewport photo's request staging and layer choice into one view planner, then add settling and cancellation for moving views.
- **[viewport-overnight](./viewport-overnight.md)** Complete the approved viewport workflow slices in reviewed commits, then investigate wide execution with isolated measurements.
- **[viewport-workflows](./viewport-workflows.md)** Review small ownership and quality-policy changes supporting Smart comparisons, persistent zoom and a Full-detail loupe.
- **[windows-release](./windows-release.md)** Build and validate a Windows x64 test archive for the current release branch.
- **[windows-work-area](./windows-work-area.md)** Finish and release the 0.9.x fix that keeps the restored window inside the Windows work area, clear of the taskbar.

<!-- INDEX:END -->
