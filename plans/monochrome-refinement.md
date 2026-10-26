---
summary: "Track the deferred monochrome color-sensitivity improvement and independent reconstruction-detail controls."
---

# Monochrome refinement

Requirements: [monochrome rendering](../docs/specs/monochrome-rendering.md) and [photographic finishing](../docs/specs/photographic-finishing.md).

The owner requested these follow-ups after reviewing shared photographic finishing. They are deferred, not part of the current cleanup.

- [ ] Improve monochrome color sensitivity through photographic review of the source-color guide, hue selection, protection and amount response.
- [ ] Add an independent detail-restoration stage to monochrome noise reconstruction. The current dedicated mono denoiser has no equivalent to Merrill color's Fine/Coarse detail controls.

Preserve authored sensor-layer mixing and keep Noise, Detail, Local Contrast and Sharpening distinct. Evaluate placement, controls, defaults and supported camera families before implementation. Local Contrast is an accepted standalone photographic effect, not a substitute for reconstruction detail.
