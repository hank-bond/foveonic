---
summary: "Capture native application behavior with shared Perfetto traces and compare explicit Off baselines, aggregate benchmarks and separate numerical checks."
kind: how-to
---

# Profiling

The [shared diagnostic requirements](./specs/performance-diagnostics.md) define native captures. The [processing profile requirements](./specs/profiling.md) also cover the aggregate reports used by existing scientific harnesses. Keep their output formats and measurement costs distinct.

| Mode | Shared native capture | Scheduling |
| --- | --- | --- |
| `off` | No optional trace | Normal baseline |
| `wall` | Host scopes, task flows, cache/I/O observations, delivery and resource counters | No profiling-only GPU waits |
| `gpu` | Wall observations plus supported device timestamp observations | Queries resolve after normal completion |

The GUI defaults to Off. Start a bounded capture from Settings → Diagnostics, or set `FOVEONIC_PROFILE=wall` or `gpu` for startup capture. Saving produces `trace.pftrace`, `manifest.json` and `summary.json` through the same SDK-backed Perfetto recorder for manual and scripted runs. Console output remains ordinary text. The active implementation and pending acceptance checks are tracked in the [diagnostics plan](../plans/performance-diagnostics.md).

Existing headless examples retain their declared aggregate JSON reports and CLI `--profile` or `--mode` options. Some explicitly synchronized GPU diagnostic routes remain useful for isolated pass measurements. They do not describe the synchronization behavior of shared native capture. Their relative host intervals cannot be placed on a common timeline across independent runs.

## Shared build artifacts

When switching worktrees that share a Cargo target, ensure the selected crate inputs are newer than cached artifacts before building. Touch that checkout's crate source inputs without changing contents, or clean only the affected package if needed. Verify compilation used the intended checkout and that the expected named tests ran with a nonzero count. A zero-test filtered result is not validation. Copy each measured executable before another worktree builds, and retain its hash and the relevant source hashes. Shared dependency artifacts may remain in place.

## Completed full processing

Build with `cargo build --release -p foveonic --example profile_pipeline`. Use a new export directory:

```sh
target/release/examples/profile_pipeline INPUT.x3f .local/profile-method-a \
  --warm 5 --wb Sunlight --mode Standard --reconstruction --profile wall \
  > .local/profile-method-a.jsonl
python3 scripts/profiling/summarize.py --input-kind benchmark-json .local/profile-method-a.jsonl
```

The runner shares the normal photographic option parser, so new algorithm controls should be added there once. Each iteration executes the complete selected final recipe from the decoded source, without an interactive retained boundary. Its first iteration is cold; warm repetitions are separate in the summary. GPU initialization is outside render intervals, and source file-cache state is uncontrolled. Choose enough repetitions for the actual comparison; one method/recipe per dataset is easiest to interpret.

Use `--capture-float OUTPUT NEW_FILE` to render a named floating image output after all timed iterations. The capture record includes source, settings, storage extent, color space, nominal linear white, active crop, rotation, data type and byte order. Raw samples use the recorded physical layout; apply the recorded view when comparing photographs. This separate render and readback are outside timing. The destination must not already exist.

Request `--profile gpu` for one additional synchronized diagnostic iteration after the ordinary warm repetitions. Its results remain in a separate mode group. Render and TIFF export have separate profiles/activity deltas. Optional `--goldens DIR` compares or creates full-float snapshots outside timed intervals; choose a distinct golden directory per intended recipe. Byte equality applies to output-equivalent optimizations, not to different denoisers or recovery methods. Evaluate photographic quality separately.

## Native-detail transitions

Compare detail-first refinement with direct Full from equivalent Merrill or Quattro Fast sessions:

```sh
cargo run --offline --release -p foveonic --example profile_smart_preview -- \
  INPUT.x3f .local/smart-transitions.json
```

The report destination must be new. Use `--region-first exposure --fractions 0.25,0.5 --repeats 3` for bounded regional comparisons. Add `--direct-only -- --tone-method universal --base-response natural` to measure a Fast-to-Full transition with Adaptive; regional Adaptive is unsupported. Render options after `--` override the camera settings used by every route. Each trial records epoch-millisecond bounds for its whole route, including the prior image and any verification, while reported durations use a monotonic clock. These bounds conservatively identify potential workload overlap.

For a separate instrumented region-first edit, add `--diagnostic wall` or `--diagnostic gpu` with `--region-first exposure` or `noise`. The first fraction selects the diagnostic viewport. `--diagnostic-only wall` or `gpu` skips the Off timing pairs. Pass `-- --merrill-banding-method directional` to select the Merrill directional method with its default row and column controls. The prior image uses Off. Diagnostic durations are not comparison timings. Node records distinguish required and available cached outputs, requested execution, published outputs, nested pass ranges, and shared executor pass ranges. Shared ranges are available only after successful graph trace resolution. Host durations exclude recipe preparation, cache storage and submission-window waits.

For a separate Full-to-Full warm edit, use `--full-edit exposure --diagnostic-only wall` or `gpu`. The available edits are exposure and noise. The initial Full image runs with Off, followed by the instrumented Full edit in the same session. This does not cover every control invalidation.

A Full-to-Full exposure sequence can be measured with`profile_smart_preview INPUT.x3f REPORT.json --full-edit exposure --benchmark-off --repeats 8 --diagnostic wall -- --reconstruction`. It primes a Full session, retains each previous image and texture through the next completed render, alternates +0.5 and original exposure, and excludes two warmup edits. The Off-mode runs report completed engine/view times. The optional Wall diagnostic runs separately in a newly primed session and records preparation spans; its durations are not Off-mode baselines. Source decoding and GPU initialization are outside both timings.

The default command uses Sunlight/Standard with camera-default controls and a central 992×1002 native-pixel detail tile. It excludes source decoding and GPU initialization. Each route starts from its own completed Fast overview. Five measured pairs follow one warmup pair, with alternating route order. Completed processing and display-texture preparation use profiling Off and retain the overview/detail textures as the GUI does. Separate GPU diagnostic runs contain stage timings and transfer/activity counters. Compare the first detail, direct Full, and additional Full continuation independently. Add the two progressive durations to measure when the whole image becomes Full. These are engine-to-ready measurements, not input-to-screen latency. Stored camera adjustments and different viewport sizes can change native app costs.

## Resource overlap

`profile_noise_resources` compares stateless processing (`async`) with a fresh editor-style session (`staged`). `switch` alternates cases using a fresh session per photograph. Each case is an object with `label`, `source` and the serialized `settings`, in a JSON array. An optional final argument selects a named image output instead of `final`, allowing a processing prefix to be measured separately. Source decoding and device setup precede measurement. Every interval ends with GPU completion. Settled memory sampling uses explicit holds after setup, completed processing, image/session release, source release and engine release.

