---
summary: "Measure compatible decoder candidates and integrate the smallest exact-sample improvement on stable Rust."
---

# Optimize fixed X3F decoding

The owner approved decoder research, explicit SIMD and independent-worker experiments as part of scaled editing optimization. Preserve [X3F input](../docs/specs/x3f-input.md) and [profiling](../docs/specs/profiling.md) requirements. Continue the measured decoder work from the [render-resource plan](./archived/render-resources-and-optimization.md). Keep fixed X3F bitstreams compatible and preserve bounds, typed damage errors, cancellation and exact samples. Future Fuji work motivates transferable lessons without adding a Fuji decoder.

## Steps

- [x] Review primary research and stable Rust SIMD availability.
- [x] Compare fused category/payload lookup, separate NEON prediction and NEON interleaving on original files.
- [x] Finish paired-prefix and independent Classic row experiments.
- [x] Integrate the smallest winning decoder change and retain focused correctness coverage.
- [x] Validate original samples, damaged streams and cancellation, then run processor checks.
- [x] Record final results and prepare a focused reviewed commit for integration.

## Findings

[Decoder optimization research](../docs/decoder-optimization-research.md) records sources, local timings and memory costs. Fused prefix/payload lookup improves complete opening. A separate NEON predictor wins its isolated arithmetic test but loses complete opening and adds roughly 96 MiB across three full planes. Explicit NEON interleaving adds little benefit.

The selected production change preserves category fallback for long symbols and byte decoding, and adds a 16 KiB short-difference lookup per active plane. Exact comparisons pass on 29 local originals. The 46 relevant CPU tests, all-target processor Clippy with warnings denied and workspace formatting pass. Final five-repeat complete loading improves by about 27–43% on the four measured originals. Common documentation indexes and full workspace integration checks are coordinated by the integration owner.

Classic row scheduling must respect the existing bounded speculative decode pool. A scoped-thread experiment alone is insufficient for production integration. Keep this attempt's production scope focused on the fused lookup unless additional evidence justifies the scheduling work.

## Later experiments

- [ ] Validate paired-prefix decoding with identical damage-detail ordering before considering its additional measured gain for production.
- [ ] Schedule independent Classic rows within the shared bounded decode pool, preserving priorities, cancellation and worker limits, then measure foreground and speculative loading separately.

These candidates remain outside the single-difference production slice. GPU speculative-boundary decoding and nightly portable SIMD are deferred.
