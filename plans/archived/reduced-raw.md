---
summary: "Reduced RAW modes use shared family recipes with real Merrill and synthetic Classic/Quattro validation."
status: archived
---

# Support reduced RAW modes

Requirements: [X3F input](../../docs/specs/x3f-input.md). The owner approved all supported camera families and reference-derived implementation where independent originals cannot be found. Use the existing reference recovery rather than repeating Merrill decompilation.

- [x] Decode and render the provided DP3 Merrill LOW and MED originals through color and monochrome paths.
- [x] Separate declared photograph dimensions from physical sensor storage and active area.
- [x] Preserve photographic aspect ratio through crop and fine rotation, and avoid unsupported MED regional preview mapping.
- [x] Share original loading and cache-miss validation/error reporting.
- [x] Recover Classic reduced output sizes and connect them to metadata-sized calibration and output geometry.
- [x] Validate Classic reduced processing with complete synthetic recipe checks.
- [x] Implement Quattro LOW equal-resolution decoding, calibration, repair and reconstruction using its recovered branches.
- [x] Complete export, native workflow and full-resolution regression checks.

## Findings

DP3 Merrill LOW stores 2464×1632 samples and produces 2336×1568 before orientation. MED stores 4928×1632 samples with a 4704×1568 active area and produces 3264×2176. Both provided captures have 90-degree orientation. Their active area begins at row 18, requiring bounded top padding as well as the bottom padding identified by the earlier [SD1 input slice](./sd1-input.md). Color and monochrome render checks cover both originals. SD1 HI and LOW remain in that check.

The X3F base header declares photographic width and height at offsets 28 and 32. Version four moves those fields to 40 and 44. Full and capture-only readers retain identical properties. Crop and rotation compose in photographic coordinates before mapping to physical storage. Merrill regional detail currently requires one-to-one presentation geometry, so MED retains whole-frame rendering.

The Classic reference manager selects SD9/SD10 output sizes 2268×1512, 1512×1008 and 1134×756 for HI, MED and LOW. The later SLR branch selects 2640×1760, 1776×1184 and 1296×864. Stored sensor dimensions and active rectangles come from each original. Acceptance requires a supported SLR, recorded reduced mode, matching declared photograph size and sensor dimensions bounded by the full sensor. Synthetic MED and LOW sources derived from each full-resolution SD9, SD10, SD14 and SD15 original pass color and monochrome rendering and TIFF/DNG export. These branches still await independent reduced originals.

Quattro LOW uses equal-resolution RGB rather than six-value RGB/blue-phase cells. The reference decoder selects ordinary unsigned plane decoding and format-35 special-column replacement at the reduced grid. Preprocessing uses LOW calibration, omits high-resolution blue repairs, then refreshes mono from restored RGB blue. Luminance filtering and residual detail have distinct LOW controls. A decoder-only change or a smaller HI buffer would not complete that rendering route. Quattro has no MED RAW mode.

The public raw.pixls.us catalogue and the public Kalpanika test repository contain full-resolution examples. Historical Kalpanika issue reports name reduced Classic, Merrill and Quattro originals, but no working reduced download was found. An old Quattro H archive link currently returns a download challenge instead of an archive. Missing real-file checks remain explicit. Do not treat synthetic checks as independent photographic validation.

## Processing evidence

Unsigned formats 31, 35, 37, 39 and 41 select RGB triplets when all three planes have the recorded sensor extent. Format 35 replaces special blue columns at the selected physical grid. Unit checks cover ordinary and special-column RGB layouts and retain the packed HI layout.

Quattro LOW preprocessing omits phase gain and HI blue defect workers. Sparse repair operates on RGB, HLP calibration selects the LOW LUT and BAVE gain, and restored RGB supplies the scalar blue guide. Reference-worker checks use owned synthetic planes with guarded buffers. RGB sparse repair matches exactly across sparse and dense cases with both correction settings. LOW HLP RGB and blue differ by at most 0.0078125 camera codes, with identical scratch values. Scalar bilateral filtering differs by at most 0.000061035156 camera codes across eight cases. These worker comparisons do not establish complete native photographic parity or final internal defect-mask parity.

Complete synthetic LOW recipes derived from DP0, DP1, DP2, DP3, sd Quattro and Quattro H originals pass color and monochrome rendering with filtering enabled, finite samples and TIFF/DNG export. LOW uses the native reduced geometry throughout and retains whole-frame preview rendering. Full-resolution packed extraction, column correction, reduced preview reconstruction and interchangeable-lens preprocessing regressions pass. Real Merrill HI/LOW and the provided DP3 LOW/MED color and monochrome regressions pass.

## Final checks

Workspace tests, strict Clippy, formatting, generated documentation indexes and five artwork checks pass. The native app completes the provided MED original through final rendering, zoom, histogram/parade/waveform and sensor/calibration inspection. Its TIFF and editable DNG both declare 2176×3264 after orientation. Shared crop/rotation GPU checks pass. Original-file tests cover SD1 HI/LOW and DP3 LOW/MED with color and monochrome, Full/Fast and the supported Merrill noise methods. Unrelated help-example and release edits remain unchanged.

Independent reduced Classic and Quattro originals remain desirable photographic validation inputs. The implemented branches currently have reference-derived preparation, numerical worker comparisons where available and synthetic complete-recipe/export coverage. No claim of complete photographic parity is made for those missing originals.
