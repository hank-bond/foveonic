---
summary: "Remove exploratory color-field paths, resolve the strict Output blocker, and prepare reviewed integration onto current main."
---

# Color-field production integration

## Approved scope

The owner approved production cleanup and integration into main. Keep Classic, Color Field: Local and Color Field: Global, the shared Highlights row, Classic-only A, and scalar color-field selectivity defaulting to 0. Preserve the [accepted equations and numerical requirements](../docs/specs/color-field.md). Remove exploratory processing variants and tuning flags. Keep regression tests and the attributed test-only reference oracle. Retain private investigation evidence without shipping its diagnostic hooks.

The owner reviewed the candidate and validation summary and approved committing, merging into main and removing the temporary worktree. Preserve retained originals and investigation evidence outside the removed worktree. Do not install, launch or publish the app.

## Steps

- [x] Audit branch state and compare its base with current main.
- [x] Capture and explain the strict Color Field Output failure, then resolve it through approved deterministic ordering without weakening assertions.
- [x] Remove alternate grids, active-area recovery, anchors-only, uniform-support and relaxed-solver paths, plus exploratory donation/agreement/boundary controls.
- [x] Keep only supported brightness selection and color selectivity in the production configuration. Migrate valid saved prototype choices on input without retaining obsolete processing paths.
- [x] Simplify CLI, GUI presets, measurement tools and tests together.
- [ ] Consolidate current guidance and archive completed experiment plans while retaining limitations and attribution.
- [ ] Validate the production-only source with numerical, cache, DNG, full corpus, GUI, workspace, Clippy and documentation checks.
- [x] Prepare an isolated uncommitted candidate on main `6574907`, preserving TIFF color, resizing, Auto sharpening and GUI changes.
- [x] Present the cleaned changes and validation for owner review. The owner approved commit, merge and worktree removal with the remaining Classic failure documented.

## Audit findings

The branch is based on `ea4a44c` and all feature changes remain uncommitted. Local main is at `6574907`, six commits ahead of that base. It contains TIFF color-space selection, reference resizing, Auto sharpening and export UI changes that overlap this branch. Main is checked out in a separate owner worktree, so integration must not silently move its ref or alter that checkout.

Exploratory production paths remain in `Approach`, `WorkArea`, donation/agreement/boundary controls, CLI flags, GPU uniform options, and the comparison example. The intended production algorithm is the full-sensor 8-pixel grid, full donation, original agreement constants, fixed original solver limits/tolerances, and Global or exactly 64-sweep Local brightness solving.

The [strict cached Output failure](./gpu-output-mismatch.md) remains a blocker. Its current equivalent setting is Local selectivity 0.25. No failing intermediate pair was captured previously. The first diagnostic campaign will capture the actual compared stages in a bounded sequence. Requesting intermediate artifacts extends buffer lifetimes, so any changed observation must be reported rather than treated as proof of a fix.

## Diagnostic outcome

The first setup attempt requested both final and its underlying tone output, which are aliases. The engine correctly rejected duplicate output requests. That incomplete setup is retained separately. One corrected, bounded staged campaign completed all eight comparisons: Global and Local at selectivity 0.25, 0.5, 1 and restored 0. Maximum final Output differences were approximately `3.7e-6` through `9.0e-6`, below `2e-4`. The actual compared stage summaries and maximum-location patches were retained. No failure-triggered rerender or repeat-until-pass sequence was used.

No failing pair was captured. Additional intermediate outputs extended GPU buffer lifetimes and may change the observation. This campaign does not explain or clear the historical failure. The temporary diagnostic source and executable were moved out of maintained examples and retained privately.

## Cleanup validation

The production configuration now contains only brightness selection and selectivity. Retired tuning and area fields are input-only migration. Obsolete approach names map to supported Global brightness, while Local remains Local. Tests verify that serialization emits only the two supported fields. Removed CLI flags are rejected instead of retained as aliases.

The comparison example now renders only the supported four recipes. Its preview option is `--previews`, applies uniformly, and rejects unknown variant selections. The renderer no longer contains donation-zero bypass, anchors-only, optional donor support or relaxed tolerance paths. Iteration caps, true-residual checks, compensated vectors and Local's 64 sweeps are unchanged.

Workspace, tool and strict Clippy checks passed. Eleven independent processor GPU tests passed, including full-raster numerical acceptance. Eight GUI GPU tests passed. The known strict cached comparison was explicitly excluded from these independent checks and was not rerun into a pass. The later main-based candidate and full validation are recorded below. No commit or merge has been made.

## Main integration and full validation

An isolated candidate applies the cleaned feature to main `6574907`. Both sides of the six textual conflicts were preserved, including output resize kernel registration, TIFF color settings, Auto sharpening and prior mismatch findings. Generated indexes were regenerated. Two new GUI tests initially selected Sharpening's A button. Restricting them to the Highlights row fixed that integration-specific test issue without changing either control.

Workspace tests and strict all-target Clippy pass. The full serial core GPU run executed 87 tests without exclusions: 83 passed and four failed. The separate GUI GPU suite passed all eight tests. The failures are:

- Color Field Local, selectivity 0.25: highlight-layer maximum `0.048828125` codes and Output maximum `0.0011783242`, above the unchanged `0.0002` bound.
- Existing exact interactive regression: red flower step 1, 22,349,169 differing components, maximum `0.035657346`.
- Existing noise/override exact regression: edge-scale 0.25, movement 1, 11,208,848 differing components, maximum `0.016362906`.
- Exact sidecar replay: the shared editing fixture now selects Color Field, whose independently solved floats are not bit-exact. Serialization equality tests pass. The original exact replay fixture selected Classic. No assertion was changed during this diagnostic pass.

The failing binary, source and log were frozen. No failure was retried into an apparent pass.

## Captured Color Field cause

One subsequent diagnostic added only failure-triggered test readbacks, without requesting extra stages or rerendering after failure. It reproduced and captured the actual failing pair. Their readbacks remained byte-identical when read again. Exactly one physical pixel, `(4544, 949)`, exceeded the Output bound.

At that pixel the recovered blue values were `5446.8173828125` and `5446.8212890625`, a difference of `0.00390625` calibrated codes. The downstream speckle detector's diagonal-minus-threshold boundary was `5446.818359375`. One value passes its strict minimum test and the other does not. Consequently one run repairs blue to `5442.02880859375`, while the other keeps `5446.8212890625`.

A diagnostic imported each captured highlight buffer into the unchanged downstream GPU graph, bypassing recovery rather than recomputing it. Both complete downstream outputs matched their respective actual failing outputs byte-for-byte. The first amplification is therefore established at speckle detection, not inferred from a later passing run. The final maximum difference is `0.0011783242` at the same pixel.

This explains the reproduced Color Field case, not the other exact-cache failures. Recovery uses unordered atomic compaction for both native and color unknowns. Different row order permits float32 CG reduction differences. The owner approved deterministic GPU compaction. It is now implemented and its original strict comparison, complete corpus exactness and unchanged numerical bounds pass. The [determinism plan](./archived/color-field-determinism.md) records the implementation and performance cost. No regression contract or Classic detector was changed. Temporary capture and replay hooks were removed from maintained source and retained privately. The latest full core run passed 88 of 89 checks. Its remaining failure is the separate existing Classic recovery-threshold comparison, not Color Field.
