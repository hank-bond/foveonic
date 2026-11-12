---
summary: "Extend real-app interaction checks and diagnostic feedback across camera families without binding behavior to the current render architecture."
---

# Behavioral testing and feedback

Implements the [performance diagnostics](../docs/specs/performance-diagnostics.md), [multi-photo editing](../docs/specs/multi-edit.md) and shared [numeric display](../docs/specs/photographic-finishing.md) requirements. The owner requested a sustainable unit, headless and headed test suite that supports architecture changes, actual slider gestures, camera-family coverage, reproducible investigations and separate latency measurements.

## Approach

Reuse the existing native scenario, input-dispatch and Perfetto owners. Add the smallest common observation and action support needed to drive actual controls and inspect their behavior. Keep the test's expected edits and geometry independent of the application state it observes. A physical macOS pointer check remains separate from egui input dispatch. Avoid a second application runtime or a broad automation framework before an actual scenario needs it.

## Test organization pass

The owner approved Astra's four organization slices. Scenarios own actions and expected behavior. Shared support owns observation, capture, process execution and image comparison. Preserve commands, evidence formats, assertions and the application runtime while changing source layout.

- [x] Move existing Rust smoke submodules into `src/smoke/` without changing their state machines.
- [x] Separate Python scenario checks from capture validation and fresh-render replay, with adjacent portable tests.
- [x] Extract complete Rust multi-edit, screenshot and feature suites, keeping actions, readiness and assertions together.
- [x] Remove Python executable-to-executable helper imports, improve local test navigation and include profiling discovery in contributor checks.

Keep application control observation in `automation.rs` and image capture/delivery timing with the render worker. Do not introduce a test framework or registry. Some older scenario checks copy observed settings into expected state. Strengthening those checks is a separate behavior change, outside this mechanical and responsibility-focused pass.

The mechanical Rust move passes all 451 GUI tests. The Python split keeps a small runner handoff: validate intended scenario edits first, then replay captured images. Named checks own slider, choice, exposure, transition and held-delivery expectations. The generic replay owns image identity, effective scale, adapter, paths, renderer execution and complete pixel/geometry comparison. All 111 existing portable checks pass after moving the image and scenario-check cases beside those modules.

Rust multi-edit, screenshot and Rotate suites now keep their targets and operations together. Rotate preserves its separate screenshot assertions, gesture actions and pending-frame readiness within one module. The central dispatcher still uses the existing state and explicit calls. Input staging has two callers and one owner in `inputs.py`. Workload identity joins UI timing in `interaction_latency.py`, removing the comparison runner's import of the application runner. The numerical matrix names its required fixture counts separately from the behavioral fixture-family list. No execution policy, processing math, test framework or evidence format changed.

Astra's follow-up review found no organization blockers or lost assertions. Its comparison preserved all 111 Python test methods, the moved helper logic and Rotate's readiness conditions. The final build passes 1,037 Rust tests, 111 portable profiling checks, five artwork checks, Clippy, formatting and document/index checks. Six targeted native runs cover multi-edit, Rotate, eight-photo controlled selection under pressure, seeded transitions, mixed choices and a sustained back-and-forth slider drag. All fourteen captured app images match fresh processing, and all forty frozen Full/Fast/Draft/Thumb comparisons remain exact. Commands and catalog listing still work. These are focused checks for the changed test support, not a rerun of the full acceptance catalog or slider matrix.

The owner subsequently approved grouping the production browser files under `src/browser/`. This mechanical move preserves the existing module names, interfaces, worker ownership and tests. All thirteen moved source/test files match the checkpoint after normalizing their module-path declarations. Validation passes all 1,037 Rust tests, 111 portable profiling checks, five artwork checks, strict Clippy, formatting and documentation/index checks. No native or numerical matrix was repeated for this source-layout-only change. The next architecture discussion is source acquisition independently of serial rendering, as recorded in the runtime plan.

