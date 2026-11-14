---
summary: "Smart overview sizing, persistent photograph views and a native-detail loupe share explicit viewport requirements."
kind: reference
status: accepted
implementation: conforming
---

# Viewport workflows

The [viewport plan](../../plans/viewport-workflows.md) records the reviewed implementation slices. [Smart](./smart-preview.md) specifies automatic preview quality and [multi-edit](./multi-edit.md) specifies selection and persistent per-photo viewing.

## Processing quality

The workspace offers exactly **Quality** and **Speed**, independently of authored photograph edits. Quality is the default. Both choose the main preview's working resolution from physical display-pixel demand for each photograph, including zoom. Quality should use at most one physical display pixel per working sample before selecting a finer grid. Speed may allow up to 1.5. Photograph count is not a quality input. Camera-specific working scales remain governed by [small previews](./small-previews.md).

Processing quality and working resolution must remain distinct. Native viewport zoom, regional detail and whole-image continuation preserve the selected processing policy. A region does not imply either policy. Export and pipeline inspection explicitly use Quality.

The loupe always requests native RGB detail with the selected processing policy. Its detail resource must not replace the main overview merely because it has finer sampling. Images from different policies must not replace or overlay one another, even when their working resolutions match. Completed older edits may continue progressive feedback within the same policy.

## Layout and interaction

Automatic layout should maximize useful photograph area in the available viewport, using each photograph's saved composition aspect ratio. It may reorder photographs independently of browser order. Layout must preserve photograph identity, authored edits, selection and persistent view state. It must retain the readable minimum tile and overflow policy. Layout search must stay bounded by visible capacity and must not enumerate photograph permutations on each frame.

Viewport Shift ranges must follow displayed visual order from top to bottom and then left to right, with the existing editing limit and nearest-to-anchor behavior. If the anchor is outside the displayed set, the clicked photograph becomes the whole range. Browser order still initializes editing targets and orders their storage. Captions remain attached to their photograph.

An ongoing pointer gesture must retain its layout through the release frame. New dimensions and viewport resizing may reflow on the following frame. Changing displayed photograph membership may replace the layout immediately. Before renditions and temporary framing presentations must not change the saved-composition aspect ratio used for layout.

