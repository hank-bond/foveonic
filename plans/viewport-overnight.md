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

1. [x] **Multi-photo visible-region renders.** Put Current detail images in the existing bounded photo owner, with active Camera/Defaults detail in Before. Preserve per-photo memory accounting, eviction, source/settings validity and base/detail ordering. Share region calculation, publication and drawing between single and multi-photo views. Keep inactive detail delivery and whole continuation tied to the complete demanded set rather than the active photo or ordinary-request subset. Other tiles retain their own Smart resolutions. Preserve existing engine fallback and explicit preview modes.
2. [x] **Aspect-aware comparison layout.** Maximize useful photograph area, support mixed orientations, preserve identity/selection/view transforms, and allow display order to differ from browser order. Keep layout stable during gestures. Use displayed visual order for viewport Shift ranges if reordering is introduced. Do not add expensive per-frame permutation searches.
3. [x] **Combine Pointer and Hand.** Ordinary clicks select/activate, dragging pans without changing the selected editing group, and double-click retains Fit/detail behavior. Keep modifier selection and browser/editor semantics. Saved photographic tools continue to own their viewport clicks. Update toolbar, cursors, shortcuts and behavioral coverage together.
4. [x] **Full-detail loupe.** Use the owner's Capture One screenshots as the visual guide: circular lens with a thin rim, selectable size and zoom, and a centered/offset placement option. Determine conventional interaction from primary Capture One documentation. Work in single and multi-photo layouts without changing editing targets or the main preview's selected quality. Request Full detail, reuse compatible pixels, replace obsolete regional demand, and avoid automatic whole-image work merely from moving the lens. Keep the first version to one independently refreshed detail request per photo. Update tool lifecycle/toolbar, persistence/help and scenario coverage coherently.
5. [x] **Functional acceptance and cleanup.** Run related native scenarios and fresh-image comparisons, the frozen 40-case camera/quality matrix, ordinary workspace checks and performance regressions relevant to changed paths. Preserve exact processing arithmetic. Keep profile sessions isolated from other apps/compilers. Review naming, responsibilities, unused paths and test simplicity with the largest available review agent. Commit completed slices and leave a usable release app.

## Detail retention measurements

- [x] Measure warm/cold source recovery, native preparation, region completion, optional whole continuation, delivery and memory under repeated detail switching.
- [x] Include default 2 GB and low 1 GB retained-memory settings, one and multiple zoomed photos, and representative camera families.
- [x] Use evidence to identify whether another source/checkpoint retention policy would help. Keep new cache concepts separate from the functional work unless a demonstrated correctness issue requires a small existing-owner fix.

## Wide-execution investigation

Only start this after the functional slices are complete. The owner authorized research and experimental probing, rather than making an unproven engine approach the application's default.

- [x] Ask the largest available research/review agent to inspect the existing engine and primary sources for stage interleaving, submission grouping and completion-wait grouping. Update [wide rendering](archived/wide-rendering.md) with the concrete proposed experiments.
- [x] Build the smallest isolated harness/prototype needed to compare widths 1/2/4/8. Keep per-photo recipes, cache identity, output equality and independent settlement. Distinguish scheduling changes from node arithmetic and shader batching.
- [x] Measure first useful result, complete-group latency, CPU coordination, submissions/waits, sampled memory and submitted-work cancellation/recovery. Compare mixed cameras/options only after the simplest warm Thumb case works. Actual foreground/background priority remains unmeasured and deferred.
- [x] Record what works, what fails and why. Retain useful profiling additions when clean. Keep unproven execution changes isolated and document a recommendation instead of forcing adoption.

## Per-slice procedure

Review the smallest concrete design and names with the review agent. Implement, run targeted checks and fix failures. Obtain final independent review, simplify and remove experimental leftovers, then commit the completed slice. Update this checklist and the owning specification as findings change. Keep a concise progress record so work continues reliably after context compaction.

## Integration progress

The layout, Pointer and multi-photo detail slices are integrated as `d8644e4`, `9820648` and `65ae1cf`. Their combined build passes 497 ordinary GUI tests. The real GPU ownership/delivery regression passes. The extended default and low-memory native transition journeys pass, with eight exact fresh-image comparisons. The older Smart smoke check expected a new regional image after already receiving Full. Its corrected assertions verify actual visible coverage and exact Full reuse rather than requiring unnecessary work.

Loupe implementation is complete. Review caught and corrected stale-detail selection, thumbnail dimensions being mistaken for camera geometry, and native Detail inheriting an ordinary reduced composition grid. Requests and frames now identify Detail explicitly, independently of regional versus Whole coverage. A cold thumbnail-only inspection first obtains Whole Detail for trustworthy camera geometry. Detail remains in the bounded photograph owner and is excluded from ordinary Whole presentation snapshots. Ordinary Full pixels may still satisfy native inspection. The reviewed native driver covers independent lens coverage, held cancellation, transient cropped/rotated photos and single-photo inspection. Integrated native execution and numerical checks passed as recorded below.

Final functional validation passes 513 GUI unit tests and the complete release workspace suite, strict release workspace Clippy, 129 profiling tests and five artwork tests. All 40 frozen camera/quality outputs are exact. The new real-worker Detail/Whole ordering test, the native-detail composition-grid regression and existing coordinator-continuation regression pass. Smart and Crop smoke checks pass after correcting retained-Full expectations. Default and low-memory native journeys pass with all loupe phases and eight exact final app-image comparisons. Captured multi-photo and single-photo lenses are available for visual review. A native cancellation failure led to press-owned selection: switching tools or releasing Space while holding the mouse cannot turn a tool gesture into a selection click. Independent review accepted the corrected code and tests. The stage-wide probe is isolated from this functional application on its own branch.

The resource measurements are recorded in the viewport plan. At that checkpoint, a 4 GB control removed one-native-view checkpoint churn, while two native views still replaced the single native processing slot. That slice included no cache-policy change. The isolated wide-execution branch is `experiment/wide-render-probe` at `e38448f`. Matched float/geometry/settings gates, five-trial timing matrices, operation-boundary resource measurements, independent supersession/failure recovery and separately verified threaded controls are complete. Application integration, actual background priority and memory admission remain later architecture work. Final frozen checks of the mechanically extracted serial operation runtime and restoration of the stock application build complete the handoff.

Final restoration rebuilt the stock processor and profiling examples from the application checkout after clearing shared release artifacts for that package, then rebuilt the normal app bundle. This prevents the shared worktree build cache from leaving an experimental processor in the review build. Completed feature and stock-control worktrees were removed after preserving executable hashes/copies and integrating their work. Only the reviewed experiment worktree remains for inspection. Both the functional processor and the isolated serial cursor pass the complete 40-case frozen matrix.
