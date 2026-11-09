---
summary: "Record LOW-style Quattro Draft and Thumb working layers with original source calibration and measured preview quality."
status: archived
---

# Quattro early reduction

Requirements: [small previews](../../docs/specs/small-previews.md). This completed slice follows the [early-reduction plan](../early-sensor-reduction.md), which retains wider acceptance work. The owner requested Quattro-first profiling and a lighter Classic audit. After reviewing the native LOW clue, the owner selected LOW-style processing for Draft and Thumb. Full and Fast retain their current representation.

- [x] Map native corrections, repairs, highlights and the current reduction boundary.
- [x] Measure resident-source fresh processing across DP and interchangeable-lens Quattro models, with representative Classic controls.
- [x] Separate synchronized GPU attribution from ordinary completed-render times.
- [x] Check retained Exposure and noise edits against fresh output.
- [x] Trace native LOW processing and the earlier same-resolution preview experiment before choosing the next implementation.
- [x] Agree an equal-resolution Draft/Thumb prototype that retains HIGH source calibration.
- [x] Reuse the existing same-size reconstruction and scalar blue filter at the requested output grid. Retain native RGB repair/highlight preparation in the first slice.
- [x] Compare Full/Fast bytes, reduced color and monochrome appearance, framing and source-free edits. Retain the extreme one-cell LOW limitation below.
- [x] Measure completed resident-source preparation and retained edits.
- [x] Collect owner visual review of defects, saturated edges and detail. The owner found the differences indistinguishable at actual Thumb size and accepted the measured Full Adaptive variation.

## Measurement

The processor checkpoint is `bb77773`. Frozen release executables use the existing completed-processing and retained-edit harnesses. Each source opens once per process. Three warmed fresh renders reuse the resident decoded source and engine metadata preparation, while rebuilding the numerical image. Source opening, decoding, TIFF export and output readback are outside the reported render interval. Source file-cache state is uncontrolled. GPU diagnostics run as separate synchronized iterations and must not be added to ordinary wall times.

Controls are Sunlight white balance, Standard color mode and enabled camera-default reconstruction. Saved sidecars are not selected. The matrix contains 24 ordinary camera-response cases, two Monochrome cases and two Universal Adaptive cases. Workload monitors report no competing application or compiler in the matrix, retained-edit or Off confirmation runs. These are engine measurements, not native UI input-to-display latency or process-memory acceptance.

Median completed render times in milliseconds, using Wall instrumentation:

| Photograph/model | Full | Fast | Draft | Thumb |
| --- | ---: | ---: | ---: | ---: |
| DP0 Quattro | 913 | 534 | 444 | 423 |
| DP1 Quattro | 801 | 466 | 374 | 355 |
| DP3 Quattro ISO 800 | 872 | 482 | 369 | 341 |
| sd Quattro | 745 | 378 | 284 | 266 |
| Quattro H | 1111 | 607 | 488 | 461 |

Separate profiling-Off Thumb medians are about 427, 359, 343, 268 and 469 ms respectively. Thus the large floor remains without instrumentation. The lighter Classic sample measures SD9 at 173 ms Full and 107 ms Thumb, and SD14 at 149 ms Full and 74 ms Thumb. Classic Thumb uses divisor four. Quattro Thumb uses divisor eight.

DP0 Monochrome Thumb measures 422 ms and Quattro H Monochrome 461 ms. Universal/Natural raises sd Quattro Thumb to 289 ms. Universal/Strong raises Quattro H Thumb to 499 ms. These options retain the requested grid. They do not account for the larger native-prefix floor.

## Baseline boundary and costs

Before the selected LOW-style migration, the [Quattro recipe](../../crates/foveonic/src/render/recipes/quattro.rs) already prepared Stage1 noise filtering, color processing, blue-guided reconstruction and finishing for explicit working geometry. HIGH-origin previews retained a color grid with half the blue width and height at every requested scale. Pooling followed native extraction, phase calibration, sparse and catalogue repairs, image-wide despeckling, optional blue-pattern correction, highlight restoration and selected shading.

At Thumb, work before the first preview pool accounts for about 93–96% of separately measured GPU execution. Pooling and all downstream passes together occupy about 11–19 ms. Dominant native operations vary by source:

