---
summary: "Versioned application changes supply the initial notes for draft GitHub Releases."
kind: reference
---

# Changelog

## Unreleased

Optimization and responsiveness improvements following the feature-focused 0.7.0 release.

### Changes

- Reduce Merrill Reference speckle-processing stalls at low thresholds while preserving the existing rendered result.

## 0.7.0

Adaptive starting renders, dehaze, saved crops and more export formats, with improved colour controls and a quieter workspace.

### Changes

- Adaptive tone offers Natural and Strong starting renders on Classic, Merrill and Quattro. Regional analysis brings shadow detail into view and compresses bright detail, responding to Exposure without changing the artistic tone sliders. Shared profile limits remain available for tuning.
- Dehaze adds automatic or picked shadow colour and a shared dark-signal protector across camera families. Zero Amount bypasses the operation, and automatic colour estimation runs only when selected.
- Exposure's A button compensates for recorded dial, bracket or combined exposure compensation. Manual edits release A, clicking A again returns to zero, and camera defaults retain the automatic choice rather than one photograph's measured value.
- Advanced Color Range offers a centred colour wheel, named custom ranges, direct photograph picking and dragging, and one selected-range preview at a time. Basic and Advanced share consistent controls and clipping protection.
- Prevent saturation clipping is enabled by default and shared by Reference, Universal and Color Range saturation. Turning it off retains the preference for boosting muted colours while allowing stronger colourfulness and possible hue shifts. Saturation sliders use a symmetric displayed range with a stronger positive response.
- Crop photographs with corner and edge handles, a dragging grid, original or custom aspect ratios, and an orientation switch. Saved crops apply to previews and every export format. C selects Crop, H or Enter returns to Hand, and a double-click in Crop resets to the full photograph.
- Export JPEG, RGB8/RGB16 PNG and lossless or lossy JPEG XL alongside TIFF and DNG. JPEG XL offers adjustable quality and encoding effort, including its slow archival setting. Colour-managed exports preserve the selected destination space, including wide-gamut JPEG XL in ordinary image viewers. TIFF adds LZW and ZIP compression.
- Full, Fast and Smart previews share denoised rendering where reduced previews are supported. Smart refines visible detail before the full photograph, and Draft is removed.
- Merrill Compact speckle repair replaces all three layers with shared donor pixels. Its threshold slider now starts at 0.2. Neutralize and its three range controls share one visible group.
- Improve Color Field recovery at clipped highlight boundaries and correct sd Quattro sparse sites before reduced previews.
- Shared control sizes, responsive rows, compact menus and a centred photograph toolbar improve editing consistency. Local Contrast is labelled Clarity. Processing one selected photograph now enables Process when its recipe and destination are ready.
- Progress and latency move below the browser. A session Log retains file operations and errors. New errors highlight Log in red until it is opened, and successful rendering no longer clears their history.

### Alpha settings

Retired experimental settings and migration readers have been removed. Older editing sidecars or presets containing those fields may need to be reset.

## 0.6.0

<!-- Add your release introduction here. -->

### Changes

