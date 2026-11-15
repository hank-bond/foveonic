---
summary: "Quality and Speed select processing fidelity independently of automatic image resolution and regional bounds."
kind: reference
status: accepted
implementation: incomplete
---

# Processing quality

The preview offers two choices: **Quality** and **Speed**. Both choose a power-of-two working image resolution from displayed image pixels, zoom and camera capabilities. Quality avoids display enlargement beyond the chosen samples. Speed may use the existing bounded enlargement allowance. The selected mode is workspace state, not an authored photographic edit.

Quality must use complete working-resolution inputs for scene estimation and as the basis of spatial guidance. This includes Adaptive Tone, Dynamic Range and Dehaze. Both policies retain the shared multiscale brightness filter's half-width, half-height guide cells and gain anchors. Large neighborhoods use its additional fourfold coefficient reduction. These internal grids do not reduce the photographic image's requested resolution. Quality scene estimation may use the documented bounded Adaptive regional summaries and Dehaze score-bin averaging while retaining complete working-resolution inputs. These statistical summaries do not resize the photographic signal. Speed may additionally use validated smaller guidance inputs, statistical sampling and other documented approximations while retaining the authored controls.

Source calibration, camera reconstruction pyramids, frequency bands and the Color Field equation grid remain part of their existing algorithms. Choosing the overall working image resolution remains independent of processing quality. Dense scene estimation means complete coverage of that working grid, not reconstruction of samples finer than its input.

One processing-quality value accompanies a render request through graph preparation, sectional rendering, Whole continuation, background preparation and delivery. Regional bounds must not choose photographic quality. At identical quality, working scale and authored settings, a section and its Whole continuation must agree over their overlapping native coordinates. Cached presentation must not substitute pixels from the other processing policy solely because image resolution matches. Unaffected intermediate computations may still be shared.

Loupe always requests native image detail using the selected processing quality. Speed therefore permits reduced guidance for native loupe and viewport detail. Quality provides the maximum-fidelity check at those same native coordinates. Explicitly switching modes may change appearance. Refinement within a mode must not alternate guide policies.

Photographic and DNG exports must use Quality independently of the preview mode. Authored controls remain unchanged. Diagnostic snapshots must preserve processing quality so captures can be replayed. Processing quality must not be saved into photographic sidecars.

The five former workspace choices are replaced rather than retained as additional user modes. Numeric Full/Fast/Draft/Thumb-sized scales may remain in engine diagnostics and fixture coverage. They describe resolution, not processing quality.

Validation must cover policy switches, cached/fresh equality, sectional/Whole overlap, neutral bypasses, exports from either preview mode, and exact unchanged output where no quality-dependent operation is active. Dense paths require independent arithmetic and sampling checks. Photographic comparisons and latency/memory measurements must report the policy and working scale separately. Owner visual acceptance remains a separate gate.
