---
summary: "Serve a requested buffer from the same buffer cached at a finer scale, as an engine capability enabled in Speed first and compared node by node."
---

# Cross-scale reuse

Held after its A/B, 2026-11-26 (owner). The implementation is on `feature/cross-scale-reuse` and is not merged.

## Outcome

The first implementation routed the Adaptive Tone guides and the Dehaze overview through one whole-photo helper. The helper rebuilt the branch's camera stage at the main render's scale and reused it when the stage merged into the main graph, area-averaged to a coarser scale in Speed. The structural check was sound, but it never found a reusable stage. Each branch resets settings that its camera recipe reads: Merrill's appearance stage reads the base response, which the Adaptive Tone branch resets, and Quattro copies the Dehaze settings into its camera controls, which the Dehaze overview resets. These branches therefore build a different image, not the same image at another size. The reverted shortcut also averaged that different image.

On the second Mac, 144 render cases against `main` produced identical pixels, so no reuse applied. Fifteen cases were 3–20% slower, none faster, because the check builds an extra camera graph per branch on every render. Graph construction is far from free, particularly for Quattro.

Revisit after [shared linear edits run after the camera family](./coherence-cleanup.md) and camera stages stop reading finishing settings. The check could then compare the camera part of the settings instead of rebuilding graphs.

## Goal

A node requests a buffer at a working scale, for example whole-photo pre-tone RGB at 1/8. When the cache already holds the same buffer, with the same settings, at a finer scale such as native or 1/2, the engine reduces that buffer to the requested scale and hands it to the node instead of building it fresh. Nodes never implement this and cannot tell which path served them.

## Owner decisions

- Full-photo renders and patches may have different rendering requirements. Patches recompute whole-photo statistics they cannot see; full renders may reuse what is already rendered.
- No branch may depend on the size a full buffer is rendered at.
- A reduced finer buffer is not less correct than one built at the smaller scale. Differences are judged by A/B comparison, not by which path came first.
- Enable in Speed first. Quality follows only if A/B comparisons show no visible difference.
- The per-node Adaptive Tone shortcut was reverted for this design. Its measurements are in the [graph-node performance plan](./graph-node-performance.md#adaptive-tone-at-reduced-fit-resolutions).

## Design to agree

- **Which buffers reduce, and how.** Each graph output's data type declares its reduction: area mean for linear RGB and camera layers, maximum for coverage masks, union for categorical flags. Types without a valid reduction, such as signed detail or encoded data, are never substituted. The existing [pooling node](../crates/foveonic/src/nodes/sensor/pool.rs) already implements mean, maximum and union by signal.
- **Finding the finer buffer.** Cache identity currently includes the working scale. The engine needs a lookup that matches everything except scale and returns the finest suitable entry whose grid is a power-of-two multiple of the request, covering the whole requested extent.
- **Serving it.** The scheduler inserts the reduction ahead of the requesting node and caches the reduced result under the requested identity, so later requests hit it directly.
- **The switch.** One processing-quality setting, on for Speed, off for Quality, read only by the engine.
- **Determinism.** Output may depend on what was cached, within the measured tolerance. Tests that compare exact pixels pin the path; the A/B harness measures both.

## A/B method

For every node whose input can be served this way, render each fixture with reuse on and off from the same cache state. Compare the final 8-bit SDR images: largest local difference, the area with the largest mean difference, and crops at both. Record fresh-processing time, peak and settled memory, as the [optimization log](../docs/optimization-log.md) does.

## Candidates

- Adaptive Tone analysis and guide.
- Dehaze analysis.
- Other whole-photo statistics and guides: to be listed from the graph before implementation.

## Moved to 0.10.0

- **Tonal ranges.** Its guide follows Adaptive Tone, so its branch needs the shared edits, which each camera family runs internally with its own context. It joins the helper once [shared linear edits run after the camera family](./coherence-cleanup.md).
- **One sample density per working grid**, which keeps Adaptive Tone's size-based branches: [coherence cleanup](./coherence-cleanup.md).

## Steps when revisited

- [x] Owner review of this design.
- [ ] Inventory outputs by data type and reduction, and the nodes that request a coarser scale.
- [ ] Scale-free cache lookup and scheduler-inserted reduction behind the Speed switch.
- [ ] A/B harness and results per candidate node.
- [ ] Independent review, full GPU suite, owner testing.
