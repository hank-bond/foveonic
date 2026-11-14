---
summary: "Attribute loupe-motion stuttering through sustained input and native presentation timing before changing scheduling."
---

# Loupe motion diagnostics

The owner reports stuttering during sustained loupe movement, especially on the hazy coast, after accepting the [layered presentation and registration work](./archived/layered-detail-presentation.md). Follow the [viewport requirements](../docs/specs/viewport-workflows.md) and [measurement contract](../docs/specs/performance-diagnostics.md). The owner approved the diagnostic slice and subsequent worker-local completion experiment below.

## Findings

The loss-free owner capture contains UI updates with a median of about 1.6 ms and maximum of 8.4 ms. Some intervals between updates during held gestures exceed 100 ms, but existing input evidence records button presses rather than motion. These intervals alone cannot distinguish idle-held time from stalled movement. A native thread sample includes waits in Metal surface acquisition and the shared wgpu submission lock. Some surface waiting is normal display pacing, so this is evidence for further attribution rather than a confirmed bottleneck.

An external automated drag replay coalesced press and release into the same or adjacent updates and did not reproduce sustained motion. The in-process driver needs to keep the gesture active across frames rather than relying on this external replay.

An apparent modifier-selection hang is unconfirmed because investigation overlapped an intentional quit. A later delayed quit completed normally with its capture saved. The shutdown path explicitly waits for diagnostic saving. Keep that observation separate from the motion-stutter investigation.

## Diagnostic slice

- [x] Agree the diagnostic scope with the owner.
- [x] Record pointer motion through the existing input diagnostics.
- [x] Drive a fixed-duration path from monotonic elapsed time through the native raw-input hook, with press and release on separate frames. Compare motion within retained coverage with a sweep crossing section boundaries. Keep screenshot/readback outside measured intervals.
- [x] Observe the existing surface-acquisition, queue-submission and presentation host intervals through one optional vendor callback, carrying viewport/pass identity. Use the existing Perfetto recorder without a vendor dependency on the image processor. The observer defaults to absent.
- [x] Obtain independent largest-model review and compare the measured waits before choosing a scheduling change.

These measurements describe host waits and in-process input cadence, not physical presentation or operating-system input latency. If those three intervals do not explain the gaps, extend attribution to preparation and command encoding next.

## Controlled results

The maintained two-photo transition journey now includes a three-second cached-coverage control and a three-second wider sweep during its single-photo loupe segment. Position advances from monotonic elapsed time rather than frame count. Input/draw barriers preserve the final sample before checking settlement, and later zoom checks use the section produced by the sweep. Independent Python checks reject absent, coalesced, stationary or unordered motion evidence.

One matched Off replay and one Wall attribution run use the owned hazy coast with the owner's Natural/Universal recipe on Apple M1 Pro, a 2 GB memory budget, a 1440 by 1000 point window, an 864 by 881.5 point viewport and pixel density two. Both are uncontended and pass the journey. The Wall run also passes fresh-image comparisons and has a complete loss-free trace. These single runs establish the symptom and attribution, not a latency regression threshold.

| Motion | Off input gap p95 / maximum, ms | Wall input gap p95 / maximum, ms | Wall UI submission p95 / maximum, ms |
| --- | --- | --- | --- |
| Cached coverage | 9.3 / 9.5 | 9.4 / 10.4 | 0.27 / 2.63 |
| Section replacement | 139.3 / 155.2 | 130.6 / 171.0 | 122.7 / 160.9 |

The Wall cached control generates no desired render requests and no processing submissions. Its surface acquisition remains ordinary display pacing, below 8 ms. The replacement sweep generates 43 desired requests and 220 processing submissions. Its longest UI submission intervals almost completely overlap render-worker completion waits. The longest UI interval is 160.9 ms, overlapping 160.7 ms of a 161.3 ms native NoiseBase completion wait.

