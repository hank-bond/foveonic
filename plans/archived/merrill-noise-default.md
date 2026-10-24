---
summary: "Enable Noise and Detail for newly opened Merrill photographs while preserving saved off choices."
status: archived
---

# Merrill Noise and Detail default

The owner expects Noise and Detail enabled by default, with Fast bypassing the work rather than changing the saved checkbox. Requirements: [detail and noise](../../docs/specs/detail-and-noise.md), [Merrill Fast](../../docs/specs/merrill-fast-preview.md) and [editing sidecars](../../docs/specs/editing-sidecars.md).

- [x] Initialize fresh Merrill controls through the existing processing-owned capture defaults.
- [x] Preserve shared serialization defaults and explicit saved off choices.
- [x] Verify native Smart starts with Noise and Detail enabled without test setup changing it.
- [x] Check no writes from viewing or quality switches, edit persistence and saved-off reopening.
- [x] Run workspace, strict Clippy, formatting, documentation and artwork checks and rebuild the app.

The previous fresh Merrill initializer left reconstruction unset. Existing recipes store complete settings, so an old saved off choice cannot be distinguished from a deliberate off choice and remains authoritative. Camera initialization runs only for missing recipes. Preview policy remains transient. The shared capture-default entry point also keeps cached rendered previews consistent with fresh GUI settings. The CLI retains its existing explicit reconstruction flags.

The actual-file sidecar test confirms default-on loading, no recipe from viewing or quality changes, explicit off persistence and byte-identical reopening. The native Smart check now relies on fresh Merrill initialization instead of turning reconstruction on in test setup. It retains the checkbox through Fast at Fit/100%, Full at 200%, panning, editing and cache reopening, and creates no source sidecars. All 529 ordinary workspace tests and five artwork tests pass, alongside the real-file sidecar check, strict Clippy, formatting and documentation checks. The native app bundle is rebuilt without installation or publication.
