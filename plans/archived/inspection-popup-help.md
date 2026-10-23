---
summary: "Prevent Quattro inspection menus from crashing when a stage lacks explanatory help."
status: archived
---

# Repair inspection popup help

Requirements: [photographic help](../../docs/specs/photographic-help.md) and [GUI workspace](../../docs/specs/gui-workspace.md).

- [x] Reproduce the owner's panic by opening the Quattro popup with actual egui pointer input.
- [x] Remove help-text unwraps from group and scalar rows and describe the missing Quattro stages.
- [x] Check help coverage across DP and SD catalogues and verify popup interactions remain usable.
- [x] Run GUI checks and rebuild the local app.

The reproduction failed at `inspect.rs:66` with the owner's exact `Option::unwrap()` panic. The existing help-coverage test enumerated the default Merrill catalogue and omitted the later Quattro calibration and reconstruction outputs. Inspection availability belongs to the processing catalogue. Optional UI explanations must not prevent selecting a supported output.

## Verification

The pointer-driven Quattro popup regression now passes. A separate check keeps undocumented group and scalar rows usable. Synthetic DP0–DP3 and SD metadata exercise the sparse calibration, phase detection, optical correction, defect, shading, color-sector and reconstruction catalogues. Early metadata has its own coverage. All 164 ordinary GUI tests pass, including existing component/scalar selection and popup-close interactions. Strict GUI Clippy passes. The local app is rebuilt without installation. Changes remain uncommitted.
