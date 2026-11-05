---
summary: "Record verified 0.7.1 publication with the appearance, processing and responsiveness fixes."
status: archived
---

# Release 0.7.1

The release followed [application releases](../../docs/specs/releases.md) and the [release guide](../../docs/releases.md). The owner reviewed the application fixes, requested grouped commits, then authorized pushing them and cutting a patch release.

- [x] Prepare version 0.7.1 and notes for all committed changes since 0.7.0.
- [x] Run release checks and push the reviewed changes and annotated tag.
- [x] Verify hosted signing, cleanup and macOS 15 startup checks.
- [x] Download and verify the signed archive and exercise the native fixes.
- [x] Publish as Latest and record the evidence.

The release includes the shared appearance fix, Process readiness and saved-decision acknowledgment, and the remembered sun/moon viewport background controls. It also includes the committed Merrill speckle, browser and Smart-preview responsiveness improvements. Shared display changes preserve the photograph's pixels while its surround changes.

Before version preparation, the workspace suite passed with 416 ordinary core tests. The final GUI suite passed 318 tests, with GPU-dependent tests kept opt-in. Strict all-target Clippy, formatting, documentation and artwork checks passed. An isolated native workflow verified Browse exports, Import copying and saved edits, Process becoming disabled after completion, and black/white surrounds with unchanged photograph pixels. Native capture also checked the fixed control group with a narrow viewport.

The versioned workspace suite, strict all-target Clippy, formatting, documentation and artwork checks pass. The local tagged bundle reports 0.7.1, arm64 and macOS 15.0 consistently in bundle and executable metadata. Its command-line startup succeeds.

Release preparation commit `51bfad4` and annotated tag `v0.7.1` are pushed. Main and the integration branch contain that commit. Hosted checks run in [the release workflow](https://github.com/hank-bond/foveonic/actions/runs/42034676673).

Both hosted jobs passed, including Developer ID signing, signature verification, credential cleanup and signed-app startup on macOS 15. The downloaded archive matches SHA256SUMS and GitHub's asset digest. Its signature, Team ID, hardened runtime, secure timestamp, 0.7.1 version, arm64 architecture and macOS 15.0 deployment metadata verify. It links only system libraries, and command-line startup succeeds. The archive SHA-256 is `40c1d7e61d74b596d40491b4557e4aaa8ff4d3ad25db171a9f4e57733cb6257b`.

An isolated native workflow of the downloaded signed app passed Browse export, Import copying and destination edits, and the narrow viewport controls. The copied original matches its source bytes. Captures confirm black and white surrounds with identical photograph pixels, and Process becomes disabled after the import and hidden decisions are saved. Source sidecar writes remained disabled during this run. Interactive macOS 15 rendering and browser-quarantine acceptance remain separate from the hosted startup check.

[Foveonic v0.7.1](https://github.com/hank-bond/foveonic/releases/tag/v0.7.1) was published as Latest on November 5, 2026 with owner approval. GitHub confirms a public, non-prerelease release with the verified archive and checksums. Publication preserved the title, notes and asset identities.
