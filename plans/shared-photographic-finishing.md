---
summary: "Integrate shared photographic tone, monochrome sensitivity, local contrast and sharpening with camera recipes and bounded interactive reuse."
---

# Shared photographic finishing

Requirements: [photographic finishing](../docs/specs/photographic-finishing.md), [tonal ranges](../docs/specs/tonal-ranges.md), [monochrome rendering](../docs/specs/monochrome-rendering.md), [native sharpening](../docs/specs/native-sharpening.md), and [interactive rendering](../docs/specs/interactive-rendering.md).

The owner authorized implementation, photographic comparisons and cleanup. Commit and release remain outside this slice. The owner subsequently approved Reference Auto sharpening as the fallback for unspecified amounts. Local Contrast has photographic approval as a standalone effect.

## Work

- [x] Establish shared settings and camera-independent photographic operations.
- [x] Offer Reference and Universal tone/color while retaining camera appearance and RAW exposure.
- [x] Extend spatial HDR to all photographic families through explicit numerical adapters.
- [x] Add six monochrome sensitivities using an independent color guide.
- [x] Offer Universal sharpening in place of each family's creative reference sharpening.
- [x] Integrate persistence, clipboard, inspection, frozen batch requests, Fast and Smart.
- [x] Evaluate Local Contrast independently of reconstruction detail and sharpening.
- [x] Make every family's tone selector switch both visible and effective controls without losing inactive values.
- [x] Limit photographic numeric readouts to three significant digits without changing stored precision or panel width.
- [x] Record the deferred [monochrome refinement](./monochrome-refinement.md) work.
- [x] Separate numerical operations and immutable aliases, and remove obsolete comparison tools.
- [x] Complete cleanup verification and rebuild the local app for review.
- [x] Unify Reference/Universal controls and bypass policy across all families, remove the combined layout, and align section badges and parameter groups.
- [x] Owner review and approval to commit.

## Architecture

Camera recipes retain sensor calibration, corrections, reconstruction and base appearance. Shared finishing consumes normalized, linear-light photographic RGB. The typed graph names tone, monochrome sensitivity, local contrast and Universal sharpening as separate operations. Each numerical operation owns its dispatch and shader. A small common shader header defines their parameter ABI and RGB access. Named host constructors prepare each operation's parameters. Inspection aliases retain the same immutable buffer without dispatching a numerical operation.

Reference remains the default tone/color and sharpening method. Unspecified Reference sharpening amounts now select Auto, while existing manual amounts retain their meaning. Universal has no Auto control. Universal tone neutralizes native contrast/saturation, retains RAW exposure and the selected camera mode, then applies shared contrast and saturation once. Negative contrast strength is reduced to keep the full control range monotone. Positive saturation protects already vivid samples. A processed-image exposure shoulder remains outside this work.

Every family's Reference tone method uses original Shadows/Highlights and bypasses HDR/Local Contrast. Universal uses the four HDR ranges and Local Contrast, and bypasses original Shadows/Highlights. Canonical effective settings serve rendering and inspection. The saved recipe retains inactive values.

HDR shares one response and spatial-filter implementation through code-unit and normalized-photographic adapters. Merrill retains its reviewed pre-curve position and camera/exposure-derived pivot. Classic and Quattro use normalized photographic RGB. Selecting a tone method does not move HDR or change its equations. A later photographic adjustment cannot recover clipping already imposed by camera appearance.

Monochrome sensitivity builds a companion ordinary color recipe with neutral creative controls and Standard camera mode where available. It shares decoded source storage and selected white balance, and stays independent of the authored monochrome mixture. Graph composition deduplicates identical technical prefixes and scopes distinct guide outputs. Hue selection uses fixed sRGB primaries, smooth overlapping weights, gray protection and near-black protection. Brightness changes apply to the dedicated monochrome result. Zero sensitivity avoids the guide branch.

Universal sharpening replaces creative reference sharpening while retaining required reconstruction. It filters square-root brightness with a Gaussian, thresholds the high-pass difference and restrains halos through local extrema. Quattro retains residual reconstruction and native local contrast while bypassing proportional and optional per-channel sharpening. Classic omits its root-domain detail addition. Merrill omits its reference signed high-pass application.

Local Contrast applies a five-tap undecimated decomposition of log brightness at six scales. Per-band and total exposure bounds restrain texture amplification. It shares response gains with Quattro preparation, but preserves its own sample domain, filters and bounds. It is a separate effect, not an alternative reconstruction method. The unused temporary components and six per-level scalar-copy dispatches have been removed. Each temporary now stores only its base and accumulated correction, and the vertical pass reads the retained prior base directly.

DNG continues using its established technical samples and appearance. New shared finishing controls do not add baking or new appearance composition. Preview and TIFF use the photographic graph.

## Interactive reuse and regions

All processing stays in the existing graph/executor. Additive spatial support protects Merrill and Quattro viewport crops. Quattro expands an oriented request before its native regional recipe and extracts the original rectangle after finishing. The full follow-up can reuse sensor preparation from the first detail render. Neutral operations omit their work. Retained buffers remain immutable.

