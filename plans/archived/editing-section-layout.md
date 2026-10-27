---
summary: "Color Editor has an independent checkbox and editing sections persist their visual drag order without changing processing."
status: archived
---

# Editing section layout

Requirements: [GUI workspace](../../docs/specs/gui-workspace.md), [Color Editor](../../docs/specs/color-editor.md), and [processing choices](../../docs/specs/import-workflow.md).

- [x] Replace Color Editor disclosure with an independent checkbox; keep disabled controls visible.
- [x] Default Color Editor below Tone and Color across supported camera panels.
- [x] Share a smaller six-dot grip and typed drop handling within Editing.
- [x] Persist tool order separately from photo edits, retaining unavailable tools.
- [x] Keep numerical pipeline order fixed and bypass Color Editor consistently in display/export.
- [x] Validate drag interaction, saved layout, source-setting retention and output bypasses.
- [x] Run checks and rebuild the review app.

Family panels keep their capability-specific tools. Merrill Noise and Detail are separate tools; Classic and Quattro retain their existing combined reconstruction controls. Tool IDs remain stable while the display order changes. Editing and outer workspace sections use distinct payload types. Drop targets check payload type before taking it, because egui consumes mismatched payloads too. A regression fixture verifies an inner target leaves an outer drag available to its owner. The arranger computes insertion slots after drawing the visible tools, snapping each indicator to its actual divider. One editing drop zone has its own payload-specific identity. Drops that leave visible order unchanged are suppressed, including hidden-tool-only moves.


## Checks

Workspace tests and strict Clippy pass. Pointer interaction verifies real grip dragging and reordering without reporting a photographic edit. Preferences round-trip tool order and restore missing/duplicate entries while preserving earlier choices. Existing family-panel tests continue to preserve saved values. Color Editor flags serialize independently, default enabled for missing fields, and remain separate from Tone bypass and output masks. Original Merrill and Quattro checks verify neutral/bypassed equality, active Color Editor with Tone bypassed, output exclusion and retained authored values for both photographs and DNG.

Formatting, documentation indexes, artwork checks and whitespace checks pass. The optimized review bundle builds. Pipeline order remains fixed; only the visual section layout is persisted. Changes remain uncommitted for owner review.
