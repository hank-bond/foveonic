---
summary: "Crop interaction separates pointer geometry and aspect-ratio preferences from the shared image view."
---

# Crop interaction

Keep crop gestures in oriented native coordinates. Pointer updates draw a draft rectangle and save photographic settings on release. Pixel sampling belongs to the processor's existing view and export paths. Use shared workspace widgets for toolbar controls and ratio dialogs.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[gesture.rs](./gesture.rs)** Resizes and moves crop rectangles in native coordinates within the shared framing plane.
- **[ratios.rs](./ratios.rs)** Presents shared crop constraints and stores named custom ratios in application preferences.

<!-- INDEX:END -->