```sh
cargo build --offline --release -p foveonic --example profile_noise_resources
target/release/examples/profile_noise_resources \
  CASES.json async off 6 0 .local/resource-times.jsonl
python3 scripts/profiling/resources.py \
  --binary target/release/examples/profile_noise_resources \
  --cases CASES.json --directory .local/resource-memory --route async --repeats 3
```

The macOS recorder starts only its own headless process, preserves raw `vmmap` snapshots, and samples outside render intervals. Run it separately from acceptance timings, builds and other measured renders. Peak physical footprint includes driver resources. The allocation window limits queued allocation traffic, not resident memory. Fresh sessions retain checkpoints while stateless renders do not, so compare completed and released states separately. The switch route loads all sources before measurement and does not emulate retaining the previous displayed photo during a transition.

## Manual native capture

Coordinate GPU work and pointer focus with the owner first. In Settings → Diagnostics, choose whether to include available GPU timestamps, press Start, reproduce the interaction and optionally add a named Mark. Press Stop and save… and choose a parent folder. The app stays open while the bounded capture is saved in a unique child folder. `FOVEONIC_DIAGNOSTICS_DIR` can select that parent folder for a controlled launch. An active or starting capture also saves on orderly application exit. Without a configured parent, exit saves under the platform temporary directory in `foveonic-captures/`. Closing during an already-requested save waits for its completion.

Before each app launch, the maintained launchers check actual executables for another Foveonic instance, compiler/build or measured workload. An existing app from another checkout also blocks the run. Resolve the competing work before retrying. The workload monitor continues through the owned app's lifetime. Any observed competing workload marks timing invalid separately from behavioral correctness. `--no-monitor` on the scenario runner also marks timing invalid, rather than treating an unchecked run as uncontended.

The maintained native launcher supplies isolated preferences, build/input identities and workload observations. It uses the app's capture controller and does not record a second event stream:

```sh
python3 scripts/profiling/native.py record \
  --binary target/release/Foveonic.app/Contents/MacOS/foveonic-gui \
  --fixture INPUT.x3f --directory .local/native-method-a --mode wall \
  --trace-processor PATH_TO_LOCAL_TRACE_PROCESSOR
```

The controlled launcher uses a unique `record-…` temporary cache namespace by default and records it in `launch.json`. Use `--cache-namespace NAME` for explicit reuse of one validated component. It does not use or clear the owner's normal cache. Starting a capture inside an ordinary running app keeps that app's existing cache unchanged.

Use the app's Stop and save… action to save while continuing to work, or close the recorded app normally to save on exit. The launcher keeps `launch.json`, raw `stdout.txt` and `stderr.txt`, and shared captures under `captures/`. It stops only its own app and monitor. Ctrl-C terminates that owned app and preserves partial artifacts. Forced termination cannot turn an unsaved recording into a complete capture. The source fingerprint describes checkout inputs at launch, not proof of the loaded binary. Capture manifests distinguish an on-disk executable hash from loaded-image attestation.

Set up the source, controls, window geometry, navigator/scopes and view scale. Supply actual screen coordinates for a physical macOS pointer drag:

```sh
python3 scripts/profiling/native.py drag --directory .local/native-method-a \
  --start 1414 --end 1532 --y 805 --steps 61 --seconds 2 --label detail-up
```

Coordinates are illustrative. External `pointer.jsonl` records dispatch evidence on its own clock. They are not merged into the trace as precise screen-presentation timestamps. Keep warm-up actions separate from measured ones. Memory sampling such as `vmmap -summary PID` belongs after timed interactions and settlement. The workload monitor detects known commands, not every competing GPU workload.

## Scripted native scenarios

The runner extends the existing opt-in native smoke driver. It launches the actual app and uses existing browser commands, edit descriptors and egui pointer dispatch. Each step labels `direct-command` versus `input-dispatch` coverage. These are not physical mouse tests. Native runs support one through eight photographs in Quality or Speed. Both modes choose working resolution from physical display demand. The transition journey checks both modes through zoom, selection, framing and Before viewing. Native detail retains the selected processing policy.

Supply an existing `profile_editing` case array containing `source` and optional serialized `settings`, or repeat `--source INPUT.x3f` for camera defaults:

```sh
python3 scripts/profiling/scenarios.py \
  --binary target/release/Foveonic.app/Contents/MacOS/foveonic-gui \
  --cases CASES.json --directory .local/native-warm-edit \
  --scenario warm-edit --count 8 --quality quality --mode wall \
  --trials 3 --repeats 5 --trace-processor PATH_TO_LOCAL_TRACE_PROCESSOR
```

The destination must be new. Inputs are hard-linked into each owned trial folder so recipes can be staged without modifying caller sidecars or copying RAW payloads. Source and destination must support hard links. The manifest records the selected inputs, recipes, hashes, hardware, quality, cache setup and automation coverage.

For a decoded-cache hit, expand `cache.decoded.lookup` in a Wall capture. Its identity, file-open and header scopes lead to the payload scope, followed by camera metadata parsing, source validation, recency and identity verification. The payload separates buffer allocation from sample reads, checksums and 16-bit-to-float conversion; camera metadata reads and checksums have their own scopes. Sample operations repeat once per bounded chunk. Sum matching sibling scopes within one lookup to see that lookup's elapsed time for an operation. Parent scopes include their children, and different lookups may overlap. These are elapsed host times, not CPU-active time. Allocation can be lazy, so first-touch page costs may appear during a later read or conversion. Compare capture overhead with a separate Off run.

For an original X3F, `source.original` separates file opening, metadata access, allocation and payload reading. `source.read.complete` reports actual bytes and read time. `decode.source` includes section parsing, sensor decoding, and camera-metadata decompression and parsing. In plane-compressed formats, `decode.sensor.planes` shows total elapsed time while `decode.plane.top`, `decode.plane.middle` and `decode.plane.bottom` appear on their actual worker threads. Each plane scope records its stored index, dimensions, sample count, compressed bytes and format; storage order is bottom, middle, top. Lookup preparation, buffer allocation and sample decoding have nested scopes. Plane intervals can overlap and must not be added together. Classic format 6 decodes interleaved layers by independent row groups, shown as `decode.sensor.rows` and `decode.rows`, rather than separate planes. The scope hierarchy report retains recorded attributes as well as task context so sizes can be compared alongside times.

