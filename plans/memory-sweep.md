---
summary: "Reduce peak and retained application memory through measured, behavior-preserving changes."
---

# Memory sweep

Requirements: [photographic output](../docs/specs/output-strategies.md) and [performance diagnostics](../docs/specs/performance-diagnostics.md).

The owner requested an autonomous sweep, prioritizing peak memory and simpler designs. Preserve export samples and editing behavior. Keep retained-resource estimates separate from measured process or GPU peaks.

- [x] Repack JPEG and RGB8 PNG directly from packed GPU output. This avoids a six-byte-per-pixel RGB16 allocation, approximately 92 MiB at 4928 × 3264. Exhaustive RGB8 PNG quantization and existing encoder tests pass. This is an allocation calculation, not a measured process peak.
- [x] Shorten Color Field solver buffer ownership. Native equations consume 48 bytes per active native pixel and coarse equations and solutions consume 96 bytes per active grid cell. The buffers can be released after their last submitted consumer. Driver residency and process-peak effects have not been measured.
- [x] Release the speculative decoded source before publishing selection assets. This shortens its overlap with publication; a native Merrill source holds approximately 184 MiB of sample data. The earlier decode peak is unchanged. The portable cache tests pass; original-file prefetch tests require declared fixtures.
- [x] Encode photographic output directly from mapped staging bytes. This avoids an eight-byte-per-pixel host copy, approximately 123 MiB at 4928 × 3264. The staging allocation stays mapped during encoding. Process and GPU peak effects require measurement. The scoped error/readback and synthetic JXL export GPU checks pass.
- [x] Reuse color texture pyramid planes after queued reads. The spatial path requests three scalar planes instead of nine, avoiding 24 bytes per atlas pixel of allocation traffic. Existing GPU band and size tests pass. Actual peak and byte-for-byte comparisons against the earlier allocation path remain to be measured.
- [x] Reuse monochrome texture planes, reducing scalar allocation requests from ten to four per active invocation. The portable monochrome tests and a full-photo cached/fresh and DNG boundary check pass. Process peak and comparison to the earlier allocation path remain unmeasured.
- [x] Release GUI preview source samples after linearization and unrotated floats after rotation. This shortens overlap by 6 bytes per input pixel, plus 12 for rotated previews. GUI rotation and resize tests pass.
- [x] Widen ICC preview input in bounded chunks instead of a full RGB16 copy, avoiding 6 bytes per preview pixel. Preview tests and an exact multi-chunk comparison pass.
- [ ] Investigate decoded-source storage and other GPU transient allocations. Measure representative workloads before claiming process-level savings or accepting complexity.
- [ ] Run applicable checks and independent review for each change. Record limits and remaining opportunities.
