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

Temporary fixture filters, repeated-render campaigns, capture hooks, failure-triggered rerenders, and event tracing were removed from maintained code. Guard regressions, binding-access audits, and bounded exact-comparison failures remain.

## Historical failures

Red-flower failures followed Shadows, Detail, and cancellation/resume edits. The Shadows comparison differed in 34,382,750 components, maximum 1.177125. Resume differed in 11,213,165 components, maximum 0.0087557435, initially near sensor coordinate (1229, 2444). CPU tone equality passed in one failing run.

A frozen turquoise Detail comparison differed in 23,467,527 components, maximum 0.005541146. Both final buffers were stable across rereads, and a second fresh render matched the cached result in full. All five subsequently regenerated continuation inputs matched. Tone reuse was disabled.

Interleaved passing and failing runs are historical observations, not a reliable estimate of failure rate.

## Recurrence during monochrome optimization

The normal six-original color regression failed on yellow flowers and stroller after Exposure step 5. It reported 35,681,789 differing components, maximum 1.1771249, and first values 0.5896426 / 0.588649. The similar maximum resembles the historical flower failure but does not establish an identical cause.

The changed histogram runs only in Monochrome. The failing case remained Color and did not dispatch it. Review found no missing exposure dependency in the tone key, and no color photographic shader changed during the optimization. This narrows the changed execution path, not the possible cause.

The unchanged strict six-original test then passed all edit sequences, retained-image checks, and cancellation/resume checks in about 124 seconds. A subsequent complete serial suite passed all 69 processor and eight GUI GPU tests. Failing intermediate buffers were not captured.

No tolerance, cache policy, scratch policy, processing fence, or retry behavior changed. The failure remains unresolved.

## Recurrence during TIFF color and Auto sharpening checks

The full serial core suite for [TIFF color and Auto sharpening](./tiff-color-and-auto-sharpen.md) passed 71 ignored tests and failed the existing six-original cache-equality regression. Blue sign passed. Leaves failed at Detail step 2 (0.8), with explicit manual sharpening 0.75 and the default camera TIFF destination. The comparison reported 29,432,493 differing float components, maximum absolute difference 0.004963994, first component 12,011,991 with 0.09225379 versus 0.09225992.

The three new feature-specific GPU checks passed, including exact Auto/manual and cached/fresh comparisons, TIFF conversion/profile/preview boundaries, and zero-strength signed/headroom identity. All eight GUI ignored checks passed independently afterward. The failed core comparison was not retried or weakened. The first divergent intermediate was not captured. No cause or relationship to the new features is established.

## Recurrence during photographic resize validation

The full serial core GPU suite with the reference photographic output resizer passed 74 tests and failed `active_inputs_match_fresh_outputs_and_keep_retained_images_immutable` on turquoise at step 13. Blue sign, leaves, rocks, and red flower passed their complete sequences first. Step 13 reenables default reconstruction and explicitly disables ColorDQ after the earlier correction-disable and WB edits. It reported 2,890,022 differing float components, maximum absolute difference `0.0028624088`, first component `36,143,175` with `0.35032853` versus `0.35032818`.

The failed comparison reads full-resolution working floats before TIFF encoding, not resized TIFF pixels. All three new resize-specific GPU/export tests passed in that suite, including frozen reference samples and all destination spaces. The ordinary workspace suite, strict all-target Clippy, formatting, and documentation checks pass. The failing comparison was not retried or weakened. GUI ignored tests were not reached by the combined command after the core failure. Failing intermediate buffers were not captured. Concurrent TIFF/sharpening/toolbar work is present, and no relationship between the failure and these changes is established.

## Recurrence during color-field work

The processor fixture suite failed the unchanged strict stock-cache comparison on `sunlit-red-flower` step 9: 19,814,441 differing float components, maximum `0.03663957`, first values `0.59178257` / `0.5918086`. Its other 69 tests passed. This case selected Classic (named Reference in that build), not Color field. The frozen failing test binary and log are retained in the isolated worktree's ignored investigation directory.

One isolated run of the same test built from the pristine `ea4a44c` source archive passed. This single passing baseline does not clear the candidate failure or prove that it shares the historical cause. No failing comparison was repeatedly rerun until it passed, and no stock assertion, storage policy, or scratch lifetime was changed to hide it. The eight separate GUI GPU tests passed. The issue remains unresolved.

After the approved Classic/Local/Full N2 and CG precision changes, one current-candidate run of the unchanged strict test passed all six originals. No stock arithmetic or assertion changed. This passing recheck does not establish a fix or identify the earlier failure's cause.

