---
summary: "Establish a shared bounded Perfetto recorder with scalar context and source/cache/CPU boundary observations."
status: archived
---

# Perfetto recorder foundation

Requirements: [performance diagnostics](../../docs/specs/performance-diagnostics.md).

- [x] Add a shared SDK recorder with scoped thread events, flows, counters, explicit elapsed observations and generation-bound scalar context.
- [x] Keep capture control and file output on one background controller thread with bounded configuration and asynchronous replies.
- [x] Observe actual source, preview and decoded-cache reads without extra reads or diagnostic recency changes.
- [x] Propagate context through the existing CPU pool and observe disk cache admission, eviction and publication at their owners.
- [x] Check disabled callbacks, cross-thread capture, bounded buffers, transition behavior and reader delegation through tests.

This commit establishes the processor API and CPU/I/O/cache observations. GUI recording controls, graph/device instrumentation, native scenarios and trace analysis remain separate integration work. It changes no work ordering, cache policy or numerical processing. Diagnostic cache namespaces stay beneath the existing temporary root and support isolated application tests.

Actual bytes returned by wrapped readers are distinguished from source identity and metadata checks. Disabled readers preserve the underlying bulk-read implementation. Timing scopes represent elapsed thread work. Explicit GPU observation APIs retain device clock provenance without claiming CPU-clock calibration. Capture completion and loss-free validity are separate concepts.

The isolated slice passed workspace tests, strict workspace Clippy, formatting and generated-index checks. SDK tests cover cross-thread flows, late observations, bounded captures and transition drops. Native UI and device-query acceptance belong to the later application integration, rather than this API commit.
