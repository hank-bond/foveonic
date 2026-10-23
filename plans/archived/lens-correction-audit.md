---
summary: "Audit available lens corrections and defer chromatic aberration after finding no supported vignetting or distortion profile route."
status: archived
---

# Lens correction audit

The owner requested importing available vignetting, distortion/geometry and chromatic-aberration corrections across supported architectures. After the resource inventory, the owner deferred chromatic aberration. This pass changes no photographic processing or thumbnail geometry.

- [x] Inspect the supplied reference implementation's processing code and installed resources.
- [x] Check native capability records for Classic, Merrill, Quattro and Tiger.
- [x] Record the available corrections and the owner's narrowed scope.

## Findings

The supplied application's `Contents/Resources/LensDatabase` contains only `Chromatic`, with 91 `.lpi` profiles. Its list declares database version 2.9 and 91 entries. Profiles cover interchangeable lenses and fixed-lens Merrill/Quattro cameras. No vignetting or distortion profile database was found in that application.

Isolated calls to each sensor library's capability interface returned intact output guards. The packed managed capability layout identifies the color and monochrome vignetting-profile and distortion-profile switches. All four libraries report these switches as zero, their associated control ranges as zero, and the vignetting/distortion correction-effect flags as zero. This establishes the absence of a supported profile-correction route in the supplied application's supported Foveon architectures. It does not establish what other application versions or sensor families provide.

The native libraries contain a generic vignette simulation effect and chromatic-aberration transforms. The vignette effect does not establish a calibrated lens-vignetting correction. Existing sensor and color-shading calibration remains part of the current recipes.

Chromatic-aberration profile decoding, selection and numerical resampling were previously recovered in the research checkout. Future integration still needs actual-database validation, GPU processing, camera/lens selection and preview/export consistency. A prototype using the retained decoder's fixed container layout did not completely decode profile 486, so the 91 containers must not be assumed uniformly validated. The owner deferred this work.

## Thumbnail observation

The DP0 original has a 5424×3616 Raw photograph, a differently framed 4816×3616 large JPEG, and a 160×106 small thumbnail. The small thumbnail differs from 3:2 by about 0.6% because of integer dimensions. Current preview-cache preparation excludes the differently framed large JPEG. The owner confirmed the viewport and navigator look correct and chose to leave the sidebar thumbnail geometry unchanged.
