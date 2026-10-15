---
summary: "Replace raw processing-buffer handles and recycling flags with guarded ownership, explicit bindings and allocation diagnostics."
---

# Guarded GPU storage

Implements the owner-approved guard approach in [render resources](../docs/specs/render-resources.md). This is an ownership improvement, not an established fix for the intermittent output mismatch.

- [x] Encapsulate raw buffers; shared read ownership and exclusive writes cover graph values, caches, retained images, and processing workspaces.
- [x] Make dispatch bindings explicit and retain borrowed guards through synchronous submission.
- [x] Replace recyclable flags with checked exclusive ownership when returning storage to scratch.
- [x] Identify allocations/generations and test reader rejection, last-reader release, binding access, and retained stability.
- [x] Move diagnostic image readback to the engine instead of exposing public raw writable handles.
- [x] Validate isolated ordinary/GPU suites and exact originals without weakening assertions.
- [x] Owner approved checkpointing; exclude the unaccepted tone-cache prototype.
- [x] Defer the unresolved mismatch and remove temporary investigation paths.
- [x] Validate the cleaned processor together with inspection differences.

## Implementation and limits

The single ordered device queue remains the submission contract. Guards do not detect missing graph edges or shader-internal races. No production completion fences, shaders, or photographic arithmetic changed.

The private [storage module](../crates/foveonic/src/gpu/storage.rs) holds shared `Arc` allocations. Mutable borrowing and `Arc::get_mut` acquire a non-clonable `WriteGuard`; retained owners block writes in release builds. Processing modules cannot access raw buffers. `Binding::Read` borrows shared ownership; `Binding::Write` holds a mutable lease through synchronous encode/submit, including internal reads of in-place stages.

Cache/image/graph clones share ownership. Scratch accepts exclusively owned ordinary floating-image layouts, not large tiled/atomic workspaces. Shape eligibility and content ownership are distinct checks. Allocation/generation IDs, size, labels, and last writers remain in assertion diagnostics. Temporary event tracing and capture/rerender probes are removed. Test-only Naga analysis audits transitive shader writes against host bindings without production reflection.

`RenderedImage::buffer()` is no longer public. Diagnostic callers use owner-checked `Engine::readback_image`; its counters are excluded from measured processing/export intervals. Production presentation stays GPU-only.

## Validation history

Methods, counts, and limitations are summarized here; local artifact paths are not prerequisites for this plan.

- Three GPU guard regressions passed: retained readers reject writes/recycling, last-owner recycling advances generations, ordered copy/read/overwrite needs no completion fence, and mislabeled shader writes are rejected. Expected rejection panics were caught.
- A frozen guarded processor with the then-unaccepted tone/diagnostic changes passed ordinary/strict checks and 54 ignored core plus five GUI tests, including six-original float/golden/TIFF equality. Earlier intermediate results were retained separately.
- A tone-free guard-only snapshot passed 52 ordinary core tests, strict checks, and all 53 ignored core tests. Three additional serial full suites passed 53 tests each. A fixed executable and source hashes established the repeat inventory; tone-reuse tests were absent. An existing GUI stayed running, so these were correctness trials, not controlled timing evidence.
- Matched guarded/original-storage comparisons passed twice per variant with tone reuse disabled. The common test list excluded guard-only regressions. The owner deferred diagnosis; see [GPU output mismatch](./gpu-output-mismatch.md).
- Cleaned combined processor/inspection validation passed 53 ordinary core and 64 GUI tests, all 52 ignored core and seven GUI tests, strict checks, examples, indexes/formatting, and eight Python tests. A second strict pass followed removal of an obsolete explicit guard drop. That snapshot contained tone reuse and did not accept it. Repeated-render/paired-tone benchmarks, raw-root capture, fixture filtering, failure rerenders, and event tracing were absent; bounded numeric reports retained exact equality.
- The separately rebuilt tone-free guard checkpoint passed 52 ordinary core and 61 GUI tests, all 52 core and five GUI GPU tests, strict checks/examples/indexes/formatting, and eight Python tests. Application crates were explicitly rebuilt; only dependency artifacts were reused. Inspection changes were excluded. It was committed as `d916440`.

No unexpected ownership violation was reported. These results validate current migration tests, not the mismatch's cause or a performance/memory improvement. Guard-only snapshots excluded concurrent inspection work; combined cleanup snapshots included it. More detailed historical validation prose is preserved in `d916440` and `088a8c2`.
