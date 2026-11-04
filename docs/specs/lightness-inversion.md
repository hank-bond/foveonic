---
summary: "Tone, Saturation and Color Range omit the experimental lightness inversion effect."
kind: reference
status: accepted
implementation: conforming
---

# Lightness inversion

Tone, Saturation and Color Range must not expose lightness inversion. Remove the experimental saved setting, CLI flag, inspection outputs and GPU processing operation. Existing Hue, Saturation and Lightness adjustments retain their meanings. Obsolete photographic and color-range fields are ignored when loading existing edits, without a schema migration. They must not be written back.

The [completed plan](../../plans/archived/lightness-inversion.md) records the removal. Color-range selection research remains independent of this discarded effect.
