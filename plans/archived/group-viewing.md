---
summary: "Link Shift viewing gestures across viewport editing targets while retaining independent photo views."
status: archived
---

# Group viewing gestures

Implementation and independent checks are complete. The owner received the build and continued to the final wide-rendering cleanup. Its matching final build includes these behaviors for further owner testing.


Requirements: [multi-edit](../../docs/specs/multi-edit.md#per-photo-viewing) and [viewport workflows](../../docs/specs/viewport-workflows.md#layout-and-interaction).

## Approved behavior

With multiple blue editing targets, Shift-drag applies the same image-relative pan displacement. Shift-scroll and pinch multiply each photo's own zoom. Shift-double-click uses the clicked photo to choose one destination for the group: the configured zoom snap or Fit. A reset shares its resulting image-relative center as well as its zoom. Ordinary gestures remain local, and white photographs remain outside group viewing. Shift gestures on blue images preserve the editing group. Shift-click on white images and captions retains range selection.

Each photograph retains its own view. Viewport input emits a semantic change once, and the existing photo-view owner distributes it after drawing. Changes to undrawn views survive until geometry is available. For a never-drawn Fit view, relative factors accumulate and absolute zoom limits apply at its first usable layout, without a gesture queue.

## Work

- [x] Reproduce local-only Shift zoom and Shift-double-click selection changes with real egui input.
- [x] Implement shared gesture intent and per-photo relative transforms.
- [x] Prove mixed states, common reset, anchors, limits, drawing order, hidden peers and selection/tool isolation.
- [x] Independently review and complete checks.
- [x] Make the matching build available for owner testing.
- [x] Deliver the reviewed build and continue under the owner’s final cleanup authorization.

The initial real-input tests failed because Shift zoom affected only the hovered photo and the first Shift-double-click changed editing selection. They pass through shared semantic input and group selection isolation. Ten additional independent tests cover real wheel modifiers, drawing order, common reset, relative pan/zoom anchors, hidden/detached state and physical zoom bounds. A resized-output counterexample failed the immediate density shortcut, then passed after sampling was derived from current image size and native extent at both display densities.

Independent review is clear. Full default and experimental workspace suites, strict lint, formatting, documentation and Python checks pass. Native three- and four-photo journeys pass 78 and 81 transitions, including real Shift-pan, Shift-wheel, common Fit/snap, white-photo preservation and native detail coverage. These are correctness checks rather than latency benchmarks.
