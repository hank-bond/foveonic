---
summary: "Prepare the reviewed Color Field boundary recovery with separate evidence and application masks for a scoped commit."
---

# Color-field boundary recovery

## Approved scope

The owner accepted the separate-mask photographic trial and requested cleanup for commit review. Update Color Field: Local and Global together. Keep Classic, method controls, saved settings, solver bounds and denoise order unchanged. Exclude unrelated branch work. Follow the [color-field requirements](../docs/specs/color-field.md).

## Findings

The plaque and viewer head in `_P2M0060.X3F` have magenta fringes beside clipped specular highlights. Many visibly wrong boundary pixels are well below raw clipping and were therefore skipped by Color Field. Other single-layer-damaged pixels have nonpositive matrix luminance and were also skipped.

A first boundary trial preserved arbitrarily small positive matrix luminance but used a much brighter layer estimate below zero. That discontinuity introduced black dots. A continuous transition to layer brightness removes the dots. Applying the same broad reliability reduction to the field and native fit then changed legitimate sweater borders and swaddle folds.

The accepted trial separates field evidence rejection from a fading four-pixel application footprint. It keeps original channel reliability in native gradients and in the native color fit for originally affected pixels. Newly included pixels use a guarded brightness anchor directly. They do not become native brightness unknowns. This retains most of the metallic-fringe improvement with much smaller changes in the owner-preferred fabric crops. Remaining differences were reviewed and accepted for this cleanup.

Local and Global comparisons covered the plaque/head, magenta petals, striped skirt, knitwear, skin and swaddle, all at −1.5 EV. Screenshot-registered fabric crops are estimates, not authored coordinates or numerical goldens. RGB8 difference counts measure preservation of those specific crops, not general photographic quality.

## Steps

- [x] Record the owner's accepted behavior and preserve the private trials.
- [x] Remove experimental switches and rejected algorithms from the candidate.
- [x] Preserve the pinned CPU modules and add an independent boundary adapter without relaxing numerical bounds.
- [x] Add regressions for the boundary footprint, original reliability, surviving channel constraints and zero-luminance continuity.
- [x] Confirm cleanup against the accepted build and validate the original-file, cache and DNG routes.
- [x] Run workspace, lint, formatting, documentation and artwork checks on the isolated candidate.
- [x] Prepare only the highlight changes for review, preserving unrelated work.

## Validation

Synthetic recovery keeps its `1e-4` maximum normalized error bound. Full-raster recovery keeps `2e-4`. Quantization is checked separately before both solvers receive identical field evidence. The independent test adapter reuses the unchanged pinned color solver and native color application. Its brightness equations and boundary decisions are evaluated independently in double precision. The imported modules remain unchanged.

Persistent photographic previews must be invalidated because their recipe bytes alone do not identify changed shader behavior. The preview signature advances to version 14, beyond the in-progress versions already used by unrelated branch work.

This plan covers commit preparation. No commit, installation or publication is authorized by the preparation request.

## Preparation results

Cleanup is byte-identical to the accepted build for full highlight-layer and final-output buffers from all five reviewed photographs in both methods. All comparisons use −1.5 EV and matched recipes.

Nine synthetic scenes pass the unchanged `1e-4` bound. All 17 full 4928×3264 sensor rasters pass `2e-4`. The largest normalized error is approximately `1.9274e-4` on `_P2M0060.X3F`. The strict fresh/cached corpus comparison passes for both methods at strength 0, 0.25, 1 and restored 0. The retained-image, selectivity and DNG/session checks pass. Original-file and editing-sidecar copies remain unchanged.

Workspace tests, strict all-target Clippy, formatting, documentation-tool tests and five artwork tests pass. The new preview test rejects signatures from processing versions 11–13. No numeric assertion, solver tolerance or iteration limit was relaxed. The pinned imported CPU modules are unchanged.

HEAD already contains source-index links to unrelated feature files that are present only in the owner's uncommitted work. Removing those links makes the isolated full index check pass, but those unrelated index removals are excluded from this patch. The highlight indexes are regenerated independently. The complete working-tree documentation check passes with the owner’s unrelated files and index entries preserved.

No commit or app replacement has been made. Broader photographic acceptance beyond the reviewed five scenes, high-ISO coverage and controlled performance measurements remain future work.
