---
summary: "The acceptance inventory connects user paths to available behavioral, smoke and numerical evidence and records gaps."
kind: reference
---

# Acceptance coverage

The [acceptance catalog](../scripts/profiling/acceptance_cases.json) maps user paths to explicit checks. List a domain with `python3 scripts/profiling/acceptance.py --domain browser --list`, or list the full inventory with `--full --list`. The runner requires an explicit domain, job or `--full` and a new output directory. Its JSON report updates after each job with outcomes, evidence locations and counts of covered, partial and missing paths. Overall completion stays false until all selected jobs have returned. A missing selected input, unsupported job, missing completion artifact or failed child makes the run unsuccessful. Job success and coverage completeness are separate report fields. Covered means every mapped job passed the declared checks and the path has no recorded gap. Partial means some evidence exists while a mapped job or behavior remains unchecked. This inventory describes maintained acceptance contracts, not every possible ordering of user input.

The [Rust smoke modules](../crates/foveonic-gui/src/smoke/AGENTS.md) own native actions, observations and scenario expectations. Feature suites keep their readiness checks with their actions and assertions. Python's [scenario checks](../scripts/profiling/scenario_checks.py) validate intended edits before [image replay](../scripts/profiling/image_replay.py) checks capture identity and compares fresh renders. Shared results stay in `results.py`, isolated input staging in `inputs.py`, and workload identity and UI timing in `interaction_latency.py`. Runnable scripts and adjacent portable tests retain the existing commands and discovery pattern.

