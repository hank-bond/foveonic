---
summary: "Measure full-quality slider feedback with active-edit input reuse, image-first replies, deferred navigator presentation, and safe supersession."
---

# Improve slider responsiveness

Requirements: [interactive rendering](../docs/specs/interactive-rendering.md), [render resources](../docs/specs/render-resources.md), [GUI workspace](../docs/specs/gui-workspace.md).

## Owner direction

The owner approved measurement, one active edit's GPU input cache, image-before-scopes delivery and safe cancellation. Detail is the immediate editing priority. Keep full-quality samples, one active/newest-pending request without debounce, and around 2 GB normal total memory. Navigator lag is acceptable. The earlier viewport/help checkpoint is `af5dd58`; full-pipeline optimization ends at `4bb4ff2`.

## Steps

- [x] Capture repeatable completed worker slider timing before changes.
- [x] Replace the four-checkpoint prototype with processing-owned effective keys, per-output versions and one bounded immutable input set; prime Detail when reconstruction is already selected.
- [x] Deliver images before revision-matched scopes; skip optional scope work for pending image requests.
- [x] Check supersession at preparation/operation boundaries; preserve non-cancellable export and same-photo/root progressive frames.
- [x] Integrate deferred GPU-generated navigator textures, ordinary egui painting, source/revision/mapping pairing and registration/freeing.
- [x] Check exact cold/cache/fresh outputs, retained performance goldens and resized TIFFs on six originals; exercise Detail, late edits, disabled stages, zero gain, WB, engine rebinding, retained ownership and interrupted graphs.
- [x] Check continuous worker drag progress, newest settlement, matching scopes, navigator mapping, pointer interaction and scrolling/overlay geometry.
- [x] Measure completed Detail feedback and native timing/normal memory versus transient peaks.
- [x] Run release workspace/examples, ignored GPU/worker, strict Clippy, documentation, formatting and Python checks.
- [x] Quantify synthetic navigator drawing independently of preparation cost.
- [x] Extend continuous worker dragging and exact cache/fresh coverage to noise presets and all advanced controls.
- [x] Record native pointer-driven interaction in the [optimization handoff](./editing-optimization-handoff.md). Owner accepted the implemented interface and closed the review checkpoint.
- [ ] Review the remaining Detail subtree and UI stalls with the owner before expanding the implementation.
- [x] Review and split the initial slice into tested commits; preserve concurrent inspection changes.
- [x] Review profiling, capture loading, organizer/control refinements and broader-control checks as separate commits.
- [x] Complete the matched diagnostic trials and defer the unresolved mismatch by owner decision; preserve evidence.
- [ ] Revisit CPU tone-table reuse at the end of optimization. It remains unaccepted.

## Architecture and discoveries

Processing modules declare exact, length-delimited byte recipes for the parameters actually consumed. `Operation` selects each output's key and dependency list. The interactive executor combines these with upstream generations and output identity, retaining cheap versions for all outputs but GPU buffers only for the active boundary. Engine/source identity is bound to the session. The initial cache safety limit is 1 GiB, not the total-memory target. Cache replacement can need a warm-up render.

Detail's complete continuation boundary includes filtered sensor tiles, column-conditioned original tiles, conditioned ratios, unchanged color tiles and (when sharpening uses it) signed signal. With sharpening it retains 885997568 bytes (845 MiB); without sharpening, 821657600 bytes (783.6 MiB). The initial direct-input implementation retained 548.5 MiB but unnecessarily recomputed unchanged downstream branches; this was corrected before closing the slice. Shared noise uniforms do not make Detail strength invalidate earlier filtering/conditioning. Tests check both necessary and unnecessary invalidation. A compound operation can run for an uncached side output with its primary already seeded: mask cached outputs out of execution demand and preserve their seeded values. The first replacement equality run exposed this invariant; the corrected executor passed all six originals.

Worker publication is image-first. Old successful same-photo/root frames can advance displayed feedback without adopting old requested settings. Scopes pair with the displayed revision and never replace a different image. Parameter identities classify coalescing for safe progressive completion; actual image invalidation remains in processing recipes. Interactive execution waits after an operation completes rather than submitting an entire obsolete graph. Already-submitted compound operations remain indivisible, and sharing the drawing queue still permits stalls.

