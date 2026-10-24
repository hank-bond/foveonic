---
summary: "Normalize embedded camera previews using their declared color space across all camera architectures."
status: archived
---

# Preview color management

The owner confirmed using Adobe RGB after observing a color jump from the immediate cached preview to the completed RAW render. The shared embedded-preview reader previously treated all decoded pixels as sRGB. Local Merrill originals declare Adobe RGB both in X3F properties and the larger JPEG's Exif interoperability tag. The shared reader now implements the correction for Classic, Merrill, Quattro and Quattro H, following [browser previews](../../docs/specs/browser-previews.md).

- [x] Resolve JPEG ICC and Exif declarations, then camera metadata, without another original-file open.
- [x] Normalize embedded bitmaps, JPEGs and nested Exif thumbnails to sRGB during background display-asset decoding.
- [x] Invalidate old preview companions without changing the decoded RAW format. Keep rendered sRGB previews out of camera conversion.
- [x] Check native and synthetic inputs across architectures, tagged/untagged previews and cache round trips.
- [x] Run repository checks and document limitations.

The existing image stack's moxcms color-management dependency is used directly for CPU display decoding. This adds no new resolved package and keeps RAW arithmetic in WGSL. The preview contract is sRGB-encoded RGB8. An embedded RGB ICC profile takes precedence over Exif sRGB/Adobe declarations. Unlabelled nested thumbnails inherit their enclosing JPEG, then camera metadata. Missing or malformed optional declarations use the next available declaration. Unknown camera encoding retains the previous sRGB assumption. Rendered previews already have sRGB encoding and bypass this reader.

The owner deferred wide-gamut presentation to a separate effort and design discussion. The existing viewport converts to sRGB and clips outside that gamut. This fix uses that same display target for embedded assets. The original camera data remains available to rebuild disposable previews for a future display path.

## Validation

Synthetic fixtures cover Classic, Merrill, Quattro and Quattro H metadata layouts, including version-4 files with no PROP section and the color setting stored only in a JPEG MakerNote. Numerical Adobe RGB fixtures agree with the viewport's transfer/matrix reference within one 8-bit code. Tests cover ICC precedence, malformed or unusable optional profile fallback, inherited thumbnail profiles, Exif declarations in either byte order, neutral colors and gamut clipping. Existing sRGB bitmap samples remain exact. A rendered-preview cache check uses Adobe RGB source metadata and verifies that saved sRGB colors receive no second conversion.

Local Merrill originals passed thumbnail decoding. Isolated native Merrill and Quattro Smart runs completed all twelve targets each, including cache reopen. Classic and Quattro H coverage in this slice is synthetic rather than a new native photographic comparison. Workspace tests, strict Clippy, formatting, artwork tests and document indexes passed. Camera JPEG processing and Fast/Full algorithms can still differ from a photographic RAW render after correct color conversion.