The owner approved combining Pointer selection and panning. Its click, drag, modifier and tool-isolation contract belongs to [multi-edit](./multi-edit.md#selection-and-tools).

The owner approved per-photo persistent viewing in both single and multi-photo views. Its implemented contract belongs to [multi-edit](./multi-edit.md#per-photo-viewing) and the [workspace navigation requirements](./gui-workspace.md). A circular loupe must work in both single- and multi-photo views without leaving the current comparison layout or changing its editing targets.

Use the agreed [viewport and image-coordinate vocabulary](../architecture/concepts.md#viewport-and-image-coordinates). A viewport describes the UI area, while its visible region describes image coordinates. Saved Crop remains separate from transient viewing. **Detail view** names persistent zoom, and **loupe** names the magnifier.

## Detail rendering and ownership

Render the currently needed native-detail region first where the processing recipe supports it. Keep compatible existing pixels visible while replacement work runs. Unsupported recipes may use whole-image processing without changing authored settings or displaying incorrect regional output. Whether reduced-resolution regional processing is worthwhile remains a measured experiment.

Loupe and persistent detail viewing must share layered presentation: draw the available whole-photo overview, then overlay the latest compatible native section at its image coordinates. Moving beyond the section must retain sharp detail wherever it still overlaps, with the overview filling uncovered areas until replacement detail arrives. Display overlap must not count as complete render coverage. Keep one latest section per photograph, without accumulating a tile history or merging numerical buffers. Both layers must follow the same viewing transform, physical-pixel alignment and clipping shape. Replacing a section must not shift its registration with the overview or previous section. Older edits must not cover a newer overview, while newer detail may continue the existing progressive feedback over an older overview. An unchanged reduced overview must not discard useful native detail merely because it arrived later.

The native loupe latency target is 400 ms from the user's request to usable native-detail pixels when the decoded source is cached. Measurements must distinguish first native processing from reuse of an already rendered region, and include source acquisition, queuing and delivery when reporting application latency. Engine-only measurements with an already resident source are useful attribution, but do not establish this application target. Record the source, settings, region, memory budget and competing workload with latency results.

The coordinator must own sequencing between region and whole-image requests. The engine executes requested processing and owns numerical reuse, alignment and required support. Completion of a region is not itself a request to render the whole image. Region-only rendering is a complete useful outcome. A whole-image continuation is chosen only when current view needs or useful preparation justify it, and must remain subordinate to immediate interaction. Loupe movement must not unconditionally start a whole-image render.

Worker completion waits must park outside the shared GPU device locks so window submission can continue. They must preserve completed-stage, cancellation, publication and resource-retirement boundaries. Completion must cover a finite submitted prefix rather than waiting for a globally idle queue while the UI keeps submitting frames.

UI interaction owns persistent zoom, pan and loupe position. Processing caches do not own viewing state. Layout reordering must not move one photograph's view state onto another photograph.

Native decoded data, reduced processing inputs, regional/full photographic output and display textures have distinct ownership and memory costs. Preparing native detail must use explicit source identity and current edits. Keep the existing memory-accounting and foreground-priority requirements. Preloading is not actual cache use. A new decoded-RAM retention policy requires evidence that it addresses the measured detail latency.

## Loupe interaction

The approved first version uses **P** to select Loupe. Press and hold over a photograph to show the lens, then drag to inspect that photograph. Releasing the pointer hides the lens. Escape or choosing another tool cancels the gesture. Loupe gestures must preserve both browser and editing selections. Space temporarily pans with the existing viewport behavior.

The toolbar menu provides Small, Medium, Large and Extra Large sizes, 25%, 50%, 100%, 200% and 400% magnification, and centered or offset placement. These workspace options persist independently of photographic settings. At 100%, one native image pixel occupies one physical display pixel. Positioning the lens within the viewport must not change which image point it inspects. Its circular image uses the existing color-managed GPU presentation with a thin dark rim.

The lens requests native detail independently of the main preview mode. Compatible existing pixels remain useful while replacement detail renders. Moving the lens replaces obsolete detail demand and must not request whole-image continuation or cancel useful whole-image work. Unsupported regional processing may return whole-image detail, but that image remains a detail resource and must not promote the main preview. A thumbnail-only displayed photograph may temporarily join the bounded photo owner for inspection without joining the editing selection.

Returning into compatible native coverage should withdraw a newer unnecessary detail request while retaining displayed pixels and numerical caches. Leaving that coverage again must permit a fresh request. Withdrawal uses the existing cancellation and admission boundaries. It does not undo compatible results already admitted for delivery. Reuse margins should be measured against both replacement frequency and the cost of larger regional jobs.

Detail identifies the consumer independently of regional or whole-image coverage. Native geometry must come from processing metadata, not composed browser-thumbnail dimensions. If no compatible processed image supplies it, the first inspection may request whole-image Detail. Its native result then supplies reliable geometry for subsequent regions. Final output resizing does not reduce native inspection resolution. Transiently admitted photographs leave the photo owner on release unless they have become editing targets. Existing retained photographs keep their ordinary lifetime and memory accounting.

The [Capture One Loupe documentation](https://support.captureone.com/hc/en-us/articles/360002472897-The-Loupe-tool) informs the shortcut, hold-and-drag interaction, size, magnification and placement options. Release-to-hide is the chosen first-version interaction here. Separate zoom and focus-point tools from that menu are outside this slice.

## Future decisions

These are outside the accepted first-version scope.

- Smart behavior beyond eight displayed photographs and the relationship to overflow tiles.
- Optional synchronized pan or zoom between photographs. Independent session viewing and restoration are specified in multi-edit.
- Further heuristics for persistent Smart whole-image continuation. The current behavior requests it, while loupe movement does not.
- More than one independently refreshed detail region for the same photograph. The first version uses one replaceable detail request per photograph.
- Source/output retention and admission under repeated detail switching and several simultaneously zoomed photographs.
