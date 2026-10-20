---
summary: "Expose one per-image highlight color restraint adjustment and remove rejected shader experiments while preserving optimized Local/Global defaults."
---

# Highlight color restraint

## Current direction

The owner subsequently clarified that the intended control selects compatible color evidence rather than applying an added neutral bias across reconstructed pixels. This implementation is not accepted as that control. The [selectivity investigation](./color-field-selectivity.md) tested the corrected direction with neutral restraint off. The owner then selected a consistency cutoff, tracked in the [unified-control plan](./unified-highlight-control.md). The neutral-bias implementation is removed and its saved field migrates to A on load. Preserve the findings below as historical evidence, not as the current processing path.

## Original approved scope

The owner approved a shared per-image restraint slider and cleanup of rejected experiments. A useful adjustment can trade less skirt bleeding for less recovered flower or skin color. It does not need one setting that improves the entire corpus. Follow the [color-field requirements](../docs/specs/color-field.md) and retain the [bleeding investigation findings](./color-field-bleeding.md).

Use the tested final native-fit neutral penalty, not a second recovery implementation or CPU correction. Zero preserves the optimized baseline. Use a finite 0–8 adjustment with intermediate settings checked photographically. Keep brightness reconstruction, Local's 64 sweeps, convergence bounds and healthy pixels unchanged. Replace the two ordinary GUI agreement/boundary sliders with this single adjustment. Preserve investigation flags, numerical oracle coverage and retained evidence. Do not commit, install, launch or replace the running app.

## Steps

- [x] Restore baseline grid gathering, connectivity, weighting and neutral equations. Remove rejected compile-time experiment hooks.
- [x] Add bounded serialized restraint with zero default, CLI selection and native WGSL application.
- [x] Replace GUI tuning rows and help. Preserve restraint across method switches, enable changes and sidecar round trips.
- [x] Migrate sensitivity tooling to intermediate restraint values and fresh/cached output comparisons.
- [x] Check default numerical acceptance, healthy samples, brightness invariance, cache invalidation and DNG routing. Broader Output comparisons retain two unresolved single-pixel outliers.
- [x] Inspect intermediate settings on skirt stripes, petals, knitwear and skin in both methods.
- [x] Complete formatting, workspace tests, strict Clippy and documentation/report checks. Eleven processor and eight GUI GPU tests passed.
- [ ] Superseded tuning acceptance: resolve the requested color-selectivity design before promoting a slider. The Output differences remain unresolved.

## Design

Store the restraint amount in the unused fourth component of the operation's neutral parameter vector. It participates in the existing prepared-operation cache key without changing uniform size or solver parameters. Add its neutral-metric penalty only to the final per-pixel color fit. The grid and both brightness methods remain unchanged. A zero penalty retains the original expression.

The GUI shows Color restraint immediately under the highlight method, with help identifying it as Highlight color restraint. There is no tuning disclosure for a single adjustment. Investigation agreement/boundary values remain independent and do not become a second ordinary editing path.

## Findings and checks

### Implementation

The final native fit uses the stored restraint amount as an extra neutral-metric penalty. No CPU photographic correction, buffer allocation or solver change was added. Grid gathering and the solver shader match the frozen optimized source. Removed hooks cover donor variance, donor purity rejection, affected-cell tone gating, alternate grid neutral weights and distance falloffs. Retained exploratory artifacts were moved out of the source directory into ignored evidence storage rather than deleted.

The GUI shows one Color restraint row for either color-field method, disabled with highlights off. Double-click resets zero. Pointer tests cover amount-only edits, render requests, reset and disabled interaction. The CLI accepts `--color-field-restraint`. Nested sidecars default missing restraint to zero and round-trip nonzero amounts. Validation rejects nonfinite values and amounts outside 0–8.

### Intermediate settings

The four-scene sweep recorded 56 fresh/cached comparisons, comprising 112 renders, and eight byte-identical retained layer/Output pairs. Both methods were compared at 0, 0.25, 1, 2 and 8, with repeated zero and a return to zero. Native registered crops show progressively less yellow in the skirt highlights, with a corresponding increase in pale flower, yellow-knit and skin highlights. Even 0.25 has a visible cost on the magenta petals. This is useful per-image latitude, not a new default or automatic correction. Local and Global have the same qualitative adjustment. Strong settings cannot selectively distinguish white fabric from genuine yellow.

### Acceptance and unresolved output differences

All eleven processor GPU tests passed, including all sixteen full-sensor identical-evidence oracle comparisons. The maximum normalized zero-restraint image error was approximately `8.85e-5`, below the unchanged `2e-4` bound. The synthetic adjustment test checks unchanged healthy samples, brightness invariance and increasing neutral restraint. The dedicated two-method cache test checks both highlight layers and Output, retained-image immutability, returning to zero, and Classic/disabled bypass. DNG retains nonzero restraint. All eight GUI GPU tests passed. Workspace tests, strict Clippy, formatting, index generation/checks, nineteen documentation-tool tests and eleven report tests passed. All sixteen originals and fourteen frozen sidecars still match their retained SHA-256 values.

The broader photographic measurements found two Output outliers despite close agreement in the pre-color highlight layers. Each is confined to one native pixel outside the registered priority crops. The 0170 Global restraint-1 maximum is `0.0019752979` RGB at (130, 3059). The 0095 Global restraint-0.25 maximum is `0.0405142903` at (1687, 1794). These are not counted as passing cache comparisons. No retries or tolerance changes were applied. The first divergent downstream stage and any relationship to the older cache issue remain unknown. Details belong to the [GPU output mismatch investigation](./gpu-output-mismatch.md).

The control is implemented for review, but broad production acceptance remains limited by those unexplained differences. No app bundle was replaced, installed or launched. No commit was made.
