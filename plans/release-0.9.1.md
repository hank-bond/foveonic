---
summary: "Remove the last-selected photograph from editing and display, and release 0.9.1."
---

# Release 0.9.1

The owner found after publishing 0.9.0 that a group does not behave as one: the most recently selected photograph is treated differently. The owner considers every such difference a bug and wants them fixed before advertising the release (2026-11-25). Requirements: [multi-photo editing](../docs/specs/multi-edit.md).

## Reported

- [x] **One photograph follows a slider more closely than the others.** Reproduced in published 0.9.0 with six Merrill photographs, a 3,000 ms white-balance drag and a 768 MiB target: updates shown per photograph were 51, 17, 3, 3, 2, 2 (median of seven drags). Three causes. The app ranked each render by its photograph's place in the plan, and rank outranks queue order. A running photograph's next edit kept its place in line. Each edit left the previous version of every stage in memory, so a long drag filled the target and the group could not render together. With the [memory policy](../docs/specs/render-scheduling.md#working-set) the same drag shows 16, 13, 15, 14, 14, 14.
- [x] **Crop and Rotate show only on the last-selected photograph.** Every displayed photograph is drawn by one function and shows the armed tool. A drag belongs to the photograph it started on. The tool bar's crop ratio, orientation switch and reset, and Reset rotation, act on every editing target, each within its own frame (owner, 2026-11-25). The crop control is an icon with no displayed value, so with differing ratios its list marks none. Other dropdowns keep “Multi”.
- [x] **Thumbnails and photographs flicker when they update.** The owner saw browser thumbnails blank for a frame (2026-11-25). A display texture was freed the moment nothing owned it, while a shape queued earlier in the frame still named it. The renderer logs `Missing texture` and paints nothing. Published 0.9.0 does the same. Textures are now freed when the next frame starts, and a native run fails on that warning.
- [x] **Cropping moved photographs around.** The arrangement scored total area with one shared row or column size, so a small change of shape could pick another order. It now follows the [arrangement rules](../docs/specs/multi-edit.md#arrangement), agreed with the owner on 2026-11-25: separately sized rows or columns that favor the smallest photograph, an equal grid behind a switch, and no change while Crop or Rotate is armed.
- [ ] **Other reads of the last-selected photograph.** Still open: the navigator and scopes source, the Editing menu's family, preset list and displayed clipping values, the automatic dehaze estimate, and the loupe's cancel rule, which uses that photograph's display mapping. None affects tools or slider response. Carried from the [coherence cleanup](./coherence-cleanup.md).

## Memory policy

The owner agreed seven statements on 2026-11-25, recorded in the [scheduling specification](../docs/specs/render-scheduling.md#working-set). Discoveries while implementing them:

- A working photo produces several images: its detail patch, its whole image, and renders that are not edits such as a Before image. Each restarts from its own stages under the same names, so “one version per stage” holds per image. Two shortcuts failed review before this was stated in the cache: releasing everything the latest render did not plan threw away the whole image's stages on each patch edit, and renaming the patch's stages left an edit with nothing to restart from.
- With one version per stage there is no history to age. A photograph's restart stages carry one age, its latest edit, and the separate pass that trimmed obsolete versions first is gone.
- Memory is released in one place, `Scheduler::release_cache`, for both the trim after a render and admission.
- Admission releases cache only for Viewport jobs. Thumbnails and exports run wider only in free memory.
- Admission is retried after every render step, so a release that cannot make the room is paid many times in one drag. Releasing unconditionally cut the six-photo drag from about 14 updates per photograph to about 11. Admission now sums an upper bound of what could be released and leaves cache alone when that is less than the job lacks. The first review rejected an earlier bound because it left out optional stages of protected photographs; this one counts them, cached images and engine preparation.
- Three maintained tests encoded the old behaviour and were changed. Two expected a stage value from an earlier setting to still be cached after the setting changed and changed back: shadow neutralization and the Quattro H scale switch. Under one version per stage the first render after returning rebuilds it once (about 240 ms for the first full-size tile of a Quattro H after editing at half size, then 30 to 40 ms). One expected a display texture to be freed on drop; it is freed when the next frame starts.

Known gaps, neither a correctness problem:

- Protection covers the working photo an editing target last rendered from. A photograph that joins the editing selection without having rendered as a viewport image can be released before its first render starts, and is then reloaded.
- An image that stops being rendered as an edit while its photograph is still edited at the same size keeps its record, so its restart stages stay protected until the photograph leaves the editing selection or that size. Changing processing quality while zoomed in does this.
- Renders that are not edits share one record per working photo, so a Before image and an export of the same photograph replace each other's stages.
- A display texture dropped while the app is idle is freed at the next frame, which may be the next input.

Not measured: how long detail takes to return after a Native working photo is released. A first zoom with nothing in memory took 380 ms on the M1 Pro, which bounds it.

## Release

- [ ] Correct the 0.9.0 notes, whose opening says every editing tool works across the group.
- [ ] Repeat the release checks on the 0.9.1 archives, including both Windows hosts.

## Later

- A user asked for a color mode of None that applies no camera color mode and no color DQ (2026-11-25). Deferred to 0.10.0 because it is not a small change. A mode supplies a matrix, a contrast and the tone row, and an empty name already means the recorded mode, so None needs a reserved name in each family. Merrill and Classic have an unselected mode for technical and DNG output, whose tone row is the camera's first. Quattro has none and resolves modes through its own tables. Classic modes also choose the monochrome mixture. What None renders in each family is a photographic decision to agree first.

- Owner idea, 2026-11-25: a per-photograph information area while editing several photographs, so each one's own values are visible. Broader than this release.
