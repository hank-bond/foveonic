---
summary: "Prepare all browsing and rendering inputs from one original read and keep cache hits independent of card I/O."
status: archived
---

# Single-read cache

The owner reported a cached green photo waiting about five seconds at an early loading percentage in Import mode after the preceding photo had finished. The exact stall has not been reproduced. Investigation found duplicate source capture-property reads before render submission and synchronous source identity/sidecar checks in cache and preview lookups.

The owner approved caching capture metadata and requested one ahead-of-time original read that also prepares thumbnails and larger previews. The [decoded cache specification](../../docs/specs/decode-cache.md) records the resulting contract.

- [x] Derive previews from the same compressed original buffer used for RAW decoding.
- [x] Publish a complete browsing/rendering readiness state, including capture metadata and available embedded previews.
- [x] Resolve ready selection, source samples, capture defaults, saved edits and previews from a local snapshot without synchronous source reads or source-file stat calls.
- [x] Keep identity refresh and changes outside the foreground loading path. Preserve per-photo edits and source-change invalidation.
- [x] Add missing phase timings to existing opt-in performance output and provide a reproducible capture command.
- [x] Verify cached behavior with the original unavailable, corruption/eviction/policy changes, and native Import/Smart workflows.

## Findings

The settled previous photo rules out waiting for its foreground source read in the reported occurrence. It does not establish a background GPU stall. Previously capture/default loaders independently read source capture properties, and render submission waited for the editing-recipe loader. Preview keys read source identity and sidecars twice. The browser read the embedded source thumbnail before looking for a disk preview. RAW prefetch reopened the original for a larger JPEG preview. These dependencies could stall on card access even when decoded samples were cached.

The existing decoded format remains unchanged. Atomic checksum-protected selection metadata and original-byte companions share its identity and temporary LRU budget. Retaining compressed originals also makes Copy originals imports independent of card reads. This reduces the number of photographs fitting in the same byte budget. The worker extracts both embedded preview sizes from its original buffer and publishes each complete photo independently. Correctly framed small previews cover cameras whose larger embedded JPEG is cropped.

Known ready identities are background-verified snapshots. Foreground cache access does not stat or reopen the source or sidecar. Refresh observes changed source identities and external sidecars. Successful sidecar saves update the local recipe immediately and queue its disk update. An unavailable original may retain its verified local bundle. Unknown and incomplete entries still use the ordinary source path. Uncached browsing may read a small embedded thumbnail before the full pre-cache read.

## Validation and limits

A real-photo test prepared the bundle, made both the source and sidecar unavailable, and verified capture properties, saved edits, thumbnail, larger preview, decoded samples, and a byte-identical original import with preserved modification time. Readiness survived background refresh. Metadata corruption was rejected. Existing tests cover source replacement, payload corruption, policy changes, eviction and bounded scheduling.

Production review binds readiness to the exact verified identity and rechecks selection snapshots after local metadata reads. Truncated original companions cannot advertise readiness, and checksum failures cannot publish an imported file. Denied metadata writes preserve the newer in-memory recipe. Budget tests account for complete bundles and protect nearby companions against distant insertions. Corruption fixtures run independently of background writers.

Isolated native Merrill Smart, Quattro Smart and Import checks completed all targets, including cache reopen, repeated processing and copying originals. Local Merrill cached selections took approximately 7–11 ms before worker submission, and decoded payload loading took approximately 83–97 ms on an M1 Pro. These observations do not reproduce or explain the owner's five-second pause. The [profiling guide](../../docs/profiling.md) provides the opt-in capture command and source/cache phase timings for diagnosing a recurrence. Workspace tests and strict Clippy passed.
