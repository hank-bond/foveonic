---
summary: "Open architecture decisions are reviewed in implementation order instead of becoming a speculative type system."
kind: reference
---

# Open questions

The [active plan](../../plans/render-runtime.md) owns progress and evidence. This page holds the unresolved choices. Close a question by updating its owning contract and vocabulary rather than retaining conflicting answers here.

## Current review: source acquisition

Request coordination and result handling now run on a dedicated coordinator thread. The runtime handles typed commands, worker results and bounded UI delivery independently of UI paints. Validation is recorded in the [active plan](../../plans/render-runtime.md#off-ui-ownership-proposal).

Requested source acquisition now uses a separate I/O thread, the existing disk cache and CPU decoding pool, and one bounded decoded-source handoff. A renderer job defers when input is missing, and coordination publishes per-photo readiness. Presentation/session reuse remains ahead of acquisition. The [active slice](../../plans/render-runtime.md#alignment-for-every-slice) tracks validation before owner review. Subsequent source questions concern optional idle acquisition and cancellation of an already-running read, rather than creating another cache owner.

## Subsequent decisions

- **Requested state:** which existing state becomes authoritative for per-photo edits, and what immutable input does the render runtime need? Avoid rebuilding the whole editor state to move scheduling off the UI.
- **Communication fit:** Crossbeam selection services incoming commands while bounded image delivery is full. The UI command queue uses nonblocking publication with coordinator-side replacement before plan publication. Measure backlog under sustained editing before considering another transport. Independent broadcast/watch consumers would require a library with explicit state/event and lag semantics.
- **Execution control and resource ownership:** decided. The [Scheduler](../specs/render-scheduling.md) owns the queue, priority, working set, admission and ticks. Its implementation follows the [scheduler plan](../../plans/render-scheduler.md).
- **Workflow evidence:** what representative collection and interactions define time to cull and edit 100 photos? Existing scenarios cover up to eight at once. A collection journey should advance through editing groups, not enlarge the editing limit.

Loupe, focus/eye analysis and other culling UX remain future consumers of the runtime. Their concrete algorithms and interactions are outside this migration.

Running several photographs together is part of the [Scheduler](../specs/render-scheduling.md) and no longer a separate investigation.
