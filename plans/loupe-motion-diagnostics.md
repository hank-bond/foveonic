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

## Section-to-Whole follow-up

The owner asked whether a structural reuse improvement would be more impactful and how long Whole completion takes after a section. The current `profile_smart_preview` route measures a central 992 by 1002 section followed immediately by Whole in the same session and settings, after a Fast overview. Three post-warmup trials alternate this route with direct Whole. Original decoding is complete before timing. Results include completed processing and texture preparation on Apple M1 Pro, with the engine's existing local checkpoint limit. They exclude application queuing, concurrent window drawing and physical presentation. The example's final blocking completion poll is suitable for this isolated measurement, not a reproduction of the UI completion path.

| Recipe | First section median, ms | Additional Whole median / range, ms | Direct Whole after overview median, ms |
| --- | --- | --- | --- |
| Hazy coast, saved Natural / Universal reconstruction settings | 826.7 | 596.7 / 587.8–598.6 | 884.3 |
| sd Quattro, capture defaults | 630.3 | 372.6 / 372.2–374.0 | 992.0 |

The Hazy diagnostic repeats roughly 81 ms of noise bilateral base, 44 ms of pattern min/max and 35–39 ms of pattern/column filtering in the Whole continuation. This recipe does not use the separate `NoiseContext` branch. Whole reuses `banding.layers` and skips earlier nodes. Packed atlas allocations have identical sizes across the two requests, but carry different requested views in their identities and are not all selected for retention. The cache also approaches its 1 GiB local limit. These observations do not yet prove equivalent numerical outputs or distinguish every non-retention reason.

A structural investigation should verify tile windows, parameters, dependencies and actual samples before separating reusable packed-data identity from requested presentation views. It must also examine retention of an expensive reconstruction boundary, since a matching identity cannot help an output that was not retained. An eventual Whole Detail is the simpler policy experiment: existing coverage logic would make later loupe movement display-only once it completes, with an upfront GPU and memory cost. Neither change is approved by this measurement alone.

## Why the measured section remains expensive

Dehaze is disabled in the Hazy recipe. Natural's spatial brightness guide requires 986 native pixels of support per side, with another four for finishing. The 992 by 1002 visible section maps to sensor bounds `[1920, 1103, 2912, 2105]`. Expanding by 990 and aligning the guide lattice produces `[928, 112, 3904, 3096]`, or 2976 by 2984 pixels, exactly matching the recorded reconstructed RGB buffer.

Merrill reconstruction uses a fixed 4 by 4 native tile grid with 1232 by 816 pixel cores on this source. That expanded rectangle and seam support select all 16 tiles. The resulting RGB atlas occupies 246,497,280 bytes for both section and Whole. The diagnostic sums show noise-base filtering at 153.1 versus 154.3 ms, column conditioning at 48.0 versus 49.3 ms, and pattern conditioning at 102.5 versus 102.7 ms. This explains the first section's large cost through legitimate support expansion and tile granularity, rather than demonstrating missing regional implementations or a cache defect.

An isolated control changes only Natural to the camera base response, leaving noise, highlights, banding, exposure and Dehaze settings intact. Across three post-warmup Off trials, first-section median falls from 826.7 to 366.4 ms. The noise and column atlases shrink to 61,624,320 bytes, selecting four of sixteen tiles. Diagnostic noise-base filtering falls to 35.5 ms, column conditioning to 12.5 ms and pattern conditioning to 24.9 ms. This is a causal measurement, not a proposed change to the owner's photographic settings.

Some first-use work remains whole-sensor even in that control, notably the recorded highlight restoration and camera repair. Row correction gains regional storage when its dependencies permit it. Adaptive's small whole-photo estimator is separate from its broad spatial application guide. Later output tone and texture preparation benefit strongly from the small displayed area, but are minor costs here. Cache identity and retention may still improve subsequent requests. They do not explain why the current first request legitimately selects the complete native tile grid.

## Small-guide application prototype

The owner approved testing the existing small whole-photo Adaptive Tone analysis guide as the application guide for native sections. Keep the estimator, fitted amounts and native RGB fine structure probes unchanged. Preserve Whole and fixed-resolution renders exactly. Regional appearance is an approximation to review through photographs, including dark detail and hard edges.

