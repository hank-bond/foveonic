---
summary: "Group editing corrections and share rounded clickable states without changing processing defaults."
---

# Editing layout and control states

Requirements: [GUI workspace](../docs/specs/gui-workspace.md) and [detail and noise](../docs/specs/detail-and-noise.md).

## Steps

- [x] Group Refinement and Corrections, shorten labels, and separate the two Advanced disclosures.
- [x] Share rounded button/checkbox styling and square, borderless dropdown cells.
- [x] Move the noise link explanation into tooltips.
- [x] Test layout, disclosures, pointer/keyboard input, and saved choices.
- [x] Check workspace tests, strict Clippy, documentation, and the rebuilt macOS app.

## Findings and checks

The trial uses a 4-point corner radius and the existing blue selection hue. Linked values are neutral; unlinked overrides are blue. Checkboxes stay neutral and use checkmarks for on/off. Dropdown cells are square and borderless: blue when selected, dark gray otherwise, with whitish hover text and mid gray backgrounds for hovered unselected cells. egui handles input and layout behind the shared frames.

GUI tests pass: 90 passed, 7 GPU tests ignored. They cover group order, alignment, distinct tone/correction Highlights, independent disclosures, unchanged settings when collapsing, preserved saved choices, rounding, hover/press feedback, disabled clicks, keyboard activation, dropdown colors, gap-free cell width, and selection at cell edges. Pointer fixtures use ordinary frame timing so tooltips do not intercept clicks.

Workspace tests, strict all-target Clippy, formatting, whitespace, and explicit-root documentation checks pass. Native captures confirm the layout, neutral checkboxes and links, blue overrides, retained symbols, and removal of the inline note. The app was rebuilt and reopened for owner review. Evidence and failed checks remain in `.local/highlight-threshold/`.

Processing settings, recipes, and defaults are unchanged by these layout and styling changes.