| Operation | Diagnostic GPU cost | Implication |
| --- | ---: | --- |
| Blue defect-cluster repair | 81–153 ms on the DP samples, 20–25 ms on sd samples | Catalogue-driven native coordinates. Each cluster currently dispatches one workgroup, with clusters submitted serially. Independent-cluster batching is a candidate that preserves the native algorithm. |
| DP sparse/phase repair | 44–46 ms | Twelve passes mix sparse sensor correction with phase-specific image processing. Separate their responsibilities before relocating the operation. |
| Blue despeckling, both passes | 22–43 ms | General filtering needs scaled support and noise. The dark-pair detector also uses native row parity. |
| RGB despeckling | 11–16 ms | Image-wide filtering is a candidate for reduced buffers, with its defect-mask contract preserved. |
| Highlight restoration | About 20 ms ordinary, 57 ms on Quattro H | Requires projected bounds and retained clipping evidence before averaging conceals saturation. |
| Blue-pattern correction | 169 ms on the selected Quattro H photograph | Native alternating-position evidence requires explicit treatment. It is also available on ordinary sd Quattro when capture aperture selects it. |

The blue-pattern operation already contains reduced intermediate masks. That alone does not make its complete work small. Its 22 dispatches include about 53 ms for horizontal run refinement, 29 and 19 ms for the two phase fields, and 15 ms for vertical refinement on the H sample. These phase names come from the fixed authored pass sequence at the measured checkpoint. They are not additional production log entries.

## Baseline retained editing

Five independent source-free Thumb sessions retain their prepared inputs. Three Exposure rounds complete the entire five-photo group in about 57–71 ms. Each photo executes only its Code Response operation. Two first-use noise settings complete the group in about 197 and 215 ms, while revisiting the retained setting takes about 77 ms. The noise edits rerun reduced Stage1 and its dependent finishing. They do not rerun phase extraction, native defect repair, blue patterns or highlight restoration.

All ten final per-photo verification comparisons match fresh output exactly. This confirms current cut reuse for these settings. It does not make first preparation fast or establish broader memory-budget behavior.

## Native LOW as a structural clue

The owner asked whether native LOW's already-reduced blue plane provides a simpler route. It does. The implemented LOW input is equal-resolution interleaved RGB, while HIGH stores R, G and four blue phases per color cell. LOW skips the separate high-blue phase gain/refresh and luma repair sequence, retains RGB repairs and highlights, derives its later blue guide from restored RGB, and uses same-size reconstruction. Both numerical reconstruction geometries already exist.

