---
status: archived
summary: "Integrate the approved Merrill multiscale filter and both shield correction choices through the production rendering path."
---

# Ship Merrill multiscale noise

Requirements: [Merrill multiscale noise](../../docs/specs/merrill-multiscale-noise.md), [Detail and Noise](../../docs/specs/detail-and-noise.md), [family controls](../../docs/specs/family-controls.md), and [Merrill preview geometry](../../docs/specs/merrill-fast-preview.md).

The owner approved a production implementation retaining Reference as the default and offering both Rows and Rows + Columns. The accepted visual priority is reduced green/magenta blotching and stripes with useful color and texture preserved. Brightness grain may remain.

## Steps

- [x] Add the shared setting, CLI choice, and Merrill Noise method dropdown with persistence and scoped recipe support.
- [x] Implement one GPU three-scale joint layer filter with named parameters and immutable inputs.
- [x] Check settings, CLI, editing scope, inactive controls, CPU filter calculations, and shader validity.
- [x] Integrate fixed shield calibration and both method choices into the shared Merrill technical recipe.
- [x] Verify method identity, Noise bypass, unchanged Reference processing, Full and regional preview agreement, and TIFF/DNG technical parity.
- [x] Complete production-only source review, workspace checks, documentation indexes, photographic review, and commit.

## Validation

The workspace CPU suite passes 654 tests. Strict workspace Clippy, formatting, generated indexes, and five artwork tests pass. Two independent GPU node fixtures cover model states, shield guards, geometry, native layer-sum preservation, immutable clipping, and sensor margins. Three GPU integration tests cover cached method switches, Noise and monochrome bypasses, native Full/Fast/Draft ordering, regional agreement, and shared TIFF/DNG preparation.

Reference TIFFs match the retained baseline byte for byte on three Merrill bodies. Matched photographic review includes dark desk and speaker regions, the screen boundary, skin, and fine hair. Both choices retain the accepted blotch reduction. Rows + Columns changes broad shadow tint, so both choices remain available.

The shield prediction agrees with the retained independent calculation within 0.00025 native codes on real photographs. Multiscale processing agrees with independent Float64 equations within 0.0014 native codes in patches around the largest prototype differences. The earlier tensor prototype has larger numerical differences, so photographic prototype bit equality is not an acceptance condition.

## Limits

The shipped shield coefficients were fitted from 340 caps on three physical bodies. They describe an additive green-magenta error prediction, without per-image fitting or temperature input. They do not establish a population-wide sensor response. The fixed column reference requires the supported 4704-column active geometry.

Filtering preserves the native layer sum and immutable source provenance. Rendered color and luminance also depend on subsequent calibration and reconstruction. The reference row detector can reject substantially more patches after joint filtering, so fine sensor-aligned lines remain a separate improvement task. This implementation does not change that detector or promote the subsequent banding experiments.
