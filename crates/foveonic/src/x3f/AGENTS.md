---
summary: "The X3F reader decodes Merrill sensor samples and camera metadata from original files."
---

# X3F input

Imported from the reference implementation at commit `d5e21fb`. The decoder owns unnormalized, interleaved sensor samples and typed metadata. Camera and geometry checks belong to `source.rs`.

Keep decoding independent of photographic adjustments and orientation. Preserve original matrix encodings and camera property-list order, which selects color-mode tone rows.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[bytes.rs](./bytes.rs)** Read bounded little-endian values and terminated strings from owned file sections.
- **[camf.rs](./camf.rs)** Decode camera metadata while preserving matrix encodings and property-list order.
- **[capture.rs](./capture.rs)** Seeks directly to X3F capture properties without reading sensor or calibration payloads.
- **[huffman.rs](./huffman.rs)** Decode category-coded differences with two-column and two-row sensor predictors.
- **[mod.rs](./mod.rs)** Reads owned X3F sections and decodes equal-resolution format-30 samples and camera metadata.

<!-- INDEX:END -->
