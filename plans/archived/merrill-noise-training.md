---
summary: "Implemented user cap calibration with grouped review, fitted noise profiles and serial-matched pixel correction."
status: archived
---

# Merrill noise training

This completed slice adds Rust cap calibration after optimization checkpoint `b485cab`. Neural training remains separate. It implements [noise requirements](../../docs/specs/merrill-multiscale-noise.md), [sensor defect calibration](../../docs/specs/sensor-defect-calibration.md), [render resources](../../docs/specs/render-resources.md) and [the editor workflow](../../docs/specs/editor-workflow.md).

## Agreed workflow

- A command in the Noise section's hamburger menu opens a calibration modal.
- Add Files and Add Directory load Merrill X3F captures off-thread. Show camera model, ISO, shutter speed and count for each combination. Preserve serial identity when grouping captures from multiple bodies of the same model.
- New or pending captures use a yellow status circle. Train fits all table entries. Completed rows turn green. Show useful progress and errors without blocking the editor.
- Guidance is at least seven caps per ISO/shutter combination, with fourteen recommended. Deduplicate paths and preserve the source files.
- Persist one user slot and retain the bundled Provided slot. Users can switch between them. Failed training must preserve the previously usable profile.
- Fit general cap-noise relationships from the selected captures. Store learned values as profile data, including capture identities and conditions.
- Learned fixed pixel locations require an exact camera serial match. Never transfer a sparse defect map between cameras. Missing identity must not enable camera-specific defects.
- Train sequentially with bounded memory on the 16 GiB Mac. Preserve the original decoded samples and use GPU image reductions. Keep numerical fitting and small tables in Rust.
- Build and test the complete workflow and prepare a separate review app for the morning. The owner reviewed the workflow and requested a commit. Publication remains separate.

## Work

- [x] Commit the optimization and fitted-data extraction checkpoint.
- [x] Map the existing regression, covariance and defect experiments to a maintained core trainer.
- [x] Add validated profile persistence and explicit immutable processing selection.
- [x] Fit shield coefficients and row priors, and identify repeatable per-camera defects from cap groups.
- [x] Integrate user/provided selection with rendering, cache identity and exports.
- [x] Add the modal, grouped loading/training status and persisted single-slot workflow.
- [x] Exercise real captures, serial isolation, synthetic statistical checks and native UI.
- [x] Record validation and remaining limits, then prepare the review build.

## Implementation boundaries

The shipped default currently uses binary shield tables plus JSON metadata for its reference axis and nearest-ISO covariance priors. The new workflow must retain that default unchanged. Camera `ColorNoiseModel` coefficients continue to come from the X3F metadata. Dark caps alone do not identify the signal-dependent shot-noise terms.

The prior research used equal capture-condition weights, ridge regression on all three native layers, mirror-reflected Gaussian bands and temporal centering for the broader column component. Row covariance priors came from fine active-row offsets grouped by ISO. The production trainer must document which fitted components it produces and validate them against small independent equations. No claim of held-out quality follows from training completion.


## Measurements and application rules

A 272-capture run across three camera bodies, twenty camera/ISO/shutter groups and ISO 100–1600 completed in 229 seconds on a 16 GiB M1 Pro. Its measured process footprint peaked at 1.61 GiB. GPU reductions retain the full-size running mean and temporal variance for one condition at a time. Rust fits small line-profile tables. A seven-capture group also completed independently.

The initial eight-code defect threshold included too much fixed-pattern grain. The selected detector requires a positive local offset exceeding 32 native codes, six temporal standard deviations and eight standard errors of the mean. Single-frame spikes fail the repeatability test. These constants describe the detector, not fitted calibration values.

Per-condition detection confirms shared physical locations. At 1/60 s, more than 99% of ISO 100 detected pixel-layer locations recurred at ISO 200 and 400 on the two bodies with those three groups. Very short captures revealed none under the stronger threshold. Higher ISO detections were mostly subsets. The trainer therefore merges locations by camera model, serial and sensor geometry. Runtime repair also requires a current positive local outlier, preserving quiet locations in other exposures. Original clipping evidence remains immutable, with a second check for mapped outliers restored by the multiscale clipping guard.

