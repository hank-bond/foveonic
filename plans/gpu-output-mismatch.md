---
summary: "Intermittent GPU output differences remain unresolved, with strict regression checks and retained investigation evidence."
---

# GPU output mismatch

## Current state

The cause and first divergent stage remain unknown. The owner initially deferred investigation after matched guarded/original-storage trials passed and normal use showed no issue. A later exact-output regression failed during monochrome optimization, and the owner approved investigation. Targeted and complete reruns passed, but this does not establish a fix.

Keep strict equality checks. Do not weaken tolerances or retry a failing comparison until it passes. Guarded storage is an ownership improvement, not an established mismatch fix.

## Established observations

- Identical source/settings have produced different full-resolution RGB floats. In one captured case, a second fresh render matched the cached result in full.
- Both original final buffers remained byte-identical across rereads. This argues against unstable final readback, not every possible measurement error.
- Regenerated continuation inputs matched afterward. The actual failing intermediate outputs were not captured, so no stage has been identified.
- Frozen builds failed with tone reuse disabled. Concurrent source edits and tone reuse are not necessary to trigger the issue.
- Four no-recycling six-original sequences passed, but disabling recycling changed memory lifetime and timing. No production scratch policy was changed.
- Guarded runs reported no unexpected ownership violation. Two paired guarded/original-storage trials passed the same 50 tests, with execution order reversed in the second pair. These observations do not establish a cause or failure-rate change.

Temporary fixture filters, repeated-render campaigns, capture hooks, failure-triggered rerenders, and event tracing were removed from maintained code. Guard regressions, binding-access audits, and bounded exact-comparison failures remain. Frozen sources, binaries, manifests, and raw evidence are retained locally. Earlier revisions preserve the detailed artifact inventory.

## Historical failures

Red-flower failures followed Shadows, Detail, and cancellation/resume edits. The Shadows comparison differed in 34,382,750 components, maximum 1.177125. Resume differed in 11,213,165 components, maximum 0.0087557435, initially near sensor coordinate (1229, 2444). CPU tone equality passed in one failing run.

A frozen turquoise Detail comparison differed in 23,467,527 components, maximum 0.005541146. Both final buffers were stable across rereads, and a second fresh render matched the cached result in full. All five subsequently regenerated continuation inputs matched. Tone reuse was disabled.

Interleaved passing and failing runs are historical observations, not a reliable estimate of failure rate.

## Recurrence during monochrome optimization

The normal six-original color regression failed on yellow flowers and stroller after Exposure step 5. It reported 35,681,789 differing components, maximum 1.1771249, and first values 0.5896426 / 0.588649. The similar maximum resembles the historical flower failure but does not establish an identical cause.

The changed histogram runs only in Monochrome. The failing case remained Color and did not dispatch it. Review found no missing exposure dependency in the tone key, and no color photographic shader changed during the optimization. This narrows the changed execution path, not the possible cause.

The unchanged strict six-original test then passed all edit sequences, retained-image checks, and cancellation/resume checks in about 124 seconds. A subsequent complete serial suite passed all 69 processor and eight GUI GPU tests. The failure log, passing logs, test executable, source snapshot, and hashes are retained locally. Failing intermediate buffers were not captured.

No tolerance, cache policy, scratch policy, processing fence, or retry behavior changed. The failure remains unresolved.

## Next diagnostic step

Keep a frozen copy of failing code and inputs. Review shader access alongside dispatch dimensions, parameters, and order. Capture stage outputs from the actual compared executions, then narrow compound operations to dispatches. Retention can prevent recycling, while diagnostic copies and fences change timing. Record those differences explicitly. No shader race, driver fault, cache-key error, or premature reuse has been established.
