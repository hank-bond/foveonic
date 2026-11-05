---
summary: "Unify Crop and Rotate presentation while keeping their drag controls independent."
status: archived
---

# Shared framing plane

Implement the owner's requirement that Crop and Rotate use the same UI plane and differ through their drag controls. Follow the [crop](../../docs/specs/cropping.md) and [rotation](../../docs/specs/rotation.md) requirements.

- [x] Replace separate crop/rotation previews with one untrimmed framing preview.
- [x] Share crop shading, frame, handles and dimensions.
- [x] Verify rotated crop gestures and switching without preview replacement or viewport reset.
- [x] Apply line-and-release behavior to Straighten using the reference labels. Keep Freehand live.
- [x] Run checks, rebuild and reopen for review.

## Findings and validation

One framing-preview flag now covers Crop and all Rotate choices. Its immutable texture contains the complete camera-oriented photograph. Both tools present saved rotation over that texture and use the same interior frame, shading, handles and pixel dimensions. Crop converts pointer coordinates through that frame. Rotate owns line and freehand guides. Tool switching preserves presentation and does not cross the shared plane's entry/exit boundary.

Straighten draws its line over a stationary photograph and commits nearest-axis alignment on release. Freehand retains live presentation of its angle draft. Both keep saved edits unchanged until release. The reference labels distinguish these interactions.

Native checks on Merrill and Quattro verify that switching Crop to Rotate and back while zoomed preserves the exact texture, render revision and viewport pixel bounds. Separate crop checks cover resizing, release, Hand, reopening, Fast and Smart. Smart checks cover framing entry/exit and regional-detail retention outside framing. The Smart smoke oracle now examines the presented tile, allowing a completed Full frame to supersede a retained hidden tile without a timing-dependent failure.

Workspace tests, strict Clippy, formatting, documentation indexes and artwork checks pass. The rebuilt native app is reopened for owner review. The independent comparison-image work remains unchanged.

Straighten also supports click-release-move-click-release. A short first release retains the anchor, pointer movement previews the line over the stationary photograph, and the second release commits it. Drag-release continues to use the same alignment calculation. Gesture checks cover both axes and input styles, retained anchors, cancellation and unchanged edits before the final release. The native rotation workflow exercises the two-click sequence before continuing through Freehand and shared-plane switching.

Before and rotation readouts now share a fixed viewport context badge. One widget owns the upper-left anchor, font, padding, translucent background and rounding. Rotation uses the proposed line/freehand angle while drawing and the saved fine angle afterward. Removing its crop-relative text position keeps it visible through zoom and pan.
