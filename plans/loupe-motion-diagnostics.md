---
summary: "Attribute loupe-motion stuttering through sustained input and native presentation timing before changing scheduling."
---

# Loupe motion diagnostics

The owner reports stuttering during sustained loupe movement, especially on the hazy coast, after accepting the [layered presentation and registration work](./archived/layered-detail-presentation.md). Follow the [viewport requirements](../docs/specs/viewport-workflows.md) and [measurement contract](../docs/specs/performance-diagnostics.md). The owner approved the diagnostic slice below. Scheduling and GPU-completion behavior remain unchanged.

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

## Next experiment

- [ ] Agree the completion-wait experiment with the owner.
- [ ] Preserve existing completed-stage, cancellation, publication and resource-retirement boundaries. Register a finite completion callback, use nonblocking polling, and wait briefly outside wgpu between polls through one shared helper. Avoid global queue-empty waiting, which could chase ongoing UI submissions.
- [ ] Compare all three presentation stages, motion cadence and detail settlement before claiming a visual fix. A shared GPU queue can still delay drawing behind already-submitted compute work after the host lock stall is removed.

Independent review supports this narrow experiment. It introduces no separate polling thread or scheduler abstraction. Polling cadence may add per-stage overhead and must be measured. The experiment remains unimplemented.
