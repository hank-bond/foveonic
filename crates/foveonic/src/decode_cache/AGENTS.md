---
summary: "The decoded-source cache groups temporary bundles, shared disk eviction and bounded background prefetch."
---

# Decoded source cache

Follow the [cache requirements](../../../../docs/specs/decode-cache.md). The public cache API and shared identity live in `mod.rs`. Private children handle bundle assets, retention and background scheduling. Preserve stored samples, metadata and cache compatibility when reorganizing these files.

The shared sensor-layer pool stays in `../decode_pool.rs` because X3F decoding also uses it. Foreground priority and cancellation belong to `../background.rs`. Cache workers must use that common policy.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[lru.rs](./lru.rs)** Applies the shared disk budget while favoring nearby source bundles and display previews.
- **[mod.rs](./mod.rs)** Reuses decoded sources, original bytes, selection metadata and previews in a bounded temporary disk LRU.
- **[original.rs](./original.rs)** Retains original X3F bytes for imports under the decoded cache's identity and budget.
- **[previews.rs](./previews.rs)** Stores bounded display previews under the decoded cache's identity, recipe, and shared LRU budget.
- **[selection.rs](./selection.rs)** Keeps complete local selection metadata and thumbnails beside decoded sources.
- **[window.rs](./window.rs)** Orders a bounded selection neighborhood for prefetch and cache retention.
- **[worker.rs](./worker.rs)** Schedules bounded cache writes, selection prefetch, and readiness scans behind foreground work.

<!-- INDEX:END -->
