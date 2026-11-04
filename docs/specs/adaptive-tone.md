---
summary: "Merrill regional base responses fit useful detail visibility with shared tuning limits independently of artistic tone edits."
kind: reference
status: accepted
implementation: incomplete
---

# Regional adaptive base response

Offer Camera, Natural and Strong under Merrill Tone's Base response choice. Natural and Strong use the reviewed regional estimator and replace the rejected fixed Highlight retention curve. Camera remains the default and must preserve existing processing. Color and Monochrome use their established reconstruction routes.

Analyze corrected, calibrated RGB after the selected camera white balance and Color mode, using that mode's neutral camera tone curve and the user's Exposure to predict display visibility. Exposure changes must refit the automatic response while reusing its pre-exposure spatial guide. Apply only the fitted spatial gain at this boundary. Exposure must still be applied exactly once through its established appearance route. Artistic HDR, saturation and color-range controls retain their existing values and meanings. Tone bypass must restore Camera without overwriting the saved choice. No automatic exposure or additional saturation boost belongs in this slice.

Retain regional brightness/detail scoring and protection against losing useful shadows. Region membership and support must vary smoothly with Exposure. Natural uses a global scene fit as its shadow reference. Strong uses Regional Natural as its reference. Scoring penalizes losses of sampled shadow brightness and contrast. After capping, dark gain cells retain their feasible reference gain, with a smooth transition to the ordinary correction above shadows. Strong protects the Natural correction where its caps permit that correction. Crossing the profiles' caps can weaken this relationship. This is not a guarantee of contrast at every pixel. Noise and detail feature estimation remains in RGB. Do not add sensor residual dependencies.

Expose separate Natural and Strong sets of maximum Shadows, Highlights and Whites amounts in application Settings, shared across photographs. Initial tuning defaults are Natural 0.45, 0.45 and 1, and Strong 0.75, 0.65 and 1. Each is bounded to 0–1 and denotes an HDR amount, not EV. These limits must not be stored in photo sidecars, copied as photo edits or captured by per-photo presets. Snapshot both profiles' current tuning into every render and export request. Only the selected base response is a photo edit.

Add a restrained automatic Blacks contribution to protect useful detail that remains near black after the general correction. Shadows remains responsible for broader dark-tone adjustment. Estimate this contribution locally from the applied response, selected camera tone and Exposure, using existing structure evidence. It must fade out as detail becomes visible, preserve exact black and avoid a uniform offset or a flat minimum brightness. Flat or unreliable near-black samples must not trigger the contribution. Initial shared per-profile Black caps are 0.03 for Natural and 0.06 for Strong, bounded to 0–1 like the other HDR amounts. Black cap zero must reproduce the existing response exactly. Manual Blacks and Shadows retain their existing meanings.

Fit scene targets on a fixed domain with refinement between coarse trials. Profile caps restrict the fitted amounts afterward. Moving a cap must not relocate the trial lattice or change other estimated amounts. Avoid abrupt selection between almost equally scored, substantially different responses. Natural aims for a restrained useful baseline. Strong aims for greater visibility and may use greater corrections. Photos that already meet both targets may render alike. A visible difference is expected where a stronger response offers useful correction, rather than forced for every photo. Compare profile outputs and cap hits over the photographic set during tuning.

The first app trial uses full-resolution whole-photo analysis whenever adaptive response is active. Pan, zoom and viewport crops must not change its fit. Fast/Smart may use the Full route for this choice until a separately reviewed approximation exists. Direct unsupported regional-processing requests must not silently fit the viewport. Original sensor clipping provenance and highlight recovery remain upstream and unchanged.

Use the existing typed graph and bounded active-photo cache. RGB input, geometry, luminance definition and guide parameters determine guide reuse. The scene fit additionally depends on the predictive mode curve, response profile, Exposure and estimator version. The applied response also depends on the profile caps. The combined node may refit on cap changes initially. Downstream artistic edits must not trigger a new baseline fit when they do not feed that boundary. Keep analysis and application on GPU without a required host readback. Broader optimization and approximate reuse across changed upstream settings are outside this slice.

During shared-limit tuning, adaptive display previews remain in memory. Persistent preview publication must include the shared limits in its identity before it is enabled for these profiles.

TIFF and DNG exports must apply the selected spatial response. DNG bakes its gain through the existing shared linear-edit route and retains the ordinary camera tone as appearance. Do not retain the fixed-curve prototype's unchanged-technical-sample requirement for adaptive output.

Validate unchanged Camera output, expected estimator defaults, limit changes, Tone bypass, cache invalidation and reuse, Full/Fast policy, Color/Monochrome, retained input ownership and photographic TIFF/DNG behavior. Build and open a local app for owner tuning without installing, publishing or changing global photographic defaults.

The [integration plan](../../plans/adaptive-tone.md) tracks implementation. Scientific references and exploratory findings remain in the [research reference](../visibility-tone-research.md) and [experiment plan](../../plans/visibility-tone-experiment.md).
