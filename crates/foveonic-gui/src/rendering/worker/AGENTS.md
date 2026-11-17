---
summary: "The render worker consumes ready sources, coordinates grouped execution and owns presentation reuse, processing sessions and idle preparation."
---

# Render execution

`mod.rs` owns service order, request execution and reply publication. Presentation reuses immutable whole-image results. Processing owns verified photo identities, native/reduced session lookup and acquisition through the source loader. Its engine working set shares foreground-use recency, required-session protection and budget trimming across both session kinds. Reduced preparation can borrow a matching retained native source without promoting the native session. Verified source replacement invalidates both kinds. Ordinary view clearing parks retained processing; explicit processing clearing releases both kinds. Jobs needing source data defer without settling, while other ready jobs can execute. Idle preparation uses the same owners under foreground priority and spare-capacity admission. Keep operations separate from the decisions that select and interrupt them.

Presentation snapshots are shared with the coordinator and inventoried by borrowing their existing resource handles. Keep snapshot reuse separate from numerical dependency validity.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[admission.rs](./admission.rs)** Estimates coarse workspace headroom for admitting additional warm Merrill photographs.
- **[diagnostics.rs](./diagnostics.rs)** Captures immutable app images and controls delivery timing for opt-in native checks.
- **[mod.rs](./mod.rs)** Keeps rendering off the UI thread and replaces outdated requests instead of queuing edits.
- **[preparation.rs](./preparation.rs)** Prepares ready decoded neighbors during idle time using the render worker's existing owners.
- **[presentation.rs](./presentation.rs)** Reframes one immutable whole-image result without reopening sources or running photographic nodes.
- **[processing.rs](./processing.rs)** Owns verified per-photo native and reduced sessions under the shared processing budget.
- **[wide.rs](./wide.rs)** Interleaves warm photo executions while ordinary worker code owns delivery.

<!-- INDEX:END -->
