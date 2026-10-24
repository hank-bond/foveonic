---
summary: "Added inline import review and bounded asynchronous processing with shared recipes and foreground priority."
status: archived
---

# Add import review and batch output

Requirements: [Import workflow](../../docs/specs/import-workflow.md), [GUI workspace](../../docs/specs/gui-workspace.md), and [editing sidecars](../../docs/specs/editing-sidecars.md). The owner approved source-protected import review, compact destination history, metadata matching, frozen asynchronous batches, and a shared Process action with remembered per-mode output settings.

## Steps

- [x] Share compiled GPU resources between independent executors and yield background submissions to foreground work.
- [x] Add source-preserving copying, compact import history, and explicit failure reporting.
- [x] Add an asynchronous batch queue with frozen settings and existing TIFF/DNG processing.
- [x] Add recursive grouped browsing, picks, hidden decisions, history filtering, and memory-only import edits.
- [x] Replace separate export/import actions with one toolbar row and per-mode processing settings.
- [x] Review ownership, asynchronous scan reconciliation, output rollback, and documentation.
- [x] Review the cleaned-up interface and implementation before the authorized checkpoint commit.

## Ownership and execution

The browser's active folder mode determines source-write policy. The app commits it only after folder discovery succeeds and configures sidecar persistence before selecting any photo. Cancelling a picker or failing discovery preserves the current source and review. Preference restoration includes the folder mode and covers explicit descendant paths. Existing preferences without a mode default to Browse.

Application preferences own the shared processing destination and separate Browse/Import output choices. The destination's persisted key remains unchanged so cleanup preserves the last chosen folder. Import review holds a destination snapshot only for its versioned asynchronous scan. Opening another source clears unimported edits and review choices. Already queued ordinary-folder sidecar saves finish on the existing writer.

The processing dialog edits output choices independently of photographic recipes. Browse processes the viewed photo through the image worker and temporarily disables edits. Import processes frozen picks through the separate background executor while interactive rendering retains priority. Extending background processing to Browse would be a further feature. Both paths share export rendering, packaging, and temporary output publication.

Each engine executor shares the device, ordered queue, compiled kernels and lazy optional kernels. Calibration estimates, GPU activity counters, render histories, and per-photo ownership remain independent. Background import work yields at bounded I/O and GPU submission checkpoints. It waits for its current GPU submission before submitting more work. Already submitted commands finish normally.

The batch worker owns one frozen batch and a replaceable source/history scan. It may service scans between batch photos. It retains one batch photo at a time. A completed local original supplies decode misses, avoiding another card read. A verified source cache hit may reuse the existing decode. Destination cache associations remain deferred.

## Review and persistence findings

History matching and hidden decisions use file identity independently of editing-recipe availability. An unreadable recipe still reports an error. Valid current review edits can replace it for processing without writing to the source. Hidden decisions require no per-photo sidecar.

Per-photo rollback includes output creation and history publication. If publishing history fails, that photo's newly created files are removed. Earlier successful files and the earlier in-memory history remain intact. There is no named session, catalog, audit log, or interrupted-job recovery system. Filename, modification timestamp and size remain the deliberately inexpensive identity heuristic.

A delayed history scan could previously replace newer import-completion status. The worker serializes scans and imports, so each scan now records how many successful imports already belong to its history snapshot. Review reapplies newer completion records after loading the snapshot. Checkbox cleanup keeps its separate receipt position so replay cannot erase choices made after the first completion. Destination and generation checks reject unrelated scan results.

## Browser and interaction findings

Folder discovery is iterative and deterministic, with root photographs before sorted descendant folders. Group geometry and navigation share a mixed-height virtual layout. Photo rows use stable path-based UI IDs. Without stable IDs, virtualization changed checkbox and range-selection targets when visible rows changed. Descendant labels remain relative to the chosen root.

Workspace shortcuts route through the raw-input hook before egui can also interpret Space as a button click or arrows as focus movement. TextEdit state identifies text and numeric editors, including DragValue editing. Editors, open menus and modal dialogs retain their keys. Browser focus remains available for accessibility without gating photo commands.

