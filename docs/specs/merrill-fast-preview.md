---
summary: "Merrill Fast uses a filtered quarter-pixel grid and Smart delivers native viewport detail before the whole frame."
kind: reference
status: accepted
implementation: conforming
---

# Merrill Fast preview

The viewport must offer Full, Fast and [Smart](./smart-preview.md) for supported Merrill color photographs. Availability must resolve from early capture properties and refine from complete metadata. Preserve the stored application-wide choice while loading or viewing unsupported sources. Full remains the initial workspace default.

Fast must preserve native framing and orientation while normally producing half the full width and height. It must filter the calibrated layers before reducing them, rather than selecting individual sensor pixels. Source metadata and decoded samples remain immutable. Native calibration, camera repair, highlights and requested despeckling remain enabled. Fast must omit [row correction](./row-banding.md) while preserving its requested setting for Full. Camera color, tone and separate sharpening run on the reduced grid with recipe-owned working geometry.

Fast must bypass the optional Noise and Detail reconstruction and ColorDQ filtering, including explicit advanced ColorDQ choices. This applies to standalone Fast and Smart's reduced overview. The requested checkbox and controls must remain unchanged in the interface, retained artifacts and sidecars. Full refinement must apply those controls when enabled. Keep the current despeckle choice. Disabling it for Fast remains a separate experiment.

Merrill Smart must initially show Fast. Its first zoom above 100% must process the viewport and its required surroundings before immediately continuing whole-frame Full. Select the existing native processing tiles, retaining their filtering context, column alignment and seam blending. Whole-frame sensor corrections and any reduced whole-image noise context may precede the regional work. Retain these immutable prerequisites for the Full continuation within the existing session budget. Panning must not request more regions. Once delivered, retain Full during panning and returning to Fit. Changing edits or source resets the refinement sequence, as for Quattro. Do not publish a whole-frame render as a regional result. Explicit output resizing may use direct whole-frame refinement until its regional sampling phases are supported.

Native-image zoom, viewport coordinates, scopes and navigator pairing must stay consistent through quality changes. Inspection, comparisons, monochrome and TIFF/DNG export use Full. Preview policy must remain outside image sidecars, and reduced images must remain immutable while Full or edits run.

The recipe must reject unsupported reduced geometry without changing framing. Complete metadata must disable reduced preview when its sensor dimensions or crop cannot be represented exactly. The [implementation plan](../../plans/archived/merrill-fast-preview.md) records geometry checks, photographic comparisons and completed timing evidence. The [Noise and Detail follow-up](../../plans/archived/merrill-fast-noise.md) records the optional reconstruction bypass and its validation.

The [regional implementation plan](../../plans/archived/merrill-regional-preview.md) records dependency analysis, equality checks and latency measurements.
