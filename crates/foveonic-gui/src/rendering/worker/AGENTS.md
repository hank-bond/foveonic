---
summary: "The serial render worker consumes ready sources and owns presentation reuse, processing sessions and idle preparation."
---

# Render execution

`mod.rs` owns service order, request execution and reply publication. Presentation reuses immutable whole-image results. Reduced processing owns worker-local session lookup and requests native recovery through the source loader. Reduced helpers borrow the native slot. Native replacement and clearing stay with the worker. Jobs needing source data defer without settling, while other ready jobs can execute. Idle preparation uses the existing owners under foreground priority and memory admission. Keep operations separate from the decisions that select and interrupt them.

Presentation snapshots are currently shared with the coordinator. Preserve that existing boundary during file organization. Changing snapshot ownership belongs to the later resource/execution cleanup.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[diagnostics.rs](./diagnostics.rs)** Captures immutable app images and controls delivery timing for opt-in native checks.
- **[mod.rs](./mod.rs)** Keeps rendering off the UI thread and replaces outdated requests instead of queuing edits.
- **[preparation.rs](./preparation.rs)** Prepares ready decoded neighbors during idle time using the render worker's existing owners.
- **[presentation.rs](./presentation.rs)** Reframes one immutable whole-image result without reopening sources or running photographic nodes.
- **[reduced.rs](./reduced.rs)** Reuses bounded reduced-photo inputs while native requests keep the existing worker session.

<!-- INDEX:END -->
