---
summary: "Correct sd Quattro sparse sensor sites before reduced Smart and Fast reconstruction."
status: archived
---

# Correct Quattro preview sensor sites

Follow the [Smart preview requirements](../../docs/specs/smart-preview.md) and [Quattro composition contract](../../docs/specs/quattro-rendering.md#composition). The owner approved applying mandatory native sensor corrections before downsampling while retaining model-specific tables and shared reconstruction.

- [x] Identify why ordinary sd Quattro differs from DP models and Quattro H.
- [x] Apply native sparse blue correction on ordinary sd reduced previews too.
- [x] Verify native corrected samples are identical across Full, Fast and Draft and feed reconstruction on sd Quattro and Quattro H.
- [x] Compare the reported sd preview before and after, confirm Full output is unchanged and rebuild the app.

## Findings

DP cameras already apply their phase-gain and sparse calibration before preview reconstruction. Interchangeable-lens cameras share the sparse correction operation with model-specific preparation. Their RGB correction already runs in every mode, but the blue-plane correction was conditional on Full or the larger sensor variant. Ordinary sd Quattro therefore averaged uncorrected sparse sites in Fast and Draft. Zooming requested Full and restored their correction. Removing that preview-only exclusion retains the same Full path and model-specific tables. General blue repairs and optional noise/detail policy remain separate.

The original-file regression checks the blue history entering reconstruction, not just the presence of a correction node. It verifies native physical extent, a nontrivial correction and exactly equal corrected samples across Full, Fast and Draft on both interchangeable-lens models. A controlled sd before/after comparison removes the regular dot grid from the reduced preview while retaining the Full render fingerprint. GPU diagnostics measure about 30 ms for the newly included native sparse correction and mask work on M1 Pro. These are isolated samples rather than an end-to-end latency guarantee.

The owner reviewed the corrected preview and approved the change. The sd Quattro and Quattro H original-file regressions, serial workspace tests, strict workspace Clippy, formatting and artwork tests pass on the isolated commit. Documentation checks pass in the working checkout. The isolated snapshot and its unchanged parent both fail the existing finishing-index link to the separately pending adaptive-tone module.
