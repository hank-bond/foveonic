---
summary: "Store decoded sensor codes as lossless 16-bit integers within the existing disk cache."
status: archived
---

# Compact decoded cache

Requirements: [decoded source cache](../../docs/specs/decode-cache.md).

## Scope

Keep the existing cache identity, LRU, publication, scheduling and float32 processing model. Replace float32 sample payloads with signed or unsigned 16-bit codes. Rebuild obsolete entries on demand. Early sensor downscaling and orchestration changes are separate work.

## Steps

- [x] Measure conversion and payload loading before implementation.
- [x] Implement compact encoding and bounded conversion in the existing load and publication paths.
- [x] Check exact signed/unsigned conversion, chunk boundaries, corruption, obsolete entries and original camera fixtures.
- [x] Run repository checks and review the scoped change.

## Findings

An optimized local harness checked actual Merrill and Quattro decoded samples. Serial conversion including float allocation took about 4.9 ms and 3.3 ms respectively. Eight-thread conversion took about 2.7 ms and 2.0 ms. OS-cache-warm payload reads with checksums took about 36.3 ms and 24.6 ms for float32. Streaming compact reads, checksums and conversion took about 14.8 ms and 11.0 ms. These are eight-sample measurements, exclude metadata parsing, and do not measure cold storage.

Decoded sample bytes halve. Total bundle capacity improves less because original files, metadata and previews retain their existing representation. Processing memory remains float32. Production conversion uses a bounded scratch buffer while reading rather than allocating another full sensor array or adding a parallel work owner.

The implemented cache round-tripped 29 local originals through its ordinary background preparation and foreground loading APIs. Coverage included Classic signed codes, Merrill full/reduced modes, Quattro and Quattro H. Samples, camera metadata, CAMF bytes and storage layouts matched exactly. Controlled OS-cache-warm comparisons of complete opens, including metadata handling, measured about 37.2 → 19.1 ms for Merrill and 25.2 → 15.3 ms for Quattro. Each used two warmups and eight measured opens, with no competing app or compiler workload detected. Decoded entries shrank from about 195 to 99 MB and 133 to 68 MB respectively.

The original-file persistence test previously waited for the decoded file to appear while background companion publication could still occupy its bounded queue. It now checks persistence directly through synchronous storage and loading. The separate ordinary-cache API probe covers complete bundle preparation and reuse. No application queue policy changed.

## Validation

Workspace tests passed on both the integrated working state and the isolated main-based cache change. The original-family persistence fixture and complete-bundle loading with the original and sidecar unavailable passed in release mode. Strict workspace Clippy, formatting and all five artwork tests passed. Exhaustive code tests covered every signed and unsigned 16-bit value. Disk-format tests checked exact compact lengths, chunk boundaries, checksum failures, obsolete float entries and rejection of lossy samples without replacing existing data.
