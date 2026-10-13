---
summary: "Replace raw processing-buffer handles and recycling flags with guarded ownership, explicit bindings and allocation diagnostics."
---

# Guarded GPU storage

Implements the owner-approved guard approach in [render resources](../docs/specs/render-resources.md). This is an ownership improvement, not an established fix for the intermittent output mismatch.

- [x] Encapsulate raw buffer handles in the GPU/storage layer; shared read ownership and exclusive write guards cover graph values, caches, retained images and processing workspaces.
- [x] Make dispatch reads/writes explicit and retain borrowed guards through synchronous submission. No delayed encoder may escape this boundary.
- [x] Replace the graph's recyclable flag with checked exclusive ownership when returning storage to scratch.
- [x] Identify allocations and write generations in diagnostics; test shared readers blocking writes/recycling, last-reader release, binding alias rejection and retained output stability.
- [x] Migrate diagnostic image readback to the engine instead of exposing a public raw writable handle.
- [x] Validate an isolated snapshot, exact fixture comparisons, ordinary/GPU tests and strict checks. Preserve all failed evidence.
- [x] Owner approved review and checkpointing; keep the tone prototype out of accepted commits.
- [x] Defer the unresolved mismatch by owner decision; preserve experiment evidence and remove temporary investigation paths.
- [x] Validate cleaned code and coherent documentation with the concurrent inspection slice; leave tone reuse uncommitted for later review.

## Scope

Single ordered device queue remains the submission contract. Ownership checks do not detect missing graph edges or shader-internal races. Processing workspaces may be modified in place only through an exclusive write guard; read bindings hold read ownership through command submission. No extra production fences, photographic arithmetic changes or performance acceptance claims.

The concurrent inspection agent owns GUI changes. Core buffer types, processing callers, tests and the benchmark readback caller are migrated here; no GUI behavior is changed.

## Implementation

`gpu_storage.rs` is a private child of the GPU module. `ReadGuard` shares an `Arc` allocation; mutable borrowing plus `Arc::get_mut` is required to acquire a non-clonable `WriteGuard`. Other owners block writes in release builds too. No raw-buffer accessor is visible to processing modules. `Binding::Read` borrows a read guard; `Binding::Write` owns a mutable lease for that call. Dispatch and GPU copy encode and submit synchronously before releasing their guards. In-place shader stages use exclusive write bindings, including their internal reads.

Cache/image/graph clones now share tracked ownership. The `Value::recyclable` flag is removed; scratch accepts only exclusively owned allocations. The graph still restricts pooling to ordinary floating-image layouts, avoiding retention of large tiled/atomic workspaces that image scratch cannot use. Pool eligibility by shape is separate from enforced content ownership.

Allocation IDs, write-generation IDs, sizes, labels and last submitted writers remain in ownership assertion diagnostics. Temporary event tracing and rerender/capture probes have been removed. Test builds audit host read/write binding modes against Naga's transitive entry-point write analysis, avoiding a separate handwritten access table. This audit adds no production shader-compilation work.

`RenderedImage::buffer()` is no longer public. The benchmark uses owner-checked `Engine::readback_image` instead; diagnostic readback contributes to engine counters but remains outside measured render/export intervals and activity deltas. Production display remains GPU-only.

## Validation

