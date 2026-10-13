---
summary: "The all-enabled benchmark separates completed renders, transfers, export, and bit-exact photographic checks."
read_when: "Measure pipeline performance or compare an optimization with retained float-image goldens."
---

# Pipeline measurements

Run `profile_pipeline` as described in [the performance plan](../../../plans/pipeline-performance.md). Use a new export directory for each round. Existing goldens are compared, never overwritten. Diagnostic full-buffer readback is outside timed rendering. Process RSS and cumulative GPU allocation bytes are not peak GPU residency.

Setup reports complete file-read, in-memory decode and total source-open wall times separately. The source's nested decoder/plane timings overlap; do not sum them with the decode total. These timings describe the one source opening, not each warm render. Filesystem cache state is uncontrolled: measure slow SD-card openings on the actual medium, independently of warmed local-drive processing.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[profile_pipeline.rs](./profile_pipeline.rs)** Measures completed fresh processing with normal photographic controls, separate GPU diagnostics and optional exact goldens.

<!-- INDEX:END -->
