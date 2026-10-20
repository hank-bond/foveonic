---
summary: "Reproduce saved-sidecar highlight comparisons, completed GPU timings, and a private toggleable browser report."
kind: how-to
---

# Evaluate color-field highlights

The [Color field requirements](./specs/color-field.md) and [plan](../plans/color-field.md) distinguish faithful reconstruction from deliberate algorithm variants. Keep private photographs, rendered assets, manifests and reports in ignored local directories.

## Render matched comparisons

Snapshot originals and editing sidecars into a private corpus directory before running comparisons. The tool reads sidecars but never writes them. Require a new output directory for each measurement round:

```sh
cargo build --offline --release -p foveonic --example compare_color_field
target/release/examples/compare_color_field CORPUS .local/color-field-review --warm 3 --gpu
```

Each recipe executes fresh full processing from a decoded source. Its completed render measurement includes a GPU completion wait. The first recipe sample is separate from warm samples. A shared initialized engine means these first samples are not new-process measurements. GPU timestamps come from a separate synchronized diagnostic iteration. Source decoding, export and preview conversion are outside render intervals.

`--scenes _P2M0095,_P2M0286,_P2M0313` selects priority originals. `--variants color-field,anchors` selects named variants. Default variants include saved-threshold Classic, Auto Classic, faithful full-raster Color field, an active-area work-domain trial, anchors only, uniform support, 4px/16px color grids, relaxed solve bounds, 64-sweep local gradient reconstruction, stricter agreement/boundaries, half donation and no donated color. Two supplemental regression frames without sidecars use an explicitly identified Sunlight/Standard, −1.5 EV recipe. Use `--no-export` for timing and repeated-convergence checks without allocating photographic outputs. This records renders and failures but cannot produce an image report. The ordinary CLI exposes `--highlight-mode classic|color-field-local|color-field-global`. Use `--color-field-selectivity 0` (the default) for the original behavior. Manual strength spans 0–1, with higher values rejecting more inconsistent local evidence. The GUI and CLI use the same strength units. The separate approach, area, donation, agreement and boundary flags remain investigation overrides, not ordinary GUI controls.

## Build and review the report

Install Pillow in an isolated Python environment, then build the report:

```sh
python3 -m venv .venv
.venv/bin/pip install pillow
.venv/bin/python scripts/color_field_report.py .local/color-field-review --corpus CORPUS
open .local/color-field-review/index.html
```

Space or an image click toggles A/B. Keys 1–9 select a candidate, and arrow keys change scenes. Full views and named PR-region crops share stable bounds. [The region manifest](../scripts/color_field_pr_regions.json) records 15 screenshot-registered detail regions across 11 reference-implementation example scenes, including both 0313 faces/skin and swaddle views. These are estimated, visually checked bounds in oriented native TIFF coordinates, not unpublished original coordinates or pixel goldens. The report links each region's PR screenshot and offers Fit / 100% display. Scenes without a PR detail shot offer full frame only. Native RGB16 TIFFs remain linked when retained. To limit storage during broad sweeps, pass `--previews "$PWD/.venv/bin/python"` to the renderer. The four supported comparison recipes then keep full-native-resolution, native-profile 8-bit WebP plus matched sRGB report views instead of native TIFFs. Only explicitly named tool-owned staging TIFFs are consumed, after preview decoding/profile checks and a provenance record. Prior retained TIFFs are never removed. Omit the flag to retain every RGB16 TIFF. Previews convert the TIFF's embedded profile into sRGB before lossless WebP encoding. Pillow's RGB preview conversion is eight-bit, so use the float tests and native TIFFs for numerical conclusions.

The report can merge separate measurement rounds with `--supplement DIRECTORY`; duplicate recipes are rejected. For the original active-area prototype rounds, use `--baseline DIRECTORY` and `--roi-supplement DIRECTORY`. These explicitly label ROI results, retain the recorded settings and binary hashes, and disclose earlier stopping bounds rather than silently treating them as current full-raster results. Raw JSONL files remain unchanged. Compare current full-raster/active-area results to isolate work domain, and compare the initial ROI baseline against its ROI variants to isolate algorithm changes.

Rebuilding a report recrops retained TIFFs or lossless native-profile previews. It does not rerun recovery or change measurements, recipes, original staged metadata, or previously generated crops. Crop filenames include the region and full rectangle so a new extent cannot silently reuse an old cached image.

The page separates first, warm and synchronized diagnostic results. It includes exact settings, source/sidecar hashes when available, crop coordinates and render failures. Allocation totals are not peak GPU residency. Validate physical memory separately from timed renders. Control overlapping GPU workloads and retain raw JSONL evidence.

Numerical checks include eight synthetic cases, an independent 64-sweep Local recurrence, matched crops, and full 4928×3264 rasters from all 16 corpus originals. Reliability quantization is checked independently against CPU sensor-code calculations, with at most one u8-bin difference at float32 rounding boundaries. Both equation solvers then receive the same quantized evidence; this avoids confusing a bin-boundary change with an equation or solver error. Run them separately from timed render comparisons:

