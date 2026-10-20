---
summary: "Investigate whether color agreement and boundary controls produce visible changes independently of GUI and cache behavior."
---

# Color-field control sensitivity

## Approved scope

The owner reports no discernible slider effect across the photographs tried and approved a sensitivity investigation. Compare each control independently at its minimum and maximum across the frozen corpus, measure output differences, inspect the registered highlight regions and compare fresh versus cached rendering. Do not change processing, control ranges, defaults or the GUI as part of this investigation. Do not modify original inputs or sidecars, commit, install or replace the running app.

Follow the [color-field requirements](../docs/specs/color-field.md). Agreement and boundary restraint remain separate gates pending photographic evidence. Earlier integration tests compared cached and fresh outputs but did not establish that moving either control changes the image. Passing those tests is not evidence that the exposed ranges are useful editing controls.

## Checks

- [ ] Trace GUI edits, worker settings, graph keys and shader consumers.
- [ ] Compare defaults and a repeated default to establish numerical variation.
- [ ] Compare agreement 0.25 / 4.0 with boundary fixed at 0.75 EV.
- [ ] Compare boundary 0.15 / 3.0 EV with agreement fixed at 1.0.
- [ ] Measure pre-despeckle calibrated layers, final floating RGB and encoded RGB16.
- [ ] Check fresh versus cached renders for both Local and Global.
- [ ] Inspect the registered PR crops and full views, distinguishing amplified differences from ordinary display differences.
- [ ] Summarize whether this is broken propagation, weak controls, clipped/invisible changes or an unresolved frontend issue. Propose changes only after the findings are reviewed.

## Method and preservation

Use one frozen measurement binary and a new ignored directory. Keep recipes, control values, failures and source/binary hashes. Retain numerical summaries and color-managed full/native-region previews. Only explicitly tool-owned staging TIFFs may be consumed through the existing verified preview workflow. Preview images are eight-bit and are not numerical goldens. Recovery arithmetic stays in WGSL. CPU comparisons and preview packaging are diagnostics, not photographic recovery.

The app may stay open, but do not close or alter it. Runtime measurements are not the purpose of this investigation. Preserve failures without retries, fallbacks or weaker assertions.

## Findings

The initial pilot and partial corpus measurements established that agreement and boundary extrema reach the shader and change fresh/cached numerical output. Registered native crops showed little useful control over the skirt's broad tint. The corpus run was interrupted for storage after eight scenes, so it is not a complete all-corpus sensitivity result. The tuning disclosure also had to be opened before the original GUI pointer test could exercise its sliders.

Subsequent [bleeding experiments](./color-field-bleeding.md) tested native-ratio evidence and neutral restraint. The owner clarified that a photographic slider should offer per-image flexibility, not one setting that improves every scene. The approved [color restraint implementation](./color-field-restraint.md) replaces the two ordinary GUI gate controls with one final-fit neutral-restraint slider. Agreement and boundary values remain independent investigation controls. This supersedes further extrema screening as the route to ordinary GUI tuning.
