---
summary: "Cap calibration separates capture review, GPU reductions, statistical fitting and validated user profile storage."
---

# Cap calibration

Follow [sensor defect calibration](../../../../docs/specs/sensor-defect-calibration.md) and [the training plan](../../../../plans/archived/merrill-noise-training.md). Keep decoded sources immutable. Reduce image samples on the GPU and fit only the small resulting tables on the CPU. Process one capture group at a time. Learned sparse locations require a matching camera serial and native geometry, with a current-shot outlier check. The bundled calibration remains immutable.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[fit.rs](./fit.rs)** Fits small native-layer transfer systems and ISO covariance priors from GPU-reduced cap profiles.
- **[mod.rs](./mod.rs)** Reviews cap capture groups, persists one user calibration and gates learned defects by camera identity.
- **[training.rs](./training.rs)** Measures one cap group at a time on the GPU before fitting shared noise and serial-specific defects.

<!-- INDEX:END -->
