---
summary: "Persist shared photographic settings in atomic versioned JSON sidecars and restore them independently of render completion."
---

# Editing sidecars

Requirements: [editing sidecars](../docs/specs/editing-sidecars.md).

## Steps

- [x] Agree adjacent versioned JSON, automatic GUI save/restore, atomic writes, and editing-only content.
- [ ] Implement strict shared serialization and bounded reads, with numeric validation and named override linkage.
- [ ] Restore per-photo settings on selection and move coalesced writes off the UI/render workers.
- [ ] Preserve requested edits across supersession, image changes, and orderly shutdown; protect invalid/unreadable sidecars.
- [ ] Validate recipe round trips, atomic failures, asynchronous selection/save behavior, and exact photographic replay.
- [ ] Review and checkpoint the first slice before DNG work.
- [ ] Agree CLI recipe-selection/override precedence before CLI integration.

## Scope

One recipe per X3F, named `<original filename>.foveonic.json`. Store processing settings, including automatic versus manual choices and advanced override linkage; omit scopes, viewport, organizer, inspection, and generated LUTs. Original X3Fs remain unchanged. Disabled reconstruction currently discards its controls in the GUI; this slice preserves that existing behavior rather than adding a second dormant settings model.

Use a dedicated filesystem worker: one newest pending load and coalesced pending saves per photo, not a FIFO of slider snapshots. Finish pending saves on orderly shutdown. Invalid loads disable automatic writes for that selection and remain visibly reported. Retain failed-save recipes for explicit retry. Missing recipes initialize per-photo defaults, replacing the previous session-wide carryover of unrelated edits.

Responsiveness continues in `/Users/work/git/foveonic-editing`; do not modify that worktree. Guard/inspection commits are `d916440` and `c3c7572`. Tone-table reuse remains uncommitted and must be excluded from checkpoints. No DNG import or recipe recovery from exported metadata is planned.
