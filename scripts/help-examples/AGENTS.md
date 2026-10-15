---
summary: "Example tooling downloads missing release photographs and explicitly renders manifest-driven help comparisons."
---

# Help example tools

Follow the [workflow](../../docs/help-examples.md) and [photographic help requirements](../../docs/specs/photographic-help.md). Keep downloads, full TIFFs, and review sheets under ignored local storage. Application builds must not invoke this tool. `sync` needs GitHub CLI; only explicit rendering needs Pillow and the release renderer. Do not install dependencies automatically.

Source X3Fs come from the append-only supplemental release, without editing sidecars. Skip existing files and leave local originals and sidecars untouched. Hold unrelated processing settings fixed between variants. Keep captions photographic and avoid claims not supported by the images. Rendering candidates must not overwrite checked-in assets unless `--write` is explicitly selected after review.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[examples.py](./examples.py)** Download official sample files and explicitly render manifest-driven photographic help.

<!-- INDEX:END -->
