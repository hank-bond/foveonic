---
summary: "Bound directory-triggered input and preview allocations and report browser GPU allocation failures."
status: archived
---

# Browser loading failures

Requirements: [X3F input](../../docs/specs/x3f-input.md) and [browser previews](../../docs/specs/browser-previews.md).

The owner reports a crash when opening a directory, with no reproduction or machine details. The audit found conditional memory failures rather than a deterministic directory-scan panic. Opening a folder selects its first photo and starts metadata, thumbnail and background cache preparation. The owner authorizes fixes and bounded reproductions of these risks.

## Approved work

- [x] Bound aggregate decoded property strings in the shared parser, preserving valid overlapping references. Reproduce amplification with a small synthetic X3F and verify early rejection.
- [x] Validate a fixed header before allocating and reading the complete original. Use a tracked reader claiming a large payload to prove bounded reads.
- [x] Reduce preview allocations while preserving linear-light color and orientation. Cover browser preparation and larger disk-cache previews. Compare the existing sampler and measure synthetic workloads in separate processes.
- [x] Recover browser upload out-of-memory errors without blocking the UI or changing the shared device's error-scope stack. Exercise controlled injection rather than exhausting the GPU.
- [x] Obtain independent largest-model review, address findings, run targeted browser/input tests and provide an owner review build.

## Evidence and limits

A PROP-only synthetic file with 65,536 UTF-16 ASCII characters and 512 unique suffix keys is about 135 KB. The previous CLI peaks at about 42 MB, versus 8 MB with one key, before rejecting the missing sensor. This safe probe demonstrates allocation growth, not the reported crash. The shared parser now caps encoded bytes and aggregate decoded string content at 1 MiB each. Allocator capacity and map overhead remain additional bounded memory.

The first independent input review found no blocker. A test initially treated an arbitrary rotation value as invalid, but the existing reader accepts that value. The corrected tracked-reader cases exercise invalid signature, version and byte order. Supported versions, short headers and changing payload length remain covered.

The linear preview view avoids a complete RGB32F copy alongside RGB16 and reuses the existing Triangle resampler. Its internal scratch still depends on source width and target height. Cancellation remains at the resampler boundary. Thumbnail reduction occurs before rotation. Complete embedded JPEG decoding and up to three background photos remain bounded pressure contributors.

Device-global error scopes cannot isolate UI uploads from simultaneous processing. Native wgpu reports allocation errors synchronously on the calling thread. A thread-local upload guard recovers only an upload's initial out-of-memory error and its subsequent invalid-resource errors. It preserves the initial cause through cleanup. Other errors retain existing fatal behavior. Device loss and broad renderer recovery remain separate lifecycle work.

Changes are isolated on `fix/browser-loading-memory` after another session reset the shared checkout during implementation. Reproductions are generated from checked-in tests and require no private photographs.

## Completed checks

The ordinary core suite passed 574 tests. The GUI suite passed 538 tests. Three native GPU preview tests passed, including controlled failures at texture creation, upload, view creation and registration. The registration check verifies the failed renderer entry is freed and a later photo can still upload. The folder-level test keeps a healthy thumbnail and metadata loading after a malformed property file. Strict release Clippy for both crates and all targets, formatting and documentation indexes passed.

CLI checks reject the 64 MiB sparse invalid-header file and the 512-key metadata fixture with named damaged-file messages. Metadata is unchanged against the prior binary for seven originals covering Classic, Merrill HI/MED/LOW and Quattro, including Quattro H. Preview reduction matches materialized linear samples exactly across all declared color spaces and odd portrait/landscape grids. Orientation comparisons allow the existing half-float display precision.

A child-process memory-limit experiment was unavailable on this macOS host (`RLIMIT_DATA` was rejected). No real out-of-memory abort is claimed. Native GPU failures are injected through the same upload error capture used by the installed handler. The original reported crash remains unconfirmed without that user's file or crash report.

The review bundle is built separately from the isolated source checkout. No merge or publication is part of this fix. A synthetic 12-megapixel preview allocation probe is checked in as an opt-in test, with separate materialized/streamed process modes. Memory and timing comparisons start only after competing compiler, app and profiler processes finish.

## Allocation comparison

The final comparison ran in a verified quiet interval, with a 100 ms competing-process monitor reporting none throughout. Separate child processes used one excluded warmup and three measured samples per mode on the same 4096 by 3072 RGB16 synthetic preview, reduced to 2048 by 1536. Materializing the full linear source peaked at about 374 MB RSS. Streaming through the same sampler peaked at about 223 MB, a reduction of about 151 MB (40%). Median reduction time fell from 202 to 150 ms. Every result retained the same CRC32, `964404ff`. These are synthetic preview preparation observations, not application-latency acceptance thresholds.

Separate CLI probes peaked at about 42 MB before versus 9 MB after the property budget, using the same 135 KB malformed metadata file. The sparse 64 MiB invalid-header file peaked at about 75 MB before versus 7 MB after early validation. Both candidate cases return a named damaged-file error. These cases demonstrate removal of the allocation pressure without exhausting the host. They do not establish the original user's crash cause.
