---
summary: "Render coordination separates request meaning, work matching, target scheduling, delivery and serialized execution."
---

# Render coordination

Requests describe desired photo results and preview policy. Matching compares requests and settings without scheduling work. The coordinator owns per-target demand, task identity and settlement. Delivery classifies how completed results may be used. Smart owns progressive detail requests. Keep these responsibilities distinct from the worker's processing resources and execution order.

Authored edits remain with the application, multi-edit runtime and sidecar session. Numerical graph validity, intermediate reuse and operation implementations remain in `foveonic`. This module coordinates that engine without reproducing its processing graph.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[worker/](./worker/AGENTS.md)** The serialized render worker integrates source acquisition, presentation reuse, reduced sessions and idle preparation.

### Source files

- **[coordinator.rs](./coordinator.rs)** Owns desired photo renders and publishes one immutable plan to the serialized executor.
- **[delivery.rs](./delivery.rs)** Classifies completed images for pixel reuse, per-photo feedback and current-view delivery.
- **[matching.rs](./matching.rs)** Defines shared settings and request comparisons without selecting work or admitting results.
- **[mod.rs](./mod.rs)** Coordinates photo render requests, matching, refinement and delivery through the existing worker.
- **[request.rs](./request.rs)** Describes photo render requests and resolves their purpose and preview geometry independently of execution.
- **[smart.rs](./smart.rs)** Refines reduced overviews into Full, using a first viewport tile where the recipe supports it.

<!-- INDEX:END -->
