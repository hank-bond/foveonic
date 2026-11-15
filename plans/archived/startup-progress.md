---
status: archived
summary: "Present a responsive loading window and capture startup host work before photographic-engine preparation."
---

# Startup progress and tracing

Implements [startup behavior](../../docs/specs/interactive-rendering.md#startup) and [startup capture](../../docs/specs/performance-diagnostics.md#startup-capture).

## Scope

Show a real loading frame before preparing the photographic engine. Report completed pipeline counts and move application initialization onto one worker. Extend the existing Perfetto capture to process-entry host intervals and the native painter's graphics setup. Keep photographic math and shader selection unchanged.

## Steps

- [x] Separate UI-owned native display configuration from owned application preparation.
- [x] Start one preparation worker after the first root Present and report pipeline counts.
- [x] Preserve loaded preferences on early close and handle worker failure without joining it.
- [x] Buffer early host observations while the existing recorder starts asynchronously.
- [x] Check startup capture ownership, early-close saving, progress and first-presentation ordering.
- [x] Run the native app and inspect startup captures. Report warm timings without claiming a controlled cold driver cache.
- [x] Obtain independent design/Rust review and make the build available for owner review.
- [x] Owner reviewed the initial functional startup slice.
- [x] Compact the loading window, add appearance-matched artwork, and restore workspace geometry.
- [x] Validate compact-window transitions, early-close geometry, both appearances and the revised native build.
- [x] Independent review of the compact visual refinement and centered placement.
- [x] Owner review of the compact visual refinement and approval to merge.

## Findings

The photographic engine eagerly prepares roughly 300 compute pipelines. Historical startup measurements attributed about 4.9 seconds to this on a first launch and about 0.2 seconds on later launches, without controlling the driver cache. eframe still creates the device and UI renderer before drawing the loading frame. The new trace separates this remaining pre-frame work from photographic initialization. The native framework deliberately hides the window until its first rendered frame to avoid a white flash.

The background-built application contains no native window pointer. A UI wrapper retains the native display owner for the window lifetime. Preparation includes cache/profile initialization and worker creation, which also perform host/file work. A two-integer snapshot and one completion channel keep the loading UI independent of preparation.

Explicit startup capture owns its recorder from process setup through handoff to the application. Inactive diagnostic controllers do not own another controller's capture. Early host observations use the existing Perfetto timestamp conversion and elapsed-interval format.

## Compact visual refinement

The native window-builder hook receives eframe's already restored and monitor-clamped workspace builder. Retain it and create a 560 × 200 loading window with a 128-point Composer icon and adjacent progress. Readiness restores size, minimum, resizability and maximized/fullscreen flags. The loading window centers once using egui's screen-centering command. Snapshot the actual original outer position before centering and restore it with the workspace. On first launch, expand around the loading window's center. On macOS the captured builder's inner position is not a runtime outer-position command, so it is never replayed as one. The pinned portable centering helper has monitor size but no monitor origin and assumes origin zero. Arbitrary secondary-display origins remain a framework limitation. If native geometry is unavailable, placement is left to the window manager.

Eframe 0.32 writes its native `window` storage value before calling App::save. While loading or while the compact geometry is still observed, preserve the original opaque value there. An empty string represents originally absent settings because Storage has no removal API and eframe reloads it as no settings. This keeps one geometry format and one native restoration owner. Workspace drawing advances after one loading-frame handoff and never waits for a native resize event, which a window manager may ignore.

The PNGs come from the existing Composer document's Default and Dark renditions. Their explicit generator path is documented with the artwork. No image-generation dependency or native Dock override is added.

## Checks and evidence

- Release GUI unit tests: 543 passed. Release core unit tests: 574 passed. Four isolated SDK lifecycle tests passed, including early retrospective intervals and inactive-controller ownership. Existing panic-save and active/starting exit-save behavior passed.
- Release Clippy for core/GUI all targets, workspace formatting, documentation indexes and whitespace checks passed.
- A native Merrill hazy-coast smoke capture completed all ten existing screenshot/interaction checks. Trace analysis reported valid capture and behavior. First root Present preceded `startup.prepare_app`, all 303 pipeline scopes were present, and native frames continued during preparation.
- One isolated warm Metal run on Apple M1 Pro measured 241 ms from entry to first root Present, 311 ms photographic preparation, and 572 ms entry to ready. There were 35 presentations during preparation. These are single-run traced observations, not a release baseline or controlled cold-cache claim. An earlier first-launch capture overlapped another compiler and is used only for structural/behavioral validation.
- A deliberately unsupported graphics backend failed before app creation and still saved its early trace. A calibration override containing a named pipe deliberately held preparation after compilation. Native Command-Q closed the loading window without joining that worker, saved the startup capture, and preserved the seeded application preferences exactly. Both fixtures were synthetic and local to the test output directory. No production delay/failure hooks were added.
- Independent review found the ownership and lifecycle design sound. Its two code findings were fixed: draw the app on the handoff frame, and name the argument milestone accurately. Photograph-loading progress is documented separately from startup's indeterminate phase.

### Compact and centered-window validation

The final GUI suite has 549 passing unit tests, including both appearances, progress and long-error layout, opaque saved/absent/malformed geometry, repeated unchanged native sizes, once-only screen centering, actual outer-position restoration and centered first-launch expansion. All six artwork tests, release GUI Clippy for all targets, formatting, indexes and whitespace checks passed.

Native screenshots confirmed the 560 × 200 client area and both icon appearances. The right column was centered against the icon and checked in a final screenshot. Primary-screen centering was checked against the actual outer rectangle, including the titlebar, to within one screen point. An early quit preserved original window and application preferences exactly and saved its capture. Releasing the held preparation restored a 1240 × 820 workspace to its original native position and completed all ten existing smoke checks with a valid capture. A fresh-storage launch separately kept both the loading window and expanded default 1440 × 1000 workspace centered and completed the smoke checks. These checks establish behavior, not latency under controlled load.

The independent review's resize-wait finding was resolved by keeping workspace admission independent of resize observation. Saved compact geometry remains protected if native resize is delayed or ignored. The final centered-window implementation and screenshot were reviewed without further findings.

## Main integration

The completed browser-loading and startup commits rebased onto the 0.8.1 capture-metadata work without conflicts. The combined release workspace passed 1,188 Rust tests, all-target workspace Clippy, six artwork tests, 185 profiling Python tests (one skipped), 11 test-environment setup tests, formatting and documentation checks. Independent integration review found the bounded reader, cache metadata refresh, export capture metadata and background startup ownership consistent.

## Deferred

Shader-module reuse by complete composed WGSL and demand-driven pipeline construction remain possible separate optimization work. Explicit wgpu pipeline-cache persistence is backend-dependent and is not the portable first step.
