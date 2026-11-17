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

The initial export-only Windows executable built offline and passed its packaged `--help` check with exit code zero. Its isolated ZIP retained the release DXC files and license notices, and its SHA-256 matched after transfer. That preliminary archive was superseded by the startup correction below and must not be used as the final 0.8.2 candidate. The owner approved shipping both corrections as patch release 0.8.2.

## Reported 0.8.1 startup regression

The same reporter could start 0.8.0 on Windows 10 but reported that 0.8.1 would not start. No error text or loading-window behavior is available. The owner requested inspection of patch changes without arranging a Windows 10 installation.

The published Windows executables import the same DLLs and functions, retain identical PE operating-system and subsystem version fields, and reserve the same 1 MiB main-thread stack. Their packaged `dxcompiler.dll` and `dxil.dll` are byte-for-byte identical. The tag diff does not upgrade wgpu, eframe or their transitive dependencies, change the Windows build workflow, or change DX12/DXC selection. This comparison found no newly imported Windows API or compiler package establishing a Windows 11 requirement.

Initial patch inspection identified the [loading-window handoff](./archived/startup-progress.md) as the principal startup change. Photographic preparation waits for the first root presentation, then compiles pipelines on a worker while the UI continues using the shared GPU device. Driver-sensitive behavior was an unconfirmed early hypothesis. The presentation callback is installed even with diagnostics disabled. Ordinary preparation failures appear in the loading window. The later interactive comparison below established the stack-overflow cause and a verified correction.

Both published versions fail with the same `Surface::configure: Invalid surface` error when launched through the noninteractive SSH session on the Windows 11 test host. This launch path cannot distinguish the reported regression and is not evidence that 0.8.1 fails on an interactive Windows desktop.

### Interactive reproduction and correction

Running the exact published archives through the logged-on user's interactive desktop distinguishes the regression. Version 0.8.0 completes all ten workspace smoke captures with exit code zero. Version 0.8.1 produces no captures and exits with `0xC00000FD` and a main-thread stack-overflow message. This reproduces on Windows 11 and does not establish a Windows 10-specific compatibility change.

The startup channel previously transported `Result<App, String>` by value and boxed the completed application only after the UI received it. The corrected worker boxes the application before sending `Result<Box<App>, String>`. The UI now receives heap-owned application state without large by-value handoff temporaries on its default stack. A clean Windows release build with this three-line ownership change completes all ten native captures with exit code zero. Independent review found no blocking design or Rust issues.

The release workflow already ran GUI unit tests, but packaged `--help` checks returned before graphics and the handoff. The owner approved including both export and startup corrections in 0.8.2, updating the unpublished tag, and strengthening pre-release instructions. The maintained packaged-GUI verifier now requires process success and every native smoke checkpoint. Full acceptance and missing-input reports remain separate gates. A small Windows 11 desktop with integrated graphics is suitable for future correctness smoke checks without being a performance test host.

### Additional release gates

The broadened GPU run observed Vulkan in the old headless defaults and DX12/DXC in the packaged GUI. A hosted Windows workspace run instead selected DX12/FXC and rejected an existing shader during compilation. The shared platform descriptor now aligns Windows headless processing and tests with the shipped configuration. The common setup verifies the same pinned compiler archive and stages both DLLs beside application and test executables. Windows CI prepares that dependency before workspace tests and reuses its verified archive when packaging.

The first revised draft CI run passed macOS workspace/Python checks but caught an unused reader method after cache recency moved to a separate write handle. The method was removed, retaining the cursor-position test. Windows failed the default-FXC path described above. Neither failure uploaded a revised release archive. Final CI and exact packaged desktop checks remain pending.
