---
summary: "Opt-in chroma diagnostics inspect matched native-layer samples, normalization and ColorDQ without changing app processing."
---

# Chroma investigation

This test-only module belongs to the interactive module so it can inspect the existing cache boundary without exposing internal buffers publicly. Use tracked GPU range copies to gather bounded numerical sample tables. Small-table arithmetic is diagnostic only. The harness does not select application defaults.

Run serially with an explicitly supplied original and a new output directory:

```sh
FOVEONIC_CHROMA_SOURCE=/absolute/path/to/photo.X3F FOVEONIC_CHROMA_OUTPUT=/absolute/path/to/new-report cargo test --offline --release -p foveonic chroma_investigation::inspect_green_outliers -- --ignored --test-threads=1 --nocapture
```

The current diagnostic follows the reported portrait's oriented top-shadow crop. It fixes recorded white balance, Standard mode, Multiscale Rows + Columns, ordinary Directional Fine and neutral texture. It compares Chroma 2, 3 and 4 at Luma zero and two. Shadow neutralization remains off. These are controlled settings rather than a reconstruction of unsaved screenshot edits. Source sidecars remain untouched.

The output records conditioned and filtered layer values, luminance denominators, brightness restoration, near-black normalization support and RGB before and after ColorDQ. Each Chroma sweep verifies identical upstream native samples and filtering ratios. Read image appearance and uncertainty separately from negative RGB values after color conversion.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[controls.rs](./controls.rs)** Checks local color guards against known bad references and supported weak colored details.
- **[gpu.rs](./gpu.rs)** Runs test-only local-reference shaders while preserving tracked GPU buffer ownership.
- **[mod.rs](./mod.rs)** Inspects small GPU sample tables around green outliers without changing photographic processing.
- **[neutralization.rs](./neutralization.rs)** Measures actual shadow-neutralization inputs, regional guides and output at reported green outliers.
- **[trials.rs](./trials.rs)** Compares local weak-color candidates through the unchanged photographic continuation and matched display views.

<!-- INDEX:END -->

## Local-reference trials

The owner authorized local correction research and subsequently approved the conservative neutral-grain finalist. Production fuses it into the ordinary texture shader. Experimental candidates remain test-only. `local.wgsl` uses the frozen pre-adoption `baseline_texture.wgsl` and current shared noise helpers, so approved-versus-baseline comparisons remain reproducible after adoption. The harness does not modify sidecars or select production processing.

Use `FOVEONIC_CHROMA_TRIALS=/absolute/path/to/config.json` with the `chroma_investigation::trials::compare_local_color` ignored test. The JSON supplies a new `output` directory and a `photos` array. Photo records contain an `id`, `description`, `path`, oriented half-open `crop`, optional `measure`, `exposure` and `luma`. Each comparison retains the same conditioned layers, filtered reference and luminance ratio, and replaces only texture output before normal photographic continuation. Chroma 2 and 4 controls represent the pre-adoption baseline. A repeated baseline replay is checked when Chroma 4 is present. Approved threshold combinations also compare the fused production texture against the standalone candidate across the full atlas. Chroma 4 photographic samples compare against normal production rendering.

The local pool trials use weak reference signal, substantial brightness restoration and optional input/neighbor disagreement. The noise scale is an unfiltered-model proxy, not calibrated uncertainty in the filtered reference. The neutral-grain trial retains the reference color at its own brightness and makes only extra restored brightness neutral where the reference is weak and disagrees with the input. Layer-weighted brightness and true linear RGB luminance are measured separately. The latter does not guarantee unchanged displayed brightness after ColorDQ, clipping and camera tone.

Use `FOVEONIC_CHROMA_CONTROLS=/absolute/path/to/new-controls.json` with `chroma_investigation::controls::local_reference_known_color_controls` for known inputs at the texture boundary. It checks an unsupported green estimate, supported dim green/blue points, a colored edge, brightness grain and bright color. These are mechanism controls rather than full sensor simulations.

Reports put photographic whole-image toggles first, with Native, +2 EV and +4 EV display views from float results. Whole-photo views use bounded nearest sampling for context. Candidate timings include allocation and a separate full-atlas output/mask pass. They are not an integration benchmark. Keep synthetic and photographic preservation claims distinct.

Photo records may also supply `probes`, an array of oriented pixel coordinates. These record exact masks, corrected layers, RGB before ColorDQ and final linear sRGB. Crop percentiles use a two-pixel grid and can miss individual outliers. The optional top-level `assessment` contains `paragraphs` and `links` for a reviewed report. Linked evidence files must be copied alongside the report.

Run `chroma_investigation::neutralization::inspect_shadow_protection` with `FOVEONIC_CHROMA_SOURCE` and a new `FOVEONIC_SHADOW_DIAGNOSIS` JSON path to inspect production shadow neutralization at the reported points. It compares full strength at default selectivity, wider color tolerance, wider hue and both widened. The transient regional guide is recreated with the unchanged production kernel. Small-table analysis can separate darkness, relative chroma and hue gates from the actual before/after output.

## Parameter sweeps

The trial configuration accepts a `candidates` array. Each record supplies `id`, `label`, `chroma`, `mode` and `radius`, with optional `limits` and `reliability`. For mode five, `limits` contains brightness-gain start/end and weak-reference signal-to-noise start/end. `reliability` contains input-disagreement start/end and maximum correction, with its last component reserved. The original values are `[2,6,1,4]` and `[0.04,0.14,1,0]`. These tune existing research gates rather than the upstream denoiser.

Set `measure_only` to true and choose `sample_step` for broad numerical screening without JPEG generation or photographic continuation. Metrics then describe texture-stage RGB before ColorDQ in code units. Compare a selected shortlist through normal continuation before judging appearance. Single-pass screening times are not latency benchmarks.

For an approved Chroma 4 candidate, `benchmark_fused` compares the frozen baseline and fused production texture with three warmups and twelve alternating completed rounds. Both include output and uniform allocation. Upstream noise, metadata preparation and photographic continuation are excluded. The result reports raw samples and records uncontrolled GPU contention.

Set `FOVEONIC_CHROMA_CONTROL_GRID` to the same configuration while running the known-input control test to evaluate its candidates. That adds noisy green/blue points and a noisy colored edge whose filtered color is deliberately correct while the input disagrees. Such fixtures reveal the ambiguity of the available evidence. They do not estimate how often it occurs in photographs. Color-retention measurements supplement the exact-preservation assertions on supported noiseless structures.

Full-atlas counts and mask sums include processing halos. The separately recorded peak layer correction is restricted to active sensor pixels in tile cores, which are projected into the image. Use it to choose a common photographic review crop for all candidates rather than treating it as the maximum final displayed error. Verify selected coordinates with exact probes. A shortlist configuration may embed a `sweep` table with `photos` and parameter `rows`, each containing `candidate`, per-photo screening `photos` and known-input `controls`. The report exposes filters for the four parameter groups. `default_candidate` selects its initial B comparison.
