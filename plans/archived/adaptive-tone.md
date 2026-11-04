---
summary: "Integrate the reviewed regional base responses across camera architectures with shared app limits and normal rendering/export ownership."
status: archived
---

# Adaptive tone integration

Implement the owner-selected [adaptive tone requirements](../../docs/specs/adaptive-tone.md). The owner will tune limits over many photos before choosing final values. Natural and Strong now need separate shared limits and an exposure-aware fit. Keep Camera default and replace the rejected fixed curve.

- [x] Promote the regional estimator to a GPU node with explicit parameters and no mandatory host readback.
- [x] Add typed guide/application operations and compose the whole-photo Merrill boundary before artistic edits.
- [x] Replace the fixed curve choice and carry the profile through settings, Tone scope, bypass, CLI and TIFF/DNG.
- [x] Add shared app tuning limits and snapshot them into active, preview and batch/export requests without photo persistence.
- [x] Use existing graph retention for unchanged RGB and guide, with exact upstream/curve/limit invalidation.
- [x] Validate defaults, cache safety, supported preview policy, Color/Monochrome and actual exports.
- [x] Build, inspect and open the local app for owner tuning.
- [x] Review owner feedback and give each profile separate limits.
- [x] Diagnose the reported darker spider result when Natural's shadow cap rises from 0.89 to 0.90 before extending the estimator.
- [x] Compare a more stable search before further limit tuning or a Blacks search.
- [x] Separate scene fitting from profile caps, refine and stabilize the fit, and reuse the guide across Exposure changes.
- [x] Validate the new per-profile tuning, photographic differences, cap hits, exposure/cap stability and native review app.
- [x] Adopt the owner's reviewed initial defaults for both profiles.
- [x] Add a local, structure-qualified Blacks contribution after general correction, with separate small profile caps.
- [x] Compare Black disabled/enabled over photographs and low Exposure extremes, including signed samples, exact black, visible tones, flat dark regions and latency.
- [x] Build and open the next isolated review app with Blacks tuning.

The owner subsequently requested quarter-pixel estimation without forcing Full display. The estimator now uses a canonical whole-photo Fast branch after Color mode, white balance and any active Dehaze. Monochrome retains its own preparation and reduces that RGB numerically. Full, Fast and Draft use this same analysis and apply its fitted response on their own working grid. Original-file GPU checks show identical analysis samples, guide and fitted amounts across all three color preview qualities for Natural and Strong. Regional preview remains unavailable for adaptive responses, so Smart refines the whole frame when necessary. Analysis and guide remain retained across Exposure and limit edits.

## Validation and remaining tuning

The initial integration matched the reviewed regional host orchestration byte for byte in Color and Monochrome for both profiles. Its Exposure-independent fit and shared caps were subsequently replaced following owner feedback. Current original-file checks compare graph composition with the numerical node, confirm DNG gain baking, unchanged Camera output, exact cached/fresh agreement and guide reuse across Exposure and cap changes. Ordinary checks cover shared tuning validation, photo persistence and scoped recipe isolation.

Actual CLI exports produced a resized Natural RGB16 TIFF and a full-resolution Strong linear DNG with a custom shadow limit. Native smoke captures cover Camera, Natural, Strong, shared-limit Settings, artistic HDR and Monochrome. Review inputs are separate copies with Natural and Local highlight recovery selected. The review app uses a separate preferences identity.

Adaptive display previews stay in memory during tuning because photo-sidecar identities intentionally omit shared limits. Persistent adaptive previews need an identity that also includes shared tuning before publication can be enabled.

Owner feedback still determines the preferred limits and whether profiles need separate values. This app trial does not claim the limits are final or that the guide's cold cost has been optimized.

The owner approved adding a small positive Blacks contribution, noting the risk of unattractive deep-shadow lift on Merrill files. Its role is to prevent useful near-black detail from being crushed while Shadows handles the general correction. The current trial estimates the toe lift locally after the applied response rather than expanding the global search dimension. Start at caps 0.03 and 0.06 for Natural and Strong. Keep the previous version as the Black-disabled comparison and leave manual HDR unchanged.

An isolated 0.80–1.00 sweep reproduces the discontinuity with the review spider's saved photographic settings, including +1.6 EV. At cap 0.89, both the initial and regional Natural fits select Shadows 0.5933, Highlights −0.15 and Whites 0. At cap 0.90, both select Shadows 0.30, Highlights 0 and Whites 0. Mean physical-frame linear RGB falls from 218.738 to 175.810, about 19.6 percent. The initial fit already changes branch. The regional guard carries that result through rather than introducing the switch.

The original cap stretched all four shadow candidates instead of only admitting additional candidates. Coupled shadow/highlight choices with close scores exchanged places abruptly. A fixed candidate lattice alone would not guarantee continuous output or monotonically brighter shadows when other fitted amounts also change. This motivated the fixed scene domain, blended/refined selection and smooth guard policies below.