- Merrill Noise offers joint multiscale sensor-layer filtering, with Rows and Rows + Columns choices. Both reduce broad color blotches. Rows + Columns also applies broader shield-based correction and can change shadow tint. Reference remains the default.
- Independent Directional banding targets narrow lines and wider stripes in both directions, with Fine and Coarse strength and size controls. It works with either noise method in Color and Monochrome. Reference banding and its row-correction switch now live together in Noise.
- Train a User noise profile from lens-cap captures in the Noise section menu. Profiles learn noise and banding weights plus camera-specific hot-pixel locations, applied only to a matching camera serial. Provided weights remain available and cover untrained ISOs.
- Compact speckle cleanup offers threshold and 3, 5 or 7 pixel size choices for bright points and small clusters. Highlight protection uses original sensor values so white balance does not hide unclipped speckles from detection. Isolated near-clipped spikes in dark surroundings can also be repaired while supported highlights remain protected.
- Neutralize shadows selectively reduces green and magenta casts in dark, nearly gray regions. Strength starts at zero, with shadow, color and hue selectivity controls.
- Merrill Detail offers stronger Fine and Coarse amounts up to three. Improve weak-color protection during texture restoration and fix block artifacts from unmapped border weights.
- Faster Merrill row estimation, band-strength edits, shadow-neutrality edits and joint filtering through shared measurements, guarded buffer reuse and bounded active-photo caching. Bounded processing submissions reduce temporary GPU resource overlap during export.
- Merrill captures recorded as Monochrome open in Monochrome automatically. Switching to Color restores ordinary color processing. The layer mixer now uses a CIE chromaticity guide with a fixed dashed selection triangle.
- Browse photographs in grid, list or large views with star ratings, metadata sorting and search. Compare highlighted photographs while retaining one active editing session. Filename tooltips show capture details, and context menus offer copy-path and Finder actions.
- Save and apply editing-section presets and camera-family defaults. White balance adds warmth and tint adjustments, with transient Before comparisons independent of saved edits.
- Tone, Saturation and Color Range are separate editing sections with independent bypasses, resets, presets and processing choices. Universal Saturation offers gamut-aware Amount and Contrast plus overlapping Shadows, Midtones and Highlights adjustments. Reference saturation remains available independently of the Tone method.
- Edit subject colors directly in the photograph with the Color Range eyedropper or D. Drag horizontally for Hue and vertically for Lightness, together in one gesture. Option/Alt + vertical adjusts Saturation. Configure the axes and sensitivity from the adjacent settings button or by right-clicking the photograph. Escape exits and Space pans.
- Performance settings group thumbnail memory, decoded disk-cache size, background workers and look behind/ahead. Thumbnail retention uses a configurable memory budget, defaulting to 256 MiB, instead of a fixed image count.
- Unavailable destinations appear red. Workspace status and errors fit on one toolbar line with full details on hover. Destination availability is checked in the background.

## 0.5.0

<!-- Add your release introduction here. -->

### Changes

- Color Editor offers eight color ranges plus All Colors, with Hue, Saturation and Lightness controls. Monochrome shares the Lightness response while retaining its sensor-layer mixture. Existing six-color sensitivity edits migrate automatically.
- Shared photographic controls across Classic, Merrill and Quattro, including Quattro H: spatial HDR ranges, Local Contrast and selectable Universal sharpening. Reference methods remain available and retain their defaults.
- Merrill Monochrome now offers independent Fine and Coarse detail with size controls. Fix flat, patchy recovered highlights in its noise filtering while preserving bright texture.
- DNG can bake active Color Editor, HDR, Local Contrast and Universal sharpening while retaining camera tone presentation in its profile. Chromatic edits use the selected pre-tone rendition. Keep the X3F and sidecar to revise baked edits.
- Processing settings can include or omit each editing section, remembered independently for Browse/Import and DNG/TIFF. Submitted batches retain their frozen settings, and copied sidecars retain the full authored recipe.
- Editing sections can be reordered with compact drag handles. Color Editor has an independent checkbox below Tone and Color by default. Disabled controls retain their values, and section order is saved separately from photo edits.
- Correct linear inspection brightness and scopes across camera code scales, improve interactive checkpoint reuse, and keep numeric readouts within three significant digits.
- Group decoded-cache storage, previews and scheduling into one source module, preserving cache behavior and compatibility.

## 0.4.2

Usability improvements to make importing a better experience.

### Changes

- Browser thumbnails load visible photographs first, then prepare the rest of the folder without requiring scrolling. Larger previews no longer hold up the thumbnail queue.
- Selected-photo loading and rendering take priority over thumbnails, processing batches and speculative caching. Scrolling promotes newly visible thumbnails, and switching folders discards stale preview work.
- Copy the viewed photograph's settings with Command/Ctrl+Shift+C and paste with Command/Ctrl+Shift+V to all highlighted photographs, or the viewed photograph when none are highlighted. Browse saves edits asynchronously, while Import keeps them in memory until processing. Text and numeric fields retain their normal keyboard behavior.
- Fix a late preview-cache miss that could leave an evicted browser thumbnail blank.

## 0.4.1

<!-- Add your release introduction here. -->

### Changes

