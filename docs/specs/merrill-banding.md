---
summary: "Merrill banding correction targets sensor-aligned offsets across multiple row and column widths while retaining natural grain and real color."
kind: reference
status: accepted
implementation: conforming
---

# Merrill banding correction

This requirement extends [Merrill multiscale noise](./merrill-multiscale-noise.md). Banding correction must address coherent native sensor row and column disturbances across several spatial widths. Single-pixel lines and broader, multi-pixel stripes are both in scope. The disturbances need not repeat periodically.

## Appearance and processing

The objective is more even shadow appearance with useful color, detail, and natural independent brightness grain retained. Fine grain is acceptable. Stronger fine-noise suppression is not a substitute for correcting wider bands. Broad image gradients and real colored structures must retain their appearance.

Image corrections must operate on native sensor layers before color conversion and run on the GPU. The three layers must be considered jointly. A correction must not introduce a hue error through inconsistent decisions about individual layers.

Row and column correction should share an estimator formulation so their behavior can be compared directly. Their measured coefficients and uncertainty may differ. Width coverage and applied strength must be assessed independently. Correction fields must remain continuous where separate spatial estimates meet.

Capture-derived noise distributions may inform confidence and strength. A measured cap mean is not sufficient justification for directly subtracting a spatial field from every photograph. Sensor position, signal level, camera variation, and capture conditions must be considered when interpreting the calibration. Validation on held-out frames from one sequence does not establish transfer to a different camera or session.

## Comparison and integration

Trials must isolate rows only, columns only, and their combination at matched spatial scales. They must preserve an unchanged control with the same white balance, color, exposure, texture, and existing noise settings. A previously accepted column improvement must remain visible when row correction is extended.

Visual review must include dark leaf backgrounds at ISO 800, smooth regions near sensor edges, the top of a car hatch, and deep regions below a desk. Review must distinguish absolute changes in a row pattern from increased visibility after perpendicular column noise is reduced. Fine lines, wider stripes, color casts, real edges, and remaining grain require separate assessment. Lower numerical noise alone is not acceptance.

Sensor axes must retain native coordinates through orientation, full-frame processing, and regional previews. The selected technical correction must be consistent between photographic and editable DNG output. Directional banding must remain independently selectable from the general denoiser. Reference remains the application default.

## Directional estimation

Directional correction must complete Fine columns and Fine rows before measuring residual Coarse bands. Coarse must estimate the remaining wide offset directly, rather than subtracting an earlier Fine estimate. A zero Coarse strength must skip its measurement and application work.

Coarse must pool local residual estimates along the same native row or column with a triangular window of radius 640 pixels. Its confidence must decrease where a short offset profile matches better after a consistent diagonal shift than at fixed sensor coordinates. This preserves slowly varying band amplitude while reducing corrections that follow diagonal scene structure. Interpolation between fitted estimates must avoid the former 40-pixel steps. Genuine low-contrast features aligned with a sensor axis remain ambiguous to this image-based estimator.

Full, Fast, and regional views must share the same complete native correction field. Regional views must select existing native tiles with their original halo and phase. Fast may reduce the corrected native layers after estimation. Measurement and application must have separate graph identities so changing a nonzero strength can reuse that band's measured offsets. Changing Fine must invalidate subsequent Coarse estimates because they measure the Fine-corrected residual.

## Interactive controls

The app must expose independent size and strength controls for Fine and Coarse bands in both directions, nested under Banding in the Merrill Noise section. Directional band refinement must be independently enabled with either the reference or multiscale denoiser. The shared Noise bypass must disable both while retaining their choices. Direction labels must describe horizontal and vertical bands in the displayed orientation. Stored controls must remain in native sensor coordinates.

Noise must group Chroma and Luma before a Banding subsection with its own enable switch and Reference or Directional method. Reference must show the existing banding amount and row-correction checkbox. Directional must show the independent axis bands. The reference row checkbox belongs to the Noise editing scope and must leave Corrections unchanged. Switching banding methods must retain their inactive controls and select the corresponding banding passes without applying both sets. Banding strength scales the measured three-layer error, including brightness and color components, independently of the general Luma and Chroma amounts.

The starting result must retain the accepted narrow local correction on both axes. Its default size is 27 native pixels and its strength is one. Coarse starts at 119 pixels with zero strength, so wider correction remains opt-in. Strength must vary continuously from zero through three. Sizes must be odd, ordered from Fine to Coarse, and bounded by supported filter context. Size must appear directly in each band group without additional indentation. The two-band format replaces the experimental three-band format without compatibility parsing.

Fine strengths through one must retain their existing interpolation and unit-strength clipping behavior. Above one, additional strength must scale only the accepted band offset. It must not amplify the black-floor or upper-clipping adjustment. A constant signed field without banding must not acquire a color bias as Fine strength increases above one.

Each spatial band's estimate must remain distinct so strength tuning does not repeatedly apply the same fine correction. Zero added strength must preserve the narrow baseline. Inactive values must survive Noise bypass, method changes, sidecar round trips, defaults, and Noise-scoped copy and reset. With directional refinement disabled, the reference method and other sensor families must retain their existing behavior.

Wider supports must exclude inactive sensor margins from their measurement. Photographic comparisons must assess fine-band compatibility separately from changes to physical-edge handling.

Banding choices and their enable state must work in Color and Monochrome. Turning Banding off must preserve valid signed sensor samples and must not create extreme colored points during texture restoration. Color proportions derived from unfiltered layers must use their nonnegative signal components with a protected normalization denominator. This must not change the accepted Banding-on rendering.

The CLI accepts `--merrill-banding-method reference|directional`. Selecting either method enables banding and preserves its row and column controls.
