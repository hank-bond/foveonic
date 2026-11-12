---
summary: "Browser modules separate folder discovery, queries, selection, drawing and bounded preview loading."
---

# Photograph browser

Follow the parent GUI source instructions. Keep filesystem discovery, metadata and image loading on their existing background workers. Browser interaction and drawing consume their snapshots without waiting for completion. Preserve the distinction between browser selection and bounded viewport editing targets.

The browser owns its thumbnail and larger-preview retention. Keep that data distinct from decoded source caching and numerical processing state. Source moves must preserve the current module interfaces and thread ownership.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[chrome.rs](./chrome.rs)** Draws compact browser view/sort/search menus and independent star-rating controls.
- **[folders.rs](./folders.rs)** Discovers descendant X3Fs and lays out folder groups for virtualized browsing.
- **[items.rs](./items.rs)** Paints aspect-preserving browser images and shared selectable metadata rows with star controls.
- **[loader.rs](./loader.rs)** Prioritizes visible thumbnails over a greedy folder pass on a separate worker from larger previews.
- **[metadata_loader.rs](./metadata_loader.rs)** Loads browser capture metadata and folder ratings off-thread and coalesces independent rating saves.
- **[mod.rs](./mod.rs)** Browses X3F paths with workspace keyboard selection and bounded embedded and rendered previews.
- **[query.rs](./query.rs)** Defines browser metadata, stable folder-local sorting and shared substring/rating filtering.
- **[review.rs](./review.rs)** Handles browser highlighting, import checkboxes, and shared hidden-photo visibility.

<!-- INDEX:END -->
