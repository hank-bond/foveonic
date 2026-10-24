---
summary: "Complete Tiger's native full-resolution rendering before consolidating and evaluating portable methods on ordinary Quattro."
status: archived
---

# Tiger integration

Implement the accepted [Tiger requirements](../../docs/specs/tiger-rendering.md), reusing the existing [Quattro composition contracts](../../docs/specs/quattro-rendering.md) and [output requirements](../../docs/specs/output-strategies.md).

- [x] Audit original metadata, native evidence, adapter limits, and complete processing dependencies.
- [x] Implement calibrated extraction and Tiger-specific repair preparation.
- [x] Implement native dynamic-range extension and matching highlight-mask handling.
- [x] Join Stage1, color/monochrome Stage2, reconstruction, finishing, and geometry.
- [x] Complete application controls, inspection, TIFF, and editable DNG.
- [x] Validate numerical fixtures, original-file boundaries, existing-family regressions, and native application behavior.
- [x] Consolidate demonstrated shared contracts and compare portable Tiger methods on ordinary Quattro without changing its defaults.
- [x] Owner approved merging and shipping Tiger. Optional ordinary-Quattro appearance changes remain deferred.

Use an isolated worktree because main contains unrelated release work. The owner authorized committing the completed Tiger work, including Fast/Smart and paired phase-detection repair. Reference research and the original sd Quattro H fixture are already supplied by the owner. Findings should be recorded here without depending on private artifact paths.

## Initial evidence

The original sd Quattro H is 6656×4480 with processing version 4.0.1.0, camera ID 41, DigitalISOGain [4,4,2], and a 6192×4128 active output. The current adapter supports the required storage buffers. Extraction uses separately rounded Float32 offset/scale followed by Double channel gain, with a 65535 ceiling. GPU fixtures include non-Float32 gains and preserve original samples exactly.

Tiger phase-detection tables use 26 knots and different site grids from ordinary sd Quattro. Expanded chroma and paired-luma gain tables match the independently checked camera equations exactly. Dark-pair correction selects the Tiger BR/3COL gain table. Ambient exposure estimation retains its non-camera-40 signed-table route.

Spatial dynamic-range extension computes 16×16 cell means, ordered forward/reverse propagation, original-cell global regressions, per-channel reconstruction and recovery masks on the GPU. Complete independent highlight fixtures cover default/fitted regressions, nonunit normalization, disabled extension, and wide images. Both masks are exact. Maximum RGB error is below 0.003 code. The native coordinate-3000 interpolation sentinel is retained for wide input.

Tiger Stage2 carries the recovery mask as an explicit graph dependency. On the retained original 160×160 native boundary, neutralization and adaptive color response are exact. Chroma protection differs by at most 0.00048828125 code. Metadata-derived Stage2 controls, precise matrix representation and every generated table match the retained original helper parameter key exactly. Reconstruction selects the separately recovered adaptive-range equations with explicit Float32 interpolation ordering.

## Validation

The retained original Stage1 crop is exact outside the last two allocation-dependent rows. Its preparation also matches the native estimated ISO, WB, bilateral strengths and nonunit-gain preservation weights [0.6,0.625,1]. Adaptive reconstruction matches independent scalar equations within one integral RGB code and three residual Float32 ULPs across four brightness ranges. GPU exponentiation accounts for the measured differences.

Complete color and dedicated monochrome TIFFs render at 6192×4128. sRGB, Adobe RGB and ProPhoto RGB exports succeed. Color and monochrome DNG tests verify appearance separation, technical-control effects, neutral mono samples, immutable retained data and engine ownership. All ten camera color modes and all WB presets render. Cached photographic edits preserve reconstruction and restore their defaults exactly. The shared decoded-source cache round-trips Tiger samples, metadata and camera ordering exactly.

Native application checks cover the embedded preview, Full photograph, organizer, scopes, zoom, sensor planes and calibration. A dedicated native inspection run also opens the recovery mask, raw color and reconstructed image. These checks are isolated from normal application preferences and sidecar writes. The new recovery preview converts integer flags into a binary Float32 sensor mask on the GPU. Its own image space retains full-grid geometry and orientation.

Pre-Tiger and current TIFF exports are byte-for-byte identical for original Classic, Merrill, DP Quattro and sd Quattro fixtures. Native Tiger evidence currently covers one original at ISO100. These bounded stage checks and functional exports do not establish a bit-perfect complete reference photograph across all Tiger settings or ISOs.

## Consolidation and ordinary Quattro experiments

The directional and channel highlight paths share one shader parameter/storage contract and precise camera-matrix arithmetic. Working-plane offsets and numerical thresholds have names. Camera-specific gains, phase sites, ceilings and noise preservation remain in Tiger preparation. Channel recovery accepts incomplete trailing color cells. Only complete active cells supply evidence, while every edge pixel has valid grid storage. Independent aligned fixtures retain their original bounds, and explicit partial-cell tests cover edge flags and immutable sources.

The ignored `compare_portable_tiger_methods_on_ordinary_quattro` test rebuilds a typed experimental graph and exports Current, adaptive reconstruction, highlight recovery and combined TIFFs plus numerical receipts. It preserves the camera's original calibration, extraction, gains and repairs. Highlight experiments use ordinary 14000-code clipping and 13000-code green limits rather than Tiger's gain-dependent ceilings. They make no application setting or default change.

Comparisons cover DP0, DP1, DP2, DP3, sd Quattro and a DP3 ISO800 original. Adaptive reconstruction has small average linear-RGB differences, approximately 0.000009–0.000237 per component. Highlight recovery changes between zero and 4.18% of components. The ISO800 lamps develop conspicuous white patches, so the paired recovery/neutralization method needs further design before adoption. Adaptive reconstruction is independently portable, subject to visual review. Single profiled GPU runs do not establish a latency improvement. The native methods are quality candidates rather than a substitute for the existing Fast/Smart latency work.

Run a comparison explicitly with local inputs:

```sh
QUATTRO_X3F=path/to/photo.x3f QUATTRO_TIGER_EXPERIMENT_DIR=path/to/new-output \
  cargo test --offline --release -p foveonic compare_portable_tiger_methods_on_ordinary_quattro -- --ignored --nocapture
```

Final checks pass: 484 workspace tests, strict Clippy, formatting and documentation indexes, ten targeted native/numerical/ownership checks, both DNG behavior checks, cache round-trips and two native application sequences. Tiger Fast/Smart is tracked in the [preview plan](./tiger-smart-preview.md). Reduced-resolution originals, other Tiger ISOs and a broader native whole-photo comparison remain follow-up coverage.

## Closure

The implemented Tiger route shipped in 0.2.0 after owner-approved integration. This completed implementation slice is archived during 0.3.0 preparation. The optional ordinary-Quattro experiments did not change defaults. Their photographic review and wider ISO/input coverage remain follow-ups under the [Tiger specification](../../docs/specs/tiger-rendering.md). Earlier worktree and uncommitted statements describe the historical implementation sequence.
