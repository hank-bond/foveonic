---
summary: "Native slider correctness checks may reuse an application session while preserving independent case inputs and image assertions."
kind: reference
status: accepted
implementation: conforming
---

# Slider test sessions

The owner approved trying session reuse to reduce repeated application startup in the native slider matrix. The initial implementation is an explicit correctness-testing option. Fresh-launch checks remain available for comparison and for behavior that depends on process startup or exit.

Each declared case must retain its source identities, photographic recipes, control pointer, gesture, theme, processing quality, deadline, observations and image comparisons. Reusing a session must restore the case's declared starting state and wait for the corresponding photographs to settle before driving the control. A preceding case's edits, selection or pending work must not satisfy the next case's checks.

Cases must keep separate evidence directories and terminal results. A failed or missing case must prevent a successful group result. Interruption must stop the owned application and retain completed and incomplete evidence without turning either into a pass.

The first implementation should group slider cases by fixture, theme and processing quality. It must preserve the existing independent expected edits and fresh-render image comparisons. A representative pilot must compare reused sessions with separate launches before the release sweep uses session reuse.

Session reuse is a correctness workload. Its timings must not be compared with fresh-launch latency baselines. Native checks remain serial on a quiet host. Startup, shutdown, recovery, restart persistence and explicitly cold-process checks must retain their required process boundaries.

Test inputs follow the [reproducibility contract](./performance-diagnostics.md#reproducible-test-inputs). Coverage remains limited to declared cases and observed controls as described in the [acceptance inventory](../testing-coverage.md).
