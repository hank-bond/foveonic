---
summary: "Manifest-driven photographic comparisons embed reviewed PNGs with labels, rendering settings, and source names."
---

# Photographic help assets

Follow the [maintenance workflow](../../../../docs/help-examples.md) and [help requirements](../../../../docs/specs/photographic-help.md). Each comparison directory has `comparison.json` and one PNG per labeled variant. Keep official X3F names and stable help IDs. Source originals belong in ignored local storage, not here. Comparison recipes live in the manifest, not in distributed sidecars.

Application builds validate and embed existing images without rendering or downloading. Render and review changes explicitly. The [shared template](../../src/help_examples.rs) presents every comparison; avoid per-control layout code. Highlights follows the owner's specified Off/Auto/+0.8 crop and −0.5 EV exposure. Bad pixels and Speckles use complementary defects, with the companion correction enabled in each Off/On pair. Row banding uses a brightened recycling-bin shadow without amplifying the removed stripes.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

_(none yet)_

<!-- INDEX:END -->
