---
summary: "Load visible thumbnails first, greedily prepare the folder, and coordinate work behind the selected photo."
status: archived
---

# Prioritize browser thumbnails

Implemented the agreed [preview priorities](../../docs/specs/browser-previews.md), preserving [decode-cache](../../docs/specs/decode-cache.md) ownership and bounded neighbor prefetch.

- [x] Confirm selected-photo work comes before visible thumbnails, other thumbnails, batches and speculative work.
- [x] Separate small-thumbnail loading from larger cached previews and promote visible requests on scroll.
- [x] Queue the remaining folder without waiting for scrolling and keep replies and texture retention bounded.
- [x] Extend the existing priority gate while retaining foreground cancellation and allowing in-memory sensor tasks to finish.
- [x] Validate folder replacement, off-screen preparation, bounded retention, late replies and native priority behavior.
- [x] Run workspace checks and build the review app.

The existing worker mixed small previews and larger cached-preview work on one FIFO, only receiving thumbnail requests for visible rows. A cached-preview read lowered that same thread's scheduling priority. Dedicated workers remove that coupling. The shared gate now tracks selected-photo, thumbnail and submitted-processing demand. Sensor tasks already running in memory skip lower-priority waits while retaining foreground cancellation. No replacement render executor or generic task framework is introduced.

The greedy pass retains the existing 256-thumbnail texture bound and pins visible entries where that bound allows. It does not promise to retain every thumbnail in an arbitrarily large folder. Evicted thumbnails can be reread on demand without restarting the whole pass. Green cache status still requires the complete RAW bundle.

Final cleanup gives larger-preview lookup state a descriptive name, keeps preview misses from suppressing evicted thumbnails, and applies inherited sensor-task scheduling in one context transition. Admission pauses occur outside the shared photo-order lock, allowing admitted photos to release their turns.

Validation covers visible-request promotion, independent thumbnail and larger-preview workers, folder replacement during an active read, rejected late replies and preview misses, off-screen completion beyond the texture limit, selected-view retries, priority ordering, foreground interruption and native QoS restoration. All 576 ordinary workspace tests pass, along with strict all-target Clippy, formatting, documentation indexes, artwork tests and the original Merrill small-preview test. The macOS review bundle builds successfully. Native card latency and interactive feel remain owner review items.
