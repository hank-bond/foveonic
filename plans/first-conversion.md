---
summary: "Establish the shortest Merrill X3F conversion path through the shader graph to DNG and TIFF."
read_when: "Read when beginning or resuming the first end-to-end conversion."
---

# Establish end-to-end conversion

Requirements: [`processing-graphs.md`](../docs/specs/processing-graphs.md) and [`output-strategies.md`](../docs/specs/output-strategies.md).

## Steps

- [x] Inspect reference decoding, calibration, color, and output paths.
- [x] Import Merrill decoding and typed metadata, with an inspection CLI.
- [x] Agree on a TIFF-first shader milestone with the owner.
- [x] Connect source data and metadata access to a typed internal graph.
- [x] Implement shader image stages and export an inspectable RGB16 TIFF.
- [x] Check six original-file exports and small GPU arithmetic fixtures. Record photographic gaps and memory use.
- [ ] Obtain owner review of the TIFF slice.
- [ ] Resolve DNG sample encoding, compression, calibration, and profile fitting with the owner. Add the DNG recipe and packaging node.
- [ ] Review DNG editing in Lightroom and Capture One.

## Progress and findings

Detail reconstruction, denoising, and highlight reconstruction can be omitted from this first slice.

- Imported the reference reader and metadata types into `crates/foveonic/`, keeping camera matrix encodings and property-list order. Omitted diagnostic C interfaces and processing-specific metadata helpers.
- `foveonic inspect` emits decoded metadata. Thirteen unit tests pass, and the optional original-file test decodes all six original X3F fixtures, checking 48,254,976 samples per file.
- One release inspection of `blue-parking-sign.x3f` took 2.94 seconds including JSON output, with about 276 MiB peak resident memory. This is an initial measurement, not a performance target.
- The reference calibration path combines black subtraction, spatial gains, and WB-dependent factors. Camera color matrices are also WB-dependent. DNG needs a deliberate separation of sample calibration and editable WB.
- Reference color-mode tone selection depends on camera property-list order. The reader retains that order.

## TIFF slice

- The owner approved TIFF first using `wgpu` and WGSL, followed by editable DNG.
- The graph has source, black estimation, calibration, camera color, tone, encoding, and TIFF packaging nodes. Connections distinguish sensor layers, black levels, calibrated layers, linear RGB, tone-mapped RGB, and encoded RGB16, including sRGB versus Adobe RGB.
- Image operations run in shaders. Host code resolves camera tables and curves, then packages read-back samples. Buffers are released after their final graph consumer.
- WB and mode are explicit CLI settings. Exposure defaults to zero stops and is applied before tone. TIFF uses the stored camera color route and its matching ICC profile. Crop and orientation are applied physically. Existing output files are rejected.
- The baseline omits repairs, ColorDQ conditioning, detail, denoising, row correction, and highlight reconstruction. Spatial-grid application uses direct bilinear interpolation. Tone uses floating curve samples with negative RGB clipped at the rendering stage.
- All six local DP2 originals render. Small GPU fixtures check trimmed black estimation, signed calibration, and all four quarter-turn orientations. TIFF readback checks RGB16 samples and embedded profiles. An Auto/Vivid/+0.5-stop export exercises alternate settings.
- Initial review of exported TIFFs and JPEG previews finds coherent ordinary colors and composition. The red-flower scene has conspicuous yellow clipped-highlight patches, and dark shadows retain noise. Quality remains for owner review.
- M1 Pro CLI exports took 0.77-1.58 seconds including decoding, GPU initialization, processing, readback, and file writing. Peak RSS was about 786-796 MiB. Reported peak memory footprint was about 1.59-1.60 GiB, including GPU allocations. These are single-run observations.
- DNG and GUI remain pending. TIFF controls and batch processing continue in [`tiff-controls-and-batch.md`](./tiff-controls-and-batch.md). No optional corrective stages or generated look have been made the application's default recipe.

## Checks

```sh
cargo test --offline --workspace
cargo test --offline --release -p foveonic shader_black_calibration_and_orientation -- --ignored
FOVEONIC_FIXTURE_DIR=path/to/x3f cargo test --offline --release -p foveonic decode_original_merrill_files -- --ignored
cargo clippy --offline -p foveonic -- -D warnings
cargo run --offline -p docs-index -- --check
```
