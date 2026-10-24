---
summary: "Display colors follow each image's declared encoding and the host monitor's active profile."
kind: reference
status: accepted
implementation: incomplete
---

# Display color management

Photographic display must honor the image's declared RGB primaries and transfer function and use the host monitor's active profile as its final destination. The entire window must have one correctly declared presentation space, including its UI, viewport, navigator and browser images. Moving the window between monitors must allow the host to apply each monitor's selected profile. This must preserve usable colors outside sRGB on capable displays.

The first implementation targets macOS. Other platforms retain an explicit sRGB fallback. Keep macOS 15 compatibility. Source profile selection applies to all supported camera architectures. Embedded previews and rendered previews must retain their gamut until presentation, rather than clipping to sRGB before caching. Cached assets must remain independent of a particular monitor.

Color management is a display operation. Keep photographic tone mapping, exposure, saved edits, scopes and TIFF/DNG processing independent of the monitor. Use the existing photographic rendering rather than adding AgX or another scene-to-display tone mapper. This effort provides SDR colors without introducing HDR photographic rendering or increased reference-white brightness.

The managed macOS window uses a floating-point window tagged with an extended sRGB encoding. Shader transfer must match that declaration. macOS performs the final display-profile conversion. A small pinned egui-wgpu patch selects the surface format. The owner accepted the visible gamut difference in the native probe and approved enabling the managed path in the application. Validate saturated colors, neutral ramps, alpha compositing and UI appearance. Use a correctly tagged sRGB surface when floating presentation is unavailable.

The application settings must offer Monitor, sRGB, Adobe RGB and ProPhoto RGB (ROMM) display-gamut choices, defaulting to Monitor. Monitor preserves colors for the host's selected display profile. The other choices limit photographic colors to their named gamut before the same host conversion. This is a viewing preference shared by viewport, navigator and browser previews. It must persist independently of photo edits and update without RAW processing, cache regeneration or changes to exports. Cached previews must retain their gamut regardless of the current viewing choice.
