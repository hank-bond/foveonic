---
summary: "Integrate Merrill Fast and Smart previews while reducing native row-correction GPU cost."
status: archived
---

# Merrill Fast and Smart preview

The owner requested a Merrill experiment similar to Quattro's reduced overview, then approved enabling Smart in the app. Requirements: [Merrill Fast](../../docs/specs/merrill-fast-preview.md) and the shared [Smart policy](../../docs/specs/smart-preview.md). The integration preserves the stored application preference and initial Full default.

- [x] Profile the existing completed Merrill render and locate the expensive reconstruction stages.
- [x] Keep native sensor corrections, then resample calibrated RGB to half width and height before color reconstruction.
- [x] Preserve original metadata, framing, Full processing, inspection and DNG behavior.
- [x] Compare completed fresh and cached work, immutable retained images and Full/Fast quality switches.
- [x] Prepare a local visual comparison and report measured latency and limitations for review.

The first slice uses the existing triangular GPU pyramid reduction. The Merrill recipe owned working geometry and approximate noise parameters during the experiment. The [accepted Fast bypass](./merrill-fast-noise.md) subsequently removed reduced noise preparation. Sensor correction and whole-image highlight processing remain native-sized. The owner subsequently approved omitting row correction in Merrill Fast and profiling GPU optimization of the Full correction. Full, inspection and DNG still follow the requested row-correction setting. No CPU photo arithmetic or replacement source metadata is needed. Regional Merrill detail is a separate follow-up. Smart initially refines to whole-frame Full above 100% using the existing serial image worker and session cache.

- [x] Omit row correction in Fast without changing persisted edits or Full behavior.
- [x] Measure GPU fitting/statistics candidates and retain demonstrated improvements.
- [x] Verify retained row-correction changes against native fixtures and complete photographic baselines.

## Measurements and findings

The initial six-photo comparison used Sunlight, Standard and reconstruction step 2 with Classic highlights. Completed fresh processing and GPU view preparation excluded decoding, exports and report-image encoding. A conservative reduced render cost roughly 483–648 ms against Full medians of 782–992 ms. Omitting row correction brought five scene medians to 308–479 ms. The sixth scene had unusually variable samples. Omitting despeckle as well saves additional time but remains a separate visual comparison, not an accepted default. Merrill Fast retains requested despeckling.

The initial reduced-noise experiment required tile operations to take working dimensions from their numerical parameters because tile atlases have no image description. Its half-size preparation retained filter footprints and scaled independent-sample variance by the triangular filter's squared weights. The accepted Fast bypass removed both that preparation and its executor adjustment. Original metadata and decoded samples remain unchanged. The historical timings below precede the bypass. The comparison tool now follows the current Fast recipe.

Native row estimation spent most of its time fitting surfaces and rejecting scene-correlated patch residuals. Independent polynomial coefficients now run concurrently without changing each coefficient's transposed Float32 recurrence. Patch statistics use compensated lane sums and a fixed compensated merge. A row-major private residual workspace gives adjacent GPU lanes adjacent storage. Row-mean and final contribution ordering, count limits and application bounds remain unchanged.

| Scene | Row correction before → after (GPU ms) |
| --- | --- |
| Blue parking sign | 157.4 → 71.4 |
| Sunlit red flower | 157.8 → 68.3 |
| Paper and recycling can | 161.0 → 69.2 |
| Glossy leaves and buds | 161.2 → 69.1 |
| Turquoise still life | 167.6 → 74.1 |
| Rocks and tree shadows | 158.2 → 69.3 |

The before executable was frozen before editing the shaders. All six complete Full Float32 outputs remain byte-identical to its retained baselines. GPU diagnostics run separately from fresh wall-time measurements. The correlation-statistics summation order changed, so this evidence establishes matching photographic output on these originals rather than universal bit identity for every possible input. The existing native patch-bank fixture also passes. Independent dark/bright/correlation decision checks cover both rejection thresholds and all three patch sizes.

The [comparison tool](../../crates/foveonic/examples/compare_merrill_preview.rs) writes a local report with Full, Fast without row correction, and a separate Fast-without-despeckle experiment. It uses six explicitly supplied example originals, three warm fresh trials, color-managed JPEG review assets and cached-edit/Full-refinement measurements. Run it with an originals directory and a new report destination. Source sidecars are not read or written.

After the GPU optimization and Fast row omission, six-scene median completed fresh rendering is 682–917 ms for Full and 312–482 ms for Fast with despeckle retained. The optional no-despeckle comparison is 235–448 ms. Cached Fast exposure changes complete in about 29–39 ms. Full continuation with despeckle retained takes 517–683 ms. These are local warmed-source observations, excluding decoding and exports. They do not predict SD-card opening time.

All 526 ordinary workspace tests, strict Clippy, formatting, documentation and artwork checks pass. Targeted GPU checks pass for original patch-bank behavior, independent bright/dark correlation decisions, filtered resampling, exact fresh/cached edits and retained-image ownership. The report's controls and eighteen ICC-tagged review assets pass local checks. Native app packaging passes.

- [x] Owner approved enabling Merrill Fast and Smart in the application.
- [x] Resolve Merrill support from early metadata and preserve the application-wide choice.
- [x] Refine Smart to Full above 100% and retain Full through panning and Fit.
- [x] Verify native interaction, scopes, navigator, edit reset and Full TIFF/DNG exports.

The recipe advertises reduced-preview and regional-preview capabilities separately. Merrill has the former. Quattro and Tiger have both. The GUI asks those capabilities when choosing refinement rather than interpreting camera families itself. Regional requests are normalized before both execution and supersession checks, preventing a whole-frame Merrill result from being published with regional coordinates.

Integration validation passes all 528 workspace tests, strict Clippy, formatting, documentation-index checks and artwork checks. Real-file worker tests compare TIFF and DNG bytes under Fast/Smart with independent Full exports for both Merrill and Quattro, then verify restored quarter-size preview and native display coordinates. Isolated native runs keep Smart visible before rendering, show the quarter overview through 100%, refine to Full at 200%, retain the same Full revision when panning, reset after edits and retain Full at Fit. They also check scopes, navigator, cache reopening and disabled source-sidecar writes. The Quattro run still publishes a regional detail tile before whole-frame Full. The rebuilt native app includes the integration.

Regional Merrill detail remains future work. Full exports and per-photo settings retain their current behavior.

## Commit preparation

The final cleanup removed the executor extent override needed only by the discarded reduced-noise experiment. Merrill's recipe owns the Fast bypass, working grid and native checkpoint. The existing pyramid operation supplies filtered reduction. Independent reduced and regional capabilities preserve the established Quattro route. Row-correction coefficient slots and bright/dark rejection thresholds now have named shader constants. The comparison tool describes the current bypass and retains the explicitly separate no-despeckle experiment. New-photo noise defaults are recorded in the [default follow-up](./merrill-noise-default.md).

All 529 workspace tests, strict Clippy, formatting, documentation and artwork checks pass after cleanup. The actual-file GPU check confirms exact fresh/cached Full restoration, immutable retained images, reduced framing and native inspection/DNG geometry without the experimental executor override. Row-banding GPU checks confirm native patch counts/bounds and bright/dark correlation rejection. Prior native Smart, sidecar lifecycle and Full TIFF/DNG export checks remain applicable. The app bundle is rebuilt. Unrelated release documentation is excluded from this commit.