- [x] Declare camera-owned RGB-to-native sampling geometry. Compose actual pooled and Q16 crop mappings at the shared application boundary.
- [x] Sample the compact analysis guide with bilinear interpolation and valid whole-photo edge clamping. Keep the native guide's integer lookup unchanged.
- [x] Reduce native RGB support to the local fine probe and gain interpolation. Retain independent support for other enabled spatial operations.
- [x] Verify mapping, unchanged fit and overlapping sections, including padded photo edges and quarter turns.
- [x] Compare exact Whole and fixed-resolution results across the maintained camera matrix.
- [x] Measure first-section and Whole continuation latency, then prepare photographic comparisons for owner review.
- [x] Complete independent code review and make the prototype available for owner testing.

The prototype carries sample geometry as metadata only. It adds no new background work, scheduler, cache policy or authored setting. Merrill's analysis remains on its padded sensor grid. Quattro's analysis uses its actual fixed-point crop mapping, which must not be replaced by a nominal divisor alone.

Final sampler validation preserves all 40 Natural renders exactly across the maintained ten-photo Full, Fast, Draft and Thumb matrix. Focused GPU checks preserve fitted amounts and overlapping regional pixels, reject inactive-guide-padding influence and require mapped identity sampling to match the original nonconstant compact-guide lookup exactly. The half-pixel block center must be transformed before the affine, rather than added in the guide lattice. The existing Merrill and Quattro analysis replay checks exercise Natural and Strong in Color and Monochrome across working scales.

On the same resident-source, Fast-overview setup, three post-warmup hazy-coast trials give a first-section median of 384.2 ms, versus the earlier 826.7 ms. Additional Whole completion is 641.8 ms, versus 596.7 ms. The median section-plus-Whole total is 1025.9 ms. These are isolated engine processing and texture-preparation measurements, not application request-to-presentation latency or proof that motion stuttering is eliminated.

Seven photographic comparisons cover hazy coast at center and photo edge, the dark speaker, the parking sign, sd Quattro and both flower cases. The flower comparisons use Strong at minus 1.5 EV. Mean absolute sRGB display-channel differences from Whole are 0.21–0.85 percentage points. The dark speaker has a 7.74-point 99th percentile and a 35.22-point maximum, so the mean is not a per-pixel equivalence claim. Full RGB detail remains native, but changed coarse spatial guidance can alter individual dark pixels. The owner must judge this approximation before acceptance.

The diagnostic example now supports explicit render-only named-output requests and records that route in session metadata. Its existing default Whole render and TIFF behavior are unchanged. This lets regional numerical comparisons use the interactive request boundary without changing the convenience API's older Whole-only validation or export policy. Independent source review approved the implementation after edge clamping and center-phase corrections. Owner visual acceptance remains outstanding.

Final checks pass 558 ordinary engine tests, both focused mapped-guide GPU tests, existing Merrill and Quattro working-grid replays, strict engine all-target Clippy, formatting and documentation indexes. The current GUI is rebuilt from the reviewed source. The prototype is uncommitted pending owner visual review.

## Separating resolution from sectional guidance

Owner review identifies a visible contrast difference in the hazy-coast label and scalloped metal. A smaller display averaging bright and dark texture is insufficient to explain the smooth label-background difference. Draft and Smart loupe observations must be distinguished from comparisons between the loupe and its reduced overview.

The opt-in [view-attribution diagnostic](../crates/foveonic/src/render/recipes/adaptive_views_test.rs) holds authored settings fixed and captures seven variants: Whole Full, Fast and Draft with their existing application guides, the same three working grids rebound to the shared analysis guide, and an actual native section with shared guidance. It preserves the estimator and rebinding updates both execution inputs and graph dependencies. Captures retain only a common photographic window with explicit sampling maps. CPU display comparisons follow the GUI geometry in f64 and do not claim bit-exact WGSL sampling arithmetic. The diagnostic asserts supported regional processing and neutral authored framing.

