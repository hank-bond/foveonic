---
summary: "Continue full-quality editing responsiveness from the reviewed checkpoint, preserving guarded ownership and deferred mismatch evidence."
---

# Editing optimization handoff

## Assignment

The owner requested continued editing-performance optimization and has since deferred the intermittent output-mismatch investigation. Preserve its evidence; do not describe it as fixed. New GUI diagnostics are deferred to a later pipeline-visualization view. Prioritize full-quality slider-to-main-image feedback and the baseline editing workspace.

The editing worktree `/Users/work/git/foveonic-editing` on `perf/editing-feedback` starts from **`d686266`**. The owner assigned continued responsiveness work there, independently of sidecars and DNG export in `/Users/work/git/foveonic-gpu`. The reviewed starting commits include read/decode profiling, independent capture metadata, organizer keyboard navigation, simplified photographic controls/help, and broader drag/navigator measurements. Guarded storage and inspection differences are being checkpointed separately; coordinate their integration with the editing worktree. Local `main` is an older checkpoint; no remote is configured.

Use a separate worktree/branch from that commit and a separate Cargo target directory. Do not copy the uncommitted tone prototype or reset/stash/overwrite the shared worktree. Guarded-storage migration is a separate reviewed ownership improvement; coordinate its integration before changing the processor's storage API. Cargo reused an earlier snapshot's binaries during the first review harness because copied source mtimes were older than shared artifacts. Final review explicitly rebuilt sources in isolated artifacts and checked the expected test inventories.

## Read first

- Repository and applicable crate/source `AGENTS.md` instructions.
- [Interactive rendering requirements](../docs/specs/interactive-rendering.md).
- [Responsiveness plan](./ui-responsiveness.md), including evidence and unresolved acceptance.
- [Render resources](../docs/specs/render-resources.md) and [GUI workspace](../docs/specs/gui-workspace.md).
- [Contribution conventions](../docs/contributing.md) before documentation changes.

The reference repository `/Users/work/git/foveonic` is read-only, including owner edits. Preserve attribution when importing code and retain existing evidence.

## Ownership and coordination

Core GPU/storage cleanup is complete after the guarded-storage migration. Temporary mismatch probes are removed; retained read/write guards and permanent ownership/correctness tests remain. Coordinate core edits with the owner of that migration. The CPU tone-table reuse prototype remains uncommitted and should be revisited only at the end of optimization. Do not integrate it into accepted work or conclude that the mismatch is fixed because a run passes.

Begin with **native editing and GUI scheduling/presentation measurements**. Likely investigation sites are `crates/foveonic-gui/src/worker.rs`, `viewport.rs`, `navigator.rs`, and adjacent tests. An inspection-differences agent has also begun changing the shared worker (paired roots, comparison textures and request identity), so these are not exclusive ownership assignments: coordinate with that agent before implementation. Use separate measurement/test files first. Coordinate before changing processing shaders or reconstruction code: preserve exact photographic equivalence and integrate the guarded-storage API consistently. Keep optimization findings in this plan rather than concurrently editing the responsiveness plan.

A separate worktree prevents source/build interference, not physical GPU contention. Coordinate GPU-heavy runs with any other active rendering or validation work. Never use timings collected during another agent's render, benchmark, memory sampling, or native smoke run as acceptance evidence. Do not stop the owner's running application without permission.

Review and discuss material architecture, memory or photographic tradeoffs before implementation. Do not introduce a compatibility path to preserve a replaced design. Do not commit, merge or publish unfinished optimizations as validated improvements.

## What is already done

- Processing-owned exact effective keys and upstream generations select one active edit's complete unchanged continuation boundary. Cheap version records do not imply retaining all image buffers.
- One active render and one replaceable latest pending request, no debounce/FIFO backlog. Safe preparation/operation boundaries permit supersession; submitted GPU work cannot be forcibly cancelled.
- Images publish before optional revision-matched scopes. Completed same-photo/root older frames can advance feedback without adopting old requested settings.
- Navigator preparation is GPU-only, deferred after main publication, and suppressed while image processing is busy. The maximum-512-edge tile uses ordinary egui painting and source/revision/mapping-safe publication. Pan/zoom/scroll do not regenerate it.
- Continuous worker dragging now covers Detail, Exposure, chroma/luma presets and all seven advanced overrides. Cache/fresh full-float comparisons cover the corresponding noise/override recipes.
- Profiling separates complete raw-file reading into memory, in-memory X3F decoding and total source opening. Filesystem cache state is uncontrolled. Warm editing reuses decoded data; historical load timings must not be charged to each edit. Local SSD reads do not characterize slow SD-card openings.
- The output-equivalent full-pipeline performance pass is closed; do not reopen it as an unrelated optimization project.

