---
summary: "Enable Tiger Fast and Smart previews through the shared overview, regional refinement and whole-frame continuation."
status: archived
---

# Tiger Smart preview

Implemented the approved [Tiger](../../docs/specs/tiger-rendering.md), [Fast](../../docs/specs/quattro-fast-preview.md) and [Smart](../../docs/specs/smart-preview.md) contracts through the shared Quattro recipe. Full output and the application-wide preview preference are preserved.

- [x] Enable Tiger eligibility in recipe preparation and early capture hints.
- [x] Validate reduced overview calibration, mask geometry and cached edits.
- [x] Compare viewport refinements with Full crops in landscape and portrait, and verify whole-frame reuse.
- [x] Exercise native Fast/Smart interactions, panning, edited frames and Full exports.
- [x] Complete repository checks and update support documentation.

Tiger retains its own digital gains, repairs, highlight inference and noise preparation. Fast uses the existing half-grid blue mean and preview reconstruction. It retains blue phase-detection correction while skipping general high-resolution blue repairs. Regional Full starts after full-frame calibration, highlight statistics and color preparation, using the existing aligned halo and retained histories. Reuse the current scheduler and cache policy.

## Evidence

Tiger Fast produces 3096×2064 from the original 6192×4128 active image. Native calibration, digital gains, low-resolution RGB repairs, highlight inference, recovery masks and noise preparation remain shared with Full. Fast uses the existing blue 2×2 mean and same-grid preview reconstruction. General high-resolution blue repairs remain deferred to Full.

Owner review found a regular dot pattern in the first Tiger Fast implementation. Skipping all high-blue repairs also skipped paired phase-detection correction. A 2×2 mean preserved those sensor-site errors. Tiger Fast now applies its calibrated paired-site correction before averaging. Before/after renders of the supplied original confirm that the visible grid disappears. A flat-field regression injects bright and dark paired-site errors at three grid positions and verifies that Fast's blue reconstruction input matches the clean field exactly. Ordinary Quattro Fast and all Full paths retain their prior numerical operations. The landscape/portrait session, edited preview, exact regional crop, whole-frame continuation and native Fast checks pass with the correction enabled.

The recipe retains Fast's filtered/calibrated/processed color histories and both highlight masks within the existing active-photo byte budget. The first Full detail reuses these histories, and the whole-frame continuation reuses regional preparation. There is no new scheduler or cache implementation.

The original-file GPU check covers landscape Standard and portrait Portrait, nondefault recovery strength, changed exposure/noise/detail, restored defaults and immutable retained previews/source samples. Three viewport positions per orientation are bit-exact against the corresponding complete Full crops. The completed Full frame is exact too. Recorded execution stages confirm that neither detail nor continuation repeats shared whole-frame color/highlight processing, and continuation does not upload the original again.

Native Fast and Smart sequences pass with isolated preferences and sidecar access disabled. Smart refines the visible viewport, immediately starts whole-frame Full, keeps the same image revision on a subsequent pan, completes edited frames and reopens from the decoded disk cache. The worker export check produces TIFF and DNG files identical to independent Full exports under both Fast and Smart preferences, then returns to a Fast preview. Early capture hints enable Tiger without waiting for a completed render.

Coverage currently uses the supplied ISO100 sd Quattro H original. Portrait coverage rotates its metadata and exercises the Portrait color mode. It does not establish other Tiger ISO or native reduced-input coverage.

All 484 workspace tests, strict Clippy, formatting and documentation index checks pass. In a separate quiet native run on the available M1 Pro, decoded-cache reopen to Fast took about 653 ms. A 992×1002 Full detail took 475 ms, followed by 505 ms for whole-frame continuation. These are single-fixture observations, not cross-camera or hardware benchmarks. A previously completed Full frame switched to Fast in 261 ms. No numerical kernels or ordinary Quattro cache policy changed.

The final Full TIFF is byte-for-byte identical to its pre-Smart Tiger baseline.