On the saved Natural hazy-coast recipe at zero exposure, the 1250 by 1550 metal/label window differs between normal Whole Full and the current native section by 2.65 mean display-RGB percentage points, with a 17.67-point 99th percentile. Whole Full rebound to the shared guide produces exactly the same stored native samples as the region over this window. The spatial application guide therefore explains this measured difference without invoking regional boundary errors or Fast resampling. Averaging into 32-pixel cells still leaves 1.04 mean display-channel percentage points of difference, confirming a broader tonal effect beyond fine texture.

The saved Strong dog recipe has a much smaller guide-only difference of 0.21 mean points and 0.87 points at the 99th percentile in its center window. Shared-guide Whole and section again have byte-identical overlapping native samples. This comparison changes both scene and profile between photographs and does not establish a camera-family cause.

Source inspection confirms that Draft and Smart loupe requests both require Full native samples. Loupe coverage can also be satisfied by an existing Whole native image. The prototype's Whole and section paths currently use different application guides while presentation considers both compatible native detail. That permits two different native appearances to be used under reuse and delivery conditions. The provenance of the owner's particular UI observation remains unverified because no capture of that run is available. A controlled UI replay is needed before attributing its mode dependence to a specific cached frame.

Candidate next experiments, not selected defaults, are a finer independent whole-photo application guide at Fast or Draft scale, native-guide precomputation and retention for exact appearance, or local edge-aware refinement of the small guide. Keep the estimator separate and keep Whole Full output unchanged unless the owner approves an appearance change. The current diagnostic slice changes no app processing or preview policy.

Independent review approved the attribution harness after adding a supported-regional-processing assertion. Final captures for both photographs pass all seven variants and reproduce the original diagnostic patches exactly. Strict engine Clippy, 558 ordinary tests, formatting and documentation checks pass. The diagnostic adds no production rendering changes. The two Native shared-guide comparisons were also checked directly on stored float rows, independently of CPU display resampling.

## Finer application-guide comparison

The owner approved testing independent Draft-sized and Fast-sized application guides against unchanged Whole Full, with the estimator, fitted amounts and authored controls held fixed. Compare the two recipe targets in separate builds. Keep the native section's small local support and share pre-curve RGB preparation between the estimator and application branches. Test-only graph mutations are unnecessary for the actual candidate recipe.

- [x] Compare both guide sizes on the coast metal/label and dog, including common native windows.
- [x] Measure completed first-section latency after Fast overview in fresh sessions with resident decoded source. Record retained checkpoint bytes and held image/texture references separately, outside timing.
- [x] Verify analysis and Whole/fixed-resolution preservation, complete independent review, and make the Fast-sized candidate available for owner inspection.

The existing Smart profiler gains scalar session-checkpoint and image/texture reference byte observations after the completion boundary. These are not peak GPU residency and can overlap across owners. No new application setting or authored recipe field is introduced.

The regional application guide now has an independent fixed target for the candidate. Its RGB preparation shares the estimator's pre-curve helper, preserving the original analysis aliases and inputs. The application guide filters its direct RGB parent, avoiding an extra alias that would change numerical ancestry and prevent reuse of an identical overview guide. The guide mask is retained, without separately retaining its RGB solely for this role. Full and explicit working-grid choices keep their existing guide path.

| Application target | Coast mean display difference from Whole Full, points | Coast first section, shared / separate caches, ms | Dog first section, shared / separate caches, ms |
| --- | --- | --- | --- |
| Existing estimator grid | 2.65 | 391 / 436 | 796 / 950 |
| Draft grid | 1.74 | 456 / 501 | 788 / 954 |
| Fast grid | 0.81 | 390 / 609 | 806 / 1165 |

These are medians of three completed trials after one warmup, with a resident decoded source and an initial Fast overview. The first-detail timing window remains the central 992 by 1002 section. Photographic quality uses the larger metal/label window documented above. The dog uses its saved Strong recipe and the coast its saved Natural recipe, so the existing estimator-grid baseline differs between these photographs. Dog mean display error improves from 0.21 to 0.11 points with the Fast guide. Coast's Fast-guide 99th-percentile difference is still 7.39 points. A lower mean is not a claim of imperceptible local differences.

