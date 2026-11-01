---
summary: "Calibration separates capture storage and availability, background job control, and shared-metadata presentation."
---

# Noise calibration

Follow [sensor calibration requirements](../../../../docs/specs/sensor-defect-calibration.md) and [the implementation plan](../../../../plans/archived/merrill-noise-training.md). Keep job transitions in the controller. Capture discovery, metadata loading, availability checks, saves and training run off the editor thread through the existing cache priority gate. Use the shared capture reader and photographic formatters. The pending list and readout metadata do not own copies of the original X3Fs. A failed retrain must preserve the usable User artifact.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[captures.rs](./captures.rs)** Retains cap identities and shared metadata, discovers input files and checks source availability off-thread.
- **[mod.rs](./mod.rs)** Coordinates cap review and one user calibration through explicit background jobs and immutable profile selection.
- **[view.rs](./view.rs)** Draws the cap calibration modal using shared photographic readouts and controller actions.

<!-- INDEX:END -->
