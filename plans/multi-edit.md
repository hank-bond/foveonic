---
summary: "Implement independent viewport editing selection, Pointer-only selection routing and shared mixed-value controls before finalizing memory policy."
---

# Multi-photo editing

Implements the accepted [multi-edit requirements](../docs/specs/multi-edit.md). The owner separated unlimited browser selection from a bounded viewport editing subset. Multiple viewport-selected photographs directly enable multi-edit. An earlier explicit mode and later selection-lock idea were replaced by Pointer V as the only viewport selection tool. Other tools preserve that selection. Browser and editor interaction remain independent of viewport tools.

## Current slice: Pointer and tool scope

- [x] Add default Pointer V alongside Hand H, Crop and Rotate.
- [x] Return finished tools and one-shot pickers to Pointer while preserving temporary Space panning.
- [x] Allow viewport selection changes only with Pointer.
- [x] Remove rotation-tool interception and cursor changes from browser clicks.
- [x] Keep browser/editor pointer interaction independent of the viewport tool.
- [x] Validate shortcut focus handling, tool cancellation, browser selection and viewport routing.

## Viewport selection and presentation

- [x] Own browser selection, viewport editing targets and the active photograph separately.
- [x] Initialize viewport targets to the first L browser-selected photographs, default eight.
- [x] Implement normal, Option and Shift viewport selection within the configurable limit.
- [x] Keep large browser selections usable with shrinking tiles and an “N more” overflow.
- [x] Preserve each photograph’s latest edited preview across switches and reject stale baseline replacements.
- [x] Expose the multi-edit count limit in Settings independently of memory-cache controls.

## Mixed controls and rendering

- [x] Resolve the selected photographs’ settings asynchronously through the existing recipe owner.
- [x] Apply slider deltas to each target with independent clamping and a mixed-value range display.
- [x] Reset all targets on double-click and support explicit numeric assignment.
- [x] Show “Multi” for differing dropdown choices and assign one chosen value to all targets.
- [x] Preserve unrelated settings and each camera’s supported controls.
- [x] Render multi-edit overviews at Thumb through bounded worker scheduling.
- [x] Preserve Import’s memory-only edits and Browse’s asynchronous per-photo saves.
- [x] Verify rapid edits, selection changes, mixed values, saturation/reversal and stale replies end to end.

## Memory work after interaction review

- [x] Inventory all retained CPU/GPU payloads and transient peak resources, deduplicating shared allocations.
- [ ] Measure one active photograph and one through eight multi-edit targets across camera families and recipes.
- [x] Remove duplicated multi-output parameter tables and obsolete optional checkpoint retention where confirmed.
- [x] Agree one user-set memory budget with a sane default targeting about 2 GB steady and 4 GB peak on 16 GB machines.
- [x] Define active, multi-edit, history and speculative-preload admission/eviction priorities from those measurements.
- [x] Keep disk-cache settings and policy separate from memory settings.

Multi-target photograph-tool gestures are deferred. Pointer selection and tool gestures must use separate routing so that later tool application can preserve the same editing subset. No selection-lock toggle or explicit multi-edit mode is planned.

## Validation and scope

The implementation scope includes the complete agreed multi-edit interaction and coordinated memory-cache work. Pointer routing is complete. Viewport selection, mixed controls, per-photo defaults, batch rendering and shared memory accounting are integrated as drafts and are being validated. The owner approved consolidating render coordination before further delivery fixes. The [scaled-pipeline plan](./full-option-scaled-pipeline.md) remains linked to the camera/scale and performance acceptance work. Preserve the existing uncommitted optimization work and unrelated help/release edits. Do not commit, publish or install this slice without owner approval.

## Pointer slice verification

Implemented default Pointer V, explicit Hand H and temporary Space panning. Tool toggles, completion and one-shot picker release return to Pointer. A shared viewport canvas routes Space independently of the selected tool. In the comparison viewport, Pointer changes the active selection and other tools preserve it. Browser thumbnail rotation interception and its now-unused asynchronous sidecar rotation queue were removed. Active-photo rotation still uses the ordinary settings and render path.

The Pointer slice passed its original 345 GUI tests, including shortcut/text-focus, browser interaction under other tools, Pointer selection routing, actual egui pan gestures and tool cancellation. Formatting, documentation checks, 22 profiling-tool tests and five artwork tests also passed. The broader integration has since passed strict workspace Clippy. Current integration acceptance is tracked below.

An optimized native bundle completed the ten-frame GUI smoke check, including the photographic view, browser list, scopes, zoom and sensor inspection. Visual review confirmed the selected Pointer button alongside Hand, Crop and Rotate. No commit, installation or release was performed.

## Render coordination design review

The owner requested a conventional concurrency design that addresses classes of delivery bugs structurally. The current integration draft duplicates request lifecycle state across App, multi-edit Runtime and Worker’s ordinary/batch slots. Independent revision, cancellation and completion state has exposed cross-photo delivery, orphaned batch work, stale automatic defaults and starvation during continuous edits. The owner approved the coordinator migration and asked for a simple implementation grounded in current needs and observed failures. The single render executor, immutable photographic buffers and bounded completion channel remain useful foundations.

