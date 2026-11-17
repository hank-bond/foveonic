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

Session reuse is a correctness workload. Its timings must not be compared with fresh-launch latency baselines. The owner approved bounded concurrent correctness groups to reduce release wall time. Performance remains exclusive on a quiet host. Startup, shutdown, recovery, restart persistence and explicitly cold-process checks must retain their required process boundaries.

Concurrent groups must preserve the same complete case set and assertions as serial execution. Each group must own separate preferences, staged sidecars, cache namespace and evidence. Declare all expected identities before launch. Missing, duplicate, failed, guarded, aborted and unrun cases must remain distinguishable and must prevent a complete passing result. The coordinator alone combines results after checking native/session/image evidence and declared source, recipe, binary and renderer identities.

Concurrency must require explicit worker and memory limits, background windows, Off mode, reused sessions, fresh image verification and workload monitoring. Reject incompatible requests before creating inputs or starting children. Keep strict quiet-host behavior when no correctness cohort is authorized. A cohort may recognize only exact declared application/renderer processes inside registered live worker sessions. Verify process birth and ownership when authorizing peers or cleaning children. Never exempt unrelated workloads by executable name or disable monitoring globally.

Reserve resources for each active group through native execution and independent verification. Account for wrapper/operating-system headroom. Sampled process guards must be identified as operational safety limits rather than guaranteed physical-memory ceilings. A reservation or guard overrun must not become a test pass or silently raise its limit. Unknown failures must not receive automatic retries.

Concurrent groups may partition a fixture, theme and processing policy by a declared case-family memory guard. Global command-line guards take precedence over case-family hints and fixture defaults. Validate hints before staging and require every member of a group to resolve to the same guard. Keep case identities, recipes, comparisons and relative order within each resource class. Additional process boundaries and ordering across resource classes must be explicit; they do not establish latency. Admission may prioritize lighter groups without changing the declared result inventory.

A reused session must also have a bounded parent-side progress deadline for the active case. Renew it only after the next complete terminal matches the declared session, case, index, count and actual process. Missing, partial, duplicated or out-of-order evidence must not extend a deadline. Keep individual budgets and the total session bound, including orderly exit, while preserving failed native results. A hung UI must not inherit the entire sum of every pending case's budget.

On interruption, a surviving controller must stop new admission, retain completed and incomplete records, terminate only owned work with bounded escalation, and reap workers. Workers must check controller identity while waiting and before launching another child. After controller loss, surviving workers must clean and reap their owned children and exit. Unverifiable ownership or incomplete cleanup must fail the run and prevent further work. Concurrent correctness results must always state that their timings are invalid for performance comparisons.

Successful raw image samples may be pruned explicitly after their producers exit. Recheck complete native, intended-edit, build, source, settings, frame, geometry and full-byte identities before deleting anything. Finalize the case report first and preserve its immutable identity in a separate retention receipt. Preserve JSON evidence, action logs, failure images, original photographs and numerical baselines. Failed, missing, changed or incompletely verified samples must remain. Retention failures must block success. Resource-accounting permissions must not be used as evidence of process ownership or death.

Test inputs follow the [reproducibility contract](./performance-diagnostics.md#reproducible-test-inputs). Coverage remains limited to declared cases and observed controls as described in the [acceptance inventory](../testing-coverage.md).
