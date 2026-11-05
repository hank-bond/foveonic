---
summary: "Prepare, verify and publish the appearance, processing and responsiveness fixes as 0.7.1."
---

# Release 0.7.1

Follow [application releases](../docs/specs/releases.md) and the [release guide](../docs/releases.md). The owner reviewed the application fixes, requested grouped commits, then authorized pushing them and cutting a patch release.

- [x] Prepare version 0.7.1 and notes for all committed changes since 0.7.0.
- [ ] Run release checks and push the reviewed changes and annotated tag.
- [ ] Verify hosted signing, cleanup and macOS 15 startup checks.
- [ ] Download and verify the signed archive and exercise the native fixes.
- [ ] Publish as Latest and record the evidence.

The release includes the shared appearance fix, Process readiness and saved-decision acknowledgment, and the remembered sun/moon viewport background controls. It also includes the committed Merrill speckle, browser and Smart-preview responsiveness improvements. Shared display changes preserve the photograph's pixels while its surround changes.

Before version preparation, the workspace suite passed with 416 ordinary core tests. The final GUI suite passed 318 tests, with GPU-dependent tests kept opt-in. Strict all-target Clippy, formatting, documentation and artwork checks passed. An isolated native workflow verified Browse exports, Import copying and saved edits, Process becoming disabled after completion, and black/white surrounds with unchanged photograph pixels. Native capture also checked the fixed control group with a narrow viewport.

The versioned workspace suite, strict all-target Clippy, formatting, documentation and artwork checks pass. The local tagged bundle reports 0.7.1, arm64 and macOS 15.0 consistently in bundle and executable metadata. Its command-line startup succeeds.
