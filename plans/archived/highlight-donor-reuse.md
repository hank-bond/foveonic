---
summary: "Reuse exact donor-bin provenance while preserving corrected 0.8.2 pixels and measuring the highlight recovery cost."
status: archived
---

# Reuse highlight donor classification

The owner approved removing repeated donor classification after the [release performance audit](../performance-diagnostics.md#release-workflow-baseline-follow-up). The change follows the [highlight restoration requirements](../../docs/specs/highlight-restoration.md), retaining exact boundary classification introduced in 0.8.2, stable row-major ordering, sequential sums, lookup interpolation and photographic behavior. This is an implementation optimization, not a new recovery method or a release publication request.

The histogram records each donor's exact bin. Prefix counts and stable scatter place each donor once within that bin's range. The table can trust this provenance instead of repeating classification in its serial loop. Original pixel input remains immutable throughout these passes. The existing chunk barriers and division/accumulation order remain unchanged.

- [x] Remove only redundant classification and retain exact histogram decisions.
- [x] Extend grouping coverage through the raw table sums and counts, including multi-block inputs, partial tails, empty bins and a bin crossing multiple 128-donor chunks.
- [x] Review the implementation independently.
- [x] Run targeted GPU checks on Metal and Windows DX12/DXC.
- [x] Compare real-photo floating output byte for byte against corrected 0.8.2 processing.
- [x] Repeat matched native Off latency and separate GPU attribution, with memory checks separate from timing.
- [x] Record results and make the candidate available for owner testing before committing or publishing.

Builds and measurements ran on the owned remote test hosts, keeping the primary Mac available for the owner's work. Small reports, hashes and setup failure evidence were retained. Successful full-float captures were removed after comparison.

The Mac initially had Rust 1.67 and no CMake. The owner approved shared build-tool preparation. Rust stable 1.100 and checksum-verified CMake 3.31.12 are available for reuse across builds. Both renderer variants built on Metal and Windows. The extended table assertion initially needed an explicit `u8` cast target because serde JSON provides another applicable equality implementation. That test-only type fix preserves the shader candidate and existing failure logs.

## Validation

Final Mac artifacts passed all 10 targeted highlight GPU checks, 631 ordinary core tests and release Clippy with warnings denied. Windows passed 629 ordinary core tests and release Clippy. The first GPU attempt passed nine tests but could not run the original-photo inspection because the fixture path was unset. Staging the pinned originals and setting the path resolved that setup failure. The final Windows GPU rerun also passed all 10 tests after the numerical matrix. The private Windows ZIP also passed the normal-stack native desktop smoke, reaching all ten checkpoints with the audited executable. These are scoped checks for this change, not a new full release acceptance sweep.

All 144 maintained photo render cases on Metal matched rebuilt exact 0.8.2 processing byte for byte, including complete float images, actual settings, resolution, processing quality and geometry. Both variants used the same host and toolchain. The matrix covers 13 pinned originals, 18 recipes, four resolutions and both processing policies. Successful float captures were deleted after comparison. Windows DX12/DXC also passed all 144 cases against its own same-toolchain 0.8.2 baseline.

Rust 1.100 Clippy also identified an existing Windows-only literal `None.unwrap_or_else` in the process boot-key fallback. The cfg branches now produce the same fallback bytes directly on Windows. macOS and Linux keep their existing optional OS boot identifier and fallback. Cache policy, hashing and nonce format are unchanged.

## Measured behavior

The native GUI comparison used the published 0.8.2 app and the private candidate on the same Apple M1 Pro. Both used the Hazy Coast original, quality processing, a scale-two 2496 × 1664 working image, the same window and viewport geometry, a 2 GiB memory-cache budget and disabled application disk cache. OS file-cache state was uncontrolled. Five fresh launches per version contributed five initial selections and 25 warm edits per recipe. The native workspace smoke check passed with the default application stack.

| Recipe and action | Before median | Candidate median | Before p95 | Candidate p95 |
| --- | ---: | ---: | ---: | ---: |
| HDR initial selection | 731.45 ms | 703.12 ms | 763.73 ms | 713.33 ms |
| HDR warm edit | 67.12 ms | 50.81 ms | 239.38 ms | 238.28 ms |
| Default initial selection | 507.96 ms | 509.07 ms | 510.27 ms | 510.31 ms |
| Default warm edit | 50.71 ms | 50.57 ms | 61.07 ms | 60.92 ms |

HDR median warm-edit latency fell about 24%. The HDR tail did not materially improve. Default processing stayed within ordinary run variance. These observations cover this workflow and do not establish a general application speedup.

Three separate paired GPU captures alternated baseline/candidate order. Executed current-image Highlights work used the same 11 dispatches and geometry. GPU pass-work samples were `[55.406, 49.992, 53.792]` ms before and `[32.974, 35.119, 35.443]` ms after. Median pass work fell about 35%, from 53.792 to 35.119 ms. This sum describes GPU pass work, not critical-path latency. All six traces were loss-free with valid behavior and complete current-image GPU queries. Each capture ended with one auxiliary scope query pending. No extra completion wait was introduced to close the recording.

Memory was sampled separately through macOS physical footprint every 100 ms. The initial three trials per version showed higher candidate peaks. A reverse-order control then tested the candidate, an exact 0.8.2 GUI rebuilt with the candidate toolchain, and the published app again. Sampled trial peaks in MiB were:

| Group | Peak samples |
| --- | --- |
| Initial published 0.8.2 | 1032.14, 1026.52, 1031.66 |
| Initial candidate | 1188.49, 1190.19, 1187.10 |
| Repeated candidate | 1191.96, 1033.47, 1186.97 |
| Rebuilt 0.8.2 control | 1034.13, 1193.14, 1031.83 |
| Repeated published 0.8.2 | 1034.07, 1072.78, 1190.99 |

Both high and low peaks occurred in unchanged baselines and the candidate. A fixed additional memory cost was not reproduced consistently. The higher mode occurred in five of six candidate trials and two of nine baseline trials, so the memory effect remains uncertain. Recorded retained GPU and reserved bytes matched at corresponding steps, with only small CPU bookkeeping differences. The patch adds no explicit buffer or host allocation paths. The sample count and transient variation do not establish equivalent peak memory, and sampling can miss peaks between observations. No memory improvement is claimed.

The native baseline is the published app. The numerical baseline is a separately rebuilt processing executable. The private candidate was built with Rust 1.100 and is ad-hoc signed on macOS. This evidence does not cover Windows latency, all camera workflows, every ignored GPU test or a full new-release gate. Published 0.8.2 is unchanged.

Private macOS and Windows test ZIPs were made available to the owner. The Windows archive includes the existing pinned DXC DLLs and their licenses. No public asset, version, installation or default was changed. The initial slice was handed over uncommitted for owner review.
