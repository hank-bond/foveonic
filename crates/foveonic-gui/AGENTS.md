---
summary: "The egui desktop app browses embedded previews and inspects named outputs of the shared GPU engine."
---

# Desktop GUI

Follow [`gui-and-batch.md`](../../plans/gui-and-batch.md) and [`pipeline-inspection.md`](../../docs/specs/pipeline-inspection.md). Keep photographic processing in `foveonic`; this crate owns interaction, background scheduling, and presentation. Share eframe's GPU device and queue with the processing engine.

Read [`src/AGENTS.md`](./src/AGENTS.md) before implementation.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[src/](./src/AGENTS.md)** GUI modules separate thumbnail browsing, interaction, bounded image jobs, and GPU presentation.

<!-- INDEX:END -->
