---
summary: "Prepare and validate 0.8.4 with reviewed capture-metadata and TIFF interoperability fixes."
---

# Release 0.8.4

The owner requested another patch release after reviewing the [EXIF fixes](./exif-export.md). Follow the [release requirements](../docs/specs/releases.md), [release guide](../docs/releases.md) and [test inventory](../docs/testing-coverage.md). Scope is capture-metadata correctness, the pinned TIFF alignment fix, maintained export checks and their reviewed validator-warning decisions. Processing algorithms, encoded image samples and DNG profile values are unchanged.

- [x] Complete source review and targeted metadata, encoder and independent-reader checks.
- [x] Set the shared version and release notes to 0.8.4 and review the release diff.
- [ ] Run workspace, original-file/GPU, Python and native GUI correctness gates and record unavailable checks.
- [ ] Obtain the required release-action approvals and build production archives into a draft.
- [ ] Verify the exact packaged archives, matching developer symbols and native startup/rendering on both desktops.
- [ ] Compare packaged-app latency and memory with published 0.8.3 and review meaningful differences.
- [ ] Review remaining risks, publish the approved release and record package identities.

Existing evidence includes 645 active core tests, 35 maintained real-source exports, 33 encoder-option combinations and 125 warning-free Adobe SDK reads with exact decoded samples. The [shared EXIF checks](../crates/foveonic/tests/common/capture_exif.rs) own validator-warning decisions. Full release gates remain separate from that targeted evidence.

The historical hosted Windows parallel native access violation remains unresolved in the [release regression plan](./gui-release-regressions.md). Prior release-specific waivers do not establish a fix or approval for this release. Native Windows testing is Windows 11. Windows 10 and native Sequoia graphics coverage remain unavailable on the prepared owner hosts. Hosted Sequoia checks only establish signed binary loading.

The owner explicitly approved committing and pushing the reviewed changes to main and creating/pushing `v0.8.4` for draft production builds. This approval does not publish the draft. Sol reviewed the complete implementation and release preparation. Native/package checks, broader correctness gates and performance comparisons remain in progress.
