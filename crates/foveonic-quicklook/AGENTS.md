---
summary: "The Quick Look crate renders X3F space-bar previews and embedded-image thumbnails for the sandboxed macOS extensions through a C interface."
---

# Quick Look previews

Follow the [Quick Look preview requirements](../../docs/specs/quick-look-previews.md). Keep photographic processing in `foveonic`. This crate chooses the preview recipe and size, asks `foveonic::preview` for upright embedded images, and hands finished pixels across the C boundary. `macos/` holds the two Swift extensions, the image wrapping they share and their entitlements, which `cargo xtask bundle` compiles into the app.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[src/](./src/AGENTS.md)** The library selects the preview recipe and working scale, falls back to embedded images, and exposes owned pixels to Swift.

<!-- INDEX:END -->
