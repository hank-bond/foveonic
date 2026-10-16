---
summary: "Cargo tasks package native apps, prepare draft-release notes, sync example photographs, and capture the README image."
---

# Workspace tasks

Follow the [release requirements](../../docs/specs/releases.md), [release plan](../../plans/archived/cargo-releases.md), and [screenshot requirements](../../docs/specs/gui-screenshot.md). Keep packaging separate from ordinary compilation. Never regenerate artwork, install an app, or publish a release as a packaging side effect.

Pure packaging, source-sync, encoding, and argument tests run in `cargo test --workspace` without Xcode, network access, or photographic fixtures. Native bundle validation and [screenshot capture](../../docs/gui-screenshot.md) are explicit on macOS. Sync originals only through `cargo xtask examples sync`, preserving the shared ignored cache and its local editing sidecars.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[src/](./src/AGENTS.md)** Task modules separate native builds, release notes, example-cache syncing, and lossless screenshot updates.

<!-- INDEX:END -->