The pinned wgpu 24.0.5 implementation confirms the mechanism. [Device polling takes the shared fence read lock](https://github.com/gfx-rs/wgpu/blob/v24.0.5/wgpu-core/src/device/global.rs#L1798-L1804), [blocking maintenance waits while retaining it](https://github.com/gfx-rs/wgpu/blob/v24.0.5/wgpu-core/src/device/resource.rs#L358-L393), and [submission requires the fence write lock](https://github.com/gfx-rs/wgpu/blob/v24.0.5/wgpu-core/src/device/queue.rs#L984-L1001). A worker-side blocking poll can therefore stall UI submission even though the application puts numerical rendering on another thread.

A separate GPU-diagnostic replay also passes. Summing the disjoint NoiseBase dispatch durations for each task requested during the sweep gives a median of 111.9 ms across fifteen tasks. The bilateral-base dispatch alone has a median of 59.9 ms. These device durations exclude host gaps and are not request latency. They confirm that real GPU work can remain ahead of window drawing even after the shared-lock stall is addressed.

Validation passes 525 ordinary GUI tests and 157 Python checks, with one platform-dependent Python check skipped. The timed-motion unit checks that skipped frames advance directly to the current position. A real egui pass-counter test verifies the painter's completed-pass identity. Independent code review found two journey ordering issues, both resolved: final input receives a complete draw before settlement checks, and the subsequent zoom test tracks the newly settled section.

## Completion experiment

- [x] Agree the completion-wait experiment with the owner.
- [x] Preserve existing completed-stage, cancellation, publication and resource-retirement boundaries. Register a finite completion callback, use nonblocking polling, and wait briefly outside wgpu between polls through one shared helper. Avoid global queue-empty waiting, which could chase ongoing UI submissions.
- [x] Compare all three presentation stages, motion cadence and detail settlement before claiming a visual fix. A shared GPU queue can still delay drawing behind already-submitted compute work after the host lock stall is removed.

Independent review supports this narrow experiment. One private polling loop serves submitted-work completion and mapped readback. Submitted stages drain before cancellation or retirement, while background readback retains its existing interruption policy. A send-only callback captures all work submitted before registration, including any racing UI submission, but later window submissions do not extend that finite boundary. The calling worker polls nonblockingly, then parks on the channel for up to one millisecond outside wgpu. This introduces no separate polling thread or scheduler abstraction. Polling cadence may add per-stage overhead and must be measured.

Two serial Wall/Off pairs replay the same recipe and journey on the same host. All pass, and the first Wall run also passes fresh-image comparisons. Both traces are complete and loss-free, with no other detected app or compiler workload. This workload check does not exclude operating-system, compositor or unrelated GPU activity.

| Section-replacement measurement, ms | Before | First candidate | Candidate repeat |
| --- | --- | --- | --- |
| Wall window submission p95 / maximum | 122.7 / 160.9 | 1.5 / 5.6 | 2.2 / 11.6 |
| Wall surface acquisition p95 / maximum | 6.9 / 8.0 | 68.8 / 127.9 | 89.8 / 674.0 |
| Off input gap p95 / maximum | 139.3 / 155.2 | 112.4 / 129.2 | 117.1 / 127.6 |

The long window-submission stall is substantially reduced. Remaining acquisition delays are not yet attributed. The repeated Wall cached control has no render requests, processing submissions or processing waits, yet includes a 390.8 ms acquisition outlier. This prevents assigning every residual delay to queued compute. Off cached-control input-gap p95 varies from 9.3 ms before to 12.3 and 19.7 ms in the candidates. These runs support the lock-contention improvement, not a precise overall speedup or a claim that visible stutter is fixed.

Summed interactive-stage waits across the journey are 19.99 seconds before, 20.08 seconds in the first candidate and 20.72 seconds in the repeat, across 1122, 1127 and 1125 stages respectively. Median individual waits remain about 3.9 ms. Whole-journey elapsed time increases from 35.9 to 40.4 seconds in the first Wall candidate, but that interval includes screenshots and test coordination. Motion-end to the subsequent post-settlement pointer release increases from 1039 to 1182 ms and then 1863 ms. Detail settlement has not improved. These are observational runs with different motion samples and request counts, not isolated per-stage polling-cost estimates.

Validation passes 40 exact render comparisons against the preserved pre-change renderer: three Classic, three Merrill and three Quattro photographs plus one Tiger, each at Full, Fast, Draft and Thumb. The submitted-work test verifies exact copy/readback and sequential reuse. Existing GPU checks verify background readback yielding, submission gating and asynchronous diagnostic completion. Independent review found no remaining correctness or design blocker after removing duplicate wait spans.

## Follow-up

- [x] Owner review of sustained loupe movement in the candidate app. Stuttering remains after the initial native section appears.
- [ ] Attribute residual acquisition stalls and measure how much compute can queue ahead of drawing before proposing a separate submission or scheduling change.

This slice changes completion servicing only. Preserve that boundary while evaluating the remaining responsiveness and settlement costs.

## Owner follow-up

The subsequent loss-free owner capture includes three held-loupe gestures over a four-photo comparison, with a 4 GB processing budget. UI updates have medians of 1.5–1.7 ms and maxima below 6.6 ms during these gestures. Window submission stays below 4.4 ms, while acquisition reaches 93–116 ms. Larger gaps between updates without corresponding measured work cannot be classified as stalls from real pointer input alone because the user may have paused movement.

Replacement processing continues after the first native section appears. The second hazy-coast gesture publishes 96 detail requests, starts 71 jobs, reports 67 superseded completions and completes three results. NoiseBase repeatedly executes. Current loupe requests contain only 32 native pixels of reuse margin per side. Loading one section therefore does not supply native coverage for arbitrary later movement.

Before this slice, returning into displayed native coverage skipped new requests without withdrawing an earlier uncovered request. That request could continue while the displayed image already covered the lens. The earlier desired and draw evidence lacked enough regional geometry to distinguish this case from genuinely uncovered movement in the owner capture.

The owner approved coverage diagnostics, pending-demand withdrawal and an independently measured reuse-margin experiment. This does not authorize a scheduling or kernel change.

- [x] Record visible bounds, compatible native image identity and coverage, submitted revision and region, and request decisions on change. Include capture identity so starting a new capture emits current state. Desired work also records its transient preview region explicitly.
- [x] Compare the original 32-pixel margin before and after withdrawing newer demand when retained native pixels cover the lens again. Preserve completed detail demand and existing resource owners.
- [x] Add a held boundary-crossing, return and second crossing to the native journey. Verify retained image identity, retirement before admission and fresh detail after leaving again. Keep this controlled test after the timed motion windows. Independent Python checks reject missing, unchanged or insufficient coverage evidence.
- [x] Compare a larger margin independently and select it only if its measured tradeoff is useful. Reject it and retain 32 pixels.
- [x] Complete final source review and checks. Make the selected build available for owner testing.

In the new baseline three-second sweep, 59 requests start 28 jobs, all superseded during motion. Withdrawal with the same 32-pixel margin still publishes 61 requests and starts 29 jobs, with 28 superseded during the window. Each observes only one native image during the sweep. Off input-gap p95 changes from 114.6 to 110.0 ms, with variable cached controls. This supports the coverage-return correctness improvement, not a substantial movement-latency gain from withdrawal alone.

The first withdrawal Wall replay passes native behavior and fresh-image equality but fails strict startup cache-read accounting. A pre-existing background read crosses capture startup without its surrounding context. Its cache lookup reports interruption and zero I/O errors, but the trace cannot satisfy the required enclosing-prefetch evidence. Preserve that failure separately. Subsequent Off and Wall replays pass the complete journey and accounting checks. Do not weaken accounting to waive the incomplete evidence.

The independent margin trial adds one native lens radius of reuse margin per side, with a 32-pixel minimum, leaving visible bounds and magnification unchanged. Its sweep publishes 31 requests and starts 17 jobs, with 15 superseded and 172 processing submissions, compared with 61 requests, 29 starts, 28 superseded and 125 submissions with withdrawal at 32 pixels. Both observe only one native image during the sweep. Off input-gap p95 is 108.1 versus 110.0 ms, while the maximum increases from 124.9 to 251.6 ms. Controls vary, so this is not a reliable regression estimate. Larger jobs progress further but show no convincing latency or useful-feedback benefit. Both trial journeys pass, and the Wall trial passes fresh-image equality. Independent review agrees to reject the default change. Restore its code, comment and geometry test rather than retaining an experimental option.

The selected slice keeps coverage diagnostics, withdrawal and the return regression. It removes unnecessary pending demand but does not establish a stutter fix. Investigating repeated expensive regional processing or GPU submission timing requires a separate agreed slice.

Final validation passes 525 ordinary GUI tests, 158 Python checks with one platform-dependent skip, strict affected-crate Clippy and documentation checks. The rebuilt selected executable is identical to the withdrawal executable used for passing Merrill native replays. The additional Quattro Wall journey exercises the new coverage-return checks and passes fresh-image equality, but its overall report remains unverified because one event was dropped during a capture transition. The Quattro Off journey fails earlier at the unchanged second-tile double-click-to-Fit assertion, before the loupe driver starts, so it supplies no loupe validation. Source review identifies no direct behavioral path from this slice to that earlier failure, but its cause remains unresolved. Preserve both reports and leave complete Quattro validation outstanding.

Potential next experiments remain separate: allow useful same-edit detail already in flight to finish while coalescing the latest region, investigate capture-start/stop evidence completeness, and resolve the earlier Quattro Fit transition failure. More completed jobs alone would not demonstrate useful feedback at the moving lens. Measure current-point coverage as well as GPU work, input cadence and settlement.
