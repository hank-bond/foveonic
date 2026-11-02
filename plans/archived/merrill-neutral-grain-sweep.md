---
summary: "A 228-setting sweep favors earlier correction with stricter color protection while preserving a stronger review alternative."
status: archived
---

# Neutral restored grain parameter sweep

The owner authorized a parameter sweep of the test-only neutral-restored-grain candidate. Keep app processing unchanged. Extend the existing [research harness](../../crates/foveonic/examples/chroma_investigation/AGENTS.md) rather than introducing a new production stage.

- [x] Parameterize gain, weak-signal, input-disagreement and maximum-correction gates with the existing candidate as the control.
- [x] Measure a broad grid on the reported portrait and other supplied Merrill photographs.
- [x] Check supported colored details and deliberately noisy color controls. Exact input/reference agreement alone is an insufficient preservation test.
- [x] Render a small representative shortlist through normal photographic continuation, with whole-image toggles and native/+2/+4 EV views.
- [x] Document tradeoffs and recommend a balance without claiming an ideal from photographs without clean targets.
- [x] Run appropriate GPU, workspace, documentation and browser checks.

Full-atlas diagnostics include processing halos. Sampled crop metrics are not whole-photo scores. Keep numerical screening before ColorDQ separate from final photographic comparisons. Use small JSON tables for the broad grid and reserve JPEG assets for the shortlist because local disk space is limited.

## Numerical screening

The first grid contains 192 parameter combinations, followed by 48 combinations focused on stricter disagreement thresholds. Twelve overlap, giving 228 distinct settings plus unchanged Chroma 2 and 4 controls. Seven photographs produce 1,708 numerical comparisons including repeats. Nine known-input scenes produce 2,196 control comparisons including repeats. All declared luminance and outside-mask assertions pass. Supported noiseless color structures remain exact.

The noisy-color controls intentionally make the filtered color correct while the input disagrees. The original candidate retains only about 8% of the noisy green and blue points' chroma. Raising the disagreement transition from 0.04–0.14 to 0.16–0.40 retains about 74% of green and nearly all blue and the sampled edge color. A 0.20–0.45 transition retains about 92% of green and all sampled blue and edge color. These fixtures expose ambiguity rather than estimate failure frequency in real sensor data.

Earlier gain onset improves suppression in the portrait's broader noisy background. A gain transition of 1–2 and weak-reference proxy of 0.5–2 limits correction to weak estimates while allowing modest restored brightness to participate. Raising the weak-signal threshold far above that extends correction elsewhere with little extra improvement in this portrait's sampled green tail. Merely limiting maximum correction to 50% or 75% reduces both the desired repair and the unwanted desaturation. Stricter disagreement provides a more useful separation in these controls.

These are numerical findings before ColorDQ. Photographic continuation and visual review determine whether the balance is useful. No single weighted score establishes an ideal from photographs without clean targets.

## Photographic shortlist

Compare current Chroma 4, the original candidate, earlier onset with moderate color protection, earlier onset with more color protection, slower onset with moderate protection and a broader less-protected control. Keep maximum correction at one for the shortlist and vary the gates.

The portrait retains the reported crop. Other photographic views move to areas containing the largest projected-core layer correction for the moderate-protection finalist. This exposes changes the original crops missed. It is a review location rather than a guaranteed maximum displayed difference after ColorDQ and tone. Every variant uses the same crop within a photograph. Whole-photo views remain available. First-stage screening retains its original measurement regions and is labeled separately.

An initial peak diagnostic included overlapping tile halos. Four selected locations had zero correction in the corresponding projected core. Exact coordinate probes caught this before report delivery. Restrict peak selection to active sensor pixels inside tile cores and regenerate the affected review locations. Full-atlas counts and mean mask remain explicitly labeled as including halos. These are not unique photographic-pixel counts.

The concurrent banding refactor changed its test-stage API to require Fine or Coarse explicitly. Four calls in the older axis study were updated to Fine, preserving that study's Fine-only, Coarse-zero recipe. Production banding implementation is outside this sweep.

## Recommended balance

Start owner review with gain 1–2, weak-reference signal proxy 0.5–2, input-disagreement transition 0.20–0.45 and maximum correction one. In the portrait, final green-excess P99 falls about 45% from unchanged Chroma 4, versus 31% for the original candidate. P99.9 falls about 46%, versus 48% originally. Correction at the three strongest traced pixels ranges from about 72% to 96%, versus approximately 96% for the original. These trade some correction of individual extremes for much better retention of noisy but correct color in the synthetic control.

The stronger alternative uses disagreement 0.16–0.40. Portrait P99 falls about 53% and P99.9 about 50%, while the noisy green point retains about 74% of its original chroma. The conservative recommendation retains about 92%. Its noisy blue point and noisy colored edge remain exact, and the deliberately bad reference retains approximately 5% of its original chroma. Those are fixture-specific results rather than guarantees for arbitrary real colored lights.

Visual improvement remains subtle. Neither finalist removes the broader green pattern. Other photographic hotspots mostly change very little. A stronger correction is available in the report to expose the tradeoff, but its additional color loss is not justified as a default by these examples. No production algorithm, setting, preset or sidecar was changed. The owner has not selected an implementation.

The owner subsequently approved the more protective finalist. Its [integration plan](./merrill-chroma-safeguard.md) records the separate production change.

## Checks and artifacts

The final review contains six candidates across seven photographs with 252 native-crop and overview JPEGs. Exact probes verify that regenerated review coordinates contain projected-core corrections rather than unused halo changes. Seven unchanged screening metric/probe tables replay identically across the banding API migration. Final photographic samples pass their no-op replay checks, and all candidate runs pass their declared texture-stage luminance and outside-mask checks.

The broad and focused numerical sweeps, nine-scene control grids and photographic shortlist pass. Workspace tests pass with 702 ordinary tests, with fixture-dependent tests remaining opt-in. Clippy, formatting, five artwork tests and document checks pass. WebKit verifies the photo gallery, six finalist choices, full-image A/B and Space toggles, equal display boosts, overviews, all 228 sweep rows and parameter filters without script errors. All photographic assets and evidence links exist.

An intermediate full-workspace rebuild exhausted local disk space and interrupted four replacement crops. After space became available, the replacements were rerun into a new directory. Report assembly reuses completed immutable JPEGs through hard links, preserving earlier outputs and avoiding another full copy. Timing outliers during this interruption are not evidence about algorithm latency.
