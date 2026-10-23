---
status: archived
summary: "Replace the provisional cache checkmark with muted caption dots backed by explicit queue and disk states."
---

# Cache status dots

The owner requested quieter dots beside filenames, outside the photograph. The [cache specification](../../docs/specs/decode-cache.md) records the color meanings.

- [x] Move grid status into the bottom-right caption area and reserve filename space in both organizer layouts.
- [x] Show gray for disabled/unavailable support, yellow for queued work, green for ready and red for supported but uncached data.
- [x] Report states from the background cache, retaining architecture eligibility by source identity and sharing immutable UI snapshots.
- [x] Verify queue, disk presence, eviction and disabled-policy transitions without changing photo processing.
- [x] Inspect native grid/list screenshots, rebuild the app and finish workspace/documentation checks.

Clearing, eviction, insertion failure and an entry exceeding the budget can produce the supported-but-uncached state. Unknown eligibility remains yellow until background metadata checking resolves it. Hover text distinguishes disabled, unsupported and unreadable sources. Readiness classification and source-file access remain off the UI thread.

Validation passed 472 workspace tests, strict all-target Clippy, formatting and documentation checks. An original-file prefetch check confirms ready Quattro/Merrill and disabled Classic states. Native grid and list captures confirm muted dots occupy the filename area. The local app was rebuilt. Changes remain uncommitted for owner review.
