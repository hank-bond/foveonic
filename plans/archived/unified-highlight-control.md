---
summary: "Replace neutral restraint with a bounded color-consistency cutoff and share one Highlights/A row across Classic, Local and Global."
status: archived
---

# Unified highlight control

## Approved scope

The owner selected the local mixed-evidence cutoff after the [selectivity investigation](./color-field-selectivity.md). Put the Method dropdown first, then one Highlights slider and independent enable checkbox for all three methods. Classic alone retains A, its existing signed threshold range, and the baseline calculated from camera exposure metadata. After reviewing the initial raw-cutoff interface, the owner clarified the expected positive-only optional-processing direction. Color Field: Local and Color Field: Global now share a 0–1 selectivity strength. The owner removed the redundant color-field A button: strength defaults to 0, preserving the original behavior. Increasing strength rejects more inconsistent evidence. Do not reinterpret 0 as a tuned cutoff.

Follow the [color-field requirements](../../docs/specs/color-field.md). Preserve per-cell original neutral coefficients, immutable source reliability, healthy samples, full-sensor geometry, Local's 64 sweeps, compensated iterates, true-residual checks and existing stopping/error bounds. Use WGSL for recovery. Remove the neutral-restraint processing path and CLI option. Migrate retired automatic and neutral-restraint settings to 0 during decoding, without rewriting owner files or retaining a second recovery implementation. Preserve independent Classic and color-field settings across method switches.

The owner subsequently approved commit, merge and worktree removal through the [production plan](../color-field-production.md). Do not install, launch or replace the running app. Preserve investigation evidence and report cache differences honestly.

## Steps

- [x] Store scalar bounded strength defaulting to 0. Remove color-field A and migrate retired automatic, raw-cutoff or neutral-restraint data on input.
- [x] Integrate local ratio-scatter weighting with explicit original neutral coefficients. Preserve the compensated solver's arithmetic and migrate all color-row fixtures to the new layout.
- [x] Remove the final-fit neutral addition. Gate variance work off in A.
- [x] Move Method first and use one Highlights row for all three methods, with A only for Classic. Classic keeps its signed range. Color-field manual strength is 0–1 and starts/resets at 0.
- [x] Migrate CLI, sidecars, help, sensitivity tooling and documentation to the selected control.
- [x] Test strength bounds, zero baseline, migration of retired A, missing/retired fields, method switches, enable state, reset and pointer edits.
- [x] Check zero/original behavior against all sixteen numerical reference rasters, manual selectivity, healthy samples, brightness and original neutral weights.
- [x] Resolve the failed strict cached Color Field Output comparison through deterministic GPU row ordering. Both methods now pass exactly across the complete corpus without changing equations or tolerances. See the [determinism plan](./color-field-determinism.md).
- [x] Check fresh-only endpoint convergence across all sixteen scenes and inspect registered photographic regions.
- [x] Finish workspace, strict Clippy, formatting, report and index checks. Eleven independent processor and eight GUI GPU tests passed. The failed cached comparison remains explicit.
- [x] Owner approved the deterministic recovery resolution and subsequent commit and merge. The separate Classic failure remains tracked. App replacement is not approved.

## Design

`Controls::selectivity: f32` stores 0–1 strength, defaulting to 0. There is no automatic processing state. Zero produces an effective cutoff of zero, disabling only the additional consistency filter. Positive strength maps to `0.005 / strength`, so 0.25, 0.5 and 1 reproduce the investigated cutoffs 0.02, 0.01 and 0.005. Extremely small positive strengths are capped only to keep twice the effective cutoff finite. Classic keeps its independent signed threshold.

Selection changes only the color-data term of affected grid equations. Keep the original guide for connectivity and for the existing neutral coefficient. Pass that coefficient explicitly in an 80-byte color equation row, while scalar brightness equations stay 48 bytes and compensated iterates retain all four lanes. Do not recompute the neutral coefficient from filtered evidence.

## Findings and checks

### Implementation and input migration

One Highlights row follows Method in all three choices. Classic keeps its existing threshold units and A behavior. Color Field has no A button. Strength is 0–1, with higher values more selective and zero as the default/reset value, preserving the original reconstruction. Switching methods preserves their independent stored parameters. No display-unit conversion is needed. Pointer tests cover Classic A, absence of color-field A, slider edits, reset, disabled interaction and checkbox enable/render requests across all three methods.

`Controls::selectivity` replaces the raw-cutoff interface throughout processing, CLI, sidecars, tests and the maintained measurement example. An input-only decoder converts saved raw cutoffs to equivalent strengths and discards the retired neutral-restraint field. Explicit strength settings, including retired null migrated to zero, take precedence. Serialization emits scalar strength and the CLI accepts only numbers, not `auto`. Serialization emits only current controls. Duplicate and unknown fields still fail validation. Tests confirm that loading migrated edits does not rewrite their files. The old neutral-bias arithmetic and CLI option are removed.

Variance work is disabled at strength 0. Manual selection filters only affected color-data terms. Original guides still own graph connectivity and the per-cell neutral coefficient. The color equation's extra vector carries that coefficient to the existing compensated product. Scalar packing, iteration caps and stopping bounds remain unchanged.