| Scenario | Measured actions after setup |
| --- | --- |
| `initial` | Select the declared group with processing state reset and disk state as found in the isolated namespace |
| `warm-disk` | Select the group after local decoded inputs are ready, with processing state reset |
| `prefetched-selection` | Wait for idle small-grid preparation of ready neighbors, then select the set without clearing retained processing state and repeat exposure edits |
| `warm-edit` | Initial selection followed by repeated relative exposure edits |
| `cached-switch` | Cached selection changes, removal and re-addition around a pending edit |
| `rapid-supersession` | Repeated edits without waiting for each intermediate result |
| `memory-pressure` | Apply the existing memory-cache preference, then repeat unique exposure edits |
| `slider-drag` | Drive a real multi-photo slider through repeated pointer movements, inspect the painted readout and geometry, and wait for the final edited photographs |
| `selection-journey` | Edit, retarget, remove and re-add viewport targets while a computed image is held at delivery, then check unchanged selection reuse |
| `selection-pressure` | Run the selection journey, lower the memory budget, prove an inactive processing entry was evicted, then edit it and check rebuilding |
| `background-edit` | Start a frozen TIFF import through the actual batch worker, edit while it is active, settle foreground results, then separately await batch completion |
| `cache-workflow` | Fit two photos with automatic resolution, zoom the first to 200%, edit both and return to Fit, measuring visible delivery separately from background Whole settlement |

`--rapid-edits`, `--pressure-mib`, `--repeats`, `--step-timeout` and `--timeout` control these existing actions. A single active image and device are set up before capture. Input hashing warms the OS filesystem cache, so `initial` does not claim a cold storage device. Warm disk means a ready local decoded entry, not a retained GPU checkpoint. Actual payload reads establish which input path ran.

`prefetched-selection` requires at least two photographs and enabled disk and memory caches large enough for the declared set. All photographs must appear in the render worker's retained-source snapshot before selection. A matching retained native session may lend its immutable decoded source without being promoted. The measured selection must not read original or decoded payloads. This scenario validates foreground reuse after prework, independently of the time and resources spent preparing the set. Use `--memory-cache-bytes` to declare retained-memory capacity separately from `--disk-cache-bytes`. The workload key verifies the effective application settings.

Each run uses a unique `FOVEONIC_DIAGNOSTICS_CACHE_NAMESPACE` by default, isolated from the owner's normal temporary cache. `--cache-namespace NAME` explicitly reuses a validated single-component namespace for controlled trials. The runner does not clear the global cache. A new staged source path may still require a new entry, so interpret reuse from the recorded identity and reads rather than the namespace name alone.

Each trial saves the same shared capture files as manual recording, plus `evidence/scenario-result.json` and raw console output. The parent `regression.json` aggregates matched trial results. Use a separate new directory with `--mode off` for the baseline. Off results contain elapsed group measurements without a trace. Keep these results distinct from Wall/GPU attribution. Fresh numerical equality is not inferred from native frame/settings checks. Associate the same fixture/recipe manifest with a separate headless cached-versus-fresh verification outside timing.

Each native trial also writes `evidence/test-result.json`. Headless render-matrix cases write the same result shape to `<case>.test-result.json`, and both aggregate reports embed it as `test_result`. It identifies processing quality separately from numeric resolution, along with the driver, scenario, binary hash, GPU adapter, original source hash, observed processing family and artifact scale.

Application checks compute expected sampling independently from drawn image bounds, native extent and display density. Quality permits at most one physical display pixel per working sample, while Speed permits 1.5. Same-policy completed finer images remain sufficient. Missing geometry, mode, family or scale evidence is incomplete. Headless fixed-resolution probes require their exact numeric grid, including Classic's divisor-four smallest grid and Merrill/Quattro's divisor-eight grid. An application fixture staged under another filename is mapped back to its declared original source and hash.

The scale assertion verifies each artifact's reported scale; it does not independently measure image dimensions. The optional image assertion below compares complete floating storage and its geometry. The app scenario still owns its interaction and final-state checks. Neither assertion establishes physical input or screen-presentation coverage.

For a two-through-eight-photo slider gesture, add `--compare-images PATH_TO_PROFILE_PIPELINE` to `slider-drag`:

```sh
python3 scripts/profiling/scenarios.py \
  --binary target/release/foveonic-gui --cases CASES.json \
  --directory .local/slider-images --scenario slider-drag \
  --quality quality --mode off --trials 1 --background \
  --compare-images target/release/examples/profile_pipeline
```

Supply an explicit value for the selected numeric control in every starting recipe. Exposure also requires `exposure_auto: false`. The check verifies those initial values and checks the gesture's edit from pointer movement, separately from the rendered output. After all measured actions and any trace save, the existing image worker reads each settled frame's floating samples to `evidence/image-NN.f32`; the UI polls completion. After the app exits, the runner independently processes those sources with the exact verified settings. Diagnostic settings snapshots retain working scale, analysis tuning and other transient fields omitted from photo sidecars. The headless example accepts these through `--settings-snapshot FILE`, which cannot be combined with photographic or scale overrides.

The shared `image_samples` assertion checks every byte, complete buffer length, storage layout, color space, nominal white and view geometry. Results preserve both image paths and hashes, geometry differences and the first differing byte. A stale frame identity, missing capture or effective value that differs from the checked edit fails the trial. The runner checks GPU adapter names and records both binary hashes. Use binaries built from the same sources; exact floating results are not portable across GPU backends. Readback and fresh verification are outside interaction timing. Image comparison also supports the declared editing journeys.

### Cache workflow measurements

`cache-workflow` requires exactly two declared manual exposures. It starts after the ordinary initial selection at the configured processing mode. Automatic Fit sizing inherits that processing state. Shared edits apply offsets of +0.125, +0.25, +0.125 and +0.375 from each starting exposure. The test independently checks those edits, per-photo identity, rendition, demanded scale and native visible coverage. It uses direct application commands without delivery holds or timed image readbacks.

The default `--native-photo-count 1` zooms the first photo. Set `--native-photo-count 2` to zoom both to 200% and test two native processing working sets. The declared count is checked independently against app evidence and included in workload matching. Per-photo Whole observations in this workflow occur at group settlement; use Whole admission rows for individual delivery timing.

```sh
python3 scripts/profiling/scenarios.py \
  --binary target/release/foveonic-gui --cases REGIONAL_CASES.json \
  --directory .local/cache-workflow-2g --scenario cache-workflow \
  --count 2 --quality quality --mode off --trials 3 --background \
  --folder-mode browse --memory-cache-bytes 2147483648 --disk-cache-bytes 0 \
  --compare-images target/release/examples/profile_pipeline
```