## Remaining performance work

1. Establish native pointer-driven slider feedback and frame-gap baselines from the frozen checkpoint, distinguishing UI handling, completed image delivery and shared-GPU drawing stalls.
2. Profile remaining main-image preparation/presentation and worker scheduling costs. Explore the smallest measured improvement first; allocation/binding overhead and presentation waits are candidates, not established bottlenecks.
3. Review any change to display storage, submission granularity or operation boundaries before expanding the architecture. Preserve full-quality arithmetic, immutable artifacts, source/revision pairing, clipping, orientation and channel/signal mappings.
4. Measure completed unprofiled wall time before/after under matched warm conditions. Timestamp-profiled runs synchronize and are diagnostic, not wall-time acceptance. Measure memory and native interaction separately.

No reduced-quality drag recipe, weakened corrections, photographic CPU processing/readback, fixed checkpoint set, cache-all preference, or chained editing from the previous slider result. Every edit continues from unchanged original dependencies. Export stays non-cancellable with editing disabled. Scopes and navigator remain secondary to main-image feedback.

## Measurements and limits

Blue-sign full-quality native Detail feedback is approximately **125–143 ms** at the accepted complete-boundary checkpoint. Completed cached processing plus image preparation was **119–131 ms**, versus **747–774 ms** fresh. Six dispatches remain, including display texture preparation. Warm native busy frame gaps: median **16.6 ms**, p95 **45.8 ms**, maximum **46.7 ms**. Background execution does not prevent contention on the shared eframe/wgpu device and queue.

Detail inputs retain **783.6 MiB** without sharpening or **845 MiB** with sharpening. Settled total physical footprint was approximately **2.1 GiB**, with a **3.5 GiB** transient peak. Approximately 2 GB total normal editing memory is a soft target, not a cache/RSS limit. GPU storage allocation totals are not residency; RSS excludes substantial GPU accounting.

Worker drag tests settle Detail/Exposure around **163/96 ms**. Earlier noise/override edits still need **286–955 ms**; brightness/layer mixing roughly **150–171 ms**. These checks establish progress/settings/scopes correctness, not native interaction acceptance.

Navigator preparation: synthetic average **1.62 ms**, native warm commonly **1.7–3.3 ms**, with cold/contention outliers. Synthetic completed batched drawing measured **0.169 → 0.122 ms** for repeated float presentation versus ordinary prepared-tile painting. This is a modest standalone-pass result, not proof that native frame stalls are removed.

## Deferred mismatch

The owner deferred this issue after both guarded and original-storage builds passed matched trials; normal-use symptoms were not observed. Frozen failures occurred with tone reuse disabled, including a fresh render that differed while a repeated fresh render matched the cached result in full. The cause and first divergent stage remain unknown. Findings and historical logs are indexed in [GPU output mismatch](./gpu-output-mismatch.md); guard validation is recorded in [guarded GPU storage](./guarded-gpu-storage.md).

Continue optimization rather than broad diagnostic reruns. Reopen if an exact-output regression fails or the owner reports a symptom. Do not weaken tolerances or use delays to hide it.

## Evidence and validation

Fixture originals: `/Users/work/git/foveonic/references/x3f`. Retained float goldens: `.local/pipeline-performance/baseline/goldens`. Use a new evidence directory; never overwrite goldens or delete failed-run evidence.

Reviewed stable checkpoint: **52 ordinary core tests**, **49 ignored core GPU/fixture tests**, **61 ordinary GUI tests**, **5 ignored GUI GPU/worker tests**, plus CLI/docs-index and Python tests. All passed in explicitly rebuilt isolated snapshots. Logs: `/tmp/foveonic-followup-*-isolated.log`, `/tmp/foveonic-followup-stable-isolated-gpu.log`; exact-output evidence: `.local/ui-responsiveness/stable-checkpoint-equality-2`.

The shared checkout retains `/tmp/foveonic-ui-{followup-gpu,followup-gpu-2,flower-isolation-5}.log`, plus later diagnostic logs and all native/equality captures. Earlier artifact-reused review checks are not authoritative for later snapshots.

Run the repository's release workspace, strict all-target Clippy, example, formatting, documentation-index and Python checks. Run fixture/GPU checks serially when GPU use is coordinated. Report scope, completed wall measurements, output equivalence, memory changes, native interaction limits and any unresolved failures before requesting acceptance.
