---
summary: "Find one owner-card photograph that demonstrates Compact speckle repair at native sizes 3, 5 and 7."
status: archived
---

# Find a Compact speckle test photograph

The owner approved a temporary diagnostic and read-only scan of two mounted SD cards to select a photograph they can add to the example set. Requirements: [Merrill speckles](../../docs/specs/merrill-speckles.md). The diagnostic stays in ignored local output and uses the actual processing stage at threshold multiplier 1, counting each repaired native pixel once.

- [x] Inventory both cards and measure a sample at Compact sizes 3, 5 and 7 with fixed companion settings.
- [x] Inspect promising images and matched crops for visible isolated speckles and small clusters.
- [x] Preserve a local candidate original and comparisons for owner review, with source identity, settings and crop coordinates.
- [x] Record findings and limitations. Adding the photograph to the published example set remains a separate owner action.

## Findings

The cards contain 1,050 actual X3Fs from DP1, DP2 and DP3 Merrill cameras at ISO 100 or 200. Another 401 matching filenames are macOS companion files. Capture metadata was inventoried across the cards. The diagnostic measured 196 photographs: 78 DP1, 83 DP2 and 35 DP3. The sample includes every ISO 200 photograph and a spread of ISO 100 scenes. This was sufficient to select a useful example without exhaustively ranking the cards.

The temporary Rust diagnostic uses the public rendering API and actual Compact shader. It compares the native highlight output with despeckled layers inside the active crop. Companion settings stay at their baseline with recorded white balance, Standard color mode, zero exposure adjustment, camera repair and Classic highlight restoration. No user defect profile or saved sidecar is loaded. The diagnostic checks that detections remain nested as the maximum radius increases. All 196 measurements completed without errors.

The initial candidate was `_P2M0772.X3F`, a DP2 Merrill ISO 200 photograph of blue painted woodwork. Compact changes 5,633 native pixels at size 3, 12,775 at size 5 and 23,406 at size 7. The increments are 7,142 and 10,631 pixels. Independent full-render exports reproduce these counts. The candidate original was copied into private local review output and verified against the card with SHA-256. It does not duplicate an original already in the local example cache. Cards, saved edits and the published collection remain unchanged.

Three 64×64 crops demonstrate an isolated point and nearby groups at oriented native centers `(1541,821)`, `(1567,2661)` and `(1674,2561)`. Their half-open bounds are `[1509,789,1573,853]`, `[1535,2629,1599,2693]` and `[1642,2529,1706,2593]`. The comparison sheet displays Off, 3, 5 and 7 at 6× nearest-neighbor magnification. The color-managed sRGB sheet was visually inspected alongside full-scene overviews and alternative leaf and dark-background candidates. Exact settings, native TIFFs, event coordinates, source identity and the frozen diagnostic remain in private local output.

Counts describe repair decisions, not independently confirmed sensor defects. Dense leaves and fine edges can produce much higher counts than a useful speckle demonstration. Larger supports also affect wood grain, making this photograph useful for checking restraint. Sizes 3, 5 and 7 describe neighborhood diameters rather than measured defect diameters. Publishing the selected photograph and adding a help comparison remain separate work.

Workspace tests, strict all-target Clippy, formatting, generated documentation checks and all five artwork checks pass. The diagnostic and rendering evidence stay outside tracked application source.

## Broader review with noise reduction

The owner found the first sheet useful for help popovers and requested a broader view with noise reduction applied. The initial rendering helper used `Settings::new` with `reconstruction: None`, so its optional noise reduction was off. Normal Merrill capture initialization explicitly supplies default reconstruction controls. The original detector counts and small-crop evidence remain valid, but those renders did not represent normal noise-reduced photographic review.

The follow-up renders explicitly apply Reference noise reduction with Chroma 2, Luminance 2 and Banding 2. Exposure, white balance, color mode, camera repair, highlight restoration and the Compact threshold remain fixed. Four new native 3136×4704 TIFFs preserve Off, 3, 5 and 7. A 1536×2560 area at `[800,400,2336,2960]` shows the post, crossbeam, grain and multiple remaining points. Color-managed sRGB PNGs support a private local comparison with native 100% viewing, a whole-photograph option, matched setting toggles and hold-Space comparison against Off. No additional exposure boost or tiny-crop magnification is applied.

The broad image was visually checked and the saved settings confirm noise reduction is enabled in every variant. Isolated JavaScript checks verify setting selection, hold-to-compare, whole-image selection and fit width. Native browser automation was unavailable, so the browser layout was not verified through automation.

## Rejection and persistence audit

The owner rejected the woodwork example because the highlighted groups were real tiny specular highlights. A detector change and a visually prominent point do not establish a sensor defect. The woodwork comparison is superseded as a recommendation. It remains evidence that larger Compact supports can remove real scene features. No processing algorithm was changed during this selection audit.

Three unrelated DP2 Merrill photographs of woodwork, leaves and a dark background share camera serial 90406890. Their repair events overlap at 2,246 native sensor positions. A conservative subset of 87 has the same dominant layer, a calibrated excess of at least 100 over the repair estimate, at least eightfold separation from changes in the other layers, and size-3 detection in all three scenes. All 87 also recur in `_P2M0791.X3F`, a matte blue-bin photograph from that camera.

The original unprocessed samples were then decoded at those coordinates in all four photographs. Seventy-one positions exceed all eight immediate same-layer neighbors by at least 20 native codes in every exposure. This repeated original-sample evidence supports persistent sensor outliers. It is a conservative evidence set, not a complete calibrated defect map or a guarantee about every Compact repair.

The replacement candidate is `_P2M0791.X3F`, preserved locally as `matte-blue-bin.x3f` with a SHA-256-verified copy. Its broad comparison retains Chroma 2 and Luminance 2 noise reduction, threshold 1 and the same native-size controls. Optional markers identify the 71 repeated original-sample outliers, including 22 in the 1536×2560 review area. Setting and marker checks cover the initial Off state, Compact 3, hold-Space comparison, whole-image coordinates and marker visibility. The revised comparison was opened for owner review. A convincing recurring sensor-cluster example requiring size 5 or 7 has not been established. Those settings remain available to inspect their effects without labeling their extra changes as sensor defects.
