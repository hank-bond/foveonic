---
status: archived
summary: "Extend Merrill Detail amounts to three while retaining the existing size support."
---

# Extend Detail amount range

Requirements: [Detail and noise](../../docs/specs/detail-and-noise.md).

The owner requested stronger Detail on a supplied ISO 100 photograph and selected an amount-only extension. Fine and Coarse amounts run from zero through three in Color and Monochrome. Defaults remain one. Size remains 0.5 through two, preserving the 64-pixel processing context. No shader arithmetic or existing valid setting changes.

- [x] Measure the current response with the saved recipe and isolate luminance cleanup and Fine Size effects.
- [x] Extend the shared range, validation and CLI boundary checks.
- [x] Verify the new endpoint on the supplied photograph.
- [x] Finish isolated checks and rebuild the review app.
- [x] Owner reviewed the range change and authorized commit.

With both amounts at two and both sizes at two, the supplied photograph's Detail gain is approximately within plus or minus 0.03 stop for 80 percent of active pixels. At amount three, it is approximately plus or minus 0.06 stop. The log-gain RMS rises from 0.02658 to 0.05317 stop. Measurements use the Provided noise profile and saved photographic settings. Detail enhances the luminance residual relative to the filtered base, so structure already retained by that base is less affected. Equal amounts cancel the explicit Fine/Coarse split, but Fine Size still changes the final low-pass slightly because the blur passes are chained.

Extending Size to three was discussed separately. Its maximum native support would rise from 60 to 90 pixels and require additional context and seam validation. That expansion is outside this slice.

The isolated range-only snapshot passes workspace tests and strict Clippy. The new endpoint is exercised through core validation, recipe round trips and CLI parsing. Full photographic continuation at amount three contains no nonfinite values. Formatting and documentation checks pass. At implementation time the working-tree test build encountered unrelated in-progress settings-test errors, so the range checks used an isolated snapshot. The native app is rebuilt and opened on the supplied photograph. The owner approved the amount-only extension for commit.
