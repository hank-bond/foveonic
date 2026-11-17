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

- **[allocation.rs](./allocation.rs)** Bounds GPU allocations for audited resolved operations before additional work is admitted.
- **[artifacts.rs](./artifacts.rs)** Collects requested graph outputs as owned images or diagnostic tables.
- **[banding_controls.rs](./banding_controls.rs)** Stores independent native-pixel widths and strengths for Merrill directional band cleanup.
- **[capabilities.rs](./capabilities.rs)** Resolves supported photographic adjustments and compatible defaults at family boundaries.
- **[driver.rs](./driver.rs)** Drives shared graph execution and claimed session renders through preparation and settlement.
- **[edits.rs](./edits.rs)** Shares scoped recipes, user defaults and camera-derived numeric editing values.
- **[execute.rs](./execute.rs)** Executes typed graph operations with selective outputs and guarded GPU storage.
- **[experimental.rs](./experimental.rs)** Groups shared session executions behind opt-in submission windows.
- **[memory.rs](./memory.rs)** Inventories shared processing owners and trims optional history under one caller-set budget.
- **[preparation.rs](./preparation.rs)** Reuses bounded immutable metadata preparation independently of decoded sensor ownership.
- **[quality.rs](./quality.rs)** Separates photographic processing quality from the image's working resolution.
- **[reduced.rs](./reduced.rs)** Prepares source-free camera dependency cuts and renders reduced sessions.
- **[scale.rs](./scale.rs)** Describes power-of-two sensor sampling and the covered, padded working grid.
- **[settings.rs](./settings.rs)** Defines shared photographic settings and validates their values.
- **[source_preparation.rs](./source_preparation.rs)** Retains the two source-derived calibration statistics without owning sensor samples.
- **[speckle_controls.rs](./speckle_controls.rs)** Defines Merrill speckle detection thresholds and compact-neighborhood support.
- **[working_set.rs](./working_set.rs)** Retains native and reduced sessions with separate foreground recency and execution protection.

<!-- INDEX:END -->
