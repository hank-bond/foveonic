---
summary: "Measure simple color-field optimizations while preserving recovery equations, solver limits and numerical acceptance."
---

# Color-field speed pass

## Scope

The owner approved an autonomous optimization pass through meaningful low-complexity ideas, followed by review. The public names are **Classic**, **Color Field: Local**, and **Color Field: Global**. Internal algorithm codes and em dashes are not method names. Classic remains default. Do not commit, install, close the running app, change owner edits or delete retained evidence.

Follow the [color-field requirements](../docs/specs/color-field.md) and [implementation plan](./color-field.md). Keep equations, full-sensor geometry, 64 Local sweeps, stopping bounds, iteration limits and the `2e-4` identical-evidence error bound unchanged. Keep compensated color iterates and genuine true-residual validation. No fallback or CPU recovery.

## Method

Freeze the starting renderer and sources. Use separate no-export directories for each candidate. Compare completed warm renders and separate diagnostic samples, including the sky, photographic priority scenes and severe-clipping regression. Recheck successful candidates across the corpus. GPU workloads must not overlap. The app may remain running but idle. Keep all failed candidates and measurements. Image storage is limited, so avoid new photographic sweeps.

## Candidate checklist

- [x] Rename public UI/help/CLI/report methods and migrate callers.
- [x] Cache repeated bind groups within a submission batch while retaining exclusive ownership guards.
- [x] Batch checkpoint submissions and reduce unnecessary host synchronization.
- [x] Compact color unknowns, omitting fixed rows already eliminated from neighboring right-hand sides.
- [x] Pack scalar brightness work vectors and avoid vector arithmetic for scalar equations.
- [x] Test solver workgroup sizes and convergence-check batch sizes.
- [x] Test compute-pass coalescing if ordering and profiling remain simple and correct.
- [x] Inspect shader recomputation, memory lifetime, graph propagation and local relaxation for further simple wins.
- [x] Remeasure accepted changes against the frozen start with Classic as a control.
- [x] Run full numerical, integration, GUI, report and repository checks and rebuild the uninstalled app.

## Baseline observations

Existing profiles show that native matrix products, updates and direction passes dominate Global. Local still pays for the shared color solve over the entire grid, including fixed rows. Bind groups are rebuilt for every dispatch. Checkpoint initialization and reduction submit separately. Graph-distance propagation is a smaller shared cost. Classic downstream processing is outside the approved optimization target.

## Results

### Retained implementation

- Color unknowns exclude fixed rows that were already eliminated into neighboring right-hand sides. The existing brightness index maps are reused after tone commitment. No new image-sized buffers are needed. Fixed-row RHS maxima remain part of the original stopping bound. A repaired-only case checks the zero-color-unknown path.
- Brightness uses packed scalar vectors and 48-byte equations instead of four-channel vectors and 64-byte equations. Color retains compensated four-component iterates and packs its two-component work vectors. Native preconditioning uses scalar arithmetic.
- The solver keeps the preconditioned residual already computed for its reduction. Direction updates reuse it instead of loading and preconditioning the equation again. Isolated heavy-Global improvements were approximately 5–10 percent. Packed equations added approximately 5–8 percent on the heavier scenes.
- A submission batch borrows the union of required resources once, caches each repeated kernel's bind group and releases guards only after submission. Normal rendering puts ordered dispatches in one compute pass. Diagnostic samples retain separate passes for per-stage timestamps and are explicitly not normal latency measurements. Coalescing alone improved complete renders by roughly 7–20 percent across the measured cases.
- Checkpoints join the preceding iteration submission. The solver retains 256-thread workgroups and checks every 16 iterations. Native/color iteration caps and true-residual validation remain unchanged. Graph-propagation batches and caps remain unchanged.

### Rejected experiments

All measured candidates used separate frozen renderers and no-export directories. Rejected code was removed, not retained behind runtime switches.

