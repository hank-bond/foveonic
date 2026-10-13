---
summary: "Profiling tools record owned native sessions, summarize shared JSON reports, and flag known overlapping workloads."
---

# Profiling tools

Follow [profiling requirements](../../docs/specs/profiling.md) and [measurement usage](../../docs/profiling.md). Keep portable analysis separate from platform-specific capture. Historical parsers and experiment orchestration remain beside their evidence under `.local/`; do not add live compatibility formats here.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[native.py](./native.py)** Record an owned native GUI session and inject explicit macOS pointer drags without changing app scheduling.
- **[summarize.py](./summarize.py)** Summarize native feedback and completed-processing JSON reports, keeping modes and stage domains separate.
- **[watch_workloads.py](./watch_workloads.py)** Flag known executing build, test and measurement programs without interpreting shell text as GPU contention.

<!-- INDEX:END -->
