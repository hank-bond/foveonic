---
summary: "Three browser views share star ratings, metadata sorting and text/rating filters without changing photographic recipes."
kind: reference
status: accepted
implementation: conforming
---

# Browser review

The browser must offer a compact image grid, a thumbnail list with metadata columns, and a large single-column image view. All views share the same highlighted selection, viewed photograph, import picks, hidden state, cache badges and relative-folder headings. Images retain their aspect ratios. View and sorting choices persist as application preferences.

The compact grid grows thumbnails with the pane until another minimum-width column fits. Grid cells meet without additional gaps and retain their internal image padding. The grid omits filenames, rating controls and import checkboxes. A green thumbnail border indicates an import pick, and distinct border weights identify the active photograph and other highlights. With multiple highlighted photos, the viewport shows a scrollable comparison. Clicking another image makes it active without discarding the highlighted set. Only the active photo drives editing controls. The active image uses the normal RAW render. Other images use cached rendered previews when available and otherwise use embedded previews. Larger previews stay bounded and prioritize visible comparison items. Pinch changes the comparison scale, and two-finger scrolling moves its canvas.

The eye control must retain its existing meaning of concealing or showing imported and manually hidden photographs. The separate view selector uses grid/list/large-preview symbols. Search stays on the main browser header row when it fits and wraps below it in narrower panes.

A separate star button beside the eye toggles rating controls in the list and large views. Ratings are hidden by default, and the visibility choice persists as an application preference. Hiding ratings removes the list's rating column and the unused Browse image-caption row. The toggle changes neither stored ratings nor rating filters. Keys 0–5 continue to rate photographs in every view while the controls are hidden.

Photographs can be rated from zero to five stars. Clicking stars on a highlighted photograph rates the highlighted visible set. Clicking elsewhere rates that photograph. Keys 0–5 rate the highlighted visible photographs, falling back to the viewed photograph. Zero clears a rating. Clicking the current star rating also clears it. Text fields and menus retain their ordinary keyboard behavior. Ratings are review metadata, independent of image edits and rendering. Browse saves ratings asynchronously in a versioned `foveonic/ratings.json` file per containing folder, keyed by sibling filenames. Import keeps rating changes in memory and saves the submitted rating alongside copied destination originals. Submitted batches freeze ratings with editing settings. Original X3Fs remain unchanged.

Hovering a rating row previews the pointed-to rating with outlined stars. A selected rating uses filled stars. Moving away restores the saved rating without changing it.

Sorting must offer filename, capture date, rating and photographic metadata already available from the parser. Missing values sort last in either direction. Stable filename/path tie breaking prevents arbitrary order changes. Folder groups retain their discovery order, with sorting inside each folder.

Search must perform a case-insensitive contains match against filenames and paths relative to the selected folder. Capture metadata must not contribute hidden text matches. A search popover offers minimum-star filters, including five stars, plus unrated and all ratings. Text and rating filters combine. No color tags or advanced query language are required. Filter changes must not automatically render another photograph. Hidden or filtered photographs are excluded from highlighted processing selections. A viewed photograph may remain displayed while filtered out.

Metadata and rating reads run off the UI thread and use ready cache metadata when available. Thumbnail and selected-photo work retain their existing priorities. Late replies from a previous folder must not update the current browser. Sorting, filtering, arrow navigation, highlighted processing and decode prefetch must use one browser order. Changing ratings must not alter photo settings, preview fingerprints or sensor-cache identities.

Rating writes merge the latest requested values per folder with atomic replacement. Unreadable or unsupported review files must remain intact and their errors must be visible. Rating changes to a highlighted set should share a folder write rather than rewriting the same file for every photograph. The review file and its worker are separate from editing-recipe persistence.
