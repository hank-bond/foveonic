---
summary: "Read Classic bitmap thumbnails through an encoding-based embedded-preview path."
status: archived
---

# Support embedded Classic thumbnails

Implement the approved next step after [Classic consolidation](./classic-consolidation.md). Follow the preview geometry requirements in [application workflows](../../docs/specs/application-workflows.md).

- [x] Discover embedded image descriptors independently of camera family.
- [x] Select the smallest decodable embedded image by pixel area.
- [x] Decode format3 padded RGB bitmaps and format18 JPEGs into tightly packed RGB8.
- [x] Preserve native presentation dimensions and apply orientation once in the GUI.
- [x] Verify SD9, SD10 and existing JPEG originals, malformed input bounds, and workspace checks.

Keep thumbnail loading independent of sensor decoding and processing. Format11 compressed previews may contribute presentation dimensions, but their decoder is outside this thumbnail slice because SD10 already embeds a format3 thumbnail. Limit encoded and decoded allocations. The processing pipeline and camera-family recipes are unchanged.

## Evidence and review

Seven acquired Classic originals decode their embedded previews without sensor processing. SD9 and SD10 select their 189×126 bitmap thumbnails and retain the larger 567×378 preview dimensions. The existing Merrill corpus retains its JPEG thumbnail decoding and native dimensions. CPU fixtures cover RGB channel order, row padding, truncated data, invalid stride, pixel-area selection and mismatched JPEG dimensions in both directions. GUI tests verify rotation and native geometry independently of resizing.

The decoder checks container extents before raster allocation and bounds encoded and decoded sizes. Independent review found no blocker. A corrupt selected preview still reports an error rather than trying a larger image, matching the previous behavior.

All 326 ordinary workspace tests, strict all-target Clippy, formatting and documentation checks pass. The native app bundle was rebuilt. This change is separate from the committed Classic processing work and awaits review.

Visual review exposed an incorrect initial BGR assumption. Format3 stores RGB bytes. The decoder now preserves channel order while stripping row padding. Same-source JPEG comparisons validate that order independently of the synthetic layout fixture.