### Positive-strength refinement

The owner found the raw cutoff direction unintuitive for optional processing. The GUI and CLI now use 0–1 selectivity, with higher values stricter. The initial strength interface retained A as an alias for zero. The owner subsequently removed it: Color Field now defaults to scalar 0 with no A button. Classic's A remains unchanged. The reciprocal parameter mapping preserves the investigated filter: 0.25 maps to cutoff 0.02, 0.5 to 0.01, and 1 to 0.005. It approaches a permissive cutoff continuously near zero rather than jumping from bypass into strict filtering. This is a control-parameter conversion, not CPU image arithmetic.

The old cutoff field is accepted only for input migration and is not serialized. Explicit current null or zero wins over retired fields. CLI raw-cutoff and neutral-restraint options are rejected. Tests cover the mapping, bounds, tiny positive strengths without overflow, read-only migration, migration of retired automatic settings to zero and positive-direction pointer edits in all three methods. Shader arithmetic is unchanged from the validated raw-cutoff implementation. Removing A updated only its explanatory WGSL comment. The final scalar-zero simplification passed workspace and tool tests, strict Clippy, eleven independent processor GPU tests and eight GUI GPU tests. Pointer tests also verify that the checkbox still enables correction and requests rendering without an automatic button. The first Clippy pass identified an unnecessary optional-reference reborrow in that UI refactor; removing it cleared the check. The refined control passed workspace and tool tests, strict Clippy, all eleven independent processor GPU tests, all eight GUI GPU tests, formatting, indexes and all eleven report tests. The known cached comparison was explicitly excluded and has not been retried or relaxed.

### Independent validation

Eleven independent processor GPU tests passed. These include all sixteen full-sensor A comparisons against the identical-evidence CPU equations, with maximum normalized error approximately `8.85e-5` below the unchanged `2e-4` bound. New controlled GPU checks verify unchanged healthy pixels and brightness, unchanged consistent regions, smoothly selected mixed evidence and an exactly unchanged original neutral coefficient. The migrated checkpoint, compensation and 64-sweep fixtures passed. The shared DNG route retains a nonzero manual cutoff. All eight GUI GPU tests passed. Workspace tests, both measurement-tool tests, strict Clippy, formatting, documentation indexes and all eleven report tests passed.

Fresh-only convergence checks completed 64 renders at cutoff 0.005 and 64 at 0.02 across the sixteen-scene corpus and both methods, without logged errors. A JSON verification initially compared widened float32 values against Python float64 literals. Correcting that analysis check used the existing completed records, without rerendering them. Before the strength-display refinement, thirty-two additional fresh CLI RGB16 renders covered A and raw cutoffs 0.02, 0.0125 and 0.005 on skirt, petals, knitwear and skin, in both methods. The corresponding strengths are approximately 0.25, 0.4 and 1; retained measurement labels remain in their original units. Registered crops confirm the investigated partial reduction in bleeding and the per-image color tradeoff. These are not cached-output rechecks.

A preliminary interleaved A comparison with the frozen optimized renderer completed 160 fresh samples. Across five scenes, paired six-sample medians changed approximately −0.2% to +2.1% for Global and −1.1% to −0.05% for Local. This is a small feature-cost check, not a new optimization claim. No speed policy or solver bound was changed.

### Strict cached Output blocker

The dedicated cache test passed its Global cutoff 0.02/0.01/0.005 and restored-A comparisons, then failed on 0095 Local cutoff 0.02. Highlight layers differed by at most `0.045898438` calibrated codes. Output differed by `0.0011789203`, above the unchanged `0.0002` bound. The failing binary, source archive, sidecar bytes, checksums and log are retained. The actual intermediate image pair was not captured, so its extent and first divergent stage remain unknown. It must not be assumed to share the earlier single-pixel failure's cause.

Under the strength interface, the failed Local cutoff-0.02 case is selectivity 0.25 with the same effective processing parameters. This historical failure was subsequently captured, traced to a speckle-detection boundary and resolved by approved deterministic solver ordering. The [determinism plan](./color-field-determinism.md) records the exact corpus comparisons and unchanged numerical bounds. That test was not rerun. Eleven other processor GPU tests were run with this known failed comparison explicitly excluded and reported separately. No assertion was weakened, and later fresh-only success does not clear the failure. The [GPU output mismatch plan](../gpu-output-mismatch.md) records the unresolved issue.

### Disposition

The requested control and deterministic recovery are implemented for review. The Color Field cached Output failure is resolved with exact comparisons and unchanged bounds. Final integration remains incomplete because the latest full suite encountered the separate existing Classic recovery-threshold cache failure. No app bundle was replaced, installed or launched. No commit was made. Originals and owner sidecars were not rewritten.

## Closure

This completed implementation or experiment is archived during release 0.3.0 preparation at the owner's request. Earlier scope, review and working-tree statements describe that historical slice. Recorded limitations and separately tracked follow-ups remain explicit.
