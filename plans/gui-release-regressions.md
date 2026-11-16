---
summary: "Resolve GUI and GPU correctness failures and require packaged desktop checks before releasing 0.8.2."
---

# GUI release regressions

The owner requires fixing known acceptance failures before publication. Keep both Windows corrections and this follow-up in 0.8.2. The initial export-only tag was pushed but remains unpublished. Its workflow was cancelled before preparing the revised release.

Requirements: [processing quality](../docs/specs/processing-quality.md), [viewport workflows](../docs/specs/viewport-workflows.md), [scopes](../docs/specs/gui-workspace.md), [Dehaze](../docs/specs/dehaze.md), [range names](../docs/specs/advanced-color-range.md) and [release verification](../docs/specs/releases.md).

## Evidence and scope

The complete 27-job legacy native smoke suite passed 23 jobs and failed four on the candidate Mac bundle. Those same four assertions fail identically in the published 0.8.1 Mac archive with the same pinned photographs and staging rules. The owner rejected deferring them to another release.

- Multi-photo selection asserted divisor eight for every unchanged overview. Automatic Quality/Speed chooses resolution from each photograph's physical display demand and camera limits. Preserve the captured scale and texture identity through selection while independently checking that initial resolution satisfies display demand.
- Dehaze asserted native resolution at Fit. The accepted behavior supports every working grid. Check selected quality, physical display demand, Whole geometry and the authored/estimated color behavior.
- Viewport scopes compared native readout coordinates with the reduced sampled buffer. Derive the expected sampled rectangle independently from visible image geometry and the actual displayed buffer extent. Preserve revision/freshness checks and histogram/density conservation.
- Color-range renaming actually dirtied retained settings and submitted image work. Preserve full name changes in session settings and sidecars while invalidating rendering only for render-relevant changes. Cover active, requested and multi-photo updates and preserve already pending work.

Correcting the fixed-scale multi-photo assertion exposed a second production defect. A rapid click on photo A followed by a click on B could trigger B's 200% snap zoom and replace its unchanged overview with native processing. egui's click count is global, while the application interpreted it as belonging to the current photograph. A focused unit test reproduces the failure without GPU timing. The correction requires click ownership, position and timing on the same surface, keeps main/navigator history separate, and excludes modified selection clicks from main-image zoom. An experimental layout-retention change did not fix the failure and was removed.

The completed selection sequence exposed another stale input assumption: browser photo seven was not the final visual tile for the mixed aspect ratios. The application correctly selected seven photos in that visual range. The test now chooses endpoints from independently sorted drawn rectangles and sends a normal click followed by a Shift-click, while still requiring all eight original paths. The complete multi-photo job now passes, including cached-texture reuse, relative edits, removal, Before/Current, visual range, Space and shared-choice stages.

The broader non-latency GUI inventory contains 47 smoke, scenario and slider jobs. After all 27 legacy smoke jobs passed, its first scenario failures identified three further harness issues. Prefetched selection could accept a finer retained image without warming the currently demanded grid, so the first actual grid transition was incorrectly labeled a warm edit. A separate named setup edit now prepares that grid while preserving and reporting its I/O, with all original measured zero-read checks retained. The selection-pressure journey had the same browser-order endpoint assumption and now uses drawn range endpoints. Source-loading, background editing and the other control-observation scenarios enabled thread-local automation on the preparation worker after the startup redesign. Initialization now happens on the UI thread before drawing, scoped to the five scenarios that request observation. Exposure reveal/readiness checks must succeed before held/background work starts, then again while it runs. Targeted native reruns passed both selection journeys and source-loading, background-edit, lifecycle and choice observation. The full matrix remains the final gate.

The Windows workspace run also exposed timestamp fixtures that opened files without write access. Correcting those fixtures revealed a production cache-recency failure: successful decoded and preview reuse silently failed to refresh modification times. A shared best-effort timestamp helper now opens a nontruncating write handle separately, preserving successful reads when timestamp updates are unavailable. Existing decoded LRU coverage and new preview/data-integrity/read-only cases validate the behavior.

The additional synthetic GPU inventory initially failed fourteen cases on Windows. Actual adapter observation showed that the headless constructor selected Vulkan while the GUI selected DX12/DXC. Six failures disappeared under the GUI configuration. Core and GUI now share the Windows descriptor. Hosted Windows unit tests independently selected DX12/FXC under the old defaults and failed shader compilation, confirming that default configuration was not an adequate test of the shipped application. Common preparation now stages the pinned DXC pair before Windows CI tests.