- `/tmp/foveonic-guards-ownership.log`: three GPU guard regressions pass (shared readers reject writes/recycling, last-owner recycling advances generations, ordered copy/read/overwrite without completion fences, and a shader write mislabeled as a read is rejected). Expected panics are caught by the two rejection tests.
- `/tmp/foveonic-guard-review-2`: frozen `d686266` GUI plus guarded processor and existing uncommitted tone/diagnostic changes. `/tmp/foveonic-guards-checkpoint-checks.log` passes release workspace tests, strict all-target Clippy, example builds, formatting and documentation-index checks. `/tmp/foveonic-guards-full-2.log` passes 54 ignored core and five GUI tests, including full-quality dragging, retained ownership, shader binding access audit and six-original exact float/golden/TIFF equality. Evidence: `.local/ui-responsiveness/guarded-equality-2`. Earlier intermediate suite evidence remains in `/tmp/foveonic-guards-full-1.log`.
- `/tmp/foveonic-guard-only-review` removes the experimental tone cache entirely while retaining the guarded implementation and bounded mismatch probes. `/tmp/foveonic-guard-only-checks.log` passes 52 ordinary core tests and strict workspace Clippy; it also captures the guard event trace with expected rejection assertions. `/tmp/foveonic-guard-only-gpu.log` passes all 53 ignored core tests. Exact output evidence: `.local/ui-responsiveness/guard-only-equality-1`.

Owner-requested repeat campaign: three additional full guard-only core GPU/fixture suites passed, 53 tests each. Logs: `/tmp/foveonic-guard-only-repeat-{2,3,4}.log`; exact-output directories: `.local/ui-responsiveness/guard-only-equality-{2,3,4}`. This is four passing full suites for the guard-only configuration, including its initial validation. A freshly rebuilt executable was copied to `/tmp/foveonic-guard-repeat/core-tests` and launched directly for all three repeats; `/tmp/foveonic-guard-repeat/manifest.json` records its SHA-256 and frozen source hashes. Test inventory confirms tone-reuse tests are absent. Execution was serial, with unchanged diagnostics and storage tracing off. An existing native GUI was left running; these are correctness trials, not controlled performance measurements.

The subsequent matched guarded/original-storage comparison passed twice in each variant. Its shared test list excluded the extra guard tests and shader-access audit, with tone reuse disabled in both. See [GPU output mismatch](./gpu-output-mismatch.md) for the retained sources, executable hashes, logs and limitations. The owner deferred the investigation: no specific cause or guard-based fix has been established.

Cleanup validation: `/tmp/foveonic-cleanup-review-1` freezes the cleaned processor and current concurrent inspection changes. Logs `/tmp/foveonic-cleanup-checks-{1,2}.log` pass release workspace tests (53 ordinary core, 64 GUI), strict all-target Clippy, examples, formatting and documentation checks; the second log follows removal of an obsolete explicit guard drop flagged by Clippy. `/tmp/foveonic-cleanup-gpu-1.log` passes all 52 ignored core and seven GUI GPU/fixture tests, including exact six-original float/golden/TIFF comparisons, guard access checks and paired inspection rendering. Evidence: `.local/ui-responsiveness/cleanup-equality-1`. Eight Python bundle/artwork tests pass. The snapshot includes the still-uncommitted tone prototype; this does not accept it for commit or establish a performance benefit. Temporary repeated-render/paired-tone benchmark tests, raw-root capture hooks, fixture filtering, failure-triggered rerenders and storage event tracing are absent. Exact-output failures retain a bounded numeric report without relaxing equality.

Checkpoint validation without tone reuse: `/tmp/foveonic-output-checkpoint-review/1-guarded-storage` passes release workspace tests (52 ordinary core, 61 GUI), all 52 core and five GUI GPU/fixture tests, strict all-target Clippy, example builds, formatting, documentation checks, and eight Python tests. Log: `/tmp/foveonic-output-checkpoint-guards.log`; exact six-original evidence: `.local/ui-responsiveness/output-checkpoint-guards-1`. A separate private target explicitly rebuilt the application crates; filesystem clones reused only dependency build artifacts. Inspection differences are excluded from this checkpoint.

No unexpected ownership violation was reported. These passes validate the migration against current tests; they do not establish the cause of intermittent fresh-render mismatches. No shaders, photographic arithmetic or production completion fences changed. Performance and memory effects have not been accepted from these correctness runs. Guard-only snapshots exclude concurrent GUI/inspection work; the combined cleanup snapshot includes it.
