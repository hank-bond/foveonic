---
summary: "Deliver Merrill Smart detail from native processing tiles before continuing the whole frame."
status: archived
---

# Merrill regional preview

Implemented the owner's approved viewport-first refinement in [Smart preview](../../docs/specs/smart-preview.md) and [Merrill preview](../../docs/specs/merrill-fast-preview.md), preserving native tile boundaries, context, orientation and requested Full controls.

- [x] Separate full-frame prerequisites from selected native tiles and retain them for the immediate Full continuation.
- [x] Restrict noise, detail, color and finishing to tiles covering the viewport and its required support.
- [x] Enable Merrill's existing GUI detail-first sequence and verify panning, edits and stale results.
- [x] Compare completed detail-first and direct Full timings, including total continuation cost.
- [x] Check regional pixels against Full at tile seams, sensor edges and rotations, including noise disabled and reduced global filtering.
- [x] Update specifications, indexes and run repository checks.

## Investigation

The shared Smart controller restarts Fast on edits for both families. Merrill previously followed Fast with whole-frame Full. Alternating Fast and Full on a Merrill fixture repeated row correction and all noise/detail stages on every Full request, taking about 590–619 ms on an M1 Pro. Consecutive Full sharpening edits settled at 61–64 ms. Tone alone measured about 4 ms GPU time. These are local observations, not a latency guarantee.

Native noise reconstruction uses a four-by-four processing grid with 64-pixel context. Directional means, column conditioning and deterministic pattern permutations depend on complete native tiles. Keep those tiles intact and process only the selected subset. Some noise settings first filter a reduced whole image. That context must remain whole-frame and be retained independently of tile extraction. Calibration, repairs, highlight recovery, despeckling and row correction initially remain native full-frame prerequisites. Evaluate further reduction against the actual spatial dependencies and measurements rather than changing their sampling unconditionally.

## Processing boundaries

| Operations | Working area and sampling | Reason |
| --- | --- | --- |
| Decode, black estimates and spatial calibration | Native whole sensor | Calibration and metadata use native positions. These already feed the reusable Fast boundary. |
| Camera defects, highlight restoration and despeckling | Native whole sensor | Sparse records, ordered corrections and whole-image donor searches require original coordinates. Downsampling before them would change their meaning. |
| Row correction | Native whole sensor, retained after correction | Surface fitting and rejection use broad image statistics. Applying the result is cheap compared with estimating it. A reduced estimator would change the algorithm. |
| Reduced global noise base | Whole image at the metadata-selected 0.375 or 0.5 linear scale, retained separately | Global directional means cannot be replaced by viewport-local filtering. The 0.5 route already filters one quarter of the pixels. |
| Local noise base, column conditioning, ratios and pattern correction | Selected complete native tiles, with existing context and any native reduction | Directional recurrences, column blocks and ordered permutations depend on tile phase and complete lines. Arbitrary smaller rectangles would change results. |
| Residual mixing, camera color and ColorDQ | Selected tiles | Pointwise operations retain the same arithmetic. |
| Projection, sharpening, tone and saturation | Requested physical region plus four finishing support pixels | Native seam contributors and sharpening neighborhoods remain available. |
| Fast overview | Existing filtered half-width/height grid | Camera color, sharpening and tone already operate on one quarter of the pixels. Noise/detail and row correction remain omitted as agreed. |

No new reduction of native sensor corrections is selected. The regional route first reduces the amount of identical work rather than changing camera interpretation. Explicit output resizing currently refines whole-frame because its sampling phases need a separate regional contract.

The recipe retains the shared pre-row boundary through Full so later edits can produce Fast without repeating repairs and highlights. Regional requests also retain corrected native layers and any reduced global context for immediate Full continuation. All admission stays within the existing one-GiB session cache limit.

## General optimization

Projection now visits only the neighboring native tile cores that can contribute to the existing eight-pixel seam. It preserves reverse native tile order and zero-weight endpoints. A separate GPU diagnostic measured whole-frame projection at about 4.3 ms versus 13.3 ms before this change on an M1 Pro. Native overlap fixtures and regional equality checks cover the same arithmetic.

## Validation and cost

Five matched-session trials on an M1 Pro, without GPU profiling synchronization or source decoding, measured median direct Full at about 560 ms. Detail appeared at about 212 ms and its Full continuation took about 512 ms. Median combined detail/full latency was about 720 ms. The regional route spends more total work to reduce the wait for the clicked area. A second photograph with a different orientation also completed the regional and Full routes.

GPU tests compare requested pixels exactly against Full across seams, corners, four rotations, sharpening and saturation adjustments, noise disabled, and the reduced global filtering route. Continuation matches a fresh Full, preserves the earlier detail image, and reuses native corrections and global context. Numerical reconstruction fixtures passed. Native Merrill and Quattro Smart checks covered detail-first delivery, Full continuation, panning without another render, edits, fitting and cache reopen. Workspace tests and strict Clippy passed.

## Production review

The whole-image context exposes required layer and edge buffers explicitly. Both direct and separated reconstruction paths validate the same reduced geometry. Invalid regional grids and rectangles are rejected before tile arithmetic. Named seam constants document the asymmetric blend support shared by tile selection and projection.

The Merrill change was validated independently of pending cache and preview-color changes. Workspace tests, strict Clippy, formatting, documentation indexes and artwork checks passed. Six GPU reconstruction tests and two photographic preview tests passed, followed by isolated native Merrill and Quattro Smart runs with twelve captured states each.
