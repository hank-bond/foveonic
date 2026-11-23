---
summary: "Architecture changes put one owner and one path per concern first, then responsive culling and one reviewable conceptual improvement at a time."
kind: reference
---

# Design principles

The [render orchestration requirements](../specs/render-orchestration.md) record the workflow and execution goals. These principles guide how we reach them.

## Keep one owner and one path

This is the first principle. Each concern has one owner and one path. When two pieces of code hold the same value or answer the same question, someone has to keep them in agreement, and that is where this project's bugs have come from. Remove the second path. Do not add handling for the cases where the two disagree.

Apply it in these situations. Each example is something this project built the other way and then had to undo.

- **When you need a value that already has an owner, read it from the owner.** The selected photo's settings lived in the app, in the editing runtime and in the sidecar session. Each frame copied them back and forth. A paste could be overwritten by a file read that finished later, because two copies each thought they were current. Now the editing store owns loaded settings and everything else reads from it.
- **When you change a value, send the owner an edit that names the change.** The panel edited a copy of the first photo's settings, and a diff worked out the edits afterwards. With several photos selected, the diff saw only the first photo, so a change that depended on each photo's own values could miss the others. Writers now name their change, and the store applies it to every target.
- **When a new case needs loading, saving or rendering, make it an input to the existing path.** The selected photo had its own sidecar load with its own ready, writable and dirty flags beside the path every other photo used. Rendering had five paths: serial, wide for one camera family, borrowed native sessions, preview prefetch, and fresh renders for the CLI. A new case is a priority or a target on the one path, and never a sibling of it.
- **When you need an answer something else already gives, ask it.** "Is this view still loading" was decided by the planner and again by a separate busy check. Memory admission was decided in the engine, in the GUI with hand-tuned constants, and again for prefetch. If the owner's answer is not enough, improve the answer there.
- **When you need a fact about a photo, read it from its one source and wait if it has not arrived.** The camera family was read from rendered metadata, or full metadata, or capture properties, whichever had arrived, and recipe resolution worked it out again. Photo size arrived by two routes. Now each photo has one summary, read once.
- **When something works on one item or many, write it for many.** Single-photo editing and multi-photo editing were separate paths with separate rules for automatic values. One photo is now a selection of one.
- **When a change always needs a follow-up step, the owner does it as part of the change.** After a recipe changes, the inspection choice may no longer be valid, and a recipe must fit its camera. Paste, editor actions and two places in the app each call the inspection check themselves. The camera fit runs in the store, again in the worker and again in core edits. A new writer that forgets the call leaves the state invalid.
- **When a fix needs a flag, a special case or a retry so that two paths agree, merge the paths.** The need for it is the signal that there are two paths.
- **When a test changes state, it does what a user would do.** Smoke scenarios wrote settings straight into app fields, so they did not exercise the path a real edit takes. Tests now write through the same edit the app uses, or through input events.

The cleanup that applies this principle started in 0.9.0. The [coherence cleanup plan](../../plans/coherence-cleanup.md) lists what remains.

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
