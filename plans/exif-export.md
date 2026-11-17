---
summary: "Audit and preserve X3F capture metadata across all supported export formats."
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

## Cross-architecture audit and warning cleanup

The owner requested deeper capture-value testing after an SD15 forum upload showed shutter `4000000/1000000000s`, numerically 1/250 s. The shared writer generated these fractions across architectures. Existing tests checked numeric quotients but not stored pairs. An isolated audit exported all 27 cached originals to TIFF, JPEG, PNG and JPEG XL, plus eight representative DNGs, using the production processing and export APIs.

The audit found descriptive lens names overwritten by numeric IDs, a DP1s standard metering value overwritten by a conflicting maker property, container-specific declaration defects and an invalid source SD15 comment. The patch preserves valid standard fields before camera fallbacks, canonicalizes rational storage, fixes the declarations, rejects malformed comments and preserves Unicode byte order. Maintained checks cover seven pinned real photographs through all five formats, plus 33 encoder-option/color-space combinations. SD15 and DP1s were added to the existing download manifest with publisher identities and license terms.

The final capture-value audit has no missing values or mismatches in 1,624 checks, including ImageUniqueID. All 116 published-code image codestreams and all eight DNG tone/look arrays remain byte-identical. Representative stored shutter pairs are SD15 1/320, SD9 1/30, Merrill 1/100, dp0 Quattro 13/10 and Quattro H 1/4000. The reported screenshot's 1/250 case also has direct raw-pair coverage.

## Adobe SDK reader checks

The owner requested independent SDK checks. The published SDK archive SHA-256 is `e9bb04e956da3605d8ef4f821bdb979a250c22546b0332b7f81a93b11da7cceb`. It was built with its bundled XMP, JPEG and JPEG XL libraries. The supplied `dng_validate` requires a DNG. A small adapter uses the unmodified `dng_info` and `dng_read_image` APIs for photographic TIFF, and `dng_jxl_decoder::Decode` with EXIF container-box parsing for JPEG XL.

Adobe reported two structural defects that ExifTool had not flagged: odd TIFF directory offsets and source-padded ImageUniqueID counts. Valid padded IDs are now normalized in both source parsing and cached-tag loading. A [pinned TIFF dependency patch](../vendor/tiff/FOVEONIC.md) fixes directory and external-value alignment. Existing caller padding was removed so the dependency owns alignment consistently.

The corrected candidate passes SDK parsing/decoding without warnings or errors for 125 real files: 90 TIFFs, eight DNGs and 27 lossless JPEG XL exports. All 2,304,000 uint16 components match independently extracted expected samples. All 63 TIFF-variant strip payloads are byte-identical before and after the structural fixes. These checks establish parsing and sample decoding, not application color-management or visual appearance acceptance. Standalone JPEG and PNG were not tested through the SDK.

## Warning decisions and final checks

The owner approved cataloguing the reviewed warnings, then required one maintained source to prevent drift. Exact warning matches, acceptance conditions and justifications live only in [the shared capture EXIF checks](../crates/foveonic/tests/common/capture_exif.rs). Test output is generated from those records. Guidance elsewhere only links there.

- [x] Audit and correct capture values across all supported architecture routes and output containers.
- [x] Add raw-storage, precedence, comment, identifier, alignment and validator checks.
- [x] Verify unchanged image codestreams, compressed strips and DNG profile arrays.
- [x] Run independent SDK parsing/decoding and compare samples exactly.
- [x] Review implementation and dependency changes with Sol and make the source patch and audit exports available for owner testing.
- [x] Run all 33 encoder-option combinations and 35 real-source exports with the reviewed warning decisions.

All 645 active core tests, the integration target's two hardware-free warning-context tests, 35 real-source exports and 33 encoder-option combinations pass. Strict processor Clippy, formatting, documentation/index and diff checks pass. The profiling-tool Python suite passed 199 tests with one existing platform-specific skip. Sol's implementation reviews found no remaining issues. Changes remain uncommitted.
