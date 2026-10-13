---
summary: "Original X3Fs and adjacent editing sidecars are authoritative; rendered exports can be regenerated from them."
kind: reference
status: accepted
implementation: incomplete
---

# Editing sidecars

## Requirements

- The application **must** preserve per-photograph editing recipes in sidecars beside their original X3Fs. The X3F and its sidecar **must** remain the authoritative inputs for re-editing and regenerating exports.
- Saving edits **must not** modify the original X3F.
- Sidecars **must** preserve photographic editing choices and advanced override linkage separately from workspace preferences and display-only inspection settings.
- Recipes **must** use versioned JSON beside the input, named by appending `.foveonic.json` to the complete X3F filename.
- The GUI **must** automatically restore saved recipes and save changes with atomic replacement. Saving **must** preserve the latest requested edits even when rendering is superseded or the user selects another photograph.
- Malformed, unsupported-version, or unreadable sidecars **must not** be silently replaced. Save/load failures **must** remain visible; writes **must not** block slider rendering.
- Exported DNG metadata **must** serve as processing documentation, not as the application's editing source of truth. DNG reimport, editing-state recovery, and metadata round-tripping through other editors are outside this workflow.

## Implementation direction

Use the shared processing settings as the version-1 recipe, preserving automatic choices as null and advanced overrides by parameter name. Explicit GUI launch WB/mode choices override the restored fields. No sidecar is created merely by viewing a photograph. CLI recipe-selection and override precedence remain decisions to review before CLI integration. The initial slice implements the shared recipe API and GUI save/restore.

## Conformance

An edited X3F can be reopened with its saved recipe and used to regenerate an export without reading an earlier DNG. Workspace preferences and inspection amplification do not change the recipe, and the original X3F remains unchanged.
