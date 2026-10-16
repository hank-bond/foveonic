---
status: archived
summary: "Inspect operation inputs and outputs alongside amplified signed layer/channel differences."
---

# Inspection differences

## Closure

The owner accepted the implemented interface results and authorized closing review-only plans. Native captures were completed as recorded below, and the visual-review checkpoint is closed. Combined RGB difference heatmaps and branching scalar diagnostics remain outside this completed slice.

Requirements: [pipeline inspection](../../docs/specs/pipeline-inspection.md), [render resources](../../docs/specs/render-resources.md), and [GUI workspace](../../docs/specs/gui-workspace.md).

## Steps

- [x] Expand group rows inline with right-aligned arrows and pointer tests.
- [x] Define settings-aware comparison inputs for compatible layer/RGB operations.
- [x] Retain paired GPU views from one render configuration and subtract before display mapping.
- [x] Add Difference, display-only amplification, explicit baseline labels, and unavailable explanations.
- [x] Validate mapping, paired worker requests, GPU presentation, and documentation checks.
- [x] Resolve the release-build artifact mismatch and build the native bundle.
- [x] Review the drag-publication failure and passing unchanged-binary rerun; owner approved checkpointing with the failure preserved.
- [x] Add the agreed Input/Output toggle, sharing the retained Difference pair and preserving its side while Difference is on.
- [x] Check side switching, pending fallbacks, navigator identity, unchanged scopes, and GPU-pair reuse.
- [x] Complete native screenshots. Owner accepted the implemented interface and closed the visual-review checkpoint.

## Scope

The owner approved Input/Output flickering beside Difference. Output is the default. Both sides use the same component and geometry, and scopes stay on the output. Difference temporarily overrides and disables the side buttons. Compatible RGB views can flicker even though combined RGB Difference remains unavailable.

First slice: signed grayscale layer and individual RGB-channel differences. Combined RGB change-magnitude heatmaps and branching scalar diagnostics remain a subsequent slice. Original sensor readings, color conversion, tone's cross-unit conversion, final composite/export geometry, and diagnostic side outputs do not have an enabled difference in this slice. Scopes continue analyzing the selected output, not the display-only difference.

## Progress

Resumed with owner approval against the updated worker, preserving image-first publication, deferred scopes, cancellation, and active-edit caching. One paired request produces both views; only the input texture is retained after its view is prepared. Difference defaults to 32× display amplification (1–1024×). Toggling off retains the pair for reuse; component/amplification changes do not schedule photographic processing. Both main viewport and deferred navigator share signed-gray mapping; navigator keys include difference amplification. Pending comparisons are explicitly labeled rather than treating an old output as a difference.

Workspace tests pass (GUI: 63 passed, seven GPU/fixture tests ignored); strict all-target GUI Clippy passes. Explicit GPU tests pass for signed differences at gains 1/32/1024, negative/over-range samples, sRGB/Adobe RGB and camera-code/linear units, normal mapping with retained inputs, and existing navigator mappings. A real Merrill worker test passes eight layer/RGB operation-input pairs plus disabled-stage baseline selection and unsupported-output rejection. Pointer tests exercise disclosure and Difference availability/request-once behavior.

Owner menu review caught two gaps: Unicode disclosure glyphs rendered as missing-font squares, and the ComboBox's default click-anywhere dismissal closed it on group clicks. Disclosure icons now reuse the drawn triangle from tool headers. Inspect dismisses on outside clicks (or Escape), with explicit closure only when a component/scalar is chosen. A pointer regression test exercises the actual ComboBox: opening/collapsing groups leaves it open and preserves selection, while component/scalar/outside clicks close it. The isolated row test also requires a drawn triangular path.

The release-build blocker was stale shared Cargo artifacts from a review snapshot, not a missing source API. Cargo marked the processor release library fresh despite its dependency list omitting `x3f/capture.rs` and its symbols omitting the capture reader. `cargo clean --release -p foveonic` invalidated only processor release artifacts; the app packaging command (now `cargo xtask bundle`) then rebuilt both crates and the native bundle successfully. Review snapshots must use separate Cargo target directories, as recorded in the responsiveness handoff. No capture source or build-script change was needed. Opt-in native evidence is ready via `FOVEONIC_GUI_SMOKE_INSPECTION=1` with `FOVEONIC_GUI_SMOKE_DIR`; it covers layer differences, amplification, component reuse, normal output, RGB-channel difference, and final return. Native screenshots and owner review are still pending.

The checkpoint attempt without tone reuse passes release workspace tests (52 ordinary core, 64 GUI), strict all-target Clippy, example builds, formatting, documentation checks, and eight Python tests. Its GUI GPU suite passes six tests, including paired comparisons and signed presentation, but `continuous_drag_makes_progress_then_settles_with_matching_scopes` fails at `worker.rs:616` with `continuous dragging starved image publication` during the `base-blend` sequence. The preceding independently rebuilt guard-only snapshot passes all five GUI GPU tests, including this drag test. This is a publication-progress failure, not an exact-output mismatch; its cause and reproducibility are not established. The inspection checkpoint was held pending a targeted rerun. Coordinate any scheduling investigation with the editing worktree; no test or scheduling workaround was applied.

The owner-approved targeted rerun passes all eleven drag controls with the unchanged frozen inspection executable. Every control publishes during dragging and settles with matching settings/scopes; `base-blend` publishes two frames during its sequence. No other Foveonic/Cargo process was detected before this rerun. GPU contention is a plausible explanation for the earlier timing-sensitive failure, not an established cause; no code or test changes were made.

## Input/Output flickering

The session-only comparison controls now select Input or Output beside Difference. The worker's comparison request retains the same paired views for either feature, including compatible combined RGB. Switching an already prepared pair does not submit a photographic job. Difference disables the side buttons and restores their stored selection when turned off. Unsupported stages return to Output, and pending inputs explicitly show the current output instead.

Main presentation and the navigator select the same retained side. Navigator identity includes the side, so it cannot reuse the other side's preview. A fixed single-line label preserves canvas geometry when flickering. Scope analysis still reads the output artifact.

Workspace tests pass with 106 GUI tests and eight opt-in tests skipped. Pointer tests cover pair requests, disabled side buttons, restoration after Difference, unsupported stages, and combined RGB. Mapping tests cover pending fallbacks. Three explicit GPU tests pass for paired worker frames, normal/difference presentation, and output-only fallback. Retained input/output texture identities stay unchanged across repeated side selections.

The offline release app bundle builds. Twelve isolated native captures cover both sides in layers and combined RGB, plus Difference and final return. The native log contains only the four expected photographic publications. Input/Output captures have identical scope panels and viewport readouts, while navigator pixels change with the side. Owner interaction review remains pending.

## Imported banding photograph

The unchanged DP2 Merrill original `_P2M0792.X3F` was added to the fixture collection as `paper-and-recycling-can.x3f` (48,458,588 bytes). SHA-256: `8276657823dcdbf1baf6d6d9e654f694ec4e9a185f05c8a9a2009936118cf299`. This is the paper-and-municipal-recycling-can ISO 200 banding example.

`FOVEONIC_FIXTURE_DIR=path/to/x3f cargo test --offline -p foveonic decode_original_merrill_files -- --ignored --nocapture --test-threads=1` passes on all seven local originals, including the imported photograph. The existing directory-scanning decoder test includes it automatically; tests explicitly listing the original six are unchanged.

Preserve the concurrent interactive rendering work. Comparisons use multiple roots in one request, not separate settings snapshots or a second photographic processing path. Reconstruction compares with its RGB base; noise and column layers each compare with their common pre-filter layer input, not with one another.
