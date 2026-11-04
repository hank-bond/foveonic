---
summary: "Historical visibility-tone experiments led to the selected regional Adaptive tone response."
status: archived
---

# Visibility-oriented Auto HDR experiment

The owner authorized continued experimentation using a discrete spatial step and broader photographic comparisons. Favor useful visible signal over broad brightness increases. Keep Exposure and existing HDR controls independent. Architecture and final interface can be revised after reviewing results. Scientific background and limits belong in the [research reference](../../docs/visibility-tone-research.md).

- [x] Preserve camera, rejected-curve and preferred manual-HDR controls on the two supplied portraits.
- [x] Prototype a GPU summary and bounded response fit using the existing spatial HDR guide and response equations.
- [x] Compare two visibility strengths and restrained reuse of perceptual saturation across the portrait and example-photo set.
- [x] Check immutable inputs, finite output, unchanged manual HDR response and completed experimental timings.
- [x] Open a photographic toggle report with whole frames and useful native-detail crops.
- [x] Review the photographs with the owner before integration or choosing final behavior.

The experiment initially ran from an ignored original-file test with test-only kernels. The owner subsequently selected the regional versions for [application integration](adaptive-tone.md). The historical comparison generator and its test-only kernels have since been removed. Camera remains the app default. It consumes corrected calibrated RGB, uses the existing edge-preserving brightness base and applies one spatial gain before ordinary camera tone. Whole-photo sampling excludes sensor borders through the image's active rectangle.

The fit uses a coarse joint histogram of base brightness, residual and maximum-channel headroom. A coarse cross-scale gradient estimate provides a structure weight. A candidate grid varies Shadows, Highlights and Whites internally. These are numerical parameters of this experimental node, not changes to the stored user sliders. Black remains anchored. Candidate penalties include dark-detail visibility, highlight clipping, loss of visible contrast, changes to useful middle tones and unreliable shadow amplification. The score is an experimental SDR proxy, not a validated human-vision metric or a calibrated noise estimator.

The first color comparison reuses the existing tonal perceptual saturation operation after spatial correction, with restrained amounts. It is not an AgX implementation. Exposure remains unchanged in every candidate.

## First comparison

Ten photographs produce sixty controlled renditions. Native crops cover both hands, the dark spider, shadowed rocks and bright flower. All versions share one corrected linear RGB input per photo, default white balance and camera color mode. Saved artistic edits are excluded. Camera and the earlier fixed curve are controls. Manual HDR uses the owner's settings for the portraits and a fixed Highlights −0.35 / Whites −0.60 control for the other examples. The latter is not a hand-tuned target.

The GPU summarizes 256×256 samples into a joint histogram and scores 96 response candidates. Shadows span 0 to 0.45, Highlights 0 to −0.45 and Whites 0 to −1. The normalized black endpoint stays fixed. Natural + color adds the existing perceptual saturation at 0.03 for shadows and 0.12 for midtones, with zero highlight adjustment. This color addition is deliberately separate from the tone fit.

Both portraits, the bright red flower and yellow-flower scene select the strongest highlight and white reductions in this grid. This flags a search boundary, not an ideal strength. The dark spider and rock-shadow scenes select shadow lift without highlight reduction. Paper, foliage and the turquoise still life remain nearly unchanged under Natural. Strong can choose the same response as Natural. It is a different score weighting, not a guaranteed additional amount.

Initial visual inspection finds more separation in bright hands, knit fabric and red petals than the fixed curve. The dark spider remains dark. These observations support owner review rather than a default change. Deep-noise behavior, halos at strong edges and color appearance remain photographic acceptance questions. A coarse cross-scale gradient can confuse texture and correlated noise. Coarse histogram bins and fixed display thresholds also limit the fit.

The first full-resolution pass reported a 102 ms median guide interval and a 96–124 ms range. That interval was later found to include potentially unfinished upstream RGB work and is superseded by the isolated component measurements below. Natural fit and application took a median 14.2 ms, with a 14.1–21.7 ms range. Natural + color took a median 50.9 ms after the guide. These single observations are historical experiment timings, not app interaction latency measurements.

