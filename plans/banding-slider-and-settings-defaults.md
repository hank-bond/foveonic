---
summary: "Add a five-stop column-banding slider and deserialize omitted image-edit fields from shared Rust defaults."
---

# Banding slider and settings defaults

The owner approved a snapping five-position slider and shared defaults for missing edit fields. Requirements: [column conditioning](../docs/specs/column-conditioning.md), [noise/detail controls](../docs/specs/detail-and-noise.md), and [editing sidecars](../docs/specs/editing-sidecars.md). The [verification plan](./reference-banding-verification.md) records native evidence and the unresolved application-export boundary.

## Completed work

- [x] Centralize neutral image settings in `Default`; constructors override only WB/mode. Apply Serde defaults to edit settings and nested tone/noise controls. Missing overrides remain linked. Keep workspace preferences separate.
- [x] Add column-banding presets 0–4, default 2, to shared controls, CLI, and change tracking. Select the HI coefficient and window together in both full/reduced paths.
- [x] Add the integer GUI slider under **Noise and detail**, with value editing, double-click reset, and help distinguishing it from **Row banding correction**.
- [x] Retain guarded native component samples for all five presets and keep the previous midpoint fixture.
- [x] Check older/partial sidecars, explicit values/nulls, invalid inputs, parameter isolation, UI snapping/reset, GPU arithmetic, cached renders, and recipe replay.
- [x] Complete scoped prose cleanup, formatting, Clippy, documentation generation/checks, and unit tests before the approved commit.

## Check results

Workspace unit/UI tests pass. Recipes lacking banding retain position 2; explicit edits and null choices survive. Invalid types, out-of-range values, duplicate/unknown fields, and unsupported versions remain errors. Preference tests still cover independent last-folder and workspace storage.

GPU outputs match all five guarded native component fixtures within the retained `0.12`-camera-code bound, preserving selected-window margins and immutable inputs. The old midpoint fixture passes gains 0/1/1.3. Parameter checks on six original photographs show that only column settings change across presets. Original-file cached-versus-fresh renders pass every nonmidpoint choice, and saved-recipe GPU replay is exact.

The [component tool](../scripts/verification/column_presets.py) records native identity and guards in `crates/foveonic/tests/data/column-presets.json`. Local check logs use the `banding-slider-*` prefix. One test-harness comparison was corrected to compare the same encoded Float32 representation rather than expanded values against their shorter JSON spelling; recipe decoding was unchanged.

## Boundaries and review

There is no migration pass, sidecar version bump, processing-default change, or installation. New saves record the effective settings explicitly. Default WB/mode stay unresolved until the existing per-photo selection path chooses them.

This is a color-path column estimator, not a multiplier on accepted row offsets; preset 0 is not Off. Matching application exports and full monochrome verification remain outside this slice. The owner reviewed the separate control meanings and approved committing banding/settings work after cleanup. Photographic review remains pending.
