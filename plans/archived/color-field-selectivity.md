---
summary: "Experiment with local color-evidence acceptance thresholds, without added neutral bias, to reduce bleeding while preserving compatible reconstructed color."
status: archived
---

# Color evidence selectivity

## Approved scope

The owner clarified that the desired slider selects or rejects color evidence locally. The implemented neutral-restraint adjustment is not that control. The owner approved another experiment round. Keep neutral restraint off, do not change the GUI or promote a new default, and retain the optimized brightness methods, full-sensor geometry, Local's 64 sweeps and existing solver bounds. Recovery arithmetic stays in WGSL. Originals, sidecars, retained evidence and the running app remain untouched. Do not commit or install.

Follow the [color-field requirements](../../docs/specs/color-field.md). This round supersedes the tuning acceptance direction in the [neutral-restraint plan](../color-field-restraint.md), not its recorded measurements or unresolved cache observations.

## Questions

- Do the surviving ratios distinguish the unwanted field color at the skirt's affected pixels, or is the color ambiguous under the available layer evidence?
- Does matching candidates to their solved color rather than only their measured cell guide improve selection?
- Does the native evidence-retention limiter preserve unwanted field color after the original confidence gate rejects it?
- Can mixed-color grid evidence be rejected without applying the same penalty to homogeneous flower or skin colors?
- Do trustworthy native donors or finer cells separate colors that an eight-pixel average mixes?

## Experiments

- [x] Freeze the current source and a zero-added-neutral-bias baseline. Preserve source/binary hashes and failed attempts.
- [x] Inspect native GPU evidence support, field mismatch and chroma diagnostics at the registered skirt region.
- [x] Test candidate-color acceptance at sampling and before the native fit, independently of the original coupled agreement/retention control.
- [x] Test ratio-scatter selection in affected grid equations while preserving guide-based connectivity.
- [x] Test local trustworthy-donor selection and a two-pixel grid with a valid donor-count requirement.
- [x] Compare both brightness methods and native crops on the skirt. Check candidates on petals, knitwear and skin.
- [x] Separate evidence rejection from changes to the existing neutral regularizer. Check the unchanged-selection baseline against all sixteen CPU-reference rasters.
- [x] Restore working source exactly and record findings. Keep failed attempts visible without substitute images.
- [x] Owner selected the original-neutral-coefficient cutoff with a 0.005–0.02 manual range and A preserving original behavior. The [unified-control plan](./unified-highlight-control.md) tracks integration and its acceptance checks.

## Method

A rejected borrowed color may fall back to the existing same-pixel fit and baseline neutral prior. Do not add a uniform neutral penalty, scale every reconstructed chroma toward gray, or label generic desaturation as selectivity. Report when a change merely removes color support instead of identifying a better color. Fully ambiguous evidence may not permit a defensible local rejection decision.

Use fixed brightness arithmetic and stopping bounds. Grid variants are explicitly distinguished and do not change production geometry. A two-pixel cell has only four samples, so the baseline minimum of eight donors cannot be retained for that investigation. Earlier two-pixel results therefore did not isolate spatial resolution alone.

Retain RGB16 TIFFs and generate matched color-managed crops/full views without replacing older reports. Preview images are eight-bit review aids. Diagnostics read GPU-computed quantities, not CPU recovery. Existing localized fresh/cached Output differences remain unresolved and are not cleared by this experiment.

## Findings

### Input and measurement discipline

The current 0170 editing sidecar had restraint set to 8 when this round began. The photographic settings otherwise matched the retained recipe. Do not overwrite that edit. A private snapshot of sixteen originals and fifteen current sidecars isolates the measurements from subsequent editing. Every accepted trial explicitly records restraint zero, and the final-fit neutral addition is absent from its compiled shader. The snapshot files still match their capture hashes.

An initial orchestration attempt rendered with the added penalty removed but still recorded restraint 8. Its four renders are retained and excluded from the accepted photographic comparisons. Two initial scatter shader versions failed WGSL validation because assignments to vector swizzles are not allowed. Their four failed method attempts have no images. Corrected sources use separately named variants. No unchanged failing solver run was retried and no tolerance was relaxed.

The round completed 138 correctly configured photographic renders and RGB16 exports, plus one GPU diagnostic crop. Twenty-six configurations were checked on 0170. Fourteen configurations were checked on all three additional priority subjects, in both methods. The offline A/B review retains full views, registered crops, effective recipes, single completed-render samples, RGB16 links, and explicit failed/unmeasured states. Previews remain eight-bit review aids.

