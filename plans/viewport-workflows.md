---
summary: "Review small ownership and quality-policy changes supporting Smart comparisons, persistent zoom and a Full-detail loupe."
---

# Viewport workflow architecture

Requirements under review: [viewport workflows](../docs/specs/viewport-workflows.md), [Smart preview](../docs/specs/smart-preview.md), [multi-edit](../docs/specs/multi-edit.md), [small previews](../docs/specs/small-previews.md) and [render orchestration](../docs/specs/render-orchestration.md). Apply the [three architecture goals](../docs/specs/render-orchestration.md#agreed-goals) to each slice. This plan is discussion, not implementation approval.

## Checkpoint and review

`02104b4` groups browser sources mechanically. `33453b3` separates requested source loading from serial rendering. Their validation is recorded in the [runtime plan](./render-runtime.md#source-loading-checks-and-lessons). The owner requested an Astra review devoted to architecture before implementing the next viewport features. The review inspected current sources and documentation and introduced no runtime changes.

The existing coordinator, source loader, processing sessions and immutable presentation resources are useful boundaries. This roadmap does not require a new scheduler framework, a universal cache manager or stage-wide engine execution. Wide rendering remains a [separate future experiment](./wide-rendering.md).

The owner approved separating [viewport, visible region and requested render region](../docs/architecture/concepts.md#viewport-and-image-coordinates). Use those meanings consistently. Zoom and pan derive an image-space visible region from UI-space viewport geometry. The owner also requires Astra's code review at the end of each implementation slice and an opportunity to test each functional difference, following the [working instructions](../AGENTS.md#work-with-the-owner).

## Findings grounded in current code

| Area | Existing behavior and consequence |
| --- | --- |
| Quality policy | [Multi-edit submission](../crates/foveonic-gui/src/multi_edit_runtime.rs) and [main submission](../crates/foveonic-gui/src/main.rs) force Thumb for overviews. [Smart](../crates/foveonic-gui/src/rendering/smart.rs) is skipped in multi-photo views. Draft/Thumb promotion also occurs before the Smart-mode check and separately in main submission. Adding count thresholds alone would leave inconsistent policy. |
| View state | [Comparison layout](../crates/foveonic-gui/src/comparison_view.rs) caps preferred equal-size tiles and optimizes tile area rather than photograph area. Only the active photograph uses the full viewport interaction path. Layout order currently follows supplied paths. Per-photo identity must survive layout changes. |
| Refinement | Smart is UI-driven and retains one initial region per source/edit sequence. The UI requests whole-image continuation after delivery. Panning intentionally does not request another region. A moving loupe needs a reviewed replacement policy rather than inheriting these rules. |
| Regional processing | [Recipe settings](../crates/foveonic/src/render/recipes/mod.rs) clear regions on reduced working grids. [Capabilities](../crates/foveonic/src/render/settings.rs) restrict native regions by camera, geometry and selected processing. Adaptive Tone, rotation and monochrome can require whole-image work. Regional correctness remains an engine concern. |
| Source readiness | The loader removes requested I/O from render execution but retains only one active or unconsumed source handoff. It is not a decoded-RAM cache. [Reduced sessions](../crates/foveonic/src/render/reduced.rs) retain no decoded samples. The worker holds one native session, not eight. Disk-ready and reduced-ready do not establish native-detail latency. |
| Memory | Decoded samples are native floating-point codes. Retained Full photographic outputs and textures also matter. Removing a native session does not release images still held by UI/coordinator snapshots. Account for old/new replacement overlap and shared allocations once. |

## Proposed review slices

Review names and behavior with the owner before implementing each item. Preserve one supported path as callers migrate.

1. **Resolve preview quality in one place.** Use one ordinary policy function for single and multi-photo requests. Resolve explicit mode versus Smart, displayed count and detail intent. Keep camera-specific scale resolution in core capabilities. Remove the scattered promotion decisions rather than wrapping them with another override. Start with quality-policy unit cases and targeted headed mode/zoom checks.
2. **Give photograph viewing state one home.** UI owns per-photo Fit/detail, zoom and pan, keyed by photograph identity. Single and multi-photo drawing use that state. Layout produces viewports and display order without deciding render quality or reading files. Visible regions derive from each photograph's transform and viewport. Pointer/Hand interaction and double-click behavior can then share a consistent gesture path.
3. **Move refinement sequencing to coordination.** UI publishes the needed visible/detail region. The coordinator requests useful rendering and handles replacement, independently of UI paints. Region completion may settle without a Full continuation. Preserve existing engine region capabilities and immutable image retention.
4. **Share detail requests with the loupe.** Start with one replaceable detail request per photo through the existing detail target. Reuse compatible Full or regional pixels by identity, edits, coverage and sampling. Circular clipping is display work over a rectangular rendered region. A loupe may replace uncovered detail demand while the zoomed tile keeps its completed compatible image. Do not union distant rectangles automatically or create a generic consumer registry. Simultaneous fresh refinement of two disjoint regions in one photograph is deferred unless demonstrated necessary.
5. **Choose retention from measurements.** Measure decoded disk loading, native RAM reuse, native preparation, region rendering, optional whole-image work and presentation separately. Only add byte-bounded native source retention if materialization is the relevant bottleneck. Keep native samples out of source-free reduced sessions. If reconstruction or retained Full output dominates, decoded-RAM caching alone cannot solve it.

Aspect-aware layout is a largely independent UX slice after stable per-photo viewing state is established. Avoid expensive permutation searches during UI drawing. Optional idle source acquisition through the loader is another separate ownership slice whose admission and foreground yielding need review together.

## Evidence required

- [ ] Quality matrix across explicit modes, Smart counts one through eight, Fit/detail, selection and framing. Count displayed photos rather than editing targets. Verify explicit reduced modes do not promote, while the loupe still receives Full.
- [ ] Layout and interaction checks with mixed aspect ratios, reorder, preserved selection, persistent zoom/pan and double-click Fit restoration.
- [ ] Detail results in transit during edit, pan, loupe movement, removal and reordering. Retain compatible displayed images while replacing work. A region-only request must settle without starting an unwanted whole-image render.
- [ ] Regional/full equality for supported recipes and correct whole-image fallback for unsupported ones. Reduced-region execution remains unimplemented until its measured benefit justifies that engine change.
- [ ] Repeated A/B native-detail switching from retained Full pixels, a resident native session, a source-free reduced session plus disk cache, and an original read. Include active Adaptive Tone and representative camera families.
- [ ] Memory for sources, processing checkpoints, Full/region outputs and display textures, including replacement overlap and low-budget behavior. No silent quality downgrade to meet an unmeasured memory assumption.

The first discussion is the unified quality policy and its vocabulary. Measurements can establish a baseline for detail recovery without changing caching policy. No whole-image continuation heuristic, additional source cache or loupe implementation is selected by this review alone.
