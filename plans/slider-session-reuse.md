---
summary: "Pilot native slider session reuse while retaining declared case isolation and independent photographic comparisons."
---

# Slider session reuse

Implement the agreed [test session requirements](../docs/specs/slider-test-sessions.md). The existing release candidate's serial validation continues while the pilot is prepared. Keep the owner's primary Mac available for separate rendering work and use the dedicated remote Mac for native checks.

- [x] Review a finite native case queue and per-case reset design with Sol.
- [x] Add an opt-in grouped runner and native session support without changing normal application behavior.
- [x] Check malformed groups, distinct gesture identities, evidence ownership, interruption and failed-case propagation.
- [x] Compare representative reused cases with separate launches using the same app, inputs and image comparisons.
- [x] Review the implementation, record the actual runtime benefit and select the release validation mode.
- [ ] Complete the packaged release acceptance sweep using the selected mode.

The previous native driver read one manifest at application startup and closed after writing its result. Session reuse therefore required a native automation change and a new app build. Existing release-package proofs retain their original binary identities and cannot be relabeled as evidence for that new binary.

The first same-binary pilot passed all seven standalone cases. The grouped run passed mixed Exposure and its reversal gesture, then failed to change Color Range Saturation. Its complete initial settings, slider geometry and injected pointer coordinates matched the standalone run, but egui never reported dragging. The failed group and its later unrun cases remain unsuccessful. A session-only pointer reset and separate target-hover frame corrected the subsequent pilot. The precise egui hit-testing mechanism was not established.

The updated pilot passed seven standalone and seven reused cases for Merrill HIGH in the dark theme at Quality. It included nested color controls, monochrome noise, equal Exposure and a repeat of the initial mixed Exposure after unrelated edits. Complete per-photo initial settings, final captured settings, geometry and raw image bytes matched across modes. Independent process observations confirmed seven standalone application processes and one reused process. Sol independently checked the raw bytes, source files, settings and process evidence.

The first standalone command took 39.4 seconds and the reused command took 23.3 seconds. These are descriptive whole-command durations, including staging and independent fresh renders, from one ordered pair. Cache and run-order effects were uncontrolled. They do not establish application latency or a general speedup. Select opt-in reuse for the broader release correctness matrix. Keep process-boundary checks and latency workloads in their existing modes.
