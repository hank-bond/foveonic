---
summary: "Validate SD1 Merrill HI and LOW originals and make damaged-file failures actionable across input readers."
status: archived
---

# Validate SD1 Merrill input

Requirements: [X3F input](../../docs/specs/x3f-input.md).

- [x] Distinguish unsupported LOW geometry from corrupt HI sensor data.
- [x] Download and decode a full-resolution SD1 Merrill original from the review linked in the specification.
- [x] Add shared typed damage evidence and filename/recovery messages to original, metadata and preview readers.
- [x] Accept validated LOW geometry and adapt highlight padding and despeckle work partitioning.
- [x] Verify public reader messages, unequal-bank GPU repair, unchanged ordinary repair, and original-file photographic/DNG exports.
- [x] Rebuild and reopen for review.

## Findings

The LOW original decodes all three sensor planes and camera calibration. Its active area is 2336×1568. The full-resolution review original decodes at 4928×3264 and renders an active area of 4704×3136. Two owner-provided HI originals have intact directories but invalid compressed sensor streams. One contains large zero-filled regions. The other completes green and blue decoding but loses valid red codes around row 932. A scalar bit reader reproduces the failures.

LOW exposes insufficient bottom padding for the highlight work region and a detector-block count not divisible by eight. Bound generated highlight, reconstruction and monochrome extensions after validating the active rectangle. Partition whole detector blocks between banks without dropping remainder blocks, retaining global record order.


SD1 uses the literal top edge in DarkShieldTop. DP Merrill uses that label for a lower shield. Retain the existing lower region where present and select DarkShieldBottom for SD1. The fixed column reference profile requires a 4704-pixel active width, so LOW Rows + Columns remains explicitly unsupported with recovery choices instead of reusing an unvalidated profile.

HI and LOW originals pass Full and Fast color rendering with enabled Reference and Multiscale Rows cleanup, and monochrome. HI also passes Multiscale Rows + Columns. LOW verifies the explicit unsupported-method message. Both export TIFF and editable DNG. The native LOW workflow completes viewport, scopes, zoom and inspection stages.

Workspace tests, strict all-target Clippy, formatting, documentation indexes and artwork checks pass. GPU repair checks cover unequal banks, ordered repair and shield prediction. Native HI and LOW workflows complete. Independent comparison work remains unchanged.
