---
summary: "Versioned application changes supply the initial notes for draft GitHub Releases."
kind: reference
---

# Changelog

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
