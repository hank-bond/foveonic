---
status: archived
summary: "Viewport scopes follow the visible crop, keep plots visible during updates, and use smooth waveform placement."
---

# Viewport scopes

Requirements: [GUI workspace](../../docs/specs/gui-workspace.md) and [interactive rendering](../../docs/specs/interactive-rendering.md).

## Steps

- [x] Implement the persistent Viewport only toggle in a separate worktree.
- [x] Reuse visible bounds and the displayed GPU buffer for background scope updates.
- [x] Keep plots visible while replacement results are pending.
- [x] Add pixel-aligned display resizing and fractional waveform/parade placement.
- [x] Check unit, GPU, worker, and native behavior.
- [x] Review the app with the owner, then simplify code and prose.
- [x] Merge only the scope changes and remove the worktree and temporary artifacts.

## Findings

The viewport and navigator already share crop geometry. Scope requests convert the viewport's inclusive pixel bounds to excluded right/bottom bounds. One replaceable pending scope job uses the existing worker, below image-processing priority. Results carry an image revision and region. Requests do not clear completed plots, and stale replies cannot replace the current request. Changing the displayed buffer still resets scopes.

The black flicker came from clearing plots before replacement data arrived. The interference pattern came mainly from snapping waveform samples to single cells. Display resizing alone was not enough. Bilinear placement now splits each sample's 256 weight units across nearby cells, preserving their total. Histogram counts, image processing, and export are unchanged. Display resizing filters only the colored bitmap, draws it at physical pixel size, and keeps parade colors separate.

The owner accepted the result and approved review, cleanup, and merge. The review removed bitmap copies and repeated brightness calculations, shortened help and documentation, and retained one scope-analysis path. Integration keeps the newer editing and styling changes, including their shared checkbox style.

## Checks

- Workspace tests, strict all-target Clippy, formatting, diff checks, and documentation/index checks pass.
- GPU tests cover four rotations, region bounds, Fit equivalence, resized outputs, sample-weight totals, fractional brightness changes, column striping, and finite clipping. View/export regression tests pass.
- Worker tests cover 100 replaced crop requests, latest-result delivery, whole-image switching, and continued editing progress.
- Thirteen native captures on each of two portrait/landscape originals cover pan, zoom, resize, all scope types, input switching, and buffer switching. They verify no blank refresh frames, unchanged histogram counts, and conserved waveform weight.
- Display tests cover physical-pixel alignment, filtering, channel separation, brightness mapping, and texture-handle reuse.
- The merged app passes workspace tests, strict Clippy, documentation checks, GPU/worker scope tests, and all thirteen native states on both originals.

A new fixture also exposed unreliable existing NaN detection on Metal. That issue is outside this visual-helper change and remains deferred.
