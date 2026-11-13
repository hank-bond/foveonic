---
summary: "Smart overview sizing, persistent photograph views and a native-detail loupe share explicit viewport requirements."
kind: reference
status: draft
implementation: incomplete
---

# Viewport workflows

This records the owner's next requested workflows for architectural review. It does not authorize their implementation. The [viewport plan](../../plans/viewport-workflows.md) separates the existing implementation from proposed changes. Existing [Smart](./smart-preview.md) and [multi-edit](./multi-edit.md) specifications describe the baseline. Their affected requirements must migrate to this direction as each slice is agreed, rather than leaving contradictory contracts in place.

## Preview quality

The owner refined the initial count-based proposal: Smart should choose by physical display-pixel demand, independently for every displayed photograph. Its concrete policy now belongs to [Smart preview](./smart-preview.md). Photo count is not a quality input. Camera-specific working scales remain governed by [small previews](./small-previews.md).

Explicit Full, Fast, Draft and Thumb modes must retain their selected processing resolution through viewport zoom and selection changes. Smart alone changes the main preview's requested resolution automatically. Presentation-only framing continues to reuse compatible pixels.

The loupe must request Full detail independently of the selected main preview mode. Using the loupe must not promote or replace an explicitly reduced main preview with Full.

## Layout and interaction

Automatic layout should maximize useful photograph area in the available viewport, using each photograph's saved composition aspect ratio. It may reorder photographs independently of browser order. Layout must preserve photograph identity, authored edits, selection and persistent view state. It must retain the readable minimum tile and overflow policy. Layout search must stay bounded by visible capacity and must not enumerate photograph permutations on each frame.

Viewport Shift ranges must follow displayed visual order from top to bottom and then left to right, with the existing editing limit and nearest-to-anchor behavior. If the anchor is outside the displayed set, the clicked photograph becomes the whole range. Browser order still initializes editing targets and orders their storage. Captions remain attached to their photograph.

An ongoing pointer gesture must retain its layout through the release frame. New dimensions and viewport resizing may reflow on the following frame. Changing displayed photograph membership may replace the layout immediately. Before renditions and temporary framing presentations must not change the saved-composition aspect ratio used for layout.

Combine Pointer and Hand interaction so selecting a photograph does not require changing tools before panning it. Photograph tools must preserve the existing separation between viewport gestures and ordinary browser/editor interaction. Exact click, drag and modifier behavior remains to be reviewed.

The owner approved per-photo persistent viewing in both single and multi-photo views. Its implemented contract belongs to [multi-edit](./multi-edit.md#per-photo-viewing) and the [workspace navigation requirements](./gui-workspace.md). A circular loupe must work in both single- and multi-photo views without leaving the current comparison layout or changing its editing targets.

Use the agreed [viewport and image-coordinate vocabulary](../architecture/concepts.md#viewport-and-image-coordinates). A viewport describes the UI area, while its visible region describes image coordinates. Saved Crop remains separate from transient viewing. **Detail view** is the proposed name for persistent zoom, and **loupe** names the magnifier.

## Detail rendering and ownership

Render the currently needed native-detail region first where the processing recipe supports it. Keep compatible existing pixels visible while replacement work runs. Unsupported recipes may use whole-image processing without changing authored settings or displaying incorrect regional output. Whether reduced-resolution regional processing is worthwhile remains a measured experiment.

The coordinator must own sequencing between region and whole-image requests. The engine executes requested processing and owns numerical reuse, alignment and required support. Completion of a region is not itself a request to render the whole image. Region-only rendering is a complete useful outcome. A whole-image continuation is chosen only when current view needs or useful preparation justify it, and must remain subordinate to immediate interaction. Loupe movement must not unconditionally start a whole-image render.

UI interaction owns persistent zoom, pan and loupe position. Processing caches do not own viewing state. Layout reordering must not move one photograph's view state onto another photograph.

Native decoded data, reduced processing inputs, regional/full photographic output and display textures have distinct ownership and memory costs. Preparing native detail must use explicit source identity and current edits. Keep the existing memory-accounting and foreground-priority requirements. Preloading is not actual cache use. A new decoded-RAM retention policy requires evidence that it addresses the measured detail latency.

## Loupe interaction

The approved first version uses **P** to select Loupe. Press and hold over a photograph to show the lens, then drag to inspect that photograph. Releasing the pointer hides the lens. Escape or choosing another tool cancels the gesture. Loupe gestures must preserve both browser and editing selections. Space temporarily pans with the existing viewport behavior.

The toolbar menu provides Small, Medium and Large sizes, 25%, 50%, 100% and 200% magnification, and centered or offset placement. These workspace options persist independently of photographic settings. At 100%, one native image pixel occupies one physical display pixel. Positioning the lens within the viewport must not change which image point it inspects. Its circular image uses the existing color-managed GPU presentation with a thin dark rim.

The lens requests native detail independently of the main preview mode. Compatible existing pixels remain useful while replacement detail renders. Moving the lens replaces obsolete detail demand and must not request whole-image continuation or cancel useful whole-image work. Unsupported regional processing may return whole-image detail, but that image remains a detail resource and must not promote the main preview. A thumbnail-only displayed photograph may temporarily join the bounded photo owner for inspection without joining the editing selection.

The [Capture One Loupe documentation](https://support.captureone.com/hc/en-us/articles/360002472897-The-Loupe-tool) informs the shortcut, hold-and-drag interaction, size, magnification and placement options. Release-to-hide is the chosen first-version interaction here. Separate zoom and focus-point tools from that menu are outside this slice.

## Open decisions

- Smart behavior beyond eight displayed photographs and the relationship to overflow tiles.
- Optional synchronized pan or zoom between photographs. Independent session viewing and restoration are specified in multi-edit.
- When persistent Smart zoom merits whole-image continuation, compared with a transient loupe's region-only demand.
- Whether the first version needs more than one independently refreshed detail region for the same photograph. One replaceable detail request per photograph is the proposed starting point.
- Source/output retention and admission under repeated detail switching and several simultaneously zoomed photographs.
