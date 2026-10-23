---
status: archived
summary: "Implement bounded decoded-source disk reuse, app settings and Quattro regional Smart refinement."
---

# Decode cache and Smart preview

The owner authorized implementing the [decoded-source cache](../../docs/specs/decode-cache.md) and [Quattro Smart preview](../../docs/specs/smart-preview.md). The preceding Quattro integration and progress work was committed as `d301a08` before this slice.

- [x] Add exact decoded-source persistence with source/version identity and reboot-scoped temporary storage.
- [x] Add bounded asynchronous insertion, LRU eviction, configuration and clear/usage controls.
- [x] Verify all source families, invalidation, exact samples/metadata, failures and cache limits.
- [x] Trace Quattro global dependencies and local supports, then implement regional processing after reusable global work.
- [x] Add Smart overview/detail presentation, zoom transitions and safe regional supersession.
- [x] Verify regional results, actual reduced work, native interaction and exports.
- [x] Run required workspace/documentation checks and rebuild the local app.

Smart is Quattro-only. Cache storage supports all three families, with Classic disabled by default. Full-frame stages may remain when their dependencies require them. The goal is lower visible latency, with accepted additional total work and tiny preview approximations where needed.

## Agreed refinements

The owner requested a provisional cache-ready badge in both organizer layouts, ahead-of-time directory decoding with foreground priority, stable workspace preview selection before render completion, and corrected DP0 preview framing. Smart now renders one clicked viewport, then immediately completes the entire frame. Panning never restarts regional processing.

## Implementation findings

Decoded storage preserves native Float32 bits, sensor layout, decompressed camera metadata and property-list ordering. The temporary namespace is scoped to boot identity. One background actor handles insertion, maintenance, a one-pass folder queue and readiness scanning. macOS background QoS applies to CPU and storage work. Foreground leases cover queued and running image jobs and scopes. Background file reads and writes pause between 1 MiB chunks, and Huffman planes inherit row-level pause checkpoints. Remaining neighbors receive priority without repeating the pass. A foreground selection can decode independently of one paused background source to avoid waiting for prefetch.

Quattro regional rendering retains whole-frame calibration, high-blue repairs, highlights and the first two processing stages. Later reconstruction and finishing use a source rectangle with a 512-pixel context margin and 128-pixel alignment. Wavelet geometry preserves the original 64-pixel phase and physical edge padding. Regional Float32 output exactly matches Full crops for the six validated sources and three positions each. The subsequent whole-frame continuation also matches independent Full samples exactly.

DP0's large JPEG is 4816×3616 while its Raw photograph is 5424×3616. A separate 160×106 JPEG preserves the full framing. Other originals keep their tiny thumbnail in Exif instead of a separate section. The preview reader now supports both, removes only confirmed black Exif padding, and uses version-4 header dimensions for native photograph coordinates. Six original Quattro previews now load at thumbnail size, including portrait and Tiger input, without sensor decoding.

## Validation

CPU checks cover exact storage, identity/version/checksum failures, eviction, disabled policies, clearing, neighbor order, foreground pause leases, early architecture availability and bounded Exif extraction. Original-file checks cover Quattro, Merrill, legacy interleaved Classic and format-30 Classic. Cold folder prefetch verifies that foreground work prevents insertion, then caches enabled Quattro/Merrill sources while skipping Classic.

Native landscape and portrait smoke checks exercise the loading preference, Fast overview, exact 100% zoom, detail-first Full refinement, whole-frame completion, panning without another render, edits, Fit, both badge layouts, settings and a decoded-cache hit on reopen. Exports remain Full under Fast and Smart preferences. The scope and navigator source/edit pairing remains covered by existing checks.

Matched five-run warm DP0 measurements on Apple M1 Pro use identical Sunlight/Standard core defaults, independent completed Fast sessions and a 992×1002 native tile, excluding decoding. Before retaining the region's whole-frame inputs, direct Full took median 773 ms, the tile 509 ms and the additional continuation 652 ms. Recipe-owned session retention now preserves original phase cells and the region's shared full-frame inputs under the existing 1 GiB interactive budget. Shared graph/cache code admits these boundaries without camera-specific numerical behavior. Medians became 751 ms direct Full, 490 ms for first detail and 394 ms for the continuation. Median time until whole-frame completion was approximately 891 ms. The progressive route therefore presents detail about 260 ms earlier and costs about 140 ms more overall. These are completed engine/view-preparation measurements under fixed controls, separate from native single-run receipts and scanout latency. The example in the [profiling guide](../../docs/profiling.md) reproduces the method. Workspace verification passes 471 ordinary tests, strict all-target Clippy, formatting, generated documentation indexes and their read-only check. The local native app was rebuilt without installation. The badge design remains provisional for owner review. Changes remain uncommitted.