The profiler now holds image and texture owners through continuation and can place native work in an independent session while retaining the overview session. The updated baseline uses these same lifetimes. The split model shares decoded data but is not the GUI's complete ReducedSession, shared working-set or delivery path. It bounds one important cache-reuse condition. The shared-cache Fast candidate skips its application guide in the node diagnostics, confirming reuse. That benefit cannot be assumed for separate native caches.

With separate caches, the Fast guide raises native checkpoint retention from about 535 to 587 MB on coast and 804 to 871 MB on dog, using 1048576-byte reporting units. These figures exclude other owners and are not peak application residency. Retained overview checkpoints and image/texture references are recorded separately. Reference owners can overlap checkpoints and must not simply be summed.

The Fast-sized candidate is available for owner evaluation, not an established latency improvement or accepted production policy. Compared with the original small guide, it improves fidelity but costs roughly 40 percent more first-section time on coast and 23 percent more on dog in the separate-cache model. Independent review found no new source blocker. The final 40-output matrix matches the preserved stored-byte SHA-256 hashes and geometry exactly. Merrill and Quattro analysis replay tests, 558 ordinary engine tests, strict Clippy, formatting and documentation checks pass. No authored settings or estimator tuning changed. The candidate remains uncommitted pending visual review.

## Owner follow-up and 400 percent loupe

The owner reports that repeated Smart loupe use on hazy coast is consistently below 200 ms and sufficiently responsive. Keep that warm interactive observation separate from the controlled first-section measurements above. The owner requested 400 percent magnification in the existing loupe menu. It enlarges native pixels through the existing lens geometry without promoting processing beyond Full.

- [x] Add the 400 percent choice through the existing zoom catalog, persistence and geometry paths.
- [x] Run existing magnification/DPI and persistence checks, complete independent review and reopen the app for owner testing.

The owner subsequently requested an Extra Large lens. It extends the existing size catalog to a 400-point diameter after Small 160, Medium 240 and Large 320. Independent review found no issue. All sixteen focused loupe checks pass, including the catalog-driven geometry and persistence paths. Strict GUI Clippy, formatting and documentation checks pass. The review app is reopened with Extra Large and 400 percent choices and the unchanged Fast-guide fallback.

The remaining appearance investigation should examine scale-dependent behavior inside brightness-guide construction before choosing another costlier grid. The filter uses local EV variance and residual-limited edge coefficients, not a simple resampled scalar image. Scaling also changes its multiscale cascade. A normalization or scale-policy explanation must be distinguished from unavoidable loss of input detail. No further guide-algorithm change is approved by this observation alone.

Read-only guide review confirms resolution-dependent algorithm changes beyond loss of input detail. Full's approximate physical filter radii are 2, 6, 12, 24, 48, 96, 192 and 384 native pixels. Fast's are 4, 12, 24, 48, 96, 192 and 384. The fixed coarse-grid threshold is in working guide cells and changes physical meaning: at radius 48, Full uses an 8-native-pixel signal/coefficient grid while Fast uses a 4-native-pixel grid. RGB averaging precedes logarithms, changing local EV means and variances. Residual limiting and edge coefficients are nonlinear, and stages compose through `a * previous_base + b`. These facts establish a scale-dependent filter, but their individual contributions to the observed contrast shift remain unmeasured. No obvious missing scale factor was found in epsilon, which is expressed in EV squared.

In the separate GPU diagnostic, coast's additional Fast application branch totals about 125 ms of nested GPU work across 15 executed nodes. The guide filter itself accounts for about 22 ms. Noise base, highlights, pattern, banding and column preparation account for much of the rest. These instrumented figures explain the extra pre-guide RGB work but must not be summed with the independent Off-mode first-section measurements. The next candidate experiment should isolate physical scale/level policy before assuming a larger guide is necessary. It remains a proposed experiment, not an implemented change.

## Lower-resolution guide algorithm investigation

The owner approved testing whether a smaller application guide can approach the current Fast-guide appearance. Keep the Fast-guide candidate as the fallback. Whole Full, the brightness estimator and authored settings remain unchanged. First isolate coarse-grid physical sampling, then examine the cascade only if the first comparison leaves a useful question. Compare the same native photographic windows and separately time completed processing. Retain a smaller candidate only when its appearance and latency justify the added code.