The ignored `photographic_shadow_limit_sweep` probe accepts `FOVEONIC_TONAL_SOURCE` and `FOVEONIC_TONAL_OUTPUT`, with an optional `FOVEONIC_TONAL_EXPOSURE` override. It loads source-sidecar edits without writing them, resolves recorded camera choices, forces Natural, and records the current fitted response and linear means. Its full 0–1 sweep checks an unchanged scene target and nondecreasing mean RGB as the shadow cap rises. It also exports photographic frames around the reported jump for toggle comparison. Other caps use their standard defaults.

The owner approved an exposure-aware fit and separate profile caps. Exposure remains downstream as an artistic gain, but its known EV shift now enters the predictor and HDR pivot. Exposure changes refit the automatic correction while retaining the pre-exposure guide. Region membership and scoring are reevaluated. Exposure is applied only once to photographic appearance.

## Stable fitting trial

Scene fitting now uses a fixed 0–1 domain and two local 27-neighbor refinements. Almost equally scored alternatives are blended with a fixed soft-min scale of 0.006. Corrections fade out when they do not measurably improve the identity response. Per-profile caps are applied after fitting, so changing a cap cannot change the scene target or exchange other fitted controls. The combined graph node currently repeats fitting when caps change. Separating fit retention from application is a possible later optimization.

An initial hard guard projection still produced a flower discontinuity in the fine Exposure sweep. It was removed. Dark/bright membership now fades across 0.15–0.25 and 0.80–0.90 encoded brightness. Regional support fades between two and six effective samples. Scoring retains a continuous penalty for sampled shadow brightness/contrast loss. The final gain also retains the feasible reference gain in dark cells and fades that protection between 0.20 and 0.35 encoded brightness. Natural refers to the global fit. Strong refers to Natural, including its global floor. Caps below Natural's caps can weaken Strong's protection. These are gain-cell and scoring policies, not per-pixel contrast guarantees.

Natural's encoded dark-visibility target is 0.055 and Strong's is 0.08. Their global contrast thresholds are 0.006 and 0.01, and regional dark-contrast targets are 0.008 and 0.012. Strong also tolerates more midtone movement and targets lower highlight brightness. This explains why the earlier Strong often hit its shadow cap. Profile caps and visibility targets are different tuning choices. No difference is forced when there is no measured need for correction.

Initial application caps remain Natural 0.45/0.45/1. Strong starts at 0.75/0.65/1. These are trial values. The owner will judge visible correction, retained depth, specular gradation and noise over photographs before choosing final limits.

## Current evidence

The 101-step spider cap sweep passes, including 0.89 and 0.90. Color and Monochrome graph outputs match direct numerical-node execution. Cached and fresh renders agree, and Exposure refits without rebuilding the guide. GPU fixtures preserve finite signed samples, exact black and retained input buffers. Zero caps apply no Exposure or other edits, and an already visible flat gray image stays unchanged under both profiles.

Ten photos at −1, 0, +1 and +2 EV produce eighty new-profile frames. A separate sixty-frame report keeps Camera, manual HDR and the original regional profiles alongside the new profiles, all using Local highlight recovery. Each uses one shared corrected RGB input per photo without per-photo estimator tuning. Small Exposure increments were also investigated on the spider and flower. Smooth scoring removes the hard guard switches, though local tradeoffs can remain steep and need visual tuning.

At 0 EV, all ten photos select different Natural and Strong corrections. The fitted shadow amount reaches its application cap on three photos under each profile. The other seven Strong fits stop below its shadow cap. Natural has seven highlight fits within 0.01 of its cap and Strong has six. No white fit reaches its cap. These are selected-amount statistics, not measurements of every protected gain cell.

The current component run reports median profiled fit/application wall time of about 26 ms for Natural and 35 ms for Strong, with the guide already complete. These are single diagnostic runs per photo and exclude upstream reconstruction, guide construction, final tone and export. They are not app interaction latency. Refinement costs more than the original search. Guide reuse avoids the earlier measured roughly 85 ms cold guide cost during Exposure edits. A later performance pass should consider compact summaries and retaining scene fits separately from cap application.

Actual CLI checks export a resized Natural RGB16 TIFF and a Strong linear DNG at +1.6 EV. Native captures verify Camera, Natural, Strong, both profile tuning groups, artistic HDR and Monochrome. The second review app uses separate preferences and copied photos with existing edits, leaving the previous review files available for comparison.

## Local Blacks trial

The near-black contribution runs in the existing half-resolution gain kernel after capped Shadows/Highlights/Whites and reference protection. Its soft toe targets encoded brightness 0.025 for Natural and 0.035 for Strong through the selected camera curve. Exposure shifts the input threshold. Toe slopes are 0.3 and 0.4. The requested gain fades toward the target and is bounded by each profile's small Black cap through the existing HDR gain equation. No global search dimension, image allocation, dispatch or host readback is added.

Local confidence requires agreement between native RGB log-brightness gradients at a two-pixel radius and the existing filtered guide at an eight-pixel radius. Samples below −14 EV are excluded. This is a structure proxy rather than a calibrated signal-to-noise estimate. It should be judged on noisy real shadows before increasing the defaults. Exact black remains zero, and flat dark regions receive no added gain. Manual HDR is unchanged.

