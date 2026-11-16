---
summary: "Package the reviewed highlight optimization as 0.8.3 and record checks before owner-reviewed publication."
---

# Prepare 0.8.3

The owner requested production packaging of the reviewed [highlight optimization](./archived/highlight-donor-reuse.md). Follow the [release requirements](../docs/specs/releases.md), [release guide](../docs/releases.md) and [test inventory](../docs/testing-coverage.md). Scope is the donor-classification optimization, its regression test, equivalent Windows boot-key fallback cleanup, build-tool guidance and the version/changelog update. Publication is a separate owner decision.

- [x] Set the shared application version and lockfile entries to 0.8.3.
- [x] Prepare version-specific notes without claiming a general speedup or reduced memory use.
- [x] Independently review the complete source and packaging changes.
- [x] Obtain approval for the reviewed commit and version tag needed by signed release CI.
- [x] Build the signed macOS and unsigned Windows production ZIPs into a draft.
- [x] Verify checksums, signatures, minimum OS, versions and matching developer symbols.
- [x] Run native smoke against the actual extracted production packages on both test desktops.
- [x] Record remaining correctness and performance evidence.
- [ ] Complete publication review, including unresolved risks and unperformed checks.

The optimization already passed 144 exact-output cases on each of Metal and Windows DX12, ten targeted GPU tests per platform, ordinary core tests, Clippy and private native smoke. Those checks apply to the reviewed optimization sources and private artifacts. They do not establish that the future production archives pass native startup, broader acceptance or release performance comparisons.

The existing matched Mac measurements show lower HDR median edit latency and highlight GPU work, with ordinary default-workflow variance. Physical-footprint peaks remain uncertain: the high mode occurred in five of six candidate and two of nine unchanged-baseline trials. Do not infer memory neutrality. Remaining coverage includes exact production executables and representative enabled ColorField, HDR, native-detail and multi-photo workflows. Record unavailable comparisons and unresolved regressions for the owner before publication.

The historical hosted Windows parallel native access violation remains unresolved, as recorded in the [release regression plan](./gui-release-regressions.md). Serial gates passing does not establish a fix. The owner's prior as-is approval applied to 0.8.2. Any 0.8.3 publication with this concern unresolved requires a separate release-specific owner decision. This does not block draft packaging.

Use hosted CI for production signing and builds. Keep the primary Mac free of application builds and GPU measurements, reuse prepared remote build tools, and retain compact evidence instead of successful full-image dumps. Public downloads contain application archives and checksums. Matching symbols remain developer artifacts. Published 0.8.2 remains unchanged.

The owner explicitly approved committing, pushing main and creating/pushing v0.8.3 for draft builds. This authorization does not include publication.

## Draft validation

Tag `v0.8.3` identifies source `0a73c150f4ab2879770d0a245637d08686fa42ff`. All four jobs in [release CI](https://github.com/hank-bond/foveonic/actions/runs/43945594090) passed. The [draft](https://github.com/hank-bond/foveonic/releases/tag/untagged-f41dc10205e1cbc0158b) contains the two application ZIPs and `SHA256SUMS`. Final downloads match the exact archives tested on the remote desktops. Both native workspace smokes passed all ten checkpoints. Windows used its default 1 MiB stack and the pinned DXC DLLs. Matching developer symbols were preserved separately and verified against each executable. The Mac bundle is Developer ID signed with hardened runtime and a secure timestamp, targets macOS 15.0 and reports version 0.8.3.

[Versioned observations](../scripts/profiling/baselines/v0.8.3/apple-m1-pro/native-release-observations.json) preserve the production executable hashes, package checks, individual timing samples, strict workload identities, separate paired memory observations and coverage gaps. All 12 catalog comparisons, explicit HDR and ColorField recipes and two reversed confirmations matched. The earlier private HDR memory gap did not recur in production-package samples. The two initially suspicious latency medians were not stable in reversed controls. Ordinary variation remains acceptable, with no invented percentage budget or claim of general performance equivalence.

## Publication review remains open

The full native GUI acceptance catalog and 1,440-case slider matrix were not rerun for 0.8.3. Prior optimization evidence includes byte-identical output on 144 cases per backend and unchanged GUI source, but does not replace an explicit decision about the remaining full-suite release gate. Windows performance comparisons remain unavailable with the current POSIX monitor. Runtime Windows evidence is Windows 11 Home build 26200, with no Windows 10 trial. The macOS 15 CI job establishes signed binary loading rather than native graphics coverage. The historical parallel native access violation remains unresolved. Publication needs owner review of these gaps and a separate 0.8.3 release-risk decision. No publication has been authorized.
