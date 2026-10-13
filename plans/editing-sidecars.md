---
summary: "Persist shared photographic settings in atomic versioned JSON sidecars and restore them independently of render completion."
---

# Editing sidecars

Requirements: [editing sidecars](../docs/specs/editing-sidecars.md).

## Steps

- [x] Agree adjacent versioned JSON, automatic GUI save/restore, atomic writes, and editing-only content.
- [x] Implement strict shared serialization and bounded reads, with numeric validation and named override linkage.
- [x] Restore per-photo settings on selection and move coalesced writes off the UI/render workers.
- [x] Preserve requested edits across supersession, image changes, and orderly shutdown; protect invalid/unreadable sidecars.
- [x] Validate recipe round trips, atomic failures, asynchronous selection/save behavior, and exact photographic replay.
- [x] Review and checkpoint the first slice before DNG work.
- [ ] Agree CLI recipe-selection/override precedence before CLI integration.

## Scope

One recipe per X3F, named `<original filename>.foveonic.json`. Store processing settings, including automatic versus manual choices and advanced override linkage; omit scopes, viewport, organizer, inspection, and generated LUTs. Original X3Fs remain unchanged. Disabled reconstruction currently discards its controls in the GUI; this slice preserves that existing behavior rather than adding a second dormant settings model.

Use a dedicated filesystem worker: one newest pending load and coalesced pending saves per photo, not a FIFO of slider snapshots. Finish pending saves on orderly shutdown. Invalid loads disable automatic writes for that selection and remain visibly reported. Retain failed-save recipes for explicit retry. Missing recipes initialize per-photo defaults, replacing the previous session-wide carryover of unrelated edits.

Guard/inspection commits are `d916440` and `c3c7572`. The unaccepted tone-table reuse prototype is preserved separately and excluded from this checkpoint. No DNG import or recipe recovery from exported metadata is planned.

## Review

The owner checked several editing choices across application restarts and accepted the observed behavior. Native failure/retry and export interactions were not separately reported; automated tests cover protected recipes, failed-save retention/retry, photo switches, and shutdown flushing. The tone-free checkpoint passed 57 ordinary core tests, 71 ordinary GUI tests, CLI and docs-index tests, strict Clippy, example builds, format/index checks, and eight Python tests. Exact GPU replay of a restored recipe passed. Restored settings also normalize unavailable inspection choices before the first render request. After integration with the maintained profiling slice, the combined workspace passed 61 ordinary core tests, 78 ordinary GUI tests, all 54 ignored core and seven GUI tests, strict checks, release workspace/example builds, indexes/formatting, and 21 Python tests. Exact six-original retained/fresh/golden floats and cold/warm TIFFs, sidecar replay, paired inspection views, and continuous dragging passed. These correctness runs do not establish a performance improvement or a fix for the deferred mismatch.
