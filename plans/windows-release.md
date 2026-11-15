---
summary: "Build and validate a Windows x64 test archive for the current release branch."
---

# Windows release test

The owner requested a CI build of the current release branch and testing its artifact on the Windows machine. The existing v0.8.0 draft uses an older tagged commit and is not changed by this test.

- [x] Establish the Windows compiler requirements: LLVM/Clang, CMake, Windows SDK and a current DXC compiler.
- [x] Upgrade wgpu and egui in the local migration and rebase the display patch on the matching upstream renderer.
- [x] Add a Windows test-artifact workflow for release-branch pushes and manual dispatch with a pinned, verified DXC download.
- [x] Finish migration checks, including strict Clippy and the macOS native-preview display test.
- [ ] Run the workflow from a reviewed commit and test the downloaded artifact on the Windows machine.

An exploratory Windows build using wgpu 25, current DXC and the original processing shaders opened release photographs on the owner's machine. The old SDK's DXC failed device creation because it did not support the requested shader model. FXC compiled very slowly and rejected valid shaders. The experiment predates the current release branch and is not the CI artifact.

The Mac workspace tests, strict Clippy, formatting, documentation index checks and the native-preview gamut and linear-filtering GPU test pass. The egui upgrade required popup lifetime, pointer-fixture and rounded button-padding updates. A GUI debug fixture needs an 8 MiB test-thread stack; release CI scopes that allowance to its test step. Windows all-target checking passes, and a current-branch release executable builds and passes --help when packaged with the pinned DXC files. A locally packaged copy was launched for owner testing. No CI artifact has been produced or tested. The Windows archive test does not publish or modify a GitHub Release.
