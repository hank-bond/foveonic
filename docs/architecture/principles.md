---
summary: "Architecture changes prioritize responsive culling, clear ownership and one reviewable conceptual improvement at a time."
kind: reference
---

# Design principles

The [render orchestration requirements](../specs/render-orchestration.md) record the workflow and execution goals. These principles guide how we reach them.

## Align each slice with the three goals

Review maintainability, latency and future execution flexibility together, using the [agreed goals](../specs/render-orchestration.md#agreed-goals). Explain which responsibility becomes clearer, which coordination or waiting cost is removed or made measurable, and how application contracts remain independent of how many photographs the [Scheduler](../specs/render-scheduling.md) runs together. A simpler boundary can be worthwhile before it produces a measured speedup. Measure the benefit of running wider rather than assuming it.

## Optimize the interaction

Judge progress by how quickly someone can cull and edit a collection. Avoid unnecessary work and reuse valid data before optimizing individual node arithmetic. Background preparation is useful only when its foreground latency and memory costs are acceptable. Running photographs together is worthwhile only when their working photos stay in memory. Avoid thrashing before running wider.

## Give each decision one owner

Requested edits, work coordination, numerical validity and displayed results have different responsibilities. Each needs a clear authority. A derived summary or cache must not become another writable copy of requested state. Share decisions at their owning boundary so callers do not reconstruct policy through scattered conditions.

## Use few concepts with explicit boundaries

Prefer ordinary functions and collections. A planner can be a function of current requests and availability. It need not be another manager, thread or registry. Add a mechanism with its first concrete use. Keep operation implementation separate from the control flow that decides whether and when it runs.

Known consumers can use typed messages and explicit handler calls. Use library channels across threads. Track the application glue each unit adds, including which ready-made primitive could replace it. If a full synchronization or subscription primitive becomes necessary, use a library rather than recreate it.

## Preserve useful distinctions

Source identity, processing reuse and permission to display a result answer different questions. Disk readiness does not mean processing is prepared. Cancelling obsolete work differs from yielding still-useful work. Keep these distinctions where consumers need them, with names the owner understands.

## Build one understandable change

Start each unit with a conceptual pass: define meanings and owners, identify existing synonyms or overloaded terms, and review the proposed names. Use the agreed vocabulary consistently in contracts, code, tests and documentation. Implement the smallest complete production slice, explain the result, obtain owner review and revise the next step. Keep one supported implementation after a migration. An isolated experiment may compare alternatives without making both production policies.

## Let implementation correct the plan

Maintain the [implemented map](../interactive-rendering-architecture.md), vocabulary and active plan as each slice lands. Record what was learned and the next unresolved choice. A high-level direction does not justify silently combining several concepts into one large refactor.

This approach draws on UIX's [principles](https://github.com/hank-bond/uix/blob/main/docs/architecture/principles.md) and [human-paced implementation](https://github.com/hank-bond/uix/blob/main/docs/architecture/human-paced-implementation.md), adapted to this application's numerical and interactive contracts.
