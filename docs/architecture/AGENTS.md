---
summary: "Architecture guidance connects shared concepts, design principles and unresolved decisions to the implemented application."
---

# Architecture

Start with [principles](./principles.md) and [concepts](./concepts.md). The [implemented architecture map](../interactive-rendering-architecture.md) remains the single description of current ownership and control flow. [Open questions](./open-questions.md) distinguishes proposals from settled concepts. The [render runtime plan](../../plans/render-runtime.md) owns the current review unit and continuation instructions.

This is a small starting structure inspired by [UIX's architecture documentation](https://github.com/hank-bond/uix/tree/main/docs/architecture). Add conventions and narrower concept documents when implementation needs them. Keep existing repository contribution rules and indexes. Do not copy UIX's application vocabulary or create empty document categories.

Update implemented descriptions only when code changes. Discuss and record names before migrating callers. A proposed boundary is not an implemented owner, and a plan is not permission to implement every later unit.

<!-- INDEX:START -->

<!-- Generated from document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

- **[concepts](./concepts.md)** _(reference)._ Shared vocabulary distinguishes requested edits, render coordination, processing reuse and display without prescribing Rust types prematurely.
- **[open-questions](./open-questions.md)** _(reference)._ Open architecture decisions are reviewed in implementation order instead of becoming a speculative type system.
- **[principles](./principles.md)** _(reference)._ Architecture changes put one owner and one path per concern first, then responsive culling and one reviewable conceptual improvement at a time.

<!-- INDEX:END -->
