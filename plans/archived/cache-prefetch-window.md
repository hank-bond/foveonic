---
summary: "Bound background decoding around the selected photograph and retain nearby results under the disk budget."
status: archived
---

# Prefetch around selection

Requirements: [Decoded source cache](../../docs/specs/decode-cache.md). The owner approved replacing the whole-folder decode pass with configurable counts around selection, defaulting to three behind and seven ahead. The reviewed import and parallel-decoder checkpoint is committed separately.

- [x] Replace folder-wide queuing with a selection neighborhood using visible browser order.
- [x] Persist independent behind/ahead counts and expose them beside the CPU budget.
- [x] Cancel superseded work and favor nearby decoded data during eviction.
- [x] Verify navigation, filtering, budget pressure, exact data reuse and native settings.
- [x] Present the new behavior for review and obtain approval to commit this follow-up.

The browser publishes order changes after folder opening, filtering and active-photo changes. Cache policy owns the neighborhood and replacement queue. Hidden rows do not consume neighbor slots. An active photo concealed by a history update remains in the order until another photo becomes active. Repeated snapshots do not restart completed or cleared work.

The current photo comes first, followed by increasing distance, favoring forward neighbors on ties. Every new selection replaces the old queue and invalidates its speculative epoch. Counts clamp naturally at directory ends without extending the other side. Both counts may be zero. No selected photo means no prefetch. Existing cancellation and bounded publication rules remain intact.

The shared eviction module considers incoming data before deleting existing files. It evicts unrelated data in LRU order, then farther neighbors before closer ones. Within one photo, decoded RAW data has preference over its display preview. If admitting a farther entry would evict closer data, insertion is skipped. The byte limit still applies when the requested neighborhood is larger than the cache. A pass finishes after those attempts and does not repeatedly decode rejected insertions. Entries outside the current window remain reusable until ordinary eviction.

The owner prioritized individual nearby-photo latency over simultaneous single-thread photo throughput. A shared scoped FIFO layer pool replaces one CPU worker per photo. Photo admission preserves source priority and releases the next photo as soon as its R/G/B tasks are submitted. Any free worker takes the next layer, including the second blue task in the five-worker example. Compressed inputs remain borrowed through scoped jobs. Native photo coordinators handle reading, metadata, cache writes and optional preview preparation. Their count bounds retained inputs. Classic interleaved streams submit one sensor task. The pool is reused until its configured size changes. Linux background priority uses an absolute minimum niceness to avoid lowering inherited pool threads twice.

Decoded-source readiness is published immediately after each cache write. Optional camera-preview work follows that publication. Already-ready RAW hits skip camera-JPEG regeneration on every selection change, and queued work has no fixed polling delay between groups. The previous one-thread-per-photo Background-QoS path measured about 2.0–2.3 seconds per supplied Quattro original, versus about 522 ms for the active parallel-plane decode. Those measurements identified the first-photo latency tradeoff and motivated the shared layer queue.

Profiling also found that macOS Background QoS dominated decode time even after sharing planes. Speculative RAW tasks now use Utility QoS, while active loads remain User Initiated and ordinary maintenance/batch jobs remain Background. All speculative work retains foreground cancellation. On the supplied Quattro original, the real cache test published the current dot at about 564–613 ms, the forward neighbor at 798–878 ms, and the backward neighbor at 1026–1140 ms. Its four-photo window used a budget holding only three decodes. Both starting positions retained current/+1/-1, rejected the farther insertion, stopped without retrying, and reused exact sample bits. These are local filesystem-cache observations, not cold-card or cross-platform guarantees.

Workspace tests, strict Clippy, formatting, documentation-index and artwork checks pass. The deterministic five-worker check holds five tasks open, releases only the first red layer, and observes the second blue layer starting before the first photo completes. Native QoS checks verify active User Initiated, speculative Utility and ordinary Background classes. Real-file foreground interruption acknowledged cancellation in about 1.3 ms, while foreground decoding stayed near its isolated timing. Cached samples match the active decode bit for bit.

The rebuilt native Smart sequence uses isolated source, preferences and temporary cache directories. It completes with twelve supplied-original links, displays the saved three-behind/seven-ahead defaults, and only writes decoded entries from the first eight eligible positions. It closes its own window and leaves source sidecars untouched. The owner approved cleanup and committing this follow-up. Native validation on other platforms remains outside this slice.