The User artifact contains capture paths and conditions, all contributing serials, fitted shield coefficients, column reference curves, per-ISO covariance priors and merged sparse maps. General coefficients use equal condition weights. A pedestal state absent from training retains the corresponding Provided tables. An ISO absent from training uses the whole Provided profile. Sparse maps use independent body matching and follow the Speckles switch for both Color and Monochrome.

The list and usable User artifact are separate atomic files in application storage. Loading additions does not replace the active artifact. Each editor request and export batch freezes its selected profile. Render keys contain the actual selected parameters. Rendered preview headers include the profile identity, while embedded previews and decoded original samples remain reusable.


## Review checkpoint

The ordinary workspace suite passes 686 tests. Focused device checks cover cap moments and profile reductions, persistent-versus-transient defects, quiet mapped locations, serial isolation, Noise/Speckles bypasses and exact cached profile switching. Existing shield and row confidence oracles pass. The historical frozen-row fixture was unavailable for this run. A full-resolution native/final float comparison and actual TIFF against the committed optimization binary were byte-identical for Provided.

The native review app exercised Add Directory, Add Files, minimum group counts, yellow pending status, successful fitting, green completion, User activation, restart persistence, Provided/User switching and cancellation. Cancelling a full retrain preserved the saved User artifact byte-for-byte. A separate three-body run produced the current 272-capture User profile in 218 seconds, with a 1.62 GiB measured peak process footprint. This is calibration workflow validation. It does not establish that a newly trained profile improves every photograph.

Clippy with warnings denied, formatting, generated documentation checks and five artwork tests pass. The feature and UI refinements were reviewed in an isolated worktree. The review bundle uses its own application identity and storage. Bundled Provided data and ordinary application defaults remain unchanged.

The calibration table now reads capture properties through the shared metadata cache and formats them through the same photographic readouts as the browser and viewport. Camera shutter descriptions take precedence over measured seconds. A separate readout metadata cache preserves labels after card removal without altering the trained artifact or numerical training groups. Existing groups acquire their missing readouts off-thread.

The review modal uses fitted table columns and normal body text for its guidance. Missing source files override a group's training status with a red circle and a missing-count tooltip. A bounded background check refreshes availability while the modal is open, and unavailable sources prevent retraining without affecting the saved usable calibration.

## Production cleanup

The controller owns explicit Import, Save and Train jobs. Capture storage and discovery live separately from modal presentation. List changes invalidate outstanding source-availability results. Atomic list/readout writes run in the background, and corrupted readout metadata cannot discard the pending capture list. Canonical paths are deduplicated before header reads. Shared capture metadata and photographic formatters remain the only readout path.

Training and rendering share sensor/shield region parsing, geometry validation and shield measurement kernels. Training measurement does not prepare a rendering correction or copy a model it will not use. Group measurement, repeatable defect detection and final fitting have distinct functions. The row-prior smoothing scale is defined with the model basis and shared with runtime row correction. No numeric detector thresholds, learned parameters or provided defaults change in this cleanup.

- [x] Run ordinary, GPU and workflow regression checks after cleanup.
- [x] Compare a full retrain against the retained calibration parameters and locations.
- [x] Rebuild and review the isolated app, preserving the existing User artifact.

The cleanup passes 689 ordinary workspace tests, focused GPU calibration and shield/row confidence checks, Clippy and documentation checks. A full 272-capture retrain reproduced all fitted parameters, provenance and sparse locations exactly. Native review confirmed import, background saves, successful training and cancellation. Cancelling a full retrain preserved the existing User artifact byte-for-byte. The wider modal uses category labels followed by colons and a compact reserved progress area. Its dimensions and footer position remain unchanged during training.

The final UI review grouped profile guidance, enlarged calibration controls and kept training progress and completion text in one status row. Editing parameter backgrounds extend outward while controls retain alignment. Disclosure lines align with parent labels, dropdowns share the value-field right edge, and viewport metadata reaches the bottom edge without asymmetric panel padding.
