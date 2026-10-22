---
summary: "The X3F reader decodes sensor samples and camera metadata from original files."
---

# X3F input

Imported from the reference implementation at commit `d5e21fb`. The decoder owns unnormalized sensor samples and typed metadata. Equal-resolution samples are interleaved RGB. Quattro samples are packed six-value cells. Camera and geometry checks belong to `source.rs`.

Keep decoding independent of photographic adjustments and orientation. Preserve original matrix encodings and camera property-list order, which selects color-mode tone rows.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[adjustments.rs](./adjustments.rs)** Reads scalar version-4 header adjustments into retained capture properties.
- **[bytes.rs](./bytes.rs)** Read bounded little-endian values and terminated strings from owned file sections.
- **[camf.rs](./camf.rs)** Decode camera metadata while preserving matrix encodings and property-list order.
- **[capture.rs](./capture.rs)** Seeks directly to X3F capture properties without reading sensor or calibration payloads.
- **[exif.rs](./exif.rs)** Reads preferred exposure metadata from an embedded JPEG's optional Exif header.
- **[format31.rs](./format31.rs)** Decodes unsigned mixed-resolution sensor planes into RGB/blue-phase cells.
- **[format6.rs](./format6.rs)** Decode row-local Huffman differences into signed interleaved sensor codes.
- **[huffman.rs](./huffman.rs)** Decode category-coded differences with two-column and two-row sensor predictors.
- **[mod.rs](./mod.rs)** Reads owned X3F sections and decodes sensor samples and camera metadata.

<!-- INDEX:END -->
