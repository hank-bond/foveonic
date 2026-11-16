---
summary: "Support logging owns bounded persistence, snapshots and structured failure reports."
---

# Support logging

Follow the [support logging requirements](../../../../docs/specs/support-logging.md). Keep application events distinct from optional performance diagnostics. Preserve launch identity, context, visible-history cutoffs and error sources across worker and UI boundaries.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[application.rs](./application.rs)** Initializes executable-owned support reporting and collects static context without delaying startup.
- **[mod.rs](./mod.rs)** Stores bounded, user-visible support reports independently of performance captures.
- **[report.rs](./report.rs)** Captures typed failure context, causes and boundary backtraces.
- **[store.rs](./store.rs)** Owns bounded support-log queues, segment retention, visibility and emergency panic evidence.
- **[subscriber.rs](./subscriber.rs)** Routes application instrumentation and bounded dependency warnings into the support store.
- **[system.rs](./system.rs)** Collects static system capacity off the UI thread without recording unrelated host information.

<!-- INDEX:END -->
