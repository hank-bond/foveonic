---
summary: "Shared finishing operations own tone, hue-guided monochrome sensitivity, local contrast and sharpening over normalized RGB."
---

# Shared photographic operations

The parent module owns editing controls, validation and the common GPU parameter layout. Each child owns one numerical operation and its shader. Camera recipes resolve their metadata into those parameters. Keep sensor reconstruction and reference camera appearance in their existing recipes.

`common.wgsl` provides the shared parameter/buffer ABI and RGB access functions. Kernel registration combines it with the owning operation shader. Preserve arithmetic order and signed/headroom behavior during organizational changes. Test complete image goldens when changing shader storage or dispatches.

Inspection names use the graph's immutable image alias. Aliases do not belong in numerical operation selectors. Neutral controls are omitted by the recipe. Local Contrast is independent of reconstruction detail and sharpening.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[local_contrast.rs](./local_contrast.rs)** Adjusts six log-brightness texture bands without coupling noise or sharpening.
- **[sensitivity.rs](./sensitivity.rs)** Adjusts monochrome brightness using subject hues from an independent color guide.
- **[sharpen.rs](./sharpen.rs)** Sharpens square-root brightness with independent threshold and halo restraint.
- **[tone.rs](./tone.rs)** Applies shared contrast and saturation to normalized photographic RGB.

<!-- INDEX:END -->