The [earlier LOW investigation and same-resolution experiment](./quattro-performance-and-fast-preview.md#native-low-investigation) established this distinction and tested a LOW-inspired Fast output. At the baseline, the general-scale path preserved HIGH preprocessing and two-resolution geometry instead. That historical experiment retained native RGB preprocessing and was not the early working-grid boundary implemented in this slice.

LOW source identity also selects different calibration tables, noise/detail controls and coordinate interpretation. Replacing HIGH metadata with LOW metadata would conflate source acquisition with processing geometry. HIGH-origin previews should preserve their original calibration and derive the working representation explicitly. Native LOW omission of luma repairs is not proof that averaged HIGH samples need no defect handling.

The five profiled Quattro originals are HIGH. No genuine LOW photographic capture is available in the shared example set. Existing synthetic LOW tests verify numerical and framing contracts, rather than independent photographic equivalence.

## Selected first slice

The owner selected an early transition into equal-resolution working layers for Draft and Thumb before porting every native HIGH operation individually. Reuse the existing same-size numerical reconstruction where applicable, retain HIGH source calibration, and project only the required defect, clipping and phase evidence. Keep Full unchanged. Compare against Full and the current reduced recipe on known defect sites, saturated colors, edges and high-ISO texture.

This proposal must establish which native corrections are still necessary before pooling and which become reduced RGB operations. It must preserve output dimensions, camera framing and selected editing controls. Use the existing geometry, graph and preparation owners rather than fabricating LOW camera metadata or adding another independently maintained preview recipe.

Independent native defect-cluster batching remains a useful low-risk candidate, using the already established Merrill approach, if the selected boundary still needs those repairs. Quattro H's measured phase-pattern cost also requires direct visual evidence before deciding how much native phase information the transition must preserve. Classic remains lower priority unless mixed-camera application tests identify a material bottleneck.

## First equal-resolution implementation

HIGH-origin Draft and smaller grids now form RGB at the requested output dimensions. The R/G component pool therefore uses half the output divisor. The original camera metadata and HIGH calibration stay unchanged. Existing native RGB repairs and highlight restoration remain before pooling. The same-size blue guide is extracted from the restored, selected-WB-shaded RGB pool. The existing scalar bilateral filter and same-size reconstruction consume it. Main color-filter variance and physical support use the actual R/G sampling divisor. HIGH source response and finishing controls remain selected from the original capture.

Full and Fast retain their two-resolution representation. The ordinary numerical graph prunes the unneeded separate HIGH blue repair chain in Draft and Thumb. Native inspection outputs remain available. This first slice still authors their metadata preparation and retains the coupled native Sparse Repair work. Removing the remaining native RGB costs is separate follow-up work rather than a claim that every Quattro operation already runs after pooling. Derived photographic previews receive a new identity.

Twenty-one matched photographic comparisons use frozen executables with alternating order. The same three-warm-render resident-source method gives these Wall medians in milliseconds:

| Photograph/model | Draft before → after | Thumb before → after |
| --- | ---: | ---: |
| DP0 Quattro | 449 → 238 | 441 → 201 |
| DP1 Quattro | 382 → 246 | 361 → 203 |
| DP3 Quattro ISO 800 | 373 → 256 | 346 → 194 |
| sd Quattro | 285 → 162 | 271 → 116 |
| Quattro H | 533 → 235 | 472 → 180 |

The three-camera Full/Fast camera-response float comparisons preserve every bit. Full Adaptive Tone is a qualification because its smaller analysis render uses the new working representation too. DP0 Natural mean RGB shifts by about -0.043%, Quattro H Natural is exact and Quattro H Strong shifts by about -0.558%. The largest Strong component change is 0.1124 on a nominal 0–1 final scale. These mean shifts satisfy the owner's previously accepted one-percent Adaptive analysis tolerance. Native camera-response and source-calibration preservation remain separate exact checks.

Reduced camera-response mean RGB shifts stay within about 0.28% across the five sources. Matched overview images remain broadly similar. Ranked display differences include high-contrast lamp/window boundaries and roof detail, so tiny average changes do not establish local equivalence. The owner reviewed the overview and enlarged crops centered on the largest display differences, found them indistinguishable at actual Thumb size and accepted this slice's appearance, including the measured Full Adaptive variation. Temporary float and TIFF payloads are removed after hashes, numerical summaries and review images are retained. Workload monitoring finds no competing app or compiler during the matched run.

One hundred source-free edits match fresh results across DP0 and Quattro H, Color, Monochrome, Natural/Strong Adaptive and spatial finishing at divisors four and eight. Normal Draft/Thumb retained checks also cover simultaneous Chroma, Luminance, Smooth/Crisp and Exposure changes on DP0, sd Quattro and H. Cold execution does not run the separate HIGH blue repair chain. Retained images remain immutable and no cached individual image allocation exceeds the working-image bound. Several working histories can now exceed the decoded source's byte count in aggregate, so the previous aggregate-size assertion was replaced with that actual allocation check. Shared memory policy remains unchanged.

Five-photo retained Exposure groups complete in about 58–72 ms, with only Code Response executed. First-use noise groups take about 224–237 ms and a repeated retained setting takes about 77 ms. All ten final per-photo comparisons match fresh output. These timings cover engine sessions, not native application interaction or process-memory limits. The remaining DP and H first-preparation times still exceed the 120 ms target.

## Extreme-grid qualification

The synthetic LOW test's requirement for positive photographic output can fail when divisor 2^31 reduces the whole sensor to one logical cell. An isolated pre-change `bb77773` worktree reproduces that failure too. It fails in Color or Monochrome depending on the run. Ordinary LOW sizes, finite-value checks and framing checks pass. The new HIGH common-grid route can expose the same one-cell behavior. A retained/fresh assertion also failed once in the broader scale sweep and passed on a diagnostic rerun. The focused ordinary Draft/Thumb checks and source-free matrix pass. Preserve these extreme-grid failures as unresolved evidence rather than weakening the assertions or treating the complete scaled-pipeline specification as conforming.

## Next reduction boundary

The owner requested the next migration after accepting the equal-resolution preview appearance. A fresh diagnostic on the reviewed prototype measures RGB despeckling at about 12 ms on DP0 and 17 ms on Quattro H. Highlight processing occupies about 20 and 55 ms respectively. Native RGB cluster repair remains about 13 and 4 ms. DP0's coupled sparse repair remains about 45 ms. These are separately synchronized GPU measurements, not additive components of ordinary completed-render wall time. The workload monitor detects no competing application or compiler.

RGB despeckling currently precedes catalogue cluster repair, optical correction and highlight restoration. Simply moving it past the existing pool would also move it after highlight restoration and selected-WB shading. That changes both operation order and signal units. The proposed next coherent slice keeps catalogue repairs native, then runs reduced RGB despeckling before reduced highlight restoration. This reorders catalogue cluster repair ahead of preview despeckling, requiring photographic validation. Preserve clipping evidence across pooling and keep native Full/Fast unchanged. The owner approved this combined slice, using the working-grid and clipping-evidence principles established for Merrill.

- [x] Run reduced RGB despeckling and spatial highlight restoration after native catalogue repair.
- [x] Verify original clipping evidence, scaled detection, native Full/Fast identity and source-free edits.
- [x] Measure first preparation and inspect defects, edges, shadows and saturated colors.
- [x] Collect owner visual acceptance of the reduced despeckle/recovery slice. The owner reviewed the rebuilt app with the shared test folder and approved continuing.

## Recovery evidence findings

The first reduced recovery prototype carried channel peaks. On the DP3 high-ISO photograph it introduced a magenta rim at a clipped lamp. Bypassing reduced despeckling left that rim unchanged. Bypassing directional recovery removed it. Reclassifying neighbors after pooling expanded a thin native detector boundary into a full working-cell correction. Using original peak levels or an averaged neutralization weight alone did not resolve it.

The selected node-owned evidence pass pools original channel peaks, native directional-detector coverage, maximum green protection and the RGB correction from the original local clamp/neutralization equations. It emits two RGBA records per working cell. This is a reduced control map, not another native image. It is kept separate from numerical RGB means. Spatial interpolation blends by original detector coverage rather than repairing every part of a mixed cell. Its local correction preserves the covariance lost by applying a nonlinear neutralization weight to averaged RGB. The enlarged lamp comparison removes the magenta rim with this separation.

Catalogue clusters and DP1 optical correction remain native before pooling. Selected-WB shading samples the original gain field at pooled cell centers after recovery. Ordinary RGB and blue noise/reconstruction controls still use their actual sampling pitch. Full/Fast and native inspection graphs retain their original operation sequence. The unused native inspection branches are pruned from reduced final execution.

The existing highlight module owns evidence and recovery parameters and shaders. One typed evidence output keeps its non-photographic layout explicit. The graph and executor retain their existing scheduling, ownership and dependency-cut responsibilities. No application scheduling changes are part of this slice.

Recovery also receives the already-retained pre-despeckle working means. If reduced despeckling changes a cell, that cell no longer inherits its original clipping, directional coverage or local correction. This prevents original defect evidence from reintroducing a repaired hot pixel. It adds a graph dependency without another native buffer.

## Reduced recovery validation

A fresh matched run uses the same frozen-executable, resident-source method as the first slice, with three warm fresh renders and alternating before/after order. The workload monitors detect no competing application or compiler during photographic, retained-edit or synchronized diagnostic measurements. Median completed wall times in milliseconds compare against the owner-accepted equal-resolution prototype:

| Photograph/model | Draft before → after | Thumb before → after |
| --- | ---: | ---: |
| DP0 Quattro | 243 → 233 | 199 → 176 |
| DP1 Quattro | 248 → 237 | 207 → 190 |
| DP3 Quattro ISO 800 | 264 → 251 | 198 → 173 |
| sd Quattro | 165 → 151 | 120 → 95 |
| Quattro H | 240 → 196 | 182 → 120 |

Six matched Full/Fast camera-response comparisons on DP0, sd Quattro and Quattro H preserve every output bit. Full Adaptive analysis remains separately qualified. DP0 Natural mean RGB changes by about +0.043%, Quattro H Natural stays exact and Quattro H Strong changes by about -0.00045% relative to the accepted first slice. Reduced camera-response mean RGB differences remain within about 0.244% across the five sources. The DP0 Monochrome mean changes by about -0.599% and Quattro H Monochrome by about -0.179%. These averages do not replace local visual review. The reviewed contact sheet, largest-display-difference crops and clipped-lamp crop cover shadows, saturated boundaries, high-ISO detail and white blossoms. The owner subsequently reviewed the rebuilt app with the shared test folder and accepted this slice.

The compact native evidence pass takes about 7.2 ms on DP0 and 11.8 ms on Quattro H in separate GPU diagnostics. Reduced RGB despeckling takes about 0.7 and 0.9 ms. Spatial recovery takes about 4.3 and 7.8 ms on the working grid. Keep these synchronized pass measurements separate from ordinary wall times. Native DP sparse repair still costs roughly 45–50 ms and native catalogue cluster searches remain additional preparation work. DP Thumb preparation therefore remains above the 120 ms target. Source decoding and disk-cache reads remain outside these render measurements.

One hundred source-free edits match fresh results across DP0 and Quattro H at divisors four and eight, covering Color, Monochrome, Natural/Strong Adaptive and spatial finishing. Every recorded WB preset matches fresh rendering at both working sizes on these cameras. Three-camera retained checks verify reduced operation extents, unchanged earlier images and the absence of native image retention. Synthetic LOW Color/Monochrome and HIGH history/control checks pass at divisors four, eight, sixteen and sixty-four. These focused runs explicitly select those divisors without changing the broader test defaults or resolving the separately recorded one-cell limitation.

Five-photo retained Exposure groups complete in about 61–76 ms. First-use noise groups take about 210–226 ms and a repeated retained setting about 74 ms. All ten per-photo final verifications match fresh output. These edits execute no native preparation, highlight evidence or recovery again.

GPU node tests verify pooled detector coverage against the original native classifier, partial-cell and halo evidence, local correction arithmetic, clipping that disappears in the numerical mean, bright-only reduced speckle detection and repaired defects that must not regain original damage. The evidence allocation is two RGBA records per working cell. The already-retained pre-filter means provide repair provenance. These logical allocation and execution checks do not establish peak process residency or native UI input-to-display latency.

## Requested sparse outputs

The owner approved continuing with the remaining sparse-repair work. Inspection of its twelve passes identifies a smaller exact-output opportunity before relocating any more processing. RGB is complete after directional clamp, pass six. Density, pattern correction and final blue smoothing do not write RGB. Density does update the exported defect mask, so a defect-only request must retain that additional pass. A requested complete HIGH-blue image retains all twelve passes. Native LOW uses its existing three-component algorithm and seven-pass sequence.

Use the existing executor's required-output set to request complete blue and defect outputs from the numerical operation. Preserve the HIGH six-component feature model for native RGB repairs even when final blue is unrequested. Avoid final blue and pattern-only scratch allocations for an RGB-only result. This is output demand, not a preview-quality or camera-option shortcut. Full/Fast and named blue/defect inspection results must retain their original equations and bytes.

- [x] Make sparse execution honor requested blue and defect outputs.
- [x] Compare RGB and defect outputs against complete execution on synthetic sparse grids, including active interpolation.
- [x] Verify photographic bytes, cached edits and native inspection histories.
- [x] Measure completed preparation and the remaining native prefix, with the app closed for timing.

## Sparse demand validation

The executor now passes its existing missing-output requirements directly to sparse repair. RGB-only demand executes seven passes. A HIGH defect-mask request executes eight, including density's mask writes. A complete HIGH-blue request retains twelve. Native LOW keeps its original seven-pass numerical route. Unrequested side outputs remain absent, so a partial execution cannot publish an incomplete blue or mask result over a compatible cached value. The original node keys and shaders remain unchanged because requested output bytes are identical.

The synthetic test exercises native HIGH six-component and LOW three-component features with both preservation settings. Dense selected sites activate ghost interpolation outside those sites and density's exported mask updates. Every requested RGB, blue and mask result matches complete execution exactly, and both input buffers remain unchanged. Dispatch and storage-request observations verify that RGB-only demand avoids the unused tail and allocations. Preview smoke checks also require seven actual sparse dispatches in ordinary DP reduced preparation.

Twenty-one matched photographic comparisons against the accepted reduced-recovery build preserve every output bit. They cover five cameras at Draft/Thumb, DP0/sd/Quattro H Full/Fast, Monochrome and Full Adaptive Natural/Strong. Each timed reduced case has three warmed fresh renders from a resident decoded source, with alternating executable order. Separate workload monitors detect no competing application or compiler during photographic, retained-edit and synchronized GPU measurements.

| Ordinary DP photograph | Draft before → after | Thumb before → after |
| --- | ---: | ---: |
| DP0 Quattro | 231 → 190 ms | 173 → 136 ms |
| DP1 Quattro | 237 → 195 ms | 181 → 143 ms |
| DP3 Quattro ISO 800 | 254 → 210 ms | 171 → 134 ms |

The sd and Quattro H recipes do not use this ordinary DP operation. Their small timing variations are controls, not gains attributed to sparse demand. Native Full/Fast still require complete HIGH-blue processing. DP0 Monochrome Thumb changes from about 167 to 130 ms with identical pixels.

DP0 synchronized diagnostics attribute about 46.9 ms to twelve sparse passes and 11.4 ms to seven. Sparse-node storage requests fall from about 376.3 to 268.1 MiB. This removes the native final-blue allocation and pattern-only scratch region. These are logical allocation requests, not process residency or peak GPU memory. Remaining DP0 GPU costs include about 13.3 ms of catalogue cluster search and 7.4 ms of original recovery evidence. Upload and recipe preparation remain separate host costs. Do not sum synchronized GPU diagnostics with host observations as ordinary render latency.

Five-photo retained Exposure groups complete in about 59–74 ms. First-use noise groups take about 233–238 ms, with a retained revisit near 75 ms. All ten per-photo verifications match fresh output, and none executes sparse repair or the other native preparation again. Three-camera retained checks, native-history/control checks and synthetic DP0 LOW Color/Monochrome checks pass at Draft/Thumb. Source-free reset and Monochrome checks add twenty exact edited comparisons after releasing the originals.

This slice avoids unrequested work while preserving native sensor-site repair. It does not relocate those site-specific equations onto averaged pixels. Further optimization of the measured native costs is separate from this completed migration.

## Final verification and cleanup

The owner accepted the reduced appearance and requested a final cross-camera/options check before the reviewed checkpoint. Keep the current native calibration, sensor-site repairs and compact recovery evidence. Further native-stage optimization and the Dehaze enabled/disabled cache transition belong to the later consistency pass.

- [x] Review numerical ownership, graph dependencies, output demand and retained buffers. No additional numerical path or scheduler is needed for this checkpoint.
- [x] Check source-free versus fresh results across applicable options on Merrill, ordinary DP Quattro, interchangeable-lens Quattro, Quattro H and representative Classic cameras.
- [x] Reconcile completed migration reports with remaining acceptance work, preserving the known extreme one-cell LOW limitation.
- [x] Complete repository checks and prepare the reviewed commit.

The final serial sweep passes 194 applicable option/grid cases on hazy-coast Merrill, DP0, DP1, sd Quattro, Quattro H, SD9, SD14 and DP2. Each case compares five source-free edits against fresh renders, giving 970 exact pixel comparisons with matching geometry and analysis. The sweep also checks cancellation and changed-engine/native-dependency outcomes. A separate Classic check covers seven originals at divisors two, four, eight and sixteen. DP0 and sd native-history/control checks cover divisors one through sixty-four. Quattro H restart checks cover divisors four through sixty-four. All twelve fixture runs pass, with no competing app or compiler observed. These are correctness checks, not new application-latency measurements.

Final workspace tests, strict all-target Clippy, formatting, generated documentation indexes and artwork checks pass. The final cleanup changes comments and documentation only. Earlier matched Full/Fast, reduced photographic, retained-edit and allocation results remain applicable. The working-grid implementation stays within the existing recipe, node, graph and executor boundaries. No scheduling or cache-policy changes are part of this checkpoint.
