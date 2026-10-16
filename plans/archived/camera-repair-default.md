---
status: archived
summary: "Camera-record repair uses source-derived cutoffs with a manual override; ordered correction now fixes the subsequently reported leaf cluster."
---

# Use camera-derived repair defaults

## Closure

The owner accepted the implemented photographic results and authorized closing review-only plans. This implementation slice is complete. Pending-review statements and proposed follow-ups below describe earlier checkpoints. Subsequent camera repair integration is recorded in the [algorithm completion plan](../complete-reference-algorithms.md).

The owner approved camera-derived minimum badness with a manual override. Requirements: [processing graphs](../../docs/specs/processing-graphs.md). UI changes and residual-defect diagnosis are tracked in the [editor review plan](./editor-review-focus.md).

## Steps

- [x] Audit the reference single-exposure cutoff and the sign photograph's records.
- [x] Add `RepairThreshold::Camera` / `Manual(u32)` and migrate existing settings/callers to `Settings::repair`.
- [x] Default enabled GUI repair and CLI `--repair` to camera selection; enable it at startup alongside the separately controlled [despeckle stage](./despeckle.md), as subsequently requested by the owner.
- [x] Preserve CLI `--repair-badness` as explicit manual selection, evaluated identically by the shared engine.
- [x] Test metadata selection, source-dependent recalculation, manual bypass, all six originals, selective inspection, and retained settings.
- [x] Verify automatic cutoff 2 and manual cutoff 2 produce identical repaired layers on the sign.
- [x] Check baseline TIFFs remain unchanged and complete regression checks.
- [x] Owner authorized grouped commits after code and automated checks.
- [x] Owner accepted the implemented photographic results and closed the review checkpoint.

## Findings and boundaries

The old enabled GUI cutoff was 8; the sign's recorded scores are 0–3. Camera metadata selects 2. The single-exposure arithmetic is adapted with attribution from the read-only reference's `repair_setup.rs` at `e47af35`: `BadnessTable`, `CaptureShutter`, `SensorTemperature`, and `BPBadnessOffset`. Float32 exponential ordering is preserved. Malformed/nonfinite tables and cutoffs are rejected; manual selection requires no threshold table.

Automatic selection remains explicit in artifact settings and is evaluated for each source. The GUI now offers only the default-enabled Bad-pixel repair checkbox, with camera-derived cutoff selection internally; owner review removed the manual slider and A toggle as diagnostic rather than photographic controls. CLI `--repair-badness` retains the manual override. Repair precedes despeckling/reconstruction.

This cutoff change initially retained the immutable-neighbor kernel and record merging. The subsequent [ordered-repair fix and node audit](./ordered-repair-and-node-audit.md) replaces that arithmetic and corrects adjacent main-list defects; fuller native repair lists and distinct packed clusters remain separate work. The new [despeckle stage](./despeckle.md) has its own detector and ordered chroma-anchored correction. Comparison established that the owner's two residual red speckles are unlisted camera-record positions corrected by the reference's later despeckle stage, not by changing this cutoff. Details and reference evidence live in the [editor review plan](./editor-review-focus.md).

## Leaf-image diagnosis before ordered repair

The newer screenshot shows a magenta spot at cropped/oriented image `(1582,2737)`, corresponding to sensor `(2801,1589)` in `glossy-leaves-and-buds.x3f`. It is a 2×2 recorded red-channel defect, unlike the earlier two unlisted isolated speckles:

| Sensor position | Camera record |
| --- | --- |
| `(2800,1589)` | `0x432f` |
| `(2801,1589)` | `0x4397` |
| `(2800,1590)` | `0x43e9` |
| `(2801,1590)` | `0x43f4` |

All four have badness 3; automatic cutoff is 2, so they are selected. The old GPU kernel nevertheless left every center unchanged because each had another selected red defect among its eight neighbors. Despeckling also leaves the block unchanged; a clustered defect does not satisfy this detector's isolated-extremum test. GPU readback confirms identical values across calibration, repair, and despeckle at all four positions.

The read-only reference's ordinary camera-repair prefix corrects red at the four centers from approximately `276/615/605/383` to `157/160/162/158` and preserves green/blue. Highlight restoration and later despeckling do not change them further; no nearby dynamic speckle records are emitted. GPU calibration values have approximately twice the reference diagnostic scale, so these are within-prefix correction comparisons, not claims of bit-identical calibration.

The reference ABI was called with Sunlight, aperture 2.8, one exposure, and diagnostic 1× adjustment; the GPU probe used shared default correction settings. No processing behavior was changed during diagnosis.

The appropriate next proposal is reference-style ordered/masked camera repair that handles adjacent records, rather than lowering the despeckle thresholds or inventing another cluster-specific toggle. The owner subsequently approved that replacement, now implemented and checked in the [ordered-repair plan](./ordered-repair-and-node-audit.md). Full native repair-list/cluster scope still needs approval before expansion.

The owner authorized grouped commits; photographic review remains pending.
