---
summary: "Finishing nodes prepare tone and resampling beside sharpening and encoding shaders."
---

# Finishing operations

Preserve signed samples and headroom until the selected operation requires encoding or clipping. The polyphase resampler also serves color reconstruction. Keep its existing tile and border behavior unchanged. Linear Color Editor classifies ranges from the same cleaned neutral color guide in Color and Monochrome. Lightness applies a scalar gain to the main image. Color also offers perceptual Hue/Saturation. Preserve the source reconstruction and declared code units. Linear basis mapping and scalar gain transfer have distinct DNG contracts.

Linear gain transfer accepts a reference pair related by scalar brightness multiplication. Use its strongest absolute RGB component to avoid luminance cancellation in signed samples. Do not use this operation to transfer arbitrary color transforms or reconstruct an inverse camera tone curve.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[photographic/](./photographic/AGENTS.md)** Shared finishing operations own tone, local contrast and sharpening with explicit brightness units.

### Source files

- **[box_contrast.rs](./box_contrast.rs)** Applies ordered box-mean local contrast to integral RGB codes with explicit noise and gain bounds.
- **[code_channel_map.rs](./code_channel_map.rs)** Mixes integral RGB codes with normalized channel weights or copies selected channels.
- **[code_crop.rs](./code_crop.rs)** Crops, rotates and resizes integral RGB codes with fixed-point mapping and source-derived interpolation phases.
- **[code_response.rs](./code_response.rs)** Maps unsigned integral RGB codes through supplied linear photographic response tables.
- **[color_editor.rs](./color_editor.rs)** Edits hue, saturation and lightness in linear RGB using an independent cleaned color guide.
- **[color_sharpen.rs](./color_sharpen.rs)** Sharpens integral RGB channels with wrapped neighborhood sums and signal-dependent thresholds.
- **[image_region.rs](./image_region.rs)** Copies a rectangular numerical signal on the GPU while preserving every component bit.
- **[linear_basis.rs](./linear_basis.rs)** Maps declared linear RGB codes into another linear sample basis without tone or clipping.
- **[linear_gain.rs](./linear_gain.rs)** Transfers a known scalar brightness edit between two linear RGB representations.
- **[mask_image.rs](./mask_image.rs)** Converts integer processing flags into a binary floating image for inspection.
- **[mod.rs](./mod.rs)** Prepares tone and resampling with shaders for sharpening and final encoding.
- **[photographic.rs](./photographic.rs)** Defines shared photographic controls and luminance-scaled GPU finishing parameters.
- **[proportional_sharpen.rs](./proportional_sharpen.rs)** Sharpens integral RGB codes from signed quarter-code detail and an adaptive amount table.
- **[resample.rs](./resample.rs)** Prepares camera resampling weights and leaves image calculations on the GPU.
- **[resize.rs](./resize.rs)** Prepares reference photographic resize kernels and samples owned floating RGB on the GPU.
- **[root_detail.rs](./root_detail.rs)** Maps signed linear codes into root space and extracts detail with a signed separable filter.
- **[signed_response.rs](./signed_response.rs)** Restores linear detail or applies photographic response to signed root-domain samples.
- **[tone.rs](./tone.rs)** Prepares a camera-derived color-mode tone table for application in a shader.
- **[tone_equalizer.rs](./tone_equalizer.rs)** Prepares coordinated HDR ranges and a reusable multiscale log-brightness base.
- **[wavelet_contrast.rs](./wavelet_contrast.rs)** Adjusts packed-Lab lightness through six lifting levels and bounded detail gains.

<!-- INDEX:END -->
