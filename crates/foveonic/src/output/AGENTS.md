---
summary: "Photographic output adapters delegate image compression to encoder dependencies."
---

# Photographic encoding

The parent output module owns shared format options and RGB sample packaging. This directory isolates native JPEG XL resource ownership and codec round-trip checks. Keep photographic processing in the GPU graph.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[jxl.rs](./jxl.rs)** Owns libjxl encoder resources and forwards photographic samples without changing their colors.

<!-- INDEX:END -->
