---
summary: "A pinned egui-wgpu patch enables managed floating-point presentation."
kind: reference
---

# egui-wgpu presentation patch

Imported from the published egui-wgpu 0.31.1 crate by the egui authors. Upstream is [egui 0.31.1](https://github.com/emilk/egui/tree/0.31.1/crates/egui-wgpu), licensed under MIT OR Apache-2.0. Preserve upstream notices and the accompanying license files.

Local changes add an optional preferred framebuffer format, use it only if supported by the native surface, and preserve upstream UI blending. The client owns platform color-space tagging and matching shader transfer. The default framebuffer choice remains upstream behavior. This supports macOS display color management without a framework upgrade or changes to photographic arithmetic.

The published manifest's license include paths are adjusted to the licenses retained in this vendored directory.

Screenshot readback handles RGBA16Float's eight-byte pixels and converts encoded float components into bounded sRGB8 screenshot events. Wide values remain available on screen but are clipped in these standard sRGB screenshots. The conversion uses the already-resolved half dependency and runs only for requested captures. Ordinary eight-bit capture remains unchanged.

The gamma encoder extends the sRGB transfer symmetrically to negative components for opaque floating photographic textures. Nonnegative UI colors and the upstream blending and alpha policy are unchanged. Photographic filtering occurs in linear light.

The renderer accepts a generic ColorTransform containing two RGB matrices and a limit flag. The application supplies the colorants and gamut policy. A shared WGSL function maps into the chosen gamut, limits samples to 0..1, and maps back before transfer encoding. The application uses the same function for its photographic callback. The backend contains no photographic gamut presets or profile matrices. Offline preview preparation leaves limiting disabled.
