---
summary: "The all-enabled benchmark separates completed renders, transfers, export, and bit-exact photographic checks."
read_when: "Measure pipeline performance or compare an optimization with retained float-image goldens."
---

# Pipeline measurements

Run `profile_pipeline` as described in [the performance plan](../../../plans/archived/pipeline-performance.md). Use a new export directory for each round. Existing goldens are compared, never overwritten. Diagnostic full-buffer readback is outside timed rendering. Process RSS and cumulative GPU allocation bytes are not peak GPU residency.

Setup reports complete file-read, in-memory decode and total source-open wall times separately. The source's nested decoder/plane timings overlap; do not sum them with the decode total. These timings describe the one source opening, not each warm render. Filesystem cache state is uncontrolled: measure slow SD-card openings on the actual medium, independently of warmed local-drive processing.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[compare_color_field.rs](./compare_color_field.rs)** Renders saved-sidecar highlight comparisons and measures completed fresh recipes without changing edits.
- **[compare_merrill_preview.rs](./compare_merrill_preview.rs)** Compares completed Merrill reduced-preview variants and writes a color-managed local review.
- **[export_dng.rs](./export_dng.rs)** Exports one supported original with explicit WB/mode for DNG reader validation, without selecting sidecars.
- **[measure_color_field_controls.rs](./measure_color_field_controls.rs)** Measures highlight selectivity in fresh/cached float outputs and RGB16, with matched preview regions.
- **[profile_pipeline.rs](./profile_pipeline.rs)** Measures completed fresh processing with normal photographic controls, separate GPU diagnostics and optional exact goldens.
- **[profile_smart_preview.rs](./profile_smart_preview.rs)** Compares completed Quattro detail-first and direct Full latency from matched Fast sessions.

<!-- INDEX:END -->
