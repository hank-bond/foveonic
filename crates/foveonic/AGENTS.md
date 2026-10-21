---
summary: "The processor crate decodes equal-resolution X3Fs and renders photographs and editable DNGs through a shared GPU graph."
---

# Processor crate

Read [`src/AGENTS.md`](./src/AGENTS.md) before implementation. Follow the selected [`first-conversion.md`](../../plans/first-conversion.md) plan.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[examples/](./examples/AGENTS.md)** The all-enabled benchmark separates completed renders, transfers, export, and bit-exact photographic checks. _Measure pipeline performance or compare an optimization with retained float-image goldens._
- **[src/](./src/AGENTS.md)** The library separates source decoding, metadata preparation, typed graph construction, GPU execution, and TIFF packaging.

<!-- INDEX:END -->
