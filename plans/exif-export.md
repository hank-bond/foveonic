---
summary: "Carry supported X3F capture metadata into every 0.8.1 export."
---

# Export capture metadata

Requirements: [output strategies](../docs/specs/output-strategies.md).

- [x] Inventory X3F properties and embedded EXIF and settle a shared output design.
- [x] Preserve compatible capture facts and adapt shared EXIF directories to all five formats.
- [x] Verify capture fields, output geometry, color and image data using synthetic fixtures.
- [x] Review the design and implementation with Astra and address its findings.
- [x] Run the repository checks and make the patch available for owner testing.

X3F standard properties distinguish camera and lens identity, exposure and capture settings, and camera-specific processing or sensor state. Its embedded JPEG may contain additional standard EXIF, including GPS and authorship. Output image dimensions, orientation, compression, thumbnail, color declarations, offsets and software describe the processed file and must be regenerated. MakerNotes, CAMF and private sensor or processing data do not have a safe general EXIF mapping. The [FOVb specification](https://raw.githubusercontent.com/wiki/drewnoakes/metadata-extractor/docs/x3f-raw-format.pdf) documents the original property names and optional JPEG EXIF. Values must not be inferred from ambiguous camera settings.

The browser retains each photo's metadata from its existing capture pass. Version-2 decoded cache entries retain their pixel payloads. They refresh capture metadata through an identity-checked metadata-only source read, then reuse it in the current browser cache session. If the original is unavailable, the cached image remains usable with the older available metadata. JPEG segment scanning skips unrelated payloads and reads at most one bounded EXIF segment.

Initial processor, GUI, release-tool, documentation and Python checks passed separately. The initial GUI run required a larger test-thread stack, which led to the investigation below. The processor GPU tests pass with direct graphics access. One parallel run could not acquire a GPU adapter. The local 0.8.1 bundle builds, reports the matching version and responds to its help command. Real-X3F headless export checks are recorded below. Interactive export review remains for owner testing.

## Stack overflow investigation

The default-stack GUI failure occurs while inserting the second large `Photo` value into the multi-edit runtime's `BTreeMap`. The existing record is 77,328 bytes on the tested Apple Silicon debug build, with 9,584-byte settings and 19,472-byte frames stored inline. This code was unchanged by the metadata patch. The owner approved Astra’s recommendation to store `Box<Photo>` map values. This keeps one owned photo allocation per entry while reducing node values to pointers. Astra found no issues in the applied fix. The existing regression and all 534 active GUI tests passed in the trial with normal stack settings. The full workspace suite now passes in one serial run with normal stack limits and graphics access. The optimized GUI suite also passes: 534 active tests, with 52 opt-in tests ignored. Strict workspace Clippy, formatting and documentation checks pass.

## Example-image export scenarios

The owner requested headless scenarios using the cached example photographs, with one case per export format, and approved ExifTool as a test dependency. The maintained integration tests use independently specified capture values and pinned input identities, covering metadata-only/full-source agreement, edited geometry and export after the loaded source path disappears. [The coverage guide](../docs/testing-coverage.md#capture-metadata-exports) records requirements and the exact command. All five cases pass using the cached examples and ExifTool. Targeted strict Clippy, formatting and documentation checks pass. Astra reviewed the tests and found no remaining issues.

## Repeatable test setup

The owner requested a repeatable environment setup path. `scripts/setup_tests.py` checks the headless test tool requirements, supports explicit installation of missing macOS packages/Rust components, and delegates image preparation to the existing sync and pinned-sample verifier. Default checks do not install or download. Bootstrap toolchains, GPU access and native app prerequisites remain explicit. Installer decisions and offline forwarding have portable unit coverage.

Setup verification passes: 11 portable setup tests, the existing profiling Python suite, dependency checks, explicit-install no-op, offline verification of 13 release examples and seven external samples, and documentation checks. Astra’s final review found no remaining issues.
