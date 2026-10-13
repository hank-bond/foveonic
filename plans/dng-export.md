---
summary: "Implement uint16 lossless-JPEG DNG export with baked technical corrections and DNG-local appearance composition, using the working x3fuse representation as a reference."
---

# DNG export

Requirements: [output strategies](../docs/specs/output-strategies.md). Finish the [sidecar checkpoint](./editing-sidecars.md) before implementing the export slice.

## Steps

- [x] Select uint16 with lossless JPEG, compressed by default; defer JPEG XL and float16 evaluation.
- [x] Agree a modified export recipe that reruns processing, bakes content corrections, and replaces pixel-tone rendering with embedded presentation.
- [x] Inspect the Rust writer and CAMF profile prototype in [x3fuse-core PR #7](https://github.com/hank-bond/x3fuse-core/pull/7), merged into local `origin/dev` at `2b0dd44`.
- [x] Check SDK normalization, white-balance editing, and highlight clipping with synthetic DNGs.
- [x] Check invertibility of the actual WB/mode matrices in six original DP2 Merrill files.
- [x] Measure the candidate export basis on GPU-processed photographs: negative samples, highlight range, and uint16 quantization error.
- [x] Agree initial negative clipping and common linear rescaling; defer signed preservation and WB-headroom refinements.
- [x] Agree WB-dependent technical corrections in samples, deferred downstream appearance, and bounded DNG-local composition.
- [ ] Finalize the export sample basis and its metadata conventions before production integration.
- [ ] Implement the smallest compressed export using this repository's processing and ownership model.
- [ ] Validate photographic appearance and editing latitude through the SDK; test Capture One and Lightroom/ACR separately.

## Photographic direction

The owner prioritizes recovered highlight detail for an ETTR-oriented Merrill workflow. Prefer linear headroom storage over a baked highlight shoulder. Allocate the range the processed image needs rather than always reserving four stops.

The owner approved the simple initial policy: clamp negative export samples to zero, rescale all channels by a common linear factor, and compensate exposure in metadata. Publish BlackLevel 0 and WhiteLevel 65535. Do not add a sigmoid or black offset for this slice. Signed preservation and more advanced WB-headroom allocation can be refined and compared later. This approval settles the negative-clipping tradeoff, not the complete sample/profile mapping or a fixed upper headroom cap.

Technical corrections may be baked using the selected WB; changing WB downstream need not rerun layer-level calibration or cleanup. ColorDQ may be baked; camera mode, tone, and downstream RGB presentation belong in embedded metadata. The current mode matrix precedes ColorDQ, reconstruction, and sharpening. A modified recipe must define this separation explicitly, including its exposure, saturation, and white-balance conventions. Removing a matrix cannot undo nonlinear processing performed under it.

The owner confirmed that their x3fuse branch's DNG representation works in Capture One. Use it as a reference for encoding and appearance conventions, not an architecture to copy. Foveonic should fit its existing GPU pipeline, typed graphs, and export ownership. Composition, splitting, fitting, and profile packaging belong in the DNG export area, with small tested helpers for the operations actually needed. Combine adjacent linear operations in the correct space and order; account for per-channel tone through the supported common curve and compensating matrix, with residual look correction where useful. Do not build a portable general-purpose rendering framework or optimize solely for one hardcoded recipe. Photographic usefulness and editing latitude are the acceptance target, not exact output across readers.

## Mapping findings

Synthetic probes used Adobe `dng_validate` 1.7.1 (2724). No SDK was vendored and no photographic GPU pipeline was changed.

- Changing `AsShotNeutral` changed the rendered white balance without changing the normalized stored samples. This establishes the standard metadata mechanism, not matching every native WB preset after our baked corrections.
- A uint16 sample representing `-0.125` using a positive offset and `BlackLevel` normalized to zero in the default SDK. The equivalent float16/Deflate sample remained `-0.125` through stage 2. Source inspection confirms that the default `SupportsPreservedBlackLevels` returns false and integer normalization clips. The offset preserves below-black distinctions in the stored integer samples, but does not guarantee that readers retain them after black subtraction.
- The renderer clips camera channels at the selected camera white before its profile look table; it also clips negative/out-of-range working RGB after the camera transform. Floating-point storage does not remove these rendering limits.
- In a synthetic highlight with a non-uniform neutral and known camera transform, insufficient normalization caused about `0.05` maximum absolute linear-ProPhoto channel error with either uint16 or float16. A common 2× range scale and matching one-stop `BaselineExposure` reduced that error below `0.00005`. This tests the scaling principle, not photographic fidelity or an approved fixed headroom setting.
- Headroom must account for the chosen camera coordinates and WB-dependent camera-white clipping, not just the largest component of an unrelated RGB buffer. Further WB edits need separate checks.
- All 288 WB/mode matrix combinations across the six DP2 Merrill files were invertible in float64; round-trip matrix error was at most `1.06e-15`. However, 240 inverse matrices had a negative coefficient. Back-projecting positive processed RGB therefore cannot guarantee nonnegative camera-like samples. This does not count actual negative photograph samples, establish GPU float32 error, or cover other Merrill bodies.

## Photographic probe

A separate diagnostic harness reran the six originals at native size using a frozen core snapshot. All are DP2 Merrill photographs. Both recipes selected Sunlight/Standard with normal content corrections: the default without reconstruction/sharpening, and default reconstruction with automatic ColorDQ and sharpening 0.5. Each active image contained 14,751,744 pixels. The latter recipe changes several operations; these results do not isolate reconstruction's effect.

The GPU back-projected the pre-tone RGB using the inverse selected camera-color matrix and removed uniform WB gains. Spatial calibration and content corrections remained baked. Standard's mode compensation was verified to be identity. This is a candidate storage basis, not restored original sensor data. Conversion, normalization, clipping, and quantization all ran on GPU; CPU work handled statistics, metadata, packaging, and JPEG compression. No production source was changed.

Three variants isolated the choices: signed float32 reference, zero-clipped float32, and zero-clipped uint16/lossless JPEG. Float32 is a measurement reference, not a replacement for the selected export encoding. The diagnostic profile used a provisional sRGB-to-XYZ/Bradford mapping, identity tone, and disabled auto-black. It is not the source's AdobeRGB/native tone or a finished profile fit. Comparisons are paired SDK sRGB renderings, not application-look equivalence.

| Photograph | Default negative pixels | Default / reconstructed-and-sharpened range, stops |
| --- | ---: | ---: |
| Blue parking sign | 57 (0.0004%) | 1.66 / 1.66 |
| Glossy leaves and buds | 19,161 (0.1299%) | 0.98 / 1.18 |
| Rocks and tree shadows | 2,420,525 (16.4084%) | 3.15 / 3.62 |
| Sunlit red flower | 0 | 2.41 / 2.43 |
| Turquoise still life | 3,366 (0.0228%) | 2.35 / 2.36 |
| Yellow flowers and stroller | 42,592 (0.2887%) | 3.24 / 3.09 |

The reconstructed-and-sharpened recipe had no negative camera-like samples in any of these six images. Except for 52 blue-sign pixels, all default negative pixels had maximum pre-tone RGB below 10% of nominal brightness. The rocks image's minimum camera-like sample was about -0.0163; its working RGB had negative components in 10,390,415 pixels, compared with 2,420,525 after back-projection. Camera-like storage greatly reduced conversion negatives here, without guaranteeing positivity elsewhere.

Range was selected from the maximum active camera-like component divided by its normalized neutral, with a 0.1% rounding allowance. These stops describe this calibrated storage mapping, not measured extra sensor dynamic range. The rocks default needed more than 8× nominal camera white in only nine pixels, so a maximum-driven allocation can spend precision on a few extreme values. No outliers were discarded.

### Reader and precision results

The 144 primary SDK runs covered both recipes, all three variants, unchanged/warm WB, -2 EV brightness, and +3 EV shadow stress. All completed without warnings. The temporary encoder was adapted from x3fuse-core `2b0dd44`, with attribution/license retained and only its cancellation adapter replaced. Its eight unit tests passed. SDK stage-1 decoding matched all 531,062,784 GPU-produced uint16 samples exactly across the twelve photographs/recipes.

- Clipping changed actual shadow color/noise, not merely unused below-black storage. In the rocks default, 1,796,086 pixels changed by more than one 8-bit sRGB code in at least one channel at baseline; +3 EV increased that to 2,391,427. Other default images had much smaller affected regions. This establishes an effect, not that clipping is photographically worse: some differences are suppressed colored shadow noise, and owner review remains pending.
- Integer rounding was small in bright areas: at baseline and -2 EV, no pixel with all reference channels at least 0.5 differed by more than one 8-bit code in any recipe. It was more apparent in deep shadows, especially after reconstruction removed native noise. For rocks with reconstruction/sharpening, baseline rounding changed 5,178,622 pixels by more than one 8-bit code, with maximum component error about 0.0129; +3 EV raised the maximum to about 0.0853. These are encoded sRGB errors under the provisional identity-tone profile, not linear-signal errors or native-look acceptance thresholds.
- A GPU-normalized float32 copy of the quantized rocks reconstruction samples rendered exactly like the uint16 JPEG at baseline and +3 EV. The measured rounding effect was therefore not JPEG loss or an additional SDK integer-normalization difference in that control.

Native-resolution 640-pixel paired crops are retained for clipping and rounding, at baseline and +3 EV. Crops are centered on maximum-error pixels and are not representative whole-image samples. They show changes mainly in fine shadow texture/noise; they do not establish visible banding, lost highlight granularity, or owner acceptance. Direct macOS conversion of a float32 SDK TIFF produced an unusably dark preview; retained review crops instead use SDK-rendered 8-bit TIFFs, with their crop coordinates and selected pixels verified.

### WB-dependent highlight range

As-shot-only range is insufficient for arbitrary later WB changes. A cooler neutral stress (R ×0.8, G ×1, B ×1.2; not a temperature-percentage claim) put camera components above SDK camera white in every recipe, including 164,406 default red-flower pixels. These tested changes needed approximately 0.39–0.59 additional stops; this is not an approved reserve or WB editing envelope.

A bounded red-flower float32 control compared original range with 1.5× range and matching exposure metadata at -1 EV. The cooler-WB comparison changed 161,024 pixels by more than one 8-bit code, with maximum sRGB component difference about 0.811. The unchanged-WB control had no differences above one code, but was not exact (maximum about 0.00261); the provisional profile/reader scaling is therefore not a proved pixel-invariant appearance mapping. The large cooler-WB effect confirms the highlight hazard and is separate from negative clipping and uint16 rounding. Native SDK 8-bit crops show false yellow patches in bright flower petals with as-shot-only range, reduced by the extra range. These remain provisional-profile pictures, not a native appearance match. Float storage alone did not prevent it.

The owner subsequently accepted negative clipping and common linear rescaling for the first slice. The observations remain useful for later signed-storage and WB-headroom refinements; they do not reverse the uint16/lossless-JPEG decision or establish a fixed headroom allocation. Final sample/profile conventions and reader acceptance remain pending.

## Evidence and limits

Diagnostic probes and their artifacts are retained locally, not checked in. The methods, results, and limitations are summarized above; the probes are not production export code or permanent conformance tests.

The final seven synthetic SDK runs completed without warnings. Initial harness failures are retained: integer rational overflow, an incomplete TIFF type-size map, invalid zero sharpness/shadow tags, and a highlight expectation that omitted SDK default auto-black adjustment. Corrected probes use `DefaultBlackRender = 1` to disable that adjustment.

The earlier signed-storage probe used uncompressed uint16 to isolate normalization and float16/Deflate with the floating-point predictor. The later photographic probe tested lossless JPEG separately as described above. Source decoding for the initial matrix checks used an already-built frozen core library; those checks remain metadata evidence, not photographic validation.

Photographic samples, generated DNGs/TIFFs, paired crops, statistics, provenance, harnesses, codec attribution, and build/run logs are retained locally. Later diagnostic executables were rebuilt as comparison/codec controls were added; the first GPU-run executable hash was not captured before that rebuild. Source hashes, final harnesses, build logs, input hashes, and immutable sample hashes are retained instead. No failing evidence was deleted.

This project's probe has not been run in Capture One or Lightroom; the owner's working x3fuse Capture One result is a separate reference. No native profile/look fit, alternate photographic WB/mode processing, other Merrill body, high-ISO survey, or owner photographic acceptance of these probes has occurred. Native signed-channel round-trip accuracy and the original editing recipe's appearance are not established by these paired SDK measurements. Sidecar checkpoint completion and production export remain separate pending work.
