---
summary: "Extend real-app interaction checks and diagnostic feedback across camera families without binding behavior to the current render architecture."
---

# Behavioral testing and feedback

Implements the [performance diagnostics](../docs/specs/performance-diagnostics.md), [multi-photo editing](../docs/specs/multi-edit.md) and shared [numeric display](../docs/specs/photographic-finishing.md) requirements. The owner requested a sustainable unit, headless and headed test suite that supports architecture changes, actual slider gestures, camera-family coverage, reproducible investigations and separate latency measurements.

## Approach

Reuse the existing native scenario, input-dispatch and Perfetto owners. Add the smallest common observation and action support needed to drive actual controls and inspect their behavior. Keep the test's expected edits and geometry independent of the application state it observes. A physical macOS pointer check remains separate from egui input dispatch. Avoid a second application runtime or a broad automation framework before an actual scenario needs it.

The first slice is the reported multi-edit slider jitter. Reproduce it through sustained pointer input in the real app, with different photo values and renders in flight. Check the shared widget in isolation, then use the same observable requirements across actual controls and the supported Classic, Merrill, Quattro and Tiger fixtures. Report which controls and configurations were reached or unavailable rather than treating omissions as passes.

## Work

- [x] Inventory existing interaction, numerical and diagnostic evidence and identify coverage gaps for the first slider scenario.
- [x] Add reusable, opt-in control observation and input driving only where the existing app harness cannot supply them.
- [x] Reproduce and correct the sustained multi-edit slider stall, with mixed and equal values, displayed text and geometry checks.
- [ ] Exercise every applicable slider across camera-family and control configurations, with explicit coverage and missing-fixture reporting.
- [x] Preserve slider failures with settings, inputs, UI observations, screenshots and optional trace artifacts. Keep Off latency runs separate.
- [x] Validate the maintained camera-family numerical matrix and headed scenarios without relying on untracked orchestration.
- [x] Report a common result and reuse one assertion across an existing headless render and real-app scenario.
- [x] Exercise controlled completion ordering against the current coordinator without a GPU.
- [ ] Add replayable generated real-app interaction sequences and controlled worker completion ordering.
- [ ] Validate ordinary, headless and real-app checks, current output equivalence, profiling overhead and documentation indexes.

The current native scenario runner accepts up to eight selected photos. A later collection journey should move across groups of a larger folder without raising the multi-edit limit. Scenario completion must follow the requested foreground work rather than requiring unrelated speculative work to be globally idle.

## Findings

The previous real-app scenario edited settings directly and the slider unit check sent only one movement. Egui reports `drag_delta()` for the current frame, but the group slider subtracted the preceding frame's delta as if it were cumulative. Repeated equal movements were dropped. The control now consumes each movement and retains fractional movement for stepped values. Unit tests fail on the prior behavior and cover sustained movement and reversal.

The native slider scenario observes actual widget output and drives the widget through egui input. It records settings, readout text and geometry, pointer position, frame time, busy state and presented task identities while the gesture and later rendering progress. It checks mixed and equal starting values, including a back-and-forth gesture. Equal values expose the formatted numeric readout. A missing or unreachable control is a failure, with an inventory and screenshot. It can scroll to an offscreen slider. The starter matrix covers 40 shared and camera-specific control cases, not every conditional control or RAW size. Its report lists observed but untested controls and missing camera families.

The existing exact image checks depended on an ignored local driver. The maintained matrix uses the existing headless renderer, checks source identity and complete float samples, and reports camera and preview coverage. A small four-family Thumb matrix repeated exactly, and separate comparisons matched the previous full-float baselines. The next expansion should add the remaining conditional controls and reproducible interaction sequences without coupling expected results to the current scheduler.

The first shared result observes a source, processing family and the scale reported by its rendered artifact in both drivers. One test-side assertion checks the published Full/Fast/Draft/Thumb scale policy independently of the application. Other assertions and evidence remain with their existing driver until they describe genuinely shared behavior. In particular, this does not independently measure image dimensions or compare the actual app image with a fresh headless image.

The same Classic and Merrill inputs were run through both drivers at Thumb quality. Both reported the same original-source hashes and passed the shared scale assertion at divisors four and eight respectively. A separate Wall capture included the same app result. Automated checks also inject a wrong delivered scale, an unknown family and an undeclared staged source so missing or incorrect evidence cannot pass the shared check.

An implementation-level test exhausts all 720 completion orders for three photos with an old and a newer request. It checks that obsolete results cannot replace admitted newer ones and that every latest request settles independently. This exercises the current coordinator directly. It is not a real-worker concurrency test or a substitute for a user-level behavioral model.

Routine headed checks select cases, fixtures, families and themes explicitly. A full matrix requires `--full` and is reserved for release checks. On macOS the behavioral runner defaults to a background window. An application-activation observer confirmed that the foreground app stayed active while a real scenario rendered and a deliberately failing case saved a usable screenshot. Background and foreground timings are distinct workloads because window occlusion may affect scheduling.

## Checks and remaining coverage

The first ten slider cases passed in both themes across four declared camera families before the background and scrolling additions. The later build passed the new Merrill and Universal controls, Quattro-specific stepped controls, an offscreen scroll, a back-and-forth exposure gesture and an eight-photo background trace. The eight-photo check observed the final rendered settings for every photo. The maintained float matrix recorded and repeated one Thumb case per family, and those four matched the existing pre-change full-float samples.

Three matched background Off and Wall trials passed for a two-photo slider gesture. Their elapsed times were close in this small sample, but that does not establish a general capture-overhead bound. Both measurements include the control observer, so the observer's own overhead is not isolated. The release matrix has not been rerun after the later harness changes. The starter cases also do not yet cover every conditional editor mode, source RAW size, physical mouse path, generated interaction sequence or alternate completion order.
