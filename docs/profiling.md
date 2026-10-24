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

Request `--profile gpu` for one additional synchronized diagnostic iteration after the ordinary warm repetitions. Its results remain in a separate mode group. Render and TIFF export have separate profiles/activity deltas. Optional `--goldens DIR` compares or creates full-float snapshots outside timed intervals; choose a distinct golden directory per intended recipe. Byte equality applies to output-equivalent optimizations, not to different denoisers or recovery methods. Evaluate photographic quality separately.

## Smart preview transitions

Compare detail-first refinement with direct Full from equivalent Merrill or Quattro Fast sessions:

```sh
cargo run --offline --release -p foveonic --example profile_smart_preview -- \
  INPUT.x3f .local/smart-transitions.json
```

The report destination must be new. The example uses Sunlight/Standard with camera-default controls and a central 992×1002 native-pixel detail tile. It excludes source decoding and GPU initialization. Each route starts from its own completed Fast overview. Five measured pairs follow one warmup pair, with alternating route order. Completed processing and display-texture preparation use profiling Off and retain the overview/detail textures as the GUI does. Separate GPU diagnostic runs contain stage timings and transfer/activity counters. Compare the first detail, direct Full, and additional Full continuation independently. Add the two progressive durations to measure when the whole image becomes Full. These are engine-to-ready measurements, not input-to-screen latency. Stored camera adjustments and different viewport sizes can change native app costs.

## Native feedback

Coordinate GPU work and pointer focus with the owner first. In one terminal:

```sh
python3 scripts/profiling/native.py record \
  --binary target/release/Foveonic.app/Contents/MacOS/foveonic-gui \
  --fixture INPUT.x3f --directory .local/native-method-a --mode wall
```

The recorder starts only its own app with isolated preferences, explicit mode, no smoke automation and no repaint override. It stores the raw stderr, source/binary hashes and a manifest. Ctrl-C stops that app, not an existing owner process. To flag known overlapping programs separately:

```sh
python3 scripts/profiling/watch_workloads.py .local/native-method-a/workloads.jsonl
```

Set up the desired source, full-quality controls, window geometry, navigator/scopes and view scale. Warm up first and retain screenshots/settings evidence. Supply actual screen coordinates for the desired slider; they are not guessed or embedded in the tool:

```sh
python3 scripts/profiling/native.py drag --directory .local/native-method-a \
  --start 1414 --end 1532 --y 805 --steps 61 --seconds 2 --label detail-up
python3 scripts/profiling/summarize.py .local/native-method-a/stderr.jsonl \
  --pointers .local/native-method-a/pointer.jsonl
```

Coordinates above are illustrative, not portable across layouts. Keep warm-up pointer records separate from measured ones. Run matched before/after repeats with GPU timestamps disabled; review raw overlap records and final-current images. Memory sampling, such as `vmmap -summary PID`, belongs after timed interactions and settlement. The workload monitor catches known commands, not all system GPU use.

## Interpret results

Host times are wall durations, including driver work or waits where applicable; they are not CPU-active measurements. Nested spans are not additive. GPU timestamps cover passes, not implicit resource initialization or inter-pass queue delays. Wall time minus GPU time is not pure queue waiting. Allocation/transfer totals are not memory residency. Repeated passes aggregate by stage with sample counts and totals per profile, not list position.

Native receipt, callback encoding and callback gaps do not establish screen scanout. External pointer/pipe timestamps select interaction windows but do not provide precise event-to-screen latency. Keep fresh full-processing costs separate from warm session feedback: a cached edit can skip the denoiser entirely. Summaries aggregate a supplied dataset; compare methods in separate datasets and use raw source/settings context when interpreting interactive sweeps.

Run portable tool tests with `python3 -m unittest discover -s scripts/profiling -p 'test_*.py'`. No dashboard, tracing service or installation is required.
