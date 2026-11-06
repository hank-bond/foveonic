---
summary: "X3F loading distinguishes damaged originals from unsupported variants and preserves supported RAW resolution modes."
kind: reference
status: accepted
implementation: conforming
---

# X3F input

## Requirements

- Support recorded RAW resolution modes across supported camera families through their existing recipes. Merrill has HI, MED and LOW. Classic SLR reduced modes use their recorded calibration and photographic dimensions. Quattro RAW has HI and LOW. JPEG-only size choices are outside this requirement. Reference-derived branches may initially use synthetic validation when originals are unavailable. Keep real-file validation limits explicit.
- Merrill HI originals with 4928×3264 sensor samples, MED originals with 4928×1632 sensor samples and LOW originals with 2464×1632 sensor samples must use the shared Merrill processing path. Reduced acceptance must require the matching camera family and recorded resolution. The fitted Multiscale Rows + Columns profile requires a 4704-pixel active width. LOW files must receive an actionable unsupported-method message for that choice and retain Reference and Multiscale Rows support.
- Preserve original samples and calibration metadata. Keep sensor storage, active area and declared photographic dimensions separate. MED Merrill’s 4704×1568 active samples produce a 3264×2176 photograph. Orientation, cropping, rotation, previews and exports must preserve that aspect ratio. Regional previews must use supported storage-to-presentation geometry or retain whole-frame rendering.
- Highlight work padding must remain inside the sensor with its required sampling halo. Color reconstruction and monochrome work padding must also remain within the stored sensor extent. Shield-based correction must select the physical shield below the active image, accounting for the different SD1 and DP Merrill metadata names. Despeckle must cover all detector blocks when eight work banks have unequal sizes, preserving record order and immutable inputs.
- Invalid file boundaries, section structures, compressed pixels, encoded strings and embedded JPEG data must report that the named file appears damaged or incomplete and suggest downloading or copying it again. Retain the technical cause for diagnosis.
- Full source loading, decoded-cache misses, capture metadata, embedded previews, GUI operations and CLI conversion must share this reporting. Unsupported versions, compression, camera models and resolutions must remain distinguishable from damaged data. Missing files and permission failures must give their own actionable messages. Background cancellation must retain its scheduling meaning.
- A valid embedded preview does not establish that sensor data is intact. Never hide sensor corruption by silently substituting a preview for processing.

## Validation

Use the owner-provided SD1 Merrill LOW and DP3 Merrill LOW/MED originals and a full-resolution original from [Photography Blog’s SD1 Merrill review](https://www.photographyblog.com/reviews/sigma_sd1_merrill_review). Seek independent reduced originals for other cameras. Downloads remain local validation inputs. This requirement does not authorize redistribution or adding them to release demos.

Check source decoding, ordinary photographic and editable DNG exports, Full and Fast presentation, and corruption messages through public input APIs. Preserve existing full-resolution Merrill shader results.

The completed [reduced RAW implementation slice](../../plans/archived/reduced-raw.md) records actual-original coverage, reference-worker checks and synthetic validation limits. Classic SLR MED/LOW and Quattro LOW still await independent reduced originals for photographic comparison.