The ten-photo validation repeats the experiment with finite-value assertions before and after camera tone and verifies unchanged input buffers. Two existing GPU fixtures pass for native HDR coupling and linear-code-unit invariance after extracting the shared response equations. All 724 ordinary workspace tests pass serially. The response extraction changes ownership of equations, not the manual control math.

## Experiment retirement

The owner selected the app's regional Adaptive tone logic and approved removal of superseded implementations, comparison probes, kernels and report tooling. This document retains the photographic findings. Current requirements and regression coverage belong to the Adaptive tone integration.

## Owner review and Local recovery comparison

The owner finds the overall results close to the desired starting rendition. This endorses further work on the direction, without selecting a final strength or approving application integration. The owner requested the entire comparison again with Color Field: Local highlight recovery.

The later Local-recovery comparison explicitly selected Local for every rendition. The Local run explicitly enables highlight reconstruction, selects the local brightness solver and retains default selectivity 0. All sixty renditions and thirty native detail crops are rerendered from that upstream recovery. White balance, color mode, reconstruction controls, candidate grid and scoring remain unchanged. Automatic responses are fitted again to the newly recovered RGB. Natural selects the same response on nine photographs. The red flower adds shadow amount 0.15. Every recorded rendition identifies the recovery method and selectivity, and the viewer labels the method. The first comparison is retained separately.

The full Local run passes finite RGB checks before and after camera tone and input immutability checks on all ten photographs.

## Strength ordering feedback

The owner sees little benefit from Natural + color and finds Natural sufficient without the additional saturation. Preserve the comparison evidence. No further color compensation is selected for the next tone experiment.

The owner reports darker shadows under Strong on the sunlit red flower. The Local comparison confirms that Natural selects Shadows 0.15 while Strong selects 0. Both select Highlights −0.45 and Whites −1, so the visible difference comes from losing the shadow lift. The Classic comparison selected the same controls for both strengths. Blue parking sign also selects a smaller shadow amount under Strong, though its highlight and white amounts differ too. A lower shadow amount alone does not establish the visible result there.

Strong currently changes several objective thresholds and weights and independently selects a candidate. It has no ordering constraint relative to Natural. Calling it stronger therefore does not guarantee retaining Natural's shadow visibility. A possible next trial is to refine Natural while protecting already useful shadow detail. The guard should evaluate the rendered response because overlapping HDR controls can change shadows even when the shadow amount stays fixed. This refinement has not been implemented or selected by the owner.

## Small-highlight feedback

The owner wants the turquoise still life's specular band closer to the manual HDR control. In the Local run, Natural selects Highlights −0.15 / Whites 0, Strong selects Highlights −0.30 / Whites 0, and the manual control uses Highlights −0.35 / Whites −0.60. Both automatic choices omit the white-range compression that distinguishes the manual result.

The score averages sample losses by whole-photo area and adds a separate amount penalty. Small bright regions can therefore have little influence. The highlight term primarily penalizes crossing a near-white display ceiling, rather than rewarding additional gradation within every bright region. Its structure weighting also gives a flat highlight core less importance than a textured region. These mechanisms are plausible explanations for under-correction. No region-specific score breakdown or sampling-miss measurement has established their relative contributions on this photograph.

A possible follow-up would assess the bright tail or coherent highlight regions separately, giving small areas with recoverable gradation adequate influence while retaining white specular points and useful middle tones. Any such trial should compare all ten scenes and preserve the original controls. No scoring change has been made from this feedback yet.

