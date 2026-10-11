---
summary: "Numerical fixtures retain reference filtering, reconstruction, tile-blending, and residual-defect repair evidence."
kind: reference
---

# Processing reference fixtures

These fixtures were generated with the owner's existing `~/git/foveonic/target/release/libfoveonic.dylib` diagnostic library while auditing source at `e47af35`, including the permitted unstaged changes. The reference repository remained read-only.

- `noise-reference.json` records component outputs for a deterministic 80×72 synthetic signal and a separate 2×2 tile-blending case. [The component test](../../src/reconstruction_test.rs) defines the input, parameters, sample locations, and arithmetic tolerances. Reference functions are `filter_tables`, `radius_map`, `bilateral`, `edge_map`, `edge_lut`, `local_mean`, `brightness`, `noise_diffusion`, `diffusion`, `osf`, `ratio`, `color_mixing`, `foveonic_reconstruction_signal`, `apply_color`, and `foveonic_blend_tiles`.
- `noise-parameters.json` records `foveonic_processing_params_source` results for the six original Merrill photographs, using Sunlight WB and each file's aperture/shutter properties. [The parameter test](../../src/noise_test.rs) checks the selected tables, histogram-derived ISO, WB-weighted model, and global sensor-gain scaling.

- `repair-reference.json` records the blue sign's two unlisted red-speckle positions, original sensor samples, and reference `foveonic_calibration_source`, `foveonic_repaired_source`, `foveonic_restored_source`, and `foveonic_despeckled_source` results. Sunlight WB, aperture 3.2, one exposure, and exposure gain 1 were used. Regular repair and highlight restoration leave both centers unchanged; despeckling emits red-layer records (`0x43ff`) and corrects them. [The record-selection test](../../src/repair.rs) checks that current camera-list selection leaves these source positions unmarked at cutoffs 0–3. The implemented separate GPU despeckle stage now corrects both centers.

- `ordered-repair-reference.json` retains a 24×20 signed/headroom synthetic source, all repaired samples, and reversed-order output from `foveonic_bad_pixels` with no override masks. It covers all seven layer combinations, partial masks, duplicate/adjacent records, cutoff selection, asymmetric borders, genuine-zero extrema sentinels, and unknown layer flags. [Ordered repair tests](../../src/repair_test.rs) compare every output sample at gains 0/1/1.3 and regress the photographic 2×2 leaf defect with independent retained artifacts.

- `despeckle-reference.json` retains a 64×64 synthetic source, all corrected samples, ordered records, and selected metadata-threshold entries from `foveonic_detect_speckle_band`, `foveonic_bad_pixels`, and `foveonic_speckle_tables`. Zero detector adjustment exposes peaks/valleys, all seven layer combinations, adjacent mixed-layer detections, native margins, and headroom clamping. Lookup samples separately use gains `[2.3,1.1,0.8]` and adjustment `[15,15,5]`. [GPU regression checks](../../src/despeckle_test.rs) compare every output sample, positive/zero Sensor gain, immutable input, and bounded record order.

GPU arithmetic uses float32. Component checks allow small numerical differences in camera-code values; overlap projection matches the reference fixture exactly. Noise JSON includes selected interior samples; repair/despeckle fixtures retain complete small-image outputs. Preserved-border checks, full-sensor reference replay, and full-image finite-value checks complement them.
