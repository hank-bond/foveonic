---
summary: "Family panels compose shared editing widgets while the processor defines supported controls."
---

# Editing controls

Choose the processing family once in the parent dispatcher. Each family panel owns its layout and includes the controls that describe its processing. Shared rows own widget behavior and photographic controls. Keep photographic processing and capability/default resolution in `foveonic`.

Camera metadata supplies white-balance and color-mode choices. Normalize unsupported settings through the processor's control contract when metadata arrives. Adding a family should add a panel without adding family switches to shared widgets.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[classic.rs](./classic.rs)** Composes Classic controls using the camera's built-in reconstruction and corrections.
- **[common.rs](./common.rs)** Shares editing rows and photographic controls used by family panels.
- **[merrill.rs](./merrill.rs)** Composes Merrill color, monochrome, reconstruction and correction controls.

<!-- INDEX:END -->
