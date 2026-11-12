---
summary: "Smart overview sizing, persistent photograph views and a native-detail loupe share explicit viewport requirements."
kind: reference
status: draft
implementation: incomplete
---

# Viewport workflows

This records the owner's next requested workflows for architectural review. It does not authorize their implementation. The [viewport plan](../../plans/viewport-workflows.md) separates the existing implementation from proposed changes. Existing [Smart](./smart-preview.md) and [multi-edit](./multi-edit.md) specifications describe the baseline. Their affected requirements must migrate to this direction as each slice is agreed, rather than leaving contradictory contracts in place.

## Preview quality

Smart should select a Fast overview for one displayed photograph, Draft for two through four, and Thumb for five through eight. Count all displayed photographs, independently of which are selected for editing. Camera-specific working scales remain governed by [small previews](./small-previews.md).

Explicit Full, Fast, Draft and Thumb modes must retain their selected processing resolution through viewport zoom and selection changes. Smart alone changes the main preview's requested resolution automatically. Presentation-only framing continues to reuse compatible pixels.

The loupe must request Full detail independently of the selected main preview mode. Using the loupe must not promote or replace an explicitly reduced main preview with Full.

## Layout and interaction

Automatic layout should maximize useful photograph area in the available viewport. It may reorder photographs independently of browser order. Layout must preserve photograph identity, authored edits, selection and persistent view state. Shift-range selection and layout stability during gestures need a concrete reviewed contract.

Combine Pointer and Hand interaction so selecting a photograph does not require changing tools before panning it. Photograph tools must preserve the existing separation between viewport gestures and ordinary browser/editor interaction. Exact click, drag and modifier behavior remains to be reviewed.

Zoom must work within multi-photo views. Double-click enters detail zoom, which persists until double-click returns that photograph to Fit. A circular loupe must work in both single- and multi-photo views without leaving the current comparison layout or changing its editing targets.

Use the agreed [viewport and image-coordinate vocabulary](../architecture/concepts.md#viewport-and-image-coordinates). A viewport describes the UI area, while its visible region describes image coordinates. Saved Crop remains separate from transient viewing. **Detail view** is the proposed name for persistent zoom, and **loupe** names the magnifier.

## Detail rendering and ownership

Render the currently needed native-detail region first where the processing recipe supports it. Keep compatible existing pixels visible while replacement work runs. Unsupported recipes may use whole-image processing without changing authored settings or displaying incorrect regional output. Whether reduced-resolution regional processing is worthwhile remains a measured experiment.

The coordinator must own sequencing between region and whole-image requests. The engine executes requested processing and owns numerical reuse, alignment and required support. Completion of a region is not itself a request to render the whole image. Region-only rendering is a complete useful outcome. A whole-image continuation is chosen only when current view needs or useful preparation justify it, and must remain subordinate to immediate interaction. Loupe movement must not unconditionally start a whole-image render.

UI interaction owns persistent zoom, pan and loupe position. Processing caches do not own viewing state. Layout reordering must not move one photograph's view state onto another photograph.

Native decoded data, reduced processing inputs, regional/full photographic output and display textures have distinct ownership and memory costs. Preparing native detail must use explicit source identity and current edits. Keep the existing memory-accounting and foreground-priority requirements. Preloading is not actual cache use. A new decoded-RAM retention policy requires evidence that it addresses the measured detail latency.

## Open decisions

- Smart behavior beyond eight displayed photographs and the relationship to overflow tiles.
- Exact persistent zoom level, pan/zoom synchronization between photographs, and restoration when leaving/rejoining a displayed set.
- Layout stability, captions and viewport Shift-range order after aspect-aware reordering.
- When persistent Smart zoom merits whole-image continuation, compared with a transient loupe's region-only demand.
- Whether the first version needs more than one independently refreshed detail region for the same photograph. One replaceable detail request per photograph is the proposed starting point.
- Source/output retention and admission under repeated detail switching and several simultaneously zoomed photographs.
