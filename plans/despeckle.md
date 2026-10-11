---
summary: "GPU speckle detection and ordered correction fix the two reported red defects; separate repair/despeckle controls default on, and the owner's Dock icon is integrated."
---

# Add reference despeckling

The owner approved the stage after the [two residual-defect diagnosis](./editor-review-focus.md#residual-defect-diagnosis), then selected separate controls with **both camera-record repair and despeckling enabled by default**. Requirements: [dynamic despeckling](../docs/specs/despeckle.md), [processing graphs](../docs/specs/processing-graphs.md), and [GUI artwork](../docs/specs/gui-workspace.md).

## Steps

- [x] Settle enabling behavior and record requirements.
- [x] Prepare reference float32 lookup tables from WB/metadata and native active margins, retaining attribution.
- [x] Implement immutable-input detection on GPU, deterministic bank/row ordering and bounded records, then ordered chroma-anchored correction on GPU.
- [x] Add typed graph operation, selective inspection, retained settings, and shared GUI/CLI controls/defaults.
- [x] Test peaks/valleys, headroom clamping, margins, all layer combinations, sequential neighbors, independent toggles, and zero/positive Sensor gain.
- [x] Check both known defects and all six originals, unchanged disabled output, photographic/native captures, timing, and memory.
- [x] Import and frame the owner's SVG, generate transparent runtime PNG, and set window/macOS Dock icons.
- [x] Update indexes and run workspace/ignored GPU tests, Clippy, formatting, and documentation checks.
- [x] Owner authorized grouped commits after code and automated checks.
- [ ] Owner photographic review and cross-platform runtime checks.

## Implementation

Four kernels perform 256-pixel block detection/ranking, bank prefix sums, deterministic record scatter, and correction. Eight banks each cap at 624999 row-ordered records. A single correction invocation preserves native read-after-write behavior for adjacent detections without GPU races or host pixel readback. Every output pixel is initialized from the immutable input before correction; retained preceding artifacts are unchanged.

Reference `repair::bad_pixels` trims neighbor minima/maxima and uses unaffected channels as chroma anchors. Its all-zero min/max sentinel behavior and record order are preserved. This shader is independent of the camera-record node, which the subsequent [ordered-repair fix](./ordered-repair-and-node-audit.md) migrated to the same native trimmed/chroma-anchored rules.

The independent stage uses unscaled selected WB factors and the native diagnostic's 1× threshold adjustment; it is not driven by reconstruction presets. Production noise-recipe coupling remains deferred. The [node audit](./ordered-repair-and-node-audit.md#despeckle-wb-scale-and-recipe-coupling) quantifies the additional sensitivity caused by using raw diagnostic WB thresholds on calibration's 2× WB signal. Positive Sensor gain is divided out during detection; correction acts on the scaled samples. Zero gain skips detection and stays black. Supported Merrill full-keep geometry aligns eight bands to 256-pixel blocks; unsupported geometry is rejected.

`Settings::repair` now starts at `Some(Camera)` and `Settings::despeckle` at `true`. GUI checkboxes remain independent and session-only. CLI adds `--no-repair`, `--despeckle`, and `--no-despeckle`; `--repair-badness` still selects a manual cutoff. `despeckle.layers` is a selectively retained, cropped/oriented three-layer artifact. Earlier roots finish before preparation of either later stage.

## Artwork

The owner supplied `/Users/work/Downloads/logo.svg`, preserved in [`crates/foveonic-gui/assets/`](../crates/foveonic-gui/assets/AGENTS.md) alongside the editable revision. The initial static Dock override was subsequently replaced by a local macOS app bundle with native automatic light/dark appearances. Non-macOS window icons use the compiled default PNG. The [icon plan](./icon-background.md) records the selected workflow, composition, source preservation, and checks. Nothing was installed or published.

## Evidence

- `despeckle-reference.json` records 64×64 native detection/ordered repair and threshold samples. The GPU test compares every sample at gain 0/1/1.3, immutable source preservation, and capacity-one bank ordering. See [fixture provenance](../crates/foveonic/tests/data/README.md).
- Known-source checks correct the bolt `(220,705)` and white-edge `(3624,2264)` red-layer defects, preserving their green/blue values and retained preceding artifacts.
- `.local/gui-despeckle-checks/`: 66 native frames across six originals, including default rendering, low-preset camera-only/repaired zoom comparisons, corrected-layer inspection, both disabled, and restored defaults. Both reported magenta centers disappear with despeckling; other isolated colored points can remain.
- `.local/despeckle-checks/default-native/` and `reconstruction-native/`: six TIFFs each with the new enabled corrections. `disabled-native/` matches all six previous neutral TIFFs byte-for-byte.
- `.local/despeckle-checks/default-native.log`: profiled six-image batch, 5.49 s / 1.67 GiB peak footprint. Despeckle GPU totals span about 49–323 ms, with sequential repair the main image-dependent cost. These are local observations, not a responsiveness target.
- `/tmp/foveonic-despeckle-{workspace-tests,gpu-tests,clippy,build}.log`: checks. Metadata/test fixtures use the existing read-only reference library audited at `e47af35`, including owner-permitted unstaged changes; source files were not modified.

Highlight restoration and fuller native camera-record repairs remain separate work. General caching and performance parallelization should preserve ordered correction if pursued.
