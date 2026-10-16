---
summary: "Task modules separate native builds, release notes, example-cache syncing, and lossless screenshot updates."
---

# Task source

Use Cargo metadata and compiler messages rather than guessing binary locations. Stage complete bundles beside their final location before replacing earlier builds. Preserve LaunchServices input-path handling and compiler icon metadata.

Source sync stages missing originals and saves an inventory only after success. Existing files are not compared or replaced. Screenshot capture clears inherited diagnostic modes, isolates app preferences, and replaces its WebP only after successful capture and a lossless decode comparison.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[bundle.rs](./bundle.rs)** Compiles native icon resources, stages complete macOS bundles, and resolves launch input paths.
- **[cargo.rs](./cargo.rs)** Finds application versions and executable artifacts through Cargo's machine-readable output.
- **[examples.rs](./examples.rs)** Syncs missing release photographs into one local cache and validates its last inventory offline.
- **[main.rs](./main.rs)** Exposes explicit Cargo commands for native bundles, example syncing, screenshots, and release notes.
- **[release.rs](./release.rs)** Validates release tags and selects one changelog section without generating introductory prose.
- **[screenshot.rs](./screenshot.rs)** Captures an isolated native GUI session and atomically refreshes its lossless WebP README asset.

<!-- INDEX:END -->
