---
summary: "Measure completed full processing and warm native feedback with explicit modes, JSON context and separate validation."
kind: how-to
---

# Profiling

The [requirements](./specs/profiling.md) distinguish three modes:

| Mode | What it records | Scheduling |
| --- | --- | --- |
| `off` | No optional core/GUI reports | Normal |
| `wall` | Current host stages, lifecycle/frame/callback observations and engine activity | No profiling-only waits or repaint requests |
| `gpu` | Wall observations plus supported GPU-pass timestamps | Existing timestamp resolution adds synchronization; diagnostic only |

Use CLI `--profile wall` or `--profile gpu`. For the GUI, set `FOVEONIC_PROFILE=wall` or `gpu` before startup; the default is `off`. GPU diagnostics fail explicitly if timestamps are unavailable. These replace bare CLI `--profile` and the old GUI timing/GPU flags.

Reports are JSON lines with `foveonic_profile: 1`, `event`, `mode`, `synchronized`, and `data`. Other stderr remains ordinary text. The core owns host/GPU stage vectors and engine activity; frontend reports add source, settings, execution and revision context. Loading appears in its own event, not in every warm render. GUI request settings may contain unresolved camera defaults; processing reports contain the selected WB/mode and controls. Derived camera tables are reproduced from the recorded source, not dumped into logs.

## Completed full processing

Build with `cargo build --release -p foveonic --example profile_pipeline`. Use a new export directory:

```sh
target/release/examples/profile_pipeline INPUT.x3f .local/profile-method-a \
  --warm 5 --wb Sunlight --mode Standard --reconstruction --profile wall \
  > .local/profile-method-a.jsonl
python3 scripts/profiling/summarize.py .local/profile-method-a.jsonl
```

The runner shares the normal photographic option parser, so new algorithm controls should be added there once. Each iteration executes the complete selected final recipe from the decoded source, without an interactive retained boundary. Its first iteration is cold; warm repetitions are separate in the summary. GPU initialization is outside render intervals, and source file-cache state is uncontrolled. Choose enough repetitions for the actual comparison; one method/recipe per dataset is easiest to interpret.

Use `--capture-float OUTPUT NEW_FILE` to render a named floating image output after all timed iterations. The capture record includes source, settings, storage extent, color space, nominal linear white, active crop, rotation, data type and byte order. Raw samples use the recorded physical layout; apply the recorded view when comparing photographs. This separate render and readback are outside timing. The destination must not already exist.

Request `--profile gpu` for one additional synchronized diagnostic iteration after the ordinary warm repetitions. Its results remain in a separate mode group. Render and TIFF export have separate profiles/activity deltas. Optional `--goldens DIR` compares or creates full-float snapshots outside timed intervals; choose a distinct golden directory per intended recipe. Byte equality applies to output-equivalent optimizations, not to different denoisers or recovery methods. Evaluate photographic quality separately.

## Smart preview transitions

Compare detail-first refinement with direct Full from equivalent Merrill or Quattro Fast sessions:

```sh
cargo run --offline --release -p foveonic --example profile_smart_preview -- \
  INPUT.x3f .local/smart-transitions.json
```

The report destination must be new. Use `--region-first exposure --fractions 0.25,0.5 --repeats 3` for bounded regional comparisons. Add `--direct-only -- --tone-method universal --base-response natural` to measure a Fast-to-Full transition with Adaptive; regional Adaptive is unsupported. Render options after `--` override the camera settings used by every route. Each trial records epoch-millisecond bounds for its whole route, including the prior image and any verification, while reported durations use a monotonic clock. These bounds conservatively identify potential workload overlap.

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

## Native feedback

Coordinate GPU work and pointer focus with the owner first. In one terminal:

```sh
python3 scripts/profiling/native.py record \
  --binary target/release/Foveonic.app/Contents/MacOS/foveonic-gui \
  --fixture INPUT.x3f --directory .local/native-method-a --mode wall
```

The recorder starts only its own app with isolated preferences, explicit mode, no smoke automation and no repaint override. It starts the workload monitor before the app, records raw stderr and observations, and stops both owned processes on exit. Its manifest includes binary and fixture hashes, plus a fingerprint of nonignored Rust, WGSL, Cargo and bundled asset inputs at recording time. The fingerprint does not prove the binary was built from those inputs and excludes ignored generated inputs. Ctrl-C stops the recorded app, not an existing owner process.

Set up the desired source, full-quality controls, window geometry, navigator/scopes and view scale. Warm up first and retain screenshots/settings evidence. Supply actual screen coordinates for the desired slider; they are not guessed or embedded in the tool:

```sh
python3 scripts/profiling/native.py drag --directory .local/native-method-a \
  --start 1414 --end 1532 --y 805 --steps 61 --seconds 2 --label detail-up
python3 scripts/profiling/summarize.py .local/native-method-a/stderr.jsonl \
  --pointers .local/native-method-a/pointer.jsonl
```

Coordinates above are illustrative, not portable across layouts. Keep warm-up pointer records separate from measured ones. Run matched before/after repeats with GPU timestamps disabled; review raw overlap records and final-current images. Memory sampling, such as `vmmap -summary PID`, belongs after timed interactions and settlement. The workload monitor catches known commands, not all system GPU use.

## Interpret results

Browse and Import processing use the same profiling mode as interactive rendering. `source_load` and `export` records identify batch work with `frontend: "batch"`. They include cache/original input classification and the export's existing host/GPU stage timings. `batch_photo` reports the whole photo operation, including source loading, original copying in Import, output publication and history. Whole-photo time can include pauses for interactive work. Nested stage durations overlap and must not be added to that total.

Host times are wall durations, including driver work or waits where applicable; they are not CPU-active measurements. Nested spans are not additive. GPU timestamps cover passes, not implicit resource initialization or inter-pass queue delays. Wall time minus GPU time is not pure queue waiting. Allocation/transfer totals are not memory residency. Repeated passes aggregate by stage with sample counts and totals per profile, not list position.

Native receipt, callback encoding and callback gaps do not establish screen scanout. External pointer/pipe timestamps select interaction windows but do not provide precise event-to-screen latency. Keep fresh full-processing costs separate from warm session feedback: a cached edit can skip the denoiser entirely. Summaries aggregate a supplied dataset; compare methods in separate datasets and use raw source/settings context when interpreting interactive sweeps.

Run portable tool tests with `python3 -m unittest discover -s scripts/profiling -p 'test_*.py'`. No dashboard, tracing service or installation is required.

## Capture an early loading stall

Run the current app with wall profiling and redirect stderr to a local file:

```sh
FOVEONIC_PROFILE=wall cargo run --release -p foveonic-gui 2> target/interactive-profile.log
```

Launch the executable directly through Cargo so its environment and stderr stay attached to the terminal. Use Browse or Import normally and quit after reproducing the stall. Ordinary build messages may share this file with the marked JSON records. `selection` reports the start and recipe-ready delay before image submission. `worker_started` reports queue delay. `source_load.input` distinguishes `decoded-cache` from `original`, with separate cached identity lookup, header, payload read/checksum and camera-metadata timings. Existing processing and presentation events cover the remaining path. Nested durations overlap. Logging remains opt-in and creates no profiling-only GPU waits.

## Headless browser CPU work

Run `cargo test --release -p foveonic-gui browser_cpu_profile -- --ignored --nocapture` without concurrent measured work. The synthetic 1,000, 10,000 and 50,000-path folders report JSON lines with seven per-operation batch samples in nanoseconds after one warmup. They cover one, 100 and all selected photographs, review counts, membership, alternating import marks, index rebuilding and reverse sorting. The `_scan` operations model the former folder-wide work; they do not run an earlier binary. This isolates host-side browser operations and does not measure native input, drawing, file discovery or GPU work. Run separate trials for comparisons and retain the complete output and build context.

Run `cargo test --release -p foveonic-gui browser_discovery_profile -- --ignored --nocapture` to measure recursive discovery and `Browser::open` separately on generated empty files. The test creates and removes its own repository-local fixture. It reports five timed samples after warmup for 1,000 files in one child folder and 10,000 files across 100 child folders. `Browser::open` also starts metadata work, which may overlap its measurements. Filesystem and OS caches are uncontrolled; these local results do not establish removable-card or network-drive latency.

Run `cargo test --release -p foveonic-gui browser_snapshot_profile -- --ignored --nocapture` to measure applying an already-discovered folder on the UI thread. The synthetic paths do not exist, so background metadata failures and worker overlap may affect timings. Cloning the input fixture occurs outside each timed interval. This records snapshot installation only, not native input or rendering.

Run `cargo test --release -p foveonic-gui batch_progress_profile -- --ignored --nocapture` to compare cloning completed batch history with reading its small status summary. Synthetic records contain one imported path and two operation messages per photo, with one error per hundred photos. The test reports host-only batch samples; it does not include locking, real imports or native interaction. The UI still clones full progress once per frame for import-history replay and event collection.

## Startup host work

With `FOVEONIC_PROFILE=wall`, the GUI emits `startup` records for App construction and the first update. Construction begins after eframe has created the window and device. Its nested host spans separate GPU pipeline creation including the existing validation-scope completion from bundled calibration-map parsing and total engine initialization. These spans overlap and must not be added. The first update is not a presentation or scanout measurement. Wall mode adds no profiling-only GPU waits. Use an isolated native launch and retain the executable and source hashes with the raw JSON.
