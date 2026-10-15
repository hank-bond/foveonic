---
summary: "Group editing corrections and share rounded clickable states without changing processing defaults."
---

# Editing layout and control states

Requirements: [GUI workspace](../docs/specs/gui-workspace.md) and [detail and noise](../docs/specs/detail-and-noise.md).

## Steps

- [x] Group Refinement and Corrections, shorten labels, and separate the two Advanced disclosures.
- [x] Share rounded button/checkbox styling and square, borderless dropdown cells.
- [x] Move the noise link explanation into tooltips.
- [x] Add button padding that scales with the font, keeping spacing between controls.
- [x] Apply the same font-relative padding to closed dropdowns, options, and Inspect disclosure rows.
- [x] Move ColorDQ gain to the top of Noise and Detail's Advanced subsection.
- [x] Test layout, disclosures, pointer/keyboard input, and saved choices.
- [x] Check workspace tests, strict Clippy, documentation, and the rebuilt macOS app.

## Findings and checks

The trial uses a 4-point corner radius and the existing blue selection hue. Linked values are neutral; unlinked overrides are blue. Checkboxes stay neutral and use checkmarks for on/off. Dropdown cells are square and borderless: blue when selected, dark gray otherwise, with whitish hover text and mid gray backgrounds for hovered unselected cells. egui handles input and layout behind the shared frames.

GUI tests pass: 90 passed, 7 GPU tests ignored. They cover group order, alignment, distinct tone/correction Highlights, independent disclosures, unchanged settings when collapsing, preserved saved choices, rounding, hover/press feedback, disabled clicks, keyboard activation, dropdown colors, gap-free cell width, and selection at cell edges. Pointer fixtures use ordinary frame timing so tooltips do not intercept clicks.

Workspace tests, strict all-target Clippy, formatting, whitespace, and explicit-root documentation checks pass. Native captures confirm the layout, neutral checkboxes and links, blue overrides, retained symbols, and removal of the inline note. The app was rebuilt and reopened for owner review. Evidence and failed checks remain in `.local/highlight-threshold/`.

## Internal button padding

egui removes padding from frameless buttons. Buttons now use an invisible egui frame for padded layout and keep the shared painted border. Minimum sizes still apply. The owner approved the appearance.

Tests cover padding at two font sizes and unchanged spacing between buttons and in the parent layout. GUI tests pass (97 passed, 8 GPU tests ignored), as do strict GUI Clippy, documentation checks and tool tests, and whitespace checks.

## Dropdown padding

White balance, Color mode, Inspect, and Scopes now use one shared dropdown helper. Closed faces use the button padding calculation. Options and Inspect disclosure rows calculate the same padding from their font sizes. Rows grow to fit the text and vertical padding while keeping gap-free, full-width menu cells. The helper restores parent spacing and retains existing dropdown and popup IDs.

Shape tests cover 14 pt and 20 pt fonts, enabled and disabled dropdowns, matching button heights, unchanged neighboring gaps, actual open-popup option padding, and disclosure-row padding. egui popup areas use the context font style rather than a local parent override, so popup tests set that shared font style. Existing pointer tests continue to cover selection, disclosure behavior, and disabled camera choices.

## ColorDQ placement

The owner moved ColorDQ gain to the first row in Noise and Detail's Advanced subsection, above the linked parameters. Its existing A toggle, manual range, automatic behavior, and stored setting are unchanged. The row is hidden with the collapsed subsection and remains unavailable when Noise and Detail is disabled. Tests cover collapsed visibility for automatic and manual settings, placement above linked controls, independent disclosure clicks, and unchanged settings when opening or closing the subsection.

Processing settings, recipes, and defaults are unchanged by these layout and styling changes.
