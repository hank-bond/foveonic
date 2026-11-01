---
summary: "Reduce completed Merrill noise and banding latency while preserving the reviewed corrections."
status: archived
---

# Merrill noise latency

The owner approved optimization after the noise cleanup checkpoint, with latency prioritized over memory usage. The pass preserved reviewed algorithms, controls, correction strengths, immutable sensor evidence and GPU ownership. The owner clarified that small latency gains do not justify changed photographic output, so this slice requires exact output. Training workflow work remained separate. The owner closed the pass after reviewing its measured gains and output checks.

This pass used [profiling](../../docs/specs/profiling.md), [interactive rendering](../../docs/specs/interactive-rendering.md), [render resources](../../docs/specs/render-resources.md), [Merrill multiscale noise](../../docs/specs/merrill-multiscale-noise.md) and [Merrill banding](../../docs/specs/merrill-banding.md). Fresh completed processing and cached edits are distinct measurements. Timestamp runs diagnose stages and are separate from wall-time acceptance runs. Preserve the existing deferred mismatch investigation boundaries in [the editing handoff](../editing-optimization-handoff.md).

## Work

- [x] Capture repeated completed baseline timings for current noise, banding and shadow controls.
- [x] Identify the dominant shader and host costs before choosing a change.
- [x] Implement and measure the smallest useful optimization without reducing quality.
- [x] Check photographic output, cached edits, regional rendering and export consistency.
- [x] Record measured gains and remaining bottlenecks, then package a review build.
- [x] Investigate retaining an unchanged directional axis when editing the other axis.
- [x] Diagnose variable fresh Coarse latency outside shader timestamps. Matched native pointer feedback remains a separate, unperformed measurement.
- [x] Optimize the native three-scale denoiser's own kernels with independent output checks and completed timing comparisons.

## Resource-overlap investigation

The owner explicitly continued optimization before release and approved a bounded export-submission trial. The earlier local release candidate remains a review checkpoint, not a selected final build.

Separate completed headless runs compare stateless queueing with fresh editor-style sessions. The Coarse case executes the same 163 dispatches in both routes. Five warmed stateless samples vary from 1.66 to 2.48 seconds, while staged processing stays near 1.74 seconds. Settled-memory snapshots run separately, with explicit five-second holds outside timed renders. The stateless process peaks at 9.7 GiB physical footprint and returns to approximately 0.54 GiB after releasing its image. The staged process peaks at 3.8 GiB and returns to approximately 0.57 GiB. Alternating two photographs peaks at 4.0 GiB and settles near 0.76 GiB with both decoded sources loaded. Dropping the engine leaves approximately 70–90 MiB. These short repeated runs show transient overlap rather than continuing retained growth. They do not prove absence of every leak.

A Metal System Trace identifies about 9.04 GiB peak allocated Metal memory and 37.0 GiB cumulative buffer allocation across four renders. Multiscale workspaces are the largest cumulative category. Driver memory wiring consumes substantial intervals in the capture. GPU intervals, driver intervals and wall time overlap and must not be summed. Other applications draw during the trace, but their recorded compute activity is negligible. Ordinary timing rounds remain separate from tracing and OS memory sampling.

The first trial waits at graph boundaries after one GiB of allocation traffic. It preserves the editor's existing completion boundaries. Four completion points stabilize the stateless Coarse case near 1.62–1.67 seconds and lower the sampled peak to 4.2 GiB. The threshold describes allocation traffic since a completion, not a hard residency cap. A single operation, retained outputs and driver resources can exceed it.

A second trial reuses expired moment/coefficient storage and alternates two full-size layer buffers inside the multiscale operation. The guarded read/write API and ordered queue remain authoritative. It reduces storage allocation traffic from 8.73 to 7.70 GiB per render, without changing the 163 dispatches or filter arithmetic. This reuse stays local to the operation and adds no persistent pool.

The selected trial uses a two-GiB allocation window. Two repeated Coarse runs have warmed medians of 1.565 and 1.568 seconds. A separate settled-memory capture peaks at 4.8 GiB and returns near 0.50 GiB after each image release. This is roughly half the original peak. A 512-MiB window reduces peak footprint further to 3.1 GiB, but costs about 0.13 seconds in the heavy case and slows lighter processing. Latency takes priority, so that smaller window was rejected. One-GiB trials fall between these policies. These are measured thresholds on the Apple M1 Pro, not a universal optimum or a hard residency guarantee.

The two-GiB trial's Fine and Monochrome warmed medians are 858 and 912 ms, compared with nearby original runs around 871 and 903 ms. Reference processing is approximately 782 versus 750–761 ms. This small Reference slowdown is retained as an explicit tradeoff rather than described as an across-the-board speedup. Runs use profiling Off and remain separate from memory sampling and builds. Existing interactive stage waits and the one-GiB checkpoint limit remain unchanged.