Dropdowns reserve their padded interaction height before their internal row is laid out. Outer row centering alone did not align them with earlier buttons. All tool-section headers receive the same leading gap. The browser owns picked/hidden counts and adapts the header to pane width.

Background decode prefetch follows the browser's displayed order. Active loading and rendering retain the shared foreground gate. Following an owner report of a delay before rendering began, inspection found unbounded full-JPEG reads and prefetch decoders retaining partial buffers while paused. Speculative prefetch now cancels through bounded I/O, decode-row, interleave and insertion checkpoints. Decoder children inherit a per-attempt foreground epoch. An interruption retries that file ahead of later rows after rendering finishes. Import batches continue to pause and resume. Full embedded-preview reads use the same bounded I/O chunks.

The opening request owns foreground priority and its start time from selection through recipe restoration and worker submission. Progress starts at selection and the first render retains that timestamp. This also exposes recipe-loading waits in the reported user latency instead of beginning the timer only after them. Warm edits retain their own request clocks.

The owner requested configurable parallel predecoding and explicit foreground thread priority. Prefetch now uses bounded groups of photo workers with one CPU decoder per photo, defaulting to three workers capped at available logical cores. Active photos keep three independent plane decoders. Background workers inherit cancellable contexts and Background QoS. Active loads and plane workers explicitly request User Initiated QoS on macOS. Policy and folder changes invalidate the speculative epoch too. Completed cache publication remains serialized to protect LRU budget checks, and its lock wait is cancellable. Group results retry unfinished paths in browser order. The controller joins each bounded group before maintenance, preventing clear/eviction from racing publication. Increasing the budget increases bounded in-flight memory as well as CPU use. The settings help explains this tradeoff.

## Validation

Workspace and GUI checks cover recursive discovery, failed-open preservation, preference restoration, protected source edits, shortcuts and text focus, virtualized navigation, checkbox/eye behavior, and delayed-scan reconciliation. Identity/history tests cover malformed recipes, preserved timestamps, collision suffixes, hidden reversals, removed-original eligibility, and malformed history.

GPU checks with the supplied ordinary Quattro original compare batch and interactive TIFF/DNG bytes against independent Full exports. They verify frozen settings, foreground priority, independent activity counters, output-space preferences without changing photo recipes, copy-only imports without GPU work, partial failure, and rollback after history publication fails.

The isolated native macOS sequence captures both settings dialogs and the single toolbar row, processes the same original twice into identical DNGs with distinct names, then completes copy-only import and a descendant hidden decision without source sidecars. It closes its own window and uses isolated source, preferences and destination folders.

A real-original interruption check starts prefetch first, acquires foreground priority, verifies speculative work unwinds while foreground priority remains held, opens another original, and then checks resumed FIFO order and identical decoded samples. A separate deterministic thread test verifies child workers unwind before foreground work finishes and that even a completed foreground interruption invalidates the old attempt.

Thread-budget checks cover legacy preference restoration, core-count clamping, concurrent photo workers, cancellation while waiting for publication, and serial speculative planes versus parallel active planes. A native macOS check reads back the assigned QoS for active loads and inherited background contexts. The bundled Smart-preview sequence completes, and the settings capture shows the new thread control with the default of three. The full workspace suite and strict Clippy pass. A local six-copy Quattro cache pass took about 19.6 seconds with one background decode thread and 8.9 seconds with three. Both passes reused filesystem-cached source bytes and verified every restored sample, so these timings describe local CPU/cache throughput rather than cold SD-card performance. A separate foreground interruption check acknowledged cancellation in about 2.3 ms and completed the independently selected decode in about 523 ms. These are observations, not scheduling guarantees.

Cross-platform native use remains outstanding. [macOS 15 compatibility](../sequoia-compatibility.md) is tracked separately. Cache aliases and automatic card detection retain their deferred status in the specification.