The first slice is the reported multi-edit slider jitter. Reproduce it through sustained pointer input in the real app, with different photo values and renders in flight. Check the shared widget in isolation, then use the same observable requirements across actual controls and the supported Classic, Merrill, Quattro and Tiger fixtures. Report which controls and configurations were reached or unavailable rather than treating omissions as passes.

## Work

### Reproducibility follow-up

The owner requires maintained testing to be rebuildable from the repository and the example-photograph release. A storage audit found that the drivers and test catalog are tracked, but the current camera-family fixture list, ten-source numerical matrix, eight-photo interaction recipes and mixed-choice recipes still live in local manifests with personal absolute paths. The Quattro H pair also uses two hard links to one original, which must be represented as an explicit generated alias rather than a second original.

The owner approved adding `merrill-hazy-coast.x3f` to the example release. It is now published with its original SHA-256, and the existing example-sync command discovers it. The other seven numerical-matrix inputs are not the owner's photographs and must not be added to that release. The owner approved a [tracked external sample manifest](../scripts/profiling/external_raw_samples.json). All seven local hashes match RAW Pixls' published checksums and catalog metadata. The manifest records download URLs, byte lengths, hashes and per-file license/source notes for direct retrieval. This preserves numerical input identities without rehosting the photographs. SD9/SD10 retain their distinct CC BY-NC-SA terms, and the other five entries retain CC0. Downloader/setup implementation remains a separate slice. Merrill Medium/Low fixture availability also needs reconciliation.

- [ ] Move the maintained fixture selections and recipes into the repository, using example asset names and portable input resolution.
- [x] Record exact external originals and their publisher metadata in the tracked seven-file manifest, verifying their local bytes against published hashes.
- [ ] Document or provide one straightforward setup command for examples, explicit aliases and selected test manifests.
- [ ] Reconcile missing example-release assets before claiming a clean-checkout run covers every camera family and RAW size.
- [ ] Pin numerical baseline generation to approved code, recipes and input hashes. Preserve existing frozen samples while this provenance is completed.
- [ ] Demonstrate rebuilding the selected tests from a new generated directory without relying on pre-existing local artifacts or personal sidecars.

### Existing coverage work

- [x] Inventory existing interaction, numerical and diagnostic evidence and identify coverage gaps for the first slider scenario.
- [x] Add reusable, opt-in control observation and input driving only where the existing app harness cannot supply them.
- [x] Reproduce and correct the sustained multi-edit slider stall, with mixed and equal values, displayed text and geometry checks.
- [x] Declare 90 numeric control cases across camera families and configurations, with explicit coverage and missing-fixture reporting.
- [x] Preserve slider failures with settings, inputs, UI observations, screenshots and optional trace artifacts. Keep Off latency runs separate.
- [x] Validate the maintained camera-family numerical matrix and headed scenarios without relying on untracked orchestration.
- [x] Report a common result and reuse one assertion across an existing headless render and real-app scenario.
- [x] Compare settled real-app Exposure slider images with independent fresh renders, outside interaction timings.
- [x] Exercise controlled completion ordering against the current coordinator without a GPU.
- [x] Add replayable generated real-app interaction sequences and controlled worker completion ordering.
- [x] Run ordinary, headless and real-app checks, output equivalence, scoped overhead comparisons and documentation checks, preserving failures.

The native scenario runner accepts up to eight selected photos. A collection journey stages 100 aliases from eight originals and checks the eight-photo editing limit. Scenario completion follows requested foreground work rather than requiring unrelated speculative work to be globally idle.

## Coordination coverage slice

The owner approved parallel work on independent journey expectations, cache-work checks and latency feedback. One integration owner maintains the native driver and runs GPU checks serially. Separate worktrees share the normal build cache. An Astra review follows integration, with a final quality and simplicity pass. Keep the implementation focused on these concrete cases rather than a general action language or a new application runtime.

