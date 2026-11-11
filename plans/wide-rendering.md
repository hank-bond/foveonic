---
summary: "Preserve deferred wide-rendering hypotheses and evidence for a separate engine investigation after application ownership is clearer."
---

# Future wide-rendering investigation

This is deferred exploration, not part of the active [render ownership migration](./render-runtime.md) or authorization to change engine execution. That migration should make this investigation easier by separating application requests, service policy and delivery from engine execution order. Do not add cursors or batching abstractions now solely for this future use.

## Hypothesis

Advancing ready operations across several photographs may reduce multi-edit latency compared with completing each whole graph serially. Test stage interleaving, submission grouping and completion-wait grouping separately. Packing several photographs into a shader dispatch is a further mechanism, not a prerequisite. Single-photo execution must use the same engine runtime as a one-photo group if a wide approach is adopted.

## Findings to retain

- The current executor keeps per-photo graph/cache, live-value, scratch and progress state and waits after each executed operation when execution is cancellable. Changing the outer loop alone does not remove those waits.
- GPU helpers submit internally. Stage interleaving does not automatically combine submissions.
- Reduced working sets currently protect one active entry. Several in-flight sessions need explicit protection until submitted operations retire, within their existing resource ownership.
- Recipes and cache hits differ. Advance independent ready work without aligning graph ordinals or imposing an all-photo stage barrier. Preserve per-photo source identity, cache outcomes, errors and settlement.

Sources: [executor](../crates/foveonic/src/render/execute.rs), [GPU facade](../crates/foveonic/src/gpu.rs), [interactive sessions](../crates/foveonic/src/interactive.rs), [reduced working sets](../crates/foveonic/src/render/reduced.rs) and the [overhead investigation](./coordination-overhead-profile.md).

## Eventual experiment

Start with warm Merrill Thumb exposure edits at widths 1/2/4/8, then mixed cameras, recipes and cache states. Measure first result, complete-group latency, useful throughput, obsolete work, graph preparation, submissions/waits, GPU activity, peak memory and foreground interruption. Preserve exact pixels and independent result settlement. Existing `profile_editing --workers N` runs complete renders concurrently and is a comparison baseline, not stage-wide execution.

Review the engine contract and names when this effort is opened. Retain an approach only if measured benefit justifies its complexity and resource cost.