| Candidate | Observation and decision |
| --- | --- |
| 128/64-thread solver groups | No consistent gain. The 64-thread sky Global run was about 7 percent slower. Retain 256. |
| 64-iteration CG batches | Extra work after recursive convergence slowed heavy Global cases by about 5–7 percent. Retain 16 after comparison with 32. |
| Fused direction and matrix-product passes | Increased neighbor-vector traffic and temporary storage. No gain over separate passes and a slower sky case. |
| Precomputed native inverse diagonal, including Local | No repeatable useful gain. Keep ordinary scalar division. |
| Workgroup-aggregated native compaction | No useful gain and a slower 0313 Global case. Avoid extra barriers and compaction machinery. |
| Color/native Jacobi initial guesses | No convincing improvement. Apparent Local gains also occurred in a native-only trial that could not change Local, so they were not credited to the seed. |
| Eight-sweep Global presmoothing | Small, inconsistent improvements did not justify extra processing and configuration. |
| Factored block preconditioner | Passed the oracle but did not provide meaningful speed gains. Keep the existing representation and checks. |
| Sum-only matrix-product/alpha reductions | No consistent gain and an occasional slower Global case. Avoid another reduction implementation. |
| Skip fixed donors in distance propagation | Small stage savings did not establish a useful complete-render gain. |
| Single-writer atomic distance stores | No useful complete-render gain. Keep the established atomic-min implementation. |
| Eight-sweep distance batches | More readbacks generally cost more than the saved sweeps. Restore 32 and the original 4096-sweep cap. |
| Precomputed edge lengths | Extra edge storage and code did not help complete renders. Restore original edge representation. |

Further ideas were screened against the scope. Subgroup reductions and backend-specific precision/compiler paths add device requirements. Multigrid, stronger spatial preconditioners, shared-memory irregular-stencil tiles and pipelined CG add substantial solver complexity. Different grids, active-only work or fewer Local sweeps change the selected photographic methods. Persistent scratch pools or splitting brightness/color into cached graph artifacts add lifetime and invalidation machinery. Source preparation, final application and optional-kernel setup are small remaining costs, so their possible savings do not justify those changes in this pass.

### Evidence discipline

The first compact-color test invocation omitted the fixture environment variable. Its four fixture-dependent failures are retained as a setup error, not numerical failures. Its short synthetic GPU work also overlapped the checkpoint-only timing trial, so that timing trial is excluded from acceptance evidence. Subsequent GPU jobs ran serially. Some later samples had larger completed times without extra dispatches or readbacks. Do not attribute those to solver iterations or to a seed that did not change that method. Final comparison uses paired baseline/optimized ordering with Classic controls.

Nine dedicated GPU tests passed on the retained code, including all 16 full rasters against identical-evidence CPU equations. Maximum normalized error was approximately `8.84e-5`, below the unchanged `2e-4` bound. The new Local test independently checks all 64 damped sweeps against a scalar CPU recurrence.

### Paired final timings

Four isolated rounds ran baseline / optimized / optimized / baseline. Each scene/method has 14 warm completed-render samples per binary. First renders and synchronized GPU diagnostics are excluded. All 540 planned renders succeeded. Classic medians stayed within approximately 0.3 percent, while color-field improvements were repeatable in both optimized rounds.

| Scene | Global before/after ms | Global time reduction | Local before/after ms | Local time reduction |
| --- | ---: | ---: | ---: | ---: |
| 0010 sky | 2209 / 1133 | 48.7% | 864 / 546 | 36.9% |
| 0095 petals | 697 / 539 | 22.7% | 579 / 493 | 14.9% |
| 0286 knit | 924 / 540 | 41.6% | 516 / 382 | 26.0% |
| 0313 skin/wrap | 1313 / 870 | 33.8% | 822 / 658 | 20.0% |
| 0399 regression | 528 / 383 | 27.6% | 465 / 372 | 20.0% |

Geometric-mean completed-time reductions across these five scenes were about 36 percent for Global and 24 percent for Local. These are M1 Pro fresh full-pipeline timings, not GUI latency. The no-export speed report is separate from the retained photographic report. No old image, timing, recipe or failure was relabeled as an optimized-binary measurement. No peak-residency claim is made.

### Final checks and handoff

All 192 planned all-corpus Classic/Global/Local validation renders and all 42 repeated severe-clipping Global/Local renders passed. Together with the paired timing rounds, the final frozen candidate/baseline checks completed 774 render samples without failures. The nine numerical/integration GPU tests and all eight GUI GPU tests passed. The unchanged strict six-original stock-cache test passed once. Its earlier intermittent failure remains unexplained, not claimed fixed.

Ordinary workspace tests, strict Clippy, formatting, generated-index checks and all 11 Python report tests passed. Browser validation checked all 15 registered regions and 195 native crops with no JavaScript errors. Rebuilding the photographic report changed only labels and findings. Every other report-data field and its raw measurement hash stayed unchanged. All 16 originals and 14 frozen sidecars still match retained SHA-256 values.

The release app and CLI were rebuilt offline. The app was not installed or launched by the assistant, no application default changed and no commit was made. Frozen baseline/candidate binaries, raw measurements, rejected trials and before-update reports remain preserved. Owner photographic review and the separate agreement/boundary-control decision remain open. No further low-complexity idea found in this pass demonstrated a meaningful gain beyond the retained changes.
