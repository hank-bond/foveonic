---
summary: "Prepare, verify and publish the 0.7.2 Smart preview, Merrill editing and licensing release."
status: archived
---

# Release 0.7.2

The [release requirements](../../docs/specs/releases.md) and [release guide](../../docs/releases.md) define the release process. Version 0.7.2 covers changes since 0.7.1. The owner reviewed the notes and approved publication after verification.

- [x] Prepare and review version, notes and release commit.
- [x] Run workspace, documentation, artwork, release notes and local bundle checks.
- [x] Push the approved tag and verify the hosted signed draft and macOS 15 startup job.
- [x] Verify downloaded assets and native Smart preview checks.
- [x] Publish the verified release as Latest.

Release preparation commit `703575c` and annotated tag `v0.7.2` were pushed. Workspace tests, strict Clippy, formatting, documentation indexes, artwork tests and release-note extraction passed. The local tagged bundle reported version 0.7.2, arm64 and a macOS 15.0 minimum. Local packaging did not install or launch it.

Both jobs in the [hosted release workflow](https://github.com/hank-bond/foveonic/actions/runs/42053972847) passed, including signed app startup on macOS 15. The downloaded archive matched SHA256SUMS with SHA-256 `c44905e034a5bd86889d371700d4122d56b2cf0359ce94b644990f386f8c868f`. Strict Developer ID signature verification passed for team `UBLDN6TFD6`, with hardened runtime and a secure timestamp. Bundle and executable report version 0.7.2, arm64 and macOS 15.0 minimum. Dynamic dependencies are system libraries only, and command-line help runs.

The downloaded signed app completed isolated native Smart preview smoke sequences on Merrill and Quattro originals on macOS 26.6.2, with separate preferences and sidecar access disabled. These checks exercised viewport detail, editing, crop transitions, Adaptive refinement and cache controls. They do not establish native Metal rendering or interactive GUI behavior on macOS 15, or browser-quarantine acceptance.

The owner approved and published [v0.7.2](https://github.com/hank-bond/foveonic/releases/tag/v0.7.2) as Latest. The existing draft title and prose were preserved.
