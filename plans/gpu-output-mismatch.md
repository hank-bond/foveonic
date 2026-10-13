---
summary: "Deferred intermittent fresh-render differences retain investigation evidence without claiming a known cause or fix."
---

# GPU output mismatch

## Decision

The owner deferred this investigation after guarded and original-storage builds both passed matched trials and no issue was observed in normal use. The cause and first divergent stage remain unknown. The guards are retained as an ownership improvement, not an established fix. Reopen if the normal exact-output regression fails or the owner observes an issue; do not weaken its assertions.

Temporary fixture filters, repeated-render campaigns, raw-root capture hooks, failure-triggered rerenders and event tracing have been removed from maintained code. Permanent guard regressions, shader binding-access audits and bounded exact-comparison failures remain. Frozen experimental sources/binaries, logs and photographic evidence are preserved outside maintained source.

## Findings

- Same source/settings can yield different final full-resolution RGB floats. The most informative failure occurred in a fresh render; another fresh render matched the cached result in full.
- Both original final GPU buffers read back byte-identically before and after the repeated render. This argues against an unstable final readback, not every possible measurement error.
- Regenerated cached-boundary roots matched afterward; outputs from the actual failing execution were not captured, so no stage has been identified.
- Failures occurred in frozen builds and with tone reuse disabled. Concurrent source edits and tone reuse are therefore not necessary to trigger them.
- Disabling scratch reuse passed four six-original sequences, but also changed memory lifetime/timing. Guarded runs reported no unexpected ownership violations. Neither result proves a recycling bug or a fix.

## Retained evidence

Initial red-flower failures followed a Shadows edit, a Detail edit and cancellation/resume. `/tmp/foveonic-ui-followup-gpu.log`, `/tmp/foveonic-ui-followup-gpu-2.log` and `/tmp/foveonic-ui-flower-isolation-5.log` are retained. The shadow failure differed in 34382750 components (71.25%), maximum absolute float difference 1.177125. The resume failure differed in 11213165 components (23.24%), maximum 0.0087557435, initially near sensor coordinate (1229,2444). Bounded recovered statistics are in `/tmp/foveonic-ui-{first,flower}-panic-difference.log`; the original large panic logs remain intact.

Flower-only runs 1–4 passed, run 5 failed, and runs 6–19 passed. Forty fresh and sixty cached fixed-configuration repeats passed in `/tmp/foveonic-ui-fresh-cached-flower-repeat.log`. A later diagnostic suite passed in `/tmp/foveonic-ui-cache-diagnostic-suite.log`. These are historical observations, not a reliable failure-rate estimate.

Frozen `d686266` plus diagnostic/tone overlays: `/tmp/foveonic-mismatch-isolation/{regenerated,reused}`, with SHA-256 overlay manifest and separate build directories. Ten alternating flower-only sequences per variant passed. `/tmp/foveonic-mismatch-regenerated-full-1.log` passed 51 tests; the reused suite failed on flower white-balance step 14, 39531465 differing components and maximum 0.8227475. Sample values reversed on a repeated fresh render, but that probe did not establish full equality against the cached result.

Frozen readback probes: `/tmp/foveonic-mismatch-readback/{regenerated,reused}`. `/tmp/foveonic-readback-reused-full-1.log` passed 51 tests. `/tmp/foveonic-readback-regenerated-full-1.log` failed on turquoise still life, Detail step 2: 23467527 differing components (48.63%), maximum 0.005541146. Both buffers were stable across rereads; a second fresh render matched the cached result in full. All five retained continuation inputs matched subsequently regenerated roots. This variant had tone reuse disabled.

`/tmp/foveonic-mismatch-readback/no-scratch` disables scratch recycling and retains released allocations until the execution ends. `/tmp/foveonic-no-scratch-full-1.log` passed its equality case and 49 other tests, but intentionally failed the existing assertion that scratch reuse occurs. Three further six-original comparisons passed in `/tmp/foveonic-no-scratch-equality-{2,3,4}.log`. No production scratch policy was changed by that experiment.

Guarded migration and repeat evidence are recorded in [guarded GPU storage](./guarded-gpu-storage.md). Four guard-only suites passed, with 53 tests each. Their test workload included three new guard tests and shader binding-access analysis, so they are not a controlled guards-only comparison with earlier unguarded runs.

## Matched comparison

`/tmp/foveonic-matched-guards/{original,guarded}` and its source/binary SHA-256 manifests preserve the matched experiment. Tone reuse, extra guard-test workload and shader-access auditing are absent from both variants. The same 50 ignored core tests run in the same order, with identical bounded readback probes and representation-only test adaptations. Both retain the original image in the separate repeated-render test. No photographic shader or production fence changed.

Two paired trials passed in both variants, reversing execution order in the second pair. Logs: `/tmp/foveonic-matched-{original,guarded}-{1,2}.log`; exact-output evidence: `.local/ui-responsiveness/matched-{original,guarded}-{1,2}`. Executables were copied and launched directly, without rebuilding between trials. These passes do not establish that guards changed the outcome. Any future claim about failure rate needs more comparable observations; performance numbers from these correctness runs are not acceptance measurements.

## If reopened

First freeze the failing code and inputs. Review shader access alongside host dispatch dimensions, parameters and order. Capture graph-stage outputs from the actual compared executions if needed, then narrow compound operations to dispatches. Diagnostic retention can prevent recycling, and diagnostic copies/fences change timing; record those differences explicitly. No shader race, driver fault, cache-key error or premature reuse has been established.