The owner approved a regional measurement trial and describes frequently using maximum Whites reduction to reveal gradation and reduce distracting speculars. The [research reference](../../docs/visibility-tone-research.md#regional-measurements-and-bright-distractions) separates regional measurement from regional application and records the additional brightness-balance objective. Keep analysis in RGB and defer optimization and cache-policy implementation until photographic review of this trial.

## Regional trial

- [x] Add overlapping regional measurements while retaining one whole-photo HDR response choice.
- [x] Keep the existing cross-scale RGB structure proxy separate from upstream denoise/detail residuals.
- [x] Compare original and regional Natural/Strong with Camera and manual HDR across all ten photos.
- [x] Measure estimator kernels separately from the existing brightness guide and correction application.
- [x] Review photographic results before optimizing or implementing caching.
- [x] Select the regional application behavior and profile limits through photographic review.
Performance and cache-policy optimization remain separate from the completed photographic experiment and release coherence pass.

The regional trial samples the same active-area 256×256 grid. It summarizes 1,235 overlapping windows of 16, 32 and 64 sample positions per side. Each window records log-brightness variation, coherent structure, and separate bright/dark subsets. Bright scoring combines predicted gradation with prominence relative to the surrounding window. Only subsets with at least four sampled positions contribute. Average bright-region loss and the largest regional bright loss receive separate weights, preventing whole-photo area from being the only importance measure. These are heuristic regional summaries, not semantic objects or calibrated signal-to-noise estimates.

The first regional pass could drop shadow lift from the existing Natural result. The second pass therefore fits original Natural first and protects its qualifying dark subsets while fitting Regional Natural. Regional Strong then applies the same protection relative to Regional Natural. Candidates are rejected when a qualifying dark subset's predicted mean brightness or contrast drops by more than 0.002 in encoded SDR output. The guard evaluates regional summaries, so it is not a guarantee for every individual pixel. Existing Natural and Strong remain unchanged controls. The current trial omits the extra color boost and rejected curve from the new comparison while preserving earlier reports.

The regional pilot on turquoise still life selected Highlights −0.45 / Whites −1, versus original Natural's Highlights −0.15 / Whites 0. With the additional original-Natural shadow guard, Regional Natural instead selects Highlights 0 / Whites −1. Its visible metal-band gradation improves on initial inspection. The flower retains Shadows 0.15 under both regional strengths, while the old Strong control drops it to 0. One fixed regional scoring formula and the shadow protection rule apply across the set, with no per-photo parameter overrides. Broad photographic acceptance remains pending.

The owner subsequently reports improvements in the intended areas with no obvious regressions. Strong's meaning remains unclear because it sometimes raises shadows further and sometimes selects the same response. On `_P2M0015`, Regional Natural selects Shadows 0.15 and Regional Strong 0.45. On `_P2M0020`, both select 0.45. Both images already select Highlights −0.45 and Whites −1 at either strength. The second image therefore reaches every trial search boundary under Natural. Its unchanged Strong result does not establish that further correction would be undesirable. The shared bounds and quantized search can hide differences in scoring targets. The shadow cap is an experimental normalized amount, not the HDR control's full range or an exposure value in stops. Final strength semantics and limits remain undecided.

The second-pass report contains sixty whole frames and thirty-six native detail crops, adding the turquoise metal-band crop. All ten scenes use Color Field: Local with default selectivity. Sixty comparable existing control TIFFs are byte-for-byte identical to the preceding Local report. Inputs remain immutable and all edited RGB is finite before and after camera tone. A synthetic GPU fixture checks signed negative samples, exact black, finite outputs and retained input ownership. Its initial expectation of stronger white reduction than the control was invalid when the control already selected maximum reduction. The fixture now tests the numerical contracts rather than requiring a particular parameter difference. Very small bright features may remain limited by the existing response's spatial support.

The updated viewer is open with the isolated timing summary. All 724 ordinary workspace tests pass serially, as do the signed/black GPU fixture and existing native HDR coupling fixture. Strict all-target Clippy, formatting, documentation indexes, whitespace checks and five artwork tests pass. The sixty whole-frame and thirty-six detail assets exist and viewer JavaScript syntax passes validation. Experimental kernels remain test-only. No new app default, cache, commit or release is selected.

## Isolated latency measurements

An early timing boundary started the guide timer after stateless rendering returned but before queued RGB work was guaranteed complete. The benchmark now drains that upstream work before starting guide measurements. Historical report intervals retain their original evidence and are labeled as guide-plus-queue timing. The viewer can load a separate isolated timing summary using `--latency path/to/summary.json`.

The corrected ten-photo run measures three completed guide repetitions per photo, five warmed fit/application repetitions per method with alternating preselected controls, and one separate GPU timestamp capture per method. All use completed mode-rendered RGB and Color Field: Local. Medians below are taken across the photo-level measurements. GPU component durations and completed wall times come from separate runs and must not be added or subtracted as a single app latency measurement.

| Work | Measurement | Median | Range across photos |
| --- | --- | --- | --- |
| Brightness guide | Completed wall | 84.8 ms | 83.5–88.2 ms |
| Brightness guide | GPU kernels | 82.1 ms | — |
| Original Natural estimator | GPU kernels, excluding correction application | 1.53 ms | 1.26–2.40 ms |
| Regional Natural estimator | GPU kernels, excluding correction application | 7.57 ms | 6.51–9.62 ms |
| Regional Strong estimator | GPU kernels, excluding correction application | 11.0 ms | 9.05–13.4 ms |
| Original Natural fit and application | Completed wall | 13.2 ms | — |
| Regional Natural fit and application | Completed wall | 14.5 ms | — |
| Regional Strong fit and application | Completed wall | 17.5 ms | — |

The matched preselected HDR paths take approximately 12.9–13.2 ms completed wall time without fitting. These observations include host allocation, submission and diagnostic parameter readback. They exclude original loading/reconstruction, final camera tone, export and GUI presentation. Differences in GPU clocking, execution overlap and timestamp-pass boundaries limit direct wall/GPU comparisons. This is a component benchmark, not completed pointer-to-display latency.

The measured full physical raster is 4,928×3,264. Its Float32 guide occupies approximately 61.4 MiB, in addition to the retained RGB input. The regional sample array is 1 MiB and summaries are approximately 77.2 KiB. The guide is the largest measured repeated cost. Reusing original/Regional Natural selections and shared measurements between strengths is also a possible later optimization. No new cache or optimized processing path is selected before photographic review.

## Feature meaning and analysis lifetime

The current structure proxy compares RGB log-luminance gradients across offsets of 2 and 8 native pixels. Aligned gradients contribute more weight. The HDR residual is separately the difference between log brightness and its broad edge-preserving base. Neither is a semantic feature detector or an actual estimate of residual uncertainty.

Merrill Detail instead separates the logarithmic luminance ratio of unfiltered and denoised sensor-basis signals into fine/coarse bands. Multiscale denoise uses native-layer regression, variance and noise-model terms. Those earlier residuals include texture and noise and do not directly represent uncertainty of the final mode-rendered RGB. This experiment does not add graph dependencies on them.

The owner selected Color mode as an upstream dependency. The analysis uses mode-processed RGB and the selected mode's tone curve to predict display response. Exact guide caching requires unchanged RGB output version, signal/luminance definition and analysis geometry. The fit additionally depends on its predictive tone curve, objective configuration and estimator version. Upstream noise, banding, detail, white balance and highlight reconstruction can alter these inputs. Downstream artistic edits should not cause the baseline to refit when they do not feed this boundary. Use existing graph identities and effective parameter keys rather than hashing photographic pixels. Whether analysis can safely remain stable across particular upstream changes needs sensitivity measurements. Optimization, cache retention and any approximate reuse remain deferred until owner review.

## Selected application slice

The owner selected Regional Natural and Strong for application tuning. Shared limits belong in app preferences and are not saved per photo. Only the profile choice belongs in photo edits. One limit set applies across photos and both profiles initially. Separate limits per profile remain a possible follow-up. The [integration plan](adaptive-tone.md) owns current application work and validation.
