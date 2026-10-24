---
summary: "Ship a macOS 15-targeted Apple Silicon build for Sequoia user testing while retaining explicit runtime coverage limits."
---

# Target Sequoia for release

The owner first requested a compatibility investigation, then approved including a macOS 15-targeted build in the still-unpublished 0.3.0 release for a user to try. The [release requirements](../docs/specs/releases.md) now select 15.0 for executable deployment, icon resources and bundle metadata. The build host remains macOS 26 with Xcode 26 or later. This authorization permits replacing the unpublished draft build while preserving its title and owner-written introduction. Published releases and tags remain unchanged.

## Checks

- [x] Locate explicit deployment and packaging restrictions.
- [x] Review the app's platform calls and the pinned Metal backend.
- [x] Build the current GUI and native icon resources for macOS 15.0 in an isolated output directory.
- [x] Check the executable's deployment command and launch the experimental bundle on the current macOS 26 host.
- [ ] Exercise the bundle on an Apple Silicon Mac running macOS 15.
- [x] Owner approved a macOS 15 runtime target for the release and requested a Sequoia user trial.
- [x] Align packaging, icon generation, CI deployment target and release documentation at 15.0.
- [x] Verify the final executable load command, bundle minimum and fallback icon resources.
- [x] Run local checks and native Smart/Import smoke tests on the available macOS 26 host.
- [ ] Verify the signed binary loads on a macOS 15 Apple Silicon runner.
- [ ] Commit and rebuild the unpublished 0.3.0 draft through signed CI, preserving the owner's prose.
- [ ] Verify the downloaded signed replacement and record exact testing limits.

## Findings

The original macOS 26 restriction was explicit in the Cargo task's deployment environment, icon compilation target, bundle minimum-system plist key, and release workflow. The original 0.3.0 draft's executable also recorded minimum macOS 26.0. Editing only the plist would not make that executable compatible with Sequoia.

The same GUI sources and locked dependencies compile successfully with `MACOSX_DEPLOYMENT_TARGET=15.0`. The experimental executable records minimum macOS 15.0 while using the macOS 26.5 SDK. Inspection of its linked frameworks and imported functions found no identified macOS 26-only dependency. Objective-C method calls and GPU driver behavior still require runtime validation. The pinned wgpu Metal backend chooses its language/capability path using runtime OS checks and supports versions older than Sequoia. App-owned background scheduling and cache boot detection do not introduce a macOS 26 API dependency.

Xcode's icon compiler accepts deployment target 15.0 for the existing Composer document and emits both `Assets.car` and `Foveonic.icns` with the usual compiler-owned icon metadata. These generated resources require an actual Sequoia icon/launch check. Building with Xcode 26 does not itself require users to run macOS 26. Apple's [Xcode deployment-target table](https://developer.apple.com/xcode/system-requirements) distinguishes the build-host requirements from supported app deployment targets. Apple's [Icon Composer guidance](https://developer.apple.com/documentation/xcode/creating-your-app-icon-using-icon-composer) describes compiler-generated icon images for earlier operating systems.

An experimental bundle with the 15.0 executable, 15.0 icon resources, and `LSMinimumSystemVersion=15.0` completed the isolated native import smoke sequence on the macOS 26 host, including rendering and copying an original without source-side writes. Its captures also verify the shared dropdown alignment and balanced section-header spacing. This is evidence for a viable Sequoia-targeted build, not a completed Sequoia runtime test.

## Reproduction

Build the separate executable without replacing the ordinary bundle:

```sh
MACOSX_DEPLOYMENT_TARGET=15.0 cargo build --offline --locked --release -p foveonic-gui --bin foveonic-gui --target-dir target/sequoia-compat
```

The normal packaging task now uses the same 15.0 target for executable, icons and Info.plist. CI also checks the actual Mach-O minimum before signing, catching a stale or incorrectly targeted executable even if the plist says 15.0. This packaging contract is distinct from runtime acceptance on a real Sequoia installation. That check stays open for the owner's user trial.

## Release preparation

The owner saved an introduction in the existing draft and approved publishing when ready. Its text is retained in the changelog and draft. The original unpublished 0.3.0 tag may move to the approved Sequoia preparation commit. Previously published tags remain fixed. The existing draft's compatibility footer is the only prose to change when its assets are replaced.

Local checks pass all 529 workspace tests, strict all-target Clippy, formatting, five artwork tests and documentation checks. The native executable's LC_BUILD_VERSION reports minos 15.0 with SDK 26.5. Bundle minimum is 15.0 and both compiled icon resources are present. The Sequoia CI guard passes against that actual build. Isolated Smart and Import smoke sequences pass on macOS 26 without source-side writes. GitHub lists `macos-15` as an [Apple Silicon runner](https://docs.github.com/en/actions/reference/runners/github-hosted-runners). A dependent CI job now downloads the signed draft and checks binary loading on that OS. This is a basic process-startup check, not a replacement for the user's GUI and rendering trial.
