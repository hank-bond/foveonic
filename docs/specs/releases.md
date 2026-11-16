---
summary: "Release tags build signed macOS and unsigned Windows archives into draft GitHub Releases without publishing them."
kind: reference
status: accepted
implementation: incomplete
---

# Application releases

## Requirements

- Local macOS packaging and launching **must** use a Rust workspace task exposed through Cargo. Normal builds **must not** require Python or regenerate checked-in artwork.
- Packaging **must** preserve native icon appearances where supported, compiler-provided icon metadata, the fallback ICNS for earlier systems, executable permissions, stable application identity, and macOS 15.0 minimum. Executable deployment target, icon compilation and bundle minimum must agree. It **must** use Cargo's reported executable path, including custom target directories.
- Application crates and bundle metadata **must** share one version. Release tags **must** use `v` followed by that version. A mismatched tag **must** fail before release assets are uploaded.
- Pushing a release tag **must** trigger GitHub Actions checks and an Apple Silicon macOS app build with Xcode 26 or later. GPU/original-file fixtures **must** remain opt-in.
- Release CI **must** sign the app with a valid Developer ID Application identity for the configured team, enable hardened runtime and obtain a secure timestamp. Local packaging **must not** require signing credentials.
- Release CI **must** verify the app's signature before creating the archive and checksum. The archive **must** preserve the app bundle and executable permissions. Release CI **must not** submit the app for notarization or require Gatekeeper acceptance.
- A macOS 15 Apple Silicon job must download the signed draft, verify its checksum/signature and run the executable's help command. This checks binary loading on Sequoia. Interactive GUI, Metal rendering and browser-quarantine acceptance remain separate runtime checks.
- Before publication, the actual packaged macOS and Windows executables must pass native GUI smoke checks on a desktop with working graphics. Require successful process exit and every expected workspace checkpoint. A help-command check or headless unit test must not substitute for native startup and rendering. Record the tested executable hash and unavailable checks.
- Before publication, each release must compare latency and memory against the previous published app on matched hardware and reproducible workloads, following the [profiling requirements](./profiling.md#release-performance-baselines). Correctness checks and instrumented trace durations do not substitute for these comparisons. Missing comparisons and unresolved regressions require an explicit release-specific owner decision.
- Missing credentials, ambiguous signing identities, or signing and signature-verification failures **must** block release uploads. There **must not** be an unsigned fallback.
- Release guidance **must** explain that downloads are not notarized and may require the per-app **Open Anyway** control in macOS Privacy & Security settings on first launch. It **must not** recommend disabling Gatekeeper globally.
- Signing credentials **must** come from GitHub Actions secrets. The Team ID **may** be a repository variable. Certificate exports and imported private keys **must** remain in temporary runner storage and be cleaned up on success, failure or cancellation.
- Release tags **must** also build an unsigned Windows x64 archive containing the GUI executable, pinned DirectX shader compiler DLLs and their license notices. Its app **must** use DirectX 12 with DXC. CI **must** verify the downloaded compiler archive before packaging.
- Successful release builds **must** attach both platform archives and their SHA-256 checksums to a draft GitHub Release. Automation **must not** publish releases or change an existing release's prose. Uploads to an already published release **must** fail.
- Public application archives **must** exclude developer PDB/dSYM files. Matching symbols **must** be retained as developer artifacts with verifiable build identities, separately from public release downloads. Public checksums cover application archives only.
- The checked-in changelog **must** supply the initial draft notes for the tagged version, with space for the owner to add introductory prose. The owner reviews prose and publishes the release.

## Release scope

Release assets include Apple Silicon ZIP archives targeting macOS 15 Sequoia or later and unsigned Windows x64 ZIP archives. Building uses Xcode 26 or later, independently of the application's deployment target. The owner approved including Sequoia-targeted builds in 0.3.0 for a user trial. Interactive GUI, Metal rendering and icon checks on macOS 15 remain outstanding and must not be described as completed by the CI process-startup check. Intel builds and installers are outside the current scope. Creating or pushing a tag and publishing its release remain separate owner-approved actions. Previously published releases remain unchanged.

## Validation

Local packaging, archive round-trip checks, task/workspace tests, and mocked draft-release handling pass as recorded in the [preparation plan](../../plans/archived/cargo-releases.md). The first [hosted release run](https://github.com/hank-bond/foveonic/actions/runs/38832699996) passed checks, built and archived the app, and uploaded assets to a draft. The [v0.1.0 release](https://github.com/hank-bond/foveonic/releases/tag/v0.1.0) is now published with the Apple Silicon archive and checksums. That initial release was unsigned and unnotarized.

The [signed-release plan](../../plans/archived/signed-releases.md) tracks the signing migration. The [0.2.0 hosted run](https://github.com/hank-bond/foveonic/actions/runs/39973024540) passed signing-only draft creation and credential cleanup. The downloaded archive checksum, Developer ID signature, hardened runtime, secure timestamp, bundle version and architecture verify. Its isolated native GUI smoke test passes without changing local photographs or sidecars. The owner approved publishing [v0.2.0](https://github.com/hank-bond/foveonic/releases/tag/v0.2.0) as Latest. Browser-quarantine override testing was not automated.
