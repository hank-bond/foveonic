---
summary: "Pilot native slider session reuse while retaining declared case isolation and independent photographic comparisons."
---

# Slider session reuse

Implement the agreed [test session requirements](../docs/specs/slider-test-sessions.md). The existing release candidate's serial validation continues while the pilot is prepared. Keep the owner's primary Mac available for separate rendering work and use the dedicated remote Mac for native checks.

- [x] Review a finite native case queue and per-case reset design with Sol.
- [x] Add an opt-in grouped runner and native session support without changing normal application behavior.
- [ ] Check malformed groups, distinct gesture identities, evidence ownership, interruption and failed-case propagation.
- [ ] Compare representative reused cases with separate launches using the same app, inputs and image comparisons.
- [ ] Review the implementation, record the actual runtime benefit and select the release validation mode.

The current native driver reads one manifest at application startup and closes after writing its result. Session reuse therefore requires a native automation change and a new app build. Existing release-package proofs retain their original binary identities and cannot be relabeled as evidence for that new binary.

The first same-binary pilot passed all seven standalone cases. The grouped run passed mixed Exposure and its reversal gesture, then failed to change Color Range Saturation. Its declared recipes, slider geometry and injected pointer coordinates matched the standalone run, but egui never reported dragging. The failed group and its later unrun cases remain unsuccessful. Retained pointer interaction is the current hypothesis. A session-only pointer reset and separate target-hover frame are ready for another build and pilot. This does not establish the cause or a runtime benefit yet.