```sh
FOVEONIC_HIGHLIGHT_DIR=CORPUS cargo test --offline --release -p foveonic gpu_color_field_ -- --ignored --test-threads=1 --nocapture
```

The normal solver uses maximum-residual bounds of `5e-7` for brightness and `2e-6` for color. Submission checkpoints preserve CG search directions unless recursive convergence fails true-residual validation. Color iterates retain rounding remainders in two otherwise unused vector lanes. Native and color unknowns are compacted in ascending physical order using integer GPU ranks and prefixes. This fixes repeated CG reduction order without changing the equations or numerical bounds, and prevents small recovery-rounding variation from crossing a downstream speckle-detection boundary. The matrix product uses explicit fused operations to retain those low components in the ratio-difference term. Reassociating them into ordinary additions failed the ill-conditioned GPU test. Compensation itself adds no buffers. The subsequent speed pass compacts fixed color rows and packs work vectors and scalar equations without changing the equations or acceptance bounds. The optional consistency cutoff now carries each cell's original neutral coefficient explicitly in an 80-byte color row. Scalar brightness rows remain 48 bytes. This prevents rejecting evidence from automatically increasing neutral bias. Strength 0 disables the extra variance work and retains the original equations. Production has no relaxed-solver mode. The full-raster CPU comparison keeps a `2e-4` normalized maximum-error limit with identical quantized evidence; this is a numerical acceptance bound, not a photographic quality score. The [speed pass](../plans/color-field-speed.md) records subsequent implementation optimizations and controlled comparisons. Standard GUI worker/GPU tests and a retained-image/cache/DNG test cover integration. The separate [unresolved stock-cache mismatch](../plans/gpu-output-mismatch.md) must not be hidden by these passing checks.

## Review local highlight selectivity

The [selectivity investigation](../plans/color-field-selectivity.md) distinguishes local evidence rejection from uniform neutral bias. The GUI puts Method first, then one Highlights slider and the enable checkbox for Classic, Local and Global. Classic retains A and its signed camera-relative threshold, relative to a baseline calculated from camera exposure metadata. Color Field has no A button. Its scalar strength defaults and resets to 0, preserving the original reconstruction. Increasing toward 1 rejects more inconsistent evidence. The checkbox alone disables highlight reconstruction.

Positive strength maps to an internal cutoff of `0.005 / strength`. Strength 0.25 reproduces cutoff 0.02, strength 0.5 reproduces 0.01, and strength 1 reproduces 0.005. As strength approaches zero the cutoff becomes increasingly permissive, without a jump into strict filtering. The cutoff measures reliable log-ratio scatter inside each grid cell. It tapers inconsistent observations out of affected grid color equations. Original graph guides and each cell's original neutral coefficient remain unchanged. Brightness equations, healthy samples and Local's 64 sweeps are unchanged. Rejecting real color variation can still lose useful flower, fabric or skin color. Judge the cutoff per photograph, not as one setting for the entire corpus.

Exploratory grid, area, solver and evidence-weighting CLI options are removed, along with neutral-restraint and raw-cutoff options. The comparison renderer offers only Classic with saved or automatic threshold and Color Field: Local or Global. Saved raw cutoffs migrate to equivalent selection strengths. Retired automatic and neutral-restraint prototype settings migrate to 0. Explicit strength settings, including a migrated null or 0, take precedence over retired fields. Loading does not rewrite the file or preserve a retired processing path.

The maintained sensitivity tool checks strengths including repeated/restored zero, fresh versus edit-session outputs and retained-image immutability:

```sh
cargo build --offline --release -p foveonic --example measure_color_field_controls
target/release/examples/measure_color_field_controls CORPUS .local/highlight-cutoff-review "$PWD/.venv/bin/python" _P2M0170,_P2M0095,_P2M0286,_P2M0313
```

Its output directory must be new. It reads sidecars without writing them. Matched color-managed previews show strengths 0, 0.25, 0.5 and 1 for both methods. Numerical records compare calibrated highlight layers, final floating Output and native-profile RGB16. Repeated/restored-zero samples omit previews. Full-resolution pixel counts are diagnostic, not proof of a useful photographic adjustment. The [unified-control plan](../plans/unified-highlight-control.md) records validation and the failed strict cached Output comparison. Do not use repeated runs of the tool to turn that failure into an apparent pass.

## Solver validation and remaining review

A passing oracle run does not establish repeated-render robustness. The retained photographic sweep recorded three color-convergence failures on supplemental 0399. Removing forced restarts alone passed oracle checks but still failed repeated rendering. Compensated color iterates and the fused ratio product then passed all eight GPU tests, the 16-frame oracle, and a predetermined 63-render 0399 check covering Global, Local, and 16px-grid reconstruction. Residual and `2e-4` image-error bounds remain unchanged. Keep the earlier failed trials visible and do not relabel their images or timings as corrected-binary results. The [plan](../plans/color-field.md) tracks validation and remaining review, including the separate strict stock-cache mismatch.

The pinned reference supplies recovery equations, not the complete output appearance. Foveonic retains its calibration, repairs, row correction, tone and export processing. End-to-end photographs need owner review even after identical-input equations pass.
