---
summary: "Render coordination separates request meaning, work matching, target scheduling, delivery and serialized execution."
---

# Render coordination

Requests describe desired photo results and preview policy. Matching compares requests and settings without scheduling work. The coordinator owns per-target demand, task identity and the result handler for settlement, reuse and admission. The runtime runs that coordinator on its own thread, handles typed commands and forwards bounded results. Delivery supplies classification and the UI's final view check. The UI applies permitted updates and acknowledges Active image delivery. Native detail handling publishes visible detail intent and preserves compatible presentation. The coordinator owns its regional completion and optional whole-image continuation. Keep these responsibilities distinct from the worker's processing resources and execution order.

Authored edits remain with the application, multi-edit runtime and sidecar session. Numerical graph validity, intermediate reuse and operation implementations remain in `foveonic`. This module coordinates that engine without reproducing its processing graph.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[worker/](./worker/AGENTS.md)** The serial render worker consumes ready sources and owns presentation reuse, processing sessions and idle preparation.

### Source files

- **[coordinator.rs](./coordinator.rs)** Owns desired photo renders, handles their results and publishes immutable execution plans.
- **[delivery.rs](./delivery.rs)** Classifies completed images for pixel reuse, per-photo feedback and current-view delivery.
- **[matching.rs](./matching.rs)** Defines shared settings and request comparisons without selecting work or admitting results.
- **[mod.rs](./mod.rs)** Coordinates photo render requests, matching, refinement and delivery through the existing worker.
- **[quality.rs](./quality.rs)** Resolves processing scale from the selected policy and physical display sampling.
- **[request.rs](./request.rs)** Describes photo render requests and resolves their purpose and preview geometry independently of execution.
- **[runtime.rs](./runtime.rs)** Owns render coordination on a dedicated thread and publishes bounded UI updates.
- **[smart.rs](./smart.rs)** Publishes native-detail intent and preserves compatible image presentation without sequencing jobs.
- **[source_loader.rs](./source_loader.rs)** Acquires decoded sources off the renderer and hands bounded results through coordination.

<!-- INDEX:END -->
