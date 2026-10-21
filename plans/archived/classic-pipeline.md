---
summary: "Build and validate an isolated end-to-end Classic GPU recipe to evaluate functional operations and shared engine machinery."
status: archived
---

# Classic pipeline experiment

Implements [Classic rendering](../../docs/specs/classic-rendering.md). The owner approved an autonomous end-to-end experiment, followed by review before committing. Main was clean and matched origin at `7d23d0b` when the experiment worktree was created.

## Work

- [x] Verify main and create an isolated worktree.
- [x] Decode a real Classic format-30 original without changing its integer samples.
- [x] Prepare camera-derived fixed-point calibration and color contracts.
- [x] Implement the selected low-ISO correction, chroma filtering, color and finishing stages in WGSL.
- [x] Compose a Classic recipe through the shared graph, executor, inspection and TIFF output.
- [x] Validate actual original rendering and intermediate arithmetic, plus Merrill regressions.
- [x] Record coverage, approximation bounds and architecture review findings.

## Decisions and findings

The first original is SD15 at low ISO. Its selected processing path bypasses the high-ISO range denoiser. Camera selection belongs in the recipe. Calibration, signed nonlinear color and finishing remain distinct functional operations. Runtime rendering must not depend on the reference implementation or machine-local research artifacts.

The existing format-30 entropy decoder is reusable. Classic image sections use a different section type while retaining equal-resolution geometry. This reuse does not extend automatically to calibration or photographic finishing.

Public LittleCMS 1.19 reproduces the measured legacy profile transform, while current LittleCMS does not. The selected numerical asset preserves its fixed-point matrix and complete separable shapers. Independent evaluation matches the profile-code transform exactly without native runtime dependencies.

## Architecture review findings

The first slice works with family-authored recipes and functional operation contracts. Camera selection stays in preparation. The graph, executor, cache, guarded storage, inspection, presentation and TIFF packaging remain shared. Separate signed linear codes, root-domain RGB and normalized linear RGB prevent accidental connections between numerically similar buffers with different meanings.

Parameters and small functional choices were sufficient for the implemented variants. No injected shader callbacks or general dependency-injection framework were needed. The highlight recipe selects calibrated thresholds. The stream operation executes those controls without a camera enum. This is a useful starting point for comparing future variants before introducing another abstraction.

The initial finishing operation combined signed LUT arithmetic and a prepared ROMM-to-sRGB matrix/shaper conversion. The subsequent [shared finishing experiment](classic-finishing.md) moves destination conversion into the common output path. A uniform RGB cube was rejected because gamut boundaries produced large interpolation errors. Preserving the separable fixed-point transform gave an exact numerical profile-code contract without native runtime dependencies.

The highlight sweep initially preserved delayed writes with an ordered inner loop, which dominated rendering time. Its horizontal dependency can be expressed directly: the oldest row’s left neighbor comes from a previously completed scratch row, while its center and right neighbor retain their original values. Reading those known states explicitly permits parallel evaluation followed by a barrier and delayed writes. This preserves the native stream without serializing the horizontal sweep. Its final four rows still retain different processing phases.

The recipe preserves full sensor geometry through processing and attaches the active crop as a shared output view. Final resizing therefore uses common presentation sampling. It does not reproduce the reference implementation's distinct tone-domain preview-resizing route. Neutral retinex generation and automatic analysis can be omitted when explicit manual controls and a constant factor table make those branches unused.

Cache keys include the effective tables, fixed-point coefficients, geometry and input dependencies. Complete LUT bytes make the prototype straightforward to audit but increase graph preparation and key sizes. Reusing immutable prepared tables is a possible later improvement after profiling. The initial version supplied normalized linear sRGB after inverse transfer. The shared finishing experiment replaces this with normalized linear ROMM to preserve wide-gamut output.

## Numerical validation

Three original files decode to 43,352,064 exact Float32 code values using the existing format-30 decoder. Complete SD15 calibration matches 14,450,688 signed reference words. Camera color matrix coefficients, nonlinear tables and captured rows match for all three profiles. Bounded repair, highlight, chroma and root/detail fixtures match exactly. Rotated detail fixtures distinguish the required first-axis order for quarter turns.

Complete composed SD15 Stage 2 differs in 661 of 14,450,688 signed words, each by one code. The same differences occur when the logarithmic FIR node is tested independently with captured inputs. Its Float32 numerical policy is explicit. These rare differences propagate through root and tone table boundaries. The exported native-size RGB16 TIFF differs in 1,296 of 13,939,200 components, with mean absolute error 0.002176 codes and maximum 221. This is a selected diagnostic-request comparison, not a claim of matching arbitrary saved application adjustments.

The original ordered highlight implementation dominated processing at 34,566 ms on Apple M1 Pro. Its parallel replacement explicitly substitutes the known earlier delayed write in the oldest-row neighborhood. Six bounded fixtures and all twelve complete SD15 intermediate buffers remain byte-identical. The complete inspection/session/export test fell from about 70 seconds to 2.14 seconds. An attempted packed workgroup despeckle optimization was reverted because the existing 62 ms pass did not justify extra storage constraints and packing logic.

## Review boundaries

This experiment establishes a complete selected Classic route and the source/recipe/operation split. It does not implement every historical Classic camera or processing branch. Further camera coverage and control mappings should follow actual fixtures. The initial profile-shaper asset established a numerical comparison without a runtime dependency on the reference implementation. It and its generator were retired after the shared finishing comparison preserved the relevant fixture and profile identities. The GUI shares preset resolution and final rendering, while its curated inspection and help vocabulary still primarily describes Merrill operations.

## Completed checks

Workspace tests, Clippy with warnings denied, formatting, generated indexes and link checks pass. Eighty-five selected GPU and original-file regression tests pass, including Merrill cached edits, monochrome and export. Six Classic original-file proofs pass after separating deferred color preparation. Calibration inspection accepts missing or unavailable color modes and absent appearance metadata, while final color rendering rejects invalid settings.

A full Merrill color-field rendering from main and this branch produces byte-identical TIFF output. The optimized SD15 fresh processing call measures about 804 ms on Apple M1 Pro, including synchronized GPU diagnostics, with about 714 ms in GPU passes. The largest remaining passes are highlight streaming at 309 ms, ordered flare statistics at 227 ms and despeckling at 62 ms. These measurements are individual runs, not a broad performance benchmark.

Calibration preparation retains a white-balance-derived color context without resolving the selected appearance or reading ColorDQ. The recipe prepares camera color only when a requested output needs it. This keeps the existing selective-inspection contract useful across families.
