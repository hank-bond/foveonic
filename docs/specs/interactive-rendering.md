---
summary: "Slider feedback uses full-quality active-edit inputs, bounded latest-request scheduling, image-first delivery, and deferred scopes and navigator presentation."
kind: reference
status: accepted
implementation: incomplete
---

# Interactive rendering

## Requirements

- Slider-to-image feedback **must** be the priority; some delay switching photos is acceptable. Measure completed input-to-image latency and UI frame stalls separately from submission and scope latency.
- Initial source-load profiling **must** separate complete raw-file reading into memory from in-memory decoding, and report total source-open wall time. Filesystem cache state **must** be identified as uncontrolled unless explicitly measured; warm local-drive reads do not establish slow SD-card latency. Warm editing reuses the decoded source and **must not** be charged its historical load timings.
- Interactive processing **must** preserve full-quality samples and the final/export recipe. Reduced-quality dragging previews are not selected.
- The active photo **may** retain immutable GPU checkpoints. Keys **must** include effective output-relevant parameters, upstream output versions, source and engine identity; retained data **must not** enter execution scratch. Cache memory **must** be bounded and released on photo change/clear.
- Retain one active edit's unchanged input boundary, rather than fixed late checkpoints or every earlier layer. Detail amount is the first priority. A control **may** affect several operations; processing-owned effective parameter keys and graph dependencies **must** identify affected work and invalidate its input cache correctly. Version metadata **may** remain for every output without retaining its buffers.
- Normal active-photo editing **should** use around 2 GB total memory, including source, display, retained inputs and scratch. This is a soft target; measure transient peaks separately. No cache-all preference is selected.
- Processing **must** remain off the UI thread, with one active request and one replaceable pending request. Superseded expensive work **must** stop at safe preparation/graph-stage boundaries; already submitted GPU work is not forcibly cancelled.
- A completed image **must** be deliverable without waiting for scopes. Scope work **must** yield to a pending image request and scope replies **must** match the displayed artifact revision.
- While dragging, completed older frames for the same photo and inspection root **may** provide progressive feedback, without overwriting current editing settings or being mislabeled as current. Other-photo/root and out-of-order replies **must** remain rejected. The newest request **must** eventually display after interaction stops.
- Export **must** remain non-cancellable, use current requested settings, and preserve immutable displayed results and provenance.
- Navigator presentation **must** use a small GPU-generated display-ready texture and ordinary egui image drawing, preserving overlays, interactions and the main viewport clipping fix. Regenerate only for content/mapping changes; no photographic readback or scope dependency. Main-image latency takes priority: navigator updates **may** lag during sliding, with explicit revision/mapping/source pairing and no other-photo/root mismatch.

## Implementation direction

Use an opt-in engine render session owning one immutable decoded source and one active-input cache (initial cache safety limit 1 GiB, within the total-memory target). Stateless CLI/export APIs keep the same executor. Keep stage keys beside processing preparation, combine them with upstream output versions in the graph, and retain only the unchanged inputs feeding affected operations throughout the required continuation, including unchanged branch inputs to downstream consumers. Prime Detail inputs for reconstructed photos; cache replacement may need a warm-up render. Keep one active request and the newest pending value; no debounce is needed. Navigator work is deferred and lower priority than main-image processing. Compare cold/cached float outputs and TIFFs exactly; include zero gain, disabled stages, setting invalidation, source/engine changes and retained-image checks.

Collect optional UI timing diagnostics and repeatable worker slider sequences. Native interaction evidence is separate from unit/worker conformance. Track findings in the [responsiveness plan](../../plans/ui-responsiveness.md). The first slice has six-original exact output evidence, progressive worker checks, native Detail timing and total-memory samples. Detail remains about 125–143 ms in the measured native workspace, with around 2.1 GiB settled total footprint; native pointer-driven acceptance, broader drag coverage, independent navigator drawing measurements and remaining shared-queue stalls are not complete.

New GUI diagnostics are deferred to a later pipeline-visualization view; editing performance and the baseline workspace take priority.
