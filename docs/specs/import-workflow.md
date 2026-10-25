---
summary: "Import mode reviews sources without source-side writes and carries selected originals, edits, and hidden-file decisions to a destination."
kind: reference
status: accepted
implementation: conforming
---

# Import workflow

## Agreed direction

Foveonic focuses on fast browsing, culling, previews, and conversion for further editing. Import should use the existing workspace. A separate import window is unnecessary.

- Browse and Import must be folder-opening actions at the far left of the main toolbar row, within their own button group. Each must open the native folder picker, including when its corresponding mode is already active. They must not switch the current folder's mode in place. The source-write policy changes only after the chosen folder opens successfully. Cancelling or failing to open a chosen folder must preserve the current folder, edits, picks, and mode. There is no separate Open folder action. Import review must open a source without saving editing sidecars at that source. This policy applies to any source folder, independently of whether it is on a memory card. Ordinary folder browsing retains its existing [sidecar persistence](./editing-sidecars.md).
- The browser must support picking photographs for import and marking photographs as hidden. Hidden is a review decision and must not delete source files.
- Import must offer two operations. Copy originals places selected X3Fs directly in the chosen destination without running an export render. Process + originals places the selected TIFF or DNG output in the chosen destination and copies selected X3Fs into its `x3f` subdirectory.
- Both operations must preserve edits for copied originals under a visible `foveonic` subdirectory of the originals' destination folder, using the latest requested edits rather than waiting for a rendered frame. Copy originals must carry edits without baking them into the X3F.
- Import-mode edits must persist only for photographs that are imported. Unimported edits must not create source sidecars.
- Import must persist hidden-file decisions in one shared file at the destination. A hidden photograph does not need an imported X3F or an individual editing sidecar.
- When reviewing a source against that destination again, the browser must recognize previous imports and hidden-file decisions. It must offer a toggle to hide both groups from review.
- Source identity must distinguish reused camera filenames. Original source path and timestamps may provide useful provenance. Filename alone is insufficient.
- The [temporary cache](./decode-cache.md) must remain disposable and must not be the durable record of imports or hidden-file decisions. Destination cache associations are deferred. Rendering may reuse a verified source cache entry, or decode the completed destination copy on a miss.
- Import review must keep edits and picks in memory, without introducing named sessions or a catalog. Opening another folder discards the previous review's unimported edits and pending review choices, even when the next folder also opens for Import. Remembering the last folder must include its mode. Restarting or explicitly reopening that remembered root, a descendant folder, or one of their X3Fs must preserve Import's source-write protection. A new explicit launch location uses Browse. Existing readable source sidecars may initialize review edits.
- A separate bounded batch queue must own snapshots of selected photographs, requested edits, destination, and output choices taken at submission. Later editing and navigation must remain independent of that submitted work.
- Batch rendering must use the same photographic recipes and TIFF/DNG packaging as ordinary export. An independent executor may share the device, queue, and compiled GPU kernels while retaining separate per-photo state and profiling counters.
- Interactive rendering must take priority over batch work. Background I/O and GPU submissions must yield to queued and active foreground work. Already submitted GPU commands finish normally. Batch work must not queue the entire frame ahead of the next interactive request.
- Requested processing must use the host's utility scheduling class on macOS, while cache and presentation maintenance retain their lower background class. This scheduling choice must preserve the shared foreground gate. Batch rendering must reuse existing decoded sources on a hit, and must not enqueue decoded-cache or preview writes for its cache misses.

## Interaction

Both opening actions use the [recursive grouped browser](./gui-workspace.md). Counts, picks, cache prefetch, history matching, and batch submission must include descendant photographs with their full source paths. Export destination layouts remain as specified below, including numeric collision suffixes for repeated filenames across source folders.

Highlight selection and import checkboxes are separate. Clicking a photograph selects it for viewing. Command/Ctrl-click toggles its highlight and Shift-click extends a range. Command/Ctrl+A highlights all visible photographs. Space checks the highlighted eligible photographs when any are unchecked, or unchecks them when all are checked. Space, Delete/Backspace, Command/Ctrl+A, arrows, and Shift-arrow range selection must work across the workspace without requiring browser focus, following the [workspace keyboard policy](./gui-workspace.md). Focused text and numeric inputs retain their editing keys, and open menus and modal dialogs retain keyboard input. When the highlight set is empty, picking and hiding must use the current visible photograph. Commands must not also activate a focused control. Checkbox and eye clicks must not select a different photo or trigger a render.

The left side of each icon caption or list filename must show one of three controls: an unchecked checkbox, a checked checkbox, or an eye for a concealed state. Already imported and manually hidden photographs both use the eye when visible. Clicking an individual eye restores an unchecked, eligible state. A manual hide reversal is saved on the next Process. Restoring an already-imported photograph makes it eligible for this review without deleting its import provenance.

Delete must uncheck and hide the highlighted photographs, turn global hiding on, and advance a hidden active photograph to the next visible photograph or the preceding photograph at the end. Clear the active view if none remain visible. On macOS the Backspace key also serves this action. Source files must remain untouched. Background import-history updates may conceal rows but must not change the viewed photograph or start another render.

