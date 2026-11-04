---
status: archived
summary: "Add shared JPEG, PNG and JPEG XL exports and lossless TIFF compression."
---

# Extend photographic exports

Requirements: [output strategies](../../docs/specs/output-strategies.md) and [import workflow](../../docs/specs/import-workflow.md).

- [x] Share format and encoding options between the processor, GUI and CLI.
- [x] Package JPEG, PNG, JPEG XL and compressed TIFF using dependencies.
- [x] Expose format-specific controls and freeze them in processing jobs.
- [x] Verify sample preservation, profiles, malformed inputs and real photographic exports.
- [x] Run workspace and documentation checks.

JPEG XL uses the statically built libjxl through `jxl-sys`. Compression effort is independent of lossless versus lossy quality. AVIF is deferred by the owner. DNG encoding remains unchanged.

## Validation

- Workspace tests, strict all-target Clippy, formatting, documentation indexes and five artwork tests pass.
- TIFF Uncompressed/LZW/ZIP, RGB16 PNG and lossless JPEG XL preserve the exact encoded 16-bit samples. JPEG XL checks cover all destination profiles at efforts 1, 7 and 11. RGB8 quantization, lossy JPEG XL color-profile equivalence, invalid options and format-specific UI controls are covered.
- The existing GPU destination-conversion check passes and retains the working image. A real Quattro batch matches independent frozen exports in DNG, TIFF, JPEG, PNG and JPEG XL, while retaining foreground priority and source-file protection.
- A Merrill photograph exports through the CLI as JPEG, RGB16 PNG, lossless JPEG XL and ZIP TIFF with explicit resizing and ProPhoto RGB destination conversion.
- The review macOS bundle builds offline, loads its help command, targets macOS 15.0 and links only system libraries dynamically. Bundled resources include the native encoder license notices. Interactive Photoshop review remains an owner check.

`jxl-sys` exposes libjxl's compression API without implementing a codec in this repository. Lossy native-color profiles are checked for equivalent color conversion rather than byte-identical ICC headers. Compression ratios have not been benchmarked across the photograph collection.

## Wide-gamut JPEG XL

A supplied lossy XYB export retained the same Adobe RGB ICC bytes as its TIFF counterpart. Apple's ImageIO returned it as unsigned sRGB, clipping negative linear-sRGB components needed to represent the wider blues. libjxl float decoding retained those components. This exposed a consequence of our ICC-blob signaling choice, rather than a missing profile. The owner confirmed both a matching lossless comparison and an improved native-color lossy prototype.

Lossy exports now signal sRGB, Adobe RGB and ProPhoto through native color fields while retaining XYB compression. ProPhoto uses a pure 1.8 gamma in those fields. Its GPU destination encoding therefore applies the corresponding curve directly to linear light. ICC-profiled TIFF/PNG and lossless JXL retain the ROMM shadow toe. Quality 100 selects the exact lossless path, including its profile and sample encoding.

Upstream describes [color signaling and XYB output spaces](https://github.com/libjxl/libjxl/blob/main/doc/format_overview.md#color-management). Its [original-profile lossy issue](https://github.com/libjxl/libjxl/issues/4552) explains why forcing original-profile VarDCT is not the chosen workaround.

Validation covers native decode metadata, saturated RGB values outside sRGB and unchanged lossless samples. A GPU fixture checks values around the ProPhoto shadow transition for lossy, lossless and quality-100 output. Real photographic CLI exports in all three spaces were decoded through Apple's ImageIO. Adobe RGB and ProPhoto lossy output retains its native destination profile and wider blue values through color management. The owner reviewed the updated application and comparisons and approved cleanup and commit.