Resolve `REGIONAL_CASES.json` from the tracked `regional_cases` recipes through `prepare_inputs.py`. Repeat with 4294967296 bytes for 4 GB, and with an enabled disk budget for primed-disk/cold-initial-processing coverage. Verify actual storage reads in separate Wall captures; readiness alone does not establish which payload was used. Use new output directories and `--mode wall --trace-processor PATH_TO_LOCAL_TRACE_PROCESSOR` for attribution. Existing cache namespace isolation applies.

Each trial's `cache_workflow` report separates per-photo first-current admissions from later retained-frame observations, foreground group completion and polled Whole settlement. Returning to Fit ends measurement; restoring the original processing mode for shared final-image assertions is cleanup. The aggregate action duration is deliberately absent because it would include unrelated work. The Off comparison tool uses the workflow's individual milestones instead.

Wall reports associate worker `memory.snapshot` trim pairs with current action windows and newly admitted task/revision identities. They also report cache events, native-source decisions, payload reads and dispatch encoding. A source read belongs to its completion timestamp, so it may have started in an earlier action. Unattributed snapshots remain in the trace analysis. Per-image backing/texture sizes describe references that can alias other owners; only the global inventory deduplicates them. These logical boundary snapshots do not measure transient peaks, driver residency or whole-process footprint. No memory-usage UI is required.

### Coordination journeys

Supply two through eight photographs with explicit manual starting exposures. `selection-journey` uses a small independent Rust model to check each photo's exposure after edits to changing subsets. It holds one computed image at the delivery boundary while real egui clicks change the editing selection, then releases it and waits for all latest results. The hold is released on scenario failure. Once a preview matches the current exposure, it must not revert to a prior exposure unless another edit changes the desired value. Final image verification uses the same fresh-processing path as `slider-drag`.

The hold is intentional harness work, recorded as `scenario.delivery_pause`. These step durations can compare the same controlled journey across builds but do not measure natural rendering throughput. The simpler `warm-edit` scenario remains the appropriate ordinary editing baseline.

For pressure coverage, select a budget small enough to evict inactive processing state for the supplied fixture set. The driver records actual retained-source snapshots before and after changing the budget. `evicted-edit` requires its target to be present before, absent after, and to execute photographic work when edited. Failure to observe eviction fails the case rather than claiming pressure coverage. A decoded disk-cache hit is valid rebuilding input. Reuse checks independently require zero target RAW payload reads and zero photographic dispatches. Unrelated background work is reported separately.

```sh
python3 scripts/profiling/scenarios.py \
  --binary target/release/foveonic-gui --cases MULTI_CASES.json \
  --directory .local/selection-pressure --scenario selection-pressure \
  --quality quality --mode wall --trials 1 --background \
  --disk-cache-bytes 0 --pressure-mib 64 \
  --trace-processor PATH_TO_LOCAL_TRACE_PROCESSOR \
  --compare-images target/release/examples/profile_pipeline
```

`background-edit` uses the existing Import worker to copy and render the first selected photograph into the trial's owned `background-output` directory. It observes a running batch that has begun executing before submitting the foreground exposure edit. UI samples record overlap with pending foreground work. A traced run additionally requires the edit marker to fall inside an actual `batch.photo` scope. The `background-finish` step waits for successful export completion after foreground settlement. This tests active background work independently of the `--background` option, which controls window placement.

Native results retain UI update timestamps during measured actions. `ui_latency` reports the maximum and nearest-rank p95 update gaps, including action transitions. `--max-ui-gap-ms N` optionally checks a caller-selected maximum. Missing samples remain incomplete. These gaps describe UI update cadence under the scenario's 20 ms repaint request, not physical input response or screen scanout. They exclude initial setup and image readback. Window size, viewport size, pixel density and per-step geometry are part of the matched workload key. Reports without complete geometry cannot establish valid latency comparisons.

### Behavioral slider checks

The `slider-drag` case requires at least two staged recipes. Supply different starting values by default, or set `--initial-values equal` to inspect the numeric readout while the photos move together. Select a control with its setting JSON pointer and its editing section, and select a theme with `--theme`. For example, with a two-photo case array whose exposure settings differ:

```sh
python3 scripts/profiling/scenarios.py \
  --binary target/release/foveonic-gui --cases CASES.json \
  --directory .local/slider-exposure --scenario slider-drag \
  --control-pointer /exposure --editing-section Tone --quality quality --mode off --trials 1
```

The input is dispatched through the real widget. Use `--drag-pattern scrub` for a back-and-forth gesture instead of the default forward drag. The driver moves the requested section into view and scrolls to an offscreen control before dragging; this setup is programmatic navigation, not a physical pointer test. `evidence/slider-drag.json` records the exact gesture, per-frame painted text and bounds, settings, frame times, render task identities and the controls the app drew. The check independently verifies relative changes, repeated movement, readout formatting and stable geometry before checking the delivered final settings. A missing, disabled or unreachable control fails explicitly and records the observed controls. A failure also requests a screenshot. Off timings for this case include the opt-in widget observer and scripted gesture, so compare them only to matched runs with the same observation enabled. A Wall run records the same work with Perfetto; it is not an Off latency baseline.

Run a declared matrix using the maintained 90 numeric control cases and an owner-supplied fixture file:

```sh
python3 scripts/profiling/behavioral.py \
  --binary target/release/foveonic-gui --fixtures FIXTURES.json \
  --cases scripts/profiling/slider_cases.json --directory .local/slider-matrix \
  --family merrill --case exposure-mixed --theme dark
```

Repeat `--family`, `--fixture`, `--case` or `--theme` to select more than one value. `--list` prints the selected runs without starting the app. Use an explicit `--full` for every declared combination, such as before a release; an unfiltered run is otherwise rejected. Select relevant cases for ordinary changes. Unknown or inapplicable selections fail rather than silently passing.

The fixture file is an array such as `[{"name":"merrill-pair","family":"merrill","raw_size":"high","sources":["FIRST.x3f","SECOND.x3f"]}]`. Each fixture contains two through eight distinct source paths and each case provides a recipe for each selected photo. Camera families are `classic`, `merrill`, `quattro` and `tiger`. A fixture may describe a different RAW size in `raw_size`. The case file selects pointers, sections, optional applicable families and themes, per-photo settings, and `processing_qualities` containing `quality` and/or `speed`. The maintained cases run both modes. Each mode has separate outputs and coverage. Inputs and sidecars are staged into owned trial folders. Add `--compare-images PATH_TO_PROFILE_PIPELINE` to compare settled slider results with fresh processing. The matrix disables its decoded disk cache by default to avoid accumulating a new cache for every staged source. Use `--disk-cache-bytes` for an explicit cache budget, or the individual native scenarios for cache-specific checks. `behavioral-report.json` identifies each run, missing families and controls that were observed but not tested. Its inventory covers only the controls drawn by the declared recipes; it does not establish coverage of hidden modes, other RAW sizes or physical mouse input. A Tiger fixture made from two aliases of the same available original tests multi-photo interaction, not two independent sensor samples.

