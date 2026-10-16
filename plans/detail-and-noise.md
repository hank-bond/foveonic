---
summary: "The reference-style full-resolution noise/detail shader slice is implemented and validated, with owner photographic review pending."
---

# Build reference-style detail and noise nodes

Requirements: [`detail-and-noise.md`](../docs/specs/detail-and-noise.md), [`processing-graphs.md`](../docs/specs/processing-graphs.md), and [`pipeline-inspection.md`](../docs/specs/pipeline-inspection.md).

The owner approved autonomous implementation after selecting the reference algorithms as the first photographic baseline. The reviewed GUI work is committed as `b1a611b`.

## Steps

- [x] Inspect reference noise parameters, filtering, directional smoothing, residual reconstruction, and processing-tile geometry.
- [x] Prepare camera noise models and effective ISO, preserving attribution.
- [x] Implement tiled GPU bilateral filtering, edge weights, and recursive directional means.
- [x] Implement luminance diffusion/extrema conditioning, residual mixing, reconstruction, and signed detail.
- [x] Connect typed atlas ports and selective public image inspection.
- [x] Expose opt-in shared GUI/CLI controls.
- [x] Validate reference arithmetic, preserved borders, neutral defaults, and retained outputs.
- [x] Review six photographs technically, measure resources, and run native checks.
- [x] Finish documentation/index and regression checks for owner review.
- [ ] Owner photographic/interaction review and cross-platform runtime checks.

## Scope and implementation findings

Reference source: `~/git/foveonic`, revision `e47af35`, including the owner's permitted unstaged changes. The repository remained read-only. Adapted modules/shaders retain attribution; [fixture provenance](../crates/foveonic/tests/data/README.md) identifies the diagnostic library and arithmetic cases.

The first slice follows full-resolution tile filtering and reconstruction. [Baseline boundaries](../docs/specs/detail-and-noise.md#baseline-boundaries) record deferred stages and necessary workspace departures. Existing defaults remain unchanged, with reconstruction off. Modern alternatives remain separate future node implementations.

The graph keeps overlapping atlases internal and projects immutable sensor/RGB/scalar artifacts. Edge-only inspection skips recursive means; ratio and signed-signal inspection skip residual mixing. Signal-only execution also skips reconstruction and tone. Signed detail remains a pre-mixing signal. Luminance-ratio and signed-detail descriptions carry their own ranges, so zero/neutral samples have meaningful presentation and scopes.

Reference fixtures exposed a delayed diffusion write at the penultimate row's left border. The shader now preserves that sample while reproducing the other delayed writes. Component checks cover these borders, the zero-initialized signed-signal border, and exact tile-overlap ramps.

Inspection review found that existing pre-tone RGB stores camera-code values. Presentation and scopes now use its 0–4095 range while preserving the buffer. A signed-scope endpoint test also caught fused-operation roundoff at −1000; shifting the range before scaling keeps its endpoints exact.

## Evidence

- Reference component fixtures cover adaptive radii, bilateral filtering, edge LUTs, both recursive means, noise/fixed diffusion, extrema conditioning, ratios, residual gains, signed signal, and reconstruction. Metadata fixtures match all six originals' selected noise parameters and effective ISO; tests check uniform gain scaling.
- GPU integration checks cover stage-local camera requirements, independently retained outputs, signed and ratio scopes, all five chroma/luminance choices, zero detail, explicit ColorDQ, and zero sensor gain. All 30 full-resolution photograph/choice combinations contain finite reconstruction values; zero gain gives a black active crop.
- `.local/detail-noise-checks/neutral-native/`: six native RGB16 TIFFs byte-identical to `.local/optimization/neutral/`.
- `.local/detail-noise-checks/reference-native/`: six native reconstructed TIFFs. `.local/detail-noise-checks/review/` contains color-managed whole-image pairs and three native-pixel crop pairs per photograph, with neutral on the left and reconstruction on the right. Review shows less color speckle across sign, leaves, shadows, and fabric while preserving texture. Clipped flower colors and isolated hot pixels remain visible follow-on concerns.
- `.local/gui-noise-checks/`: 108 native captures, 18 per original. They cover inspection, detail zero, both noise endpoints, ColorDQ, gain, scopes, native zoom, and disabling reconstruction. A pixel comparison confirms distinct control results and exact viewport/navigator restoration to the committed GUI's neutral captures.
- `.local/gui-noise-final-checks/`: final 108-capture rerun confirms corrected pre-tone RGB and ratio mappings, distinct control results, and exact neutral viewport/navigator restoration. The final Blue TIFF also matches the earlier reconstructed export byte-for-byte.
- One native attempt timed out awaiting the `noise.edges` screenshot after processing completed. A fresh isolated run completed all 18 captures; the partial attempt is retained under `.local/gui-noise-interrupted-checks/`. Its cause was not reproduced.
- Final checks pass: core 30 ordinary tests and 17 ignored GPU/fixture tests; GUI 18 ordinary tests and one ignored worker test; workspace/tool tests, Clippy, formatting, documentation/index checks, and diff checks. Logs are `/tmp/foveonic-detail-final-{tests,gpu-tests,clippy,docs,build,capture-check}.log`.

## Timing and memory

Measured locally on Apple M1 Pro. All processing uses shared physical CPU/GPU memory. Peak footprint includes GPU resources and differs from resident-set reporting.

- Six native neutral exports: 2.83 s total, 1.31 GiB peak footprint.
- Six profiled native reconstruction exports: 7.64 s total; median GPU compute about 0.765 s, 2.03 GiB peak footprint. Timestamp resolution waits at stage boundaries.
- One unprofiled native reconstruction export: 1.37 s total, 2.93 GiB peak footprint.
- Warm native reconstruction/inspection requests: final median 0.814 s across 72 requests (earlier run 0.826 s); GUI peak footprint about 3.67–3.68 GiB.
- A full-sensor RGB plane is about 184 MiB. The 4×4 atlas with 64-pixel context uses about 235 MiB per RGB plane. Owned workspaces and queued GPU reads dominate the initial peak; checkpoint caching remains deferred.

## Checks

```sh
cargo test --offline --workspace
FOVEONIC_FIXTURE_DIR=/Users/work/git/foveonic/references/x3f \
  cargo test --offline --release --workspace -- --ignored --test-threads=1
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo run --offline -p docs-index
cargo run --offline -p docs-index -- --check
cargo fmt --all -- --check
git diff --check
```

Native reconstruction checks use isolated preferences:

```sh
cargo xtask bundle --offline
FOVEONIC_GUI_SMOKE_DIR=.local/gui-noise-final-checks/blue-parking-sign \
FOVEONIC_GUI_SMOKE_NOISE=1 \
  target/release/Foveonic.app/Contents/MacOS/foveonic-gui \
  /Users/work/git/foveonic/references/x3f/blue-parking-sign.x3f
```

The owner authorized grouped commits after code and automated checks. Photographic/interaction review remains pending.