### Surviving evidence

Of 640,000 pixels in the registered skirt crop, 84,470 were eligible for reconstruction. Of those, 74,580 had a fully trusted layer pair and 814 had no surviving pair. The median reliability-weighted mismatch between native ratios and the sampled field proposal was approximately 0.01394 log units. At a cutoff of 0.035, 22,270 proposals exceeded the threshold.

These diagnostics describe proposals before the final native fit, not final-image error. A trusted pair from two surviving layers constrains only one ratio. It does not determine the full missing color, so the large supported fraction does not prove that white can always be distinguished from light yellow.

### Candidate screening

- Rejecting sampled solved colors at 0.01/0.035 log-ratio mismatch and rejecting the final sampled prior at 0.02/0.05 changes selected pixels but does not clearly remove the skirt wash. The native fit already restrains some incompatible proposals.
- Rejecting heterogeneous intact donor cells at 0.02 standard deviation has little visible influence on this crop. Changing region-peak boundary matching to local matching, alone or with finer cells, does not clearly solve the wash.
- A two-pixel grid with a valid four-sample donor requirement is a distinct, successfully measured variant. It does not demonstrate a clean solution despite correcting the earlier donor-count confound.
- Selecting actual intact native donors within 4/8 pixels, or within 8/16 pixels with ratio-first matching and a two-stop brightness allowance, does not show a clear photographic advantage. It also introduces a search per affected pixel, so it is not selected for integration.
- Filtering high-scatter layer-ratio observations in affected grid equations gives the clearest adjustable response. Standard-deviation cutoffs 0.10/0.05/0.02/0.01/0.005 were tested. Lower thresholds reject more mixed evidence. Native measurements, source clipping reliability, graph guides and brightness equations stay unchanged. This is local evidence selection, not an added uniform neutral penalty.

### Important neutral-weight interaction

The first scatter implementation recomputed the existing adaptive neutral regularizer from the reduced evidence weights. Rejecting evidence therefore also increased that existing neutral weight in some cells. Its strongest setting reduced skirt bleeding but created conspicuous pale petal patches. Do not attribute all of that whitening to pure selection.

Paired experiments first held the neutral coefficient at a common weak constant. The cleaner subsequent experiment preserves each cell's original adaptive coefficient instead. It computes that coefficient from the unfiltered guide and passes it explicitly to the solver. Only accepted color-data weights change. The experimental color row grows from 64 to 80 bytes to carry the coefficient. Brightness rows, compensated iterates, true-residual checks and stopping bounds do not change.

This original-coefficient version retains a partial reduction in skirt bleeding, with fewer pale-petal artifacts than the coupled version. Cutoffs 0.005 and 0.02 provide a visible per-image tradeoff. It does not identify ground-truth colors or eliminate all bleeding. Genuine gradients and texture can also look inconsistent, so strict settings can still lose useful reconstructed color. That cost is a tuning decision for the owner, not grounds for rejecting the control merely because another photograph needs a different setting.

The explicit-coefficient baseline, with no evidence filtering, passed all sixteen identical-evidence CPU comparisons. Its maximum normalized error was approximately `8.85e-5`, below the unchanged `2e-4` bound. This validates the baseline layout/arithmetic experiment. It is not production acceptance for the filtered algorithm or a replacement for selected-control cache and numerical tests.

### Disposition

The owner selected the mixed-evidence cutoff with original neutral coefficients preserved. The other tested mechanisms did not show a stronger photographic selection effect. The initial interface exposed raw cutoffs. The owner then clarified that the ordinary control should be positive-only optional processing: strength defaults to 0, preserving original behavior, and increasing toward 1 rejects more inconsistent evidence. Color Field has no redundant A button; Classic retains its automatic baseline. Positive strength maps to cutoff `0.005 / strength`. The [unified-control plan](./unified-highlight-control.md) records the current mapping, data migration and shared Method/Highlights layout and Classic-only A.

The four core working files and comparison example were restored byte-for-byte to their pre-round contents. The diagnostic example and all trial sources/binaries were retained outside maintained source. The ordinary comparison renderer was rebuilt from restored source. No app bundle, original input or owner sidecar was replaced. No commit, install or launch was performed. The earlier single-pixel Output differences remain unresolved and are not cleared by these comparisons.

## Closure

This completed implementation or experiment is archived during release 0.3.0 preparation at the owner's request. Earlier scope, review and working-tree statements describe that historical slice. Recorded limitations and separately tracked follow-ups remain explicit.