Three remaining failures came from synthetic fixtures: a missing two-tile window and two Quattro sources that no longer met supported native geometry/calibration requirements. Those fixtures retain their original numerical and ownership assertions. A residual-LUT test conflated exact index selection with backend-dependent final multiply/add contraction. Three sentinel tables now expose the exact native indices independently through unchanged production kernels.

The remaining corrections preserve small compensated ColorField updates against compiler simplification, use positive interpolation weights for small HDR gains, correct highlight donor division at integer-bin boundaries, and count signed-detail range endpoints before normalized arithmetic. Independent equations, exact tiny updates, true residual/convergence checks, adjacent-float boundaries and density conservation remain the acceptance criteria. The final 249-case synthetic suite passes on Metal and Windows DX12/DXC. All 155 public-original cases have passing Metal evidence, with Windows verification still running. The offscreen GUI inventory passes all 42 cases on both platforms after correcting stale float-target, uniform-layout, duplicate-request and setup-baseline fixtures. The full native matrix and exact release archives remain gates.

These corrections must not replace expected behavior with values copied from the candidate. Geometric scale/region expectations come from the accepted display and scope contracts. The before/after cached-image assertion remains an identity invariant.

The final Windows original-file sweep exposed two Quattro failures at divisor `2^31`. An independent real-size probe isolated zero highlight evidence while RGB pooling retained the correct mean. Cells exceeding 65,536 native samples now reduce each row separately and accumulate those rows in their original order. Ordinary cells retain the existing pass. Threshold and partial-edge fixtures compare the two paths byte for byte, including nonzero highlight correction. Real-size fixtures retain exact peaks, classifications, halos and input samples. The new near-zero correction assertion has an explicit eight-ULP budget at its 2000-code mean. Existing numerical tolerances are unchanged. Both complete Quattro scale sequences and all 252 synthetic cases now pass on Metal and Windows.

Comparison against pristine released 0.8.1 exposed a second boundary issue in the earlier donor fix. A rounded boundary product could promote a genuinely below-boundary donor. Integer comparisons now implement the documented donor ratio without assuming fused floating arithmetic. Independent boundary fixtures include the synthetic counterexample and affected original donors. Numerical verification retains the original released-code mismatch and can classify an intended change only when the complete output exactly matches pristine 0.8.1 with that donor shader alone. Any remaining difference is a blocker.

The 144-case numerical comparison completed with 104 outputs identical to pristine 0.8.1 and 40 exactly matching its independently verified donor-only variant. No difference remains unexplained. Original mismatch reports and both sets of outputs are retained.

One hosted Windows ordinary core process exited with a native access violation after the diagnostic tests completed. The current candidate passed, and a controlled repeat of the untouched earlier source also passed on the identical runner image, Rust and Clang versions. The original failure remains unattributed and unreproduced. Failure hooks now retain the exact test executable, symbols, graphics module identities and optional GPU phase trace for any recurrence. Require the final candidate's complete checks to pass without claiming that the historical crash was fixed.

The previous packaged candidate passed native startup, rendering, normal-stack and matching-symbol checks on both platforms. Its broader run passed 38 jobs and 141 slider cases before another local build blocked admission. That run remains incomplete. Complete report-accuracy corrections and restart the full native suite with the updated package.

## Checks

- [x] Reproduce all four failures on the candidate and published baseline.
- [x] Independently review the required behavior with Astra and diagnose the two areas with Terra.
- [x] Correct render-neutral name propagation with focused ownership/sidecar tests.
- [x] Replace obsolete scale/coordinate assumptions with geometric expectations and retained-image invariants.
- [x] Reproduce the cross-photo double-click defect in a deterministic unit test.
- [x] Verify the corrected click ownership with native multi-photo selection and the existing double-click/tool cases.
- [x] Pass all four targeted native jobs.
- [ ] Pass the complete native GUI smoke suite and workspace checks.
- [ ] Verify exact packaged macOS and Windows binaries through native startup and rendering.
- [ ] Review final changes independently and publish the approved 0.8.2 patch.
- [ ] After 0.8.2 ships, review how failing ordinary and opt-in correctness tests escaped the release process. Agree enforceable release gates and test ownership with the owner before implementing the follow-up.

Prepare photographs with `python3 scripts/profiling/prepare_inputs.py --offline --directory <new-directory>`. Run all `driver: smoke` jobs from the maintained acceptance catalog using eight sources from generated `scenario-cases.json`, the pinned Quattro H photograph for `--tiger-source`, and Hazy Coast for `--dehaze-source`. Use the packaged GUI executable and a new evidence directory. Capture the suite report and failed-job logs. Baseline comparison uses the published 0.8.1 archive and identical sources/selectors. These runs check behavior and do not establish latency or physical screen presentation.
