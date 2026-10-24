---
summary: "Explicit native display experiments validate presentation independently of photographic processing."
---

# GUI experiments

These examples run only when explicitly requested. Keep their preference storage isolated from the main application. Close only the subprocess created for the check.

Run the managed-color probe with `cargo run --offline -p foveonic-gui --example managed_color_probe` on macOS. It compares retained Adobe RGB colors with an sRGB-clipped row and a neutral ramp while preserving ordinary UI blending. `FOVEONIC_DISPLAY_PROBE_EXIT_MS` closes its own window automatically for native checks. The probe fails clearly if floating managed presentation is unavailable.

The application uses managed floating-point presentation by default on macOS, with a tagged sRGB fallback when that format is unavailable. Viewport, navigator and browser previews follow the same presentation contract. The [plan](../../../plans/display-color-management.md) records integration and validation.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[managed_color_probe.rs](./managed_color_probe.rs)** Compares extended photographic colors, sRGB clipping, neutral ramps and ordinary egui output in a managed window.

<!-- INDEX:END -->