- [x] Add a small Rust model of selected photos and their expected manual exposures, initialized from fixture recipes.
- [x] Add a real-app edit/retarget/remove/re-add journey with controlled delivery timing and final per-photo image verification.
- [x] Extend existing trace checks with target-scoped reuse and rebuild expectations, requiring observed retention snapshots for eviction.
- [x] Add UI update-gap summaries, optional caller budgets, and window/viewport/pixel-density workload identity.
- [x] Verify retained and pressure journeys through actual app runs and traces across the mixed camera set.
- [x] Verify foreground editing while a real frozen export is active, then separately await that export for cleanup.
- [x] Run targeted ordinary checks, review with Astra, and simplify the integrated code.

The delivery hold pauses an already computed image before publication. It makes the known stale-result handoff deterministic while the UI continues accepting input. Its interval is explicitly recorded as harness work, and these journeys are not natural rendering-throughput benchmarks. The background case uses the existing import/export worker and no delivery hold. It must observe actual batch execution before editing and preserve evidence of foreground/background overlap.

Rust owns the application drivers, independent edit expectations and GPU capture. Python extends the existing external profiling tools to launch isolated processes, inspect artifacts, query Perfetto and summarize results. Common image assertions serve both headless and app evidence. Keep processing and scheduling policy out of those orchestration scripts.

Retained and memory-pressure journeys passed with eight photos across Classic, Merrill, Quattro and Tiger. All final image buffers matched fresh renders exactly. The pressure run observed a processing entry before eviction, its absence afterward, and photographic work for that photo on its next edit. Switching to retained output and restoring the selection required no photographic dispatches or RAW payload reads. The final reviewed pressure run had a maximum UI update gap of about 80 ms against an explicitly selected 250 ms test budget. These update gaps describe application callbacks, not physical input or screen latency.

A separate two-photo run edited exposure during actual TIFF export. Its trace placed the edit inside a batch-photo operation, and the final edited images matched fresh processing. Export cleanup completed separately from the foreground edit. Two natural Off edit trials retained matching window and viewport geometry. The controlled delivery journey remains a correctness check rather than a throughput comparison.

Astra's review kept the focused Rust drivers and Python orchestration. The resulting corrections propagate trace behavior failures into the shared result, mark incomplete traces as incomplete evidence, scope new reuse checks to the target photo, and label mixed command/input actions accurately. Workspace tests, Clippy, formatting, profiling-tool tests and artwork tests passed for that slice.

## Broad acceptance coverage

The owner approved parallel implementation of broad symptom detection before further architectural changes. Reuse the existing native smoke suites, scenario driver, slider matrix, numerical baselines and latency comparison tools. Map supported user paths to explicit checks and report partial or missing evidence. A suite pass must not imply that undeclared paths or unavailable fixtures were exercised.

- [x] Maintain one user-path catalog and subset/full acceptance runner, including existing feature suites.
- [x] Implement bounded seeded cross-feature transitions with replayable actions and independent expected state.
- [x] Implement Browse persistence, Import isolation, file operations and failure/recovery checks through application owners.
- [x] Expand declared editor-control configurations and report remaining untested controls.
- [x] Record and repeat numerical baselines across three Classic, Merrill and Quattro sources plus the available Tiger source, covering preview qualities.
- [x] Maintain repeated natural latency scenarios and matched baseline comparisons independently of correctness checks and controlled delivery pauses.
- [x] Run integrated native checks serially, then review correctness, quality and simplicity with Astra.

One integration owner runs builds and native measurements. Agents use isolated worktrees and the shared build cache. No architectural runtime or rendering-math changes belong to this testing slice. Expected application defects discovered by these checks remain failures until fixed or explicitly documented for owner review. Keep replay data and failure artifacts sufficient to reproduce symptoms without requiring a new tracing framework.

The numerical baseline records ten originals at Full, Fast, Draft and Thumb. All 40 comparisons repeated with identical floating samples and geometry.

