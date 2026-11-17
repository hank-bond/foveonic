---
summary: "Persistent support logs preserve launch context, startup progress and actionable failures for user-supplied issue reports."
kind: reference
status: accepted
implementation: conforming
---

# Support logging

## Owner direction

The owner approved implementing always-available local support logs in 0.8.2. Users must be able to provide actionable evidence when reporting problems, including failures before the application window opens. The [workspace status](./workspace-status.md) uses retained launch history from the shared support store. Detailed [performance captures](./performance-diagnostics.md) remain opt-in.

Optimize this work for diagnosing user-reported failures. Existing developer diagnostics already serve internal investigation. Performance analysis, latency measurement, memory tracing and user-facing Perfetto capture are outside this slice. User-supplied Perfetto latency and memory traces are a later goal. Static RAM and GPU-memory capacities remain part of system identification.

One run means one application launch. The Log modal should separate launches with spacing and a dated, timestamped header. Each header should offer Copy run logs. Copy logs at the bottom should copy all visible retained history. Both operations should include the relevant system, application and operation settings, rather than copying only displayed message text.

Clear should rotate the active file and clear the visible history. The owner confirmed that copied reports must exclude cleared entries. Older rotated files may remain on disk within the retention limit. Preserve the current launch identity and its context across clearing and automatic rotation. A persisted display/copy cutoff should keep cleared entries hidden after restarting.

## Records and retention

Use versioned JSONL records with a launch ID, ordered sequence, wall-clock timestamp, elapsed time, severity, event type and structured fields. Persist logs in a documented per-user application log directory. Initialize logging at process entry before argument/profile parsing and graphics setup. Write a minimal launch record first, then enrich context as system information, preferences and the selected graphics adapter become available. Unknown, unavailable and not-yet-loaded values must remain distinguishable.

Defaults are one active file plus three closed archives, with rotation at 1,000 records or 4 MiB, whichever comes first. Bound individual records at 64 KiB and report truncation. Split context across bounded records when necessary to retain essential settings. Each new segment must repeat the original launch context and latest resolved context so retaining only recent segments does not erase essential evidence. File segments must not appear as additional application launches. Multiple application instances must not interleave writes or rotate one another's active files. Each concurrently running instance may retain its own active segment in addition to the shared closed-archive allowance. Protection of active files takes precedence over pruning. The implementation plan defines file ownership and locking.

A bounded writer queue and one writer thread should keep ordinary disk I/O away from the UI and processing threads. Report dropped-event counts and preserve capacity for errors. Write complete lines promptly, without syncing every routine event. Orderly shutdown and handled startup failures, including explicit process-exit paths, must attempt a bounded drain so the final error is not abandoned in the queue. Logging failure must not indefinitely delay exit. Support reads must tolerate a truncated final line. Logging failure must be visible without recursively attempting to log that same failure.

Copied reports must disclose incomplete history caused by retention, overflow or truncation. Essential launch identifiers and settings must survive repeated context headers rather than being silently removed by the record-size limit.

## Diagnostic context

Record the application version/build and architecture, exact OS product/version/build, CPU model and core counts, physical RAM, adapter identities, selected GPU/backend/driver, relevant device limits, and application processing, cache and display settings. Capture graphics compiler identity when available. Relevant launch overrides should use explicitly supported fields rather than an unrestricted environment dump.

On macOS, retain the product build identifier separately from the Darwin kernel version. An unavailable query must remain explicitly unavailable.

Keep dedicated GPU memory, shared-system-memory allowances and unified-memory working-set guidance separately labeled. Do not report a device buffer limit as physical VRAM or add these different memory quantities together. Before adapter selection, report selection as pending rather than guessing from another adapter or previous launch.

Operation records should retain job identity, source/destination, available camera and image details, output format/encoding, requested processing policy, effective recipe and completion/failure with elapsed time. Capture frozen settings used by that operation. A later UI snapshot must not replace an older export's settings. Export context must reflect camera-choice resolution and export overrides.

Support settings snapshots include transient working scale, processing quality, preview region, selected color-range preview, Dehaze estimation, processing sections and adaptive-tone tuning. These diagnostic fields must not change photo-sidecar or embedded editing-history serialization.

Failures should retain the attempted operation, relevant paths, full error cause chain, error kind and numeric OS code before errors are converted to UI strings. Preserve existing user-facing explanations. Log startup phase entry and completion, first presentation, coarse preparation progress, readiness and orderly exit. Entry records are needed to identify operations that never return.

The owner also requested stack traces on logged errors whenever possible. Capture a backtrace at the earliest useful error boundary and preserve it as the error crosses worker and UI boundaries. A trace captured later while displaying an error must not be represented as the original failure stack. Reuse available error backtraces, label capture location and unavailable/truncated traces, and include the trace in copied reports.

Routine logs should retain activity messages and warnings/errors at source loading, processing and export boundaries. They should not collect per-frame, pointer-motion, shader-dispatch or every slider-update events. Adding support-log links to optional performance captures is deferred with user-facing trace collection.

## Startup and crash limits

Install an early panic hook that preserves the existing hook and records the payload, thread, source location and best-effort backtrace. Its emergency write must not depend on an available writer queue or a lock held by the panicking thread. Preserve startup breadcrumbs before beginning potentially failing graphics operations. Include the log location in startup failure guidance.

Failures before Rust process entry, such as an executable or DLL loader failure, cannot create an application log. Rust panic handling also does not establish coverage of native access violations, forced termination or every driver crash. A missing orderly-exit record means an incomplete launch, not a confirmed crash diagnosis.

## Implementation and acceptance

Connect early logging, progressive system context, existing activity messages and startup/export failure context to the persistent sink and requested modal controls. Validate useful reports for an early startup failure and the Windows export workflow before expanding event coverage. Select system-information dependencies and any narrow library warning/error adapter during implementation review. Avoid introducing a parallel profiling framework.

The Astra-led audit in the [implementation plan](../../plans/support-logging.md) identifies shared boundaries, correlation and error ownership across the application. Implement those user-failure boundaries in reviewed slices for 0.8.2. Structured logging and error-handling crates are approved when they simplify the implementation and follow Rust conventions. Keep existing typed control-flow errors and processing behavior intact. Acceptance requires the end-to-end persisted reports, modal controls, error context and boundary coverage, rather than a design document or Activity-only persistence.

Checks should cover grouping two launches, rotation within a long launch, preserved context, Clear followed by restart, copy/view agreement, oversized records, queue overflow, unwritable logs, a partial final line, concurrent producers and process isolation. Child-process checks should exercise a failure and panic before UI creation. An export whose frozen settings differ from later UI edits must report the export's settings. Synthetic adapter contexts should cover discrete, unified and unavailable memory information.
