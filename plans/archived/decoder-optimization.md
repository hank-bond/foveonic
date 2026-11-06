---
status: archived
summary: "Measure compatible decoder candidates and integrate the smallest exact-sample improvement on stable Rust."
---

# Optimize fixed X3F decoding

The owner approved decoder research, explicit SIMD and independent-worker experiments as part of scaled editing optimization. Preserve [X3F input](../../docs/specs/x3f-input.md) and [profiling](../../docs/specs/profiling.md) requirements. This extended the measured decoder work from the [render-resource plan](./render-resources-and-optimization.md). Keep fixed X3F bitstreams compatible and preserve bounds, typed damage errors, cancellation and exact samples. Future Fuji work motivates transferable lessons without adding a Fuji decoder.

## Steps

- [x] Review primary research and stable Rust SIMD availability.
- [x] Compare fused category/payload lookup, separate NEON prediction and NEON interleaving on original files.
- [x] Finish paired-prefix and independent Classic row experiments.
- [x] Integrate the smallest winning decoder change and retain focused correctness coverage.
- [x] Validate original samples, damaged streams and cancellation, then run processor checks.
- [x] Record final results and prepare a focused reviewed commit for integration.

## Findings

[Decoder optimization research](../../docs/decoder-optimization-research.md) records sources, local timings and memory costs. Fused prefix/payload lookup improves complete opening. A separate NEON predictor wins its isolated arithmetic test but loses complete opening and adds roughly 96 MiB across three full planes. Explicit NEON interleaving adds little benefit.

The selected production change preserves category fallback for long symbols and byte decoding, and adds a 16 KiB short-difference lookup per active plane. Exact comparisons pass on 29 local originals. The 46 relevant CPU tests, all-target processor Clippy with warnings denied and workspace formatting pass. Final five-repeat complete loading improves by about 27–43% on the four measured originals. Common documentation indexes and full workspace checks remain part of the coordinated integration validation.

The completed follow-up keeps the single-difference lookup and adds bounded Classic row scheduling. Active loads use at most four inherited workers. Speculative photos queue disjoint row chunks directly in their configured shared FIFO pool, preserving admission, per-row cancellation and earliest-row errors after all tasks join. No additional full-image allocation is introduced.

Final five-repeat source-opening medians improve from 98.528 to 27.115 ms on SD9, 99.547 to 28.491 ms on SD10 and 135.266 to 37.401 ms on SD14. Shared pools of one, two and four workers preserve exact samples and their configured concurrency. The final 53 focused CPU tests, all-target processor Clippy and workspace formatting pass. A separate SD14 harness memory trial adds about 208 KiB peak footprint and 368 KiB peak RSS with foreground workers, with the same expected/current frame ownership in both modes. These are local process observations, not memory limits.

## Completed follow-ups

- [x] Validate paired-prefix decoding with identical damage-detail ordering before considering its additional measured gain for production. The compatible candidate passes 29 originals and damage/cancellation checks but regresses three of four matched source-opening trials. Reject and remove it while retaining the single-difference lookup.
- [x] Schedule independent Classic rows within the shared bounded decode pool, preserving priorities, cancellation and worker limits, then measure foreground and speculative loading separately. Real-row sample/error checks and deterministic pool tests cover one, two and four workers, photo admission, active/queued cancellation and joining every chunk after failure.

GPU speculative-boundary decoding and nightly portable SIMD remain outside this completed investigation. There is no further codec implementation in this attempt.