Navigator generation runs separately after image publication and a 16 ms presentation deferral, only when no primary render is busy. One replaceable pending job produces a maximum-512-pixel-edge `Rgba8UnormSrgb` texture through the same presentation shader as the main viewport. Native egui registration happens on the UI thread; old IDs are freed. Keys include retained metadata identity, path/root, revision, channel, extent and signal space. Pan/zoom/scroll do not regenerate it. A compatible older tile can remain visible with an updating label; mismatched content cannot. Concurrent inspection-difference work shares presentation mapping/bindings with this path; its separate progress is recorded in [inspection differences](./archived/inspection-differences.md).

## Evidence

Baseline warm exposure/saturation/sharpen feedback was approximately 746–768 ms.

Final complete-boundary equality checks on six originals matched full float output and performance goldens. Cold/warm resized TIFF bytes also matched. Fifteen edits per original include five Detail strengths and ten earlier/late configuration changes.

Completed unprofiled Detail/image preparation on the blue sign: warm fresh renders 747–774 ms versus cached 119–131 ms, six dispatches including full-resolution texture preparation. First cached value was 490 ms, excluded from warm comparison. The earlier direct-input run measured fresh 752–803 ms versus cached 162–170 ms with ten dispatches. A separate timestamp-profiled run produced synchronous diagnostic durations, not acceptance wall times. The final boundary skips tiled color/ColorDQ and signed-signal preparation. Detail mixing, restoration/projection, sharpening, tone and display texture preparation remain; cached input reuse does not remove their runtime.

Worker measurements: warm exposure/saturation/sharpen approximately 66–85 ms; changing into Sharpen incurred a 911 ms boundary warm-up. Final complete-boundary continuous checks: Detail published two frames during dragging and settled in 158 ms; Exposure published four and settled in 102 ms, both with matching scopes. The test also handles settlement during its last polling interval. This is worker evidence, not native physical-slider coverage.

Initial isolated native measurements: warm Detail input-to-UI 185–206 ms; first edit-group transition 554 ms. Warm busy frame gaps median 20.7 ms, p95 41.1 ms, maximum 53.2 ms. UI handling is short, but shared-queue rendering is not yet stall-free. One native run was sampled with `vmmap` while rendering. Its 1829 ms outlier is measurement interference, not an unperturbed latency result.

Final complete-boundary native measurements: warm Detail input-to-UI **125–143 ms**; first edit-group transition 548 ms. Warm busy frame gaps median 16.6 ms, p95 45.8 ms, maximum 46.7 ms. Normal settled physical footprint: **2.1 GiB**, versus **1.8 GiB** in the superseded direct-input state. This trades approximately 235 MiB more retained input for faster feedback; retaining signed signal when sharpening is enabled adds a further 61 MiB. Native complete-boundary peak footprint approximately **3.5 GiB**; reconstruction/inspection smoke peak approximately **3.7 GiB**. Process RSS was approximately 0.94 GiB and excludes substantial GPU accounting: do not substitute it for total usage. These are local blue-sign measurements, not a universal bound across all scenes/backends. The observed settled state is close to the soft normal target; cold/replacement/in-flight peaks are larger.

Navigator synthetic mapping and completed preparation: warm average 1.62 ms for 512×341. Native warm preparation commonly 1.7–3.3 ms, with occasional queue-contention/cold values up to 19 ms, logged separately after main publication. GPU fixture checks layers, linear/tone RGB, channels, scalar, ratio and signed mapping. Ordinary-painter tests check scroll clipping without compression, full UVs, overlay alignment and absence of navigator callbacks. Five alternating completed 25-draw follow-up batches measured median 0.169 ms for repeated float presentation versus 0.122 ms for ordinary egui painting of the prepared tile. Native-sized synthetic source, 512×341 target; mapped pixels agree within one encoded code. This includes preparation/submission and amortized completion, excludes common overlays, and uses standalone passes rather than the complete native callback path. The saving is modest, not evidence that navigator replacement removes native frame stalls. Per-draw fences obscured the difference (~1.5 ms for either path).

