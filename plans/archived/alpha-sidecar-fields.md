---
summary: "Use shared defaults and tolerate extra alpha editing fields without sidecar migrations."
status: archived
---

# Tolerant alpha sidecars

Requirements: [editing sidecars](../../docs/specs/editing-sidecars.md).

- [x] Ignore extra fields in the recipe envelope and nested photographic control structs.
- [x] Preserve shared defaults for omitted fields and validation of supplied known values, duplicate known fields and format/version markers.
- [x] Check nested unknown controls and partial recipes without changing supplied edits.

An experimental `research` object in a demo sidecar blocked both edit restoration and complete cache publication. Ordinary Serde field tolerance fixes the shared parsing boundary. No migration or fallback reader is needed. Ignored fields are not retained on a subsequent save. Unrelated import, review and calibration records keep their existing parsing policy. Optional diagnostic observation remains a separate integration.

The implementation uses the existing shared photo-setting defaults. Tests inject unknown fields into every serialized control object and verify exact known edits, partial nested defaults and invalid supplied values.
