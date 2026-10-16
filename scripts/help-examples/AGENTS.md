---
summary: "Python tooling explicitly renders manifest-driven help comparisons from the source cache maintained by Cargo."
---

# Help example tools

Follow the [workflow](../../docs/help-examples.md) and [photographic help requirements](../../docs/specs/photographic-help.md). Keep downloads, full TIFFs, and review sheets under ignored local storage. Application builds must not invoke this tool. Sync sources with `cargo xtask examples sync`. Python only handles explicit rendering with Pillow and the release renderer. Do not install dependencies automatically.

Source X3Fs come from the append-only supplemental release, without editing sidecars. The [Cargo source-sync task](../../crates/xtask/AGENTS.md) owns downloads and offline inventory checks. Leave local originals and sidecars untouched. Hold unrelated processing settings fixed between variants. Keep captions photographic and avoid claims not supported by the images. Rendering candidates must not overwrite checked-in assets unless `--write` is explicitly selected after review.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[examples.py](./examples.py)** Render manifest-driven photographic help from the example cache maintained by Cargo.

<!-- INDEX:END -->
