---
summary: "Reference-style GPU noise filtering and detail reconstruction provide an inspectable photographic baseline."
kind: reference
status: accepted
implementation: conforming
---

# Detail and noise processing

## Requirements

- Noise filtering and detail reconstruction **must** run in GPU shaders through the shared GUI/CLI engine.
- The first implementation **must** follow the reference algorithms, stage order, and camera-derived parameters. Necessary departures **must** be documented and checked with arithmetic fixtures and photographs.
- The first slice **must** provide full-resolution noise-aware base filtering, edge-guided smoothing, luminance conditioning, and residual detail reconstruction. Column/pattern correction and the reduced-resolution branch remain follow-on work.
- The graph **must** expose useful image stages for inspection and preserve the settings behind retained artifacts.
- Controls **must** be shared by the GUI and CLI. Reconstruction **must** remain opt-in during photographic review. Camera-record repair and despeckling use the independently agreed enabled defaults in [processing graphs](./processing-graphs.md).
- Modern alternatives **must** use distinct node implementations so their photographic results and resource use can be compared with the reference-style baseline.

## Implementation direction

Use the reference's 4×4 processing tiles with 64-pixel context. Prepare camera noise models and control-table selections in Rust. Shader stages build a bilateral/edge-guided base, condition luminance with diffusion, derive residual gains, and restore detail after camera color conversion and optional ColorDQ. Keep the signed detail signal available for inspection.

The first slice uses full-resolution filtering across ISO choices. Chroma and luminance controls select the reference's five table rows, numbered 0–4 with default 2. A finite nonnegative detail multiplier scales the reference residual strength, with 1× retaining that selection and 0× displaying the filtered base.

## Inspection and controls

`Settings::reconstruction` enables the path with shared `noise::Controls`. The GUI exposes Reference reconstruction, Chroma noise, Luma noise, and Detail. CLI options are `--reconstruction`, `--chroma-noise`, `--luma-noise`, and `--detail`; parameter options also enable the path.

Public images are `noise.layers` (calibrated sensor codes), `noise.edges` (recursive weights, nominal 0–1), `detail.ratio` (multiplicative luminance residual, displayed 0–2 with 1 neutral), `reconstruction.rgb` (working linear camera-color RGB in 0–4095 code units), and `detail.signal` (signed camera-code detail, displayed −1000…1000 with zero at middle gray). The existing `color.rgb` and optional `color-dq.rgb` expose the filtered base before restoration. Display mappings preserve processing values. Tile atlases remain internal typed connections.

## Linked advanced controls

- Selected advanced parameters **should** each have one basic reconstruction control as their parent. Camera/ISO metadata may also contribute to their calculated values. Several controls may influence the same visual effect.
- Each advanced control **must** support linked and overridden states. Linked controls **must** display their calculated values, look slightly darker, and be read-only while the link button remains interactive. Unlinked controls **must** use explicit overrides and normal editable styling.
- Unlinking **must** capture the current calculated value without changing the image. Changing a basic control **must** update its linked advanced values and preserve unlinked overrides. Relinking **must** adopt the latest calculated value.
- The detail multiplier **must** remain separate and apply after linked/overridden parameters are resolved. It does not become another parent of those advanced controls.
- GUI displays and engine execution **must** use the same parameter calculation. Retained artifacts **must** preserve the selected basic controls and overrides.

The first advanced set exposes existing preset-driven fields with their own link buttons:

| Parameter | Basic parent | Manual range |
| --- | --- | --- |
| Base filter blend | Luma noise | 0–1 |
| Mean edge cutoff | Luma noise | 0–0.99 |
| Mean decay | Luma noise | 0–1 |
| Edge sensitivity | Luma noise | 0.01–4 |
| Luma noise scale | Chroma noise | 0–64 |
| Brightness mix | Chroma noise | 0–1 |
| Layer detail mix | Luma noise | 0–1 |

These coefficients retain the reference's coupling and nonlinear effects; higher numbers do not uniformly mean stronger noise reduction. Tooltips describe each field and its parent. Camera-only bilateral range/radius, diffusion step/contrast, and extrema clamping remain internal. Mean decay near 1 can reveal finite-tile boundaries.

`Controls::overrides` stores optional values by `noise::Parameter`; `None` is linked. `noise::calculated_values` uses the shared engine preparation with overrides removed. CLI accepts repeated `--noise-override KEY=VALUE`, enabling reconstruction. Zero/Off semantics remain a separate decision; the reference's preset 0 and detail multiplier 0 retain their existing meanings.

## Baseline boundaries

The full-resolution slice retains existing calibration/repair and explicit ColorDQ selection. Column correction, pattern suppression, row preprocessing, the high-ISO reduced-resolution branch, automatic ColorDQ selection, and highlight reconstruction remain follow-on work. Signed reference detail is available for inspection; final sharpening continues through the existing optional Gaussian node.

Shaders use float32 arithmetic and owned, initialized workspaces. Source images stay immutable. Tile extraction defines outer context corners, and directional passes preserve their initialized tile borders. The signed signal has a zero-filled two-pixel tile border. The 64-pixel context separates these workspace borders from projected cores. Numerical checks cover reference interior arithmetic, diffusion's delayed border writes, and overlap ramps.

## Conformance

Checks cover metadata-derived ISO/models, gain scaling, bilateral/edge filtering, recursive directional means, diffusion and extrema borders, ratios, residual mixing, reconstruction, signed detail, and tile-overlap projection against retained reference fixtures. GPU integration checks cover selective execution, independently retained image outputs, scalar meanings, all five noise choices, explicit ColorDQ, and zero sensor gain. Six original Merrill photographs provide native captures, native-size TIFF comparisons, and timing/memory evidence in the [implementation plan](../../plans/detail-and-noise.md). Owner photographic review and cross-platform runtime checks remain pending. Linked-control checks cover parameter bounds, single-parent calculation, exact captured-parameter equality, preserved overrides, pointer-driven unlink/relink, changed filtering, and finite output at manual extrema. Native checks on six originals and UI-preference restart evidence are recorded in the [editor review plan](../../plans/editor-review-focus.md).
