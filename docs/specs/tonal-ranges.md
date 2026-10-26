---
summary: "Experimental Merrill HDR controls use a multiscale spatial base for preview and TIFF without affecting DNG."
kind: reference
status: accepted
implementation: conforming
---

# Tonal ranges

## Experiment

Merrill color and monochrome must offer Blacks, Shadows, Highlights and Whites controls. After rejecting the first darktable-inspired response, the owner selected a replacement grounded in Capture One HDR research. The owner reviewed Global/Spatial and selected the spatial approach. Global must be removed from the UI and processing choices.

Controls range from -1 to +1. Positive values brighten the selected range and negative values darken it. Values are normalized amounts rather than stops. Zero adjustments must preserve existing camera appearance exactly and skip the additional photographic work. Input fields must fit the controls' complete range without resizing the editor panel when values change or the pointer moves. Earlier percentage-based experimental sidecars must be read into the normalized spatial recipe without changing their amount strength. New saves must identify the normalized units.

HDR must derive a multiscale edge-preserving brightness base and preserve local detail through the coordinated response. Its tonal pivot must follow our selected camera curve and Exposure. Blacks must account for the shadow response and Whites for the highlight response. Existing camera-curve settings must remain available without silently changing saved recipes.

Photographic pixel arithmetic must run on the GPU. Inspection must expose the brightness base and adjusted linear RGB. Amount changes must reuse unchanged inputs and the spatial base through the existing bounded render session. Fast and regional Smart rendering must preserve framing and use consistent spatial support. Source samples and retained outputs must remain immutable. The mathematical response may follow the researched HDR operation without claiming complete Capture One appearance or bitwise spatial-filter equivalence.

## Export boundary

The experiment includes viewport rendering and TIFF. HDR preview edits must not affect DNG pixels or DNG appearance metadata, and must not block DNG export. The existing DNG pipeline must retain its current treatment of camera color mode and other previously supported settings. Spatial HDR must not be baked into the samples. Its omission from DNG must be explained in control help.

The owner values the HDR mapper as a preview of possible further editing in another editor. It does not require adding Dehaze or committing to baked HDR adjustments in editable DNG output.

## Evaluation

Review portraits, bright skies, textured objects and deep shadows. Check exact neutral output, range selectivity, native response samples, finite signed/headroom samples, region/full consistency, cached/fresh equality, and completed rendering cost. The spatial response has owner photographic approval. Further tuning remains possible.

Implementation, source-study findings and measured limitations belong in the [experiment plan](../../plans/tonal-ranges.md).
