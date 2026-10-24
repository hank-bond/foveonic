---
summary: "Check whether the Apple Silicon app needs macOS 26 APIs or can target macOS 15 Sequoia."
---

# Audit Sequoia compatibility

The owner requested a compatibility investigation. The [current release requirements](../docs/specs/releases.md) still specify macOS 26. This audit does not change the supported release target.

## Checks

- [x] Locate explicit deployment and packaging restrictions.
- [x] Review the app's platform calls and the pinned Metal backend.
- [x] Build the current GUI and native icon resources for macOS 15.0 in an isolated output directory.
- [x] Check the executable's deployment command and launch the experimental bundle on the current macOS 26 host.
- [ ] Exercise the bundle on an Apple Silicon Mac running macOS 15.
- [ ] Agree a supported release minimum before changing packaging, CI, and release documentation together.

## Findings

The current macOS 26 restriction is explicit in the Cargo task's deployment environment, icon compilation target, bundle minimum-system plist key, and release workflow. The current packaged executable also records minimum macOS 26.0. Editing only the plist would not make that executable compatible with Sequoia.

The same GUI sources and locked dependencies compile successfully with `MACOSX_DEPLOYMENT_TARGET=15.0`. The experimental executable records minimum macOS 15.0 while using the macOS 26.5 SDK. Inspection of its linked frameworks and imported functions found no identified macOS 26-only dependency. Objective-C method calls and GPU driver behavior still require runtime validation. The pinned wgpu Metal backend chooses its language/capability path using runtime OS checks and supports versions older than Sequoia. App-owned background scheduling and cache boot detection do not introduce a macOS 26 API dependency.

Xcode's icon compiler accepts deployment target 15.0 for the existing Composer document and emits both `Assets.car` and `Foveonic.icns` with the usual compiler-owned icon metadata. These generated resources require an actual Sequoia icon/launch check. Building with Xcode 26 does not itself require users to run macOS 26. Apple's [Xcode deployment-target table](https://developer.apple.com/xcode/system-requirements) distinguishes the build-host requirements from supported app deployment targets.

An experimental bundle with the 15.0 executable, 15.0 icon resources, and `LSMinimumSystemVersion=15.0` completed the isolated native import smoke sequence on the macOS 26 host, including rendering and copying an original without source-side writes. Its captures also verify the shared dropdown alignment and balanced section-header spacing. This is evidence for a viable Sequoia-targeted build, not a completed Sequoia runtime test.

## Reproduction

Build the separate executable without replacing the ordinary bundle:

```sh
MACOSX_DEPLOYMENT_TARGET=15.0 cargo build --offline --locked --release -p foveonic-gui --bin foveonic-gui --target-dir target/sequoia-compat
```

Compile the existing Composer document with the packaging task's normal icon arguments, substituting minimum deployment target `15.0` and a separate output directory. Assemble a separate experimental bundle using that executable and those resources, preserving compiler icon metadata and setting minimum system version to `15.0`. Keep ordinary release defaults unchanged until the runtime check and support decision are complete.
