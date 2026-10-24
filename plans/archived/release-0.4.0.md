---
summary: "Prepare and verify the signed v0.4.0 draft with Merrill viewport-first Smart previews and managed display colors."
status: archived
---

# Release 0.4.0

The owner requested a release of the Merrill Smart improvements and color management. Follow [release requirements](../../docs/specs/releases.md) and [release guidance](../../docs/releases.md). Preserve the owner's existing release prose and leave the new introduction for draft review. CI creates a signed draft and does not publish it.

- [x] Review changes since v0.3.0 and select the next minor version.
- [x] Prepare version 0.4.0 and its changelog section.
- [x] Run workspace, documentation, artwork and tagged bundle checks.
- [x] Commit reviewed changes and push main and an annotated v0.4.0 tag.
- [x] Verify hosted signing, draft assets and Sequoia startup checks.
- [x] Download the signed archive and verify its checksum, signature, deployment target and isolated native behavior.
- [x] Present the verified draft for the owner's introduction and publication review.

The release includes the committed Merrill regional processing and complete-source cache work, together with the reviewed display integration. Display checks cover GPU color mapping, linear preview filtering, UI transparency and switching all four gamut choices without a RAW render. Cross-monitor and macOS 15 interactive GUI checks remain explicit acceptance work. The existing Sequoia CI job verifies signed executable startup only.

Local preparation passes 543 workspace tests, strict all-target Clippy, formatting, documentation-index checks, five artwork tests and release-note extraction. The tagged bundle reports 0.4.0 in both version fields. Its executable and bundle both target macOS 15.0, and the help command runs. The final integration smoke check completed all eight display checkpoints with the same photographic and thumbnail revision across every gamut choice.


## Signed draft verification

Release preparation is commit `97965c5`, tagged `v0.4.0`. The [hosted release run](https://github.com/hank-bond/foveonic/actions/runs/40131464189) passed workspace checks, Developer ID signing, signature verification, archive upload and credential cleanup. The same signed download passed its startup check on macOS 15.7.9. The draft remains unpublished for the owner's introduction and publication review, and the published 0.3.0 release remains Latest.

The downloaded `Foveonic-v0.4.0-macos-arm64.zip` matches SHA256SUMS and the hosted asset digest, with SHA-256 `688da6304777737b10b76b6f29ea192e4c5e2bc5cebaf5dcb2c475379175d990`. Developer ID certificate chain, hardened runtime, secure timestamp, bundle version 0.4.0 and executable/bundle deployment target 15.0 verify.

The signed app completed twelve isolated Merrill Smart checkpoints and eight display checkpoints. Smart delivered a regional Full image before its whole-frame continuation, preserved the completed Full revision on pan and reopened through the cache. Every display limit used the floating surface and retained the same photographic and thumbnail revision. The overview capture was visually inspected. The source original's hash stayed unchanged, no source sidecars were written and the downloaded app's signature remained valid after both launches. These CLI downloads do not verify browser quarantine, and native checks ran on the available development Mac rather than macOS 15 or multiple monitors.


## Fast/Draft replacement and publication

The owner requested one correction before publication: standalone Fast must honor Noise and Detail, while Smart may bypass it in its Draft overview. The [completed follow-up](./fast-preview-controls.md) records the reduced-noise geometry, GPU evidence and native checks. The still-unpublished tag moved to the final candidate `c879683`, and the [replacement hosted run](https://github.com/hank-bond/foveonic/actions/runs/40133008016) passed signing and Sequoia startup verification.

The replacement archive's SHA-256 is `22801e69d53b04c49c766c139139a192e6534c5f558767f712960a84e208e586`, superseding the initial draft checksum above. The signed app passes eight display, seven Fast and twelve Smart native checkpoints. After the owner saved introductory prose and authorized publication, [v0.4.0](https://github.com/hank-bond/foveonic/releases/tag/v0.4.0) was published as Latest. The release notes remain unchanged by publication. The published tag is fixed at `c879683`.
