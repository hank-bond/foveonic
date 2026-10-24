---
summary: "Integrate ICC-derived display limits, managed macOS presentation and gamut-preserving previews."
---

# Display color management

Implements [display color management](../docs/specs/display-color-management.md). The owner approved macOS first, accepted the native probe's visible gamut difference, and requested integration with Monitor, sRGB, Adobe RGB and ProPhoto RGB (ROMM) application viewing choices.

- [x] Inspect the pinned and newest egui-wgpu presentation APIs.
- [x] Preserve attribution and isolate the backend patch from photographic processing.
- [x] Validate native floating-window creation, its color-space tag, extended photographic samples and unchanged UI compositing.
- [x] Owner accepted the native probe's visible gamut difference.
- [ ] Review monitor/profile changes and run on macOS 15 hardware.
- [x] Preserve embedded and rendered preview gamut without binding caches to a monitor.
- [x] Use one presentation contract for viewport, navigator, browser and UI, with an sRGB fallback.
- [x] Verify numerical conversion, native presentation, macOS 15 API availability and unchanged export code.
- [x] Provide a concrete opt-in experiment for owner review and run repository checks.

## Findings

The project pins egui-wgpu 0.31.1 and wgpu 24. The newest documented egui-wgpu 0.37.0 still exposes only presentation mode and frame latency through its surface configuration. Its native painter derives the default color space rather than exposing a caller-selected one. Upgrading alone does not supply the required application hook. See the [current surface configuration](https://docs.rs/egui-wgpu/0.37.0/egui_wgpu/struct.SurfaceConfig.html) and [native painter](https://github.com/emilk/egui/blob/0.37.0/crates/egui-wgpu/src/winit.rs).

Before this work, the preview decoder and disk previews normalized to bounded sRGB RGB8. The viewport shader also clipped after converting photographic RGB to sRGB. Both boundaries lost out-of-gamut colors before the host can display them. RAW working buffers remain floating point and retain their declared primaries.

The pinned Metal backend advertises RGBA16Float surfaces and enables extended-range content for that format. It leaves the layer color-space property to the client. The implementation tags the actual CAMetalLayer and matches shader transfer to the tag. Apple's [color-management guidance](https://developer.apple.com/library/archive/technotes/tn2313/_index.html) and [Metal layer color space](https://developer.apple.com/documentation/quartzcore/cametallayer/colorspace) describe the compositor contract. A floating format alone is insufficient.

## Upstream color and blending discussions

The owner supplied [egui #2712](https://github.com/emilk/egui/issues/2712) and [#8283](https://github.com/emilk/egui/issues/8283). These identify separate contracts: declaring the surface color space, converting image primaries and transfer functions, and handling blending and premultiplied alpha. Managed sRGB correctness does not itself provide a wider output gamut.

[PR #2071](https://github.com/emilk/egui/pull/2071) deliberately selected gamma-space UI blending for text appearance. [PR #7311](https://github.com/emilk/egui/pull/7311), merged for egui 0.32, later changed texture filtering and native-texture expectations. The unmerged [PR #1765](https://github.com/emilk/egui/pull/1765) and issue #8283 explain why applying a transfer directly to premultiplied RGB can produce dark fringes. Selecting the existing linear shader alone would alter the UI's compositing and is not a sufficient patch.

The revised first experiment uses RGBA16Float tagged as extended gamma-encoded sRGB, retaining egui's existing shaders and UI blending. Photographic sampling remains in linear light and transfer encoding happens only at presentation. Verify the native extended transfer numerically before relying on negative encoded components. This preserves the approved floating-window design while testing a smaller backend change. A linear-window alternative needs explicitly correct alpha conversion or a separate final composition pass.

A local CoreGraphics check converted extended linear sRGB to extended encoded sRGB at eight inputs from -0.4 to 1.4, including both sides of the transfer knee. Native results matched a sign-preserving sRGB transfer within 0.00000012. Both named spaces are available since macOS 10.12. This validates the extended encoding math, not the complete window/compositor path.

[wgpu #10013](https://github.com/gfx-rs/wgpu/issues/10013) reproduced unmanaged Metal colors, and [PR #10286](https://github.com/gfx-rs/wgpu/pull/10286) merged an explicit sRGB-layer tag on September 29, 2026. This validates checking actual layer metadata rather than assuming that an sRGB texture format declares its color space. It does not establish the behavior of every earlier wgpu version. Screenshots must preserve or interpret their own ICC profile, and native color comparisons should use a color-managed reference or Digital Color Meter configured for the intended space.

## First experiment

The first experiment used an opt-in managed path with the normal app launch. A standalone `managed_color_probe` example compares retained Adobe RGB colors with their sRGB-clipped counterparts and a neutral ramp. Its preference storage is isolated. Run it with `cargo run --offline -p foveonic-gui --example managed_color_probe`.

The native probe opened an RGBA16Float surface and verified that its actual CAMetalLayer retained the extended sRGB tag. Two GPU tests exercise the photographic shader's negative out-of-sRGB components and egui's half-transparent white over white. Both passed. Managed and default Merrill Smart smoke runs each completed twelve native frames covering overview, viewport detail, full refinement and editing. Float screenshot readback converts to ordinary bounded sRGB8 for the existing capture interface. These captures validate interaction and UI appearance, not physical wide-gamut display matching.

At the first experiment checkpoint, only main-viewport RAW colors retained their gamut. Browser, cached and navigator previews still used their sRGB representations. The owner subsequently approved the application integration described below. SDK availability checks place the native color-space APIs below macOS 15, but this experiment has not run on macOS 15 or multiple monitors. Photographic processing and TIFF/DNG export code are unchanged.

Workspace tests, strict all-target Clippy, formatting, documentation generation/checks and artwork tests pass. The vendored source comparison limits changes to format selection and float screenshot capture. That initial patch retained upstream UI shaders and renderer. The application integration adds the extended transfer and shared gamut uniform described below.

## Application integration

Managed macOS presentation is enabled by default after owner acceptance of the probe. Known embedded primaries are retained as RGB16. Optional valid ICC profiles normalize to ROMM RGB16. Preview companions use lossless Deflate TIFF with a declared color space. Selection metadata and preview companion versions reject older sRGB-only assets. Native photographic textures retain linear extended sRGB, including negative components. Their filtering happens before the final sign-preserving transfer. The backend shader extension leaves nonnegative UI colors and alpha policy unchanged.

The application display-gamut preference defaults to Monitor, with sRGB, Adobe RGB and ProPhoto RGB (ROMM) limits. One shared WGSL function applies the chosen limit in the viewport and egui's native-image path. Navigator preparation and disk previews retain full gamut. Changing the preference updates presentation uniforms and redraws existing textures, with no photographic request or cache invalidation. On platforms or surfaces using the sRGB fallback, physical output remains bounded by that fallback.

## Cleanup decisions

The application owns named display choices and obtains all primary and gamut matrices from the existing export ICC colorants. The backend patch now accepts generic matrices, with no camera or photographic preset knowledge. Viewport and native preview presentation share the same matrix-and-limit function. Colors inside the selected gamut are preserved, rather than stretched to fill it. Bounded sRGB fallback limits after gamut mapping so its viewport and native previews agree.

Managed presentation is the normal platform path, removing the experimental environment switch and optional display-host state. Display-asset transfer functions live with preview pixels, independently of TIFF export recipes. Keep the native color probe as an explicit developer diagnostic for physical display checks.

Retain RGBA16Float window and preview textures because signed extended components preserve wide colors while supporting linear photographic interpolation and existing UI blending. Retain RGB16 lossless TIFF preview companions because they avoid repeated lossy encoding and preserve gradient precision with existing dependencies. Their larger byte count remains subject to the existing cache budget and LRU policy. Native RAM previews stay bounded to two larger previews and 256 thumbnails.

Before the generic-transform cleanup, native Merrill Smart completed twelve checkpoints, native Quattro display switching completed seven checkpoints, and a bundled Merrill run completed all four gamut choices across eight frames. All display-switch reports retained the same photographic and thumbnail revision. The bundle and executable both target macOS 15.0. Cross-monitor and macOS 15 runtime checks remain manual acceptance work.

After cleanup, six focused GPU tests passed, covering photographic mapping, native preview filtering, UI transparency, navigator diagnostics and gamut-preserving preview readback and caching. The rebuilt native bundle completed eight display checkpoints across all four gamut choices, retaining the same photographic and thumbnail revision. Workspace tests, strict all-target Clippy, formatting, documentation and artwork checks passed.
