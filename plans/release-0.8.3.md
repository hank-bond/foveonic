---
summary: "Package the reviewed highlight optimization as 0.8.3 and record checks before owner-reviewed publication."
---

# Prepare 0.8.3

The owner requested production packaging of the reviewed [highlight optimization](./archived/highlight-donor-reuse.md). Follow the [release requirements](../docs/specs/releases.md), [release guide](../docs/releases.md) and [test inventory](../docs/testing-coverage.md). Scope is the donor-classification optimization, its regression test, equivalent Windows boot-key fallback cleanup, build-tool guidance and the version/changelog update. Publication is a separate owner decision.

- [x] Set the shared application version and lockfile entries to 0.8.3.
- [x] Prepare version-specific notes without claiming a general speedup or reduced memory use.
- [x] Independently review the complete source and packaging changes.
- [x] Obtain approval for the reviewed commit and version tag needed by signed release CI.
- [ ] Build the signed macOS and unsigned Windows production ZIPs into a draft.
- [ ] Verify checksums, signatures, minimum OS, versions and matching developer symbols.
- [ ] Run native smoke against the actual extracted production packages on both test desktops.
- [ ] Record remaining correctness and performance evidence and publication decisions.

The optimization already passed 144 exact-output cases on each of Metal and Windows DX12, ten targeted GPU tests per platform, ordinary core tests, Clippy and private native smoke. Those checks apply to the reviewed optimization sources and private artifacts. They do not establish that the future production archives pass native startup, broader acceptance or release performance comparisons.

The existing matched Mac measurements show lower HDR median edit latency and highlight GPU work, with ordinary default-workflow variance. Physical-footprint peaks remain uncertain: the high mode occurred in five of six candidate and two of nine unchanged-baseline trials. Do not infer memory neutrality. Remaining coverage includes exact production executables and representative enabled ColorField, HDR, native-detail and multi-photo workflows. Record unavailable comparisons and unresolved regressions for the owner before publication.

The historical hosted Windows parallel native access violation remains unresolved, as recorded in the [release regression plan](./gui-release-regressions.md). Serial gates passing does not establish a fix. The owner's prior as-is approval applied to 0.8.2. Any 0.8.3 publication with this concern unresolved requires a separate release-specific owner decision. This does not block draft packaging.

Use hosted CI for production signing and builds. Keep the primary Mac free of application builds and GPU measurements, reuse prepared remote build tools, and retain compact evidence instead of successful full-image dumps. Public downloads contain application archives and checksums. Matching symbols remain developer artifacts. Published 0.8.2 remains unchanged.

The owner explicitly approved committing, pushing main and creating/pushing v0.8.3 for draft builds. This authorization does not include publication.