- [x] Compare physically normalized coarse sampling at Fast and Draft against existing guides.
- [x] Review any remaining cascade or sampling effect and run a bounded follow-up where useful.
- [x] Review the findings and implementation independently, remove rejected experiments, and make the chosen build available for owner testing.

The first experiment keeps the original cascade while matching its coarse lattice and threshold to native-pixel units where representable. Fast uses a four-working-pixel coarse sample instead of eight. Draft's coarse sample becomes identical to its two-working-pixel fine sample. The estimator remains unchanged. When application and estimator scales coincide, the experiment constructs a distinct application guide so it actually tests the altered filter instead of reusing the unchanged estimator guide.

| Regional application guide | Coast mean / p99 display difference from Whole Full, points | Dog mean / p99, points |
| --- | --- | --- |
| Existing Fast | 0.808 / 7.39 | 0.110 / 0.512 |
| Fast, normalized coarse lattice | 0.687 / 7.16 | 0.062 / 0.550 |
| Draft, normalized coarse lattice | 1.589 / 15.11 | 0.129 / 0.985 |
| Draft, normalized lattice and quantized cascade | 1.681 / 15.23 | 0.139 / 1.074 |

The cascade follow-up rounds native stage radii into the working guide grid and removes adjacent duplicates. This adds a radius-two stage rather than repeating a minimum-radius stage. Repeating identical affine filters would strengthen smoothing, not recreate an unavailable subpixel neighborhood. The added stage worsens both photographs. At Fast it also worsens the normalized-lattice results to 0.770 mean points on coast and 0.074 on dog. Whole native patch hashes remain identical across all variants. The same saved settings, physical windows, color conversion and display statistics described above apply.

Three post-warmup trials with resident decoded sources and independent overview/native sessions give first-section medians of 651 / 1241 ms for existing Fast on coast / dog, 621 / 1253 ms for normalized Fast and 533 / 1092 ms for normalized Draft. This measures the completed central 992 by 1002 section and texture preparation, not GUI delivery or warm native-section interaction. The quality window remains the larger label/metal region on coast. Runs are isolated from app/compiler workloads and bulk cleanup. Short sequential trials do not establish a precise speedup for the small Fast timing differences. Normalization also prevents reuse of an ordinary Fast-overview guide, a disadvantage not measured by this independent-session setup.

Keep the existing Fast application guide. Draft still shows a stronger label/metal contrast difference, and the modest normalized-Fast mean improvement does not justify another filter policy, more coarse-grid work and lost overview-guide reuse. Independent review agrees. The tested policies failed to justify replacement. This does not prove that every remaining discrepancy is unavoidable resolution loss. All algorithm experiments and experimental environment controls have been removed from source, restoring the pre-experiment engine files exactly.