The bounded cache counts aliased storage once and gives explicit downstream edit boundaries admission priority over speculative early checkpoints. A 1 GiB budget can evict earlier entries while a later retained result still serves the edit. Checkpoint roots follow retained-result boundaries, preventing upstream recomputation merely to refill optional entries. This correction keeps monochrome sensitivity edits from unnecessarily recalibrating the source.

## Findings retained from review

Quattro Smooth/Crisp jointly changes residual restoration, native local contrast and adaptive sharpening. At low ISO, native local contrast uses packed-Lab lifting gains and a lowest-band kernel. At high ISO it uses bounded box local contrast. From the middle preset toward Smooth, residual restoration is unchanged. Toward Crisp, all three components change. The shared filter does not reproduce the full compound preset.

The native gain table runs coarsest to finest, whereas the shared decomposition builds finest to coarsest. Its initial direct mapping wrongly applied the strongest fine-band gain to the broadest band. The corrected mapping reverses that order. A sinusoidal GPU check independently evaluates four periods and both amount signs. The owner subsequently accepted the shared filter's distinct look. No replacement Detail-method selector is planned.

The original implementation-study evidence does not establish complete application equivalence for tone, saturation or HDR spatial filtering. These are independently implemented photographic methods. Reference pipelines and defaults remain selectable.

The one-off tone, shared-finishing and component-isolation report generators were removed after review. Numerical regressions and general pipeline/Smart profilers remain. Local generated reports are not application dependencies.

## Measurements and verification

Earlier review trials on an Apple M1 Pro measured completed processing plus display-texture preparation, excluding decode, initialization, readback and export. One warmup preceded three rotated fresh trials and three cached edits. Other machine activity was uncontrolled. Cached portrait edits measured approximately 64 ms for tone, 90 ms for sharpening, 82 ms for HDR, 198 ms for Local Contrast and 103 ms for monochrome sensitivity. These describe the pre-cleanup review build, not event-to-screen latency or a new cleanup benchmark.

Before cleanup, capture complete Float32 and TIFF baselines for color and monochrome on Merrill, ordinary Quattro, Quattro H, DP1s and SD9. Verify exact equality after shader organization and temporary-storage changes. Reproduce full-frame comparisons with `profile_pipeline`, using a new export directory, `--goldens` and the selected photographic controls. Source decode and TIFF export are reported separately.

Retain GPU property checks for neutral/headroom behavior, hue selection, monotone contrast, saturation restraint, flat fields, immutable inputs, threshold/halo controls and frequency response. Retain original-photo checks for cached/fresh equality, hidden-control bypasses, unchanged DNG samples/appearance, and exact regional/full agreement. Preserve Draft-to-viewport-to-full continuation without repeated sensor preparation.

Cleanup verification passes all 597 ordinary workspace tests, strict workspace Clippy, formatting, generated indexes/document checks, whitespace checks and five artwork tests. Six GPU numerical/property checks pass, including timestamp capacity and dispatch counts. Original-photo fresh/session and unchanged-DNG checks pass for all five sampled camera models. Merrill, Quattro and Quattro H viewport crops match full-frame pixels exactly, including Draft-to-viewport-to-full continuation.

All ten full-resolution color/monochrome cases match their captured Float32 and TIFF baselines byte for byte after cleanup. Each Local Contrast execution uses six fewer dispatches. Temporary base/correction buffers use two Float32 components instead of four, and the separate scalar copy is gone. The engine's reported cumulative allocation bytes decrease accordingly. Those totals do not measure peak resident memory or user-visible latency. The macOS app bundle builds successfully. Subsequent UI and policy checks pass all 597 ordinary workspace tests and strict Clippy. Original-photo pixel comparisons verify that inactive Reference/Universal controls have no effect in color or monochrome on Merrill, Quattro, Quattro H, DP1s and SD9. The isolated native editor capture completes successfully and confirms the section badges, Sharpening labels, Auto default, grouped highlight controls and reduced child-text gaps. The owner approved committing this implementation before the monochrome follow-up.

Further monochrome work is tracked in its dedicated plan. DNG finishing support and a processed-image exposure shoulder remain separate decisions.

A follow-up M1 Pro comparison used three measured runs after warmup, with Reference sharpening explicitly on Auto. Full renders were broadly similar: the Merrill portrait baseline was 867 ms and Universal tone/sharpening 846 ms, while dp0 Quattro measured 978 ms and 994 ms. Cached contrast/saturation edits measured 85 ms Reference versus 61 ms Universal on Merrill, and 126 ms versus 105 ms on Quattro. Cached sharpening measured 85 ms versus 81 ms on Merrill and 206 ms versus 125 ms on Quattro. Local Contrast alone measured 126 ms and 176 ms respectively. Timings include display-texture preparation but exclude source loading, initialization, readback, export and presentation. Sharpening amounts were not visually strength-matched. Other machine activity was uncontrolled.
