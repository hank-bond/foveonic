---
summary: "User cap calibration must distinguish general noise distributions from camera-specific sparse pixel defects."
kind: reference
status: accepted
implementation: incomplete
---

# Sensor defect calibration

A calibration workflow should let users select lens-cap captures and build calibration data in the app. Captures must be grouped by camera model and the `CAMSERIAL` metadata value, ISO and shutter time, with at least seven frames per group. Fourteen frames is the recommended two-buffer capture set. Insufficient or mixed groups must be identified before building a profile.

Fixed hot-pixel locations and their layer offsets belong to the individual camera. A body-specific sparse defect map must never be distributed or applied as a universal Merrill defect map. A generalized noise distribution model may be shipped separately when independent photographic and held-out noise checks support its scope. A camera-specific profile must store its camera serial and match it to the current capture before applying fixed defect locations. A missing or mismatched serial must leave that sparse map inactive. Generalized training must retain every source camera serial as provenance and for evaluation across bodies, without restricting the resulting generalized noise model to those serials. Both kinds of data must retain the capture-condition provenance needed to determine applicability.

Existing camera defect metadata must be evaluated first. Its records identify affected native layers and repair neighborhoods, not the rendered color of a defect. Calibration must distinguish unlisted defects from incompletely corrected listed defects. Observed recurrence in a selected sample does not establish that all visible speckles are persistent. Calibration must distinguish repeatable pixel offsets from transient outliers and ordinary signal-dependent noise. Sparse correction must preserve ordinary grain, real fine detail and unlisted pixels. Residual speckle correction must offer separate threshold and size controls for defects that have no matching calibration and for transient outliers. Those controls must remain distinct from the calibrated sparse map and must preserve fine scene structure. Blurred dust marks are outside this correction.

The workflow interface, profile storage, supported shutter interpolation, and application rules require a separate implementation design. This requirement does not make the current research camera a universal hot-pixel reference.

Compact speckle Size must offer only the supported native-pixel diameters 3, 5 and 7 as explicit choices. The control must not accept even diameters and then silently round them.
