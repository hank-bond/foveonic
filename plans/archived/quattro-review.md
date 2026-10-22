---
summary: "Review Quattro Fast integration for recipe ownership, named numerical controls, clear geometry and preserved output."
status: archived
---

# Quattro integration review

The owner requested a final architecture and quality pass before wrapping up the Quattro work. The completed review preserved the reviewed Fast appearance and Full output, following the [Fast preview requirements](../../docs/specs/quattro-fast-preview.md) and existing recipe, operation, GPU ownership and bounded worker conventions.

- [x] Inspect the Fast recipe, shared numerical nodes, geometry, cache identity and GUI policy boundaries.
- [x] Name and explain new numerical controls and simplify repeated geometry handling.
- [x] Clarify crop and reconstruction APIs without moving camera policy into shared operations.
- [x] Reconcile completed experiment notes and record deferred work.
- [x] Verify Full baselines, reviewed Fast output, cached edits, exports and native presentation.
- [x] Run workspace and documentation checks and rebuild the local app.

Weighted progress, formal node receipts and a timing DAG viewer are deferred at the owner's request. Background decode, disk caching, sidebar preview persistence and automatic viewport refinement remain separate future slices.

The review found the existing boundaries appropriate. Fast shares HIGH calibration and the authored technical stages, selects its working grid in the Quattro recipe and runs through the ordinary executor. Full inspection, comparisons, monochrome and exports retain their existing paths. Numerical operation keys distinguish same-size reconstruction and changed finishing geometry. Preview preference serialization stays separate from photo editing sidecars.

Cleanup names the reduced filter support and variance approximation, LOW-derived finishing factors, residual bounds, Q16 cell-center offset and ordered cluster patch geometry. Stage2B and Stage3 now share the recipe's working-extent conversion. The final crop retains its physical extent directly instead of recovering it from a reduced extent. The crop API explicitly says `on_half_grid` rather than accepting an unexplained boolean. Arithmetic order, operation keys and graph output names remain unchanged.

The Preview control stays beside Inspect, with shared group margins used for drawing and reserved height. The loading bar's hover text retains displayed-quality provenance while its normal contents remain a bar and milliseconds. Completed performance work is archived separately from the earlier archived optimization pass, with historical notes and deferred opportunities preserved.

## Validation

All 454 ordinary workspace tests and strict all-target Clippy pass. The complete Full Float32 outputs match the frozen baselines on DP0, DP1, DP2, DP3, sd Quattro and DP3 ISO800. Fresh/cached Fast renders match through exposure and noise/detail edits, quality switches and retained-image checks on all six originals. All six Fast RGB16 TIFFs remain byte-identical to the previously reviewed integration outputs. Bounded averaging/residual and filtered-guide GPU tests pass. A CPU check verifies that same-size and doubled reconstruction have distinct keys and reject mismatched filter geometry.

The GUI export test now checks both TIFF and DNG byte-for-byte against independent Full exports while the workspace requests Fast and an inspection root. It returns to Fast afterward. Native landscape and portrait runs pass Full/Fast switching, zoom, edits, scope regions, native coordinates and completed progress checks. The compact viewport groups and bar/total display were visually checked. The local app bundle was rebuilt without installation. Documentation generation/checks, formatting and diff checks pass.

These checks preserve the demonstrated preview appearance. They do not establish native LOW parity, automatic Full refinement, broader high-ISO quality or performance proportions across hardware.
