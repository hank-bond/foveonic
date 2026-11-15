---
summary: "Build and validate a Windows x64 test archive for the current release branch."
---

# Windows release test

The owner requested a CI build of the current release branch and testing its artifact on the Windows machine. The existing v0.8.0 draft uses an older tagged commit and is not changed by this test.

- [x] Establish the Windows compiler requirements: LLVM/Clang, CMake, Windows SDK and a current DXC compiler.
- [x] Upgrade wgpu and egui in the local migration and rebase the display patch on the matching upstream renderer.
- [x] Add a Windows test-artifact workflow for release-branch pushes and manual dispatch with a pinned, verified DXC download.
- [x] Finish migration checks, including strict Clippy and the macOS native-preview display test.
- [x] Run the test workflow from a reviewed commit and test the downloaded artifact on the Windows machine.
- [ ] Review and validate the tag-triggered Windows draft upload alongside the macOS release.

An exploratory Windows build using wgpu 25, current DXC and the original processing shaders opened release photographs on the owner's machine. The old SDK's DXC failed device creation because it did not support the requested shader model. FXC compiled very slowly and rejected valid shaders. The experiment predates the current release branch and is not the CI artifact.

The Mac workspace tests, strict Clippy, formatting, documentation index checks and the native-preview gamut and linear-filtering GPU test pass. The egui upgrade required popup lifetime, pointer-fixture and rounded button-padding updates. A GUI debug fixture needs an 8 MiB test-thread stack; release CI scopes that allowance to its test step. Windows all-target checking passes, and a current-branch release executable builds and passes --help when packaged with the pinned DXC files. A locally packaged copy was launched for owner testing. The Windows archive test does not publish or modify a GitHub Release.

The branch workflow built commit `b1cf3e2` in [run 43676201644](https://github.com/hank-bond/foveonic/actions/runs/43676201644). The downloaded archive matched its SHA-256 after transfer to the Windows machine. Its executable passed --help and completed the ten-capture native GUI smoke on an isolated copy of blue-parking-sign.x3f. The final capture was inspected. The existing v0.8.0 draft and tag were not changed.

The tag workflow now reuses the Windows build, waits for the Mac draft, and attaches the Windows ZIP with a two-archive checksum manifest. Sequoia validates the Mac checksum after the manifest is complete. Local mocked checks cover successful upload, rejection of an already-published release and rejection when the tag no longer names the run commit. Hosted tag-workflow verification is still pending.

The owner requested a normal Windows application without the initial console window. The Windows build now uses the GUI subsystem and shows startup errors in a native dialog. The rebuilt executable was verified as GUI subsystem 2, its help process returned zero, and an intentionally invalid startup configuration produced the dialog and exit code 1. The owner observed the app open normally.

The first tag-triggered release run passed its Windows job and workspace tests but macOS Clippy rejected a Rust 1.100 deprecation and a newly flagged test loop. The deprecation allowance retains the workspace minimum Rust version, and the test iteration was made explicit. The reviewed fixes pass the Mac workspace tests and strict Clippy. A Windows Rust 1.100 Clippy check also passes with an unrelated Windows-only cache fallback lint explicitly excluded from that diagnostic run. No new draft assets were uploaded by the failed run.

## Windows export failure

The owner approved fixing [issue #1](https://github.com/hank-bond/foveonic/issues/1) and testing on the Windows machine. The [output publication requirements](../docs/specs/import-workflow.md#interaction) apply to both platforms.

- [x] Reproduce the failed file operation on Windows and identify its access requirement.
- [x] Open the completed temporary output with write access before syncing it.
- [x] Review the correction independently for design, cleanliness and idiomatic Rust.
- [x] Validate existing publication tests on Windows and photographic export through the shared wrapper.
- [x] Make the correction available for owner testing.

The shared publication helper reopened each completed output through `File::open` and then called `sync_all`. A synthetic Windows probe reproduced error 5 on that read-only handle in a writable temporary folder. Opening the same file with write access allowed syncing and renaming. The correction grants write access without creating or truncating the completed file. The handle closes before publication. Existing synthetic tests cover both conflict policies, failed rendering, collisions, destination preservation and temporary cleanup.

All seven maintained `importing::tests` pass on Windows with the corrected helper. A disposable headless caller exercised the GUI's existing `export::write` on an RTX 3090 through DX12 and the packaged DXC. It loaded the pinned example `blue-parking-sign.x3f`, applied +0.5 EV and 96 × 64 output sizing, and published a TIFF twice under Overwrite and once under Rename. The TIFF decoder independently read each result as 96 × 64 RGB16 with nonzero samples. Only the two published files remained. This checks real photographic export without claiming native Windows UI coverage.

The Mac workspace passes 1,195 ordinary Rust tests and strict all-target Clippy. The existing GPU copied-original test and frozen multi-format batch/export test pass separately. Formatting, all three Python test groups and documentation checks pass. Independent code and documentation reviews found no blocking issues.

A Windows release executable builds offline and passes its packaged `--help` check with exit code zero. An isolated ZIP includes the executable, unchanged release DXC files and license notices. Its SHA-256 matches after transfer back to the Mac workspace. The archive was prepared before release approval and is available for owner testing. Interactive Windows GUI validation remains pending. The owner subsequently approved shipping the correction as patch release 0.8.2.

## Reported 0.8.1 startup regression

The same reporter could start 0.8.0 on Windows 10 but reported that 0.8.1 would not start. No error text or loading-window behavior is available. The owner requested inspection of patch changes without arranging a Windows 10 installation.

The published Windows executables import the same DLLs and functions, retain identical PE operating-system and subsystem version fields, and reserve the same 1 MiB main-thread stack. Their packaged `dxcompiler.dll` and `dxil.dll` are byte-for-byte identical. The tag diff does not upgrade wgpu, eframe or their transitive dependencies, change the Windows build workflow, or change DX12/DXC selection. This comparison found no newly imported Windows API or compiler package establishing a Windows 11 requirement.

The principal startup change is the [loading-window handoff](./archived/startup-progress.md). Photographic preparation now waits for the first root presentation, then compiles pipelines on a worker while the UI continues using the shared GPU device. These are plausible presentation/driver-sensitive failure points, not a confirmed Windows 10 diagnosis. The presentation callback is installed even with diagnostics disabled. Ordinary preparation failures appear in the loading window. There is no evidence yet supporting a startup code correction.

Both published versions fail with the same `Surface::configure: Invalid surface` error when launched through the noninteractive SSH session on the Windows 11 test host. This launch path cannot distinguish the reported regression and is not evidence that 0.8.1 fails on an interactive Windows desktop.
