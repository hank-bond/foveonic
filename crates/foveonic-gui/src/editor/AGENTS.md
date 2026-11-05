---
summary: "Editor workflow separates scoped settings, application presets and defaults from authored photos and transient comparisons."
---

# Editor workflow

Follow the [editor requirements](../../../../docs/specs/editor-workflow.md). Tool identities and recipe scopes share one definition. Applying a tool preset changes only its photographic fields. Application presets/defaults stay in preferences. Authored photo recipes remain in the existing sidecar session.

Reset and comparison baselines must come from the camera adapter. Keep display-only comparison state outside photo settings, processing snapshots and preview publication. Preserve user-authored As Shot choices when render-time camera choices resolve.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[actions.rs](./actions.rs)** Applies queued editor actions through shared recipe scopes and the existing editing session.
- **[advanced_color.rs](./advanced_color.rs)** Stores transient color-tab selection and range-name editing apart from saved adjustments.
- **[before.rs](./before.rs)** Keeps transient camera/default renditions separate from authored edits and photographic preview publication.
- **[dehaze.rs](./dehaze.rs)** Tracks automatic shadow colors and asynchronous patch picking while rejecting stale replies.
- **[direct_color.rs](./direct_color.rs)** Routes on-image color gestures into existing range sliders using asynchronous guide picks.
- **[direct_color_settings.rs](./direct_color_settings.rs)** Stores direct-color gesture preferences and draws the eyedropper and configuration popover.
- **[menus.rs](./menus.rs)** Collects scoped recipe actions from header buttons and a named-preset dialog.
- **[mod.rs](./mod.rs)** Owns editing-tool identities, scoped recipes and application presets separately from rendered images.
- **[recipes.rs](./recipes.rs)** Stores named scoped presets and camera-family defaults in application preferences.
- **[sections.rs](./sections.rs)** Maintains visual editing-tool order using processing-owned photographic section identities.
- **[white_balance.rs](./white_balance.rs)** Selects a neutral patch asynchronously and keeps Custom balance separate from camera presets.

<!-- INDEX:END -->