## Localized differences during restraint review

A later four-scene color-restraint sweep recorded two fresh/cached Output outliers at nonzero settings. Each exceeds `1e-4` at exactly one oriented native pixel, outside the registered priority crops:

- 0170, Global restraint 1: maximum RGB difference `0.0019752979` at (130, 3059). The corresponding highlight-layer maximum is `0.0107421875` calibrated codes.
- 0095, Global restraint 0.25: maximum RGB difference `0.0405142903` at (1687, 1794). The corresponding highlight-layer maximum is `0.021484375` calibrated codes.

All eight retained layer/Output pairs remained byte-identical. Repeated/restored-zero samples and the other fresh/cached records showed only small numerical variation. The separate two-method restraint cache test passed, but does not invalidate these broader measurement outliers. No retry, fallback, relaxed assertion or cache-policy change was applied. A discontinuous downstream operation is a hypothesis, not an identified cause. Whether these localized differences share the earlier large-area mismatch is unknown. The [restraint plan](./color-field-restraint.md) keeps this acceptance limitation explicit.

## Strict failure during unified highlight integration

After the owner selected the consistency cutoff, the unchanged strict fresh/cached Output bound failed on 0095 with Local and cutoff 0.02. The highlight-layer maximum was `0.045898438` calibrated codes. The final Output maximum was `0.0011789203`, above the `0.0002` bound. Global cutoff 0.02/0.01/0.005 and restored A had passed earlier in that same test sequence. The failure occurred before the remaining Local cases completed.

The failing test binary, source archive, input-sidecar bytes, checksums and log were retained. The actual failing intermediate images were not captured, so the location, extent and first divergent downstream stage are unknown. Do not assume this is the same single-pixel mechanism seen during neutral-restraint review. The test was not rerun, skipped into an apparent pass or weakened. Eleven independent processor GPU tests and eight GUI GPU tests passed separately. Fresh-only convergence and photographic runs do not clear this cached comparison. The later positive-strength interface maps selectivity 0.25 to the same cutoff 0.02. That UI remapping does not clear the failure. The [unified-control plan](./archived/unified-highlight-control.md) retains this production-acceptance blocker.

## Main-integrated reproduction

The first full main-based color-field validation ran all 87 core GPU tests without exclusions. It passed 83 and failed the color-field Output comparison, two existing exact-cache regressions, and exact recipe replay. The separate GUI GPU suite passed all eight tests.

The Color Field Local/selectivity-0.25 failure was captured in one diagnostic using the original two requested outputs. Exactly one pixel exceeded the Output bound. Replaying the captured recovered layers through the unchanged downstream GPU graph reproduced both actual final outputs byte-for-byte. A `0.00390625`-code blue difference straddles a strict speckle-detection boundary, causing a discrete repair difference and a final maximum of `0.0011783242`. The [production plan](./color-field-production.md) records the actual values and scope. This establishes the first amplification for this case only, not a cause for the separate large-area exact-cache failures.

Unordered recovery compaction permits different CG reduction orders. Deterministic GPU ordering is a proposed numerical-reproducibility change, not an implemented fix. Neither the detector nor any acceptance assertion was changed. Temporary diagnostic hooks were removed after capture and replay.

## Deterministic Color Field resolution

The owner approved deterministic recovery ordering. Native and color unknowns now use stable integer GPU compaction instead of unordered atomic row allocation. No recovery equation, tolerance, cap or Classic detector changed. The formerly failing comparison now has zero difference in both recovered layers and Output. A broader regression completed 128 fresh/cached pairs over all sixteen originals, both methods, and four strength/reset steps. All highlight and final-image comparisons were byte-identical. Exact recipe replay also passed unchanged. The [determinism plan](./archived/color-field-determinism.md) records numerical acceptance and measured cost.

The subsequent full serial core run passed 88 of 89 tests. Its sole failure was the existing Classic recovery-threshold cached/fresh comparison, with 31,019,819 differing components and maximum `0.9444937`. The two other exact-cache comparisons that failed previously passed in this run, which does not clear their history. One pristine-main threshold control passed. The candidate failure remains unresolved and was not repeatedly retried. Its cause has not been connected to Color Field or the source-order change.

## Next diagnostic step

Keep a frozen copy of failing code and inputs. Review shader access alongside dispatch dimensions, parameters, and order. Capture stage outputs from the actual compared executions, then narrow compound operations to dispatches. Retention can prevent recycling, while diagnostic copies and fences change timing. Record those differences explicitly. No shader race, driver fault, cache-key error, or premature reuse has been established.
