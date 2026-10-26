---
summary: "Original X3Fs and editing recipes in a visible source-folder subdirectory are authoritative."
kind: reference
status: accepted
implementation: conforming
---

# Editing sidecars

## Requirements

- The application **must** preserve per-photograph editing recipes in sidecars in a visible `foveonic` subdirectory of their original X3Fs' folder. The X3F and its sidecar **must** remain the authoritative inputs for re-editing and regenerating exports.
- Saving edits **must not** modify the original X3F.
- Sidecars **must** preserve photographic editing choices, including Tone and Color and Refinement bypass choices, independent Merrill Noise/Detail enable choices, detail amounts and sizes separately from workspace preferences and display-only inspection settings. Rendering mode and [monochrome mixing/noise settings](./monochrome-rendering.md) must persist alongside inactive color controls. Older recipes missing these fields must select Color and the shared default mono controls without a version bump.
- Recipes **must** use versioned JSON in that subdirectory, named by appending `.foveonic.json` to the complete X3F filename. Existing adjacent recipes **must** remain readable and move into the subdirectory on the next successful save. The subdirectory recipe takes precedence when both exist. Invalid older copies remain available for manual recovery.
- Omitted image-edit fields **must** use the shared Rust `Default` values, including nested tone/noise and texture controls. Supplied values **must** remain authoritative; explicit null **must** retain its optional-setting meaning. Invalid supplied values, duplicate/unknown fields, and unsupported versions **must** remain errors. Adding a field with unchanged default behavior **must not** require migration or a version bump.
- Neutral defaults **must** leave camera-dependent WB/mode unresolved for the existing per-photograph selection path. Application preferences **must** remain separate from image-edit settings and sidecars.
- The GUI **must** automatically restore saved recipes and save changes with atomic replacement. Saving **must** preserve the latest requested edits even when rendering is superseded or the user selects another photograph.
- Initializing a new photograph's defaults and switching preview quality or zoom **must not** create a sidecar or change its requested photographic settings. After an edit, the saved recipe includes all photographic controls, including the requested Noise and Detail selection.
- Explicit [Import mode](./import-workflow.md) must retain review edits in memory instead of saving source sidecars. Submitted import jobs freeze those edits and save them alongside copied destination originals. Ordinary Browse mode keeps automatic source-side persistence.
- Malformed, unsupported-version, or unreadable sidecars **must not** be silently replaced. Save/load failures **must** remain visible; writes **must not** block slider rendering.
- Exported DNG metadata **must** serve as processing documentation, not as the application's editing source of truth. DNG reimport, editing-state recovery, and metadata round-tripping through other editors are outside this workflow.

## Implementation direction

Use the shared processing settings as the version-1 recipe, preserving automatic choices as null and independent texture settings by name. `Settings::default()` owns neutral choices; `Settings::new(wb, mode)` replaces the camera-choice fields. Struct-level Serde defaults fill omitted fields independently at each nested settings level. Retired Merrill overrides and experimental mode flags are read only for migration and are omitted on save, as described in [detail and noise](./detail-and-noise.md#saved-settings-and-migration). Workspace preferences already have their own defaults and persistence. Explicit GUI launch WB/mode choices override the restored fields. No sidecar is created merely by viewing a photograph. CLI recipe-selection and override precedence remain decisions to review before CLI integration. The initial slice implements the shared recipe API and GUI save/restore.

## Conformance

Sensor gain was removed before external use. Existing local sidecars were migrated directly by deleting only that field, as recorded in the [removal plan](../../plans/archived/remove-sensor-gain.md). This field has no legacy reader. The adjacent-path reader supports the later sidecar-directory move without changing recipe contents.

An edited X3F can be reopened with its saved recipe and used to regenerate an export without reading an earlier DNG. Workspace preferences and inspection amplification do not change the recipe, and the original X3F remains unchanged.

The shared API and GUI slice implement these requirements. Automated checks cover exact round trips and GPU replay, protected malformed/future/unreadable recipes, coalesced latest writes, failed-save retry, photo switches, shutdown flushing, and inspection normalization. The owner accepted settings restoration across application restarts. CLI recipe selection remains outside this slice.
