---
summary: "The all-enabled benchmark separates completed renders, transfers, export, and bit-exact photographic checks."
read_when: "Measure pipeline performance or compare an optimization with retained float-image goldens."
---

# Pipeline measurements

Run `profile_pipeline` as described in [the performance plan](../../../plans/pipeline-performance.md). Use a new export directory for each round. Existing goldens are compared, never overwritten. Diagnostic full-buffer readback is outside timed rendering. Process RSS and cumulative GPU allocation bytes are not peak GPU residency.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[profile_pipeline.rs](./profile_pipeline.rs)** Measures completed cold/warm all-enabled processing, transfers, export, and optional bit-exact goldens.

<!-- INDEX:END -->
