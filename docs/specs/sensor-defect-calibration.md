---
summary: "User cap calibration must distinguish general noise distributions from camera-specific sparse pixel defects."
kind: reference
status: accepted
implementation: conforming
---

# Sensor defect calibration

A calibration workflow should let users select lens-cap captures and build calibration data in the app. Captures must be grouped by camera model and the `CAMSERIAL` metadata value, ISO and shutter time, with at least seven frames per group. Fourteen frames is the recommended two-buffer capture set. Insufficient or mixed groups must be identified before building a profile.

Fixed hot-pixel locations and their layer offsets belong to the individual camera. A body-specific sparse defect map must never be distributed or applied as a universal Merrill defect map. A generalized noise calibration profile may be shipped separately when independent photographic and held-out noise checks support its scope. A camera-specific profile must store its camera serial and match it to the current capture before applying fixed defect locations. A missing or mismatched serial must leave that sparse map inactive. Generalized training must retain every source camera serial as provenance and for evaluation across bodies, without restricting the resulting generalized noise profile to those serials. Both kinds of data must retain the capture-condition provenance needed to determine applicability.

Existing camera defect metadata must be evaluated first. Its records identify affected native layers and repair neighborhoods, not the rendered color of a defect. Calibration must distinguish unlisted defects from incompletely corrected listed defects. Observed recurrence in a selected sample does not establish that all visible speckles are persistent. Calibration must distinguish repeatable pixel offsets from transient outliers and ordinary signal-dependent noise. Sparse correction must preserve ordinary grain, real fine detail and unlisted pixels. Residual speckle correction must offer separate threshold and size controls for defects that have no matching calibration and for transient outliers. Those controls must remain distinct from the calibrated sparse map and must preserve fine scene structure. Blurred dust marks are outside this correction.

The active workflow groups training observations by exposure condition, then shares learned locations within each camera serial and native geometry. A learned location must still be a positive local outlier in the current shot before repair. ISO and shutter matching must not be required merely to locate a physical defect. Exposure provenance remains available for evaluating its strength.

## User calibration workflow

The Noise section menu must open a modal for cap calibration. Users must be able to add individual X3Fs or directories. A table must show camera model, ISO, shutter speed and count, with serial identity retained for separate bodies. Loading and untrained entries must show a yellow status circle. Train must fit all current entries, and each successfully completed group must become green. File loading and training must run off the UI thread, with visible progress and errors. Groups with unavailable source files must show a red circle and identify the missing count. Train must remain disabled until those files are restored or their groups removed. The open modal must recheck availability off-thread so card reconnection updates the status. Calibration guidance must use the normal body text size. The modal must reserve its training space so its dimensions stay stable during loading and training. User-facing text must call the calibration a profile, reserving model for neural networks or other explicitly mathematical uses.

Calibration imports must use the same capture metadata reader and cache as the browser and viewport. The table must use their shared photographic formatters, including the camera's shutter fraction from `SH_DESC` when available. Numeric exposure values remain the training group identity. Saved readout metadata must remain available when the source card is disconnected.

The app must persist one User calibration slot alongside the immutable Provided slot. The selected slot must apply consistently to editing and export, with correct cache invalidation. Training failure must leave the existing usable slot intact. Capture paths and training identities must remain available for review and retraining. At least seven caps per ISO/shutter combination are required, with fourteen recommended.

Camera-specific learned locations must match the recorded camera serial before application. A missing or different serial must never activate a sparse map. General noise assumptions and sensor-specific defects must retain separate application rules. The workflow does not add neural training or a temperature model. Untrained ISOs must retain the Provided profile. Learned locations remain camera-specific across exposure conditions.

Compact speckle Size must offer only the supported native-pixel diameters 3, 5 and 7 as explicit choices. The control must not accept even diameters and then silently round them.

The profile picker must distinguish Provided correction weights from User weights learned from cap captures. Use bold category labels followed by colons. Keep the modal wide enough for readable guidance. Buttons and dropdowns must use the shared padded control sizing and align vertically within their rows. Training progress and its completion message must share one compact status row above the footer. User weights may better match the sampled cameras. User profiles additionally store camera-specific hot-pixel locations. With Speckles enabled, a matching serial activates that additional location-based correction alongside the selected speckle filter. Provided never supplies camera-specific defect locations. Noise and banding weights at untrained ISOs use Provided, independently of serial-matched locations.
