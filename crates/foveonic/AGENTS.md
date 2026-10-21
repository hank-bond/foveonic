---
summary: "The processor crate decodes X3F sensor layouts and shares a GPU graph for inspection, photographs and editable DNGs."
---

# Processor crate

Read [`src/AGENTS.md`](./src/AGENTS.md) before implementation. Follow the selected [`first-conversion.md`](../../plans/first-conversion.md) plan.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[examples/](./examples/AGENTS.md)** The all-enabled benchmark separates completed renders, transfers, export, and bit-exact photographic checks. _Measure pipeline performance or compare an optimization with retained float-image goldens._
- **[src/](./src/AGENTS.md)** The library separates source decoding, metadata preparation, typed graph construction, GPU execution, and TIFF packaging.

<!-- INDEX:END -->