Behavioral matrices request a background window by default. On macOS it stays below normal windows and does not request application activation. Pass `--foreground` to watch a run. Individual scenarios use a foreground window unless given `--background`. The selected window mode is recorded with the workload; compare latency only between runs with the same mode, since the operating system can schedule obscured windows differently.

### Exact render matrix

The maintained headless `profile_pipeline` example reports float-sample digests and image geometry outside its timed render. `render_matrix.py` records and checks these results against full float buffers. It refuses a comparison if a baseline buffer is missing or changed, and checks source hashes, recipe options, effective scale and image geometry as well as every output byte. Candidate samples stay in the new run directory; the baseline is read-only. Baseline and app comparisons use the same image assertion.

```sh
cargo build --release --offline -p foveonic --example profile_pipeline
python3 scripts/profiling/render_matrix.py --binary target/release/examples/profile_pipeline \
  --cases RENDER_CASES.json --directory .local/render-baseline --require-coverage
python3 scripts/profiling/render_matrix.py --binary target/release/examples/profile_pipeline \
  --cases RENDER_CASES.json --directory .local/render-candidate \
  --baseline .local/render-baseline --require-coverage
```

`RENDER_CASES.json` is an array of objects with a unique `label`, camera `family`, `source`, and optional `raw_size`, `resolutions`, `processing_qualities` and `options`. Numeric resolutions are `full`, `fast`, `draft` and `thumb`. They are internal size probes, separate from the application's two processing modes. The default crosses all four resolutions with `quality` and `speed`, using Sunlight white balance and Standard color mode. For example, `{"label":"merrill-haze","family":"merrill","source":"INPUT.x3f","resolutions":["full","fast"],"processing_qualities":["quality","speed"],"options":["--dehaze","35","--haze-color","auto"]}` declares both policies at two working grids. The runner owns `--resolution` and `--processing-quality`.

The coverage gate requires three distinct source contents each for Classic, Merrill and Quattro, plus one for Tiger, with all resolutions and both modes for every case. Smaller matrices report omitted families, resolutions and modes. Family and RAW size come from the manifest and are not independently verified. Both recording and comparison use new directories and save per-case output and a manifest. The new schema rejects historical `qualities` fields instead of inferring their meaning.

Exact comparisons use immutable approved outputs for the same resolution, processing mode, input and recipe. Repeating a newly recorded candidate against itself establishes reproducibility, not approval of a changed algorithm. The maintained test-input manifest retains all ten camera fixtures and adds Merrill adaptive, HDR, local-color and automatic Dehaze cases for dense-processing evaluation. Its source hashes remain unchanged. Paired Quality/Speed outputs report differing sample counts, maximum and mean absolute differences, RMS, image geometry and both hashes outside timed renders. These comparisons remain marked `visual_acceptance: unreviewed`. Any NaN or infinity fails the pair and matrix, with difference metrics withheld. Inspect representative crops before approving dense results. Their differences never replace a baseline or become an automatic pass for photographic quality. GPU/backend changes can affect bit equality.

The [implementation plan](../plans/performance-diagnostics.md) records the native scenario, smoke, numerical and overhead checks. Physical manual-control verification remains pending desktop availability. Portable tests alone do not establish native behavior or latency.

## Compare controlled Off baselines

New scenario reports include one matched-workload key derived from ordered RAW hashes, declared recipe hashes, actual initial settings/scales, case parameters, recorded host hardware and measurement setup. Staged paths, task revisions and build identities do not define the work. The runner rejects mixed trial keys and preserves a failed report. Full initial settings and per-trial build/artifact identities remain in the existing results.

Run matched baseline and candidate scenarios in separate new directories with `--mode off`, repeated trials and the monitor enabled. Compare their reports:

```sh
python3 scripts/profiling/compare.py \
  .local/baseline/regression.json .local/candidate/regression.json \
  --output .local/comparison.json
```

The command requires complete behavioral success, checked/uncontended timing for every trial, identical workload keys and matching action groups. It reports median and nearest-rank p95 deltas by action, with sample counts and preserved report/build/artifact identities. Wall/GPU runs, failed or partial trials, mixed inputs/settings and contended or unmonitored timings are rejected. Reports created before workload keys were recorded are not accepted.

Without a budget it reports deltas without a regression verdict. An explicit `--max-regression-percent 10` checks both median and p95 against a caller-selected 10% maximum increase. `--budget warm-edit=5` overrides the limit for that action. Limits are examples, not recommended thresholds. A budget violation returns a nonzero status after emitting the comparison. Choose budgets from controlled repeated variability and review the raw trials. These samples do not establish statistical significance.

Hardware matching includes the recorded host descriptor and actual application adapter, backend, vendor, device and related hardware fields from the native result. Window size, viewport size, pixel density and per-step geometry are recorded in the workload key. Match other operating conditions separately when making a performance claim. The comparison command does not add app capture work.

## Capture an existing smoke suite

Existing opt-in smoke modes also use the shared startup recorder. Add `FOVEONIC_PROFILE=wall` or `gpu` to an existing smoke invocation. After its original assertions and optional final hold complete, the driver saves to `FOVEONIC_GUI_SMOKE_DIR/capture` before closing. Off runs preserve their ordinary screenshot/check behavior without starting a recorder.

```sh
FOVEONIC_GUI_SMOKE_DIR="$PWD/.local/smoke-smart-trace" \
FOVEONIC_GUI_SMOKE_SMART=1 FOVEONIC_PROFILE=wall \
target/release/Foveonic.app/Contents/MacOS/foveonic-gui INPUT.x3f
```

Use each suite's existing fixture and setup requirements. Capture does not broaden its assertions or impose a different photo-completion order. `smoke.step` records suite, target, mode, begin and checked phases. Checked frames retain original delivered identities separately from presentation revision. The analyzer requires all declared checks and the terminal `smoke.completed` marker. It preserves intentional pending-preview checks rather than requiring every target to have an idle worker.