Ten photos at 0, −2 and −4 EV produce 120 Black-disabled/enabled profile comparisons with Local highlight recovery and no per-photo estimator tuning. Eighty additional review frames restore two or four stops for inspection while retaining the fit at the listed negative Exposure. The report uses toggles. Black-disabled 0 EV exports match all twenty earlier Natural/Strong TIFFs byte for byte. The fitted Shadows/Highlights/Whites values remain identical within every off/on pair.

The median affected frame area is about 0.1–0.2 percent at 0 EV, 2.7–2.9 percent at −2 EV and 19.8–20.4 percent at −4 EV. Maximum added gain stays below about 1.52× for Natural and 2.04× for Strong. Synthetic checks preserve visible midtones, flat dark samples, finite signed output, exact black and retained input buffers. Structured near-black samples gain roughly 20–30 percent at the default caps while retaining gradation.

Paired diagnostic gain-stage timings add roughly 0.1 ms or less at the tested settings. Timing jitter sometimes reverses the measured difference. These measurements exclude scene fitting, guide construction and app presentation, and are not a total latency guarantee. Graph/Monochrome/DNG baking, cached/fresh agreement, Exposure guide reuse, manual HDR coupling and the 101-step shadow-cap sweep also pass with Blacks enabled.

The third isolated review bundle includes eight Settings caps and passes native captures for Camera, Natural, Strong, shared tuning, artistic HDR and Monochrome. Concurrent Dehaze work initially prevented test compilation and shader startup. After that work supplied its test module and corrected shader parsing, fresh combined workspace tests, strict Clippy, formatting and artwork checks passed. Leave both workstreams uncommitted for the owner's combined cleanup and review.

The owner subsequently limited Base response to Universal tone for workflow consistency. Reference hides the selector and resolves the Camera baseline without overwriting the stored profile. The shared active-settings boundary also applies this rule to rendering and DNG appearance, while the activity predicate keeps inspection and preview policy consistent. Switching back to Universal restores the saved profile. Regression checks cover both profiles, sidecar retention, method switching, UI visibility and Color/Monochrome photographic and DNG output.

These method-boundary checks pass, including the original-file GPU comparison, GUI suite and strict Clippy. An earlier combined workspace run failed the concurrent Dehaze inspection test. Subsequent integration and release coherence checks pass the current Dehaze inspection, preview and export paths. No Dehaze processing was changed for the method-boundary adjustment.


The quarter-pixel follow-up passes original-file checks for identical analysis pixels, guides and fitted amounts across Full, Fast and Draft under both profiles. Cached Exposure and limit changes retain guides and match fresh output. Color and Monochrome preserve the Reference baseline and bake active spatial gains into finite DNG output. Native Smart captures show Natural and Strong using Draft feedback, alongside Camera, artistic HDR and Monochrome checks. Published preview identity advances to discard renderings from the former analysis path.

The owner renamed the visible selector to Adaptive with Disabled, Natural and Strong. Shared profile caps moved from Settings into a Tone-menu modal and use the shared numeric row layout. Internal photographic field identities remain unchanged.

Native Smart captures verify Disabled, Natural and Strong and the shared-limit modal. The modal uses both profile groups and the ordinary slider/value-field layout. Existing full-workspace and original-file checks pass after the UI changes.

The owner’s reviewed application limits are now the initial defaults: Natural Blacks/Shadows/Highlights/Whites 0.075/0.25/0.35/1.0, Strong 0.115/0.75/0.65/1.0. Shared saved tuning retains precedence.

Adaptive now extends to Classic and Quattro through the shared Universal path. Adapters supply a predictive neutral camera curve, code white and explicit working RGB bounds. Classic reuses its full-size input and guide for fitting; Merrill and Quattro keep fixed reduced analysis. Camera appearance retains the selected response after the baked spatial gain in DNG. Adaptive remains disabled by default.

Extension checks pass on Merrill, SD15, ordinary Quattro and Quattro H originals in Color and Monochrome. GPU replay matches the composed Adaptive node, reduced color previews share fixed analysis and fit parameters, and Classic analysis keeps the full working grid. Classic and ordinary Quattro also pass Auto/manual Exposure, cached/fresh previews and DNG sample/profile agreement with Adaptive active. Native Classic captures cover Disabled, Natural, Strong, limit tuning, artistic HDR and Monochrome.

The owner’s latest saved tuning lowers Strong Shadows to 0.55. Natural remains Blacks/Shadows/Highlights/Whites 0.075/0.25/0.35/1.0; Strong is now 0.115/0.55/0.65/1.0. Smart’s overview now uses normal Fast with requested denoising, Detail and ColorDQ instead of the minimal diagnostic Draft recipe.

The owner accepted the app-wired behavior for release closeout. Subsequent structural cleanup is recorded in [release coherence](./release-coherence.md).