Seven photographic cases cover the prior six recipes plus Reference processing. Fourteen full native/final Float32 buffers match the pre-resource-change checkpoint byte for byte. Full temporary buffers were removed after recording hashes. The validator's `final` alias resolves to a concrete artifact name, so its name-based TIFF branch did not execute. The subsequent native-kernel comparison exports by requested output position and verifies eight actual TIFFs. The resource checkpoint's 677 ordinary workspace tests, twelve focused GPU checks for independent multiscale equations, guarded ownership, cached edits, Monochrome and profiling equivalence, and strict all-target release Clippy pass.

One earlier memory capture ended on a partial JSON log line. The maintained recorder now consumes only newline-complete records, keeps binary/case identities and raw snapshots, and rejects failed snapshots or snapshots that exceed the hold interval. The incomplete capture is excluded. The completed capture exercises the maintained tool and common report envelope.

## Native filter kernel optimization

The owner approved a deeper pass on the three-scale denoiser itself. The horizontal moment and coefficient-average passes now share reflected row neighborhoods within 32-by-4 workgroups. Samples use separate scalar planes in workgroup storage. Each group loads its halo once, then preserves the original per-pixel tap order and equations. All lanes complete their shared loads before partial groups return. Host and shader workgroup dimensions are checked together. The independent numerical fixture now also exercises a partially occupied native-scale horizontal group.

Two completed Off runs bracket a baseline on the Apple M1 Pro. Eleven warmed samples per run put the prefix ending at `multiscale.layers` at 147.7 and 147.2 ms, versus 152.8 ms. This includes original upload, black/shield preparation and the denoiser. It is not isolated shader execution. A whole Fine-band photograph measures 866.2 ms before and 857.7 ms after, with five warmed samples per route. Separate GPU diagnostics put the denoiser passes around 65–69 ms versus 75 ms. These modest whole-image gains are distinct from the earlier large band-edit cache gains. Cross-GPU performance and native input-to-screen latency remain unmeasured.

Eight photographic cases include both multiscale choices, Reference, DP1, DP2, DP3, ISO800, active Coarse bands and Monochrome. All sixteen complete native/final Float32 buffers and all eight actual TIFF files are byte-identical to the resource checkpoint. Exact temporary renders were removed after retaining hashes and case descriptions.

Rejected trials include an interior reflection shortcut with no useful speed gain, narrower cooperative groups, and cooperative vertical coefficient measurement that increased latency. Combining the finest-scale correction and update saved a dispatch, but changed isolated photographic pixels substantially despite very small aggregate error. That fusion and its helper refactor were removed. Horizontal sharing alone preserves every compared byte. A generic workgroup-pointer accessor was also rejected because the current shader validator does not support those function arguments. The retained accessors are explicit and local.

## Earlier row and shadow changes

Row correction prepares independent pixel residuals and variances in parallel before reducing spans in their existing compensated order. It also prepares the high-pass reliability field once per row, replacing repeated convolution inside the three serial mean invocations. The mean and confidence passes consume that same field. The pixel residual workspace is reused for the later signal guide after its last read is submitted. The additional variance workspace remains a deliberate temporary-memory tradeoff for latency.

Interior row and directional-band samples skip modulo reflection. Boundary samples retain the original folding equations and all selected support sizes. The directional filter retains every bilateral tap.

Shadow neutralization has a separate typed regional-guide dependency. Its key includes support and the signal basis, independent of strength and selectivity. The ordinary bounded session frontier can retain the guide and the unmodified input while editing shadow controls. The photographic output name and correction pass remain unchanged. That slice introduced no additional persistent image pool, CPU image processing or completion fence.

## Checks and rejected trials

Six photographic cases cover four originals across DP1, DP2 and DP3, including ISO800 leaf shadows, a portrait, the dark bracket, active Coarse banding and Monochrome. Twelve full buffers, native row output and final image output for every case, match the committed cleanup checkpoint exactly. This covers about 579 million Float32 components. Full temporary captures were removed after comparison and receipt generation.

A cooperative workgroup-neighborhood band filter passed its small numerical checks but increased full-image latency substantially on the Apple M1 Pro. It was removed. Sharing fine and signal taps inside the serial span loop did not provide a useful improvement and was removed. An initial overlapping diagnostic run was excluded. Final timings must use completed runs without concurrent builds, tests or other measured renders.

## First optimization slice

The workspace's 676 ordinary tests, strict all-target release Clippy, formatting and documentation checks pass. Fourteen focused GPU checks cover independent row equations, directional edge protection and transpose, native tile phase, strength scaling, shadow appearance, cached guide reuse, regional/whole rendering, Monochrome and photographic/DNG preparation. The optimized review app was rebuilt, signed and opened successfully. This slice remained uncommitted during owner review.

Completed headless measurements used the 14-core Apple M1 Pro, Full processing, the same dark DP3 capture, identical saved settings and five warm samples per route in the final comparison. Engine/device setup and source decoding were outside the interval. Every timed render and cached edit was explicitly completed. Separate synchronized GPU samples were diagnostic only. Earlier three- and five-sample rounds support the same useful editing gains.

