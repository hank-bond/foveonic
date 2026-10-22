---
summary: "Review Quattro consistency across family controls, recipe preparation, numerical nodes and shared execution."
status: archived
---

# Quattro organization

The owner requested a focused organization pass, preserving existing photographic behavior and the [family control contract](../../docs/specs/family-controls.md), [Quattro rendering](../../docs/specs/quattro-rendering.md) and [shared output requirements](../../docs/specs/output-strategies.md). This pass adds no photographic features or generalized plugin framework.

## Ownership

| Layer | Responsibility |
| --- | --- |
| Input | Decode by encoding and retain source properties without applying photographic adjustments. |
| Family recipe | Interpret camera calibration, saved adjustments and defaults. Select order, branches and numerical policies. |
| Numerical nodes | Apply functional controls and immutable tables with explicit signal units, rounding and border contracts. |
| Graph and engine | Own immutable resource snapshots, validate dependencies, execute work, preserve immutable results, cache bounded restart inputs and honor cancellation. |
| Shared output | Fit numerical appearance inputs and encode, display or package results. |
| Family UI | Compose appropriate controls using shared widgets and processing-owned defaults and constraints. |

The graph executor and Quattro numerical nodes contain no camera-family branches. Composite filtering nodes express numerical sequences and retained histories, rather than selecting a camera. Existing Merrill metadata preparers remain a separate historical cleanup opportunity, rather than being moved solely to make filenames symmetric.

## Consolidations

- [x] Resolve Quattro display and reset defaults through its recipe for both capture-only and complete metadata. An untouched early noise slider previously changed when full metadata arrived.
- [x] Keep early photographic availability and saved rendering-mark interpretation with Quattro preparation rather than the GUI.
- [x] Share the GUI adapter that turns lightweight capture properties into metadata for default resolution.
- [x] Move source-dependent sharpening policy out of shared Settings into the recipe boundary.
- [x] Use one Quattro color-matrix preparation for photographic reconstruction and DNG appearance, preserving Float32 ISO-factor order and white-balance aliases.
- [x] Keep DNG camera interpretation in family recipes, with numerical appearance inputs consumed by the shared fitter.
- [x] Check cancellation during Quattro preparation at the same requested-output boundaries used by the other recipes.
- [x] Name repeated fixed numerical constants where their units were unclear, retaining exact arithmetic order.
- [x] Verify profile preparation, complete cached/fresh outputs, UI transitions and workspace checks.

## Intentional differences

Quattro retains separate low-resolution RGB and high-resolution blue histories. Its factory Auto calibration, fixed daylight Stage1 normalization and selected photographic WB serve different roles. Combining them would change processing.

Quattro exposes exposure within two stops, signed sharpening, seven-position Smooth/Crisp and a normalized highlight-recovery amount. Its technical reconstruction is mandatory. Color and monochrome retain separate Smooth/Crisp defaults. Monochrome disables WB and color-mode choices. Classic keeps color modes active in monochrome because they select its sensor mixture, while Merrill keeps its monochrome WB behavior. These differences remain in family preparation and panel composition.

The three recipes share graph execution, buffer ownership, active-edit caching, presentation, output conversion, TIFF writing and DNG fitting/encoding/packaging. Their differing numerical reconstruction algorithms remain separate operations. Sharing requires matching units, arithmetic, rounding and borders.

Quattro's visible composition remains linear. Splitting its graph into additional stage-context objects would obscure the two retained source histories without removing policy duplication. The stage module names retain their correspondence with checked reference boundaries. Fixed algorithm coefficients remain node constants when they are not camera-dependent choices.

CPU recipe preparation remains a measured latency issue. This pass clarifies preparation ownership but does not introduce a preparation cache or change its lifetime. Later optimization can use that boundary without putting state into nodes or UI.

## Validation

All 443 ordinary workspace tests, strict all-target Clippy, formatting and documentation checks pass. The GUI transition tests exercise real controls with capture properties followed by full metadata and preserve both untouched settings and displayed defaults. Quattro preparation cancellation stops before later calibration or GPU dispatch.

The moved Merrill profile preparation matches its previous matrices, neutral values, exposure metadata, tone curves and complete look tables exactly for edited Standard, Vivid and monochrome cases at two storage scales. A regenerated saved-default Quattro DNG matches the prior export's IFD payloads and every raw sample byte. Classic, Merrill and Quattro DNG technical-boundary GPU tests pass. Fixed-constant cleanup preserves the bounded proportional-sharpen native fixtures exactly.

The application is rebuilt for owner review. Changes remain uncommitted in the isolated worktree.