The lifecycle journey observes Process disabled without a destination and through pick, unpick and repick before clicking it. A separate shutdown case submits Browse edits and requests close immediately, then verifies the saved sidecars after process exit. This does not prove a write was still pending at close. The choice journey uses separately declared settings, while generated transitions and the 100-alias collection cover bounded mixed workflows. The acceptance runner records natural latency baselines separately from comparisons and uses explicit disk-cache budgets.

## Findings

The previous real-app scenario edited settings directly and the slider unit check sent only one movement. Egui reports `drag_delta()` for the current frame, but the group slider subtracted the preceding frame's delta as if it were cumulative. Repeated equal movements were dropped. The control now consumes each movement and retains fractional movement for stepped values. Unit tests fail on the prior behavior and cover sustained movement and reversal.

The native slider scenario observes actual widget output and drives the widget through egui input. It records settings, readout text and geometry, pointer position, frame time, busy state and presented task identities while the gesture and later rendering progress. It checks mixed and equal starting values, including a back-and-forth gesture. Equal values expose the formatted numeric readout. A missing or unreachable control is a failure, with an inventory and screenshot. It can scroll to an offscreen slider. The matrix declares 90 numeric control cases and lists observed but untested controls and missing camera families. Hidden modes and undeclared RAW sizes remain outside that inventory.

The existing exact image checks depended on an ignored local driver. The maintained matrix uses the existing headless renderer, checks source identity and complete float samples, and reports camera and preview coverage. A small four-family Thumb matrix repeated exactly, and separate comparisons matched the previous full-float baselines.

The first shared result observes a source, processing family and the scale reported by its rendered artifact in both drivers. One test-side assertion checks the published Full/Fast/Draft/Thumb scale policy independently of the application. Other assertions and evidence remain with their existing driver until they describe genuinely shared behavior.

The slider scenario can capture each final app artifact and compare its complete floating samples and geometry against fresh processing for the declared numeric control. The check verifies declared starting values and derives the intended change from pointer movement before replaying the exact verified result. Diagnostic snapshots retain transient render settings that are intentionally absent from sidecars. The existing image worker performs opt-in image readback and image file writes through a bounded diagnostic queue; the UI polls completion. Capture begins after all measured actions and any trace save, and fresh processing runs after the app exits. Both headless baseline checks and app comparisons use the same image assertion. This does not establish physical screen presentation.

The same Classic and Merrill inputs were run through both drivers at Thumb quality. Both reported the same original-source hashes and passed the shared scale assertion at divisors four and eight respectively. A separate Wall capture included the same app result. Automated checks also inject a wrong delivered scale, an unknown family and an undeclared staged source so missing or incorrect evidence cannot pass the shared check.

Actual Exposure gestures passed exact image comparisons for a two-photo Merrill pair and an eight-photo mixed set containing three Classic, two Merrill, two Quattro and one Tiger photograph. Every final floating buffer and its complete geometry matched fresh processing. A separate two-photo Wall run saved its trace before image capture and included the successful image assertions in its summary. A deliberately changed byte in a copy of a real captured buffer failed the shared result and reported its byte offset. Portable checks also reject truncated samples, incomplete geometry, wrong starting exposure, stale frame identity and an effective exposure that differs from the checked edit. These results cover this particular Exposure journey, not all controls or source modes.

An implementation-level test exhausts all 720 completion orders for three photos with an old and a newer request. It checks that obsolete results cannot replace admitted newer ones and that every latest request settles independently. This exercises the current coordinator directly. It is not a real-worker concurrency test or a substitute for a user-level behavioral model.

Routine headed checks select cases, fixtures, families and themes explicitly. A full matrix requires `--full` and is reserved for release checks. On macOS the behavioral runner defaults to a background window. An application-activation observer confirmed that the foreground app stayed active while a real scenario rendered and a deliberately failing case saved a usable screenshot. Background and foreground timings are distinct workloads because window occlusion may affect scheduling.

