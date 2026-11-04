---
summary: "Remove the experimental lightness inversion controls, recipe fields and GPU operation after owner review."
status: archived
---

# Lightness inversion removal

The owner removed selective Color Range inversion and then global Tone inversion after photographic review. The [requirements](../../docs/specs/lightness-inversion.md) record that decision.

Remove the checkbox, saved setting, scoped copying, CLI flag, inspection aliases, GPU node, shader and obsolete inversion tests and native replay. Keep Advanced HSL and its full RGB Color DNG baking. Ordinary Monochrome lightness remains a scalar gain. Existing DNG basis mapping remains useful for other color transforms and Dehaze.

Validation covers workspace tests, GPU startup, Advanced lightness DNG reprojection in Color and Monochrome, shared control layout, native Adaptive rendering and the rebuilt review application.

Photographic controls and Advanced range records use ordinary serde unknown-field ignoring. This keeps other authored edits readable when an experimental field is removed, without storing the obsolete field or running a migration. Native review exposed the need and a saved-settings regression check covers it.
