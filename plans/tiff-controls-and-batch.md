---
summary: "Add straightforward TIFF controls, optional corrective nodes, resizing, and shared-engine batch processing."
---

# Add TIFF controls and batch processing

The owner approved the straightforward additions after the first TIFF slice. Follow [`processing-graphs.md`](../docs/specs/processing-graphs.md) and [`application-workflows.md`](../docs/specs/application-workflows.md).

## Steps

- [x] Add contrast, shadows, highlights, saturation, and global sensor gain.
- [x] Add selectable simple isolated bad-pixel repair (subsequently replaced by ordered repair).
- [x] Add basic unsharp-mask sharpening and output resizing.
- [x] Expose shared settings through single-file and batch CLI commands.
- [x] Check small shader fixtures and real photographs. Record findings and constraints for the architecture discussion.
- [x] Agree on ColorDQ gain selection with the owner and integrate the tested primitive.
- [x] Obtain owner approval to commit the processor work before starting the GUI.

## Progress and findings

At completion of this initial slice, corrective nodes were explicitly selectable and all six original-file TIFFs with neutral controls were byte-identical to the preceding slice. Camera repair and despeckling were subsequently enabled by default; disabling both preserves that baseline. GUI and DNG work continue after this slice and the owner's architecture review.

- Contrast and endpoint controls extend reference-derived tone preparation. The GUI review replaced independent RGB gains with one global sensor gain after calibration and before repair/color conversion. The CLI now uses `--sensor-gain`; the old channel-gain option is removed. Saturation uses reference-style brightness weights after tone. Gain and saturation nodes reuse matrix arithmetic.
- The initial repair used camera coordinates, channel flags, and neighbor masks selected by explicit minimum badness, averaging immutable neighbors and skipping adjacent same-layer records. The [ordered-repair fix](./ordered-repair-and-node-audit.md) replaces it with reference sequential masked/chroma-anchored arithmetic.
- This slice originally used two separable Gaussian passes and a post-tone unsharp blend. The [remaining-algorithm pass](./complete-reference-algorithms.md) replaces it with native brightness-proportional high-pass sharpening before tone; Gaussian radius/blur stages are retired.
- Bilinear resizing shares the encoding pass, sampling pixel centers inside the oriented active crop. `--size` specifies exact dimensions and can change aspect ratio. Dedicated minification filtering remains deferred.
- `batch OUTPUT_DIR INPUT_OR_DIR...` shares `Settings` and one engine with single-file rendering. Directory scans are nonrecursive. Name collisions and existing outputs are rejected before conversion; conversion stops at the first failure.
- The owner approved explicit ColorDQ gain for this slice, as recorded in [`output-strategies.md`](../docs/specs/output-strategies.md). `--color-dq-gain` multiplies the camera's `ColorDQCamRGB` amplitudes; zero omits the node. Conditioning runs after camera color conversion and before tone; the global sensor gain precedes this RGB-space processing. Reference automatic gain uses ISO scaling capped by noise-derived parameters and remains deferred. The lookup test now exercises the production kernel directly. An adjusted native-size batch with gain 1 succeeds on all six originals; an explicit gain-0 export remains byte-identical to the baseline. Review files are in `.local/tiff-controls/color-dq/`.
- Five small GPU fixtures pass, including both transfer routes, non-square crops, all quarter-turn orientations, resizing, isolated repairs, blur/blend arithmetic, and ColorDQ lookup behavior. Workspace tests and all-target Clippy pass.
- All six originals complete adjusted batches at native size and at explicit 1200x800 dimensions. TIFFs and aspect-preserving JPEG review previews are in `.local/tiff-controls/`. The adjusted look is a test configuration. Known clipped-highlight false colors and shadow noise remain.
- On the M1 Pro, one native-size adjusted six-file batch took 6.54 seconds, with about 919 MiB peak RSS and 2.87 GiB reported peak memory footprint. A resized batch took 6.36 seconds and 2.48 GiB peak footprint. With ColorDQ gain 1 added, the same native-size batch took 6.63 seconds and 3.05 GiB peak footprint. These single runs include decoding and packaging. Extra full-resolution intermediates increase memory; pooling and preview-resolution execution remain architecture discussion items.

## Checks

```sh
cargo test --offline --workspace
cargo test --offline --release -p foveonic shader_ -- --ignored --test-threads=1
cargo clippy --offline -p foveonic --all-targets -- -D warnings
cargo run --offline -p docs-index -- --check
cargo fmt --all -- --check
```
