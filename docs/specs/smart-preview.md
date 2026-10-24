---
summary: "Smart preview shows Fast before Full detail, with viewport-first refinement where the recipe supports it."
kind: reference
status: accepted
implementation: conforming
---

# Smart preview

The viewport preview selector must offer Smart for supported Merrill, ordinary Quattro and Tiger color photographs. The application-wide preference must remain visible while loading. Resolve unavailable Fast/Smart choices from early capture metadata, showing Full and disabling the other modes without replacing the stored preference. Full metadata may refine that availability before it is used for subsequent requests. Classic Smart remains future work. Existing Full and Fast behavior remains available. The initial workspace choice remains Full unless the owner selects another preference.

Smart must initially show Fast, including fitted views and zooms of 100% or below, measured in physical display pixels per native photograph pixel. The first zoom above 100% must request Full processing. Quattro, Tiger and [Merrill](./merrill-fast-preview.md) must process the active viewport and necessary neighboring samples first, then start whole-frame Full as soon as that detail result is delivered. Recipes may include complete native processing tiles around the viewport. Explicit output resizing may fall back to direct whole-frame refinement where regional sampling is unsupported. Panning must not queue more regional renders. Until the whole frame completes, retain the interactive Fast overview and any completed detail tile at its original coordinates. Once Full completes, use it throughout the image, including after returning to Fit. Changing source or edits resets this sequence. Outdated results must not replace the current source or requested edits.

Whole-frame statistics and operations with global dependencies may remain whole-frame. Reuse their results where valid, and restrict later operations to the requested region where their contracts allow. Cropping a completed full render alone does not satisfy regional-processing support. The recipe must own region geometry, alignment and required margins. Shared numerical operations remain independent of camera identity and GUI policy.

The overview and detail must share native crop, orientation and viewport coordinates. Scopes and navigator must keep their established source/edit pairing. Inspection, comparisons, monochrome and exports remain Full. Displayed regions and reduced overview buffers must remain immutable while newer work runs. Reduced/full appearance differences accepted for Fast also apply during Smart refinement.

Embedded previews must use the smallest suitable camera thumbnail before rendering. The native photograph geometry must remain consistent with the Raw output and orientation. Version-4 header dimensions, separate JPEG thumbnails and bounded Exif thumbnails may provide this framing without sensor decoding. Remove confirmed camera thumbnail padding. Differently cropped camera JPEGs must not define Raw viewport coordinates. Whole-image scopes and the navigator follow the complete image. Viewport scopes may use regional detail when it covers the complete visible area.
