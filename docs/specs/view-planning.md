---
summary: "Every viewport photo is one view whose looks, images in use and layers come from one pure planner, with one image store, one reconcile rule and no per-mode request paths."
kind: reference
status: accepted
implementation: incomplete
---

# View planning

## Why this design

Six-photo zoom testing on 2026-11-19 showed edits refreshing only the native patch while the image behind it kept older edits, and a patch for a dragged-past position delaying the one at the final position. Two attempts followed.

1. A planner that returned the renders still needed. The coordinator kept its own retirement rules, so a cache of the last plan with reset rules and separate withdrawal logic were needed, and each disagreement between those owners was a bug.
2. A planner that returns every image in use, adopted photo by photo. Zoom, loupe and fit view moved; Before, inspection, comparison, the crop and rotation plane and the active photo's own store and request path did not. Every remaining failure sat on the seam between the planner and those paths: clearing the worker, restoring the photograph after inspection, Before photos losing work, and full-size images pinning memory. None was a planner bug, and native scenarios found each one because no specification recorded the behavior.

The lessons are that each question needs exactly one owner, that every case belongs in the tables below before code, and that a migration must never leave one photo with two owners. The owner set the bar on 2026-11-19: nothing is off limits as long as the result is simpler, more consistent and has less sprawl.

## Before this design

| Concern | Owners before this design |
|---|---|
| What is displayed | Editing render paths, the loupe's photo, the excluded active photo, Before's photo rendition, framing state |
| Requests | View planner (Current zoom, loupe, fit), overview path (`set_overviews`, `multi_edit` dirty flags), Before staging (`refine`, `advance_refinement`), the active photo's `submit_with_quality`, export |
| Coordinator rules for viewport photos | `keep_in_use`, plus `detail_replaced`, `purpose_replaced`, `cancel_refinement_for`, `retain_overviews`, `retire_before`, `activate`, background-completion retirement |
| Stored images | `App::frame` (active photo), `multi_edit` frame, detail and preview per photo, `Before::saved` and `Before::detail`, kept Before images in `ImageCache` |
| Where a result goes | `delivery.rs` destinations (Active, Overview, Preview) and rejection rules |
| What is drawn | `drawn_detail`, `loupe_images`, `current_photo_frame`, `rendition_frame`, `navigator_uses_preview` |

## Vocabulary

Agreed terms. Code and prose use these names only.

- **Browser selection:** the photos highlighted in the browser.
- **Viewport photos:** the photos the viewport draws from the browser selection. Every viewport photo has its settings loaded and is planned.
- **Editing selection:** the viewport photos edits from the shared controls apply to, chosen by clicking tiles and bounded by the editing limit.
- **History photos:** photos that left the viewport selection and keep their finished images.
- **Loading:** a viewport photo whose saved settings have not been read yet. It keeps its images and work until they arrive.
- **View mode:** how a viewport photo is shown: fit, the crop and rotation plane, zoomed, or under the loupe.
- **Visible region:** the part of a photo's image inside its tile on screen, in native pixels.
- **Patch:** a native-resolution render of part of a photo. Its **patch region** is the visible region plus the recipe's margins.
- **Loupe visible region** and **loupe patch region:** the same two regions for the loupe's lens.

**Action scope.** One rule decides what an action changes, from where it comes:

| Source | Scope | With Shift |
|---|---|---|
| A pointer action on a photo: zoom, pan, double-click fit, crop or rotation drag, color pick, the bar under a photo | That photo | The editing selection, applied or sampled from that photo |
| Shared controls: panel sliders and buttons, keyboard shortcuts | The editing selection | — |
| View switches: tool choice, inspection output, comparison, Before | All viewport photos | — |

With the Pointer tool, clicking is itself the selection action: a click selects the photo and Shift-click extends the selection to a range. One function returns an action's scope, and every tool applies its edit to that scope through one path that writes each photo's settings.

There is no active photo. Tools used by pointing act on the photo pointed at, whether or not it is in the editing selection. The crop and rotation plane, inspection outputs and comparison apply to every viewport photo. Scopes, the navigator and the information bar show only when the viewport selection is one photo.

## Model

Each frame answers three questions, each in one place.

| Question | Owner | Output |
|---|---|---|
| What is on screen, and how? | The viewport | One view per viewport photo |
| Which images do those views use? | The planner | The set of images in use |
| What is drawn? | The planner's layer rule | Layers per view |

### Views

A view is one viewport photo and how it is shown:

- **Look:** the settings to render. Current uses the authored edits. Before is a render shortcut: the same view with Camera or Defaults settings in place of the authored ones, keeping the authored crop and rotation. A look is only a settings value, so Before uses no separate path.
- **Output:** the final photograph, a pipeline inspection output, or a comparison. Inspection and comparison change what the render produces, not how the view is planned.
- **Shown:** fit; zoomed to a settled region; under the loupe at a settled region; or the crop and rotation plane.

Every viewport photo is a view, including the photo under the loupe.

### Rendered images

The app keeps one collection of rendered images, keyed by photo, look and output; each entry holds a whole image and a patch. A view looks up its own key. An image a view uses is shown; any other image is kept for reuse or dropped by the memory policy, so toggling Before or changing output moves nothing. `App::frame`, `multi_edit`'s per-photo frames, `Before::saved`, `Before::detail` and the Before cache-and-restore code are replaced by this collection. Delivery accepts a result when it is an image in use and newer than the entry's image of that kind; nothing else routes results.

