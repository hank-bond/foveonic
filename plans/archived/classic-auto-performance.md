---
summary: "Parallelize the measured serial reductions in Classic automatic white balance."
status: archived
---

# Optimize Classic automatic white balance

The owner approved optimizing the measured Auto estimator while preserving its processing behavior. Follow the numerical and camera-independent operation requirements in [Classic rendering](../../docs/specs/classic-rendering.md) and the timing distinctions in [profiling](../../docs/profiling.md).

- [x] Retain baseline renders and separate first-estimate timing from cached renders.
- [x] Parallelize black statistics, candidate scores and weighted neutral accumulation into bounded deterministic chunks.
- [x] Compare final images and neutral statistics with the baseline and independent evidence.
- [x] Measure first-estimate latency, review numerical differences and run required checks.

SD10's selected Auto request spends approximately 600 ms of GPU time in estimation, dominated by serial accumulation, candidate scoring and black statistics. Source decoding and the cached rendering path are separate costs. Keep image arithmetic on the GPU and read back compact statistics only. Preserve candidate selection, weights, histogram policy and the engine-owned latest-source cache. Do not change camera recipes or thumbnail processing during this pass.

## Results

All three reductions use bounded 256-pixel GPU chunks. Only compact compensated partial sums leave the GPU. The host combines them in fixed order with Float64 precision. Candidate policy and its Float32 arithmetic, final Float32 weighted-sum boundary, histogram rules and engine caching remain unchanged. Signed sensor black sums are exact within each chunk.

On the Apple M1 Pro, the selected SD10 Auto request's total GPU estimator time falls from approximately 603 ms to 17 ms in separate synchronized diagnostics. Completed first-render wall time, excluding source decoding and engine setup, falls from 832 ms to 231 ms. SD9 falls from 1091 ms to 268 ms and SD14 from 857 ms to 255 ms. Cold values include preparation and first-use work. Cached renders do not rerun the estimator. A cached-calibration DP1 control retains byte-identical output.

SD10 final Float32 image data is byte-identical to the baseline. SD9 and SD14 improve existing reduction precision: estimated neutrals now agree with independent reference receipts within 5e-16 on all three originals. Their full-sensor linear ROMM buffers change with mean absolute errors of 0.0000522 and 0.0001446 relative to the earlier GPU output. Largest component differences are 0.00293 and 0.02796. These are measured numerical corrections, not universal bit-parity claims. Isolating the black-statistics optimization leaves the same image differences, confirming that weighted/scoring reduction precision caused them.

## Validation

Three independent GPU fixtures cover signed black sums with long tails and unequal shields, all six candidate scores over the original first-quarter domain, sparse weighted tails and the special radius-30/radius-45 coefficients. Existing signed channel means and shader validation still pass. Original-file tests validate preparation controls, neutral estimates and zero-work cache reuse on SD9, SD10 and SD14.

All 329 ordinary workspace tests, strict all-target Clippy, formatting and documentation checks pass. The native app bundle is rebuilt. Changes remain uncommitted for owner review.
