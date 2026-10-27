---
summary: "Gather decoded-cache implementation and tests into a directory module for release 0.5.0."
status: archived
---

# Organize decode cache

Requirements: [decoded source cache](../../docs/specs/decode-cache.md).

- [x] Move the cache root, private children and adjacent tests into `decode_cache/`.
- [x] Preserve the public API, module visibility, storage format and scheduling behavior.
- [x] Add module guidance and update release notes and source indexes.
- [x] Run repository checks and rebuild the optimized release review bundle.

The owner approved this organizational change for 0.5.0. The layer pool remains separate because both X3F decoding and cache scheduling use it. No codec, numerical processing or cache-policy changes are included.

All ten moved source and test files match their previous contents except for module path declarations. The public module name and private child names are unchanged. Cache format versions, identities and storage locations remain unchanged.

Workspace tests, strict Clippy, formatting, documentation indexes and artwork checks pass. The optimized 0.5.0 review bundle builds. Original-image comparisons from the release cleanup remain applicable because this slice only changes source locations. Commit and publication remain pending owner review.