## Checks and remaining coverage

The first ten slider cases passed in both themes across four declared camera families before the background and scrolling additions. The later build passed the new Merrill and Universal controls, Quattro-specific stepped controls, an offscreen scroll, a back-and-forth exposure gesture and an eight-photo background trace. The eight-photo check observed the final rendered settings for every photo. The maintained float matrix recorded and repeated one Thumb case per family, and those four matched the existing pre-change full-float samples.

Three matched background Off and Wall trials passed for a two-photo slider gesture. Their elapsed times were close in this small sample, but that does not establish a general capture-overhead bound. Both measurements include the control observer, so the observer's own overhead is not isolated. Final integrated native and full-matrix checks and review remain pending. Undeclared editor modes and RAW sizes, physical mouse input and every possible interaction ordering remain outside the reported coverage.

The broad native run exposed stale harness assumptions. Quattro Smooth/Crisp fixtures must start on the displayed sixth-step grid. The older multi-photo smoke assumed every Thumb used divisor eight, although Classic uses four. Smart needs a second source for neighbor preparation, and retained reopening must preserve the original thumbnail pixel revision when its presentation revision advances. Its direct edits must update both fixture recipe owners before reselection. A retained reopen does not require a decoded-disk cache hit. Dehaze reset now restores a distinct saved amount to exercise an actual render change. Legacy smoke steps fail with readiness state after a 60-second deadline. These corrections change test setup and expectations, not application processing.

Trace validation now compares the ordered step IDs, actions and photo counts against the completed app result. An otherwise valid trace that omits a whole step cannot pass. Acceptance progress reports also remain incomplete until every selected job finishes.

## Pending edit re-add failure

The eight-photo cached-switch check exposed a failure with a mixed set ordered as two Merrill photos, two Quattro photos, one Tiger and three Classic photos. It uses Thumb, a 1 GiB memory budget and no decoded disk cache. Select A, B, then A again, Option-add B, change both exposures by +0.125, Option-remove B while work is pending, then Option-add B again. This case belongs to the regular correctness catalog as well as the natural latency catalog.

In three initial Off trials and a separate Wall diagnostic, the final step did not settle. B's editor and sidecar request were +0.275 EV, while its retained frame remained +0.15 EV, was not admitted, and the worker reported idle. A's edited frame was correct. The failed result includes per-target expected settings, editor and sidecar requests, frame identities and admission. Preserve these failures independently of the corrected runs below.

The Wall summary recorded ten desired, completed and admitted jobs but nine UI image acceptances. Source inspection suggests a result can be marked settled before the UI's later destination recheck rejects it, after which an identical re-added request coalesces without re-delivery. The saved trace confirms that B's edited task completed and was admitted during removal, but had no UI acceptance. The later re-add generated no new desired B task. A diagnostic event now records final UI destination skips separately from coordinator admission so this handoff can be inspected directly. This is an application delivery finding, not a rendering-math change or an excuse to relax the test. Fixing that ownership handoff should be a separate reviewed implementation slice.

The natural latency baseline used three trials per case, with five edits per warm-edit trial. Eleven cases completed and matched their comparison workloads. Ten stayed within an explicitly selected 50% median/p95 validation budget. Draft's median stayed near 25 ms while its p95 moved from about 30 to 53 ms. A targeted repeat passed the same gate. Preserve both observations. With fifteen edit samples, nearest-rank p95 is the maximum, so this is a tail alert that needs variability calibration, not established processing regression. No optimization is claimed from these test runs. The cached-switch case has no valid successful latency baseline until its correctness failure is fixed.

The owner approved two focused delivery fixes after reviewing these findings. The first keeps a completed photograph in the existing bounded per-photo owner when its result still permits photo updates but no longer has a viewport destination. Superseded authored edits and changed calibration do not receive that permission. Whole-photograph eligibility is shared with preview preparation and excludes inspection, framing, temporary masks and regional images. The original eight-photo Wall reproduction now passes without changing its expectations. Its trace still exercises the late UI visibility rejection, and the final retained B result has the requested +0.275 EV.

