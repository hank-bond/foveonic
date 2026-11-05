---
summary: "A neutral-patch eyedropper saves Custom white balance with independent Warmth and Tint fine adjustments."
kind: reference
status: accepted
implementation: conforming
---

# White balance picker

White Balance must offer an eyedropper for Color rendering across supported camera families. Place its square button immediately to the right of the Mode selector, matching the dropdown's measured height and shared control spacing. A click must sample a small linear patch before shared photographic edits and save a Custom balance. Warmth and Tint remain independent fine adjustments, reset to zero by a successful pick. The camera preset used to prepare the sampled patch remains the calibration anchor. Selecting As Shot or another camera preset replaces Custom. Reset, scoped presets, sidecars, photographic exports and editable DNG appearance must retain their shared White Balance behavior.

Sampling must run on the image worker using native oriented/cropped coordinates, independent of Full, Fast or Smart display resolution. Only small patch statistics may leave the GPU. Dark and non-finite samples must not determine balance. Calibrated linear values above nominal white must remain sampleable. Nominal white is a unit scale and must not be used as a sensor-clipping threshold. An invalid patch must leave settings unchanged and provide a notice through the shared workspace status and session Log, without inserting a row into the editing panel. The sampled neutral is stored in D50 XYZ, normalized to Y=1, and neutralized in the existing shared Bradford cone basis. This adjusts chromaticity while retaining the sampled patch's original luminance before fine adjustments.

The picker must disarm on mouse release, use the shared eyedropper button and tool routing, and yield to Hand, Escape, crop and other pickers. Photo, settings or token changes must discard pending results. A successful selection must use the ordinary render and sidecar update path.

The source study used Darktable's [spot white-balance selection](https://github.com/darktable-org/darktable/blob/master/src/iop/temperature.c), which derives bounded channel coefficients from a sampled patch. The independent implementation uses Foveonic's existing cone-space white-balance transform. The private numerical study exposes separate white-balance sample/pick entry points and a global channel-gain operation, but the inspected material does not establish its picker arithmetic or output equivalence.