The [external RAW sample manifest](../scripts/profiling/external_raw_samples.json) pins the seven Classic, Quattro and Quattro H originals used by the numerical matrix. Each entry records its local filename, publisher download URL, SHA-256, byte length, camera family and source-reported license and notes. Fetch these photographs directly from RAW Pixls rather than adding them to the project's example release. SD9 and SD10 are listed under CC BY-NC-SA 4.0, while the other five are CC0. The manifest preserves those different terms. The owner's photographs, including Hazy Coast, remain in the [example release](https://github.com/hank-bond/foveonic/releases/tag/example-photographs).

Prepare both original collections from the repository root:

```sh
cargo xtask examples sync
python3 scripts/profiling/fetch_samples.py
```

Both commands use `.local/example-photographs` by default. The external fetcher verifies every existing sample's byte length and SHA-256 before starting downloads. It leaves mismatched originals untouched and fails. Downloads are staged, verified and linked into place without overwriting another file. Use `--offline` to verify cached external samples without network access, or `--directory PATH` for a separate sample cache.

The [test input definitions](../scripts/profiling/test_inputs.json) track the ten-source numerical matrix, eight-photo interaction recipes, mixed-choice recipes and five slider fixtures. They name originals without personal paths. The seven external identities remain in their existing manifest. Other originals have pinned hashes in the test definitions. The Quattro H fixture explicitly aliases one original rather than claiming two independent photographs.

The [input setup command](../scripts/profiling/prepare_inputs.py) syncs both collections through their existing commands, verifies all declared originals and generates runner-compatible JSON plus aliases in a new directory:

```sh
python3 scripts/profiling/prepare_inputs.py --directory .local/test-inputs
# Use cached originals without network access:
python3 scripts/profiling/prepare_inputs.py --offline --directory .local/test-inputs-offline
```

Use `--source-dir PATH` for another original collection. Generated `render-cases.json`, `scenario-cases.json`, `choice-cases.json` and `fixtures.json` go to the existing runners' `--render-cases`, `--cases`, `--choice-cases` and `--fixtures` arguments respectively. `provenance.json` records the verified original hashes, aliases and declarations. Existing output directories are preserved. Missing or changed originals fail before generating anything. The owner's Medium/Low Merrill originals are included in the example release, so both native RAW sizes can be rebuilt through the same setup command.

For a run, supply the built GUI through `--binary`, source photographs with repeated `--source`, and scenario input recipes with `--cases`. Broad selection, cached-switch and collection cases need eight declared manual exposure recipes. Use a representative mixed-camera set, with the Merrill pair first, to exercise the pending-edit re-add regression recorded in the plan. The choice journey takes `--choice-cases`, with two recipes using Sunlight and Incandescent white balance, neutral bias, enabled Tone with contrast 0.25, and manual exposure. Multi-edit smoke requires eight source paths, and comparison and Smart neighbor-prefetch smoke require at least two. Supply a Tiger original separately with `--tiger-source` and a source that produces an automatic haze estimate with `--dehaze-source`. The slider matrix requires `--fixtures` and `--slider-cases`. Final app-image checks require `--renderer`, and the headless regression matrix also requires `--render-cases` and an approved `--baseline` directory. Capture checks need a local `--trace-processor`. Missing requirements are reported per job. Sources are hard-linked into a new owned folder with separate sidecars and home. Failed acceptance cases remain unsuccessful and are recorded in the plan. Windows default to the background. `--foreground` opts into foreground windows.

Legacy suites combine direct application operations, synthetic egui events, readiness checks, assertions and screenshots. Each catalog job describes its specific evidence. Exit status and a terminal screenshot establish sequential completion, not trace integrity. Their screenshots are evidence for visual inspection, not automatic image comparisons. The multi-edit smoke also checks selection, edits, retained textures and completion. The slider matrix declares 90 numeric control cases and exercises actual egui widget input for the declared fixtures and controls. It can compare settled results with fresh renders. Its inventory is limited to observed controls and declared configurations. Native scenarios have their own structured results and may combine direct commands with input dispatch. The render matrix compares complete floating samples and geometry against the supplied baseline, without claiming GUI coverage. None establishes physical pointer or screen presentation.

The opt-in `behavioral.py --reuse-session` option groups slider correctness cases by fixture, theme and processing quality. It requires Off mode and `--compare-images`. Each case opens separate staged inputs and recipes, captures its settled images and retains its own evidence. After the shared app exits, the existing independent image checks run for every case. Reused sessions are marked unsuitable for fresh-launch latency comparisons. `acceptance.py --reuse-slider-sessions` passes this option only to the slider matrix. Other jobs retain their existing process boundaries. See the [session requirements](specs/slider-test-sessions.md).

The transition journey combines seeded selection and exposure edits with single- and multi-photo quality changes, fixed Draft/Thumb drawn zoom/pan/Fit, matching scopes, framing and Before/After. Smart zoom also checks that display-demand changes preserve the Before rendition. The journey verifies quality for retained inactive tiles with one editing target, preserves active inspection during overview refresh, and restores the original photographic processing identity on returning to Final. It includes a controlled in-flight edit and quality change, and records the complete sequence and expected composition. The lifecycle journey checks Browse saves and reopening, Import isolation, clipboard command handlers, scoped defaults/resets, frozen copied-original recipes, a truncated RAW and valid-file recovery, and error acknowledgement. It observes the actual Process button disabled without a destination, enabled when picked, disabled after unpicking, and enabled after repicking before clicking it. Clipboard command output is captured before it reaches the OS clipboard. The browser journey types and clears search, changes name order, applies rating/pick commands, navigates, and checks the browser selection remains larger than the editing limit. Collection coverage stages 100 labeled aliases from eight originals, checks the eight-photo editing bound and retained edits, then returns to the original folder. Those aliases are a collection-size test, not 100 independent sensor samples.

The capture-exit job leaves a recording active and closes the actual application. Its report requires a saved trace, valid capture analysis and an `application_exit` action. The shutdown-edit job submits a Browse edit and requests close immediately, then checks each saved sidecar after process exit. It verifies accepted edits and revision advancement but does not establish that a sidecar write was still pending at close. Startup and already-pending capture-save shutdown cases also have controller-boundary tests. Background export checks verify a real TIFF can be decoded and has the expected dimensions after the app exits. macOS uses its installed image reader, and other hosts report missing decoding evidence explicitly.

Latency jobs are selected separately with `--domain latency` or a specific job. Use `--record-latency-baseline` to record a baseline, then supply it with `--latency-before` as a matched natural Off regression report for one job, or a directory containing `JOB_ID/regression.json` for several jobs. Each records at least three new trials. `--latency-budget` supplies an optional maximum regression percentage. Without it the result reports observed deltas without a budget decision. Acceptance jobs default to a disabled decoded disk cache, with an explicit 1 GiB budget for warm-disk, prefetched-selection and Smart neighbor-prefetch jobs. Legacy launches also retain the binary hash, inputs, window placement and owned cache namespace. Natural scenarios cover initial group selection, warm disk, single-photo edits at Full/Fast/Draft/Thumb, and eight-photo Thumb edits, switching, supersession and pressure. Initial selection begins after one active photo and the GPU device have loaded. These timings do not measure process startup, and input hashing warms the filesystem cache. Controlled delivery waits and screenshot durations are excluded from these latency comparisons. See [profiling usage](profiling.md) and the [behavioral testing plan](../plans/behavioral-testing.md) for fixture formats and remaining coverage.

Each legacy smoke step has a 60-second liveness deadline. A stalled step fails with its requested/displayed state, admission, scopes, navigator and thumbnail readiness in the owned log. The direct smoke launcher also terminates its own process group on interruption or its overall timeout. Delegated runners forward interruption to their scenario owner, which closes its application. Forced termination of an unresponsive wrapper can still prevent cleanup. Check owned processes before controlled measurements.

The catalog explicitly leaves native file-dialog selection missing and preference-input/restart coverage partial. Other recorded gaps include native clipboard round-trips, rating filters and persistence, destination disconnects during export, and headed recorder-startup/save races. Unit or handler coverage is identified separately where it exists.

## Test environment setup

Pillow is a declared dependency in the pinned [test requirements](../scripts/test-requirements.txt). The common setup command prepares one reusable `.local/test-python` environment for all Python suites. Individual tests do not install dependencies or create environments. Use `.local/test-python/bin/python` for the Python check groups (`.local/test-python/Scripts/python.exe` on Windows).

Run the [test setup command](../scripts/setup_tests.py) from the repository root. Python 3.11+ and Rust 1.88+ with Cargo are bootstrap prerequisites. Native builds also need a C/C++ compiler. On macOS, install/select Xcode command-line tools first. Building the native app bundle still requires Xcode 26+ as described in [release guidance](./releases.md).

```sh
# Check installed tools without installing or downloading anything:
python3 scripts/setup_tests.py
# Explicitly install missing supported tools, then sync/verify the images:
python3 scripts/setup_tests.py --install --images
# Repeat using the existing tools and cached originals only:
python3 scripts/setup_tests.py --images --offline
```

The command checks Rust, Cargo, rustfmt, Clippy, a C/C++ compiler, CMake, ExifTool and the shared Python test dependencies. Windows requires `clang-cl`, as used by the native build. Online image preparation also requires GitHub CLI and an authenticated GitHub session for the example release. The tool list and install mappings live in the setup script. Missing prerequisites and failed installations return a failing exit status. `--install` installs missing Homebrew packages on macOS, missing Rust components through rustup, and prepares the shared Python environment from the pinned requirements. Repeated runs reuse dependencies that already match. Homebrew, Rust and Xcode must already be installed. On other systems, install the reported native tools with the system's package manager and rerun the checker. Tests never install tools themselves. `--offline` cannot be combined with `--install`.

On Windows x64, setup also checks the pinned DirectX shader compiler archive and its `dxcompiler.dll` and `dxil.dll` files. `--install` downloads and verifies the same archive used by the Windows release workflow, then stages both DLLs beside application and test executables under Cargo's debug and release target directories. Cargo metadata resolves the target root, including a custom `CARGO_TARGET_DIR` or Cargo configuration; its offline lookup requires the workspace dependencies to be available locally. Existing different DLLs are left untouched and reported as conflicts. A later check verifies the cached archive and all staged copies without downloading. These paths cover native Windows builds; an explicit cross-target build may need additional staging beside its executables. The trace processor remains a separately supplied tool for trace-based checks.

For Windows builds, configure the compiler as in CI before running Cargo:

```powershell
$env:CC = 'clang-cl'
$env:CXX = 'clang-cl'
$env:CXXFLAGS = '-DNOMINMAX -DWIN32_LEAN_AND_MEAN -D_CRT_SECURE_NO_WARNINGS'
```

`--images` reuses the existing example sync and external-sample verifier. Repeated runs preserve originals and sidecars. `--source-dir PATH` selects another image cache. For the generated recipes and aliases used by the larger behavioral suite, continue using `prepare_inputs.py` above with a new output directory. GPU availability, graphical sessions, independent pixel baselines and the optional Trace Processor remain requirements of the specific suites that use them.

## Capture metadata exports

The headless [capture export tests](../crates/foveonic/tests/capture_export.rs) process five real-image cases: Merrill TIFF and JPEG XL, Classic JPEG, Quattro PNG, and Quattro H DNG. Each checks the source against the maintained sample hashes, compares metadata-only loading with full decoding, removes a disposable source copy before exporting, and independently reads the exported capture details and geometry with ExifTool. Cases apply a quarter turn and centered half crop. All outputs resize to 96×64. The originals and saved editing recipes remain untouched. These checks cover the processing and export APIs, not desktop UI interactions.

Requirements: a working GPU, the example collections prepared above, and `exiftool` on `PATH`. Use the [test environment setup command](#test-environment-setup) to check or explicitly install it. A missing photograph, changed source hash or unavailable ExifTool fails the selected test.

Run all five cases serially from the repository root:

```sh
cargo test --offline --release -p foveonic --test capture_export -- --ignored --test-threads=1
```

The default source directory is `.local/example-photographs`. Set `FOVEONIC_FIXTURE_DIR` to use another prepared collection. Normal workspace runs list these file/GPU-dependent cases as ignored. The command above explicitly runs them. Existing synthetic encoder tests separately check decoded pixel values, precision and color declarations.

These five cases verify exported metadata and container dimensions. They do not establish pixel-level crop/rotation correctness or full-image decoder interoperability. Existing numerical geometry and encoder tests provide separate evidence for those behaviors.
