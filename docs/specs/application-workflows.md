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
- The GUI **must** offer photographic processing controls broadly comparable to the reference app in `~/git/foveonic`.
- The GUI and CLI **must** use the same processing engine and interpretation of supported settings.
- Image processing **must** use GPU shaders where feasible.
- Processing **must** follow [`processing-graphs.md`](./processing-graphs.md), with outputs defined by [`output-strategies.md`](./output-strategies.md).

## Conformance

A Merrill photograph can be processed through the GUI and CLI using equivalent settings. A batch can produce either supported output type through the shared engine.

## Implementation latitude

The initial end-to-end slice may expose a small subset of controls and omit detail recovery, noise suppression, and highlight reconstruction. Supported controls and algorithms expand incrementally.
