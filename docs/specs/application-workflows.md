---
summary: "A personal Merrill X3F app shares processing settings and one image engine between its GUI and batch CLI."
kind: reference
status: accepted
implementation: incomplete
---

# Application workflows

## Requirements

- The application **must** process Merrill-architecture X3F files.
- The application **must** provide a GUI and a CLI supporting batch processing.
- The GUI **must** offer photographic processing controls broadly comparable to the reference app in `~/git/foveonic` and follow [GUI workspace](./gui-workspace.md) requirements.
- Selecting an X3F in the GUI **must** restore its saved [editing recipe](./editing-sidecars.md), or initialize white balance and color mode from that file's recorded settings when no recipe is saved, and render automatically. Explicit launch overrides **may** replace those choices, and the controls **must** remain editable. Missing or unsupported choices **must** require manual selection.
- Replacing the selected image's embedded preview with its pipeline image **must** preserve the image rectangle and pan/zoom when their presentation geometry matches. Native-pixel zoom and image placement **must** use the image's presentation dimensions.
- The GUI and CLI **must** use the same processing engine and interpretation of supported settings.
- Image processing **must** use GPU shaders where feasible.
- Processing **must** follow [`processing-graphs.md`](./processing-graphs.md), with outputs defined by [`output-strategies.md`](./output-strategies.md).

## Conformance

A Merrill photograph can be processed through the GUI and CLI using equivalent settings. A batch can produce either supported output type through the shared engine.

## Implementation latitude

The initial end-to-end slice may expose a small subset of controls and omit detail recovery, noise suppression, and highlight reconstruction. Supported controls and algorithms expand incrementally.