`smoke.tick` scopes identify driver work, with screenshot PNG encoding under `smoke.artifact.png`. These smoke intervals include existing screenshot readback and artifact work, so use the named scenarios or Off comparisons for latency measurements. A fatal assertion after App construction preserves an active smoke capture during unwinding, emits `smoke.failed` and records the failure in the manifest. This failure-only path waits for the existing background save after the measured interaction has failed. The analyzer reports behavioral failure even if no step marker was reached or trace saving completed successfully. External termination and startup failures before App construction can still prevent a save.

## Analyze a shared capture

Open `trace.pftrace` in the Perfetto viewer for interactive scopes, threads, task flows and counters. Use a separately acquired, explicit local [Trace Processor](https://perfetto.dev/docs/reference/trace-processor-cli) executable for machine analysis. Normal runs do not download or install it:

```sh
python3 scripts/profiling/trace_analysis.py \
  .local/native-warm-edit/trial-00/capture \
  --trace-processor PATH_TO_LOCAL_TRACE_PROCESSOR
```

The analyzer preserves its SQL, CSV results and tool stderr under `analysis/`, records the tool version/hash and attaches results to `summary.json`. `analysis/scope-hierarchy.json` retains recorded slice parents, depth, ancestor names and process/thread metadata. These are instrumented scope relationships, not sampled function stacks. The compact summary groups inclusive elapsed stages by exact task/photo lineage and GPU pass durations by node/submission/clock. Nested stage totals remain nonadditive. It checks expected photo/task/revision identities, actual scale, desired-work/progress settlement and original versus local RAW payload reads. Every current photo must also have a matching native encoding event at the requested scale, including an old task reused by cached switching. Per-photo lifecycle observations distinguish recorded queue residence, render elapsed time, request-to-UI latency and receipt-to-encoding delay without adding overlapping measures. Preview and metadata reads remain separate storage kinds. Cached image reuse can have no new receipt event, so the report does not invent one.

First-current and all-current observations, whole-group completion, per-photo median/tail values, task outcomes and counter peaks use the recorded common host clock. Original delivered task/revision/photo identities remain distinct from later presentation revisions. Draw milestones refer to native command encoding, not physical screen presentation. Node identity is present when execution knows the graph node.

A successful save sets `complete`, not loss-free validity. Incremental descriptors refresh every second so recent ring-buffer data can still be decoded after overflow. Overwritten captures remain invalid for complete-work or latency claims. The analyzer must find Trace Processor buffer statistics and inspect nonzero data-loss/error records, per-buffer overwritten/discarded data and explicit emission-transition drops. SDK service-lifetime discard warnings stay visible and cannot independently identify loss in this session. Check `validity`, `behavior_valid` and any separate GPU query status. A manual recording has no invented scenario pass/fail result.

GPU observations retain raw ticks, period, clock ID, submission and known task/node provenance. Unavailable calibration remains explicit. Query starts, resolutions, pending work, drops and failures describe GPU observation completeness separately from host trace validity. The report retains every unresolved query identity and groups status by purpose, so completed current images cannot hide an auxiliary Scopes or Navigator tail. Stopping a capture can naturally cut that tail. Aggregate GPU completeness remains false without adding a GPU wait to finish the recording. Host scopes are elapsed intervals, not CPU-active samples. Nested or parallel scopes cannot be added. Retrospective elapsed observations use their explicit `start_ns`, `end_ns` and `duration_ns` fields. GPU times are not moved onto fabricated host positions, and wall time minus GPU duration is not a measurement of queue waiting.

Use `python3 scripts/profiling/summarize.py --input-kind benchmark-json REPORT.jsonl` for a declared aggregate headless report. It summarizes that dataset without converting independent relative clocks into a shared timeline. Shared captures use `trace_analysis.py`. Run portable tool checks with `python3 -m unittest discover -s scripts/profiling -p 'test_*.py'`.

## Capture an early loading stall

Set `FOVEONIC_PROFILE=wall` on a direct native launch, or use `native.py record --mode wall`, before reproducing the loading problem. Startup capture uses the same controller. Stop and save from Settings while the app remains open. Task admission, source access and image delivery can then be queried from the shared trace. For a later loading stall, start a manual capture before opening the folder. Console redirection alone does not save a trace.

## Headless browser CPU work

Run `cargo test --release -p foveonic-gui browser_cpu_profile -- --ignored --nocapture` without concurrent measured work. The synthetic 1,000, 10,000 and 50,000-path folders report JSON lines with seven per-operation batch samples in nanoseconds after one warmup. They cover one, 100 and all selected photographs, review counts, membership, alternating import marks, index rebuilding and reverse sorting. The `_scan` operations model the former folder-wide work; they do not run an earlier binary. This isolates host-side browser operations and does not measure native input, drawing, file discovery or GPU work. Run separate trials for comparisons and retain the complete output and build context.

Run `cargo test --release -p foveonic-gui browser_discovery_profile -- --ignored --nocapture` to measure recursive discovery and `Browser::open` separately on generated empty files. The test creates and removes its own repository-local fixture. It reports five timed samples after warmup for 1,000 files in one child folder and 10,000 files across 100 child folders. `Browser::open` also starts metadata work, which may overlap its measurements. Filesystem and OS caches are uncontrolled; these local results do not establish removable-card or network-drive latency.

Run `cargo test --release -p foveonic-gui browser_snapshot_profile -- --ignored --nocapture` to measure applying an already-discovered folder on the UI thread. The synthetic paths do not exist, so background metadata failures and worker overlap may affect timings. Cloning the input fixture occurs outside each timed interval. This records snapshot installation only, not native input or rendering.

Run `cargo test --release -p foveonic-gui batch_progress_profile -- --ignored --nocapture` to compare cloning completed batch history with reading its small status summary. Synthetic records contain one imported path and two operation messages per photo, with one error per hundred photos. The test reports host-only batch samples; it does not include locking, real imports or native interaction. The UI still clones full progress once per frame for import-history replay and event collection.

## Startup host work

Startup observations begin after eframe creates the native window and device. The shared capture records App construction and later update work when recording is active. Existing engine-initialization spans distinguish pipeline creation and calibration-map preparation. Nested host spans overlap and must not be added. Capture-start readiness bounds what can be observed, so startup capture does not imply coverage before the recorder becomes active. The first update is not a screen-presentation measurement.

## Cached multi-photo edits and host scheduling

The internal Thumb resolution uses camera-specific sizing: Classic uses one-quarter dimensions, while Merrill and Quattro use one-eighth. Explicit `--scale-divisor` requests retain their numeric divisor. Mixed-family runs report each photograph's resolved working scale separately.

Build the maintained harness once in optimized mode. Use the same executable and distinct originals for each schedule. The example below uses shell placeholders for original paths:

```sh
cargo build --offline --release -p foveonic --example profile_editing
shasum -a 256 target/release/examples/profile_editing
mkdir -p .local/editing-profile
target/release/examples/profile_editing .local/editing-profile/serial.jsonl \
  --resolution thumb --workers 1 --repeats 7 --verify PHOTO_A.x3f PHOTO_B.x3f
target/release/examples/profile_editing .local/editing-profile/concurrent.jsonl \
  --resolution thumb --workers 2 --repeats 7 --verify PHOTO_A.x3f PHOTO_B.x3f
python3 scripts/profiling/summarize.py --input-kind benchmark-json .local/editing-profile/serial.jsonl
```

Add up to the intended real photo-set size. The runner rejects repeated canonical paths and existing report files. It loads each original and warms one retained session on its own executor. Executors share compiled kernels and the ordered device queue. A fixed Rayon pool schedules complete renders with the requested host concurrency. Each measured edit alternates exposure by ±0.25 EV around the supplied base, or chroma reconstruction strength between 1 and 2 with `--edit noise`. Warmup sets chroma to zero for the latter. Optional photographic CLI controls follow `--`. Use `--sidecars` to require each saved recipe and `--gui-analysis` to match the GUI’s automatic-analysis request. Use `--sweep` for unique exposure values instead of alternating two values. Named internal resolution choices map to sensor divisors 1, 2, 4 and 8. `--scale-divisor N` accepts any positive power of two. Reports record requested and effective divisors with the exact settings.

The default mode is Off. Repeat a matched trial with `--mode wall` for attribution. GPU diagnostics require `--mode gpu --workers 1` because their timestamp resolution perturbs scheduling. Repeats share warm sessions, while separate invocations have independent warmups. Setup and first renders are reported separately from edits. Summaries also separate the first edit from later settled edits because restart-boundary cache priming may change its cost. `--verify` compares each final cached linear image byte for byte with an independent fresh render after the timed work. It also records CRC32 values for comparison across matched schedules. This checks cache/scheduling consistency for those samples. It does not establish photographic quality at reduced scales. Verification temporarily adds one fresh render's resources to the measured working set.

The default logical retained-session budget is 2048 MiB. Override it explicitly with `--cache-budget-mib N` for an intentional larger experiment. The budget is checked after each warmup and edit group, rather than reserving device memory in advance. Originals, retained final images/textures, in-flight scratch and driver memory add to this figure. Use settled resource sampling separately. Use the app’s actual budget for comparable photo-switching measurements.

Use `--reduced --workers 1 --cache-budget-mib 128` to measure source-free reduced sessions in the shared processing working set instead of independent native sessions. This route seals recipe-declared camera dependencies, releases their originals and reports deduplicated GPU, session CPU and engine-preparation storage. It uses one engine and a shared memory target across active and inactive sessions. The required foreground session is protected, so retained resources can exceed the target. `active_bytes` and `historical_bytes` describe the session ownership. An evicted or invalidated upstream checkpoint reloads the original during the edit and records `reduced_reloaded`. Refresh preserves every remaining valid dependency cut. Downstream exposure edits should keep it warm. Fresh verification reopens originals after timing. Keep whole-process memory trials separate from verification, which temporarily adds a native render. The GUI uses the same processing working-set owner, with its own background loading and display resources.

Each ready-set report includes `session_resources` for retained photographs. Its fields separate immutable metadata, decoded source data, session ownership, GPU checkpoints, graph structure, shared parameter payloads and retained preparation. `output_parameters` attributes parameter payloads to graph outputs and versions. Inspect these fields when a small preview retains unexpectedly large CPU data. Shared allocations can appear in several per-photo or per-output descriptions, so use aggregate working-set totals for a deduplicated footprint.

The GUI combines native and reduced processing owners with held images, metadata, display textures, browser previews, navigator and scopes under the memory-cache preference. Budget pressure first trims obsolete optional versions and frontiers, then cold preparation and inactive actual-use history. Required live inputs can exceed the preference and are reported as over budget. The preference controls retained photographic data, rather than total process RSS. File bytes held by disk-cache writes, decoding scratch, GPU scratch, allocator overhead and driver allocations still affect application peak memory.

For application acceptance, measure an isolated native launch separately from timings. Use one active photograph, then one through eight viewport editing targets across supported families. Wait for all requested images, edit exposure repeatedly with unique values, and sample settled RSS and macOS physical footprint. Keep cold loading, switching, warm editing and fresh-output verification distinct. Retain the displayed target count, actual delivered scale and logical inventory alongside process samples. A headless retained-cache trial alone does not establish the GUI's steady or peak application memory.

The opt-in native multi-edit smoke driver uses a folder containing eight original files. Pass one file as the startup argument. Use a new capture directory for each run:

```sh
FOVEONIC_GUI_SMOKE_DIR="$PWD/.local/multi-acceptance" \
FOVEONIC_GUI_SMOKE_MULTI=1 FOVEONIC_GUI_SMOKE_MULTI_SWEEP=50 \
FOVEONIC_GUI_SMOKE_HOLD_SECONDS=30 FOVEONIC_PROFILE=off \
target/release/Foveonic.app/Contents/MacOS/foveonic-gui PHOTO_SET/FIRST.x3f
```

The driver checks selection gestures, retained texture identity, relative edits, removal and re-addition, Before/Current transitions, mixed choices and completed progress. Its JSON captures include the process ID, requested and delivered settings, delivered scale, retained inventory and edit-group durations. The optional final hold permits settled process-memory sampling after `multi-choice.json` appears. Use a separate Wall run to investigate repeated submissions. Profiling records and screenshots add overhead and are not scanout measurements.

The ignored `decode_cache::tests::parallel_prefetch_throughput_preserves_exact_samples_and_disk_budget` test measures six Quattro sources through the host-sized CPU pool with three admitted photographs. Set `QUATTRO_X3F` to an authorized original and run it alone in release mode. Its readiness interval includes CPU preparation and disk publication. Process user/system time and physical footprint describe different costs. A host-sized pool permits parallel work without promising full utilization throughout serial dependencies or disk waits.

Every photo reports its host start/end relative to its edit group. Whole-group completion and first-photo completion measure throughput and initial feedback separately. Existing device-wide completion waits may include other executors' work, so individual concurrent durations are not isolated GPU costs. Host concurrency does not implement stage-major scheduling across photographs. Use this baseline to justify any later executor/submission changes.

Use the shared native trace for a causal application timeline. Aggregate headless report timestamps retain their own group origins. They are not exported with synthetic placement between groups. GPU pass durations in aggregate reports remain isolated diagnostic measurements with their declared synchronization cost.

## Graph checkpoints

Use `profile_checkpoints` to connect processing costs with retained numerical checkpoints. It opens each decoded source once, warms compiled kernels, then gives each control/trial a fresh executor and session. Each independent sequence runs base, unchanged, edit A, distinct edit B and return A. The retained source, compiled kernels and device queue are shared. Per-photo preparation starts fresh.

Prepare the checked-in recipes using the [reproducible input procedure](./specs/performance-diagnostics.md#reproducible-test-inputs), then run a focused cell:

```sh
cargo build --release --offline -p foveonic --example profile_checkpoints
target/release/examples/profile_checkpoints \
  .local/test-inputs/checkpoint-cases.json .local/checkpoints-off.jsonl \
  --case sign-natural --control exposure --mode off --repeats 3
python3 scripts/profiling/checkpoints.py .local/checkpoints-off.jsonl \
  --output .local/checkpoints-off-report.json
```

Use new paths for each capture and report. `--scale-divisor 2`, `4` or `8` selects smaller sensor grids. `--regional` instead uses each recipe's declared native visible region. Omit the case/control filters to cover all declared recipes and edit axes. Each capture records the effective settings, source identity and adapter. The maintained recipes include Merrill color/Natural, monochrome/Natural and both noise approaches. They measure rendering directly through a source-retaining `RenderSession`, including at reduced divisors. These cells do not measure the app's source-free reduced-session owner, GUI texture creation or group scheduling.

Run separate `--mode wall` and `--mode gpu` captures for attribution. The example explicitly waits after each render in every mode to measure completed processing. GPU mode also requests timestamp diagnostics. Only unverified Off runs are ordinary latency evidence. `--verify` compares every phase's actual pixels and geometry with fresh processing after timing and marks the entire capture unsuitable for latency comparison. Readbacks are released immediately. Neither the diagnostic records nor the report retain processing buffers.

The report preserves graph topology, output allocation/generation identities, cache version ancestry, overlapping retention flags, node host/GPU timing and graph/scratch/cache boundary ownership. It deduplicates physical storage by allocation identity. CPU source, metadata, preparation and parameter resources remain separate. Boundary bytes exclude node-internal transient peaks, frontend allocations and driver residency. Optional node observation overhead is reported separately, but it does not include every trace or graph-metadata cost. Node total spans include some diagnostic work, and the observation counter includes scans before those spans. Do not subtract the entire observation counter from node totals. Compare separate Off and Wall captures to assess the broader diagnostic effect.

The analyzer reports missing controls, trials, phases and required verification. Unequal verification fails the command. Its ancestry rankings describe measured contributing GPU work per byte and must not be interpreted as marginal latency saved by a new cache policy. Shared producers and preparation outside graph execution require subsequent targeted experiments. The [checkpoint plan](../plans/archived/checkpoint-profiling.md) records those findings and follow-ups.

## Explain checkpoints in app captures

Start a Wall capture from Settings → Diagnostics and use the app normally, including Import, multi-edit, quality changes, zoom/loupe or Before. Stop and save the capture, then run the existing reader:

```sh
python3 scripts/profiling/trace_analysis.py CAPTURE_DIRECTORY --trace-processor TRACE_PROCESSOR
```

`analysis/checkpoint-explanations.json` links each graph's requested outputs, dependency table, exact cache versions and observed availability with executed node costs. It identifies producers that ran for missing outputs despite cached siblings. The entry inventory describes unique referenced GPU allocations, including data retained before recording started. Task rows include existing render/queue/UI observations, actual processing route and worker budget boundaries. Native encoding observations identify command encoding rather than physical presentation.

A loss-free capture establishes validity of the recorded history, not completeness before capture started. An absent prior eviction stays unknown. Partial or cancelled graphs are explicitly incomplete, and graph observations without a retained header are listed as orphans. Cache-instance IDs distinguish native, reduced and replacement caches for one photo. Allocation aliases count once within an inventory. Do not sum per-cache inventories or eviction reference bytes into memory freed or a process peak. Existing worker totals provide the deduplicated budget observations.

This report explains actual work. It does not simulate retention policy, estimate counterfactual latency savings or track every reference lifetime. Native capture adds one scalar numerical-cache snapshot per graph and demand metadata to existing output events. It does not enable the headless profiler's per-node memory scans or retain processing buffers. Use separate matched Off runs to assess capture overhead.

The maintained warm-capture check starts recording after exposure preparation, then changes detail. Its source comes from the example release:

```sh
FOVEONIC_FIXTURE_DIR=.local/example-photographs \
FOVEONIC_CHECKPOINT_TRACE_DIR="$PWD/.local/warm-checkpoint-capture" \
cargo test --release --offline -p foveonic --lib native_checkpoint_capture_preserves_warm_edit_work_and_pixels -- --ignored --test-threads=1
python3 scripts/profiling/trace_analysis.py .local/warm-checkpoint-capture --trace-processor TRACE_PROCESSOR
FOVEONIC_CHECKPOINT_TRACE_DIR="$PWD/.local/warm-checkpoint-capture" \
python3 -m unittest discover -s scripts/profiling -p 'test_checkpoint_trace.py'
```

Choose a new capture directory. The Rust check compares activity and exact pixels with ordinary execution. The Python integration check reads the actual native metadata and validates graph completeness, version links and retained dependency tables. Portable tests separately cover partial producer reuse, allocation aliases, distinct/replaced caches, exact eviction matching and missing history.

## Capture storage

The live Perfetto ring buffer uses up to 128 MB in RAM while recording. Saving writes a capture bundle containing `trace.pftrace`, `manifest.json` and `summary.json`. Analysis adds files within that bundle.

Manual Stop/Save and orderly-exit saving retain the four most recent completed manual bundles in the same destination parent. These saves carry an explicit retention marker. Explicit scripted saves stay unmarked and are excluded, along with unmarked older bundles, unrelated folders and incomplete saves. Saving and cleanup use the recorder's existing background thread. Cleanup runs only after a successful save and always preserves that new capture.

`FOVEONIC_DIAGNOSTICS_DIR` selects a stable parent for manual saves across app launches. Without it, orderly-exit saves use the platform temporary directory's `foveonic-captures` folder. The explicit Stop/Save button otherwise lets you choose a destination. Keep using the same parent for the four-run limit to apply across launches. Copy a bundle outside that managed parent when it needs longer preservation.

For local owner reviews, use a stable project folder:

```sh
FOVEONIC_DIAGNOSTICS_DIR="$PWD/.local/foveonic-captures" FOVEONIC_PROFILE=wall target/release/Foveonic.app/Contents/MacOS/foveonic-gui
```