One global eye button in its own browser group with “N picked, N hidden” must toggle concealment of both imported and manually hidden photographs. Turning concealment off reveals their individual eyes. The toggle must leave workspace shortcuts immediately available. The hidden count includes both imported and manually hidden photographs, even when revealed. The group must sit beside the file count and Icons/List controls with a vertical divider when the panel is wide enough, and move intact to the next row in a narrower panel. There are no separate hide filters or Pick visible, Pick, Hide, or Clear toolbar buttons.

The main toolbar must contain one row: Browse / Import opening group, Destination, Processing settings, and Process. Both modes must use Process as their output action. There are no separate Export TIFF, Export DNG, or batch Import buttons. Controls must share the existing padding and vertical centering. The destination control must show a compact folder name with its full path on hover. Remember the last chosen destination in application preferences across restarts, independently of the source folder and photographic sidecars. Application preferences own the shared destination. Import review retains a snapshot only for its asynchronous history scan. The same destination is available in both modes and remains defined before processing so Import can match its history.

Processing settings must open a compact settings dialog and persist separately for Browse and Import. Each mode defaults to DNG. Browse offers one output format, DNG or TIFF, and TIFF color space when TIFF is chosen. Import additionally offers Copy originals or Process + originals. Copy originals retains the existing direct-copy layout; Process + originals writes the selected output format at the destination and originals in its x3f subdirectory. Preserve format and color choices while switching operations. Settings changes must not trigger a photographic render or alter source editing recipes. GUI processing uses the mode's selected TIFF destination space, independently of a legacy sidecar's output-space field.

Process must use the saved mode settings immediately without a per-run format or save dialog. Browse processes all highlighted photographs in browser order, falling back to the viewed photograph when no highlights remain. Each photograph uses its own saved or latest requested edits. Browse must prepare the selected recipes asynchronously before enabling Process. Import processes the picked photographs and pending hidden decisions. Both modes use the same independent frozen batch queue and retain interactive browsing during processing. Browse exports must not copy originals or write import history. A destination must be chosen first. Browse requires ready photographic recipes, and Import must wait for metadata/history scanning and any existing batch.

Processing settings must offer Overwrite and Rename for conflicts with all destination files, including processed TIFF/DNGs, copied X3Fs, and their editing sidecars, with Overwrite as the default. Remember this preference separately for each mode and freeze it with the submitted batch. Overwrite must atomically replace previous files only after their replacements finish successfully. Rename must choose a free numeric filename suffix and refuse to overwrite a file that appears before publication. Different photographs sharing a basename in the same batch must receive distinct output names under either policy.

The Import-mode Process action must save hidden decisions and manual hide reversals even with no photographs selected for copying. Imported and hidden status remains destination-scoped. The viewport remains available for focus checking and editing. Batch progress and errors remain visible after opening a folder with Browse. One batch runs at a time, independently of interactive photo selection.

## Destination layouts

Copy originals:

```text
destination/
  image.x3f
  foveonic/
    image.x3f.foveonic.json
```

Render and copy originals:

```text
destination/
  image.tiff
  image.dng
  x3f/
    image.x3f
    foveonic/
      image.x3f.foveonic.json
```

The rendered layout shows both output types. The chosen output type is generated. The shared import-history file belongs to the chosen destination in either mode, independently of where the originals are copied.

## Identity and destination history

Store one `foveonic/import-history.json` at the chosen destination. Each identity has one current imported or hidden disposition with its original source path as provenance. Update this compact file rather than accumulating an audit log. Hidden decisions do not require per-photo sidecars. Copied photo sidecars retain the editing recipe, while import provenance stays in the shared history file.

Match using the original filename, modification timestamp including available subsecond precision, and file size. The source location is provenance rather than a required match, allowing a card to reappear at another mount location. This is an intentionally inexpensive heuristic. Repeated camera filenames, clocks, and sizes can collide. Import must not read whole originals merely to hash them for matching. Preserve original modification timestamps on copied X3Fs. Creation timestamps are not required for matching.

History matching and hidden decisions must depend on source identity independently of whether an editing recipe can be loaded. Recipe failures must remain visible and must prevent processing that photo unless valid current review edits are available. A delayed history scan must preserve imports completed after that scan and must not undo later checkbox or hidden choices.

An imported disposition must refer to an existing original beneath the destination. A removed or overwritten original becomes eligible again. When replacing an original with a different source identity, remove the previous identity's claim to that destination. Malformed or unsupported history must remain an error and must not be replaced as though empty.

## Failures and conflicts

File conflicts follow the selected Overwrite or Rename policy, using the existing per-file writers. A later file or history failure leaves already completed files in place and reports the photograph for retry. There is no additional import staging, backup, or rollback system. Successful earlier photographs remain imported. Failed photographs remain picked for retry and errors are shown at the end. Required photo outputs must finish before recording that photo as imported. A history-write failure must be reported.

Closing the application can interrupt active import work. Import does not promise resumable jobs or a transaction across the entire batch. Temporary files are not an audit trail or a user-managed recovery system.

## Deferred work

- Destination aliases for existing decoded and display cache entries, with shared LRU accounting.
- Automatic memory-card detection and additional shortcut or presentation refinements after owner use.

Implementation and validation are tracked in the [import plan](../../plans/archived/import-workflow.md).
