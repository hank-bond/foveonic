---
summary: "Align despeckle threshold WB with the actual calibration factors while retaining independent adjustment and Sensor-gain behavior."
---

# Match despeckle WB to calibration

The owner approved fixing the [WB-scale mismatch found by the node audit](./ordered-repair-and-node-audit.md#despeckle-wb-scale-and-recipe-coupling). Requirements: [dynamic despeckling](../docs/specs/despeckle.md).

## Scope

Pass calibration's actual three float32 WB factors into threshold preparation instead of independently reconstructing unscaled diagnostic factors. This includes calibration's factor of two, selected WB, and metadata adjustments. Keep the fixed 1× despeckle adjustment, independent toggles, Sensor-gain normalization, camera repair, stage order, and reconstruction recipes unchanged. Noise-recipe coupling and fuller camera correction are not authorized by this change.

## Steps

- [x] Share prepared calibration factors with despeckle preparation; remove the duplicate WB calculation.
- [x] Test metadata adjustment selection, scale agreement, invalid factors, preset changes, selective roots, positive/zero Sensor gain, and retained artifacts.
- [x] Verify the two original speckles and repaired leaf block, and compare all six full-source outputs against reference replay with identical factors.
- [x] Render default/reconstructed/disabled routes; check the exact neutral baseline and review native captures.
- [x] Update current documentation and indexes; run workspace/GPU tests, strict Clippy, formatting and diff checks.
- [x] Owner authorized grouped commits after code and automated checks.
- [ ] Owner photographic review.

## Evidence

- `render.rs` passes the three factors from the same prepared calibration object into `despeckle::prepare`. Threshold generation widens those float32 values for its existing reference table arithmetic. Duplicate WB-list/aperture/temperature/sensor preparation is removed; calibration's arithmetic itself is unchanged.
- [Despeckle tests](../crates/foveonic/src/nodes/sensor/despeckle_test.rs) cover exact factors, all original-file WB presets, Float32 versus integer `DespAdjust`, invalid factors, and Sensor-gain-independent tables. A nonzero-threshold GPU regression demonstrates that the old mismatch repairs a marginal point that the matched scale preserves, while a strong defect is still corrected at gains 0/1/1.3. The photographic test also compares the graph result against direct application prepared from actual calibration factors.
- Both original unlisted red speckles remain corrected with green/blue unchanged; the ordered-repair leaf block remains corrected before despeckling. Retained earlier artifacts remain immutable.
- Reference replay used identical full-sensor inputs and actual float32 calibration factors. Every output sample agrees within 0.000977 camera codes across six originals. Detection counts match the scale-matched audit column: 4800 / 7061 / 62632 / 3164 / 4742 / 16597. These are candidate counts, not verified defect counts.
- Six TIFFs per default/reconstructed/disabled route were compared. All disabled outputs match the neutral baseline byte-for-byte. Whole-image and native-crop review compared previous mismatched defaults, matched defaults, and matched reconstruction. Whole-image appearance is stable; known highlight/remaining-defect limitations are unchanged.
- Native review covered 72 captures (six 11-frame correction runs and the six-frame leaf run). Reported defects remain corrected in native rendering, with reconstruction, inspection and independent disabling exercised.
- Final release workspace checks pass: core 36 ordinary tests, CLI 2, GUI 26, plus documentation tests. All 28 ignored GPU/fixture/worker regressions pass. Strict all-target Clippy, release builds, documentation generation/check/tests, formatting and diff checks pass.

The owner authorized grouped commits; no noise-recipe adjustment coupling changes were made. Photographic review remains pending.
