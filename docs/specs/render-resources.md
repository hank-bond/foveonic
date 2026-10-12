---
summary: "Owned GPU render results separate image processing from display and export, with explicit pixel meaning and geometry."
kind: reference
status: accepted
implementation: conforming
---

# Render resources

## Requirements

- Rendering **must** provide an owned GPU-resident result separately from file export.
- Image resources **must** describe their dimensions, layout, and processing space. Source metadata and the selected rendering settings **must** remain distinguishable from current pixel meaning.
- Display and export **must** consume rendered resources without requiring CPU pixel readback for display.
- Retained results **must** remain unchanged by subsequent renders and scratch-buffer reuse.
- The engine **must** permit sharing the GUI's GPU device and queue.
- Scratch reuse **must** respect graph consumers and retained outputs.

Active-photo checkpoint caching follows [interactive rendering](./interactive-rendering.md). Resource ownership and processing boundaries must permit intermediate retention without treating retained storage as scratch. General multi-photo caching remains deferred. GUI toolkit, display color management, and DNG calibration decisions remain separate.

## Conformance

A result can outlive source decoding, feed GPU display preparation and TIFF export, and remain valid while another image is rendered. Graph branches preserve their inputs until all consumers have submitted their reads.
