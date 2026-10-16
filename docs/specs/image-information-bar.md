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
- On image selection, capture highlights **must** be read independently of sensor-layer decoding and rendering, so they can appear beside the selected filename while its preview or loading view is shown. Metadata requests **must** retain only the latest pending selection and reject replies from earlier selections, including reselecting the same file. Early properties **must not** be used as complete processing metadata.
- The middle **must** show the image filename.
- Capture settings, filename, and visible-region readouts **must** be text selectable and copyable using the normal platform copy shortcut, without making them editable.
- The right side **must** show viewport dimensions and the visible region's top-left and bottom-right locations, allowing the owner to identify a region from a screenshot.
- Information **must** describe the image currently shown, including the selected inspection buffer, rather than a newer request still processing.
- Coordinates **must** refer to the displayed cropped image area, with orientation applied, rather than the full sensor readout. Use image-pixel coordinates relative to that image's top-left origin; panning reports a different visible subset without moving the origin.
- Region reporting **must** account for pan, zoom, crop, rotation, and display scaling. Screen/UI coordinates and image-pixel coordinates **must** be distinguished.

## Coordinate readout

The right readout uses `W×H · (x0,y0)–(x1,y1)`: 0-based integer image-pixel indices, inclusive at both ends. Width and height count the visible pixels, including partly visible boundary pixels. Letterboxing is excluded. A fitted image reports its whole crop; zoom and pan report the corresponding subset. An entirely off-canvas image has no visible-region readout.

The filename remains at the true center. Side readouts take priority over filename width; clipping keeps the three areas separate on narrow panes. Hovering the bar exposes the full filename, metadata, coordinates, and bound convention.

## Conformance

Pointer-driven tests drag-select and copy each of the three readouts, including the full coordinate string. Property-only read fixtures and background-reader tests cover bounded input reads, latest-selection coalescing, clearing, and same-file reselection. Six original files have identical capture properties through the early and full-source paths. A native loading-view capture shows all five highlights before processing replies are displayed; see the [metadata-first plan](../../plans/archived/metadata-first-loading.md). Unit checks cover photographic formatting, fit/zoom/pan bounds, off-canvas handling, and portrait extents. Native captures cover six originals, layer/RGB views, repair settings, and magnified regions. An isolated restart reopens the saved folder with neutral photographic settings. See the [editor review plan](../../plans/editor-review-focus.md).

## Visual reference

The visual reference showed a dark, single-row bar with metadata left and filename centered. Use viewport reporting on the right rather than the reference's rating controls.

The related defect investigation and pending implementation decisions are tracked in the [editor review plan](../../plans/editor-review-focus.md).