| Completed route | Cleanup checkpoint median | Optimized median |
| --- | --- | --- |
| Fresh render, Fine bands | 997 ms | 854 ms |
| Banding edit, Coarse enabled | 1,454 ms | 1,103 ms |
| Neutralize shadows edit, Fine bands | 208 ms | 154 ms |

Chroma, exposure and Fine-only banding edits are essentially unchanged. Fresh Coarse renders remain variable and have no claimed repeatable improvement. These figures measure completed engine work, not native input-to-screen latency. The guide reuse retains one additional four-component image during shadow edits. Pixel evidence adds one three-component temporary workspace after reusing the existing signal-guide storage. Memory residency was not independently measured.

Separate shader diagnostics put native pixel evidence plus span reduction near 114 ms versus 191 ms for the original span measurement, and the row signal guide near 25 ms versus 186 ms. Coarse directional neighborhoods fell from about 1,116 ms to 773 ms. These observations identify useful work reductions but do not replace completed wall-time comparisons.

At the first checkpoint, the largest remaining Coarse cost was bilateral neighborhood measurement. The combined directional operation recomputed both axes even when only one axis control changed. Splitting its dependency boundary became the next slice, with the existing projected outputs and regional phase preserved. Fresh rendering also has substantial costs outside shader timestamps. Native feedback, transfer/allocation behavior and completion boundaries need separate evidence before changing their architecture.

## Directional measurement reuse

The next slice separates source tiling, per-axis measurement and per-axis correction into typed graph stages. Measurement keys track support and required estimate slots. Correction keys track strength. Column output feeds row measurement in the same order as before. Nonzero strength changes can reuse estimates, while row support changes can reuse the completed column work. No new cache pool or cache-limit increase is introduced.

Twelve full buffers, including projected band-corrected layers and final output across the same six cases, match the first optimization slice exactly. The temporary comparison buffers were removed after recording equality. Six targeted GPU tests pass, including assertions that repeated strength changes skip both bilateral filters and row support changes skip column processing. Existing spatial phase, boundary, inactive and strength-scaling checks remain in place.

The owner questioned the serial axis order. It is an implementation choice, not a physical requirement. Independent measurement of both axes from one unchanged input, followed by combined correction, is deferred for a separate visual experiment. This slice preserves order solely to preserve the reviewed photographic result.

Completed headless comparisons used the same dark DP3 case on the Apple M1 Pro, with five samples per route. One first-slice run was bracketed by two measurement-reuse runs, without concurrent builds or tests. The figures below are route medians. They describe repeated edits after the ordinary active frontier warms, not input-to-screen latency.

| Route with Coarse enabled | First slice | Measurement reuse, two runs |
| --- | --- | --- |
| Native row strength | 1,157 ms | 277 / 295 ms |
| Native column strength | 1,153 ms | 735 / 733 ms |
| Native row support size | 1,111 ms | 750 / 719 ms |

Fine-only row-strength edits changed from 391 ms to 288 / 301 ms. Chroma, exposure and shadow edits stayed near their previous costs. The first band edit still warms the new frontier and can cost about as much as before. Fresh-render timing remains variable, with no claimed improvement from this slice. Retention stays within the existing one-GiB active-photo cache limit. The broader GPU sweep found a structural test that assumed one banding node. Its assertion now identifies the final correction stage explicitly.

Final verification passes 677 ordinary workspace tests, all fourteen focused GPU checks and strict all-target release Clippy. All seven recipe GPU checks were rerun successfully after updating the structural assertion. Formatting and documentation indexes are checked separately. The review app was rebuilt, signed and opened; native strength edits rendered successfully and the original strength was restored. This interaction is a smoke check, not a matched input-to-screen measurement. These checks preceded the owner’s commit approval.

## Closing verification

The final pass retains horizontal neighborhood sharing and the earlier guarded buffer reuse and bounded submission policy. The 677 ordinary workspace tests, twelve focused GPU checks, strict all-target release Clippy, formatting, documentation indexes, thirteen profiling-tool checks and five artwork checks pass. The final memory capture again peaks at 4.8 GiB and returns near 0.50 GiB after each frame release. Dropping the engine leaves approximately 68 MiB. No continued retained growth was observed in the three-frame capture.

Temporary validation source and full diagnostic renders were removed. Training code and data remain separate. The owner approved committing the verified changes after closing the pass. The earlier release bundle predates this pass. Native input-to-screen latency, cross-GPU performance, independent axis measurement and the user training workflow remain outside this completed slice.

Before commit, the owner required fitted priors to leave algorithm source. The shipped ISO/covariance table and shield reference axis now live in the model metadata artifact. A shared loader validates it once and retains the immutable parsed data. Selection still uses the nearest measured ISO, with the same lower-ISO tie and endpoint behavior. The shield coefficient binary is unchanged. This storage change does not introduce user profile loading or serial matching.

The storage migration preserves all 212 parameter bytes, checked against the previous Rust literals. Eight photographic recipes again match in sixteen complete float buffers and eight TIFFs. The updated 679 ordinary tests, nine focused row/shield/recipe GPU checks and strict all-target release Clippy pass. Temporary migration examples and full render captures were removed before commit.