The [Fast Guided Filter paper](https://arxiv.org/abs/1505.00996) computes coefficients at reduced resolution and applies them to original-resolution guidance. That is a useful later alternative to upsampling an already-filtered scalar base. Our accumulated cascade is a different filter, so this paper is inspiration rather than evidence that substituting its method would preserve our appearance. A coefficient-based path or native-mask downsampling diagnostic would be a separate investigation.

## Separate reduced Dynamic Range guide

The owner approved comparing a smaller independent Dynamic Range guide with the existing native guide. Unlike Adaptive's pre-adjustment guide, this guide describes the input after Adaptive, perceptual edits and Color Editor. Preserve that ordering, native local brightness measurements and Whole Full output. Start with a fixed Fast target for regional native rendering. Measure first-section preparation separately from retained-guide interaction, with active Dynamic Range controls and identical settings across candidates.

- [x] Capture active-control native-guide baselines and prototype independent reduced guidance.
- [x] Compare native sections with Whole output, verify geometry and fixed-mode output, and measure preparation/reuse costs.
- [x] Obtain independent design review, remove rejected paths and open the chosen build for owner review.

The candidate constructs its own Fast guide through the existing camera recipe, stopping at Dynamic Range's mask inspection before applying the controls. This includes the preceding Adaptive, perceptual and Color Editor edits. The unused inspection expansion is not an execution dependency. Its compact guide is retained under the existing session budget and shared through stable numerical ancestry. There is no new cache owner or scheduler. Native RGB requires only two pixels of gain/interpolation support. Tonal support is now owned by the shared edit-support calculation rather than separately added in Merrill and Quattro.

Adaptive and Dynamic Range share one guide-sampling POD and shader coordinate helper. The sampler owns guide extent, valid bounds and the RGB-to-guide affine. It maps the native block center before changing grids, clamps inactive padding and adjusts the input origin when graph storage becomes compact. Native guide application keeps its original integer lookup and gain arithmetic. A separate guide-geometry uniform preserves the 64-byte gain-application uniform already shared with Adaptive.

The photographic comparison uses coast Natural and dog Strong at zero exposure, with Shadows +0.5, Highlights -0.5, Blacks +0.25 and Whites -0.25. Changing only Dynamic Range's guide adds 0.804 mean display-RGB points on coast, with a 4.48-point p99 and 17.02-point maximum. Dog changes by 0.216 mean points, with 0.907-point p99 and 4.44-point maximum. Those local differences require owner review. Relative to Whole Full, coast's complete regional difference increases from 0.634 to 1.317 mean points. Dog increases from 0.082 to 0.296. The existing Adaptive approximation is present in both regional candidates.

| Completed section measurement | Native DR guide | Separate Fast DR guide |
| --- | --- | --- |
| Coast first section, ms | 1032 | 648 |
| Dog first section, ms | 1364 | 1213 |
| Coast warm exposure edits, ms | 7.0 | 7.8 |
| Dog warm exposure edits, ms | 134.9 | 66.8 |

First-section figures are medians of three post-warmup trials with resident decoded sources and independent overview/native sessions. Warm edits alternate exposure between zero and +0.5 over six measured trials after priming and two warmups, retaining the preceding displayed artifacts. They use one session and include completed section processing and texture preparation without Whole continuation. These are different cache conditions, not interchangeable app latency estimates. Neither includes physical GUI presentation. Runs are isolated from other app/compiler work.

Native-session checkpoint retention changes from about 1012 to 698 MB on coast and 977 to 1000 MB on dog, using 1048576-byte reporting units. This is logical checkpoint retention, not peak application memory. The smaller native neighborhood saves substantial storage on coast, while preparing another reduced branch slightly increases retention on dog.

All forty fixed-output comparisons preserve their frozen bytes and geometry across three Classic, three Merrill and three Quattro photographs plus one Tiger, at Full, Fast, Draft and Thumb. Active-control Full/Fast/Draft photographic captures also remain exact on coast and dog. Shared-sampler extraction preserves every captured candidate patch exactly. GPU tests cover identity, scaled fractional mapping, odd valid bounds, padding and overlapping sections. The Merrill fixture test keeps exact Whole fresh/cached and regional fresh/cached checks, and compares overlapping sections with the same guide at its existing 0.00001 tolerance. Whole-versus-regional guide equivalence is now the separately measured appearance approximation, not a widened geometry tolerance.

Independent review found no blocking design issue. The moving-section fixture requires a guide build on the first request and no rebuild on subsequent sections. It passes on coast and dog in Color and Monochrome. All 559 ordinary engine tests pass, as do strict engine and GUI all-target Clippy, formatting and documentation checks. The candidate remains uncommitted pending owner photographic acceptance.

## Checkpoint and preview-policy migration

The owner reported a Smart-only Whites microcontrast flicker at 200 percent on hazy coast with both Natural and Disabled Adaptive. The current section and Whole continuation select different application-guide resolutions because selection is inferred from regional bounds. This permits a same-edit appearance transition when the Whole image replaces its section. The checkpoint preserves that known limitation for recovery. The owner subsequently authorized committing the current work, then autonomously replacing the five preview modes with Quality and Speed through an explicit consistent processing policy. Owner visual acceptance will follow that migration rather than blocking each implementation slice. The spatial-view diagnostic now also supports Dynamic Range-only captures with Adaptive disabled.