Memory accounting is unchanged by this step: shown images are reported through the app's inventory and kept images through the scheduler's cache, as today. Unifying them is a separate step, and it must account CPU and GPU memory as separate pools, because on a discrete GPU display textures and render buffers use VRAM, a separate and usually smaller budget than system memory.

### Images in use

| Shown | Images in use |
|---|---|
| Fit | Whole image at the tile's scale |
| Zoomed | Patch covering the settled region; whole image at the tile's scale behind it; full size when background completion is enabled |
| Zoomed or loupe, a current full-size image exists | That full-size image alone |
| Loupe | Patch covering the settled lens region; the tile's own whole image if the tile shows one |
| Loupe, photo without any image | A whole-photo patch at the tile's scale to place the lens |
| Crop and rotation plane | Whole image of the uncropped editing extent |
| Zoomed or loupe without regional support | Full size |

Rows apply to every look and output: an inspection output or comparison zoomed in uses a patch, because that is what is viewed. Until regional rendering of inspection outputs and comparisons is verified, they count as without regional support and use full size. It uses the processing quality the view policy selects for it.

Until an image with the current look arrives, the stored image of that kind stays as a placeholder; a look or output with no image yet shows the photo's Current image and places its patch from it. A viewport photo keeps only the images for what it shows, what it shows with Before off, and its Current image, so outputs it no longer shows do not hold memory and turning Before off is immediate. Across views, patches come first with the selected photo's first, whole images once no view needs a patch, then full size.

### Reconciling

The coordinator receives the images in use with the photos they belong to and makes work match them in one rule: render what is missing; a planned photo keeps work matching an image in use; a loading photo keeps its work; a history photo keeps only finished images; all other work stops. Export is separate. The coordinator has no other rules for photos.

A render that fails is recorded with its request. The request counts as done until it changes, so one failing photo neither retries forever nor holds back other photos' whole images.

### Clearing and freshness

An image is current when it was rendered for the view's look and output and after the last worker clear. Clearing the worker therefore re-renders displayed images, which stay as placeholders meanwhile. Republishing is never needed, because the planner publishes the complete set.

### Settling and up to date

The viewport owns settling: a moving view settles when its region has rested for 120 ms, and only settled regions reach the planner. The viewport is up to date when no image in use is missing and no view is settling.

### Drawing

1. **Behind:** the whole image, a placeholder until a current one replaces it.
2. **Top:** the patch when it is current. While the look changes faster than patches arrive, the latest patch stays on top unless the whole image already has a newer look.
3. The navigator and the browser thumbnail downscale the whole image.

### Presentation

What a view draws is its presentation: the whole image and patch the layer rule selects. Everything that follows the displayed image follows the presentation's identity, the delivered task and content revision of each layer, and recomputes when it changes:

| Consumer | Follows |
|---|---|
| Scopes and colour picks | The active view's presentation |
| Navigator, information bar | The active view's whole image |
| Browser thumbnail | The photo's whole image for its Current look |
| GPU drawing state | Each view's presentation |

There are no presentation revisions: reusing an existing image changes the presentation's identity, so consumers notice it without rewriting revisions.

| Event | Behavior |
|---|---|
| Selecting a photo | Its view is planned like any other; existing images for its look are reused. Work for photos that left the view stops by reconciling, so selection never clears the worker. |
| Selection progress | Shown from selection until the selected view is up to date. |
| Selection priority | Viewport demand is held from selection until the selected view's images in use have been published. |
| Before toggle | A look change for the editing selection's views; no images move and the coordinator needs no activation or retirement calls. |
| Accepted result | Updates metadata, the dehaze estimate when requested, and reports errors, as today. |
| Export | Unchanged; exports are not views. |

## Memory and interactivity

Memory claims follow images in use at the scale the view needs. A finer image than needed is shown but does not keep its native work. Full size is optional and speculative because rendering it can take longer than interactivity allows at the current scale.

Research to do before choosing a policy: whether a reduced whole image or a downscaled checkpoint can be derived from native sensor data already resident for a patch, so a zoomed photo needs one decode and its checkpoints can each check what they need. Measure that against keeping separate reduced sensor data within the memory target, then choose by interactivity.

## What goes away

- The overview path, `multi_edit` dirty flags and `needs_render`/`request`.
- Before's staged requests: `Refinement`, `refine`, `advance_refinement`, `start_region`, `whole_admitted`, `cancel_refinement`, `retire_before`, `activate`, `submit_before_photos`.
- The active photo's separate request path and the `excluded` photo.
- The coordinator's `detail_replaced`, `purpose_replaced` and background-completion retirement for viewport photos.
- `delivery.rs` destinations and most rejection rules.
- `App::frame`, `Before::saved`, `Before::detail`, the per-photo frames in `multi_edit`, and the separate image pickers.
- `restore_photograph` and `mark_stale`, which only bridge the old stores.
- Presentation revisions and their rewriting on reuse, progress `begin`/`delivered` bookkeeping, the worker clear on selection, and the coordinator's `activate` and `retire_before`.

## Migration

One step moves all displayed-photo rendering to this model, so no photo has two owners at any point. The old paths are deleted in the same step. Export, scopes, colour picks, idle preparation and browser thumbnail loading keep their own owners.

## Testing

- Every table row is a planner unit test.
- An app-level test runs the planning loop for each view kind without a GPU, covering the cases scenarios found: clearing, returning from inspection, Before toggles, the loupe on a photo that is not edited, and leaving the view.
- Native scenarios remain acceptance checks; a failing wait names the condition it waited for and the last `view.plan` event.
