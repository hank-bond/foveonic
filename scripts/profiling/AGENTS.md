---
summary: "Profiling tools record owned native sessions, summarize shared JSON reports, and flag known overlapping workloads."
---

# Profiling tools

Follow [profiling requirements](../../docs/specs/profiling.md) and [measurement usage](../../docs/profiling.md). Keep portable analysis separate from platform-specific capture. Do not add compatibility formats for historical parsers or one-off experiments.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[behavioral.py](./behavioral.py)** Run declared real-app slider cases across camera fixtures and report observed coverage.
- **[compare.py](./compare.py)** Compare matched, uncontended native Off reports with explicit optional regression budgets.
- **[file_hash.py](./file_hash.py)** Hash profiling inputs and tools without retaining their complete file contents.
- **[native.py](./native.py)** Record shared native captures and inject explicit macOS pointer drags without changing app scheduling.
- **[render_matrix.py](./render_matrix.py)** Record or compare exact rendered samples across an explicit camera and preview matrix.
- **[resources.py](./resources.py)** Sample settled macOS memory at explicit boundaries in an owned headless render run.
- **[results.py](./results.py)** Report shared render observations and preview-scale assertions across test drivers.
- **[scenarios.py](./scenarios.py)** Run declared real-app diagnostic scenarios with isolated inputs and shared Perfetto artifacts.
- **[summarize.py](./summarize.py)** Summarize declared aggregate benchmark JSON; native SDK captures use trace_analysis.py.
- **[trace_analysis.py](./trace_analysis.py)** Query real Perfetto captures for loss, scenario correctness and shared-clock latency.
- **[watch_workloads.py](./watch_workloads.py)** Check and record actual app, compiler and measurement processes that can contaminate timings.

<!-- INDEX:END -->
