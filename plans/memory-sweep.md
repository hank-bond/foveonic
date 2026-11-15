---
summary: "Reduce peak and retained application memory through measured, behavior-preserving changes."
---

# Memory sweep

Requirements: [photographic output](../docs/specs/output-strategies.md) and [performance diagnostics](../docs/specs/performance-diagnostics.md).

The owner requested an autonomous sweep, prioritizing peak memory and simpler designs. Preserve export samples and editing behavior. Keep retained-resource estimates separate from measured process or GPU peaks.

- [x] Repack JPEG and RGB8 PNG directly from packed GPU output. This avoids a six-byte-per-pixel RGB16 allocation, approximately 92 MiB at 4928 × 3264. Exhaustive RGB8 PNG quantization and existing encoder tests pass. This is an allocation calculation, not a measured process peak.
- [x] Shorten Color Field solver buffer ownership. Native equations consume 48 bytes per active native pixel and coarse equations and solutions consume 96 bytes per active grid cell. The buffers can be released after their last submitted consumer. Driver residency and process-peak effects have not been measured.
- [ ] Investigate mapped export readback, decoded-source storage and other GPU transient allocations. Measure representative workloads before claiming process-level savings or accepting complexity.
- [ ] Run applicable checks and independent review for each change. Record limits and remaining opportunities.