Approved design: one UI-thread render coordinator owns immutable desired intents per photograph and purpose, generations, scheduling state and completion admission. Publish the latest complete bounded work plan through one replaceable mailbox. The worker owns the Engine, caches and numerical execution and returns typed immutable results. A multi-edit changes the target set atomically and does not introduce a second queue protocol. Before, inspection, scopes and picks carry distinct purposes or the identity of their source image. Exports retain a separate frozen command queue.

Cancellation is advisory. Result identity and coordinator admission must remain correct when cancellation loses a race. Cache validity is independent of current-view admission. Presentation changes retain compatible pixel identity. Continuous editing must coalesce work while preserving fair progress across targets, and must not restart the active photograph indefinitely. Any provisional feedback must advance displayed progress within its compatible interaction context and must never overwrite authored settings or mark an obsolete result settled.

- [x] Agree the coordinator’s ownership and request/result identities.
- [x] Replace duplicated ordinary/batch lifecycle and global cancellation state with the selected coordinator.
- [x] Test reordered completions, target removal, Before transitions, stale defaults, rapid edits and full mailboxes with a deterministic fake executor.
- [x] Verify fair progress during a continuous edit stream and latest completion after release.
- [x] Inventory actual queued, in-flight and displayed payload ownership under the shared memory budget.

The coordinator migration uses existing Rust synchronization primitives, one render executor, concrete request identities and tests for the observed delivery races. Keep further changes grounded in these owners and demonstrated needs.

## Integrated progress

The integration contains a bounded independent viewport selection, the configurable default of eight editing targets, compact comparison layout/overflow, explicit mixed numeric and choice descriptors, per-photo worker-resolved automatic/reset values, and a master memory-cache preference separate from disk caching. The latest complete ordinary workspace run passed 503 processor tests, 399 GUI tests and 52 supporting-crate tests. Strict workspace Clippy passed. These checks include the progress and retained-selection regressions.

Exact shared parameter payloads and generic optional-history trimming reduced the eight-photo 128 MiB history trial from 80 reloads to 20. A 256 MiB history trial retained all eight, completed warm edit groups in about 78 ms and retained about 217 MB of processing data. Fresh pixels matched cached results. The initial retained-cache default is 1 GiB to leave room for live processing within the owner’s approximate 2 GB steady / 4 GB peak goals. This budget is not an allocator or total-process cap.

The final native acceptance passed 17 captures and 50 unique eight-photo edit groups in both Wall and Off runs. It checked exact texture reuse during unchanged switches and re-adds, Thumb delivery with one or several editing targets, relative edits, target removal during rendering, Before/Current, mixed choices and settled progress. The Off run completed edit groups in a median 313 ms, with a 278–347 ms observed range. Logical retained data was about 1.06 GB, below the 1 GiB preference, and reported no overage. Settled macOS physical footprint was about 1.6 GB and process peak footprint was 3.72 GB. These measurements cover this eight-photo mixed-camera set and recipe. Broader recipe/count profiling and owner interaction review remain open.

Seven continuous Full slider streams published increasing image revisions before release and settled with matching scopes. Active restart inputs and graph identity survived forced memory pressure. Full TIFF/DNG export returned correctly to Fast/Smart previews. All 25 authorized original files retained exact decoded samples. The six-photo Quattro prefetch trial improved from about 6.27 s to 3.35 s after moving pure CPU preparation outside disk publication, while peak process footprint increased from about 1.65 GB to 2.69 GB. These are separate throughput trials, not native UI timings or evidence of full CPU utilization.

## Owner follow-ups

Scrubbing a mixed slider through a tiny fraction reproduced a one-frame readout overflow, expanding the editor from 330 to 344 logical pixels. Numeric formatting and width reservation now share the widget owner and reserve space before input changes the value. The owner selected `0.00` for tiny values while retaining stored precision. A real egui drag regression checks formatting and fixed panel width in both themes.

The owner reported a completed multi-edit leaving progress running. Completion now settles progress from the coordinator's remaining desired work, before display filters can skip a reply. Per-image revision guards remain responsible for stage updates. Sibling-order and removed-target regressions pass.

The owner requested all host CPU capacity across multi-photo processing. One persistent processing pool now uses the host's available logical CPUs. Parallel decoding and cache preparation share that pool. Pure CPU preparation, compression and checksums run before the existing disk-publication lock. Up to three photographs are admitted independently of CPU worker count. Disk I/O, UI work and serialized GPU execution retain separate owners. Dependency-bound streams and serial recipe walks remain serial.

Native checks caught two additional selection transitions. A compatible retained photo no longer creates an opening timer when selected, and a coalesced render intent no longer restarts completed timing. Removing an unfinished target leaves its latest edit eligible for rendering when that target returns. The native acceptance driver verifies exact texture reuse for unchanged switches and re-adds, latest edited delivery for unfinished re-adds, and stopped progress after settlement.

## Intermittent cached-selection investigation

The owner reported eight-photo Import selections taking 2–3.3 seconds despite green disk-cache indicators and Reference processing. Other selections were quick. Inspection found matching local decoded headers and complete companion bundles for both groups. During a slow mixed set, three decoded entries recorded successful local reads. This establishes some disk-cache hits, but does not yet attribute the complete selection delay or exclude all source accesses. The progress label measures the whole request lifecycle, rather than file reading alone. Trace initial selection separately from warmed exposure edits and preserve the distinction between local decoded data and retained processing checkpoints. A failed local read must also invalidate the green indicator immediately, including through later readiness scans.
