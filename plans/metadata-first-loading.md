---
summary: "Read capture highlights independently of image decoding so the information bar updates while the selected photograph loads."
---

# Show capture highlights before image loading

The owner approved a lightweight latest-selection metadata reader independent of the render worker. Requirements: [image information bar](../docs/specs/image-information-bar.md). Keep processing metadata and render scheduling unchanged.

## Steps

- [x] Add a bounded seek-based X3F property reader reusing the existing property parser, without reading sensor or CAMF payloads.
- [x] Add an independent background reader with one active/latest pending selection and stale-selection rejection.
- [x] Feed early properties to the existing photographic formatter while no rendered frame is available; retain displayed-frame provenance after rendering.
- [x] Test property-only reads, malformed inputs, rapid switching/clearing/reselection, and original-file equivalence.
- [x] Run GUI/core tests and strict Clippy.
- [x] Finish formatting and documentation/index checks and tool tests.

## Implementation and evidence

`source::read_capture_properties` reads only the file header, section directory, and PROP payload, using the existing UTF-16 parser and Merrill camera check. The seek reader bounds allocations and rejects malformed, duplicate, overlapping, missing, and oversized property sections. Tracked-read fixtures prove that sensor/CAMF payloads are untouched.

The GUI capture reader has its own thread, one replaceable pending request, and one ready result. A selection generation rejects stale results even after returning to the same path; selection/clear resets displayed properties and pending publication. It requests a repaint on completion. Capture failures leave the highlights blank without overriding the render worker's status/errors. Early properties feed only the information bar; processing controls still receive complete metadata from the existing image worker. After a frame arrives, its own properties take precedence.

Workspace tests pass: 53 ordinary core tests, two CLI tests, 54 GUI tests, and docs-index tests; opt-in GPU/fixture tests are skipped except the six-original source test. The release original-file test confirms exact equality between property-only and fully decoded metadata on all six originals. Strict workspace all-target Clippy passes. The local macOS release bundle builds. A native loading-view capture shows the selected filename and all five highlights while the app displays its loading/preview state; the standard twelve-state native smoke run finishes successfully. Owner rapid-browsing review remains pending.

## Research

All five capture highlights are available in the PROP section on six local originals. A read-only Python probe read 1664–1684 bytes per file versus 48–59 MB for whole files; warmed median was approximately 0.06 ms, with uncontrolled-cache initial probes below 3 ms. These are local probe observations, not end-to-end GUI timing guarantees. Splitting the existing render worker's load alone would still delay new metadata behind a previous image's work.
