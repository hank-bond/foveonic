---
summary: "Complete the approved viewport workflow slices in reviewed commits, then investigate wide execution with isolated measurements."
---

# Viewport workflow overnight run

The owner approved completing the remaining functional workflow in chunks with tests, independent review using the largest model available, cleanup and commits along the way. The owner will review the resulting application in the morning. This authorizes proceeding between these agreed functional slices without waiting for an intermediate owner response. Preserve a usable build and a separate commit for each conceptual improvement. Follow [viewport workflows](../docs/specs/viewport-workflows.md), [Smart preview](../docs/specs/smart-preview.md), the [viewport plan](./viewport-workflows.md), and the three [architecture goals](../docs/specs/render-orchestration.md#agreed-goals).

## Completed checkpoint and storage

- [x] Commit shared ordinary photo submission (`da73107`).
- [x] Commit coordinator-owned regional/whole sequencing (`85d543a`).
- [x] Audit storage with one inventory agent and three focused inspection agents.
- [x] Remove this project's debug build outputs, obsolete legacy acceptance output and 38 diagnostic cache namespaces proven by this checkout's launch records. Actual free space increased by approximately 40.5 GB. Keep the release build, current test fixtures, the frozen numerical baseline and research inputs/evidence. Other repositories and personal/system application data were not removed.

## Functional slices

1. [ ] **Multi-photo visible-region renders.** Put Current detail images in the existing bounded photo owner, with active Camera/Defaults detail in Before. Preserve per-photo memory accounting, eviction, source/settings validity and base/detail ordering. Share region calculation, publication and drawing between single and multi-photo views. Keep inactive detail delivery and whole continuation tied to the complete demanded set rather than the active photo or ordinary-request subset. Other tiles retain their own Smart resolutions. Preserve existing engine fallback and explicit preview modes.
2. [ ] **Aspect-aware comparison layout.** Maximize useful photograph area, support mixed orientations, preserve identity/selection/view transforms, and allow display order to differ from browser order. Keep layout stable during gestures. Use displayed visual order for viewport Shift ranges if reordering is introduced. Do not add expensive per-frame permutation searches.
3. [ ] **Combine Pointer and Hand.** Ordinary clicks select/activate, dragging pans without changing the selected editing group, and double-click retains Fit/detail behavior. Keep modifier selection and browser/editor semantics. Saved photographic tools continue to own their viewport clicks. Update toolbar, cursors, shortcuts and behavioral coverage together.
4. [ ] **Full-detail loupe.** Use the owner's Capture One screenshots as the visual guide: circular lens with a thin rim, selectable size and zoom, and a centered/offset placement option. Determine conventional interaction from primary Capture One documentation. Work in single and multi-photo layouts without changing editing targets or the main preview's selected quality. Request Full detail, reuse compatible pixels, replace obsolete regional demand, and avoid automatic whole-image work merely from moving the lens. Keep the first version to one independently refreshed detail request per photo. Update tool lifecycle/toolbar, persistence/help and scenario coverage coherently.
5. [ ] **Functional acceptance and cleanup.** Run related native scenarios and fresh-image comparisons, the frozen 40-case camera/quality matrix, ordinary workspace checks and performance regressions relevant to changed paths. Preserve exact processing arithmetic. Keep profile sessions isolated from other apps/compilers. Review naming, responsibilities, unused paths and test simplicity with the largest available review agent. Commit completed slices and leave a usable release app.

## Detail retention measurements

- [ ] Measure warm/cold source recovery, native preparation, region completion, optional whole continuation, delivery and memory under repeated detail switching.
- [ ] Include default 2 GB and low 1 GB retained-memory settings, one and multiple zoomed photos, and representative camera families.
- [ ] Use evidence to identify whether another source/checkpoint retention policy would help. Keep new cache concepts separate from the functional work unless a demonstrated correctness issue requires a small existing-owner fix.

## Wide-execution investigation

Only start this after the functional slices are complete. The owner authorized research and experimental probing, rather than making an unproven engine approach the application's default.

- [ ] Ask the largest available research/review agent to inspect the existing engine and primary sources for stage interleaving, submission grouping and completion-wait grouping. Update [wide rendering](./wide-rendering.md) with the concrete proposed experiments.
- [ ] Build the smallest isolated harness/prototype needed to compare widths 1/2/4/8. Keep per-photo recipes, cache identity, output equality and independent settlement. Distinguish scheduling changes from node arithmetic and shader batching.
- [ ] Measure first useful result, complete-group latency, CPU coordination, submissions/waits, memory and foreground interruption. Compare mixed cameras/options only after the simplest warm Thumb case works.
- [ ] Record what works, what fails and why. Retain useful profiling additions when clean. Keep unproven execution changes isolated and document a recommendation instead of forcing adoption.

## Per-slice procedure

Review the smallest concrete design and names with the review agent. Implement, run targeted checks and fix failures. Obtain final independent review, simplify and remove experimental leftovers, then commit the completed slice. Update this checklist and the owning specification as findings change. Keep a concise progress record so work continues reliably after context compaction.
