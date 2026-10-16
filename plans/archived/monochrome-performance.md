---
status: archived
summary: "Column-parallel GPU histogram storage cuts monochrome render time roughly three to four times with exact photographic output."
---

# Optimize monochrome pipeline performance

Requirements: [profiling](../../docs/specs/profiling.md), [monochrome rendering](../../docs/specs/monochrome-rendering.md), and [render resources](../../docs/specs/render-resources.md).

## Scope

The owner reported roughly 2700 ms for completed pipeline rendering. The initial evaluation excluded UI latency. After reviewing the measurements, the owner approved a histogram-kernel optimization with unchanged filtering and exact output checks. The owner later approved cleanup, commit, and push.

- [x] Compare completed color and monochrome rendering on representative originals.
- [x] Separate ordinary wall time from synchronized GPU diagnostics.
- [x] Isolate histogram cost with a separate noise-0 diagnostic.
- [x] Preserve the baseline executable, shader, and photographic goldens.
- [x] Optimize histogram execution without changing arithmetic.
- [x] Check reference fixtures, full-sensor repeatability, cached edits, and exports.
- [x] Rerun matched benchmarks and record resource tradeoffs.
- [x] Run workspace, GPU/GUI, lint, format, and documentation checks.

## Measurement method

Use the release [completed-processing example](../../crates/foveonic/examples/profile_pipeline.rs) on Apple M1 Pro. Each case opens its source once and reuses the engine, but reruns the full recipe without an interactive cache. Render intervals include GPU completion and exclude source loading, setup, diagnostic readback, and export. Filesystem cache state is uncontrolled.

Settings are Sunlight WB, sharpening 0.75, default corrections, neutral exposure/tone, and 800×1200 TIFF packaging. Processing still runs at full sensor resolution. Color selects Standard and default reconstruction. Monochrome uses equal-layer mixing and noise/banding 2. Noise 0 skips the histogram and is not a quality-equivalent replacement.

The initial comparison used `--warm 3 --profile gpu`. All cases added `--wb Sunlight --sharpen 0.75 --size 800x1200` and a new export directory. Color added `--mode Standard --reconstruction`. Mono added `--rendering monochrome`, with `--mono-noise 0` for isolation. Warm wall measurements use no timestamps. A separate diagnostic run synchronizes per-stage GPU timing.

The owner closed the GUI before measurement. No known concurrent GUI or benchmark process was running. This does not rule out all system GPU activity.

## Initial findings

| Original | Color ms | Mono ms | Mono noise 0 ms | Histogram GPU ms |
| --- | ---: | ---: | ---: | ---: |
| Blue parking sign | 758 | 2625 | 387 | 2223 |
| Rocks and tree shadows | 794 | 2557 | 579 | 1985 |
| Sunlit red flower | 1019 | 3101 | 529 | 2566 |

Render times are medians of three warm completed renders. Mono ranges were 2619–2630, 2552–2595, and 3096–3127 ms. Histogram times come from separate diagnostics and are not additive to those wall samples.

Directional filtering took 16–30 ms. Other mono-specific stages each took roughly 1–5 ms. Submission returned in 70–83 ms, and TIFF export took about 11 ms. Ordinary rendering reported no CPU readbacks or timestamp resolves. Histogram execution, rather than metadata preparation, export, or UI scheduling, dominated the measured cost.

The original histogram used one active invocation and 8 KiB of workgroup state per column/strip. Each invocation serially walked roughly 408 rows. Poor parallel efficiency was a structural hypothesis, not a measured occupancy result.

## Selected implementation

Run 64-lane workgroups with one lane per interior column. Store each bin's UInt32 count beside its Float32 sum in bin-major GPU scratch. Adjacent lanes can access adjacent columns. Reuse the scratch across all fixed strips, clearing it before each strip. Every initialization, update, and weighted sum retains its original Float32 order.

At the tested Merrill extent, scratch adds 40,222,720 bytes, about 38.4 MiB, and one allocation. It adds no dispatch, submission, copy, or readback. Normal mono totals remain 53 dispatches and 55 submissions. Allocation totals are cumulative, not peak residency.

### Experiments

- Separate count/sum arrays with 32 lanes reduced blue histogram time to about 322 ms and completed rendering to 721 ms.
- Eight independent strip banks used about 307 MiB and slowed the histogram to 436 ms. This layout was rejected.
- With one bank, 64 lanes measured 311 ms and 128 lanes measured 344 ms. The selected workgroup size is 64.
- Interleaving each count/sum pair reduced blue histogram time to 219–232 ms and completed rendering to about 624 ms. One buffer also removed the second scratch allocation and binding.

All layouts passed the retained full-float blue golden comparison. Production code contains only the selected path.

## Matched results

The preserved baseline and selected executables each ran five warm repetitions with identical settings. Full-float golden comparison was outside timing. GPU diagnostics were separate synchronized runs.

| Original | Before ms | After ms | Speedup | Histogram GPU before/after ms |
| --- | ---: | ---: | ---: | ---: |
| Blue parking sign | 2639 | 624 | 4.23× | 2233 / 232 |
| Rocks and tree shadows | 2569 | 789 | 3.26× | 1998 / 189 |
| Sunlit red flower | 3172 | 887 | 3.58× | 2568 / 349 |

Process peak RSS, including untimed validation/export, was 1113–1189 MiB before and 1118–1121 MiB after. This does not measure dedicated Metal residency. The explicit 38.4 MiB scratch increase remains the resource tradeoff.

## Equivalence and regression checks

Full-sensor floats and resized RGB16 TIFFs match the baseline byte-for-byte on all seven originals with equal-layer mixing and again with pure-blue mixing, fourteen photographic cases. Native pure-blue DNG encoded samples also match the baseline byte-for-byte and validate with Adobe DNG SDK 1.7.1 without warnings. Prose cleanup changed only ImageDescription. Encoded samples and all other metadata tags still match the baseline.

A new component test compares an 83×344 signal against an independent rolling histogram. It covers eight strips, both traversal directions, a partial workgroup, distinct preserved borders, zero/HDR samples, three range divisors, repeated runs, and immutable inputs.

The first complete GPU suite failed the existing color cache comparison on yellow flowers after an Exposure edit. The unchanged isolated rerun and subsequent complete suite passed. The [intermittent mismatch](../gpu-output-mismatch.md) remains unresolved. No tolerance, cache policy, or retry behavior was changed.

The owner then requested hiding irrelevant editing controls while retaining both modes' settings. Pointer, expanded-section, and metadata-loading tests cover that follow-up. Final check results and remaining photographic acceptance are recorded in the [implementation plan](../monochrome-rendering.md).
