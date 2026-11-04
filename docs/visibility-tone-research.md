---
summary: "Visibility-oriented tone research separates useful contrast, spatial compression, noise and color appearance for an isolated Auto HDR experiment."
kind: reference
---

# Visible detail and adaptive tone

The owner wants a starting rendition that makes more recorded signal and detail visible, without greatly brightening deep shadows or replacing artistic decisions. The next experiment is a separate spatial step. Exposure and the existing HDR sliders keep their meanings. Automatic exposure is outside this slice. The rejected fixed-curve findings remain in the [highlight-retention plan](../plans/archived/highlight-retention.md).

## Research principles

[Mantiuk, Daly and Kerofsky, Display Adaptive Tone Mapping (2008)](https://www.cl.cam.ac.uk/~rkm38/pdfs/mantiuk08datm.pdf) optimize visible contrast distortion subject to a display model. Their formulation includes luminance adaptation, spatial contrast sensitivity and masking. It supports evaluating contrast visibility rather than simply spreading a histogram. Their metric is display-dependent. An uncalibrated heuristic must not be presented as the paper's perceptual model.

[Durand and Dorsey, Fast Bilateral Filtering for the Display of High-Dynamic-Range Images (2002)](https://people.csail.mit.edu/fredo/PUBLI/Siggraph2002/DurandBilateral.pdf) separate broad illumination from detail using an edge-preserving base. Compressing that base retains more local texture. This supports reusing the project's existing multiscale HDR guide and coordinated gain response, rather than compressing every pixel through another global shoulder.

[Eilertsen, Mantiuk and Unger, Real-time Noise-aware Tone Mapping (2015)](https://computergraphics.on.liu.se/rntm/) explicitly considers noise visibility, local tone curves and detail reproduction. Increasing the visibility of noise is not equivalent to revealing useful scene detail. The experiment begins with already corrected RGB and a coarse cross-scale structure estimate. That estimate is not a calibrated noise model.

Section 4.1 of the [paper](https://computergraphics.on.liu.se/rntm/material/ttmo2015.pdf) estimates local contrast from Gaussian-window log-luminance variance and weights histogram contributions by contrast above the estimated noise level. It identifies uniform surfaces and noisy dark regions as cases where pixel frequency poorly represents importance. This supports evaluating useful structure locally. It does not establish a preference for dimming featureless reflections. That preference needs its own photographic objective.

[Mantiuk et al., Color Correction for Tone Mapping (2009)](https://www.cl.cam.ac.uk/~rkm38/pdfs/mantiuk09cctm.pdf) measure changes in colorfulness after contrast changes and propose nonlinear saturation correction. Preserving RGB ratios does not by itself preserve color appearance. [Darktable's AgX documentation](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/agx/) describes controlled highlight color transitions toward white, including desaturation. Visible color can require reducing brightness. Boosting saturation cannot preserve arbitrary colorfulness at display white.

## Experiment rather than a new default

The first candidate scores a bounded set of existing HDR responses against a whole-photo summary of broad brightness, residual contrast and channel headroom. It rewards making structure visible, penalizes clipped detail, restrains changes to already useful tones and limits shadow amplification. The display assumptions are fixed SDR thresholds, not measurements of the owner's monitor or viewing conditions.

Compare two conservative scoring strengths and one restrained perceptual-color variant. Keep a camera baseline, the rejected curve and manual HDR examples as controls. Use photographic toggles across both supplied portraits and the example photographs. Evaluate skin, bright fabric, foliage, saturated objects, dark backgrounds and specular reflections. Aggregate scores are diagnostics, not proof of photographic preference.

The [experiment plan](../plans/visibility-tone-experiment.md) records the processing slice, measurements and review. Application integration, regional caching and final interface choices follow photographic evaluation.

## Regional measurements and bright distractions

Measuring local structure and choosing independently varying local corrections are separate decisions. One possible incremental experiment would retain the existing spatial HDR response and whole-photo parameter selection while scoring overlapping regions at several scales. Regional aggregation could give a small coherent highlight with recoverable gradation adequate influence without assigning excessive importance to a single bright sample. Actual cost and photographic effects would need measurement.

The owner also often reduces Whites strongly to make bright reflections less distracting. Restoring visible gradation and reducing bright visual competition are related but distinct aims. A flat bright region may have little measured contrast and still dominate attention. A candidate objective should therefore consider local brightness relative to surroundings alongside noise-aware structure and displayed contrast. This is a proposed expression of the owner's rendering preference, not a universal perceptual optimum.

Display clipping can hide differences that remain in recovered linear RGB. Lowering the response can reveal those differences. Where all usable sensor information is lost and reconstruction supplies no gradation, a tone operation can lower the reflection's brightness but cannot recover missing scene detail. The evaluation should distinguish retained linear variation, reconstructed variation and a genuinely uniform highlight.
