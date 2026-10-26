---
summary: "Versioned application changes supply the initial notes for draft GitHub Releases."
kind: reference
---

# Changelog

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