- Merrill Noise and Detail now have independent controls. Fine and Coarse detail each have an amount and size, with neutral defaults tuned for a gentler portrait rendering. Chroma and Luma cleanup no longer add the earlier detail contrast effect.
- Faster Merrill noise filtering and detail edits, with unnecessary filtering skipped for neutral or disabled controls. The same reconstruction serves Full, Fast, Smart viewport refinement, TIFF and DNG processing.
- One Detail change inspection view shows the combined Fine/Coarse brightness adjustment. Merrill Auto sharpening now starts at zero, with manual sharpening kept separate.
- Noise, Detail, Tone and Color, and Refinement can be bypassed while retaining their settings. Disabled controls stay visible. Compact label columns, brighter slider tracks, and grouped detail controls give sliders more room.
- Existing Merrill recipes retain their cleanup settings and use neutral detail defaults when the new texture controls are absent. Quattro retains its own reconstruction controls.
- Hold Option/Alt while navigating the browser with arrow keys to retain the zoom and viewport position across photographs. Shift+arrow extends the highlighted selection.
- Browse processing handles all highlighted photographs with frozen edits. Processing settings offer Overwrite or Rename on conflicts, defaulting to Overwrite for rendered files, copied originals and their sidecars.
- DNG exports preserve available capture details, including date, camera, lens, exposure, aperture and ISO.
- Background cache decoding cancels obsolete queued work when the selection moves. Interactive work and submitted processing take priority over cache maintenance, and batches avoid filling the decode cache unnecessarily.
- Support X3F 3.1 headers consistently across RAW decoding, metadata and preview reads.

Apple Silicon downloads target macOS 15 or later. They are Developer ID signed and unnotarized. First launch may require **Open Anyway** in macOS Privacy & Security settings. Interactive display behavior on macOS 15 and across multiple monitors still needs user testing.

## 0.4.0

<!-- Add your release introduction here. -->

### Changes

- Merrill Smart now renders the zoomed viewport at Full quality before refining the whole photograph. The completed Full image stays ready for panning, with shared sensor corrections and noise context reused between stages.
- Faster Merrill processing at tile boundaries and better reuse of earlier processing when editing Smart previews.
- Standalone Fast keeps Noise and Detail enabled when requested, processing at quarter resolution. Smart starts with a lighter Draft recipe that skips optional Noise and Detail before Full refinement.
- Color-managed display on macOS uses the monitor's selected profile across the viewport, navigator and browser previews, preserving colors beyond sRGB on capable displays.
- Application settings offer Monitor, sRGB, Adobe RGB and ProPhoto RGB (ROMM) viewing limits. Switching the display gamut redraws immediately without reprocessing the RAW or changing exports.
- Embedded previews honor camera color-space metadata and valid embedded ICC profiles across supported camera architectures. Rendered previews retain their gamut in the temporary cache, regardless of the selected viewing limit.
- A ready decoded-photo cache now includes metadata and embedded preview inputs, avoiding additional source-file reads when opening a cached photograph. This reduces waiting on SD cards and other slow sources.

Apple Silicon downloads target macOS 15 or later. They are Developer ID signed and unnotarized. First launch may require **Open Anyway** in macOS Privacy & Security settings. Interactive display behavior on macOS 15 and across multiple monitors still needs user testing.

## 0.3.0

So the two big changes here are
1. Added the first iteration of the Import mode, which does not save any sidecar data to the loaded directory, and offers "already imported" hiding if you tend to not format your cards very often.  Expect this to change as I refine my taste here.
2. Merrill now supports its own implementation of Smart preview.  Currently it does not do viewport rendering, only 1/4 res with noise and detail reconstruction off, and Full res with it on upon zoom.

Also I am retargeting to support macOS 15, but unsure if it actually works so it will need testing.

A guided walkthru mode and better user facing documentation will come later once the experience starts to stabilize.

### Changes

