---
summary: "Separate quarter-resolution Fast controls from Smart's reconstruction-bypassing Draft recipe and publish the verified release."
status: archived
---

# Fast preview controls

The owner requested that standalone Fast honor Noise and Detail at quarter resolution, while Smart may keep its lighter overview. The owner approved Draft as the recipe name. Implements [Merrill previews](../../docs/specs/merrill-fast-preview.md) and [Smart previews](../../docs/specs/smart-preview.md).

- [x] Separate Fast and Draft at the recipe boundary while preserving saved edits.
- [x] Map Merrill noise geometry to the reduced grid and retain Quattro's existing noise controls.
- [x] Verify actual GPU stages, reduced pixels, cache reuse and immutable Full/Draft results.
- [x] Run repository checks and isolated native Fast/Smart checks.
- [x] Update the unpublished 0.4.0 draft, preserve owner notes and verify its replacement signed archive.

Fast and Draft share half-width, half-height working geometry. Fast runs requested noise reconstruction and ColorDQ on that grid. Camera-coordinate bounds map to the reduced samples, while filtering support remains in working pixels. Native source metadata and saved controls remain unchanged. Merrill row correction continues to be omitted for reduced rendering as previously agreed. Quattro's mandatory reconstruction and noise controls are already active in reduced rendering and remain unchanged.

The photographic GPU check confirms Fast submits the requested reconstruction and ColorDQ stages while Draft omits them. Reduced results remain finite, controls change the pixels, cached and fresh Fast agree, and earlier Full, Fast and Draft images remain immutable. Sidecar encodings preserve the requested controls. Workspace tests, strict Clippy, formatting, documentation and artwork checks pass. The local tagged bundle completed seven Fast and twelve Smart native checkpoints, including Draft overview, viewport detail, Full continuation and cache reopening.


## Release verification

Committed as `c879683` and selected as the final `v0.4.0` candidate before publication. The unpublished tag moved from the first color-management draft to this commit under the owner's correction request. The [replacement hosted run](https://github.com/hank-bond/foveonic/actions/runs/40133008016) passed all checks, signing, archive upload, credential cleanup and signed executable startup on macOS 15.7.9.

The downloaded replacement archive matches its checksum and hosted digest: `22801e69d53b04c49c766c139139a192e6534c5f558767f712960a84e208e586`. Version 0.4.0, executable/bundle target 15.0, Developer ID chain, secure timestamp and hardened runtime verify. The signed app completed eight display, seven Fast and twelve Smart checkpoints. Fast remained quarter resolution with the requested processing, Smart reported Draft before native regional Full, gamut switching preserved photographic revisions, and source hashes and sidecar protections remained intact. Signature verification still passed after the native checks.

The owner saved the draft introduction and explicitly authorized publication after verification. [v0.4.0](https://github.com/hank-bond/foveonic/releases/tag/v0.4.0) is published as Latest with those notes unchanged. The final published tag remains fixed at `c879683`. Interactive macOS 15 and cross-monitor acceptance remain in the active display plan.
