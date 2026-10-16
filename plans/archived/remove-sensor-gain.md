---
status: archived
summary: "Remove global Sensor gain and migrate local sidecars directly without a legacy reader."
---

# Remove Sensor gain

Requirements: [processing graphs](../../docs/specs/processing-graphs.md), [GUI workspace](../../docs/specs/gui-workspace.md), and [editing sidecars](../../docs/specs/editing-sidecars.md).

## Decision

The owner approved removing Sensor gain from the GUI, CLI, saved settings, and processing graph. Exposure remains the ordinary brightness control. Highlight correction has its own threshold slider. Camera-derived calibration and white-balance gains remain unchanged.

The owner requested direct migration of existing sidecar files, not a deprecation or compatibility path. Remove only `settings.sensor_gain`. Keep all other edits and the existing format version. Unknown settings remain errors.

## Steps

- [x] Commit the preceding work as `d58c541` before beginning removal.
- [x] Remove the shared setting, CLI option, GUI row/help, graph node, inspection output, and gain compensation.
- [x] Migrate all 21 discovered example-photograph and original-fixture sidecars.
- [x] Update help recipes, tests, and current requirements.
- [x] Check workspace tests, Clippy, formatting, documentation, and the macOS bundle.
- [x] Check the changed shaders against small GPU fixtures.
- [x] Obtain owner approval to commit and push removal.

## Migration

No app process was running during migration. Every discovered sidecar stored gain 1, so deleting the field does not change its appearance. Original X3Fs and all other sidecar fields were left untouched.

The shared sidecar decoder stays strict. There is no old-setting field, migration branch, or version bump in the application. Historical plans retain their earlier gain measurements as historical evidence. Help PNGs are unchanged because their recipes previously used gain 1.

## Checks

Workspace tests, strict all-target Clippy, formatting, documentation generation/checks, help-tool tests, and three bundle tests pass. The release bundle is rebuilt at `target/release/Foveonic.app`. A local probe validates and round-trips all 21 migrated sidecars with the strict application decoder.

Seven small GPU regressions pass for highlight restoration/thresholds, ordered despeckling, matched WB thresholds, row correction, signed color conversion, and retained image ownership. Noise parameter preparation still matches the retained reference fixtures on six originals.

The first highlight GPU check caught a now-unused uniform binding after removing gain normalization. Removing that binding from the donor-table dispatch fixes validation. The unchanged native fixture then passes. Full-photograph GPU rerenders and native pointer checks were not run for this change.

Existing optional performance goldens made with nonneutral Sensor gain are historical and cannot be reused as current pixel expectations. No goldens or photographic assets were regenerated.
