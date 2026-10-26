---
summary: "Shared photographic finishing offers universal tone, monochrome sensitivity and sharpening beside existing camera-derived methods."
kind: reference
status: accepted
implementation: conforming
---

# Photographic finishing

Camera recipes must retain sensor calibration, corrections, noise cleanup, reconstruction and base camera appearance. Photographic operations with matching numerical meanings should be reusable across architectures. Existing methods and defaults must remain available while the owner evaluates alternatives.

Universal tone must offer consistent contrast and saturation independent of the camera tone implementation. Positive saturation should restrain already colorful samples. Negative saturation should remain ordinary desaturation. RAW exposure retains its existing interpretation. The approved spatial [HDR response](./tonal-ranges.md) must become available across photographic architectures. The tone-method choice must not switch HDR algorithms or move the camera adapter's HDR boundary. Merrill retains the reviewed pre-curve HDR placement. Classic and Quattro adapt normalized photographic RGB to the same response math. Existing clipping in a camera appearance cannot be undone by a later photographic adjustment. New photographic behavior applies to preview and TIFF. Existing DNG behavior remains unchanged for this evaluation.

Across Classic, Merrill and Quattro, Reference tone must expose the original Shadows and Highlights, and bypass the four HDR ranges and shared Local Contrast. Universal must expose Blacks, Shadows, Highlights, Whites and Local Contrast, and bypass the original Shadows and Highlights. Exposure, contrast and saturation remain common. Method changes retain the inactive values without applying hidden edits. This choice does not change either HDR implementation or its processing position.

Monochrome must retain its authored layer mixture and reconstruction. Red, Yellow, Green, Cyan, Blue and Magenta sensitivity controls must identify subject colors from an independent white-balanced color guide before monochrome mixing and creative saturation. Brightness adjustments apply to the resulting monochrome image. Hue transitions must be smooth and nearly achromatic pixels protected. Zero sensitivity must avoid the guide branch. Guide generation must reuse the normal source and rendering operations, remain independent of the selected monochrome mixture, and participate in bounded interactive reuse.

Sharpening must offer Reference and Universal methods on all families. Universal must replace reference creative sharpening contributions and offer Amount, Radius, Threshold and Halo suppression. Necessary reconstruction remains independent. Neutral-centered multiscale Local Contrast is an accepted standalone photographic effect. It must remain distinct from denoising, reconstruction detail and sharpening, rather than replace any of them.

Quattro Smooth/Crisp combines residual restoration, local contrast and adaptive sharpening. Shared Local Contrast must not be presented as an equivalent reconstruction method. The owner accepted its distinct look after the comparison. Reference remains the default method. Its sharpening amount defaults to Auto when no amount is saved. Explicit saved manual amounts remain unchanged. Universal sharpening uses manual controls without Auto.

All pixel operations run on the GPU through the existing typed graph and executor. Inspection, per-image persistence, section bypasses, frozen batch requests, Fast and Smart must share the same settings. Spatial operations must use sufficient regional support or explicitly run over a larger frame. Neutral operations must skip their work and preserve samples. Retained outputs remain immutable.

Section headers must show their supported export formats at the right: DNG | TIFF for Reference controls and TIFF for Universal finishing. Sharpening uses Method and Amount labels. Highlight correction and its Threshold belong in one parameter group. Indented text must use half the previous gap after its pipe without changing checkbox indentation.

Photographic numeric readouts must show at most three significant digits while retaining full stored precision. Formatting and numeric-field sizing must agree so value changes do not expand the editor panel. Very small or large magnitudes may use compact scientific notation.

Review must cover portraits, hair, fabric, foliage, sky and shadows. Validate arithmetic, finite headroom, neutral/reference output, cached/fresh agreement, regional agreement, TIFF inclusion and unchanged DNG output. Report measured responsiveness and remaining visual decisions before selecting new defaults.