## Cache-off Import preview readiness

With the legacy smoke disk cache explicitly disabled, reopening the same photo in Import reaches a valid current image, matching settings, settled scopes and a ready navigator, but has no rendered browser-thumbnail revision. The Import screenshot sequence waits for that preview and fails its deadline. The normal cache-enabled run and the separate cache-off lifecycle/copy checks passed. Treat this as a separate preview/readiness finding, not a demonstrated failure to process files. The preview persistence policy gates deferred preview preparation, while folder reopening clears browser revisions and may reuse a retained viewport image without another image reply.

The approved second fix republishes a browser preview from a compatible retained photograph on selection. One publication helper serves incoming Active and Overview images, retained offscreen completions and cached selection. It identifies pixels by their delivered revision, checks that both browser memory pools still contain the preview, and uses the existing GPU snapshot operation without readback or photographic rendering. Disk persistence retains its separate policy. The original cache-off Import sequence now completes through copying.

Legacy scripted actions now request a regular repaint while active. This prevents their progress and deadline diagnostics from depending on unrelated disk-cache notifications. Thumbnail readiness uses delivered-image identity rather than requiring a new presentation revision to regenerate unchanged pixels.

## Integrated validation checkpoint

The declared numeric-control matrix completed 720 applicable case/theme/fixture runs, with 1,440 final app images matching independent fresh processing. It covers the four available camera families and Merrill Medium/Low fixtures. All observed declared numeric controls were exercised. Tiger uses two aliases of the one available original, not two independent sensor samples. The ten-original headless matrix repeated all 40 Full/Fast/Draft/Thumb results exactly.

The integrated catalog has 46 non-latency jobs. At the initial checkpoint, forty-four passed after focused harness corrections. The pending-readd correctness case and cache-off Import preview-readiness case failed, with their distinct symptoms above. The inventory also reports native dialogs as missing and records partial paths rather than treating those as passes. No rendering math or runtime scheduling policy changed during that testing pass. An Astra review found no testing-code blockers, including in the failure-only snapshots and explicit cache-budget controls.

The final ordinary checks passed 1,035 Rust tests, 111 portable profiling tests and five artwork checks, plus Clippy, formatting and documentation/index validation. Parallel implementation used isolated worktrees and a shared build cache. Native runs were serial. Completed owned worktrees and identifiable test-only cache entries were cleaned up after preserving their evidence and changes.

### Focused delivery fixes

The follow-up build passes nine related catalog jobs: multi-photo smoke, Smart, Before/After workflow, cache-off Import, eight-photo cached switching, controlled selection, eight-photo selection under memory pressure, seeded view transitions and lifecycle recovery. The four image-capturing journeys compare fourteen complete app buffers and their geometry exactly against fresh processing. All forty frozen Full/Fast/Draft/Thumb cases remain exact. The original eight-photo cached-switch scenario also passes three new natural Off trials. These runs establish completion and retained edits, not a calibrated performance improvement or an automatically approved latency baseline.

Two CPU regressions cover the final view recheck's independent photo-update permission and preview availability after thumbnail or larger-preview eviction. Ordinary checks pass 1,037 Rust tests, 111 profiling-tool tests, Clippy, formatting and documentation/index validation. The broader catalog checkpoint remains historical evidence rather than a claim that all 46 jobs were rerun on this build.

### Source-loading ownership coverage

The `source-loading` case now holds one native load while another photo's visible Exposure control stays enabled and its new Thumb frame reaches settled/admitted delivery. It runs with original access and decoded disk hits. The intentional pause is behavioral evidence, distinct from natural latency. The regular cache catalog includes this case. Related regression evidence and remaining loading boundaries live in the [runtime plan](./render-runtime.md#source-loading-checks-and-lessons).
