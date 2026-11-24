---
summary: "Render coordination separates request meaning, work matching, target scheduling, delivery and serialized execution."
---

# Render coordination

Requests describe desired photo results and preview policy. Matching compares requests and settings without scheduling work. The coordinator owns per-target demand, task identity and the result handler for settlement, reuse and admission. The runtime runs that coordinator on its own thread, handles typed commands and forwards bounded results. Delivery supplies classification and the UI's final view check. The UI applies permitted updates and acknowledges Active image delivery. Native detail handling publishes visible detail intent and preserves compatible presentation. The coordinator owns its regional completion and optional whole-image continuation. Keep these responsibilities distinct from the worker's processing resources and execution order.

Authored edits remain with the application, multi-edit runtime and sidecar session. Numerical graph validity, intermediate reuse and operation implementations remain in `foveonic`. This module coordinates that engine without reproducing its processing graph.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[worker/](./worker/AGENTS.md)** The render worker consumes ready sources, coordinates grouped execution and owns presentation reuse, processing sessions and idle preparation.

### Source files

- **[coordinator.rs](./coordinator.rs)** Owns desired photo renders, handles their results and publishes immutable execution plans.
- **[matching.rs](./matching.rs)** Defines shared settings and request comparisons without selecting work or admitting results.
- **[mod.rs](./mod.rs)** Plans viewport photos' images, reconciles render work against them and stores delivered images.
- **[quality.rs](./quality.rs)** Resolves processing scale from the selected policy and physical display sampling.
- **[rendered.rs](./rendered.rs)** Keeps the app's rendered images in one collection keyed by photo, look and output.
- **[request.rs](./request.rs)** Describes photo render requests and resolves their purpose and preview geometry independently of execution.
- **[runtime.rs](./runtime.rs)** Owns render coordination on a dedicated thread and publishes bounded UI updates.
- **[smart.rs](./smart.rs)** Sizes native patches around the visible region and compares image regions.
- **[source_loader.rs](./source_loader.rs)** Acquires decoded sources off the renderer and hands bounded results through coordination.
- **[view_images.rs](./view_images.rs)** Holds every viewport photo's view, its rendered images and the planning state carried between frames, without a GPU.
- **[view_plan.rs](./view_plan.rs)** Derives each viewport photo's render requests and drawn layers from its view, current request and rendered images.
- **[viewport_plan.rs](./viewport_plan.rs)** Plans every viewport photo's images from inputs read off the app, without a GPU.
- **[views.rs](./views.rs)** Reads what each viewport photo shows off the app, plans its view through the view owner and selects what is drawn.

<!-- INDEX:END -->
