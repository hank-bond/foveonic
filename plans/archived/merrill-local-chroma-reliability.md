---
summary: "Local chroma trials isolate amplified weak-color noise, limited neutral-grain gains and shadow-neutralization rejection."
status: archived
---

# Local chroma reliability

The owner authorized continuing the [green-outlier investigation](./merrill-chroma-outliers.md). Compare local color safeguards on the reported portrait and other Merrill photographs, keeping the existing app unchanged. See the [opt-in research harness](../../crates/foveonic/examples/chroma_investigation/AGENTS.md) for reproduction.

- [x] Verify that candidate changes remain inside their masks and preserve their declared brightness measure.
- [x] Check known bad estimates, supported dim colored points, real edges and independent brightness texture.
- [x] Compare matched photographic crops and whole-photo views, including the reported portrait, car shadows, foliage, eyes, hair and rotated scene structures.
- [x] Generate photographic A/B toggles with Native, +2 EV and +4 EV display views.
- [x] Record gains, failures, latency scope and the next recommendation for owner review.

## Candidate scope

Keep Current Chroma 2 and 4 controls. Compare a local blend cap, weak-reference pooling, input/neighbor agreement checks, wider pooling support and RGB-luminance preservation. The controls use the same native noise processing and photographic continuation. Candidate inputs remain immutable. The app, source sidecars and processing defaults are unchanged.

Layer-weighted brightness is not the same as final linear RGB luminance. Measure both rather than treating the former as proof of unchanged visual brightness. ColorDQ, clipping and the camera curve can further affect displayed brightness.

The study also tests a separate candidate that retains reference color at its own brightness and removes its hue only from extra restored brightness when confidence is low. Input/reference agreement protects supported colored details in its known-input controls. No correction was selected for production.

## Findings

Eight candidates across seven photographs produced 56 matched comparisons. All passed their declared texture-stage luminance and outside-mask assertions. Seven sampled photographic no-op replays matched exactly. The separate 36 known-input controls passed. Loose pooling spreads color around dim points and across an edge. Agreement-checked variants preserve those controls, which contain noiseless input/reference agreement rather than a full sensor noise simulation.

The three strongest traced green pixels receive no correction from the guarded pools. Even the loose pool barely changes them because neighboring estimates agree on the green proportions. Neutralizing only extra restored brightness reduces final green excess at those three pixels by about 96%. In the portrait measurement region, the sampled 99.9th percentile falls about 48%. Much of the background color remains. Other sampled crops have little or no final change. Natural photographs have no clean target, so these measurements establish a local effect rather than recovered scene color or general safety.

Crop statistics use a two-pixel grid, which misses the five individually traced coordinates. Exact probes therefore record texture masks, corrected layers, pre-ColorDQ RGB and final linear sRGB separately. JPEG review views apply display boosts to float results before encoding. Overviews use nearest sampling for context rather than fine-detail judgement.

Separate production shadow-neutralization probes explain the owner's observation. At full strength with default selectivity, the three strongest green points remain exactly unchanged. Their darkness and hue qualify, but their regional chroma relative to lightness is approximately 1.90–2.04 against the default color tolerance of 0.25. The near-neutral protection suppresses correction. Maximum color tolerance gives only about 7–9% correction with default hue width. Widening hue alone does not help. A rule intended to preserve real colored objects can therefore preserve extreme noisy color too.

Warm candidate passes took approximately 20–57 ms including allocations and a separate output/mask pass. Upstream noise processing and downstream photographic continuation are excluded. Other GPU activity was uncontrolled. No production integration or latency claim follows from these prototype timings.

## Recommendation

Keep production processing unchanged. Present the limited neutral-restored-grain improvement for owner review. If useful, investigate confidence in the filtered reference before selecting a correction. The present weak-signal gate uses the unfiltered noise model as a proxy, whose thresholds are not calibrated for the filtered estimate. No widening of shadow neutralization or replacement of the denoiser is approved.

## Checks

The opt-in photographic study, exact-pixel replay, known-input controls and production shadow-neutralization probe passed. Workspace tests, Clippy, formatting, document indexes and five artwork tests passed. WebKit verified gallery navigation, whole-image A/B toggles, matched exposure boosts, overview switching, loaded photographs and evidence links without script errors. All 336 photographic assets and linked evidence files exist.
