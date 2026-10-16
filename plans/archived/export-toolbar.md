---
status: archived
summary: "Move TIFF color selection from Editing to a padded, grouped Open/Export toolbar."
---

# Refine the export toolbar

The owner requested more spacing in the Open/Export row and TIFF color selection in the save dialog if supported. They clarified that buttons keep standard workspace padding. Inner groups and the transparent, borderless outer container each use 4 logical pixels. Open/TIFF groups use the unselected dropdown-item gray background without borders. DNG remains an ungrouped button. File-count and Zoom rows must vertically center their labels and nearby buttons. The approved fallback groups the two Open buttons and places a four-option color-space dropdown beside Export TIFF, outside Editing.

Requirements: [GUI workspace](../../docs/specs/gui-workspace.md) and [output strategies](../../docs/specs/output-strategies.md).

## Steps

- [x] Check save-dialog support. The current rfd FileDialog API has file filters and standard dialog settings, but no custom accessory controls or selected-filter result.
- [x] Implement the approved toolbar fallback with nested padding, borderless gray inner groups, and unchanged button padding. Remove the editor selector.
- [x] Preserve selected export settings, sidecars, disabled states, and fresh export requests without rerendering for a destination-only change.
- [x] Check pointer interaction, layout, editor removal, GUI tests/Clippy, formatting, and indexes.
- [x] Complete ordinary workspace tests and all-target strict Clippy after concurrent resizing work compiles.
- [x] Balance the file-count/Zoom surrounding gaps, match their text tone, and size the TIFF dropdown for its longest option.
- [x] Size Inspect from available menu labels and its selected label, sharing font-aware sizing with TIFF.
- [x] Owner approved the reviewed changes for commit and requested a refreshed repository screenshot.
- [x] Refresh the screenshot with the documented capture task and review the generated image.

## Scope

Keep the existing Camera/default, sRGB, Adobe RGB, and ProPhoto RGB choices. Keep the native save dialog and shared TIFF processing unchanged. Do not introduce a platform-specific custom save panel or change button padding anywhere in the workspace.

## Implementation and checks

A shared frame supplies 4 logical pixels of padding. Open folder/Open X3F and Export TIFF/color space are two borderless inner groups filled with the shared unselected dropdown-item gray. The outer toolbar is transparent and borderless with the same padding. Export DNG has no surrounding group. Buttons, dropdown headers, and menu options retain the existing shared widget styling and padding. Inner toolbar rows top-align equal-height controls, while the outer row centers the groups and ungrouped DNG button. Organizer file-count and navigator Zoom rows reserve their full padded control height first, then vertically center labels and buttons. This avoids the first shorter label appearing above later buttons.

The TIFF selector moved out of Editing. Its value still lives in the shared recipe and reaches fresh export requests. The existing settings comparison queues sidecar saves, but toolbar destination changes do not signal photographic rerenders. Native file dialogs and worker exports are unchanged.

All 124 ordinary GUI tests, ordinary workspace tests, and workspace all-target strict Clippy pass. Pointer tests cover all four destinations, Open/TIFF/DNG actions, disabled states, preserved settings, and editor removal. Geometry checks cover standard button padding at two font sizes, aligned controls, exactly two borderless gray backgrounds, an undecorated outer container, equal inner/outer padding, centered file-count/Zoom labels and buttons from the first frame, matching leading/trailing band spacing at both display densities, and longest-option dropdown sizing across selections and font sizes. GUI formatting, generated indexes, and whitespace checks pass.

Initial broader checks were blocked by concurrent resize-test compilation. That work was left untouched and later compiled, allowing ordinary workspace tests and strict Clippy to pass. No GPU processing shader was changed in this toolbar slice. The owner accepted the top toolbar's grouping and padding. Follow-up screenshots showed bottom-heavy space around the file-count and Zoom rows. The shared row now adds a leading gap equal to the layout's automatic trailing item spacing, leaving the accepted toolbar padding unchanged. File count uses the same normal label tone as Zoom. TIFF dropdown width is measured from its longest label plus arrow and standard padding rather than a fixed width. Inspect uses the same measurement helper, accounting for stage labels, indented components, and its selected label. Font-size and color/monochrome tests check compact defaults and fully visible long component selections without changing inspection state. A follow-up screenshot showed the popup frame and scrollbar consuming the reserved disclosure space. Inspect now budgets both explicitly, and scrolling-popup tests verify that calibration labels stay clear of their arrows at both font sizes. The owner approved committing these refinements. `cargo xtask screenshot --offline` refreshed the README image from the unchanged recipe and existing seven-photo cache. The native 2650×2194 capture was reviewed and lossless WebP encoding verified by the task. No local sidecars or ordinary preferences were changed.
