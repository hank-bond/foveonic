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
- [ ] Quantify navigator drawing benefit independently of preparation cost.
- [ ] Extend continuous-drag coverage to noise/advanced controls and record native pointer-driven interaction/owner review.
- [ ] Review the remaining Detail subtree and UI stalls with the owner before expanding the implementation.
- [ ] Review the scoped diff before committing; preserve concurrent inspection changes.

## Architecture and discoveries

Processing modules declare exact, length-delimited byte recipes for the parameters actually consumed. `Operation` selects each output's key and dependency list. The interactive executor combines these with upstream generations and output identity, retaining cheap versions for all outputs but GPU buffers only for the active boundary. Engine/source identity is bound to the session. The initial cache safety limit is 1 GiB, not the total-memory target. Cache replacement can need a warm-up render.

Detail's complete continuation boundary includes filtered sensor tiles, column-conditioned original tiles, conditioned ratios, unchanged color tiles and (when sharpening uses it) signed signal. With sharpening it retains 885997568 bytes (845 MiB); without sharpening, 821657600 bytes (783.6 MiB). The initial direct-input implementation retained 548.5 MiB but unnecessarily recomputed unchanged downstream branches; this was corrected before closing the slice. Shared noise uniforms do not make Detail strength invalidate earlier filtering/conditioning. Tests check both necessary and unnecessary invalidation. A compound operation can run for an uncached side output with its primary already seeded: mask cached outputs out of execution demand and preserve their seeded values. The first replacement equality run exposed this invariant; the corrected executor passed all six originals.

Worker publication is image-first. Old successful same-photo/root frames can advance displayed feedback without adopting old requested settings. Scopes pair with the displayed revision and never replace a different image. Parameter identities classify coalescing for safe progressive completion; actual image invalidation remains in processing recipes. Interactive execution waits after an operation completes rather than submitting an entire obsolete graph. Already-submitted compound operations remain indivisible, and sharing the drawing queue still permits stalls.

Navigator generation runs separately after image publication and a 16 ms presentation deferral, only when no primary render is busy. One replaceable pending job produces a maximum-512-pixel-edge `Rgba8UnormSrgb` texture through the same presentation shader as the main viewport. Native egui registration happens on the UI thread; old IDs are freed. Keys include retained metadata identity, path/root, revision, channel, extent and signal space. Pan/zoom/scroll do not regenerate it. A compatible older tile can remain visible with an updating label; mismatched content cannot. Difference presentation is not active yet; its future mapping/bindings must be shared with this path too.

## Evidence

Baseline `/tmp/foveonic-ui-slider-baseline.log`: warm exposure/saturation/sharpen feedback approximately 746–768 ms. Superseded four-checkpoint evidence remains in `/tmp/foveonic-ui-slider-cache.log`, `/tmp/foveonic-ui-cache-equality.log` and `.local/ui-responsiveness/cache-equality-1`.

Final complete-boundary equality: `/tmp/foveonic-ui-complete-boundary-gpu.log` and `.local/ui-responsiveness/complete-boundary-equality-1`, including retained performance goldens. Earlier replacement evidence remains in `/tmp/foveonic-ui-active-equality-2.log`, `.local/ui-responsiveness/active-input-equality-2` and `.local/ui-responsiveness/final-equality-1`. Six originals matched full float output and retained goldens; cold/warm resized TIFF bytes also matched. Fifteen edits per original include five Detail strengths and ten earlier/late configuration changes. Earlier failed-run evidence is retained in `active-input-equality-1`.

Completed unprofiled Detail/image preparation, blue sign, `/tmp/foveonic-ui-complete-boundary-profile.log`: warm fresh renders 747–774 ms versus cached 119–131 ms, six dispatches including full-resolution texture preparation. First cached value was 490 ms, excluded from warm comparison. The earlier direct-input run, `/tmp/foveonic-ui-detail-profile.log`, measured fresh 752–803 ms versus cached 162–170 ms with ten dispatches. A separate timestamp-profiled run is `/tmp/foveonic-ui-detail-profile-2.log`; its synchronous durations are not acceptance wall times. The final boundary skips tiled color/ColorDQ and signed-signal preparation. Detail mixing, restoration/projection, sharpening, tone and display texture preparation remain; cached input reuse does not remove their runtime.

