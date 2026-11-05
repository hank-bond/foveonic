---
status: archived
summary: "Audit graph execution, validate bounded repair preparation reuse, and record the selected and rejected cleanup trials."
---

# Processing node audit

This audit examined the processing graph after the [responsiveness campaign](./responsiveness-campaign.md). It reviewed recipe activation, inspection roots, preparation, GPU passes, cache dependencies and quality modes. The source baseline was `fc109b3`. Integration incorporated the owner's later committed `bde9502` GUI and release changes. Those later changes were outside the original processing measurements.

The graph contained 112 operation variants. The kernel enum contained 317 source variants. After the repair cleanup it has 315 production kernels and two test-only kernels. Shader includes and direct and indirect consumers were reconciled, including view preparation, scopes, output encoding, DNG, white-balance estimation, pickers and noise calibration. Registration or a neutral shader alone does not establish execution. A requested inspection output can require work that the final image skips.

Representative diagnostics covered Merrill Full, Fast and regional editing; Quattro and Tiger Full, Fast, monochrome and regional paths; and Classic photographic, linear-edit and DNG paths. All 46 finishing and graph-plumbing operations were observed executing in at least one selected case. Color and monochrome coverage included directional banding's measurement, correction, preview and regional stages. Sensor coverage included Color Field's nonempty solve, HighlightDensity, shield and row guards, and multiscale processing. A synthetic serial-matched defect exercised CalibratedDefects. ColumnGain's GPU arithmetic was checked with synthetic samples, but no available original reached its version-specific metadata wrapper. Operation execution does not establish that every helper, setting, camera or cache frontier was exercised.

The [profiler](../../docs/profiling.md) now records scoped operation identity, required outputs, cache availability, requested execution, published values, host time and engine activity. GPU diagnostics associate nested and shared timestamped passes with the operation after the existing trace resolution. Profiling Off adds no diagnostic collection or waits. Timed GPU passes exclude queue gaps, untimed copies and allocations. Host spans include driver work and sometimes waits. Neither allocation traffic nor elapsed timestamps establishes physical memory residency or display latency.

## Repair preparation reuse

Interactive Merrill sessions now retain at most one successfully prepared camera-repair record set under its exact effective threshold. The source-bound slot resets with engine ownership, uses checked capacity accounting and has a 4 MiB logical CPU cap. Oversized entries are not retained. Disabled and early inspection paths do not force preparation. Graph version keys and output dependencies retain exact equality. Stateless and DNG renders remain uncached. The cap excludes allocator overhead and the separate graph key.

Matched A/B/B/A runs used three Merrill photographs, two warmup edits and eight alternating 0/+0.5 EV Full-to-Full edits per process, with profiling Off. Baseline/candidate/candidate/baseline process medians were 63.95/51.15/53.66/62.85 ms for blue-sign, 61.18/50.93/52.05/59.09 ms for turquoise and 60.93/52.51/52.28/61.14 ms for glossy leaves. The mean of each pair of process medians fell by 17.34%, 14.38% and 14.16% respectively. These measurements include completed engine rendering and view preparation. They exclude source decoding, initialization and screen presentation. They are not a cold-start or general UI latency claim.

Separate untimed runs with the same benchmark settings compared baseline and candidate cached +0.5 EV floating outputs byte-for-byte on all three photographs. Requested and actual image settings, views and descriptions matched. Focused tests also covered threshold changes, disabled and re-enabled repair, cancellation, another source and engine, and retained image ownership. Monitor samples showed no recognized competing workload during the matched runs, although other application GPU use was not fully observable.

The sampled prepared record set retained 2,723,216 logical CPU bytes in the session. Its separate exact graph key was 1,383,372 bytes. Allocator overhead, peak physical memory, initial-render overhead, unusual oversized real metadata and every possible control invalidation remain unmeasured. A wider highlight or banding preparation cache was not included.

## Other trials and cleanup

- A shorter directional lookup prefix preserved table bits and matched complete float outputs and named `column.layers` captures. Alternating completed renders showed no reproducible end-to-end benefit, so the trial was rejected and restored. An isolated construction microbenchmark measured a small saving, but did not represent whole-render latency.
- Empty sensor-correction copy bypasses were not selected. Sampled originals had actual correction work, and an empty metadata list alone does not exclude image-derived detections or inspection dependencies.
- The low-level Repair shader was only dispatched by its GPU oracle. Its enum, registration and shader composition are now test-only, with compile-time eager-pipeline count alignment. The oracle and a production render/export smoke passed. No startup improvement is claimed.
- Three byte-identical compensated-arithmetic functions are shared by the chroma-repair and RGB-despeckle shaders. Their camera-specific bindings and formulas remain local. Focused GPU tests passed, and three named DP3 Quattro float outputs matched byte-for-byte. No full-final-image comparison was performed; independent native boundary fixtures were unavailable. This is a maintenance simplification with no performance claim.
- Earlier reduced Dehaze analysis was rejected after visible scene-dependent crop shifts. Adaptive fit reuse was rejected when input generations prevented reuse. Those results remain in the [responsiveness campaign](./responsiveness-campaign.md).

## Coverage limits and future leads

The sampled Merrill highlight flag array occupied 64,339,968 bytes, and its exact graph key also serializes the flags. Any future reuse work must account for retained and transient memory and preserve exact equality. A single active RowGuard diagnostic summed 169.25 ms of synchronized GPU passes. This is a lead for stage-level investigation, not an unprofiled latency or improvement claim.

Real ColumnGain metadata, independently trained matching cap-capture groups and independent Classic white-balance receipts were unavailable in the permitted local fixtures. Synthetic cap tests exercised selected reductions but not full real-capture training. Color-pick and cap-training paths have timestamp limitations. Merrill monochrome does not support Fast or regional processing, and regional Adaptive remains unsupported. A real monochrome Auto-zero case, some band-strength edits and guarded native boundary fixtures remain unmeasured. Some diagnostic sweeps overlapped another session's build or test work; those runs support execution coverage only, not timing comparisons.

## Validation

On the integrated checkout, `cargo test --offline --workspace`, strict all-target workspace Clippy, formatting and documentation-index checks passed. The GUI suite passed 318 tests with 28 ignored. Python asset and profiling tests passed 5 and 18 tests respectively. A copied release test executable ran ten focused GPU tests covering profiler attribution, cached-main/missing-side identity, cancellation, repair-session ownership and repair/chroma/despeckle arithmetic. Each selected test ran and passed. Broad ignored GPU tests and unavailable native fixtures were not represented as complete coverage. Independent source review found no integration blocker.
