---
summary: "Open architecture decisions are reviewed in implementation order instead of becoming a speculative type system."
kind: reference
---

# Open questions

The [active plan](../../plans/render-runtime.md) owns progress and evidence. This page holds the unresolved choices. Close a question by updating its owning contract and vocabulary rather than retaining conflicting answers here.

## Current unit: off-UI ownership

The owner approved moving request coordination and result handling off the UI thread. The runtime now handles typed commands, worker results and bounded UI delivery on a dedicated coordinator thread. Validation and owner review are tracked in the [active plan](../../plans/render-runtime.md#off-ui-ownership-proposal).

## Subsequent decisions

- **Requested state:** which existing state becomes authoritative for per-photo edits, and what immutable input does the render runtime need? Avoid rebuilding the whole editor state to move scheduling off the UI.
- **Communication fit:** Crossbeam selection services incoming commands while bounded image delivery is full. The UI command queue uses nonblocking publication with coordinator-side replacement before plan publication. Measure backlog under sustained editing before considering another transport. Independent broadcast/watch consumers would require a library with explicit state/event and lag semantics.
- **Execution control:** what small shared policy separates cancellation, yielding, priority and GPU completion? Decide the bound on already-submitted speculative work from measurements. A lower-priority host thread cannot guarantee GPU preemption.
- **Resource ownership:** which owner reports availability, decides admission and releases each representation? Clarify the existing serial lifecycle without building concurrent session admission or a second cache manager.
- **Workflow evidence:** what representative collection and interactions define time to cull and edit 100 photos? Existing scenarios cover up to eight at once. A collection journey should advance through editing groups, not enlarge the editing limit.

Loupe, focus/eye analysis and other culling UX remain future consumers of the runtime. Their concrete algorithms and interactions are outside this migration.

Wide execution and its session-admission/graph-advancement questions belong to the [separate deferred investigation](../../plans/wide-rendering.md). Clear ownership should make that experiment easier, but it is not a required implementation step or success criterion here.