Worker `/tmp/foveonic-ui-active-worker.log`: warm exposure/saturation/sharpen approximately 66–85 ms; changing into Sharpen incurred a 911 ms boundary warm-up. Final complete-boundary continuous checks, `/tmp/foveonic-ui-complete-boundary-drag.log`: Detail published two frames during dragging and settled in 158 ms; Exposure published four and settled in 102 ms, both with matching scopes. The test also handles settlement during its last polling interval. This is worker evidence, not native physical-slider coverage.

Native isolated captures: `.local/ui-responsiveness/native-noise-1`, `native-detail-1` and `native-detail-idle-1`. `/tmp/foveonic-ui-native-detail-idle.log`: warm Detail input-to-UI 185–206 ms; first edit-group transition 554 ms. Warm busy frame gaps median 20.7 ms, p95 41.1 ms, maximum 53.2 ms. UI handling is short, but shared-queue rendering is not yet stall-free. `native-detail-1` was sampled with `vmmap` while rendering; its 1829 ms outlier is measurement interference, not an unperturbed latency result.

Final complete-boundary native evidence: `.local/ui-responsiveness/native-complete-boundary-1` and `/tmp/foveonic-ui-native-complete-boundary.log`. Warm Detail input-to-UI **125–143 ms**; first edit-group transition 548 ms. Warm busy frame gaps median 16.6 ms, p95 45.8 ms, maximum 46.7 ms. Normal settled physical footprint: **2.1 GiB**, `/tmp/foveonic-ui-native-complete-boundary-memory.log`, versus **1.8 GiB** in the superseded direct-input state. This trades approximately 235 MiB more retained input for faster feedback; retaining signed signal when sharpening is enabled adds a further 61 MiB. Native complete-boundary peak footprint approximately **3.5 GiB**; reconstruction/inspection smoke peak approximately **3.7 GiB**. Process RSS was approximately 0.94 GiB and excludes substantial GPU accounting: do not substitute it for total usage. These are local blue-sign measurements, not a universal bound across all scenes/backends. The observed settled state is close to the soft normal target; cold/replacement/in-flight peaks are larger.

Navigator synthetic mapping and completed preparation: `/tmp/foveonic-ui-navigator-gpu-2.log`, warm average 1.62 ms for 512×341. Native warm preparation commonly 1.7–3.3 ms, with occasional queue-contention/cold values up to 19 ms, logged separately after main publication. GPU fixture checks layers, linear/tone RGB, channels, scalar, ratio and signed mapping. Ordinary-painter tests check scroll clipping without compression, full UVs, overlay alignment and absence of navigator callbacks. Per-frame drawing benefit remains unquantified.

Final checks: `/tmp/foveonic-ui-complete-boundary-gpu.log` and `/tmp/foveonic-ui-final-{workspace,clippy,build,docs,python}.log`. Release tests passed: 48 ordinary core tests, 48 ignored core GPU/fixture tests, 42 ordinary GUI tests (including a concurrent scope-layout regression) and four ignored GUI GPU/worker tests, plus docs-index/CLI tests. Only the existing `block v0.1.6` future-incompatibility warning remains. No commits, pushes, installation or evidence deletion in this slice.


## Source-load profiling

Source profiling separates complete file reading into memory from total in-memory X3F decoding and total `Source::open` time. The old benchmark `decode_ms` included file reading; setup now names each duration explicitly. Nested decoder/parallel-plane timings overlap and are not additive to the decode total. File-read timing includes opening, size checks, allocation and the full read. OS cache state is uncontrolled; a first read in a process is not necessarily cold SD-card I/O. Existing local-drive evidence cannot establish card latency. The owner deferred new GUI diagnostics to a later pipeline-visualization view: prioritize performance and baseline editing UI.