- Apple Silicon builds now target macOS 15 Sequoia or later, with compatible icon resources. Sequoia runtime testing is still pending user feedback.
- Browse and Import open folders recursively, with photographs grouped by subfolder.
- Import mode keeps source cards and folders free of editing writes. Pick photographs, hide rejects, and remember imported or hidden files at the destination for the next review.
- One Process action uses saved settings for DNG, TIFF and TIFF color space. Import can copy originals directly or process them and keep originals and edits in an `x3f` subfolder.
- Background batches freeze the submitted edits and yield to interactive browsing and rendering.
- Decoded-photo caching follows a configurable neighborhood around the selected photograph, defaulting to three behind and seven ahead. A configurable pool decodes sensor layers in photo order, with lower priority than interactive work.
- Fast and Smart previews now support Merrill color photographs. Fast uses filtered half-width, half-height previews, and Smart refines the whole image to Full above 100% zoom for detail and panning.
- Merrill Fast skips row correction and Noise and Detail while preserving the saved controls for Full. New Merrill photographs start with Noise and Detail enabled, and existing saved choices remain unchanged.
- Faster Merrill GPU row correction, clearer cache readiness, and consistent browser picking and hiding shortcuts across the workspace.

Apple Silicon downloads target macOS 15 or later. They are Developer ID signed and unnotarized. First launch may require **Open Anyway** in macOS Privacy & Security settings.

## 0.2.0

The big change here is support for the other sensor architectures.  I'm trying out a bunch of optimization techniques to get latency down as much as possible.  The common approach is that we are progressively rendering and caching.  A new smart rendering mode will open the image in a 1/4 downscale, a double click will then render the full size for the viewport only, and then immediately render the full size for the whole sensor.  This makes interacting with quattro and quattro h files really snappy.  We also decode the files to the cache ahead of time, which shaves off about 400-600ms (on an M1 Pro) at the cost of using temporary directory space.  This gets cleared by your system when you restart, so you don't have to worry about clearing it out.

### Changes

- Classic camera support with color and monochrome rendering, TIFF export, and experimental editable DNG export.
- Quattro and Quattro H color and dedicated monochrome rendering, TIFF export, and experimental editable DNG export.
- Fast and progressive Smart previews for Quattro and Quattro H, clearer rendering progress, and a configurable decoded-photo disk cache.
- Local and Global Color Field highlight recovery for Merrill, with deterministic GPU processing and a shared Highlights control.
- Embedded X3F thumbnails, faster Classic Auto white balance, and faster Quattro repair preparation.
- Developer ID signing for Apple Silicon downloads on macOS 26 or later. Downloads are not notarized and may require **Open Anyway** in macOS Privacy & Security settings on first launch.

## 0.1.1

Small changes here.  Added the color space export UI element if you wanted to export in a space different from what you selected in camera.  Spacing around buttons has also been improved.

### Changes

- Grouped Open and TIFF export controls, with TIFF color-space selection beside Export TIFF.
- More consistent spacing, centered file-count and zoom controls, and compact dropdowns with clear disclosure arrows.
- TIFF export in sRGB, Adobe RGB, or ProPhoto RGB with matching embedded profiles. The camera's stored space remains the default.
- Camera-derived Auto sharpening alongside manual amounts and Off. Existing settings retain their previous behavior.
- Higher-quality TIFF resizing using the reference implementation's photographic filter instead of bilinear sampling.

## 0.1.0

This is the first release that covers the core functinality.  The focus in on replicating the original Merril look we are all used to.  There are some slight processing improvements like the Highlight correction having a brightness inclusion threshold, which helps to avoid artifacts.

Notable features are (some) hover tool tips, DNG Support with Dng Camera Profile based Color Mode implementation.  Both a CLI and GUI backed by the same WebGPU shader rendering pipeline.  

The UI is a bit spartan and lacks polish, right now I'm mostly focused on basic functional parity.

### Changes

- GPU processing for Merrill X3F photographs through a desktop editor and batch CLI.
- Color and monochrome rendering, including a sensor-layer monochrome mixer.
- Noise and detail controls, sharpening, highlight restoration, bad-pixel repair, speckle removal, and banding correction.
- Automatic editing sidecars, folder browsing, navigator, scopes, and pipeline inspection with input/output and difference views.
- TIFF export and experimental linear DNG export.
- Embedded photographic help and downloadable example photographs.
- Native macOS light/dark icons, Cargo-based app packaging, and unsigned Apple Silicon release builds for macOS 26 or later.
