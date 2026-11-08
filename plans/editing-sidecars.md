---
summary: "Persist shared photographic settings in atomic versioned JSON sidecars and restore them independently of render completion."
---

# Editing sidecars

Requirements: [editing sidecars](../docs/specs/editing-sidecars.md).

## Steps

- [x] Agree adjacent versioned JSON, automatic GUI save/restore, atomic writes, and editing-only content.
- [x] Implement shared serialization and bounded reads, with numeric validation and named override linkage.
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

### Alpha field tolerance

The owner chose ordinary field tolerance during alpha development instead of migrations for changing controls. A demo sidecar containing an experimental `research` object previously blocked both edit restoration and cache-bundle preparation. The four affected local demo recipes were backed up and deleted at the owner's request. Their contents remain useful inputs for validating the tolerant reader.

- [x] Ignore extra fields in the sidecar envelope, shared settings and nested photo-control models. Keep unrelated import/review/calibration record policies intact.
- [x] Keep existing Serde defaults for missing settings and shared validation for supplied known values. Preserve duplicate-known-field checks by parsing original JSON bytes.
- [x] Record field differences only during an active shared diagnostic capture. Ordinary loads do not project or enumerate the serialized model.
- [x] Check nested unknown fields, missing defaults, supplied-value errors, saved edits and the real demo recipe. Rebuild the local review app.
- [ ] Collect owner feedback and a saved capture from the updated review app.

The format/version marker is unchanged and no migration reader was added. Diagnostic field paths describe defaults applied and fields not retained by the current serialized model, including conditionally omitted empty labels. They do not control parsing. Retired control objects are ignored without translating their algorithms into current settings.

The workspace tests pass with 519 core and 416 GUI tests, followed by the targeted sidecar tests, strict workspace Clippy, format/index checks and artwork checks. An isolated real-original cache check loaded the unmodified experimental hazy-coast recipe, published a Ready bundle and reopened its 48,254,976 samples through the local decoded cache. Supplied known edits in the selection snapshot matched the restored recipe. This proves the reported sidecar failure no longer blocks readiness without relying on deletion of that recipe.

The owner's earlier ordinary review launch was not recorded, so its actual render scale and cache-read behavior cannot be recovered. The next review used opt-in Wall recording with ordinary application preferences/cache and a declared capture directory, but closing without manual save discarded that in-memory recording. The resulting shutdown fix is tracked in the [diagnostic plan](./performance-diagnostics.md). Controlled latency claims still require the workload check and separate Off runs.

### Earlier acceptance

The owner checked several editing choices across application restarts and accepted the observed behavior. Native failure/retry and export interactions were not separately reported; automated tests cover protected recipes, failed-save retention/retry, photo switches, and shutdown flushing. The tone-free checkpoint passed 57 ordinary core tests, 71 ordinary GUI tests, CLI and docs-index tests, strict Clippy, example builds, format/index checks, and eight Python tests. Exact GPU replay of a restored recipe passed. Restored settings also normalize unavailable inspection choices before the first render request. After integration with the maintained profiling slice, the combined workspace passed 61 ordinary core tests, 78 ordinary GUI tests, all 54 ignored core and seven GUI tests, strict checks, release workspace/example builds, indexes/formatting, and 21 Python tests. Exact six-original retained/fresh/golden floats and cold/warm TIFFs, sidecar replay, paired inspection views, and continuous dragging passed. These correctness runs do not establish a performance improvement or a fix for the deferred mismatch.
