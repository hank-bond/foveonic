---
summary: "Rendering separates shared settings, authored recipes, graph execution, and owned inspection artifacts."
---

# Rendering internals

The [public renderer](../render.rs) owns the engine and display/export entry points. Keep recipe construction separate from execution and artifact collection. Selective inspection must avoid preparing unrelated later metadata. Retained results and checkpoints stay immutable.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[recipes/](./recipes/AGENTS.md)** Photographic and DNG-base builders compose shared technical nodes before one executor runs the graph.

### Source files

- **[artifacts.rs](./artifacts.rs)** Collects requested graph outputs as owned images or diagnostic tables.
- **[banding_controls.rs](./banding_controls.rs)** Stores independent native-pixel widths and strengths for Merrill directional band cleanup.
- **[capabilities.rs](./capabilities.rs)** Resolves supported photographic adjustments and compatible defaults at family boundaries.
- **[edits.rs](./edits.rs)** Shares scoped photographic recipes and user defaults between interactive loading, batch work and preview caching.
- **[execute.rs](./execute.rs)** Executes typed graph operations with selective outputs and guarded GPU storage.
- **[settings.rs](./settings.rs)** Defines shared photographic settings and validates their values.
- **[speckle_controls.rs](./speckle_controls.rs)** Defines Merrill speckle detection thresholds and compact-neighborhood support.

<!-- INDEX:END -->
