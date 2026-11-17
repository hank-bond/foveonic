---
summary: "Ship 0.8.3 with reviewed highlight optimization, verified packages, owner-approved publication and recorded coverage gaps."
status: archived
---

# Release 0.8.3

The owner requested production packaging of the reviewed [highlight optimization](./highlight-donor-reuse.md). The work followed the [release requirements](../../docs/specs/releases.md), [release guide](../../docs/releases.md) and [test inventory](../../docs/testing-coverage.md). Scope is the donor-classification optimization, its regression test, equivalent Windows boot-key fallback cleanup, build-tool guidance and the version/changelog update. The owner approved draft preparation and publication separately.

- [x] Set the shared application version and lockfile entries to 0.8.3.
- [x] Prepare version-specific notes without claiming a general speedup or reduced memory use.
- [x] Independently review the complete source and packaging changes.
- [x] Obtain approval for the reviewed commit and version tag needed by signed release CI.
- [x] Build the signed macOS and unsigned Windows production ZIPs into a draft.
- [x] Verify checksums, signatures, minimum OS, versions and matching developer symbols.
- [x] Run native smoke against the actual extracted production packages on both test desktops.
- [x] Record remaining correctness and performance evidence.
- [x] Complete publication review, including unresolved risks and unperformed checks.

The optimization already passed 144 exact-output cases on each of Metal and Windows DX12, ten targeted GPU tests per platform, ordinary core tests, Clippy and private native smoke. Those checks apply to the reviewed optimization sources and private artifacts. Actual production-package evidence is recorded below. These earlier checks did not replace the full broader GUI acceptance gate.

The initial private matched Mac measurements showed lower HDR median edit latency and highlight GPU work, with ordinary default-workflow variance. Initial physical-footprint peaks were uncertain: the high mode occurred in five of six candidate and two of nine unchanged-baseline trials. Those trials did not establish memory neutrality. Subsequent production-package comparisons covered explicit ColorField, HDR, native-detail and multi-photo workflows, as recorded below.

The historical hosted Windows parallel native access violation remains unresolved, as recorded in the [release regression plan](../gui-release-regressions.md). Serial gates passing does not establish a fix. The owner's prior as-is approval applied to 0.8.2. The owner later approved 0.8.3 publication with this concern still unresolved.

Hosted CI produced the signed Mac and unsigned Windows packages. Tests reused the prepared remote hosts and build tools, keeping the primary Mac free of application builds and GPU measurements. Compact records were retained in place of successful full-image dumps. Public downloads contain application archives and checksums. Matching symbols remain developer artifacts. Published 0.8.2 remains unchanged.

The owner explicitly approved committing, pushing main and creating/pushing v0.8.3 for draft builds. The initial authorization covered draft preparation. The owner gave separate publication approval afterward.

## Draft validation

Tag `v0.8.3` identifies source `0a73c150f4ab2879770d0a245637d08686fa42ff`. All four jobs in [release CI](https://github.com/hank-bond/foveonic/actions/runs/43945594090) passed. The [release](https://github.com/hank-bond/foveonic/releases/tag/v0.8.3) contains the two application ZIPs and `SHA256SUMS`. Final downloads match the exact archives tested on the remote desktops. Both native workspace smokes passed all ten checkpoints. Windows used its default 1 MiB stack and the pinned DXC DLLs. Matching developer symbols were preserved separately and verified against each executable. The Mac bundle is Developer ID signed with hardened runtime and a secure timestamp, targets macOS 15.0 and reports version 0.8.3.

[Versioned observations](../../scripts/profiling/baselines/v0.8.3/apple-m1-pro/native-release-observations.json) preserve the production executable hashes, package checks, individual timing samples, strict workload identities, separate paired memory observations and coverage gaps. All 12 catalog comparisons, explicit HDR and ColorField recipes and two reversed confirmations matched. The earlier private HDR memory gap did not recur in production-package samples. The two initially suspicious latency medians were not stable in reversed controls. Ordinary variation remains acceptable, with no invented percentage budget or claim of general performance equivalence.

## Publication and remaining coverage

The full native GUI acceptance catalog and 1,440-case slider matrix were not rerun for 0.8.3. Prior optimization evidence includes byte-identical output on 144 cases per backend and unchanged GUI source, but does not replace an explicit decision about the remaining full-suite release gate. Windows performance comparisons remain unavailable with the current POSIX monitor. Runtime Windows evidence is Windows 11 Home build 26200, with no Windows 10 trial. The macOS 15 CI job establishes signed binary loading rather than native graphics coverage. The historical parallel native access violation remains unresolved. After being shown the full-suite gap and unresolved parallel-crash concern, the owner explicitly requested publication of this reviewed release. That authorization does not establish a fix or completion of the unperformed checks.

Published [v0.8.3](https://github.com/hank-bond/foveonic/releases/tag/v0.8.3) as Latest at 2026-11-16 20:53:52 UTC. Before publication, the fixed tag, successful CI source and all three asset digests were checked again against the reviewed validation record. GitHub confirmed the release is published, stable and Latest. The archives, checksums and release notes were unchanged. The versioned observations remain the immutable pre-publication draft-validation snapshot.
