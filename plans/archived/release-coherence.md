---
summary: "Record the release component review, structural cleanup and validation before grouped commits."
status: archived
---

# Release coherence review

The owner approved cataloging and cleaning up the current uncommitted release, preserving the photographic logic wired into the app. Older tone experiments and their report tooling may be removed. Performance work, commits and publishing are outside this pass. The owner also approved removing unused experimental code and settings migrations throughout the affected features.

## Component catalog and sequence

- [x] Shared UI: button groups, section headers, responsive form columns, dropdowns, menus and shared icon behavior. Remove the Basic/Advanced padding exception and redundant family-specific composition.
- [x] Adaptive tone: retain the reviewed regional fit, per-profile shared limits, exposure awareness and architecture adapters. Remove superseded experiments and keep regression coverage of the selected behavior.
- [x] Dehaze and Corrections: review numerical ownership, optional estimation, asynchronous picking, bypass and the shared shadow protector.
- [x] Saturation and clipping: review Reference/Universal boundaries, common perceptual arithmetic, policy ownership, balanced controls and DNG parity.
- [x] Advanced Color Range and direct editing: review saved versus transient state, bounded names, list/wheel layout and shared gestures.
- [x] Exposure Auto: review metadata resolution, toggle semantics and dynamic presets/defaults.
- [x] Smart preview and speckles: review quality policy and common donor repair without changing accepted processing.
- [x] Cross-cutting closeout: inspect serialization, preview identity, inspection/export plumbing and documentation. Run required checks and photographic regressions appropriate to the cleanup.

The main ownership boundaries are numerical nodes in the core, camera context in recipe adapters, persisted edits in the shared settings schema, transient tools in the editor, and reusable layout/visual behavior in GUI widgets and common controls. Supporting changes belong to their feature rather than a separate processing framework.

## Review findings

The initial inventory includes 126 modified tracked files plus new processing, controls and test modules. The reference recovery changes are source-summary edits. The sensor arithmetic change is the compact speckle repair using common donor pixels for every layer. Most preview/export changes support the feature groups above.

Basic/Advanced currently uses a separate button-group helper with no vertical padding. The normal group already owns the intended padding. Merrill also retains a Tone wrapper identical to the shared Tone path.

The superseded tone experiment is test-only, but still registers seven kernels and retains two shader copies, a report generator and photographic comparison probes. Production Adaptive tone has its own numerical and integration tests.


The cleanup replaces HDR, Color Range, noise and highlight-recovery migration readers with direct current-schema deserialization. Removed compatibility includes percentage HDR conversion, six-color arrays and CLI aliases, retired noise override records, recovery aliases/cutoff conversion, the old Sharpening field name and adjacent sidecar discovery/moving. Current metadata readers for older physical cameras remain supported. Missing fields retain ordinary defaults.

Adaptive tone keeps its production regression probes. Superseded photographic report generators and experiment-only kernels/shaders are removed. Regional weighting and shadow-penalty switches had no current off caller and are now unconditional. Fitted-amount readback remains test-only. Dehaze likewise always uses its shared protector for positive Amount, with measured fixtures replacing the old unprotected runtime switch.


Shared UI now uses one padded button-group helper and one header height. Exposure and Dehaze use a common square Auto button. Form rows reserve its measured width, and the cross-theme/font geometry checks include it. The Dehaze swatch uses egui's existing sRGB transfer functions instead of a second local implementation. Noise, Color Range and Saturation retain the shared responsive form column. The obsolete persisted import-only destination name is removed.

Saturation review confirms one perceptual WGSL operation for global, tonal, Basic and Advanced adjustments. Reference saturation keeps its deliberate native direction, with the same shared policy setting and a matching sparse DNG sampler. The CPU sampler is required for profile fitting rather than an alternate image renderer. No additional processing framework is needed.

Color Range review confirms that names are serialized metadata, excluded from GPU keys and render comparisons. Tab, selection, rename and picker state stay in the editor. Basic and Advanced reuse the gesture implementation and shared range bounds. The clean classification guide is constructed once for the two current range operations.

Exposure Auto resolves capture intent at the shared settings boundary and clears Auto on the effective request to avoid applying compensation twice. Authored settings and defaults retain the dynamic choice. The A row and source menu use common controls. Smart uses the existing Fast recipe, with no duplicate reconstruction path. The compact speckle repair uses one radius and common trimmed donors across layers and retains the existing highlight protection.

The first full ordinary workspace pass succeeds: 407 core, 299 GUI, 14 CLI, 16 task-runner, and 19 documentation tests. Nine Dehaze GPU fixtures pass after removing the unprotected switch. Current Adaptive tone, compact speckle, shared clipping and cross-family preview/DNG checks are retained for final validation. Performance measurements were not run.

The final preview audit removes the obsolete Draft quality and its Merrill reconstruction, banding and ColorDQ bypasses. Smart and explicit Fast now have one reduced recipe. Current regression cases use Full/Fast rather than retaining the former behavior as a diagnostic. The reduced original-file test retains noise application, geometry, cached/fresh equality, immutable outputs, native inspection and full DNG checks. Native Dehaze replay now checks opt-in Auto and manual reset rather than assuming Auto is the default.


Final checks pass for the current numerical paths: nine Dehaze GPU fixtures, Adaptive tone signed/black fixtures, six compact speckle cases, Reference clipping across camera families, and Classic/Merrill/Quattro Adaptive preview and Dehaze DNG comparisons. After Draft removal, the original-file Fast, multiscale and directional-banding checks pass. The ordinary workspace suite, strict Clippy, formatting, document indexes and five artwork tests pass. The cache handoff test now gives its separate asynchronous clear job a fresh completion deadline rather than sharing the insertion deadline.

Native review passes eight Advanced Color Range captures, ten Dehaze captures, eight Exposure Auto captures and twelve Smart captures, including denoised overview, detail, pan, edits and cache panels. The review bundle is rebuilt locally. No commits, publishing, installation or photographic default changes are part of this cleanup. Retired settings are no longer translated. Files containing unsupported fields can be rejected by the current strict structs and remain protected from overwrite.

- [x] Owner review of the catalog and cleanup before committing the release work.

The owner approved grouped commits after the cleanup review. Shared UI widgets, speckle repair, current settings schemas, shared finishing, editor integration, preview policy and documentation form separate commit groups.
