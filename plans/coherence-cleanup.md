---
summary: "Finish removing duplicated owners and near-duplicate paths in 0.10.0, following the one-owner principle that 0.9.0 started applying."
---

# Coherence cleanup

Implements the first [design principle](../docs/architecture/principles.md#keep-one-owner-and-one-path). 0.9.0 ships wide rendering and the start of this cleanup. 0.10.0 completes it before feature work resumes (owner, 2026-11-22). The work ends when the major sources of duplicated state and near-duplicate paths are gone. Smaller ones are recorded and left when removing them is worth less than it costs.

Each item needs a design discussion and owner approval before implementation.

## Carried from 0.9.0

Details are in the linked plans. Move each item here when its work starts.

- [ ] **Ratings in the photo's sidecar.** [Persisted stores](./view-planning.md#persisted-stores-owner-direction-replaces-step-2-pieces-24) slice 3. Breaking change: `ratings.json` is not carried over.
- [ ] **`AppSettings`.** Slice 4. One file per key and one atomic JSON writer. Breaking change: preferences leave `app.ron`.
- [ ] **Import history edits.** Slice 5.
- [ ] **Automatic values only from renders.** Follows on the store. Editor actions on the whole editing selection landed in 0.9.0.
- [ ] **One readiness state per photo.** [Release-gate items](./view-planning.md#follow-ups), item 1.
- [ ] **Two jobs called "normalize".** Item 2.
- [ ] **One source for the camera family in render requests.** Item 3.
- [ ] **Split `Settings` into recipe and render parameters.** Item 4. Do this before the sidecar record is defined. Two things from the [Quick Look work](./quick-look-previews.md) land here:
  - White balance and color mode become a typed choice, As Shot or a named preset, in the recipe. The render parameters hold the concrete name that render resolved, so the compiler guarantees recipes never see As Shot. In 0.9.2 the engine already owns As Shot's place in the option lists and its resolution, with an empty string as the stored form, so this changes the type, not who decides.
  - Processing quality and working scale are render parameters. `for_export` (applied by the CLI, GUI export and `dng::write_source`) and Quick Look's Speed and preview scale stop editing a recipe copy.
  - With a camera part, [cross-scale reuse](./cross-scale-reuse.md) can compare it directly instead of rebuilding the camera stage to see whether it merges into the main graph.
- [ ] **Test harness.** Item 5. Direct smoke targets move onto the scenario driver. Scenario waits name their condition.
- [ ] **Scheduler.** [Render scheduler](./render-scheduler.md) step 9 leftovers, step 10 (export and CLI on the scheduler), step 11 and the GUI naming follow-up. Step 10 also gives the GUI and the command line one export path. Today the GUI's `export::write` fills camera choices, applies `for_export`, renders, encodes or writes DNG, logs support events and publishes atomically through `importing::write_output`. The CLI's `convert` and `export-dng` branch repeat most of it with different events and write straight to the destination, so a failure leaves a partial file, and the existing-output check is made in three places. Move `export::write` into core as the one path and delete `convert`. Batch output naming becomes one core function: the GUI adds `-1` suffixes and the CLI rejects collisions, which becomes a conflict policy (owner decision pending).

## Found later

Record duplicated paths found after 2026-11-22 here, with where they are and what disagrees.

- [ ] **Shared linear edits run after the camera family, not inside it.** Found building [cross-scale reuse](./cross-scale-reuse.md), 2026-11-26. Shared finishing runs after `build_camera`, but each family's builder calls `linear_edits::build` itself with a context it assembles: Dehaze basis, guide white, sample density and Adaptive Tone analysis. So a branch that needs the image after shared edits, such as the tonal-ranges guide, can only rebuild the whole camera recipe with a stop name. The family should return its camera stage and a description of it, and the shared edits should run once after it. Every whole-photo branch then becomes the camera stage at a scale plus shared edits up to a stage, and tonal ranges can join the whole-photo helper. This shapes 0.10.0's family-trait slice for graph building, and the release plan's finding that linear edits already run after `build_camera` needs that correction.
- [ ] **Camera stages read finishing settings.** Found by [cross-scale reuse](./cross-scale-reuse.md), 2026-11-26. Merrill's camera appearance reads the base response, and Quattro copies the Dehaze settings into its camera controls. A camera stage therefore changes when an edit that runs after it changes, so no branch that resets those edits can share the main render's camera stage. Settle it with the camera part of the recipe.
- [ ] **Editing values come from one render reply.** Found in 0.9.3 testing, 2026-11-27; owner: fix in the 0.10.0 settings rework. The editor stays disabled until a photo has numeric and reset values. The worker computes them only for a `Purpose::Current` render (`rendering/worker/host.rs`), and `receive_image` stores them only when that reply is still current (`main.rs`). Clicking through SD10, SD14 and SD15 faster than they rendered left their editors disabled with no error, while the kept images meant returning started no new render. Opened directly, each photo's editor works. The values depend on settings and metadata, plus the render's Dehaze estimate, so they should not hinge on one reply. Reproduce with a behavioral test first.
- [ ] **One sample density per working grid.** Found building [cross-scale reuse](./cross-scale-reuse.md), 2026-11-26. Adaptive Tone sizes a guide on the working grid from the recipe's working pixels per native pixel (`linear_edits::Context.scale`) and a guide on a separate branch from the working scale's factor. They differ for Quattro, whose RGB grid is not at the working scale's density. That keeps three size-based branches in the Adaptive Tone section, which cannot fold into the whole-photo helper until one density is defined. It is camera geometry, so it belongs with the camera description.

- [ ] **Remove the selected photo as a source of edits and choices.** Owner, 2026-11-23: the last-selected path is not useful and is buggy. `App::selected` still drives crop and rotation edits (`edit_selected`), the Editing menu's camera family and preset list, the values shown for prevent clipping and auto compensation with several photos, the navigator source and paste normalisation. Copy settings, Save preset and Set as default are disabled with several photos in 0.9.0. Crop presets per photo are not built.
- [ ] **No ignored tests left unsorted.** Owner, 2026-11-24: committed for 0.10.0. The `#[ignore]` flag currently covers 495 maintained GPU checks in ten groups and 151 excluded tests. Sort every one:
  - Each maintained test names the photograph it needs from the tracked manifests, so one command runs them all. This removes the environment variants that aim one variable at different files, and with them most of the [planner](../scripts/profiling/rust_test_inventory.py). Only the five checks against the maker's software keep their own process.
  - The 129 excluded tests that compare against private captured data: recreate the data from tracked photographs, or delete the test. Nothing may depend on files held on one machine.
  - The 18 benchmarks and timing experiments leave the test flag for the [comparable runs](./comparable-runs.md) tool or are deleted.
  - The four remaining exclusions (missing originals, capture-specific values) are decided one by one.
  - A renamed test must fail a local check. In 0.9.0 a rename left a stale inventory name that only the test machine found.
- [ ] **One source of a photograph's defaults for the app and the command line.** Moved to 0.9.2 with the [Quick Look previews](./quick-look-previews.md#slice-2-one-source-of-a-photographs-defaults), which need it (owner, 2026-11-25). The app opens a photograph through the camera's capture defaults (`camera_settings`), so an unedited Merrill photograph has noise reduction on. The command line and the render tool start from `Settings::new`, which leaves it off. Consequences found in 0.9.0: the render cases named camera defaults did not run Merrill noise reduction, a crash in it on integrated graphics was missed, and the release tables were first measured with it off. Done in 0.9.2: one path from `camera_settings` and As Shot resolved only by render. A saved Merrill recipe without noise controls loads the camera default (owner, 2026-11-25).
- [ ] **Smaller command-line copies.** Found reviewing the Quick Look work, 2026-11-25. Each is small; take them with the export path or alone.
  - The X3F extension check is repeated in the CLI batch scan, the GUI browser, noise calibration and two examples.
  - `FOVEONIC_CALIBRATION_RESOURCES` is read by the CLI and the GUI separately. The engine should read it.
  - `--format` keeps its own name table beside `Format::from_extension`.
  - `--processing-quality` does nothing for render, `export-dng` or batch, because `for_export` replaces it. Remove it from the CLI and keep it in the profiling examples.
  - The help text repeats value ranges by hand and has drifted: it says `--fine-detail 0..2`, but 3 is accepted.
- [ ] **The command line offers the engine's controls, not its own list.** Owner, 2026-11-27, for 0.10.0. The CLI parses a hand-written set of flags beside the engine's control contract, so it drifts from the app. Found in 0.9.3: Classic and Merrill open with noise reduction on, but only `--reconstruction` exists, so nothing turns it off and the flag itself does nothing. Other toggles have on/off pairs (`--repair` / `--no-repair`). Derive flags, value ranges and help from the engine's control descriptions, so every control the app can set has the same name, default, range and both on and off forms on the command line. This absorbs the drifted help ranges and `--processing-quality` above.
- [ ] **Noise node: skip margin pixels nobody reads.** The node filters 16 tiles with a 64-pixel margin each, 29% more pixels than the image. Find which margin pixels later steps read before narrowing any step. Output must stay bit-identical.
- [ ] **The memory budget follows the GPU's reported memory.** Owner, 2026-11-24, for 0.10.0. The app reads dedicated and shared GPU memory at startup and logs them but does not act on them. An integrated GPU borrows system RAM, so the app's memory and its GPU memory are one pool, and a small discrete card (a user has 2 GB) has a hard limit. Lower the budget and render one photo at a time when the adapter is small or shared. Aim: the app works, slowly if need be, on a 2 GB card and on integrated graphics with 16 GB. No 2 GB card is available for testing.
- [ ] **Steps in flight follow the memory budget.** Owner, 2026-11-24: replaces the fixed one-step rule in 0.10.0. Submit steps ahead while their storage fits the spare budget the scheduler already uses for width, and wait otherwise. Measured for one photo over 144 render cases: no limit (0.8.4) 29.6 s and a typical full-size peak of 2.19 GiB; two steps level with 0.8.4 in both; one step 30.1 s and 1.55 GiB, with many-step renders 10 to 40 ms slower.
  - First prove a check that releases finished storage without blocking. Without one, unbounded submission reached 5.9 GB on a full-size Quattro render.
  - Each wait costs 0.4 to 1 ms, which likely also explains the warm-edit item below. Measure both together.
- [ ] **Warm one-photo edits are 4 to 5 ms faster when every step waits for itself.** Measured 2026-11-24, interleaved on one tree, 40 edits each: waiting every step 34.5 ms, one step in flight 36 to 40 ms, 0.8.4 39 to 40 ms. Fresh renders go the other way (35 s against 30 s over the 144 render cases), so 0.9.0 keeps one step in flight. Find why a short warm edit loses time to the in-flight step before choosing a rule by workload.
- [ ] **Stage profile beside the trace.** Owner, 2026-11-24. Perfetto is the one trace recorder, but each render also keeps its own list of stage names and times (`Profile`), which tests, the profiling example and the comparable-runs stage times read. Find what the profile records that a trace does not, and what needs it with tracing off. Then either read stages from the trace or state why the profile stays.
- [ ] **One GPU wait per delivered image.** Delivery waits once per finished frame instead of once per tick, which may limit the gain from wide rendering. Not measured.

## Features for 0.10.0

Owner direction, 2026-11-27. These build on the settings and export work above.

- [ ] **Preview and convert from Finder.** Quick Look previews follow two app settings, resolution and default recipe, and a Finder Quick Action converts with the same settings into every supported format. See the [Quick Look plan](./quick-look-previews.md#preview-and-convert-from-finder-0100). Depends on `AppSettings` and on one export path.

## Audit

- [ ] Before closing this plan, audit the app against the principle's list and record what is left and why.
