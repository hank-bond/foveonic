---
summary: "Replace the separate serial and Merrill-only wide paths with one core Scheduler, working set and priority order before releasing 0.9.0."
---

# Render scheduler

The owner held 0.9.0 after review found that wide rendering runs only warm Merrill work beside a separate serial path, with admission decided in the GUI. 0.9.0 will ship its current features, including linked multi-photo viewing, on this architecture. Implement one slice at a time with owner review between slices.

Requirements: [render scheduling](../docs/specs/render-scheduling.md), [render orchestration](../docs/specs/render-orchestration.md), [processing graphs](../docs/specs/processing-graphs.md) and [interactive rendering](../docs/specs/interactive-rendering.md). Vocabulary: [scheduling names](../docs/architecture/concepts.md#scheduling-vocabulary). Release: [0.9.0](./release-0.9.0.md).

## Starting point

Review of the merged candidate found these overlaps. Each slice removes some of them. Paths refer to the code at `9ec9a78`.

- **Five render paths.** `driver::render_session` waits after every operation. The wide path in `render/experimental.rs` lets its caller wait once per group. `Engine::render_session_observed` renders a borrowed native session and serves non-Merrill zoomed regions and export. `render_reduced_session_observed` serves preview prefetch. `render_artifacts` serves fresh renders and the CLI.
- **Merrill only.** `Wide::inputs` in `worker/wide.rs` sends every other camera family to the serial path. `admission::workspace` returns no estimate for other families.
- **Three completion modes.** `Completion::Queued`, `Operation` and `Window`, plus `SubmissionWindow`, `BackgroundStage`, buffered progress and a separate per-operation reservation branch. `Operation` is a one-operation window.
- **Admission split three ways.** The engine estimate (`estimate_job`) covers only Reduced, whole-image, final-only work with diagnostics off. The GUI uses hand-tuned Merrill constants and a 1 GiB-per-native-photo floor (`worker/admission.rs`) and discards the engine estimate. Serial trimming skips while wide jobs run, and prefetch has its own capacity check. Diagnostics disable the engine estimate, and GPU profiling forces serial.
- **Per-photo exclusivity three times.** The working-set execution claim, `Cursor::claim` in the coordinator and a scan of `Wide::jobs`.
- **Priority twice.** The serial path uses decode-cache execution wrappers. Wide uses `at_priority`, which never runs idle work and repeats export and pick checks. Render priority is reached through the decode cache. The core gate's `Decode` level is declared last but ranks between navigator and thumbnails.
- **Session lifecycle in the GUI.** `Photos::render_reduced` sequences engine lookup, source opening, refresh, preparation, insertion, rendering, removal and activation. `Photos::warm` repeats part of it.
- **Duplicated setup.** Settings resolution, edit-change detection, cancellation, progress reporting and diagnostics are written separately for each path.
- **Handoff machinery.** `Start`, `Next`, the serial fallback queue, `retire()` before serial work, `executions_idle` checks and wide-only test counters.
- **Width set two ways.** `set_render_width(1)` selects serial in tests while `FOVEONIC_WIDE_RENDER_WIDTH=1` selects wide width one. Width travels as a UI command though only tests change it.
- **Feature flag.** `wide-experiment` is off in core and on in the GUI, so the CLI build lacks the wide API.
- **Single active photo.** One `active` working-set slot protects one photo between renders. Multi-photo editing relies on recency for the rest.
- **Kept images in the GUI.** Before images stay per photo until replaced or the photo leaves the edit set. The GUI reports their memory to the worker separately.

## Slices

Each slice ends with an independent review, owner testing and the checks listed for it. Record discoveries below.

- [x] **1. Baseline.** Record exact outputs, latency and memory for the current candidate to compare against. Add behavioral tests:
  - a slider drag while speculative work runs causes no upstream recompute
  - a Before render keeps the current edit's restart inputs.

  Report whether either test fails today.
- [x] **2. Priority.** Introduce one `Priority` with the five tiers. Use it in the queue order and the cross-thread gate. Stop reaching render priority through decode-cache methods. Merge prefetch, idle and run-ahead decode levels into Speculative, ranked below Thumbnail and Export. Requested decoding keeps its job's tier. Update the decode-cache and browser-preview specifications.
- [x] **3. One execution path.**
  - Merge `RenderExecution` and its inner `Job` into one execution that steps through its graph.
  - Replace `Completion` and `SubmissionWindow` with the tick wait.
  - Route non-Merrill zoomed regions, export, prefetch, the CLI and fresh renders through it. Delete the other render paths and the `wide-experiment` flag.
  - Behavior stays one job at a time.
  - Check exact outputs for every camera family against slice 1. Measure export throughput without the queued mode.
- [x] **4. Estimates.** Produce a memory estimate for every family, resolution, region, quality and diagnostic mode. Compare estimates with measured allocations. Diagnostics and profiling must not change them.
- [x] **5. Working set and guards.** Core parts are done: `WorkingSet`, `WorkingPhoto` types (`NativePhoto`, `ReducedPhoto`), `Resolution`, `PhotoKey`, `Guard`, no active photo or parking, recency on use, and one trimming order. The GUI parts land with slice 6 so the worker is rewritten once.
  - Rename sessions to working photos and `SessionKind` to Native and Reduced resolution.
  - Replace execution claims with guards. Remove `active`, parking and `SessionUse`. Use one trimming order.
  - Move path and source-identity tracking and the cold-preparation sequence from GUI `Photos` into the working set.
  - Make kept results working-set entries with GUI guards and weak handles. This covers Before images, Smart detail, previews and speculative full images. Remove GUI memory reporting (`MemoryBudget.frontend`, `PendingImage`, `memory_inventory`).
  - Update [render resources](../docs/render-resources.md) to match.
- [x] **6. Scheduler at width one.** Design:
  - `render::Scheduler<J>` in core owns the working set, the memory limit, the queue and the admitted executions. Jobs carry a target, a priority and an exclusive flag. A newer job for a queued target replaces it and keeps its place within the tier. Waiting for a source is a queue state that the host's readiness check clears.
  - The GUI supplies a `Host` with four hooks: `start` (reuse a shown image, request a source, prepare inputs, or describe the render), `interruption` (continue, cancel or yield), progress, and `finish` (deliver the result). These hold the existing request semantics. The Scheduler holds every ordering, admission and tick decision.
  - Exclusive jobs (color picks, scopes, export and idle preparation) start only when no execution is admitted.
  - Each tick takes the highest tier with ready jobs, retires lower-tier executions back to the queue, applies cancellations and yields, admits, steps every admitted execution once and waits for the GPU once.
  - The Scheduler takes over the queue, ticks, source waiting, color picks, scopes, export and speculative preparation.
  - Remove `Worker`, `Cursor`, `Wide` and `admission.rs`.
  - Behavior should match slice 5.
- [ ] **7. Wider admission.**
  - [x] Admit jobs within the budget, dropping only unguarded results.
  - [x] Add the test-only width cap and remove the width command and environment variable.
  - [x] Check exact output at widths one, two, four and eight for every family.
  - [ ] Measure latency and memory for single-photo and multi-photo editing, including Speed and Quality and zoomed regions. These run with the release performance comparison.
- [ ] **8. Documentation and release.** Update the [implemented map](../docs/interactive-rendering-architecture.md), [profiling guide](../docs/profiling.md), conformance status and remaining plans. Return to the [0.9.0 plan](./release-0.9.0.md) for the full release lanes and the comparison with published 0.8.4.

- [ ] **9. Memory target and protection.**
  - Rename the memory limit to a target. It is a best-guess steady state, not a hard cap: admission plans whole jobs against estimates, trimming returns to the target, and peak should stay close to it. No artificial upper bound, so large machines can raise it freely.
  - Protect visible photos (the latest viewport requests) instead of the most recent photo. Each keeps its latest edit's restart inputs. Other photos may be evicted.
  - Order restart inputs by when their edit was last used instead of protecting only the last two. Pressure removes the oldest first. Only foreground use stamps recency. In multi-edit, one change's checkpoints get adjacent recency across photos, so they leave close together.
  - Replace the cache's own `Cache::LIMIT` eviction with the working-set trim so one ordering applies.
  - Sensor data stays until its working photo is evicted.
  - One recency pass: after cached images, obsolete and optional checkpoints and orphan preparation, drop whichever was used longest ago, an edit's restart inputs or a whole working photo that is not visible.
  - Apply the agreed names in code: `set_shown` → `set_visible`, `KeptResults`/`KeptHandle` → `ImageCache`/`CachedImage`, `required_cuts` → sensor data.
- [ ] **10. Export and CLI on the Scheduler.** Export becomes an ordinary render job at Export priority that writes when it finishes. The CLI runs its renders through a Scheduler with a minimal host. Neither keeps its own execution path.

- [ ] **GUI naming follow-up.** Rename the GUI `Purpose` (what a result is) to `Rendition`, its `Preview` value to `Thumbnail`, and the GUI `Rendition` (Current, Camera, Defaults) to `SettingsSource`, since it only chooses edit settings. `Destination` (main viewport, overview tile or thumbnail, decided when a result arrives) and the core `Purpose` stay.

- [ ] **11. Comment cleanup.** Remove comments that do not add value. Agree the criteria with the owner before changing anything.

## Decisions

- Names (2026-11-18): file, sensor data, working scale, checkpoint, restart input; selected, visible and active photos; image (what a job delivers), display texture, visible image and cached image. They are recorded in [concepts](../docs/architecture/concepts.md#scheduling-vocabulary).
- Trimming (2026-11-18) replaces the slice 5 and 6 rules in Discoveries: one recency pass across restart inputs and whole non-visible working photos. Visible photos and their latest edit's restart inputs are never dropped.
- Purposes (2026-11-18): every job carries a purpose (Edit, View, Auxiliary, Thumbnail, Export, Speculative) that sets its priority. Every use is recorded with its purpose, and one table in the working set decides what it refreshes. Only edits refresh restart inputs, so Before images and color picks keep their photo without displacing the edit's inputs. Visibility is per working photo.
- The CLI uses the application's default memory target, never the GUI's user setting, and does not assume the GUI is installed. `Engine::render` stays as a direct render for tools and tests.
- Export ranks above Speculative and below Thumbnail. Once interaction stops, export should run at roughly CLI speed; speculative work uses leftover compute.

## Follow-ups before publishing 0.9.0

- **Evaluate checkpoint retention concretely.** Measure plain recency on real editing sequences, especially Quattro detail edits after tone and color edits. Consider weighting recency by rebuild cost, frequency of use, smaller checkpoint formats and spilling to disk only if measurements show a need.
- **Pipeline memory audit.** First findings from per-step profiles on an M1 Pro (one cold and four warm full-resolution renders per family):
  - Sensor upload converts 16-bit codes to 32-bit floats on the CPU, doubling the size (Merrill 97 MB decoded, 193 MB uploaded). Converting on the GPU would halve the upload. Until then the CPU conversion is a SIMD candidate.
  - Quattro native photos retain the uploaded sensor cells (about 130 MB) for regional views.
  - Display textures use `Rgba32Float` (16 bytes per pixel, about 235 MB for a full Merrill image). `Rgba16Float` would halve them without visible loss, since only geometry and filtering follow. Browser previews already downsample to 16-bit.
  - The memory target is one number summing CPU and GPU bytes. On a discrete GPU, display textures and render buffers use VRAM, a separate and usually smaller budget, so the sum cannot show which pool is short. Account CPU and GPU memory as separate pools, and when unifying the accounting of shown and kept images (after the view-planning migration), count them per pool.
  - The GUI keeps each visible image's buffer beside its display texture to rebuild the texture after crop or rotation changes, for scopes and for diagnostic capture. Consider applying crop and rotation when drawing and computing scopes from the texture, so the buffer can be released once the texture exists.
- **Test-suite pass.** Map spec requirements to behavioral tests, find gaps and reorganize. The main goal is a sustainable way to manage coverage across the full surface: one maintained list of common rendering tasks (thumbnail multi-edit, multi-photo native zoom, Before/After, single-photo zoom, export and others), each with its automated scenario, checks and profiling, so regressions surface without manual testing. Agree that design with the owner before reorganizing. Findings so far:
  - Many tests check implementation rules, such as "the most recent photo stays", so every design change breaks them without finding defects. Prefer tests of required behavior.
  - Runs are slow: 95 affected core tests took 82 minutes locally. Run long suites on the performance Mac through the inventory plan, and consider splitting GPU groups across processes.
  - The chroma research experiments are ignored tests under `interactive` that panic without research paths. Move them out of the test build, for example into examples.
  - Native scenarios carry behavior no specification records. View planning found these only by breaking them: clearing the worker re-renders displayed photos; the loupe never replaces a photo's main image; returning from inspection restores the kept photograph; a cold-start step expects fresh renders after a clear. Record each as a specification row with its check.
  - Scenario failures are bare timeouts ("step N did not settle"). Diagnosing each took longer than fixing it and needed a trace. A failing wait should name the condition it waited for and the last relevant state, for example the photo whose image never became current and the last `view.plan` event.
  - The feedback loop is 10–20 minutes per native run on the performance Mac. Unit tests passed throughout and found none of the integration problems, so cheaper integration checks between the planner and the app (for example the app's planning loop without a GPU) would shorten it.
- **Diagnostic capture cleanup (pre-release prep).** If capture stays in the release build so users can send traces, it must work smoothly without Python. Found so far: quitting the app waits for capture pruning, which reads every capture's `summary.json` one byte per system call (`prune_captures` in `crates/foveonic/src/diagnostics_capture.rs`). `trace_analysis.py` writes its analysis into those files, growing them to 3–39 MB, and quitting then took minutes.
- **Quattro performance leads.** Full renders take about 47 ms per megapixel against Merrill's 13, mostly from extra stages. Luma clusters takes 126–156 ms on DP Quattro files but 32 ms on the larger SD Quattro H, with 60 dispatches in one step. Quattro decode takes 300–390 ms against Merrill's 146 ms.

## Discoveries

- **Import hiding (during slice 9).** Imported and hidden files took about 30 s to conceal while rendering was busy, because the whole import scan waited on the priority gate and published history and settings together. Measured on SD cards with 182–1269 files, history matching costs a few milliseconds and the cold settings pass 3–4 ms per file. History matching now runs on its own ungated thread and publishes first. Settings stay gated and stop when a newer scan arrives. Two GPU tests cover a single scan and a destination change behind a paused earlier scan; the review found the second case.

- **Slice 9 review.** The first version let a Before render take over the edit's restart-input protection, kept the 64 GiB cap, held a pending stamp in the cache that a failed refresh could leak into a later speculative commit, protected both resolutions of a visible photo and could miss a photo loaded during the tick. Purposes, an explicit stamp on each render execution, per-working-photo visibility refreshed before every trim and removing the cap fixed them. Three memory tests had assumed the most recent photo was protected.

- **Baseline (slice 1).** The exact-output matrix for the held candidate covers three Classic, three Merrill, three Quattro and one Quattro H source at every resolution and both qualities through the fresh render path. Rendered samples are kept outside the repository for comparison. Earlier latency and memory evidence for the held candidate is in the [0.9.0 plan](./release-0.9.0.md). Slice 7 measures widths against that.
- **Speculative work already costs edited work (slice 1).** `speculative_whole_image_keeps_the_edited_region_warm` fails on the held candidate. After a speculative whole-image render under a full budget, the next regional exposure edit runs two dispatches instead of one. `before_render_keeps_the_current_edit_warm` passes. Both are ignored GPU tests in the worker test module.
- **Priority (slice 2).** `foveonic::priority` replaces the internal `background` gate. `Priority` has the five tiers and the gate's methods take a tier: `demand`, `ready`, `attempt`, `when_ready`, `run` and `speculate`. Run-ahead decoding now pauses for thumbnails like other speculative work. Requested decoding keeps the requester's tier. The decode cache exposes the shared gate through `Cache::priority` instead of one method per tier. The GUI's `TaskPriority` is gone.
- **One execution path (slice 3).** `RecipeExecution` in `render/driver.rs` prepares a recipe, starts its `Traversal` and steps it, borrowing the input for each call. Fresh sources, borrowed sessions, sealing graphs and working-set sessions all use it. `RenderExecution` wraps it with the working-set claim. `Completion`, `SubmissionWindow`, `experimental.rs` and the `wide-experiment` flag are gone. Each step runs at most one required node without waiting. `Engine::tick` owns the single GPU wait, after which progress is published and scratch trimmed. Profiled nodes record the tick wait as their completion wait. Non-Merrill zoomed regions now render through the working set like Merrill. All 80 exact-output matrix cases match the baseline.
- **Fresh renders wait every step (slice 3).** The queued mode that let fresh renders and exports submit up to 2 GiB of allocations before waiting is gone. Export throughput is measured in slice 7.
- **Estimates (slice 4).** Every remaining node gets an allocation estimate. Audited contracts cover eight operations. Every other operation reserves its described output size, or 16 bytes per pixel when the output has no image description, plus temporaries at three times that size. Native sessions, zoomed regions, profiling and diagnostics no longer disable the estimate. A GPU test checks that the estimate exists, is unchanged by profiling and covers cumulative allocations for Classic, Merrill, Quattro and Quattro H at reduced and native-region views. The per-node gaps that set the factor of three were Quattro two-scale noise (3.6 times its generic estimate at one temporary), sparse repair, raw color blend and phase gain, and Merrill highlights. These are candidates for audited contracts if widths prove too narrow.
- **Working set (slice 5).** Sessions are now working photos: `WorkingSet`, `NativePhoto`, `ReducedPhoto`, `Resolution`, `PhotoKey`, `PhotoRender`, `PhotoInUse` and `Guard`. The decode cache's private recipe state became `PlannedRecipe`. There is no active photo or parking. Foreground use moves a working photo to most recent. Trimming drops obsolete history, then optional alternatives, then preparation no retained photo owns, then least recently used photos. Restart inputs are never trimmed piecemeal.
- **Trimming order (slices 5 and 6).** Existing tests caught two cases. With a budget too small for even one photo, trimming evicted the photo being edited after every render. Making all restart inputs untrimmable then caused older photos to be evicted whole instead of losing checkpoints, so they had to be decoded again. Trimming now drops obsolete and optional history, then older photos' restart inputs, then orphan preparation, then least recent photos. The most recent photo keeps the restart inputs of its two latest recipes and is never evicted. Keeping two recipes also keeps Before and After warm. These rules need owner review because the design discussion did not spell them out.
- **Scheduler (slice 6).** `render::Scheduler` owns the working set, the memory limit, a `Queue` and the admitted executions. `Queue` orders by tier, then by first submission. A replacement keeps its place and takes the new priority. Waiting jobs hold back lower tiers. Deferred speculative jobs neither run nor hold a tier until memory changes. The GUI's `WorkerHost` (`worker/host.rs`) supplies start, interruption, progress, yield and finish hooks. Color picks, scopes, exports and idle preparation are exclusive jobs. `Wide`, `admission.rs`, `Cursor`, the serial execute closure and the wide test suites are gone. GUI `Photos` keeps only path and source identity and borrows the working set. Cursor service-order tests moved to queue unit tests.
- **Kept results (slices 5 and 6).** `render::KeptResults` holds results the UI keeps without showing. On leaving Before, the UI moves Before images into it and keeps weak handles. On entering Before it takes back what survived and renders the rest again. The Scheduler counts kept results against the budget and drops the oldest first when trimming and before giving up on running wider. Other UI-held images are still reported through the frontend inventory.
- **Peak estimates (slice 7).** A sum of every allocation made the scheduler pre-trim other photos before native renders and kept speculative work from ever fitting. Estimates are now the peak new memory over the remaining steps: outputs accumulate, released inputs become reusable scratch, temporaries add per step, plus 10% tile-padding headroom. The coverage test compares them with measured boundary memory growth for every family and view. Measured full-resolution renders were 1.3 to 2 times below the estimate. The pre-trim and `ExecutionBudget` are gone. The first job runs, and trimming after the tick restores the budget.
- **Speculative admission (slice 7).** Counting a speculative render's transient peak meant idle full-image completion never ran at the default 2 GiB budget. Since the budget is the steady-state cache, speculative jobs now run when their delivered result fits in free memory. This interpretation needs owner review.
- **Width verified (slice 7).** `every_family_renders_identically_at_every_width` renders two photographs from each family as warm reduced edits and native regions at width caps 1, 2, 4 and 8. Wider runs admit several jobs together and match width one exactly. `warm_group_latency_by_width` is the maintained timing experiment.
- **Coverage removed with the wide worker (slice 6).** The deleted wide tests also covered preview yielding below navigator demand, live budget reduction retiring peers, and replacement during submission. Queue unit tests and the regression set cover ordering and replacement. Budget reduction during a group has no behavioral test yet. The `wide*.py` investigation scripts were already orphaned before this work because their `profile_wide` binary no longer exists. They remain for owner review.
- **Independent review of slices 3–7.** Fixed:
  - The GUI host loaded or prepared photos before the scheduler checked fit, so a job that did not fit changed recency and memory and retried every tick. The host now loads only for a job that runs alone. Jobs that cannot start defer until a job finishes or retires, trimming frees memory, or the budget changes.
  - Use is recorded at admission.
  - The scheduler keeps one job per photo, so a reduced and a native job for one photo no longer fail with `PhotoInUse`.
  - Speculative jobs waiting for an original retry only after another job makes the photo resident. They no longer spin.
  - A running task whose priority changed was queued a second time and replied twice. Queued render jobs whose stamp was served are dropped.
  - Kept results are dropped only to run wider, not for the first job or speculative work.
  - Trimming also runs after any finished job.
  - Idle preparation counts kept results.
  - The retirement guard waits before the traversal releases storage.
  - Export releases a photo it loaded.
  - Restoring Before frees its kept entry even when a frame is already shown.
  - Remaining session names were renamed. `Scheduler` behavior has GPU unit tests with a recording host.
- **Open conformance gaps (for owner review).**
  - Export runs to completion inside an exclusive job, so it cannot yield to viewport demand at each step.
  - The CLI renders through the shared execution but not through a `Scheduler`.
  - Idle reduced preparation still runs as one exclusive job with its own result-fits check, which is the speculative admission rule applied in the host.
  - Delivery waits once more for view-texture conversion before handing a frame to the UI.
  - At width above one, each job's profile records the shared tick wait.
- **Second review.**
  - The width test sampled admitted jobs after the tick had removed finished ones. Warm edits finish in one step, so width was never seen. The scheduler now records the widest stepped set.
  - Speculative jobs without their original defer instead of spinning.
  - Speculative and first jobs never drop kept results. Wider admission drops them only when that makes the candidate fit.
  - Kept handles discard their result when dropped, so a photo leaving the editor frees its Before images.
  - A job retired for overflow stays deferred until memory changes.
  - Start timing events follow the residency check.
  - A failed export still releases a photo it loaded.
  - The full inventory run and GUI regression need rerunning after these fixes.
- **Inventory run before the second-review fixes.** 487 of 495 required tests passed. Failures to investigate:
  - `reduced_active_cut_survives_shared_budget_pressure` expects the most recent photo to be evicted, which conflicts with the never-evict rule.
  - Four GUI presentation tests (`presentation::tests::*` and `completed_finer_snapshots_*`) unwrap `None` or saw no dispatches. The likely cause is the deferral and residency changes in `start_render`.
  - Two GPU gate timing tests timed out: `background_readback_yields_*` and `manual_capture_resolves_*`. They may be flaky; rerun them alone.
  - The width test fails because of how width was sampled, which the second-review fixes address.
- **Inventory follow-up.**
  - The presentation tests read per-revision dispatch counts. The host had recorded them for every finished job, including cancelled and retried attempts, and not for presentation reuse. It now records them only for delivered frames.
  - The reduced-photo budget test now expects the most recent photo to stay.
  - The priority slice had made `Gate::run` wait at entry. Work that holds a GPU map, such as a background readback or a trace capture, then deadlocked behind viewport demand. `run` again starts immediately and pauses only at the work's own checkpoints. The batch scan's pause point calls `checkpoint` explicitly.
  - The width test passes with width counted where jobs step.
- **Slice 7 measurements.**
  - **Correctness:** all 495 required inventory tests pass on the current code.
  - **Warm edits by width:** `warm_group_latency_by_width` measured an 8-photo warm exposure edit (release build, one family fixture each). Reduced views took 84.7, 79.5, 78.9 and 77.7 ms at width caps 1, 2, 4 and 8. Native 512-pixel regions took 101.1, 87.7, 86.2 and 83.4 ms. Width helps native regions most. Reduced edits are already short.
  - **Fresh renders and export:** `profile_pipeline` compared the pre-work baseline, slice 3 and the current build. Medians varied 2–4× between identical runs on this host, so they cannot show a difference. Minimum completed times were comparable: Merrill 78–94 ms in all builds, SD Quattro H 126–140 ms at baseline and 131–145 ms now. Peak RSS was equal or lower (SD Quattro H 1.42 against 1.46–1.54 GiB).
  - **Still needed:** the release performance comparison needs an uncontended host.
- **Final review of the last changes.**
  - Restoring `Gate::run`'s no-wait entry had also removed the entry pause that every public wrapper had before (`background`, thumbnail and processing work), so the import scan stopped pausing between files. `run` waits at entry again. The two GPU tests now raise viewport demand after their work has entered, which matches production.
  - Wider speculative jobs had been admitted under the result test and retired under the peak test, so they could cycle every tick. They are now admitted under the same peak test `retire_overflow` applies, without dropping kept results.
  - A view that can reuse the shown image no longer waits for its working photo.
  - Export releases a photo it loaded on every error path.
  - Kept results are freed after the store unlocks, and a taken handle removes its entry once.
  - Unit tests, the GUI correctness group and the added GPU group pass.
- **Slice 7 completion.** Admission already admitted jobs while they fit, so this pass changed no admission logic.
  - The UI width command and its 1–8 clamp are gone. The worker reads a test-only cap that `TestWorker::set_width_cap` writes, so `Scheduler::set_width_cap` is the only production setter. The inventory planner test uses a neutral sample variable, and the wide-rendering experiments document is marked as a historical record.
  - `every_family_renders_identically_at_every_width` now covers Speed as well as Quality.
  - Scheduler GPU tests now cover a lowered target retiring the most recently submitted job while the older one keeps running, an exclusive job starting only after admitted jobs finish, higher-tier demand retiring every lower-tier job, the width cap and a speculative peer leaving cached images in place where an edit peer drops them. The scheduler has no eviction path, so retirement cannot evict a working photo. This closes the budget-reduction gap left when the wide worker was removed.
  - The owner decided that latency and memory measurements for this slice run with the release performance comparison.
