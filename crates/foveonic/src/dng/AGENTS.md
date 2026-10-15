---
summary: "DNG export owns camera-coordinate storage, ordered presentation composition, and uncompressed TIFF-based packaging."
---

# DNG export source

Follow [the DNG plan](../../../../plans/dng-export.md) and [output requirements](../../../../docs/specs/output-strategies.md). Keep photographic sample arithmetic on GPU. Host calculations evaluate metadata curves/color tables, not image pixels. `gpu.rs` is compiled as a child of the renderer so its export methods retain private guarded storage and engine ownership checks.

Each file embeds one selected appearance. Combine adjacent matrices in the order they are applied. Moving a matrix across channel tone changes the result. SDK validation and exact sample decoding do not establish that an editor renders the intended look.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[appearance.rs](./appearance.rs)** Keeps the selected DNG appearance separate from its editable camera samples.
- **[gpu.rs](./gpu.rs)** Makes linear camera samples on the GPU so DNG retains room for later edits.
- **[writer.rs](./writer.rs)** Writes a linear technical base and one selected appearance into an uncompressed DNG.

<!-- INDEX:END -->
