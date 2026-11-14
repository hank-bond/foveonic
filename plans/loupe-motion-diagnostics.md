---
summary: "Attribute loupe-motion stuttering through sustained input and native presentation timing before changing scheduling."
---

# Loupe motion diagnostics

The owner reports stuttering during sustained loupe movement, especially on the hazy coast, after accepting the [layered presentation and registration work](./archived/layered-detail-presentation.md). Follow the [viewport requirements](../docs/specs/viewport-workflows.md) and [measurement contract](../docs/specs/performance-diagnostics.md). The diagnostic design below is proposed and has not been implemented. Agree its scope with the owner before implementation or scheduling changes.

## Findings

The loss-free owner capture contains UI updates with a median of about 1.6 ms and maximum of 8.4 ms. Some intervals between updates during held gestures exceed 100 ms, but existing input evidence records button presses rather than motion. These intervals alone cannot distinguish idle-held time from stalled movement. A native thread sample includes waits in Metal surface acquisition and the shared wgpu submission lock. Some surface waiting is normal display pacing, so this is evidence for further attribution rather than a confirmed bottleneck.

An external automated drag replay coalesced press and release into the same or adjacent updates and did not reproduce sustained motion. The in-process driver needs to keep the gesture active across frames rather than relying on this external replay.

An apparent modifier-selection hang is unconfirmed because investigation overlapped an intentional quit. A later delayed quit completed normally with its capture saved. The shutdown path explicitly waits for diagnostic saving. Keep that observation separate from the motion-stutter investigation.

## Proposed diagnostic slice

- [ ] Agree the diagnostic scope with the owner.
- [ ] Record pointer motion through the existing input diagnostics.
- [ ] Drive a fixed-duration path from monotonic elapsed time through the native raw-input hook, with press and release on separate frames. Compare motion within retained coverage with a sweep crossing section boundaries. Keep screenshot/readback outside measured intervals.
- [ ] Observe the existing surface-acquisition, queue-submission and presentation host intervals through one optional vendor callback, carrying viewport/pass identity. Use the existing Perfetto recorder without a vendor dependency on the image processor. The observer defaults to absent.
- [ ] Obtain independent largest-model review and compare the measured waits before choosing a scheduling change.

These measurements describe host waits and in-process input cadence, not physical presentation or operating-system input latency. If those three intervals do not explain the gaps, extend attribution to preparation and command encoding next.
