---
summary: "Tiger uses its native calibrated single-frame route while sharing proven numerical operations with Quattro."
kind: reference
status: accepted
implementation: conforming
---

# Tiger rendering

Support ordinary full-resolution sd Quattro H color and dedicated monochrome processing, photographic controls, inspection, TIFF, and editable DNG. Preserve the existing [output contract](./output-strategies.md). Decode caching and asynchronous photographic previews must retain the shared application policy.

Use Tiger's metadata-derived digital gains, clipping, calibration and repair selection, dynamic-range extension, highlight masks, color preparation, reconstruction controls, and final active crop. Share numerical operations only where arithmetic, units, rounding, and borders agree. Camera decisions remain recipe-owned. Original decoded samples remain immutable. Pixel processing and image-derived statistics belong on the GPU.

Validate full-frame memory requirements and adapter limits before enabling final output. Retain explicit failures for unsupported routes. Independently checked bounded reference operations must be joined and tested against original-file boundaries. Existing Classic, Merrill, and ordinary Quattro output must remain unchanged.

Tiger color previews must support the shared [Fast](./quattro-fast-preview.md) and [Smart](./smart-preview.md) policies. Preserve native calibration, full-frame highlight statistics, and recovery-mask dependencies while reducing overview reconstruction and finishing. A Full viewport refinement must match the corresponding Full image crop and immediately continue to the whole Full frame. Reduced-resolution originals, SFD, Retinex and advanced tone curves remain separate work. The subsequent consolidation pass should identify shared contracts before comparing optional methods on ordinary Quattro. It must preserve camera-specific calibration.
