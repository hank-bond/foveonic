---
summary: "Finishing nodes prepare tone and resampling beside sharpening and encoding shaders."
---

# Finishing operations

Preserve signed samples and headroom until the selected operation requires encoding or clipping. The polyphase resampler also serves color reconstruction. Keep its existing tile and border behavior unchanged.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[mod.rs](./mod.rs)** Prepares tone and resampling with shaders for sharpening and final encoding.
- **[resample.rs](./resample.rs)** Prepares camera resampling weights and leaves image calculations on the GPU.
- **[resize.rs](./resize.rs)** Prepares reference photographic resize kernels and samples owned floating RGB on the GPU.
- **[tone.rs](./tone.rs)** Prepares a camera-derived color-mode tone table for application in a shader.

<!-- INDEX:END -->
