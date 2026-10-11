---
summary: "A compact image information bar shows capture settings, filename, and visible-region dimensions and coordinates for photographic review."
kind: reference
status: accepted
implementation: conforming
---

# Image information bar

## Agreed requirements

- The GUI **must** show a compact bar directly below the main image viewport.
- The left side **must** show available shutter, aperture, focal length, ISO, and exposure compensation. Formatting follows [GUI workspace](./gui-workspace.md).
- The middle **must** show the image filename.
- The right side **must** show viewport dimensions and the visible region's top-left and bottom-right locations, allowing the owner to identify a region from a screenshot.
- Information **must** describe the image currently shown, including the selected inspection buffer, rather than a newer request still processing.
- Coordinates **must** refer to the displayed cropped image area, with orientation applied, rather than the full sensor readout. Use image-pixel coordinates relative to that image's top-left origin; panning reports a different visible subset without moving the origin.
- Region reporting **must** account for pan, zoom, crop, rotation, and display scaling. Screen/UI coordinates and image-pixel coordinates **must** be distinguished.

## Coordinate readout

The right readout uses `W×H · (x0,y0)–(x1,y1)`: 0-based integer image-pixel indices, inclusive at both ends. Width and height count the visible pixels, including partly visible boundary pixels. Letterboxing is excluded. A fitted image reports its whole crop; zoom and pan report the corresponding subset. An entirely off-canvas image has no visible-region readout.

The filename remains at the true center. Side readouts take priority over filename width; clipping keeps the three areas separate on narrow panes. Hovering the bar exposes the full filename, metadata, coordinates, and bound convention.

## Conformance

Unit checks cover photographic formatting, fit/zoom/pan bounds, off-canvas handling, and portrait extents. Native captures cover six originals, layer/RGB views, repair settings, and magnified regions. An isolated restart reopens the saved folder with neutral photographic settings. See the [editor review plan](../../plans/editor-review-focus.md).

## Visual reference

The owner supplied a dark, single-row bar with metadata left and filename centered. Use viewport reporting on the right rather than the reference's rating controls:

`/var/folders/gy/q6q29v292flbjxhnv2kzvkfm0000gn/T/pi-clipboard-96275a40-f955-4abd-b21e-bef44e8160f9.png`.

The related defect investigation and pending implementation decisions are tracked in the [editor review plan](../../plans/editor-review-focus.md).