Release tests passed: 48 ordinary core tests, 48 ignored core GPU/fixture tests, 42 ordinary GUI tests (including a concurrent scope-layout regression) and four ignored GUI GPU/worker tests, plus docs-index/CLI tests. Only the existing `block v0.1.6` future-incompatibility warning remains. The initial slice was reviewed in four commits (`1dee6e3`, `509a320`, `3e750e8`, `7458c38`) and fast-forwarded into local `main`.

## Follow-up measurements

Blue-sign full-float cached/fresh equality was checked for all alternative chroma/luma presets and two values of every advanced override, each followed by a Detail edit; retained original image stays immutable. All eleven controls publish ordered progressive frames and settle with exact artifact settings and matching scopes. Detail settles in 163 ms, Exposure 96 ms; noise/early overrides take 286–955 ms, while brightness/layer mixing takes 150–171 ms. Earlier-stage edits still need significant work; these checks establish progress/correctness, not fluid native interaction.

An unaccepted tone-reuse experiment retained one metadata-derived CPU tone table (256 KiB) keyed by processing-owned resolved camera curve, image depth and effective mode/control inputs. It is not present in the accepted source. Unchanged Detail/exposure moves reuse it; stateless rendering retains fresh preparation and photographic arithmetic is unchanged. The experiment measured ~0.015 ms reused host preparation; warm completed cached image runs 113–122 ms after initial warm-up. The alternating identical-GPU-path A/B measured regenerated/reused medians 137.8/135.5 ms with overlapping distributions: do not claim the whole latency difference as a repeatable improvement yet. Native feedback and queue contention still need measurement. The owner deferred tone-table reuse until the end of optimization.

Source profiling separates complete file reading into memory from total in-memory X3F decoding and total `Source::open` time. The old benchmark `decode_ms` included file reading; setup now names each duration explicitly. Nested decoder/parallel-plane timings overlap and are not additive to the decode total. File-read timing includes opening, size checks, allocation and the full read. OS cache state is uncontrolled; a first read in a process is not necessarily cold SD-card I/O. Existing local-drive evidence cannot establish card latency. The owner deferred new GUI diagnostics to a later pipeline-visualization view: prioritize performance and baseline editing UI.

## Ownership and deferred mismatch

The owner approved guarded GPU storage: shared readers, exclusive writes and explicit dispatch bindings replace raw processing handles and the recyclable flag. Implementation and regression evidence are recorded in [guarded GPU storage](./archived/guarded-gpu-storage.md). This is a structural ownership improvement, not an established mismatch fix.

The intermittent output difference is **unresolved and deferred**, not a blocker to continued editing work by owner decision. Normal-use symptoms were not observed, and both guarded and original-storage builds passed the latest matched trials. Findings, experiment limitations and all historical failed-run evidence are indexed in [GPU output mismatch](./gpu-output-mismatch.md). Temporary investigation paths are removed; exact-output assertions remain strict. Reopen on a new failure or owner report rather than continuing an unstructured repeat campaign.

## Reviewed follow-up checkpoint

Five independently rebuilt checkpoints cover read/decode profiling, early capture properties, organizer keyboard navigation, correction controls/help and broader drag/drawing checks. Commits: `6ceb3aa`, `648d266`, `8539202`, `e311e0e`, `d686266`. Each passes release workspace tests, strict all-target Clippy, example builds, formatting and documentation checks. Final stable snapshot: 52 ordinary core tests, 49 ignored core GPU/fixture tests, 61 ordinary GUI tests and five ignored GUI GPU/worker tests, plus CLI/docs-index and Python tests. GPU float-golden and resized TIFF comparisons pass. Passing this checkpoint does not resolve the earlier intermittent failure. The investigation is now deferred and its temporary code removed; tone reuse remains unaccepted for later review.

Review snapshots require explicit source rebuilds when sharing Cargo artifacts: unchanged relative paths and older copied mtimes allowed the first validation harness to reuse an earlier checkpoint binary. Subsequent checks used isolated artifacts and freshly touched snapshot sources to confirm the intended test inventories.
