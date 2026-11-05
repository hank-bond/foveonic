---
summary: "Consolidate photograph-tool ownership, shared framing and menu geometry, then verify and commit."
status: archived
---

# Tool foundation cleanup

Implement the [workspace tooling requirements](../../docs/specs/gui-workspace.md). The owner approved cleanup and commits for the current tools, keeping the combined Crop/Rotate rendering coherent and making future tools straightforward to add. A zoom/loupe is context for the foundation, not part of this implementation.

- [x] Give photograph-tool selection and cancellation one owner.
- [x] Keep tool-local gestures, asynchronous replies and options separate from selection.
- [x] Remove cross-tool activation chains and redundant presentation routing.
- [x] Review saved rotation geometry and remove redundant numerical construction.
- [x] Tighten shared split buttons and anchor their menus to the complete button.
- [x] Verify gesture, picker, geometry and native workflows, then commit in coherent groups.

Preserve the independent comparison-image changes. Use a small concrete tool enum and shared routing. Avoid a plugin framework or speculative tool capabilities.

## Review findings

The previous activation rules were distributed across five tool modules and their panel/keyboard callers. A single concrete tool enum now owns selection, explicit cancellation and return to Hand. Gestures, picker tokens, pending replies and options remain local. Explicit selection clears pending replies even after a picker has disarmed. Source changes and pointer routing share the same coordinator. A common frame-eligibility check no longer couples White Balance to the Color Range enable state. The native WB check now bypasses Color Range before picking.

Combined Crop/Rotate interaction and overlay composition live in the framing module. Main and navigator use a named presentation value instead of an angle/size tuple. Saved rotation geometry computes normalized output mapping directly, removing an intermediate conversion through output pixel dimensions. Exact native sampling remains a required active path for neutral fine angles.

Rotation interaction tests are adjacent to the controller. Native workflow checks remain opt-in. Shared split buttons use a narrower options segment and the complete control rectangle for popup placement. A pointer-driven menu regression verifies both the anchor and separate action/options targets.

## Verification

Workspace tests and strict all-target Clippy pass. GPU checks verify shared rotated preview/TIFF/DNG/picker sampling and exact quarter-turn presentation. Native workflows pass for combined framing, line and two-click alignment, direct color editing, Dehaze, and white balance with Color Range disabled. Menu interaction checks verify the complete-button popup anchor without invoking the main action. Documentation indexes, formatting and artwork checks pass. Independent comparison artifacts and scripts remain outside the grouped rendering and GUI commits.
