---
summary: "Editor tools share As Shot white-balance bias, scoped presets, architecture defaults and transient Before comparisons."
kind: reference
status: accepted
implementation: conforming
---

# Editor workflow controls

Each editing section must offer a reset arrow and a preset menu. Ordinary reset restores that section from the user's configured defaults. Option/Alt-click restores its camera-as-shot baseline instead. Matching controls at the top of Editing apply to the whole photographic recipe. Section actions must leave other sections and workspace state unchanged.

Reset and preset glyphs should be compact within their existing click targets. Reset uses a left-pointing undo arrow with a rounded return inside a bordered button. Editing-tool drag grips should match the larger workspace grip for visibility.

Preset menus must support naming, saving, applying and deleting custom presets. Presets and user defaults live in application storage, separate from photo sidecars. Section presets store the same photographic schema as whole-recipe presets but apply only their declared scope. Preview quality, viewport, output dimensions, TIFF destination space, processing masks, browser ratings and panel arrangement must not be transferred. User defaults are retained per camera architecture. Saved photo edits take precedence. Setting defaults must not rewrite existing photographs.

White Balance must offer As Shot alongside available camera presets, with neutral-centered Warmth and Tint adjustments anchored to the selected balance. As Shot follows each file rather than copying another photograph's resolved preset. A zero bias must preserve the existing rendering. White balance must remain coherent between display, TIFF and DNG. DNG neutral/profile composition must apply the adjustment once and retain normal downstream WB adjustment. Temperature readouts must not imply an exact recorded Kelvin value when only gains are available.

Warmth and Tint use the same linear cone-response adjustment in every color pipeline, before Color Editor and shared photographic edits. Sensor monochrome retains those offsets as inactive values. Its camera WB selector follows the existing family capability. Pure WB bias does not alter technical DNG samples. Its neutral and profile carry the selected interpretation. Previously baked WB-dependent reconstruction remains baked.

Y toggles the requested edit against a camera-as-shot rendition. Shift+Y compares against the user's default recipe. The comparison must use the same RAW pipeline and image geometry, preserve zoom/pan, and display a Before badge at the upper left of the viewport. Comparison is transient. It must not change authored settings, sidecars, processing submissions, navigator/browser photographic preview recipes or preset contents. Text inputs and open menus retain their keyboard exclusions. Editing returns to the requested edit view.

The camera-as-shot baseline resolves the file's recorded WB, color mode and capture-dependent photographic defaults through the existing camera adapter. It is a RAW rendition rather than the embedded JPEG. User defaults overlay that baseline while respecting camera capabilities. Unsupported controls must be normalized without changing the saved preset or default template.

## Verification

Automated checks cover scoped copying, independent Noise/Detail resets, preset replacement and persistence, family defaults, preview invalidation, reset modifiers and shortcut routing. Original-file GPU checks cover shared WB and chromatic DNG reprojection on Classic, Merrill, Quattro and Quattro H. Native checks exercise camera/default comparisons, unchanged authored recipes, retained viewport coordinates, reuse of the current GPU image and exclusion of Before frames from photographic preview publication.
